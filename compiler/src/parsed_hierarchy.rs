use std::collections::{BTreeMap, BTreeSet};

use bladeink::story::INK_VERSION_CURRENT;
use serde_json::{json, Map, Value};

use crate::error::CompilerError;

#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    Bool(bool),
    FunctionCall(String),
    Expression(Expression),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Equal,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    Variable(String),
    DivertTarget(String),
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignMode {
    Set,
    AddAssign,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalVariable {
    pub name: String,
    pub initial_value: Expression,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Divert {
    pub target: String,
    pub arguments: Vec<Expression>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub display_text: String,
    pub selected_text: Option<String>,
    pub body: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Text(String),
    OutputExpression(Expression),
    Newline,
    Glue,
    Divert(Divert),
    Conditional {
        condition: Condition,
        when_true: Vec<Node>,
        when_false: Option<Vec<Node>>,
    },
    ReturnBool(bool),
    Assignment {
        variable_name: String,
        expression: Expression,
        mode: AssignMode,
    },
    Choice(Choice),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Flow {
    pub name: String,
    pub parameters: Vec<String>,
    pub nodes: Vec<Node>,
    pub children: Vec<Flow>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ParsedStory {
    globals: Vec<GlobalVariable>,
    root: Vec<Node>,
    flows: Vec<Flow>,
}

#[derive(Debug, Default)]
struct EmittedContainer {
    content: Vec<Value>,
    named: Map<String, Value>,
}

#[derive(Clone)]
struct EmitScope {
    path: String,
    top_flow_name: Option<String>,
    child_flow_names: BTreeSet<String>,
}

struct EmitContext {
    global_variables: BTreeSet<String>,
    top_flow_names: BTreeSet<String>,
}

impl EmittedContainer {
    fn push(&mut self, value: Value) {
        self.content.push(value);
    }

    fn insert_named(&mut self, name: String, value: Value) {
        self.named.insert(name, value);
    }

    fn into_json_array(
        self,
        name: Option<&str>,
        count_flags: Option<i32>,
    ) -> Result<Value, CompilerError> {
        let mut values = self.content;
        let has_name = name.is_some();
        let has_flags = count_flags.unwrap_or_default() > 0;

        if !self.named.is_empty() || has_name || has_flags {
            let mut terminator = self.named;

            if let Some(flags) = count_flags.filter(|flags| *flags > 0) {
                terminator.insert("#f".to_owned(), json!(flags));
            }

            if let Some(name) = name {
                terminator.insert("#n".to_owned(), json!(name));
            }

            values.push(Value::Object(terminator));
        } else {
            values.push(Value::Null);
        }

        Ok(Value::Array(values))
    }
}

impl ParsedStory {
    pub fn new(globals: Vec<GlobalVariable>, root: Vec<Node>, flows: Vec<Flow>) -> Self {
        Self {
            globals,
            root,
            flows,
        }
    }

    pub fn to_json_value(&self) -> Result<Value, CompilerError> {
        let context = EmitContext::new(self);
        let root_scope = EmitScope::root(&self.flows);

        let mut root_container = emit_nodes(&self.root, &root_scope, &context)?;
        root_container.push(json!(["done", {"#n": "g-0"}]));

        let mut named_content = Map::new();

        for flow in &self.flows {
            named_content.insert(flow.name.clone(), emit_flow(flow, &context)?);
        }

        if !self.globals.is_empty() {
            named_content.insert(
                "global decl".to_owned(),
                emit_global_declarations(&self.globals)?.into_json_array(None, None)?,
            );
        }

        Ok(json!({
            "inkVersion": INK_VERSION_CURRENT,
            "root": [
                root_container.into_json_array(None, None)?,
                "done",
                if named_content.is_empty() {
                    Value::Null
                } else {
                    Value::Object(named_content)
                }
            ],
            "listDefs": {}
        }))
    }

    pub fn to_json_string(&self) -> Result<String, CompilerError> {
        let json = self.to_json_value()?;
        serde_json::to_string(&json).map_err(|error| {
            CompilerError::InvalidSource(format!("failed to serialize compiled ink: {error}"))
        })
    }
}

impl EmitContext {
    fn new(story: &ParsedStory) -> Self {
        Self {
            global_variables: story.globals.iter().map(|var| var.name.clone()).collect(),
            top_flow_names: story.flows.iter().map(|flow| flow.name.clone()).collect(),
        }
    }
}

impl EmitScope {
    fn root(flows: &[Flow]) -> Self {
        Self {
            path: "0".to_owned(),
            top_flow_name: None,
            child_flow_names: flows.iter().map(|flow| flow.name.clone()).collect(),
        }
    }

    fn child_flow(&self, child: &Flow) -> Self {
        let path = if self.path == "0" {
            child.name.clone()
        } else {
            format!("{}.{}", self.path, child.name)
        };

        Self {
            path,
            top_flow_name: self
                .top_flow_name
                .clone()
                .or_else(|| Some(child.name.clone())),
            child_flow_names: child
                .children
                .iter()
                .map(|nested| nested.name.clone())
                .collect(),
        }
    }

    fn choice_branch(&self, branch_name: &str) -> Self {
        Self {
            path: format!("{}.{}", self.path, branch_name),
            top_flow_name: self.top_flow_name.clone(),
            child_flow_names: self.child_flow_names.clone(),
        }
    }

    fn resolve_divert_target(&self, target: &str, context: &EmitContext) -> String {
        if target == "END" || target == "DONE" || target.contains('.') {
            return target.to_owned();
        }

        if context.global_variables.contains(target) && !context.top_flow_names.contains(target) {
            return target.to_owned();
        }

        if self.child_flow_names.contains(target) {
            if let Some(top_flow_name) = &self.top_flow_name {
                return format!("{top_flow_name}.{target}");
            }
        }

        target.to_owned()
    }

    fn is_variable_divert(&self, target: &str, context: &EmitContext) -> bool {
        context.global_variables.contains(target) && !context.top_flow_names.contains(target)
    }
}

fn emit_global_declarations(globals: &[GlobalVariable]) -> Result<EmittedContainer, CompilerError> {
    let mut container = EmittedContainer::default();
    container.push(json!("ev"));

    for global in globals {
        emit_expression(&global.initial_value, &mut container.content);
        container.push(json!({ "VAR=": global.name }));
    }

    container.push(json!("/ev"));
    container.push(json!("end"));

    Ok(container)
}

fn emit_flow(flow: &Flow, context: &EmitContext) -> Result<Value, CompilerError> {
    let parent_scope = EmitScope::root(&[]);
    let scope = parent_scope.child_flow(flow);
    let mut container = emit_nodes(&flow.nodes, &scope, context)?;

    prepend_parameters(&mut container, &flow.parameters);

    if container.content.is_empty() && !flow.children.is_empty() {
        let target = if let Some(top_flow_name) = &scope.top_flow_name {
            format!("{top_flow_name}.{}", flow.children[0].name)
        } else {
            flow.children[0].name.clone()
        };
        container.push(json!({"->": target}));
    }

    for child in &flow.children {
        container.insert_named(
            child.name.clone(),
            emit_nested_flow(child, &scope, context)?,
        );
    }

    container.into_json_array(None, None)
}

fn emit_nested_flow(
    flow: &Flow,
    parent_scope: &EmitScope,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let scope = parent_scope.child_flow(flow);
    let mut container = emit_nodes(&flow.nodes, &scope, context)?;

    prepend_parameters(&mut container, &flow.parameters);

    if container.content.is_empty() && !flow.children.is_empty() {
        let target = if let Some(top_flow_name) = &scope.top_flow_name {
            format!("{top_flow_name}.{}", flow.children[0].name)
        } else {
            flow.children[0].name.clone()
        };
        container.push(json!({"->": target}));
    }

    for child in &flow.children {
        container.insert_named(
            child.name.clone(),
            emit_nested_flow(child, &scope, context)?,
        );
    }

    container.into_json_array(None, None)
}

fn prepend_parameters(container: &mut EmittedContainer, parameters: &[String]) {
    if parameters.is_empty() {
        return;
    }

    let mut prefix: Vec<Value> = parameters
        .iter()
        .rev()
        .map(|parameter| json!({"temp=": parameter}))
        .collect();
    prefix.append(&mut container.content);
    container.content = prefix;
}

fn emit_nodes(
    nodes: &[Node],
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<EmittedContainer, CompilerError> {
    let mut out = EmittedContainer::default();
    let mut next_choice_index = 0;

    for node in nodes {
        match node {
            Node::Text(text) => out.push(json!(format!("^{text}"))),
            Node::OutputExpression(expression) => {
                out.push(json!("ev"));
                emit_expression(expression, &mut out.content);
                out.push(json!("out"));
                out.push(json!("/ev"));
            }
            Node::Newline => out.push(json!("\n")),
            Node::Glue => out.push(json!("<>")),
            Node::Divert(divert) => emit_divert(&mut out, divert, scope, context),
            Node::ReturnBool(value) => {
                out.push(json!("ev"));
                out.push(json!(value));
                out.push(json!("/ev"));
                out.push(json!("~ret"));
            }
            Node::Conditional {
                condition,
                when_true,
                when_false,
            } => out.push(emit_conditional(
                condition,
                when_true,
                when_false.as_deref(),
                scope,
                out.content.len(),
                context,
            )?),
            Node::Assignment {
                variable_name,
                expression,
                mode,
            } => emit_assignment(variable_name, expression, mode, &mut out),
            Node::Choice(choice) => {
                emit_choice(&mut out, choice, scope, next_choice_index, context)?;
                next_choice_index += 1;
            }
        }
    }

    Ok(out)
}

fn emit_divert(
    out: &mut EmittedContainer,
    divert: &Divert,
    scope: &EmitScope,
    context: &EmitContext,
) {
    let resolved_target = scope.resolve_divert_target(&divert.target, context);

    if resolved_target == "END" {
        out.push(json!("end"));
        return;
    }

    if resolved_target == "DONE" {
        out.push(json!("done"));
        return;
    }

    if !divert.arguments.is_empty() {
        out.push(json!("ev"));
        for argument in &divert.arguments {
            emit_expression(argument, &mut out.content);
        }
        out.push(json!("/ev"));
    }

    if scope.is_variable_divert(&resolved_target, context) {
        out.push(json!({"->": resolved_target, "var": true}));
    } else {
        out.push(json!({"->": resolved_target}));
    }
}

fn emit_assignment(
    variable_name: &str,
    expression: &Expression,
    mode: &AssignMode,
    out: &mut EmittedContainer,
) {
    match mode {
        AssignMode::Set => {
            out.push(json!("ev"));
            emit_expression(expression, &mut out.content);
            out.push(json!("/ev"));
            out.push(json!({"VAR=": variable_name, "re": true}));
        }
        AssignMode::AddAssign => {
            out.push(json!("ev"));
            emit_expression(
                &Expression::Variable(variable_name.to_owned()),
                &mut out.content,
            );
            emit_expression(expression, &mut out.content);
            out.push(json!("+"));
            out.push(json!({"VAR=": variable_name, "re": true}));
            out.push(json!("/ev"));
        }
    }
}

fn emit_choice(
    out: &mut EmittedContainer,
    choice: &Choice,
    scope: &EmitScope,
    choice_index: usize,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    out.push(json!("ev"));
    out.push(json!("str"));
    out.push(json!(format!("^{}", choice.display_text)));
    out.push(json!("/str"));
    out.push(json!("/ev"));

    let branch_name = format!("c-{choice_index}");
    let branch_scope = scope.choice_branch(&branch_name);
    out.push(json!({
        "*": branch_scope.path,
        "flg": 20
    }));

    let mut branch_nodes = Vec::new();
    if let Some(selected_text) = &choice.selected_text {
        branch_nodes.push(Node::Text(selected_text.clone()));
        branch_nodes.push(Node::Newline);
    }
    branch_nodes.extend(choice.body.clone());

    let branch_container = emit_nodes(&branch_nodes, &branch_scope, context)?;
    out.insert_named(
        branch_name,
        branch_container.into_json_array(None, Some(5))?,
    );

    Ok(())
}

fn emit_expression(expression: &Expression, out: &mut Vec<Value>) {
    match expression {
        Expression::Bool(value) => out.push(json!(value)),
        Expression::Int(value) => out.push(json!(value)),
        Expression::Float(value) => out.push(json!(value)),
        Expression::Str(value) => {
            out.push(json!("str"));
            out.push(json!(format!("^{value}")));
            out.push(json!("/str"));
        }
        Expression::Variable(name) => out.push(json!({"VAR?": name})),
        Expression::DivertTarget(target) => out.push(json!({"^->": target})),
        Expression::Binary {
            left,
            operator,
            right,
        } => {
            emit_expression(left, out);
            emit_expression(right, out);
            out.push(json!(match operator {
                BinaryOperator::Add => "+",
                BinaryOperator::Subtract => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::Equal => "==",
            }));
        }
    }
}

fn emit_condition(condition: &Condition, out: &mut Vec<Value>) -> Result<(), CompilerError> {
    match condition {
        Condition::Bool(value) => out.push(json!(value)),
        Condition::FunctionCall(name) => {
            let mut call = BTreeMap::new();
            call.insert(format!("{name}()"), Value::String(name.clone()));
            out.push(serde_json::to_value(call).map_err(|error| {
                CompilerError::InvalidSource(format!("failed to serialize function call: {error}"))
            })?);
        }
        Condition::Expression(expression) => emit_expression(expression, out),
    }

    Ok(())
}

fn emit_conditional(
    condition: &Condition,
    when_true: &[Node],
    when_false: Option<&[Node]>,
    scope: &EmitScope,
    conditional_index: usize,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let mut out = Vec::new();

    out.extend(emit_conditional_branch(
        Some(condition),
        when_true,
        scope,
        conditional_index,
        0,
        context,
    )?);

    if let Some(when_false) = when_false {
        out.extend(emit_conditional_branch(
            None,
            when_false,
            scope,
            conditional_index,
            1,
            context,
        )?);
    }

    out.push(json!("nop"));

    Ok(Value::Array(out))
}

fn emit_conditional_branch(
    condition: Option<&Condition>,
    branch: &[Node],
    scope: &EmitScope,
    conditional_index: usize,
    _branch_index: usize,
    context: &EmitContext,
) -> Result<Vec<Value>, CompilerError> {
    let mut out = Vec::new();

    if let Some(condition) = condition {
        out.push(json!("ev"));
        emit_condition(condition, &mut out)?;
        out.push(json!("/ev"));
    }

    let mut branch_content = emit_nodes(branch, scope, context)?;
    branch_content.push(json!({"->": format!("{}.{}", scope.path, conditional_index + 1)}));

    let mut named = Map::new();
    named.insert("b".to_owned(), branch_content.into_json_array(None, None)?);

    if condition.is_some() {
        out.push(Value::Array(vec![
            json!({"->": ".^.b", "c": true}),
            Value::Object(named),
        ]));
    } else {
        out.push(Value::Array(vec![
            json!({"->": ".^.b"}),
            Value::Object(named),
        ]));
    }

    Ok(out)
}
