use crate::{
    error::CompilerError,
    parsed_hierarchy::{
        ContentList, DebugMetadata, ExternalDeclaration, FlowArgument, FlowDecl, FlowLevel, List,
        ListDefinition, ListElementDefinition, Number, NumberValue, ParsedFlow, ParsedNode,
        ParsedNodeKind, Return as ParsedReturn, SequenceType, Story, Tag, VariableAssignment,
    },
    string_parser::{
        CharacterRange, CharacterSet, CommentEliminator, ParseSuccess, StringParser,
        StringParserStateElement,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum InkParseLevel {
    Top,
    Knot,
    Stitch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceMarker {
    pub indentation_depth: usize,
    pub once_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceClause {
    pub start_text: String,
    pub choice_only_text: Option<String>,
    pub inner_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatherMarker {
    pub indentation_depth: usize,
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceTypeAnnotation {
    pub flags: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivertPrototype {
    pub target: Vec<String>,
    pub arguments: Vec<String>,
    pub is_thread: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandLineInput {
    pub is_help: bool,
    pub is_exit: bool,
    pub choice_input: Option<i32>,
    pub debug_source: Option<i32>,
    pub debug_path_lookup: Option<String>,
    pub user_immediate_mode_statement: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InkParser {
    source_name: Option<String>,
    parser: StringParser,
    parsing_string_expression: bool,
    parsing_choice: bool,
    tag_active: bool,
}

#[derive(Default)]
struct ParseSection {
    nodes: Vec<ParsedNode>,
    flows: Vec<ParsedFlow>,
    global_declarations: Vec<VariableAssignment>,
    global_initializers: Vec<(String, crate::parsed_hierarchy::ParsedExpression)>,
    list_definitions: Vec<ListDefinition>,
    external_declarations: Vec<ExternalDeclaration>,
}

impl InkParser {
    pub fn new(input: impl Into<String>, source_name: Option<String>) -> Self {
        let processed = CommentEliminator::process(input.into());
        Self {
            source_name,
            parser: StringParser::new(processed),
            parsing_string_expression: false,
            parsing_choice: false,
            tag_active: false,
        }
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source_name.as_deref()
    }

    pub fn parse_story(mut self, count_all_visits: bool) -> Result<Story, CompilerError> {
        let source = self.parser.input_string().to_owned();
        let source_name = self.source_name.clone();
        let parsed = self.parse_at_level(InkParseLevel::Top);
        let mut story = Story::new(&source, source_name, count_all_visits);
        story.root_nodes = parsed.nodes;
        story.flows = parsed.flows;
        story.global_declarations = parsed.global_declarations;
        story.global_initializers = parsed.global_initializers;
        story.list_definitions = parsed.list_definitions;
        story.external_declarations = parsed.external_declarations;
        story.rebuild_parse_tree_refs();
        Ok(story)
    }

    pub fn parse_story_with_file_handler<F>(
        self,
        count_all_visits: bool,
        file_handler: F,
    ) -> Result<Story, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        parse_story_with_file_handler_inner(
            self.parser.input_string(),
            self.source_name.clone(),
            count_all_visits,
            &file_handler,
        )
    }
}

fn parse_story_with_file_handler_inner(
    source: &str,
    source_name: Option<String>,
    count_all_visits: bool,
    file_handler: &dyn Fn(&str) -> Result<String, CompilerError>,
) -> Result<Story, CompilerError> {
    let mut merged = Story::new(source, source_name.clone(), count_all_visits);
        let mut chunk = String::new();

        for line in source.lines() {
            let mut include_parser = InkParser::new(line, source_name.clone());
            if let Some(filename) = include_parser.include_statement_filename()
                && line.trim_start().starts_with("INCLUDE")
            {
                if !chunk.trim().is_empty() {
                    let chunk_story = InkParser::new(chunk.clone(), source_name.clone())
                        .parse_story(count_all_visits)?;
                    merge_story(&mut merged, chunk_story);
                    chunk.clear();
                }

                let included = file_handler(&filename)?;
                let included_story = parse_story_with_file_handler_inner(
                    &included,
                    Some(filename.clone()),
                    count_all_visits,
                    file_handler,
                )?;
                merge_story(&mut merged, included_story);
                continue;
            }

            chunk.push_str(line);
            chunk.push('\n');
        }

        if !chunk.trim().is_empty() {
            let chunk_story = InkParser::new(chunk, source_name).parse_story(count_all_visits)?;
            merge_story(&mut merged, chunk_story);
        }

        Ok(merged)
}

impl InkParser {

    pub fn parser(&self) -> &StringParser {
        &self.parser
    }

    pub fn parser_mut(&mut self) -> &mut StringParser {
        &mut self.parser
    }

    pub fn end_of_line(&mut self) -> Option<ParseSuccess> {
        self.newline().or_else(|| self.end_of_file())
    }

    pub fn newline(&mut self) -> Option<ParseSuccess> {
        let _ = self.whitespace();
        self.parser.parse_newline().map(|_| ParseSuccess)
    }

    pub fn end_of_file(&mut self) -> Option<ParseSuccess> {
        let _ = self.whitespace();
        self.parser.end_of_input().then_some(ParseSuccess)
    }

    pub fn multiline_whitespace(&mut self) -> Option<ParseSuccess> {
        let mut count = 0;
        while self.newline().is_some() {
            count += 1;
        }
        (count > 0).then_some(ParseSuccess)
    }

    pub fn whitespace(&mut self) -> Option<ParseSuccess> {
        self.parser
            .parse_characters_from_string(" \t", true, -1)
            .map(|_| ParseSuccess)
    }

    pub fn any_whitespace(&mut self) -> Option<ParseSuccess> {
        let mut found = false;
        loop {
            if self.whitespace().is_some() || self.multiline_whitespace().is_some() {
                found = true;
            } else {
                break;
            }
        }
        found.then_some(ParseSuccess)
    }

    pub fn spaced<T>(&mut self, mut rule: impl FnMut(&mut StringParser) -> Option<T>) -> Option<T> {
        let _ = self.whitespace();
        let result = self.parser.parse_object(|parser| rule(parser))?;
        let _ = self.whitespace();
        Some(result)
    }

    pub fn multispaced<T>(
        &mut self,
        mut rule: impl FnMut(&mut StringParser) -> Option<T>,
    ) -> Option<T> {
        let _ = self.any_whitespace();
        let result = self.parser.parse_object(|parser| rule(parser))?;
        let _ = self.any_whitespace();
        Some(result)
    }

    pub fn create_debug_metadata(
        &self,
        start: &StringParserStateElement,
        end: &StringParserStateElement,
    ) -> DebugMetadata {
        DebugMetadata {
            start_line_number: start.line_index() + 1,
            end_line_number: end.line_index() + 1,
            start_character_number: start.character_in_line_index() + 1,
            end_character_number: end.character_in_line_index() + 1,
            file_name: self.source_name.clone(),
        }
    }

    pub fn parse_identifier(&mut self) -> Option<String> {
        let first = self.parser.parse_characters_from_char_set(
            &self.identifier_character_set(),
            true,
            1,
        )?;
        let rest = self
            .parser
            .parse_characters_from_char_set(&self.identifier_character_set(), true, -1)
            .unwrap_or_default();
        Some(format!("{first}{rest}"))
    }

    pub fn bracketed_name(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let identifier = self.parse_identifier();
        if identifier.is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        if self.parser.parse_string(")").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.parser.succeed_rule(rule_id, identifier)
    }

    pub fn choice_marker(&mut self) -> Option<ChoiceMarker> {
        let _ = self.whitespace();
        let marker = self.parser.parse_single_character()?;
        let once_only = match marker {
            '*' => true,
            '+' => false,
            _ => return None,
        };

        let mut indentation_depth = 1;
        loop {
            if self
                .parser
                .parse_object(|parser| {
                    let _ = parser.parse_characters_from_string(" \t", true, -1);
                    match parser.parse_single_character() {
                        Some('*') | Some('+') => Some(ParseSuccess),
                        _ => None,
                    }
                })
                .is_some()
            {
                indentation_depth += 1;
            } else {
                break;
            }
        }

        Some(ChoiceMarker {
            indentation_depth,
            once_only,
        })
    }

    pub fn gather_marker(&mut self) -> Option<GatherMarker> {
        let _ = self.whitespace();
        let mut indentation_depth = 0;

        loop {
            if self
                .parser
                .parse_object(|parser| {
                    let _ = parser.parse_characters_from_string(" \t", true, -1);
                    if parser.parse_string("->").is_some() {
                        return None;
                    }
                    parser.parse_string("-").map(|_| ParseSuccess)
                })
                .is_some()
            {
                indentation_depth += 1;
                let _ = self.whitespace();
            } else {
                break;
            }
        }

        if indentation_depth == 0 {
            return None;
        }

        let identifier = self.bracketed_name();
        let _ = self.whitespace();

        Some(GatherMarker {
            indentation_depth,
            identifier,
        })
    }

    pub fn split_choice_clause(text: &str) -> ChoiceClause {
        let mut start_text = text.to_owned();
        let mut choice_only_text = None;
        let mut inner_text = String::new();

        if let Some(open) = text.find('[')
            && let Some(close) = text[open + 1..].find(']')
        {
            let close = open + 1 + close;
            start_text = text[..open].to_owned();
            choice_only_text = Some(text[open + 1..close].to_owned());
            inner_text = text[close + 1..].to_owned();
        }

        ChoiceClause {
            start_text,
            choice_only_text,
            inner_text,
        }
    }

    pub fn parse_number_literal(&mut self) -> Option<Number> {
        self.parser
            .peek(|parser| parser.parse_float())
            .map(|_| {
                Number::new(NumberValue::Float(
                    self.parser.parse_float().expect("peeked float"),
                ))
            })
            .or_else(|| {
                self.parser
                    .parse_int()
                    .map(|value| Number::new(NumberValue::Int(value)))
            })
    }

    pub fn parse_bool_literal(&mut self) -> Option<Number> {
        let identifier_characters = self.identifier_character_set();
        self.parser.parse_object(|parser| {
            match parser.parse_characters_from_char_set(&identifier_characters, true, -1) {
                Some(word) if word == "true" => Some(Number::new(NumberValue::Bool(true))),
                Some(word) if word == "false" => Some(Number::new(NumberValue::Bool(false))),
                _ => None,
            }
        })
    }

    pub fn sequence_type_symbol_annotation(&mut self) -> Option<SequenceTypeAnnotation> {
        let annotations = self
            .parser
            .parse_characters_from_string("!&~$ ", true, -1)?;

        let mut flags = 0u8;
        for ch in annotations.chars() {
            match ch {
                '!' => flags |= SequenceType::Once as u8,
                '&' => flags |= SequenceType::Cycle as u8,
                '~' => flags |= SequenceType::Shuffle as u8,
                '$' => flags |= SequenceType::Stopping as u8,
                ' ' => {}
                _ => return None,
            }
        }

        (flags != 0).then_some(SequenceTypeAnnotation { flags })
    }

    pub fn sequence_type_word_annotation(&mut self) -> Option<SequenceTypeAnnotation> {
        let rule_id = self.parser.begin_rule();
        let mut flags = 0u8;
        let mut parsed_any = false;

        loop {
            let checkpoint = self.parser.begin_rule();
            let Some(word) = self.parse_identifier() else {
                self.parser.cancel_rule(checkpoint);
                break;
            };

            let flag = match word.as_str() {
                "once" => SequenceType::Once as u8,
                "cycle" => SequenceType::Cycle as u8,
                "shuffle" => SequenceType::Shuffle as u8,
                "stopping" => SequenceType::Stopping as u8,
                _ => {
                    self.parser.fail_rule::<()>(checkpoint);
                    break;
                }
            };
            self.parser.succeed_rule(checkpoint, Some(()));
            parsed_any = true;
            flags |= flag;
            let _ = self.whitespace();
        }

        if !parsed_any || self.parser.parse_string(":").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.parser
            .succeed_rule(rule_id, Some(SequenceTypeAnnotation { flags }))
    }

    pub fn start_tag(&mut self) -> Option<Tag> {
        let _ = self.whitespace();
        self.parser.parse_string("#")?;
        let was_active = self.tag_active;
        self.tag_active = true;
        let _ = self.whitespace();
        Some(Tag::new(true, self.parsing_choice || was_active))
    }

    pub fn end_tag_if_necessary(&mut self) -> Option<Tag> {
        if !self.tag_active {
            return None;
        }

        self.tag_active = false;
        Some(Tag::new(false, self.parsing_choice))
    }

    pub fn parse_divert_arrow(&mut self) -> Option<String> {
        self.parser.parse_string("->")
    }

    pub fn parse_thread_arrow(&mut self) -> Option<String> {
        self.parser.parse_string("<-")
    }

    pub fn parse_divert_arrow_or_tunnel_onwards(&mut self) -> Option<String> {
        let mut count = 0;
        while self.parser.parse_string("->").is_some() {
            count += 1;
        }

        match count {
            0 => None,
            1 => Some("->".to_owned()),
            2 => Some("->->".to_owned()),
            _ => Some("->->".to_owned()),
        }
    }

    pub fn dot_separated_divert_path_components(&mut self) -> Option<Vec<String>> {
        let _ = self.whitespace();
        let first = self.parse_identifier()?;
        let _ = self.whitespace();
        let mut components = vec![first];

        while self
            .parser
            .parse_object(|parser| {
                let _ = parser.parse_characters_from_string(" \t", true, -1);
                parser.parse_string(".")?;
                let _ = parser.parse_characters_from_string(" \t", true, -1);
                Some(ParseSuccess)
            })
            .is_some()
        {
            components.push(self.parse_identifier()?);
        }

        Some(components)
    }

    pub fn expression_function_call_arguments(&mut self) -> Option<Vec<String>> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let mut arguments = Vec::new();
        loop {
            let _ = self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }

            let argument = self
                .parser
                .parse_until_characters_from_string(",)", -1)?
                .trim()
                .to_owned();
            arguments.push(argument);
            let _ = self.whitespace();

            if self.parser.parse_string(")").is_some() {
                break;
            }
            self.parser.parse_string(",")?;
        }

        self.parser.succeed_rule(rule_id, Some(arguments))
    }

    pub fn divert_identifier_with_arguments(&mut self) -> Option<DivertPrototype> {
        let _ = self.whitespace();
        let target = self.dot_separated_divert_path_components()?;
        let _ = self.whitespace();
        let arguments = self
            .expression_function_call_arguments()
            .unwrap_or_default();
        let _ = self.whitespace();

        Some(DivertPrototype {
            target,
            arguments,
            is_thread: false,
        })
    }

    pub fn start_thread(&mut self) -> Option<DivertPrototype> {
        let _ = self.whitespace();
        self.parse_thread_arrow()?;
        let _ = self.whitespace();
        let mut divert = self.divert_identifier_with_arguments()?;
        divert.is_thread = true;
        Some(divert)
    }

    pub fn flow_decl_argument(&mut self) -> Option<FlowArgument> {
        let first = self.parse_identifier();
        let _ = self.whitespace();
        let divert_arrow = self.parse_divert_arrow();
        let _ = self.whitespace();
        let second = self.parse_identifier();

        if first.is_none() && second.is_none() {
            return None;
        }

        let is_divert_target = divert_arrow.is_some();
        if matches!(first.as_deref(), Some("ref")) {
            return Some(FlowArgument {
                identifier: second.unwrap_or_default(),
                is_by_reference: true,
                is_divert_target,
            });
        }

        Some(FlowArgument {
            identifier: if is_divert_target {
                second.unwrap_or_default()
            } else {
                first.unwrap_or_default()
            },
            is_by_reference: false,
            is_divert_target,
        })
    }

    pub fn bracketed_knot_decl_arguments(&mut self) -> Option<Vec<FlowArgument>> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let mut arguments = Vec::new();
        loop {
            let _ = self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }

            let argument = self.flow_decl_argument()?;
            arguments.push(argument);
            let _ = self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }
            self.parser.parse_string(",")?;
        }

        self.parser.succeed_rule(rule_id, Some(arguments))
    }

    pub fn knot_declaration(&mut self) -> Option<FlowDecl> {
        let rule_id = self.parser.begin_rule();
        let equals = self.parser.parse_characters_from_string("=", true, -1)?;
        if equals.len() <= 1 {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let identifier = self.parse_identifier()?;
        let (is_function, name) = if identifier == "function" {
            let _ = self.whitespace();
            (true, self.parse_identifier()?)
        } else {
            (false, identifier)
        };

        let _ = self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();
        let _ = self.whitespace();
        let _ = self.parser.parse_characters_from_string("=", true, -1);

        self.parser.succeed_rule(
            rule_id,
            Some(FlowDecl {
                name,
                arguments,
                is_function,
            }),
        )
    }

    pub fn stitch_declaration(&mut self) -> Option<FlowDecl> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        self.parser.parse_string("=")?;
        if self.parser.parse_string("=").is_some() {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let is_function = self.parser.parse_string("function").is_some();
        if is_function {
            let _ = self.whitespace();
        }
        let name = self.parse_identifier()?;
        let _ = self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();

        self.parser.succeed_rule(
            rule_id,
            Some(FlowDecl {
                name,
                arguments,
                is_function,
            }),
        )
    }

    pub fn external_declaration(&mut self) -> Option<ExternalDeclaration> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let keyword = self.parse_identifier()?;
        if keyword != "EXTERNAL" {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let name = self.parse_identifier()?;
        let _ = self.whitespace();
        let arguments = self
            .bracketed_knot_decl_arguments()
            .unwrap_or_default()
            .into_iter()
            .map(|arg| arg.identifier)
            .collect();

        self.parser
            .succeed_rule(rule_id, Some(ExternalDeclaration::new(name, arguments)))
    }

    pub fn list_declaration(&mut self) -> Option<VariableAssignment> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let keyword = self.parse_identifier()?;
        if keyword != "LIST" {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let var_name = self.parse_identifier()?;
        let _ = self.whitespace();
        if self.parser.parse_string("=").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();

        let mut definition = self.list_definition()?;
        definition.set_identifier(var_name.clone());

        let assignment = VariableAssignment::new(var_name, None);
        self.parser.succeed_rule(rule_id, Some(assignment))
    }

    pub fn list_definition(&mut self) -> Option<ListDefinition> {
        let _ = self.any_whitespace();
        let mut elements = Vec::new();

        let first = self.list_element_definition()?;
        elements.push(first);

        loop {
            let checkpoint = self.parser.begin_rule();
            let _ = self.any_whitespace();
            if self.parser.parse_string(",").is_none() {
                self.parser.cancel_rule(checkpoint);
                break;
            }
            let _ = self.any_whitespace();
            let Some(element) = self.list_element_definition() else {
                return self.parser.fail_rule(checkpoint);
            };
            self.parser.succeed_rule(checkpoint, Some(()));
            elements.push(element);
        }

        Some(ListDefinition::new(elements))
    }

    pub fn list_element_definition(&mut self) -> Option<ListElementDefinition> {
        let rule_id = self.parser.begin_rule();
        let in_initial_list = self.parser.parse_string("(").is_some();
        let mut needs_to_close_paren = in_initial_list;

        let _ = self.whitespace();
        let name = self.parse_identifier()?;
        let _ = self.whitespace();

        if in_initial_list && self.parser.parse_string(")").is_some() {
            needs_to_close_paren = false;
            let _ = self.whitespace();
        }

        let mut explicit_value = None;
        if self.parser.parse_string("=").is_some() {
            let _ = self.whitespace();
            let number = self.parse_number_literal()?;
            let NumberValue::Int(value) = number.value() else {
                return self.parser.fail_rule(rule_id);
            };
            explicit_value = Some(*value);

            if needs_to_close_paren {
                let _ = self.whitespace();
                if self.parser.parse_string(")").is_some() {
                    needs_to_close_paren = false;
                }
            }
        }

        if needs_to_close_paren {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(
            rule_id,
            Some(ListElementDefinition::new(
                name,
                in_initial_list,
                explicit_value,
            )),
        )
    }

    pub fn list_expression(&mut self) -> Option<List> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        if self.parser.parse_string(")").is_some() {
            return self.parser.succeed_rule(rule_id, Some(List::new(None)));
        }

        let mut items = Vec::new();
        loop {
            let path = self.dot_separated_divert_path_components()?;
            items.push(path.join("."));
            let _ = self.whitespace();

            if self.parser.parse_string(")").is_some() {
                break;
            }
            self.parser.parse_string(",")?;
            let _ = self.whitespace();
        }

        self.parser
            .succeed_rule(rule_id, Some(List::new(Some(items))))
    }

    pub fn command_line_user_input(&mut self) -> Option<CommandLineInput> {
        let _ = self.whitespace();

        if self.parser.parse_string("help").is_some() {
            return Some(CommandLineInput {
                is_help: true,
                ..Default::default()
            });
        }

        if self.parser.parse_string("exit").is_some() || self.parser.parse_string("quit").is_some()
        {
            return Some(CommandLineInput {
                is_exit: true,
                ..Default::default()
            });
        }

        self.debug_source()
            .or_else(|| self.debug_path_lookup())
            .or_else(|| self.user_choice_number())
            .or_else(|| self.user_immediate_mode_statement())
    }

    fn debug_source(&mut self) -> Option<CommandLineInput> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        if self.parser.parse_string("DebugSource").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        self.parser.parse_string("(")?;
        let _ = self.whitespace();
        let offset = self.parser.parse_int()?;
        let _ = self.whitespace();
        self.parser.parse_string(")")?;

        self.parser.succeed_rule(
            rule_id,
            Some(CommandLineInput {
                debug_source: Some(offset),
                ..Default::default()
            }),
        )
    }

    fn debug_path_lookup(&mut self) -> Option<CommandLineInput> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        if self.parser.parse_string("DebugPath").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.whitespace()?;
        let path = self.parser.parse_characters_from_char_set(
            &self.runtime_path_character_set(),
            true,
            -1,
        )?;

        self.parser.succeed_rule(
            rule_id,
            Some(CommandLineInput {
                debug_path_lookup: Some(path),
                ..Default::default()
            }),
        )
    }

    fn user_choice_number(&mut self) -> Option<CommandLineInput> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let number = self.parser.parse_int()?;
        let _ = self.whitespace();
        if self.end_of_line().is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(
            rule_id,
            Some(CommandLineInput {
                choice_input: Some(number),
                ..Default::default()
            }),
        )
    }

    fn user_immediate_mode_statement(&mut self) -> Option<CommandLineInput> {
        let _ = self.whitespace();
        let statement = self
            .parser
            .parse_until_characters_from_string("\n\r", -1)?
            .trim()
            .to_owned();
        (!statement.is_empty()).then_some(CommandLineInput {
            user_immediate_mode_statement: Some(statement),
            ..Default::default()
        })
    }

    pub fn include_statement_filename(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let keyword = self.parse_identifier()?;
        if keyword != "INCLUDE" {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let filename = self
            .parser
            .parse_until_characters_from_string("\n\r", -1)?
            .trim_end_matches([' ', '\t'])
            .to_owned();
        self.parser.succeed_rule(rule_id, Some(filename))
    }

    fn try_parse_external_declaration_line(&mut self) -> Option<ExternalDeclaration> {
        let rule_id = self.parser.begin_rule();
        let declaration = self.external_declaration()?;
        let _ = self.end_of_line();
        self.parser.succeed_rule(rule_id, Some(declaration))
    }

    fn try_parse_list_statement(&mut self) -> Option<ListDefinition> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let keyword = self.parse_identifier()?;
        if keyword != "LIST" {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let var_name = self.parse_identifier()?;
        let _ = self.whitespace();
        if self.parser.parse_string("=").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();

        let mut definition = self.list_definition()?;
        definition.set_identifier(var_name.clone());
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(definition))
    }

    fn try_parse_global_declaration_statement(
        &mut self,
    ) -> Option<(VariableAssignment, (String, crate::parsed_hierarchy::ParsedExpression))> {
        let rule_id = self.parser.begin_rule();
        let node = self.try_parse_assignment_line()?;
        let encoded = node.name()?;
        let (mode, name) = encoded.split_once(':')?;
        if mode != "GlobalDecl" {
            return self.parser.fail_rule(rule_id);
        }
        let expression = node.expression()?.clone();
        let mut declaration = VariableAssignment::new(name, None);
        declaration.set_global_declaration(true);
        self.parser
            .succeed_rule(rule_id, Some((declaration, (name.to_owned(), expression))))
    }

    pub fn return_statement(&mut self) -> Option<ParsedReturn> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let keyword = self.parse_identifier()?;
        if keyword != "return" {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        self.parser
            .succeed_rule(rule_id, Some(ParsedReturn::new(None)))
    }

    pub fn content_text(&mut self) -> Option<String> {
        self.content_text_allowing_escape_char()
    }

    pub fn content_text_allowing_escape_char(&mut self) -> Option<String> {
        let mut out = String::new();

        loop {
            let part = self.content_text_no_escape();
            let got_escape = self.parser.parse_string("\\").is_some();
            let had_part = part.is_some();

            if let Some(part) = part {
                out.push_str(&part);
            }

            if got_escape {
                if let Some(ch) = self.parser.parse_single_character() {
                    out.push(ch);
                }
            }

            if !had_part && !got_escape {
                break;
            }
        }

        if out.is_empty() { None } else { Some(out) }
    }

    pub fn line_of_mixed_text(&mut self) -> Option<ContentList> {
        let _ = self.whitespace();
        let text = self.content_text()?;
        let mut list = ContentList::new();
        list.push_text(text);
        list.trim_trailing_whitespace();
        list.push_text("\n");
        let _ = self.end_of_line();
        Some(list)
    }

    fn content_text_no_escape(&mut self) -> Option<String> {
        let pause_characters = CharacterSet::from("-<");

        let mut end_characters = CharacterSet::from("{}|\n\r\\#");
        if self.parsing_choice {
            end_characters = end_characters.add_characters("[]".chars());
        }
        if self.parsing_string_expression {
            end_characters = end_characters.add_characters("\"".chars());
        }

        self.parser.parse_until(
            |parser| {
                parser
                    .parse_string("->")
                    .or_else(|| parser.parse_string("<-"))
                    .or_else(|| parser.parse_string("<>"))
                    .or_else(|| {
                        if parser.parse_newline().is_some() {
                            Some(String::new())
                        } else {
                            None
                        }
                    })
            },
            Some(&pause_characters),
            Some(&end_characters),
        )
    }

    fn identifier_character_set(&self) -> CharacterSet {
        let mut set = CharacterSet::from("0123456789_");
        for mut range in Self::list_all_character_ranges() {
            set = set.add_characters(range.to_character_set().into_iter());
        }
        set
    }

    fn runtime_path_character_set(&self) -> CharacterSet {
        self.identifier_character_set()
            .add_characters(['-', '.'].into_iter())
    }

    fn list_all_character_ranges() -> Vec<CharacterRange> {
        vec![
            CharacterRange::define('\u{0041}', '\u{007A}', "[\\]^_`".chars()),
            CharacterRange::define('\u{0100}', '\u{017F}', [].into_iter()),
            CharacterRange::define('\u{0180}', '\u{024F}', [].into_iter()),
            CharacterRange::define(
                '\u{0370}',
                '\u{03FF}',
                "\u{0374}\u{0375}\u{0378}\u{0387}\u{038B}\u{038D}\u{03A2}"
                    .chars()
                    .chain('\u{0378}'..='\u{0385}'),
            ),
            CharacterRange::define('\u{0400}', '\u{04FF}', '\u{0482}'..='\u{0489}'),
            CharacterRange::define(
                '\u{0530}',
                '\u{058F}',
                "\u{0530}"
                    .chars()
                    .chain('\u{0557}'..='\u{0560}')
                    .chain('\u{0588}'..='\u{058E}'),
            ),
            CharacterRange::define('\u{0590}', '\u{05FF}', [].into_iter()),
            CharacterRange::define('\u{0600}', '\u{06FF}', [].into_iter()),
            CharacterRange::define('\u{AC00}', '\u{D7AF}', [].into_iter()),
            CharacterRange::define('\u{0080}', '\u{00FF}', [].into_iter()),
            CharacterRange::define('\u{4E00}', '\u{9FFF}', [].into_iter()),
            CharacterRange::define('\u{3041}', '\u{3096}', [].into_iter()),
            CharacterRange::define('\u{30A0}', '\u{30FC}', [].into_iter()),
        ]
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Story-level parsing
    // ─────────────────────────────────────────────────────────────────────────

    /// The nesting level inside which statements are being parsed.
    /// Mirrors the `StatementLevel` enum in the C# reference parser.
    fn parse_at_level(&mut self, level: InkParseLevel) -> ParseSection {
        let mut parsed = ParseSection::default();

        loop {
            self.multiline_whitespace();
            if self.parser.end_of_input() {
                break;
            }

            // ── Break conditions and sub-flow definitions ──────────────────
            match level {
                InkParseLevel::Stitch => {
                    if self.peek_knot_start() || self.peek_stitch_start() {
                        break;
                    }
                }
                InkParseLevel::Knot => {
                    if self.peek_knot_start() {
                        break;
                    }
                    if let Some(stitch) = self.try_parse_stitch() {
                        parsed.flows.push(stitch);
                        continue;
                    }
                }
                InkParseLevel::Top => {
                    if let Some(external) = self.try_parse_external_declaration_line() {
                        parsed.external_declarations.push(external);
                        continue;
                    }
                    if let Some(definition) = self.try_parse_list_statement() {
                        parsed.list_definitions.push(definition);
                        continue;
                    }
                    if let Some((declaration, initializer)) = self.try_parse_global_declaration_statement() {
                        parsed.global_declarations.push(declaration);
                        parsed.global_initializers.push(initializer);
                        continue;
                    }
                    if let Some(knot) = self.try_parse_knot() {
                        parsed.flows.push(knot);
                        continue;
                    }
                    if let Some(stitch) = self.try_parse_stitch() {
                        parsed.flows.push(stitch);
                        continue;
                    }
                }
            }

            // ── Statement rules (shared at all levels) ─────────────────────
            if let Some(choice) = self.try_parse_choice() {
                parsed.nodes.push(choice);
                continue;
            }

            if let Some(gather) = self.try_parse_gather() {
                parsed.nodes.push(gather);
                continue;
            }

            if let Some(assignment) = self.try_parse_assignment_line() {
                parsed.nodes.push(assignment);
                continue;
            }

            if let Some(logic) = self.try_parse_logic_line() {
                parsed.nodes.push(logic);
                continue;
            }

            if let Some(divert) = self.try_parse_conditional_divert_line() {
                parsed.nodes.push(divert);
                continue;
            }

            if let Some(mut line) = self.try_parse_divert_line() {
                parsed.nodes.append(&mut line);
                continue;
            }

            if let Some(seq_node) = self.try_parse_multiline_sequence() {
                parsed.nodes.push(seq_node);
                continue;
            }

            if let Some(conditional) = self.try_parse_multiline_conditional() {
                parsed.nodes.push(conditional);
                continue;
            }

            if let Some(mut line) = self.try_parse_mixed_line() {
                parsed.nodes.append(&mut line);
                continue;
            }

            // Nothing matched – skip to next line
            self.skip_line();
        }

        parsed
    }

    // ── Peek helpers ──────────────────────────────────────────────────────────

    /// Returns true if the next non-whitespace content looks like a knot header (`==…`).
    fn peek_knot_start(&mut self) -> bool {
        self.parser
            .peek(|p| {
                let _ = p.parse_characters_from_string(" \t", true, -1);
                let eq = p.parse_characters_from_string("=", true, -1)?;
                if eq.len() > 1 { Some(()) } else { None }
            })
            .is_some()
    }

    /// Returns true if the next non-whitespace content looks like a stitch header (`= name`).
    fn peek_stitch_start(&mut self) -> bool {
        self.parser
            .peek(|p| {
                let _ = p.parse_characters_from_string(" \t", true, -1);
                p.parse_string("=")?;
                // A second `=` means it's a knot, not a stitch
                if p.parse_string("=").is_some() {
                    return None;
                }
                Some(())
            })
            .is_some()
    }

    // ── Flow definitions ──────────────────────────────────────────────────────

    /// Try to parse a knot (or function) definition starting with `==`.
    fn try_parse_knot(&mut self) -> Option<ParsedFlow> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        // Must start with at least `==`
        match self.parser.parse_characters_from_string("=", true, -1) {
            Some(ref s) if s.len() > 1 => {}
            _ => return self.parser.fail_rule(rule_id),
        }

        self.whitespace();

        let Some(ident) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };

        let (is_function, name) = if ident == "function" {
            self.whitespace();
            let Some(fn_name) = self.parse_identifier() else {
                return self.parser.fail_rule(rule_id);
            };
            (true, fn_name)
        } else {
            (false, ident)
        };

        self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();
        self.whitespace();
        // Optional trailing `===`
        let _ = self.parser.parse_characters_from_string("=", true, -1);

        // Consume the rest of the header line
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        // Parse the knot body
        let parsed = self.parse_at_level(InkParseLevel::Knot);

        Some(ParsedFlow::new(
            name,
            FlowLevel::Knot,
            arguments,
            is_function,
            parsed.nodes,
            parsed.flows,
        ))
    }

    /// Try to parse a stitch definition starting with a single `=`.
    fn try_parse_stitch(&mut self) -> Option<ParsedFlow> {
        // Peek first to avoid leaving orphaned begin_rule on the stack when failing
        if !self.peek_stitch_start() {
            return None;
        }

        let rule_id = self.parser.begin_rule();
        self.whitespace();

        // Single `=`
        let Some(_) = self.parser.parse_string("=") else {
            return self.parser.fail_rule(rule_id);
        };
        // Reject `==` (which is a knot)
        if self.parser.parse_string("=").is_some() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        // Optional `function` keyword
        let mut is_function = false;
        let kw_rule = self.parser.begin_rule();
        if let Some(kw) = self.parse_identifier() {
            if kw == "function" {
                self.parser.succeed_rule(kw_rule, Some(()));
                self.whitespace();
                is_function = true;
            } else {
                // Not `function` – restore and re-parse as the stitch name below
                self.parser.fail_rule::<()>(kw_rule);
            }
        } else {
            self.parser.cancel_rule(kw_rule);
        }

        let Some(name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };

        self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();
        // Consume the rest of the header line
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        let parsed = self.parse_at_level(InkParseLevel::Stitch);

        Some(ParsedFlow::new(
            name,
            FlowLevel::Stitch,
            arguments,
            is_function,
            parsed.nodes,
            Vec::new(), // stitches have no children
        ))
    }

    // ── Statement parsers ─────────────────────────────────────────────────────

    /// Parse a line that is *only* a divert: `-> target`.
    fn try_parse_divert_line(&mut self) -> Option<Vec<ParsedNode>> {
        let rule_id = self.parser.begin_rule();

        self.whitespace();

        if self.parser.parse_string("->->").is_some() {
            self.whitespace();
            let node = if let Some(parts) = self.dot_separated_divert_path_components() {
                ParsedNode::new(ParsedNodeKind::TunnelOnwardsWithTarget)
                    .with_target(parts.join("."))
            } else {
                ParsedNode::new(ParsedNodeKind::TunnelReturn)
            };
            let _ = self.end_of_line();
            return self.parser.succeed_rule(rule_id, Some(vec![node]));
        }

        if self.parser.parse_string("->").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let target_components = self.dot_separated_divert_path_components();
        let _ = self.end_of_line();

        let nodes = match target_components {
            Some(parts) => {
                let target = parts.join(".");
                let _ = self.expression_function_call_arguments(); // optional args
                self.whitespace();
                if self.parser.parse_string("->").is_some() {
                    vec![ParsedNode::new(ParsedNodeKind::TunnelDivert).with_target(target)]
                } else {
                    vec![ParsedNode::new(ParsedNodeKind::Divert).with_target(target)]
                }
            }
            None => vec![], // bare `->` (gather-style, no target)
        };

        let _ = self.end_of_line();
        self.parser.succeed_rule(rule_id, Some(nodes))
    }

    fn try_parse_assignment_line(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();

        self.whitespace();

        let default_mode = if self.parser.parse_string("~").is_some() {
            self.whitespace();
            if self
                .parser
                .parse_object(|parser| {
                    parser.parse_string("temp")?;
                    let _ = parser.parse_characters_from_string(" \t", true, -1);
                    Some(())
                })
                .is_some()
            {
                "TempSet"
            } else {
                "Set"
            }
        } else if self.parser.parse_string("VAR").is_some() {
            self.whitespace();
            "GlobalDecl"
        } else {
            return self.parser.fail_rule(rule_id);
        };

        let name = self.parse_identifier()?;
        self.whitespace();

        let (mode, expression_override) = if self.parser.parse_string("+=").is_some() {
            ("AddAssign", None)
        } else if self.parser.parse_string("-=").is_some() {
            ("SubtractAssign", None)
        } else if self.parser.parse_string("++").is_some() {
            ("AddAssign", Some(crate::parsed_hierarchy::ParsedExpression::Int(1)))
        } else if self.parser.parse_string("--").is_some() {
            (
                "SubtractAssign",
                Some(crate::parsed_hierarchy::ParsedExpression::Int(1)),
            )
        } else if self.parser.parse_string("=").is_some() {
            (default_mode, None)
        } else {
            return self.parser.fail_rule(rule_id);
        };

        let expression = if let Some(expression) = expression_override {
            let _ = self.end_of_line();
            expression
        } else {
            self.whitespace();
            let expression_text = self
                .parser
                .parse_until_characters_from_string("\n\r", -1)?;
            let _ = self.end_of_line();
            parse_expression_text(expression_text.trim())?
        };
        self.parser.succeed_rule(rule_id, Some(()));

        Some(
            ParsedNode::new(ParsedNodeKind::Assignment)
                .with_name(format!("{mode}:{name}"))
                .with_expression(expression),
        )
    }

    fn try_parse_conditional_divert_line(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        // Use a closure so we can properly call fail_rule on any early return
        let result = (|| {
            self.parser.parse_string("{")?;
            // Only read single-line content — multi-line blocks are NOT conditional diverts
            let content = self.parser.parse_until_characters_from_string("}\n\r", -1)?;
            // Must close on same line (no newline before `}`)
            self.parser.parse_string("}")?;
            let _ = self.end_of_line();

            let (condition_text, branch_text) = content.split_once(':')?;
            let branch_text = branch_text.trim();
            let target = branch_text.strip_prefix("->")?.trim();
            let condition = parse_expression_text(condition_text.trim())?;
            Some((condition, target.to_owned()))
        })();

        match result {
            Some((condition, target)) => {
                self.parser.succeed_rule(rule_id, Some(()));
                Some(
                    ParsedNode::new(ParsedNodeKind::Divert)
                        .with_target(target)
                        .with_condition(condition),
                )
            }
            None => self.parser.fail_rule(rule_id),
        }
    }

    fn try_parse_logic_line(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();
        if self.parser.parse_string("~").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.whitespace();

        if let Some(keyword) = self.parse_identifier()
            && keyword == "return"
        {
            self.whitespace();
            let expression_text = self
                .parser
                .parse_until_characters_from_string("\n\r", -1)
                .unwrap_or_default();
            let _ = self.end_of_line();
            self.parser.succeed_rule(rule_id, Some(()));

            let trimmed = expression_text.trim();
            return if trimmed.is_empty() {
                Some(ParsedNode::new(ParsedNodeKind::ReturnVoid))
            } else {
                Some(
                    ParsedNode::new(ParsedNodeKind::ReturnExpression)
                        .with_expression(parse_expression_text(trimmed)?),
                )
            };
        }

        self.parser.fail_rule::<()>(rule_id);
        let rule_id = self.parser.begin_rule();
        self.whitespace();
        if self.parser.parse_string("~").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.whitespace();
        let expression_text = self
            .parser
            .parse_until_characters_from_string("\n\r", -1)?;
        let _ = self.end_of_line();
        let expression = parse_expression_text(expression_text.trim())?;
        match expression {
            crate::parsed_hierarchy::ParsedExpression::FunctionCall { .. } => self
                .parser
                .succeed_rule(rule_id, Some(ParsedNode::new(ParsedNodeKind::VoidCall).with_expression(expression))),
            _ => self.parser.fail_rule(rule_id),
        }
    }

    /// Parse a line of mixed content: text, `<>` glue, optional trailing divert.
    ///
    /// Mirrors `LineOfMixedTextAndLogic` / `MixedTextAndLogic` in the C# parser,
    /// restricted to the subset we currently handle.
    fn try_parse_mixed_line(&mut self) -> Option<Vec<ParsedNode>> {
        self.whitespace();

        let mut nodes: Vec<ParsedNode> = Vec::new();
        let mut had_content = false;

        // Interleave: text … inline-expressions … glue …
        loop {
            if let Some(text) = self.content_text() {
                if !text.is_empty() {
                    nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(text));
                }
                had_content = true;
            }

            // Handle inline {…} expressions/sequences
            if self.parser.peek(|parser| parser.parse_string("{")).is_some() {
                let mut expr_nodes = self.parse_braced_inline_content("\n\r")
                    .unwrap_or_default();
                if !expr_nodes.is_empty() {
                    had_content = true;
                }
                nodes.append(&mut expr_nodes);
                continue;
            }

            if self.parser.parse_string("<>").is_some() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Glue));
                had_content = true;
                continue; // look for more text after glue
            }

            break;
        }

        // Optional trailing divert (`-> target`)
        if let Some(divert) = self.try_parse_inline_divert() {
            Self::append_trailing_space(&mut nodes);
            nodes.push(divert);
            had_content = true;
        } else if !had_content {
            return None;
        } else {
            Self::trim_trailing_whitespace_nodes(&mut nodes);
        }

        // Newline at the end of every non-pure-tag line (mirrors C# behaviour)
        nodes.push(ParsedNode::new(ParsedNodeKind::Newline));
        let _ = self.end_of_line();

        Some(nodes)
    }

    /// Try to parse an inline `-> target` (does not consume a trailing newline).
    fn try_parse_inline_divert(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();

        if self.parser.parse_string("->->").is_some() {
            self.whitespace();
            let node = if let Some(parts) = self.dot_separated_divert_path_components() {
                ParsedNode::new(ParsedNodeKind::TunnelOnwardsWithTarget)
                    .with_target(parts.join("."))
            } else {
                ParsedNode::new(ParsedNodeKind::TunnelReturn)
            };
            return self.parser.succeed_rule(rule_id, Some(node));
        }

        if self.parser.parse_string("->").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let Some(parts) = self.dot_separated_divert_path_components() else {
            // Bare `->` with no target (gather-style)
            return self.parser.succeed_rule(rule_id, None);
        };

        let target = parts.join(".");
        let _ = self.expression_function_call_arguments(); // optional args
        self.whitespace();

        let node = if self.parser.parse_string("->").is_some() {
            ParsedNode::new(ParsedNodeKind::TunnelDivert).with_target(target)
        } else {
            ParsedNode::new(ParsedNodeKind::Divert).with_target(target)
        };
        self.parser.succeed_rule(rule_id, Some(node))
    }

    // ── Text-node helpers ──────────────────────────────────────────────────────

    /// Trim trailing inline whitespace from the last `Text` node.
    /// If the node becomes empty it is removed (and the previous node is also trimmed
    /// recursively, matching C# `TrimEndWhitespace`).
    fn trim_trailing_whitespace_nodes(nodes: &mut Vec<ParsedNode>) {
        if let Some(last) = nodes.last_mut() {
            if last.kind() == ParsedNodeKind::Text {
                let trimmed = last
                    .text()
                    .unwrap_or("")
                    .trim_end_matches([' ', '\t'])
                    .to_owned();
                if trimmed.is_empty() {
                    nodes.pop();
                    // Recurse – there may be more trailing whitespace-only nodes
                    Self::trim_trailing_whitespace_nodes(nodes);
                } else {
                    *last = ParsedNode::new(ParsedNodeKind::Text).with_text(trimmed);
                }
            }
        }
    }

    /// Trim trailing whitespace from the last `Text` node and append a single
    /// space.  Used before a trailing divert (`TrimEndWhitespace(terminateWithSpace:true)`
    /// in C#).
    fn append_trailing_space(nodes: &mut Vec<ParsedNode>) {
        if let Some(last) = nodes.last_mut() {
            if last.kind() == ParsedNodeKind::Text {
                let trimmed = last
                    .text()
                    .unwrap_or("")
                    .trim_end_matches([' ', '\t'])
                    .to_owned();
                *last = ParsedNode::new(ParsedNodeKind::Text).with_text(format!("{trimmed} "));
            }
        }
    }

    /// Try to parse a choice line: `* text` / `+ text` / `* [text]` / etc.
    ///
    /// Returns a `ParsedNode::Choice` with:
    /// - `indentation_depth`: number of `*`/`+` markers
    /// - `once_only`: true for `*`, false for `+`
    /// - `start_content`: text before `[` (shown in list and after choosing)
    /// - `choice_only_content`: text inside `[…]` (shown only in list)
    /// - `children`: inner content nodes (trailing same-line divert etc.)
    /// - `name`: optional label `(identifier)` after the markers
    fn try_parse_choice(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();

        // Optional leading horizontal whitespace
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        // Detect the first marker character: `*` or `+`
        let first_char = self.parser.peek(|p| p.parse_single_character())?;
        let is_choice = first_char == '*' || first_char == '+';
        if !is_choice {
            return self.parser.fail_rule(rule_id);
        }

        // Count consecutive identical markers (depth)
        let once_only = first_char == '*';
        let marker_char = if once_only { "*" } else { "+" };
        let markers = self
            .parser
            .parse_characters_from_string(marker_char, true, -1)?;
        let depth = markers.len();

        // Optional horizontal whitespace after markers
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        // Optional label `(identifier)` before the content
        let label = self.try_parse_inline_label();

        // Optional additional whitespace
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        let choice_condition = self.parse_choice_condition();

        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        // Start content: text before `[` or end-of-line
        let start_nodes = self.parse_choice_start_content();

        // Choice-only content: text inside `[…]`
        let mut choice_only_nodes = self.parse_choice_only_content();

        // Inner content: everything after `]` on the same line (may be text + optional divert)
        let inner_nodes = self.parse_choice_inner_content();

        if should_close_quoted_choice(&start_nodes, &choice_only_nodes, &inner_nodes) {
            append_text_suffix(&mut choice_only_nodes, "'");
        }

        // Consume the rest of the line
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        let mut node = ParsedNode::new(ParsedNodeKind::Choice);
        node.indentation_depth = depth;
        node.once_only = once_only;
        node.is_invisible_default = start_nodes.is_empty() && choice_only_nodes.is_empty();
        node.start_content = if choice_only_nodes.is_empty() {
            trim_end_whitespace_nodes(start_nodes)
        } else {
            start_nodes
        };
        node.choice_only_content = choice_only_nodes;
        if let Some(condition) = choice_condition {
            node = node.with_condition(condition);
        }
        if let Some(label_name) = label {
            node = node.with_name(label_name);
        }
        // Inner content becomes `children`
        if !inner_nodes.is_empty() {
            node = node.with_children(inner_nodes);
        }
        Some(node)
    }

    /// Parse the start content of a choice (text before `[` or end-of-line).
    fn parse_choice_start_content(&mut self) -> Vec<ParsedNode> {
        self.parse_inline_content_until("[\n\r")
    }

    /// Parse choice-only content inside `[…]`.
    fn parse_choice_only_content(&mut self) -> Vec<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("[").is_none() {
            self.parser.cancel_rule(rule_id);
            return Vec::new();
        }
        let nodes = self.parse_inline_content_until("]");
        // Consume closing `]`; if missing, treat as empty choice-only
        let _ = self.parser.parse_string("]");
        self.parser.succeed_rule(rule_id, Some(()));
        nodes
    }

    /// Parse any inner content after `]` on the same choice line (text + optional divert).
    fn parse_choice_inner_content(&mut self) -> Vec<ParsedNode> {
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);
        let mut nodes = trim_end_whitespace_nodes(self.parse_inline_content_until("\n\r"));
        if let Some(divert) = self.try_parse_inline_divert() {
            if !nodes.is_empty() {
                Self::append_trailing_space(&mut nodes);
            }
            nodes.push(divert);
        }
        nodes
    }

    fn parse_choice_condition(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let mut conditions = Vec::new();

        loop {
            let rule_id = self.parser.begin_rule();
            let _ = self.parser.parse_characters_from_string(" \t", true, -1);
            if self.parser.parse_string("{").is_none() {
                self.parser.cancel_rule(rule_id);
                break;
            }

            let Some(condition_text) = self.parser.parse_until_characters_from_string("}", -1) else {
                return None;
            };
            if self.parser.parse_string("}").is_none() {
                return None;
            }

            self.parser.succeed_rule(rule_id, Some(()));

            let Some(condition) = parse_expression_text(condition_text.trim()) else {
                return None;
            };
            conditions.push(condition);
        }

        let mut iter = conditions.into_iter();
        let first = iter.next()?;
        Some(iter.fold(first, |left, right| crate::parsed_hierarchy::ParsedExpression::Binary {
            left: Box::new(left),
            operator: "And".to_owned(),
            right: Box::new(right),
        }))
    }

    fn parse_inline_content_until(&mut self, terminators: &str) -> Vec<ParsedNode> {
        let was_parsing_choice = self.parsing_choice;
        self.parsing_choice = true;
        let mut nodes = Vec::new();

        loop {
            if self
                .parser
                .peek(|parser| {
                    parser
                        .current_character()
                        .filter(|ch| terminators.contains(*ch))
                        .map(|_| ())
                })
                .is_some()
            {
                break;
            }

            if self.parser.peek(|parser| parser.parse_string("{")).is_some() {
                let mut expression_nodes = self.parse_braced_inline_content(terminators)
                    .unwrap_or_default();
                nodes.append(&mut expression_nodes);
                continue;
            }

            if self.parser.parse_string("<>").is_some() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Glue));
                continue;
            }

            let Some(text) = self.content_text_allowing_escape_char() else {
                break;
            };
            if !text.is_empty() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(text));
            }
        }

        self.parsing_choice = was_parsing_choice;
        nodes
    }

    fn parse_braced_inline_content(&mut self, terminators: &str) -> Option<Vec<ParsedNode>> {
        let rule_id = self.parser.begin_rule();
        self.parser.parse_string("{")?;
        let content = self.parse_balanced_brace_body()?;
        self.parser.succeed_rule(rule_id, Some(()));

        let trimmed = content.trim();

        // ── Sequence: contains | separators ──────────────────────────────────
        // A sequence looks like {a|b|c} or {stopping: a|b|c} or {cycle: a|b}
        // Detect a sequence type annotation first, then split on | (not nested).
        if trimmed.contains('|') {
            // Check for optional type annotation prefix "word:" or "symbol"
            let (seq_type_flags, elements_str) =
                parse_sequence_type_and_elements(trimmed);
            let elements: Vec<Vec<ParsedNode>> = elements_str
                .iter()
                .map(|s| parse_inline_content_string(s, terminators))
                .collect();
            let mut seq_node = ParsedNode::new(ParsedNodeKind::Sequence);
            seq_node.sequence_type = seq_type_flags;
            // Store each element as a child container node (kind=Text acts as
            // a plain wrapper; we reuse children Vec<ParsedNode> of a
            // "container" node via a Sequence element node).
            let element_children: Vec<ParsedNode> = elements
                .into_iter()
                .map(|nodes| {
                    ParsedNode::new(ParsedNodeKind::Text)
                        .with_text("")
                        .with_children(nodes)
                })
                .collect();
            seq_node = seq_node.with_children(element_children);
            return Some(vec![seq_node]);
        }

        // ── Conditional inline: {cond: branch} ───────────────────────────────
        if let Some((condition_text, branch_text)) = trimmed.split_once(':') {
            // Only treat as conditional if condition_text looks like an expression
            // (not a sequence type keyword followed by no '|').
            if let Some(condition) = parse_expression_text(condition_text.trim()) {
                let branch_text = branch_text.trim_start();
                if let Some(target) = branch_text.strip_prefix("->") {
                    return Some(vec![ParsedNode::new(ParsedNodeKind::Divert)
                        .with_target(target.trim())
                        .with_condition(condition)]);
                }
                return match condition {
                    crate::parsed_hierarchy::ParsedExpression::Bool(true) => {
                        Some(parse_inline_content_string(branch_text, terminators))
                    }
                    crate::parsed_hierarchy::ParsedExpression::Bool(false) => Some(Vec::new()),
                    _ => Some(vec![build_inline_conditional_node(condition, branch_text, terminators)]),
                };
            }
        }

        // ── Plain expression: {expr} ──────────────────────────────────────────
        if let Some(expr) = parse_expression_text(trimmed) {
            return Some(vec![ParsedNode::new(ParsedNodeKind::OutputExpression)
                .with_expression(expr)]);
        }

        // Unknown content — return empty so the outer loop continues past it.
        Some(Vec::new())
    }

    /// Try to parse a gather line: `-` (not `->`) optionally followed by content.
    ///
    /// Returns a `ParsedNode::GatherPoint` with:
    /// - `indentation_depth`: number of `-` markers
    /// - `name`: optional label `(identifier)` after the markers
    /// - `children`: optional content nodes on the same line
    /// Try to parse a multiline sequence block:
    ///
    ///   { [type:] \n
    ///     - element1 content \n
    ///     - element2 content \n
    ///   }
    ///
    /// Returns a `Sequence` ParsedNode.
    fn try_parse_multiline_sequence(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        let result = (|| {
            self.parser.parse_string("{")?;
            let content = self.parse_balanced_brace_body()?;
            self.end_of_line();
            parse_multiline_sequence_block(&content)
        })();

        match result {
            Some(parsed) => self.parser.succeed_rule(rule_id, Some(parsed)),
            None => self.parser.fail_rule(rule_id),
        }
    }

    fn try_parse_multiline_conditional(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        let result = (|| {
            self.parser.parse_string("{")?;
            let content = self.parse_balanced_brace_body()?;
            self.end_of_line();

            if !content.contains('\n') {
                return None;
            }

            let first_nonempty = content
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("")
                .trim();
            if let Some((header, _)) = first_nonempty.split_once(':') {
                let header = header.trim();
                if matches!(header, "cycle" | "once" | "shuffle" | "stopping")
                    || header
                        .split_whitespace()
                        .all(|word| matches!(word, "cycle" | "once" | "shuffle" | "stopping"))
                {
                    return None;
                }
            }

            parse_multiline_conditional_block(&content)
        })();

        match result {
            Some(parsed) => self.parser.succeed_rule(rule_id, Some(parsed)),
            None => self.parser.fail_rule(rule_id),
        }
    }

    fn parse_balanced_brace_body(&mut self) -> Option<String> {
        let mut depth = 1usize;
        let mut string_open = false;
        let mut content = String::new();

        while let Some(ch) = self.parser.parse_single_character() {
            match ch {
                '"' => {
                    string_open = !string_open;
                    content.push(ch);
                }
                '{' if !string_open => {
                    depth += 1;
                    content.push(ch);
                }
                '}' if !string_open => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(content);
                    }
                    content.push(ch);
                }
                _ => content.push(ch),
            }
        }

        None
    }

    /// Parse a sequence type annotation (word or symbol form) and return the flags.
    /// Leaves the parser position after the annotation (but before `:` if word form).
    fn sequence_type_annotation_flags(&mut self) -> u8 {
        // Try word form first: one or more words like "stopping", "shuffle once"
        if let Some(flags) = self.parser.peek(|p| {
            let mut combined: u8 = 0;
            let word_map = [
                ("stopping", SequenceType::Stopping as u8),
                ("cycle",    SequenceType::Cycle    as u8),
                ("shuffle",  SequenceType::Shuffle  as u8),
                ("once",     SequenceType::Once     as u8),
            ];
            loop {
                p.parse_characters_from_string(" \t", true, -1);
                let mut matched = false;
                for (kw, flag) in &word_map {
                    if p.parse_string(kw).is_some() {
                        combined |= flag;
                        matched = true;
                        break;
                    }
                }
                if !matched { break; }
            }
            if combined != 0 { Some(combined) } else { None }
        }) {
            // Consume the word(s)
            let mut combined: u8 = 0;
            let word_map = [
                ("stopping", SequenceType::Stopping as u8),
                ("cycle",    SequenceType::Cycle    as u8),
                ("shuffle",  SequenceType::Shuffle  as u8),
                ("once",     SequenceType::Once     as u8),
            ];
            loop {
                self.parser.parse_characters_from_string(" \t", true, -1);
                let mut matched = false;
                for (kw, flag) in &word_map {
                    if self.parser.parse_string(kw).is_some() {
                        combined |= flag;
                        matched = true;
                        break;
                    }
                }
                if !matched { break; }
            }
            let _ = flags; // suppress unused warning
            return combined;
        }

        // Try symbol form
        let symbol_map = [
            ('!', SequenceType::Once     as u8),
            ('&', SequenceType::Cycle    as u8),
            ('~', SequenceType::Shuffle  as u8),
            ('$', SequenceType::Stopping as u8),
        ];
        let mut combined: u8 = 0;
        loop {
            let ch = match self.parser.peek(|p| p.parse_single_character()) {
                Some(c) => c,
                None => break,
            };
            let mut found = false;
            for (sym, flag) in &symbol_map {
                if ch == *sym {
                    self.parser.parse_single_character();
                    combined |= flag;
                    found = true;
                    break;
                }
            }
            if !found { break; }
        }
        combined
    }

    fn try_parse_gather(&mut self) -> Option<ParsedNode> {        let rule_id = self.parser.begin_rule();
        let marker = self.gather_marker();
        let Some(marker) = marker else {
            return self.parser.fail_rule(rule_id);
        };
        let depth = marker.indentation_depth;
        let label = marker.identifier;

        // Optional horizontal whitespace
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        // Optional content on the same line (text + optional divert)
        let mut content_nodes = Vec::new();
        let mut line_nodes: Vec<ParsedNode> = Vec::new();
        if let Some(text) = self.content_text() {
            let trimmed = text.trim_end_matches([' ', '\t']).to_owned();
            if !trimmed.is_empty() {
                line_nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(trimmed));
            }
        }
        if let Some(divert) = self.try_parse_inline_divert() {
            if !line_nodes.is_empty() {
                Self::append_trailing_space(&mut line_nodes);
            }
            line_nodes.push(divert);
        }
        if !line_nodes.is_empty() {
            line_nodes.push(ParsedNode::new(ParsedNodeKind::Newline));
            content_nodes.append(&mut line_nodes);
        }
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        let mut node = ParsedNode::new(if label.is_some() {
            ParsedNodeKind::GatherLabel
        } else {
            ParsedNodeKind::GatherPoint
        });
        node.indentation_depth = depth;
        if let Some(label_name) = label {
            node = node.with_name(label_name);
        }
        if !content_nodes.is_empty() {
            node = node.with_children(content_nodes);
        }
        Some(node)
    }

    /// Try to parse a parenthesised inline label: `(identifier)`.
    fn try_parse_inline_label(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let Some(ident) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if self.parser.parse_string(")").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.parser.succeed_rule(rule_id, Some(ident))
    }

    /// Skip to the end of the current line (fallback for unrecognised content).
    fn skip_line(&mut self) {
        let _ = self
            .parser
            .parse_until_characters_from_string("\n\r", -1);
        let _ = self.parser.parse_newline();
    }
}

fn trim_end_whitespace_nodes(mut nodes: Vec<ParsedNode>) -> Vec<ParsedNode> {
    if let Some(last) = nodes.last_mut()
        && last.kind() == ParsedNodeKind::Text
    {
        let trimmed = last
            .text()
            .unwrap_or("")
            .trim_end_matches([' ', '\t'])
            .to_owned();
        if trimmed.is_empty() {
            nodes.pop();
        } else {
            *last = ParsedNode::new(ParsedNodeKind::Text).with_text(trimmed);
        }
    }
    nodes
}

fn should_close_quoted_choice(
    start_nodes: &[ParsedNode],
    choice_only_nodes: &[ParsedNode],
    inner_nodes: &[ParsedNode],
) -> bool {
    if choice_only_nodes.is_empty() {
        return false;
    }

    let starts_with_quote = start_nodes
        .first()
        .and_then(|node| node.text())
        .is_some_and(|text| text.starts_with('\''));
    let inner_starts_with_comma_quote = inner_nodes
        .first()
        .and_then(|node| node.text())
        .is_some_and(|text| text.starts_with(",'"));

    starts_with_quote && inner_starts_with_comma_quote
}

fn append_text_suffix(nodes: &mut [ParsedNode], suffix: &str) {
    if let Some(last) = nodes.last_mut()
        && last.kind() == ParsedNodeKind::Text
    {
        let mut text = last.text().unwrap_or("").to_owned();
        text.push_str(suffix);
        *last = ParsedNode::new(ParsedNodeKind::Text).with_text(text);
    }
}

fn parse_inline_content_string(input: &str, terminators: &str) -> Vec<ParsedNode> {
    let mut parser = InkParser::new(input, None);
    parser.parsing_choice = terminators.contains('[') || terminators.contains(']');
    parser.parse_inline_content_until(terminators)
}

/// Parse an optional sequence type annotation and return (flags, elements).
///
/// Input is the trimmed interior of `{...}`.  The type annotation may be:
/// - a word keyword followed by `:`, e.g. `stopping: a|b|c`
/// - a symbol prefix, e.g. `!a|b|c` (once), `&a|b|c` (cycle),
///   `~a|b|c` (shuffle), `$a|b|c` (stopping)
///
/// Default (no annotation) is Stopping (flags = 1).
fn parse_sequence_type_and_elements(input: &str) -> (u8, Vec<String>) {
    use crate::parsed_hierarchy::SequenceType;

    // Word annotation: "word: rest"
    let word_keywords = [
        ("stopping", SequenceType::Stopping as u8),
        ("cycle",    SequenceType::Cycle    as u8),
        ("shuffle",  SequenceType::Shuffle  as u8),
        ("once",     SequenceType::Once     as u8),
    ];
    for (kw, flags) in &word_keywords {
        if let Some(rest) = input.strip_prefix(kw) {
            if let Some(rest2) = rest.trim_start().strip_prefix(':') {
                return (*flags, split_sequence_elements(rest2));
            }
        }
    }

    // Symbol annotation prefix
    let symbol_map = [
        ('!', SequenceType::Once     as u8),
        ('&', SequenceType::Cycle    as u8),
        ('~', SequenceType::Shuffle  as u8),
        ('$', SequenceType::Stopping as u8),
    ];
    if let Some(ch) = input.chars().next() {
        for (sym, flags) in &symbol_map {
            if ch == *sym {
                return (*flags, split_sequence_elements(&input[ch.len_utf8()..]));
            }
        }
    }

    // Default: Stopping
    (SequenceType::Stopping as u8, split_sequence_elements(input))
}

/// Split sequence content on top-level `|` characters (not nested in `{}`).
fn split_sequence_elements(input: &str) -> Vec<String> {
    let mut elements = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in input.chars() {
        match ch {
            '{' => { depth += 1; current.push(ch); }
            '}' => { depth = depth.saturating_sub(1); current.push(ch); }
            '|' if depth == 0 => {
                elements.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    elements.push(current);
    elements
}

fn parse_expression_text(input: &str) -> Option<crate::parsed_hierarchy::ParsedExpression> {
    use crate::parsed_hierarchy::ParsedExpression;

    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    if let Some(inner) = strip_wrapping_parentheses(input) {
        return parse_expression_text(inner);
    }

    if let Some(target) = input.strip_prefix("->") {
        return Some(ParsedExpression::DivertTarget(target.trim().to_owned()));
    }

    for (token, operator) in [
        ("&&", "And"),
        ("||", "Or"),
        (" and ", "And"),
        (" or ", "Or"),
        (">=", "GreaterEqual"),
        ("<=", "LessEqual"),
        ("==", "Equal"),
        ("!=", "NotEqual"),
        (">", "Greater"),
        ("<", "Less"),
        ("+", "Add"),
        ("-", "Subtract"),
        (" mod ", "Modulo"),
        ("*", "Multiply"),
        ("/", "Divide"),
        ("%", "Modulo"),
    ] {
        if let Some((left, right)) = split_expression_once(input, token) {
            return Some(ParsedExpression::Binary {
                left: Box::new(parse_expression_text(left)?),
                operator: operator.to_owned(),
                right: Box::new(parse_expression_text(right)?),
            });
        }
    }

    if let Some(rest) = input.strip_prefix("not ") {
        return Some(ParsedExpression::Unary {
            operator: "!".to_owned(),
            expression: Box::new(parse_expression_text(rest)?),
        });
    }

    if let Some(rest) = input.strip_prefix('!') {
        return Some(ParsedExpression::Unary {
            operator: "!".to_owned(),
            expression: Box::new(parse_expression_text(rest)?),
        });
    }

    if input.starts_with('"') && input.ends_with('"') && input.len() >= 2 {
        return Some(ParsedExpression::String(input[1..input.len() - 1].to_owned()));
    }

    if input == "true" {
        return Some(ParsedExpression::Bool(true));
    }
    if input == "false" {
        return Some(ParsedExpression::Bool(false));
    }

    if let Ok(value) = input.parse::<i32>() {
        return Some(ParsedExpression::Int(value));
    }
    if let Ok(value) = input.parse::<f32>() {
        return Some(ParsedExpression::Float(value));
    }

    if let Some(name) = input.strip_suffix(')')
        && let Some((name, args)) = name.split_once('(')
    {
        let arguments = split_arguments(args)
            .into_iter()
            .map(|arg| parse_expression_text(&arg))
            .collect::<Option<Vec<_>>>()?;
        return Some(ParsedExpression::FunctionCall {
            name: name.trim().to_owned(),
            arguments,
        });
    }

    Some(ParsedExpression::Variable(input.to_owned()))
}

fn split_expression_once<'a>(input: &'a str, token: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0usize;
    let mut string_open = false;
    let chars: Vec<(usize, char)> = input.char_indices().collect();

    let mut i = 0usize;
    while i < chars.len() {
        let (byte_index, ch) = chars[i];
        match ch {
            '"' => string_open = !string_open,
            '(' | '{' | '[' if !string_open => depth += 1,
            ')' | '}' | ']' if !string_open && depth > 0 => depth -= 1,
            _ => {}
        }

        if !string_open && depth == 0 && input[byte_index..].starts_with(token) {
            if (token == "+" || token == "-")
                && (byte_index == 0
                    || matches!(input[..byte_index].chars().last(), Some('(' | '[' | '{' | '+' | '-' | '*' | '/' | '%' | '<' | '>' | '=' | '!')))
            {
                i += 1;
                continue;
            }
            return Some((&input[..byte_index], &input[byte_index + token.len()..]));
        }

        i += 1;
    }

    None
}

fn strip_wrapping_parentheses(input: &str) -> Option<&str> {
    if !input.starts_with('(') || !input.ends_with(')') {
        return None;
    }

    let mut depth = 0usize;
    let mut string_open = false;
    for (idx, ch) in input.char_indices() {
        match ch {
            '"' => string_open = !string_open,
            '(' if !string_open => depth += 1,
            ')' if !string_open => {
                depth = depth.saturating_sub(1);
                if depth == 0 && idx != input.len() - 1 {
                    return None;
                }
            }
            _ => {}
        }
    }

    Some(&input[1..input.len() - 1])
}

fn split_arguments(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut string_open = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                string_open = !string_open;
                current.push(ch);
            }
            '(' | '{' | '[' if !string_open => {
                depth += 1;
                current.push(ch);
            }
            ')' | '}' | ']' if !string_open && depth > 0 => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !string_open && depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    args.push(trimmed.to_owned());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        args.push(trimmed.to_owned());
    }

    args
}

fn parse_multiline_conditional_block(content: &str) -> Option<ParsedNode> {
    let lines: Vec<&str> = content.lines().collect();
    let first_idx = lines.iter().position(|line| !line.trim().is_empty())?;
    let first = lines[first_idx].trim();

    let mut initial_condition = None;
    let branch_specs = if first.starts_with('-') {
        parse_conditional_branches(&lines[first_idx..], false)?
    } else {
        let (header, inline_after) = first.split_once(':')?;
        initial_condition = parse_expression_text(header.trim());
        let mut body_lines: Vec<String> = Vec::new();
        if !inline_after.trim().is_empty() {
            body_lines.push(inline_after.trim_start().to_owned());
        }
        body_lines.extend(lines[(first_idx + 1)..].iter().map(|line| (*line).to_owned()));

        let has_branch_markers = has_top_level_conditional_branches(
            &body_lines.iter().map(|line| line.as_str()).collect::<Vec<_>>(),
        );
        if has_branch_markers {
            parse_conditional_branches(
                &body_lines.iter().map(|line| line.as_str()).collect::<Vec<_>>(),
                true,
            )?
        } else {
            vec![ConditionalBranchSpec {
                condition: None,
                content: body_lines.join("\n"),
                is_else: false,
            }]
        }
    };

    let mut saw_branch_condition = false;
    let mut children = Vec::new();
    for (idx, branch) in branch_specs.into_iter().enumerate() {
        let mut branch_node = ParsedNode::new(ParsedNodeKind::Conditional);
        branch_node.is_inline = false;
        branch_node.is_else = branch.is_else;
        if let Some(condition) = branch.condition.clone() {
            branch_node = branch_node.with_condition(condition);
        }
        branch_node.set_children(parse_nested_statement_block(&branch.content));

        if initial_condition.is_some() {
            if branch.condition.is_some() {
                branch_node.matching_equality = true;
                saw_branch_condition = true;
            } else if saw_branch_condition && branch.is_else {
                branch_node.matching_equality = true;
            } else if idx == 0 {
                branch_node.is_true_branch = true;
            } else {
                branch_node.is_else = true;
            }
        } else if branch.is_else {
            branch_node.is_else = true;
        }

        children.push(branch_node);
    }

    let mut node = ParsedNode::new(if initial_condition.is_some() {
        ParsedNodeKind::SwitchConditional
    } else {
        ParsedNodeKind::Conditional
    });
    if let Some(condition) = initial_condition {
        node = node.with_condition(condition);
    }
    node.set_children(children);
    Some(node)
}

fn parse_multiline_sequence_block(content: &str) -> Option<ParsedNode> {
    let mut lines = content.lines();
    let header = lines.find(|line| !line.trim().is_empty())?.trim();
    let (seq_type_text, _rest) = header.split_once(':')?;
    let seq_type_flags = parse_sequence_flags_text(seq_type_text.trim())?;

    let remaining: Vec<&str> = content.lines().skip_while(|line| line.trim() != header).skip(1).collect();
    let mut elements = Vec::new();
    let mut current = String::new();
    let mut brace_depth = 0isize;

    for line in remaining {
        let trimmed = line.trim_start();
        let is_new_element = brace_depth == 0 && trimmed.starts_with('-') && !trimmed.starts_with("->");
        if is_new_element {
            if !current.is_empty() {
                elements.push(current.clone());
                current.clear();
            }
            current.push_str(trimmed[1..].trim_start());
            current.push('\n');
        } else {
            current.push_str(line);
            current.push('\n');
        }
        brace_depth += net_brace_delta(line);
    }
    if !current.is_empty() {
        elements.push(current);
    }
    if elements.is_empty() {
        return None;
    }

    let element_children: Vec<ParsedNode> = elements
        .into_iter()
        .map(|element| {
            let mut nodes = parse_nested_statement_block(&element);
            if !nodes.is_empty() {
                nodes.insert(0, ParsedNode::new(ParsedNodeKind::Newline));
            }
            ParsedNode::new(ParsedNodeKind::Text)
                .with_text("")
                .with_children(nodes)
        })
        .collect();

    let mut seq_node = ParsedNode::new(ParsedNodeKind::Sequence);
    seq_node.sequence_type = seq_type_flags;
    seq_node.set_children(element_children);
    Some(seq_node)
}

fn parse_sequence_flags_text(text: &str) -> Option<u8> {
    let mut flags = 0u8;
    for word in text.split_whitespace() {
        flags |= match word {
            "stopping" => SequenceType::Stopping as u8,
            "cycle" => SequenceType::Cycle as u8,
            "shuffle" => SequenceType::Shuffle as u8,
            "once" => SequenceType::Once as u8,
            _ => return None,
        };
    }
    (flags != 0).then_some(flags)
}

#[derive(Clone)]
struct ConditionalBranchSpec {
    condition: Option<crate::parsed_hierarchy::ParsedExpression>,
    content: String,
    is_else: bool,
}

fn parse_conditional_branches(lines: &[&str], allow_bare_else: bool) -> Option<Vec<ConditionalBranchSpec>> {
    let mut branches = Vec::new();
    let mut current_header: Option<(Option<crate::parsed_hierarchy::ParsedExpression>, bool)> = None;
    let mut current_content = String::new();
    let mut brace_depth = 0isize;

    for line in lines {
        let trimmed = line.trim_start();
        let is_top_level_branch = brace_depth == 0 && trimmed.starts_with('-');
        if is_top_level_branch {
            if let Some((condition, is_else)) = current_header.take() {
                branches.push(ConditionalBranchSpec {
                    condition,
                    content: current_content.clone(),
                    is_else,
                });
                current_content.clear();
            }

            let header = trimmed[1..].trim_start();
            if let Some(rest) = header.strip_prefix("else:") {
                current_header = Some((None, true));
                if !rest.trim().is_empty() {
                    current_content.push_str(rest.trim_start());
                    current_content.push('\n');
                }
                continue;
            }

            let (condition_text, rest) = header.split_once(':')?;
            current_header = Some((parse_expression_text(condition_text.trim()), false));
            if !rest.trim().is_empty() {
                current_content.push_str(rest.trim_start());
                current_content.push('\n');
            }
        } else {
            if current_header.is_none() {
                if !allow_bare_else {
                    return None;
                }
                current_header = Some((None, false));
            }
            current_content.push_str(line);
            current_content.push('\n');
        }

        brace_depth += net_brace_delta(line);
    }

    if let Some((condition, is_else)) = current_header.take() {
        branches.push(ConditionalBranchSpec {
            condition,
            content: current_content,
            is_else,
        });
    }

    (!branches.is_empty()).then_some(branches)
}

fn has_top_level_conditional_branches(lines: &[&str]) -> bool {
    let mut brace_depth = 0isize;
    for line in lines {
        if brace_depth == 0 && line.trim_start().starts_with('-') {
            return true;
        }
        brace_depth += net_brace_delta(line);
    }
    false
}

fn net_brace_delta(line: &str) -> isize {
    let mut delta = 0isize;
    let mut string_open = false;
    for ch in line.chars() {
        match ch {
            '"' => string_open = !string_open,
            '{' if !string_open => delta += 1,
            '}' if !string_open => delta -= 1,
            _ => {}
        }
    }
    delta
}

fn parse_nested_statement_block(content: &str) -> Vec<ParsedNode> {
    let mut parser = InkParser::new(content, None);
    let parsed = parser.parse_at_level(InkParseLevel::Stitch);
    parsed.nodes
}

fn build_inline_conditional_node(
    condition: crate::parsed_hierarchy::ParsedExpression,
    branch_text: &str,
    terminators: &str,
) -> ParsedNode {
    let mut branch = ParsedNode::new(ParsedNodeKind::Conditional)
        .with_children(parse_inline_content_string(branch_text, terminators));
    branch.is_true_branch = true;
    branch.is_inline = true;

    let mut conditional = ParsedNode::new(ParsedNodeKind::Conditional).with_condition(condition);
    conditional.set_children(vec![branch]);
    conditional
}

fn merge_story(into: &mut Story, mut other: Story) {
    into.global_declarations.append(&mut other.global_declarations);
    into.global_initializers.append(&mut other.global_initializers);
    into.list_definitions.append(&mut other.list_definitions);
    into.external_declarations.append(&mut other.external_declarations);
    into.const_declarations.append(&mut other.const_declarations);
    into.root_nodes.append(&mut other.root_nodes);
    into.flows.append(&mut other.flows);
}

#[cfg(test)]
mod tests {
    use super::InkParser;
    use crate::{
        parsed_hierarchy::{Content, NumberValue},
        string_parser::{ParseSuccess, StringParserStateElement},
    };

    #[test]
    fn ink_parser_preprocesses_comments_and_line_endings() {
        let parser = InkParser::new("A\r\n/*x*/\r\nB // tail\r\n", Some("test.ink".to_owned()));
        assert_eq!("A\n\nB \n", parser.parser().input_string());
        assert_eq!(Some("test.ink"), parser.source_name());
    }

    #[test]
    fn whitespace_helpers_match_reference_shape() {
        let mut parser = InkParser::new(" \t\n\nx", None);
        assert_eq!(Some(ParseSuccess), parser.whitespace());
        assert_eq!(Some(ParseSuccess), parser.multiline_whitespace());
        assert_eq!(Some('x'), parser.parser().current_character());
    }

    #[test]
    fn create_debug_metadata_is_one_based() {
        let parser = InkParser::new("abc", Some("a.ink".to_owned()));
        let start = StringParserStateElement::default();
        let end = StringParserStateElement::default();
        let metadata = parser.create_debug_metadata(&start, &end);
        assert_eq!(1, metadata.start_line_number);
        assert_eq!(1, metadata.start_character_number);
        assert_eq!(Some("a.ink".to_owned()), metadata.file_name);
    }

    #[test]
    fn identifier_supports_ascii_and_unicode_ranges() {
        let mut parser = InkParser::new("héllo_世界", None);
        assert_eq!(Some("héllo_世界".to_owned()), parser.parse_identifier());
    }

    #[test]
    fn content_text_honours_escape_and_stops_before_divert() {
        let mut parser = InkParser::new(r#"hello \-> world -> target"#, None);
        assert_eq!(Some("hello -> world ".to_owned()), parser.content_text());
        assert_eq!(
            Some("->".to_owned()),
            parser.parser_mut().parse_string("->")
        );
    }

    #[test]
    fn line_of_mixed_text_trims_inline_whitespace_and_appends_newline() {
        let mut parser = InkParser::new("hello \t\n", None);
        let list = parser.line_of_mixed_text().expect("expected text line");
        let pieces: Vec<&str> = list
            .content()
            .iter()
            .map(|item| match item {
                Content::Text(text) => text.text(),
            })
            .collect();
        assert_eq!(vec!["hello", "\n"], pieces);
    }

    #[test]
    fn bracketed_name_parses_choice_and_gather_labels() {
        let mut parser = InkParser::new("(route_name)", None);
        assert_eq!(Some("route_name".to_owned()), parser.bracketed_name());
    }

    #[test]
    fn choice_marker_supports_sticky_and_nested_depth() {
        let mut parser = InkParser::new(" + + option", None);
        assert_eq!(
            Some(super::ChoiceMarker {
                indentation_depth: 2,
                once_only: false
            }),
            parser.choice_marker()
        );
    }

    #[test]
    fn gather_marker_stops_before_arrow_and_reads_optional_name() {
        let mut parser = InkParser::new(" - (join) continue", None);
        assert_eq!(
            Some(super::GatherMarker {
                indentation_depth: 1,
                identifier: Some("join".to_owned())
            }),
            parser.gather_marker()
        );

        let mut divert = InkParser::new("-> target", None);
        assert_eq!(None, divert.gather_marker());
    }

    #[test]
    fn split_choice_clause_supports_weave_style_inline_brackets() {
        let clause = InkParser::split_choice_clause("\"I am tired[.]\" he said");
        assert_eq!("\"I am tired", clause.start_text);
        assert_eq!(Some(".".to_owned()), clause.choice_only_text);
        assert_eq!("\" he said", clause.inner_text);
    }

    #[test]
    fn number_and_bool_literals_match_reference_shapes() {
        let mut ints = InkParser::new("12", None);
        assert!(matches!(
            ints.parse_number_literal().map(|n| n.value().clone()),
            Some(NumberValue::Int(12))
        ));

        let mut floats = InkParser::new("12.5", None);
        assert!(matches!(
            floats.parse_number_literal().map(|n| n.value().clone()),
            Some(NumberValue::Float(value)) if (value - 12.5).abs() < f32::EPSILON
        ));

        let mut bools = InkParser::new("true", None);
        assert!(matches!(
            bools.parse_bool_literal().map(|n| n.value().clone()),
            Some(NumberValue::Bool(true))
        ));
    }

    #[test]
    fn sequence_type_annotations_support_symbol_and_word_forms() {
        let mut symbols = InkParser::new("!~", None);
        assert_eq!(
            Some((super::SequenceType::Once as u8) | (super::SequenceType::Shuffle as u8)),
            symbols.sequence_type_symbol_annotation().map(|a| a.flags)
        );

        let mut words = InkParser::new("once shuffle:", None);
        assert_eq!(
            Some((super::SequenceType::Once as u8) | (super::SequenceType::Shuffle as u8)),
            words.sequence_type_word_annotation().map(|a| a.flags)
        );
    }

    #[test]
    fn start_and_end_tag_track_active_state() {
        let mut parser = InkParser::new("# tag", None);
        let start = parser.start_tag().expect("start tag");
        assert!(start.is_start());
        let end = parser.end_tag_if_necessary().expect("end tag");
        assert!(!end.is_start());
        assert!(parser.end_tag_if_necessary().is_none());
    }

    #[test]
    fn parses_divert_shapes_and_thread_starts() {
        let mut parser = InkParser::new("knot.stitch(arg, 2)", None);
        let divert = parser
            .divert_identifier_with_arguments()
            .expect("divert with args");
        assert_eq!(vec!["knot".to_owned(), "stitch".to_owned()], divert.target);
        assert_eq!(vec!["arg".to_owned(), "2".to_owned()], divert.arguments);

        let mut thread = InkParser::new("<- worker()", None);
        let divert = thread.start_thread().expect("thread");
        assert!(divert.is_thread);
    }

    #[test]
    fn parses_flow_decl_arguments_and_headers() {
        let mut arg = InkParser::new("ref -> target", None);
        let parsed = arg.flow_decl_argument().expect("arg");
        assert!(parsed.is_by_reference);
        assert!(parsed.is_divert_target);
        assert_eq!("target", parsed.identifier);

        let mut knot = InkParser::new("=== function my_knot(ref x, -> target) ===", None);
        let parsed = knot.knot_declaration().expect("knot");
        assert!(parsed.is_function);
        assert_eq!("my_knot", parsed.name);
        assert_eq!(2, parsed.arguments.len());

        let mut stitch = InkParser::new("= place(ref x)", None);
        let parsed = stitch.stitch_declaration().expect("stitch");
        assert_eq!("place", parsed.name);
        assert_eq!(1, parsed.arguments.len());
    }

    #[test]
    fn parses_external_include_and_return_headers() {
        let mut external = InkParser::new("EXTERNAL my_func(x, y)", None);
        let declaration = external.external_declaration().expect("external");
        assert_eq!("my_func", declaration.name());
        assert_eq!(
            &["x".to_owned(), "y".to_owned()],
            declaration.argument_names()
        );

        let mut include = InkParser::new("INCLUDE path/to/file.ink\n", None);
        assert_eq!(
            Some("path/to/file.ink".to_owned()),
            include.include_statement_filename()
        );

        let mut ret = InkParser::new("return\n", None);
        assert!(ret.return_statement().is_some());
    }

    #[test]
    fn parses_list_definitions_and_literals() {
        let mut definition = InkParser::new("a, (b = 5), c", None);
        let list_definition = definition.list_definition().expect("list definition");
        let items = list_definition.item_definitions();
        assert_eq!("a", items[0].name());
        assert_eq!(1, items[0].series_value());
        assert_eq!("b", items[1].name());
        assert!(items[1].in_initial_list());
        assert_eq!(Some(5), items[1].explicit_value());
        assert_eq!(6, items[2].series_value());

        let mut literal = InkParser::new("(alpha, things.beta)", None);
        let list = literal.list_expression().expect("list literal");
        assert_eq!(
            Some(&["alpha".to_owned(), "things.beta".to_owned()][..]),
            list.item_identifier_list()
        );

        let mut empty = InkParser::new("()", None);
        assert!(empty.list_expression().expect("empty list").is_empty());
    }

    #[test]
    fn parses_command_line_input_shapes() {
        let mut help = InkParser::new("help", None);
        assert!(help.command_line_user_input().expect("help").is_help);

        let mut choice = InkParser::new("3\n", None);
        assert_eq!(
            Some(3),
            choice
                .command_line_user_input()
                .expect("choice")
                .choice_input
        );

        let mut debug_source = InkParser::new("DebugSource(12)", None);
        assert_eq!(
            Some(12),
            debug_source
                .command_line_user_input()
                .expect("debug source")
                .debug_source
        );

        let mut debug_path = InkParser::new("DebugPath knot.0.c-0", None);
        assert_eq!(
            Some("knot.0.c-0".to_owned()),
            debug_path
                .command_line_user_input()
                .expect("debug path")
                .debug_path_lookup
        );
    }
}
