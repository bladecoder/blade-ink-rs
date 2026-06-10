impl ValidationContext {
    fn validate_nodes_diverts(&self, nodes: &[Node], _scope: &str) -> Result<(), CompilerError> {
        for node in nodes {
            self.validate_node_divert(node)?;
        }
        Ok(())
    }

    fn validate_node_divert(&self, node: &Node) -> Result<(), CompilerError> {
        match node {
            Node::Divert(d) => self.check_divert(d)?,
            Node::TunnelDivert { target, .. } => self.check_target(target)?,
            Node::ThreadDivert(d) => self.check_divert(d)?,
            Node::Choice(c) => {
                for n in &c.body {
                    self.validate_node_divert(n)?;
                }
            }
            Node::Conditional {
                when_true,
                when_false,
                ..
            } => {
                self.validate_nodes_diverts(when_true, "")?;
                if let Some(wf) = when_false {
                    self.validate_nodes_diverts(wf, "")?;
                }
            }
            Node::SwitchConditional { branches, .. } => {
                for (_, body) in branches {
                    self.validate_nodes_diverts(body, "")?;
                }
            }
            Node::Sequence(seq) => {
                for branch in &seq.branches {
                    self.validate_nodes_diverts(branch, "")?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn check_divert(&self, d: &Divert) -> Result<(), CompilerError> {
        self.check_target(&d.target)
    }

    fn check_target(&self, target: &str) -> Result<(), CompilerError> {
        // Variable diverts (VAR? targets) can't be checked statically
        if target.is_empty() || target == "->" {
            return Ok(());
        }
        // Targets starting with '$' are internal compiler-generated
        if target.starts_with('$') {
            return Ok(());
        }
        if self.function_names.contains(target) {
            return Err(CompilerError::invalid_source(format!(
                "Function '{target}' can only be called as a function, not diverted to"
            )));
        }
        if self.valid_targets.contains(target) || self.flow_names.contains(target) {
            return Ok(());
        }
        // An unqualified target like "shove" may match "knot.shove" in valid_targets
        let suffix = format!(".{target}");
        if self.valid_targets.iter().any(|t| t.ends_with(&suffix)) {
            return Ok(());
        }
        Err(CompilerError::invalid_source(format!(
            "Divert target not found: '-> {target}'"
        )))
    }

    fn validate_nodes_function_calls(&self, nodes: &[Node]) -> Result<(), CompilerError> {
        for node in nodes {
            self.validate_node_function_calls(node)?;
        }
        Ok(())
    }

    fn validate_node_function_calls(&self, node: &Node) -> Result<(), CompilerError> {
        match node {
            Node::OutputExpression(expr) | Node::ReturnExpr(expr) => {
                self.validate_expr_function_calls(expr)?
            }
            Node::Assignment { expression, .. } => self.validate_expr_function_calls(expression)?,
            Node::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                self.validate_condition_function_calls(condition)?;
                self.validate_nodes_function_calls(when_true)?;
                if let Some(wf) = when_false {
                    self.validate_nodes_function_calls(wf)?;
                }
            }
            Node::SwitchConditional { value, branches } => {
                self.validate_expr_function_calls(value)?;
                for (case, body) in branches {
                    if let Some(case) = case {
                        self.validate_expr_function_calls(case)?;
                    }
                    self.validate_nodes_function_calls(body)?;
                }
            }
            Node::Choice(choice) => {
                for condition in &choice.conditions {
                    self.validate_condition_function_calls(condition)?;
                }
                self.validate_nodes_function_calls(&choice.body)?;
            }
            Node::Sequence(sequence) => {
                for branch in &sequence.branches {
                    self.validate_nodes_function_calls(branch)?;
                }
            }
            Node::VoidCall { name, args } => {
                self.check_function_call_target(name)?;
                for arg in args {
                    self.validate_expr_function_calls(arg)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_condition_function_calls(
        &self,
        condition: &Condition,
    ) -> Result<(), CompilerError> {
        match condition {
            Condition::FunctionCall(name) => self.check_function_call_target(name),
            Condition::Expression(expr) => self.validate_expr_function_calls(expr),
            Condition::Bool(_) => Ok(()),
        }
    }

    fn validate_expr_function_calls(&self, expr: &Expression) -> Result<(), CompilerError> {
        match expr {
            Expression::FunctionCall { name, args } => {
                self.check_function_call_target(name)?;
                for arg in args {
                    self.validate_expr_function_calls(arg)?;
                }
            }
            Expression::Negate(expr) | Expression::Not(expr) => {
                self.validate_expr_function_calls(expr)?;
            }
            Expression::Binary { left, right, .. } => {
                self.validate_expr_function_calls(left)?;
                self.validate_expr_function_calls(right)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn check_function_call_target(&self, name: &str) -> Result<(), CompilerError> {
        if self.function_names.contains(name)
            || self.external_functions.contains(name)
            || is_builtin_function(name)
        {
            return Ok(());
        }

        if self.flow_names.contains(name) {
            return Err(CompilerError::invalid_source(format!(
                "'{name}' hasn't been marked as a function, but it's being called as one"
            )));
        }

        Ok(())
    }

    fn validate_nodes_variable_divert_targets(
        &self,
        nodes: &[Node],
        parameters: &BTreeSet<String>,
        divert_parameters: &BTreeSet<String>,
    ) -> Result<(), CompilerError> {
        for node in nodes {
            self.validate_node_variable_divert_target(node, parameters, divert_parameters)?;
        }
        Ok(())
    }

    fn validate_node_variable_divert_target(
        &self,
        node: &Node,
        parameters: &BTreeSet<String>,
        divert_parameters: &BTreeSet<String>,
    ) -> Result<(), CompilerError> {
        match node {
            Node::Divert(d) | Node::ThreadDivert(d) => {
                self.check_variable_divert_target(&d.target, parameters, divert_parameters)?;
                // Check args: if an arg is DivertTarget(name) and name is a divert_parameter,
                // that's wrong — it shouldn't be preceded by '->'
                for arg in &d.arguments {
                    self.check_divert_target_arg(arg, divert_parameters)?;
                }
            }
            Node::TunnelDivert { target, args, .. } => {
                self.check_variable_divert_target(target, parameters, divert_parameters)?;
                for arg in args {
                    self.check_divert_target_arg(arg, divert_parameters)?;
                }
            }
            Node::Choice(choice) => {
                self.validate_nodes_variable_divert_targets(
                    &choice.body,
                    parameters,
                    divert_parameters,
                )?;
            }
            Node::Conditional {
                when_true,
                when_false,
                ..
            } => {
                self.validate_nodes_variable_divert_targets(
                    when_true,
                    parameters,
                    divert_parameters,
                )?;
                if let Some(wf) = when_false {
                    self.validate_nodes_variable_divert_targets(wf, parameters, divert_parameters)?;
                }
            }
            Node::SwitchConditional { branches, .. } => {
                for (_, body) in branches {
                    self.validate_nodes_variable_divert_targets(
                        body,
                        parameters,
                        divert_parameters,
                    )?;
                }
            }
            Node::Sequence(sequence) => {
                for branch in &sequence.branches {
                    self.validate_nodes_variable_divert_targets(
                        branch,
                        parameters,
                        divert_parameters,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn check_variable_divert_target(
        &self,
        target: &str,
        parameters: &BTreeSet<String>,
        divert_parameters: &BTreeSet<String>,
    ) -> Result<(), CompilerError> {
        if parameters.contains(target) && !divert_parameters.contains(target) {
            return Err(CompilerError::invalid_source(format!(
                "Since '{target}' is used as a variable divert target, it should be marked as: -> {target}"
            )));
        }

        Ok(())
    }

    /// Error if a DivertTarget expression wraps a name that is already a divert parameter
    /// (it shouldn't be preceded by '->').
    fn check_divert_target_arg(
        &self,
        expr: &Expression,
        divert_parameters: &BTreeSet<String>,
    ) -> Result<(), CompilerError> {
        if let Expression::DivertTarget(name) = expr
            && divert_parameters.contains(name.as_str())
        {
            return Err(CompilerError::invalid_source(format!(
                "The parameter '{name}' is already a divert target; \
                 it shouldn't be preceded by '->'."
            )));
        }
        Ok(())
    }

}
