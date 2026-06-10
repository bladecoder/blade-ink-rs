impl ValidationContext {
    fn validate_nodes_vars(&self, nodes: &[Node], scope: &ScopeInfo) -> Result<(), CompilerError> {
        // Only validate if we have a restricted scope (inside a stitch)
        if scope.forbidden.is_empty() {
            return Ok(());
        }
        for node in nodes {
            self.validate_node_vars(node, scope)?;
        }
        Ok(())
    }

    fn validate_node_vars(&self, node: &Node, scope: &ScopeInfo) -> Result<(), CompilerError> {
        match node {
            Node::OutputExpression(expr) => self.validate_expr_vars(expr, scope)?,
            Node::Assignment { expression, .. } => self.validate_expr_vars(expression, scope)?,
            Node::ReturnExpr(expr) => self.validate_expr_vars(expr, scope)?,
            Node::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                self.validate_condition_vars(condition, scope)?;
                self.validate_nodes_vars(when_true, scope)?;
                if let Some(wf) = when_false {
                    self.validate_nodes_vars(wf, scope)?;
                }
            }
            Node::SwitchConditional { value, branches } => {
                self.validate_expr_vars(value, scope)?;
                for (case, body) in branches {
                    if let Some(e) = case {
                        self.validate_expr_vars(e, scope)?;
                    }
                    self.validate_nodes_vars(body, scope)?;
                }
            }
            Node::Choice(c) => {
                for cond in &c.conditions {
                    self.validate_condition_vars(cond, scope)?;
                }
                self.validate_nodes_vars(&c.body, scope)?;
            }
            Node::Sequence(seq) => {
                for branch in &seq.branches {
                    self.validate_nodes_vars(branch, scope)?;
                }
            }
            Node::VoidCall { args, .. } => {
                for a in args {
                    self.validate_expr_vars(a, scope)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_condition_vars(
        &self,
        cond: &Condition,
        scope: &ScopeInfo,
    ) -> Result<(), CompilerError> {
        if let Condition::Expression(expr) = cond {
            self.validate_expr_vars(expr, scope)?;
        }
        Ok(())
    }

    fn validate_expr_vars(
        &self,
        expr: &Expression,
        scope: &ScopeInfo,
    ) -> Result<(), CompilerError> {
        match expr {
            Expression::Variable(name) if scope.forbidden.contains(name.as_str()) => {
                return Err(CompilerError::invalid_source(format!(
                    "Unresolved variable: {name}"
                )));
            }
            Expression::Variable(_) => {}
            Expression::Negate(e) | Expression::Not(e) => {
                self.validate_expr_vars(e, scope)?;
            }
            Expression::Binary { left, right, .. } => {
                self.validate_expr_vars(left, scope)?;
                self.validate_expr_vars(right, scope)?;
            }
            Expression::FunctionCall { args, .. } => {
                for a in args {
                    self.validate_expr_vars(a, scope)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct ScopeInfo {
    /// Variables that are explicitly forbidden in this scope (e.g., parent knot's temps when inside a stitch)
    forbidden: BTreeSet<String>,
}

/// Collect all temp variable names defined via `~temp x = ...` or `~ temp x = ...`
/// (i.e. `Node::Assignment { mode: TempSet }`) within a flat list of nodes.
/// Does NOT descend into child flows.
fn collect_temps_from_nodes(nodes: &[Node]) -> BTreeSet<String> {
    let mut temps = BTreeSet::new();
    collect_temps_recursive(nodes, &mut temps);
    temps
}

fn collect_temps_recursive(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Assignment {
                variable_name,
                mode: AssignMode::TempSet,
                ..
            } => {
                out.insert(variable_name.clone());
            }
            Node::Choice(c) => collect_temps_recursive(&c.body, out),
            Node::Conditional {
                when_true,
                when_false,
                ..
            } => {
                collect_temps_recursive(when_true, out);
                if let Some(wf) = when_false {
                    collect_temps_recursive(wf, out);
                }
            }
            Node::SwitchConditional { branches, .. } => {
                for (_, body) in branches {
                    collect_temps_recursive(body, out);
                }
            }
            Node::Sequence(seq) => {
                for branch in &seq.branches {
                    collect_temps_recursive(branch, out);
                }
            }
            _ => {}
        }
    }
}
