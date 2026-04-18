/// Semantic validation pass over the parsed AST.
///
/// Runs after parsing and before emitting JSON. Detects errors that require
/// knowledge of the whole story structure, such as:
/// - Divert targets that don't exist anywhere in the story
/// - Variables referenced in a stitch/knot that are not in scope
use std::collections::BTreeSet;

use crate::{
    ast::{AssignMode, Condition, Divert, Expression, Node, ParsedStory},
    error::CompilerError,
};

/// Validate the parsed story. Returns the first error found, or Ok(()).
pub fn validate(story: &ParsedStory) -> Result<(), CompilerError> {
    let ctx = ValidationContext::build(story);
    ctx.validate_story(story)
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

struct ValidationContext {
    /// All valid divert targets: knot names, "knot.stitch", "knot.stitch.label",
    /// gather labels at root scope, etc.
    valid_targets: BTreeSet<String>,
    /// Top-level flow names (knots + functions).
    flow_names: BTreeSet<String>,
    /// Function flow names.
    function_names: BTreeSet<String>,
    /// Declared EXTERNAL function names.
    external_functions: BTreeSet<String>,
}

impl ValidationContext {
    fn build(story: &ParsedStory) -> Self {
        let mut valid_targets: BTreeSet<String> = BTreeSet::new();
        let mut flow_names: BTreeSet<String> = BTreeSet::new();
        let mut function_names: BTreeSet<String> = BTreeSet::new();

        // Reserved targets always valid
        for t in &["END", "DONE", "->->"] {
            valid_targets.insert(t.to_string());
        }

        // Global VARs can hold divert values and be used as divert targets
        for g in story.globals() {
            valid_targets.insert(g.name.clone());
        }

        // Collect gather/choice labels from root nodes
        collect_labels_from_nodes(story.root(), "", &mut valid_targets);

        // Collect knot/stitch names, their labels, and parameters (params can be divert targets)
        for flow in story.flows() {
            flow_names.insert(flow.name.clone());
            if flow.is_function {
                function_names.insert(flow.name.clone());
            }
            valid_targets.insert(flow.name.clone());
            // Parameters can be used as divert targets inside the flow
            for param in &flow.parameters {
                valid_targets.insert(param.clone());
            }
            // Gather labels within the knot's own nodes
            collect_labels_from_nodes(&flow.nodes, &flow.name, &mut valid_targets);
            for stitch in &flow.children {
                let qualified = format!("{}.{}", flow.name, stitch.name);
                valid_targets.insert(qualified.clone());
                for param in &stitch.parameters {
                    valid_targets.insert(param.clone());
                }
                collect_labels_from_nodes(&stitch.nodes, &qualified, &mut valid_targets);
            }
        }

        Self {
            valid_targets,
            flow_names,
            function_names,
            external_functions: story.external_functions.iter().cloned().collect(),
        }
    }

    fn validate_story(&self, story: &ParsedStory) -> Result<(), CompilerError> {
        // Validate root nodes
        self.validate_nodes_diverts(story.root(), "")?;
        self.validate_nodes_function_calls(story.root())?;

        // Validate each flow
        for flow in story.flows() {
            // Collect all temps defined in this flow's own nodes (includes nested nodes)
            let flow_params: BTreeSet<String> = flow.parameters.iter().cloned().collect();
            let flow_divert_params: BTreeSet<String> =
                flow.divert_parameters.iter().cloned().collect();
            let flow_temps = collect_temps_from_nodes(&flow.nodes);
            let flow_scope = ScopeInfo {
                forbidden: BTreeSet::new(),
            };

            self.validate_nodes_diverts(&flow.nodes, &flow.name)?;
            self.validate_nodes_function_calls(&flow.nodes)?;
            self.validate_nodes_variable_divert_targets(
                &flow.nodes,
                &flow_params,
                &flow_divert_params,
            )?;
            self.validate_nodes_vars(&flow.nodes, &flow_scope)?;

            for stitch in &flow.children {
                let stitch_params: BTreeSet<String> = stitch.parameters.iter().cloned().collect();
                let stitch_divert_params: BTreeSet<String> =
                    stitch.divert_parameters.iter().cloned().collect();
                let stitch_temps = collect_temps_from_nodes(&stitch.nodes);
                // Forbidden: parent knot's temps and params that are NOT also defined in the stitch
                let mut forbidden = BTreeSet::new();
                for name in flow_temps.iter().chain(flow_params.iter()) {
                    if !stitch_params.contains(name) && !stitch_temps.contains(name) {
                        forbidden.insert(name.clone());
                    }
                }
                // A stitch can only see its own params/temps, NOT the parent knot's
                let stitch_scope = ScopeInfo { forbidden };

                let qualified = format!("{}.{}", flow.name, stitch.name);
                self.validate_nodes_diverts(&stitch.nodes, &qualified)?;
                self.validate_nodes_function_calls(&stitch.nodes)?;
                self.validate_nodes_variable_divert_targets(
                    &stitch.nodes,
                    &stitch_params,
                    &stitch_divert_params,
                )?;
                self.validate_nodes_vars(&stitch.nodes, &stitch_scope)?;
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Divert validation
    // -----------------------------------------------------------------------

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
            }
            Node::TunnelDivert { target, .. } => {
                self.check_variable_divert_target(target, parameters, divert_parameters)?;
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

    // -----------------------------------------------------------------------
    // Variable scope validation
    // -----------------------------------------------------------------------

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

fn is_builtin_function(name: &str) -> bool {
    matches!(
        name,
        "RANDOM"
            | "SEED_RANDOM"
            | "POW"
            | "FLOOR"
            | "CEILING"
            | "INT"
            | "FLOAT"
            | "MIN"
            | "MAX"
            | "READ_COUNT"
            | "TURNS_SINCE"
            | "CHOICE_COUNT"
            | "TURNS"
            | "LIST_VALUE"
            | "LIST_ALL"
            | "LIST_INVERT"
            | "LIST_COUNT"
            | "LIST_MIN"
            | "LIST_MAX"
            | "LIST_RANGE"
            | "LIST_RANDOM"
    )
}

/// Collect gather/choice labels and their qualified paths into `targets`.
fn collect_labels_from_nodes(nodes: &[Node], prefix: &str, targets: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::GatherLabel(label) => {
                if prefix.is_empty() {
                    targets.insert(label.clone());
                } else {
                    targets.insert(format!("{prefix}.{label}"));
                }
            }
            Node::Choice(c) => {
                if let Some(lbl) = &c.label {
                    if prefix.is_empty() {
                        targets.insert(lbl.clone());
                    } else {
                        targets.insert(format!("{prefix}.{lbl}"));
                    }
                }
                collect_labels_from_nodes(&c.body, prefix, targets);
            }
            Node::Conditional {
                when_true,
                when_false,
                ..
            } => {
                collect_labels_from_nodes(when_true, prefix, targets);
                if let Some(wf) = when_false {
                    collect_labels_from_nodes(wf, prefix, targets);
                }
            }
            Node::SwitchConditional { branches, .. } => {
                for (_, body) in branches {
                    collect_labels_from_nodes(body, prefix, targets);
                }
            }
            Node::Sequence(seq) => {
                for branch in &seq.branches {
                    collect_labels_from_nodes(branch, prefix, targets);
                }
            }
            _ => {}
        }
    }
}
