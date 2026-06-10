impl ValidationContext {
    // -----------------------------------------------------------------------
    // Stitch name collision with vars
    // -----------------------------------------------------------------------

    fn validate_stitch_name(&self, stitch: &Flow) -> Result<(), CompilerError> {
        if self.global_var_names.contains(&stitch.name) {
            return Err(CompilerError::invalid_source(format!(
                "The name '{}' has already been used for a var declaration.",
                stitch.name
            )));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Duplicate gather labels
    // -----------------------------------------------------------------------

    fn validate_no_duplicate_gather_labels(&self, nodes: &[Node]) -> Result<(), CompilerError> {
        let mut seen: BTreeMap<String, ()> = BTreeMap::new();
        self.collect_and_check_gather_labels(nodes, &mut seen)
    }

    fn collect_and_check_gather_labels(
        &self,
        nodes: &[Node],
        seen: &mut BTreeMap<String, ()>,
    ) -> Result<(), CompilerError> {
        for node in nodes {
            match node {
                Node::GatherLabel { label, .. }
                    if seen.insert(label.clone(), ()).is_some() =>
                {
                    return Err(CompilerError::invalid_source(format!(
                        "A gather point with the same label '{label}' already exists in this scope."
                    )));
                }
                Node::Choice(c) => {
                    self.collect_and_check_gather_labels(&c.body, seen)?;
                }
                Node::Conditional {
                    when_true,
                    when_false,
                    ..
                } => {
                    self.collect_and_check_gather_labels(when_true, seen)?;
                    if let Some(wf) = when_false {
                        self.collect_and_check_gather_labels(wf, seen)?;
                    }
                }
                Node::SwitchConditional { branches, .. } => {
                    for (_, body) in branches {
                        self.collect_and_check_gather_labels(body, seen)?;
                    }
                }
                Node::Sequence(seq) => {
                    for branch in &seq.branches {
                        self.collect_and_check_gather_labels(branch, seen)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Choice directly inside conditional (without a weave gather point)
    // -----------------------------------------------------------------------

    fn validate_no_choice_in_conditional(&self, nodes: &[Node]) -> Result<(), CompilerError> {
        for node in nodes {
            match node {
                Node::Conditional {
                    when_true,
                    when_false,
                    ..
                } => {
                    self.check_no_direct_choice_in_branch(when_true)?;
                    if let Some(wf) = when_false {
                        self.check_no_direct_choice_in_branch(wf)?;
                    }
                    // Recurse into the branches
                    self.validate_no_choice_in_conditional(when_true)?;
                    if let Some(wf) = when_false {
                        self.validate_no_choice_in_conditional(wf)?;
                    }
                }
                Node::SwitchConditional { branches, .. } => {
                    for (_, body) in branches {
                        self.check_no_direct_choice_in_branch(body)?;
                        self.validate_no_choice_in_conditional(body)?;
                    }
                }
                Node::Choice(c) => {
                    self.validate_no_choice_in_conditional(&c.body)?;
                }
                Node::Sequence(seq) => {
                    for branch in &seq.branches {
                        self.validate_no_choice_in_conditional(branch)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn check_no_direct_choice_in_branch(&self, nodes: &[Node]) -> Result<(), CompilerError> {
        for node in nodes {
            if let Node::Choice(c) = node {
                // A choice inside a conditional is allowed only if it explicitly diverts.
                // Without a divert there is no safe continuation out of the conditional.
                if !choice_has_explicit_divert(c) {
                    return Err(CompilerError::invalid_source(
                        "Choices with conditions need to explicitly divert afterwards.".to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Temp variable name collision with function names
    // -----------------------------------------------------------------------

    fn validate_temp_names(
        &self,
        nodes: &[Node],
        params: &BTreeSet<String>,
    ) -> Result<(), CompilerError> {
        self.check_temps_in_nodes(nodes, params)
    }

    fn check_temps_in_nodes(
        &self,
        nodes: &[Node],
        params: &BTreeSet<String>,
    ) -> Result<(), CompilerError> {
        for node in nodes {
            match node {
                Node::Assignment {
                    variable_name,
                    mode: AssignMode::TempSet,
                    ..
                } => {
                    if self.function_names.contains(variable_name.as_str()) {
                        return Err(CompilerError::invalid_source(format!(
                            "The name '{}' has already been used for a function.",
                            variable_name
                        )));
                    }
                    if params.contains(variable_name.as_str()) {
                        return Err(CompilerError::invalid_source(format!(
                            "The name '{}' has already been used as a parameter name.",
                            variable_name
                        )));
                    }
                }
                Node::Choice(c) => self.check_temps_in_nodes(&c.body, params)?,
                Node::Conditional {
                    when_true,
                    when_false,
                    ..
                } => {
                    self.check_temps_in_nodes(when_true, params)?;
                    if let Some(wf) = when_false {
                        self.check_temps_in_nodes(wf, params)?;
                    }
                }
                Node::SwitchConditional { branches, .. } => {
                    for (_, body) in branches {
                        self.check_temps_in_nodes(body, params)?;
                    }
                }
                Node::Sequence(seq) => {
                    for branch in &seq.branches {
                        self.check_temps_in_nodes(branch, params)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Function purity checks
    // -----------------------------------------------------------------------

    fn validate_function_purity(&self, flow: &Flow) -> Result<(), CompilerError> {
        if !flow.is_function {
            return Ok(());
        }
        // Functions may not contain stitches
        if !flow.children.is_empty() {
            return Err(CompilerError::invalid_source(format!(
                "Function '{}' may not contain stitches.",
                flow.name
            )));
        }
        // Functions may not contain choices or diverts
        self.check_function_body_purity(&flow.nodes, &flow.name)
    }

    fn check_function_body_purity(
        &self,
        nodes: &[Node],
        func_name: &str,
    ) -> Result<(), CompilerError> {
        for node in nodes {
            match node {
                Node::Choice(_) => {
                    return Err(CompilerError::invalid_source(format!(
                        "Function '{func_name}' may not contain choices."
                    )));
                }
                Node::Divert(d) if d.target != "END" && d.target != "DONE" => {
                    return Err(CompilerError::invalid_source(format!(
                        "Function '{func_name}' may not contain diverts (found '-> {}').",
                        d.target
                    )));
                }
                Node::Conditional {
                    when_true,
                    when_false,
                    ..
                } => {
                    self.check_function_body_purity(when_true, func_name)?;
                    if let Some(wf) = when_false {
                        self.check_function_body_purity(wf, func_name)?;
                    }
                }
                Node::SwitchConditional { branches, .. } => {
                    for (_, body) in branches {
                        self.check_function_body_purity(body, func_name)?;
                    }
                }
                Node::Sequence(seq) => {
                    for branch in &seq.branches {
                        self.check_function_body_purity(branch, func_name)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Function parameter name collision with existing knots/vars
    // -----------------------------------------------------------------------

    fn validate_flow_parameter_names(&self, flow: &Flow) -> Result<(), CompilerError> {
        for param in &flow.parameters {
            if self.function_names.contains(param) {
                return Err(CompilerError::invalid_source(format!(
                    "The name '{}' has already been used for a function.",
                    param
                )));
            }
            if self.global_var_names.contains(param) {
                return Err(CompilerError::invalid_source(format!(
                    "The name '{}' has already been used for a var declaration.",
                    param
                )));
            }
        }
        Ok(())
    }

}
