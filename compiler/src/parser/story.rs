impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn parse(&self) -> Result<ParsedStory, CompilerError> {
        if self.source.is_empty() {
            return Err(CompilerError::invalid_source(
                "ink source is empty; expected at least one line of text".to_owned(),
            ));
        }

        let normalized = self.source.replace("\r\n", "\n");
        let lines = split_lines(&normalized);

        if lines.is_empty() {
            return Err(CompilerError::invalid_source(
                "ink source is empty; expected at least one line of text".to_owned(),
            ));
        }

        let mut globals = Vec::new();
        let mut list_declarations = Vec::new();
        let mut external_functions = Vec::new();
        let mut consts = std::collections::HashMap::new();
        let mut root = Vec::new();
        let mut flows = Vec::new();
        let mut current_flow: Option<FlowBuilder> = None;
        let mut current_stitch: Option<FlowBuilder> = None;
        let mut line_index = 0;

        while line_index < lines.len() {
            let ln = line_index + 1;
            if let Some(header) = parse_header(lines[line_index].content) {
                match header {
                    Header::Knot {
                        name,
                        parameters,
                        ref_parameters,
                        divert_parameters,
                    } => {
                        finalize_stitch(&mut current_flow, &mut current_stitch)
                            .map_err(|e| e.with_line(ln))?;
                        if let Some(flow) = current_flow.take() {
                            flows.push(flow.build());
                        }

                        current_flow = Some(FlowBuilder {
                            name,
                            is_function: false,
                            is_root_stitch: false,
                            parameters,
                            ref_parameters,
                            divert_parameters,
                            nodes: Vec::new(),
                            children: Vec::new(),
                        });
                    }
                    Header::Function {
                        name,
                        parameters,
                        ref_parameters,
                        divert_parameters,
                    } => {
                        finalize_stitch(&mut current_flow, &mut current_stitch)
                            .map_err(|e| e.with_line(ln))?;
                        if let Some(flow) = current_flow.take() {
                            flows.push(flow.build());
                        }

                        current_flow = Some(FlowBuilder {
                            name,
                            is_function: true,
                            is_root_stitch: false,
                            parameters,
                            ref_parameters,
                            divert_parameters,
                            nodes: Vec::new(),
                            children: Vec::new(),
                        });
                    }
                    Header::Stitch {
                        name,
                        parameters,
                        ref_parameters,
                        divert_parameters,
                    } => {
                        finalize_stitch(&mut current_flow, &mut current_stitch)
                            .map_err(|e| e.with_line(ln))?;
                        let parent_is_root_stitch =
                            current_flow.as_ref().is_some_and(|f| f.is_root_stitch);
                        if current_flow.is_none() || parent_is_root_stitch {
                            // Top-level stitch (no parent knot, or sibling of another root stitch)
                            if let Some(flow) = current_flow.take() {
                                flows.push(flow.build());
                            }
                            current_flow = Some(FlowBuilder {
                                name,
                                is_function: false,
                                is_root_stitch: true,
                                parameters,
                                ref_parameters,
                                divert_parameters,
                                nodes: Vec::new(),
                                children: Vec::new(),
                            });
                        } else {
                            current_stitch = Some(FlowBuilder {
                                name,
                                is_function: false,
                                is_root_stitch: false,
                                parameters,
                                ref_parameters,
                                divert_parameters,
                                nodes: Vec::new(),
                                children: Vec::new(),
                            });
                        }
                    }
                }

                line_index += 1;
                continue;
            }

            let statement = parse_statement(&lines, &mut line_index, false)?;
            match statement {
                ParsedStatement::Global(global) => globals.push(global),
                ParsedStatement::Const(c) => {
                    consts.insert(c.name.clone(), c.initial_value);
                }
                ParsedStatement::List(list_decl) => list_declarations.push(list_decl),
                ParsedStatement::ExternalFunction(name) => external_functions.push(name),
                ParsedStatement::Nodes(mut nodes) => {
                    target_nodes(&mut root, current_flow.as_mut(), current_stitch.as_mut())
                        .append(&mut nodes)
                }
            }
        }

        finalize_stitch(&mut current_flow, &mut current_stitch)?;
        if let Some(flow) = current_flow.take() {
            flows.push(flow.build());
        }

        Ok({
            let mut story = ParsedStory::new(globals, root, flows);
            story.list_declarations = list_declarations;
            story.external_functions = external_functions;
            story.consts = consts;
            story
        })
    }
}

impl FlowBuilder {
    fn build(self) -> Flow {
        Flow {
            name: self.name,
            is_function: self.is_function,
            parameters: self.parameters,
            ref_parameters: self.ref_parameters,
            divert_parameters: self.divert_parameters,
            nodes: self.nodes,
            children: self.children,
        }
    }
}
fn finalize_stitch(
    current_flow: &mut Option<FlowBuilder>,
    current_stitch: &mut Option<FlowBuilder>,
) -> Result<(), CompilerError> {
    if let Some(stitch) = current_stitch.take() {
        let flow = current_flow.as_mut().ok_or_else(|| {
            CompilerError::invalid_source("stitch declared without enclosing knot".to_owned())
        })?;
        flow.children.push(stitch.build());
    }

    Ok(())
}

fn target_nodes<'a>(
    root: &'a mut Vec<Node>,
    current_flow: Option<&'a mut FlowBuilder>,
    current_stitch: Option<&'a mut FlowBuilder>,
) -> &'a mut Vec<Node> {
    if let Some(stitch) = current_stitch {
        &mut stitch.nodes
    } else if let Some(flow) = current_flow {
        &mut flow.nodes
    } else {
        root
    }
}
