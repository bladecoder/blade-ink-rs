use std::collections::{BTreeMap, BTreeSet};

use bladeink::story::INK_VERSION_CURRENT;
use serde_json::{json, Map, Value};

use crate::error::CompilerError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    Bool(bool),
    FunctionCall(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Bool(bool),
    Int(i32),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalVariable {
    pub name: String,
    pub initial_value: Expression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChoiceStyle {
    ChoiceOnly,
    EchoLabelOnSelect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub body: Vec<Node>,
    pub style: ChoiceStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Text(String),
    OutputExpression(Expression),
    Newline,
    Glue,
    Divert(String),
    Conditional {
        condition: Condition,
        branch: Vec<Node>,
    },
    ReturnBool(bool),
    Assignment {
        variable_name: String,
        expression: Expression,
        mode: AssignMode,
    },
    Choice(Choice),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flow {
    pub name: String,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
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

struct EmitContext {
    global_variables: BTreeSet<String>,
    flow_names: BTreeSet<String>,
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

        let mut root_container = emit_nodes(&self.root, "0", &context)?;
        root_container.push(json!(["done", {"#n": "g-0"}]));

        let mut named_content = Map::new();

        for flow in &self.flows {
            let flow_container = emit_nodes(&flow.nodes, &flow.name, &context)?;
            named_content.insert(
                flow.name.clone(),
                flow_container.into_json_array(None, None)?,
            );
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
            flow_names: story.flows.iter().map(|flow| flow.name.clone()).collect(),
        }
    }

    fn is_variable_divert(&self, target: &str) -> bool {
        self.global_variables.contains(target) && !self.flow_names.contains(target)
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

fn emit_nodes(
    nodes: &[Node],
    container_path: &str,
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
            Node::Divert(target) => {
                if target == "END" {
                    out.push(json!("end"));
                } else if target == "DONE" {
                    out.push(json!("done"));
                } else if context.is_variable_divert(target) {
                    out.push(json!({"->": target, "var": true}));
                } else {
                    out.push(json!({"->": target}));
                }
            }
            Node::ReturnBool(value) => {
                out.push(json!("ev"));
                out.push(json!(value));
                out.push(json!("/ev"));
                out.push(json!("~ret"));
            }
            Node::Conditional { condition, branch } => out.push(emit_conditional(
                condition,
                branch,
                container_path,
                out.content.len(),
                context,
            )?),
            Node::Assignment {
                variable_name,
                expression,
                mode,
            } => emit_assignment(variable_name, expression, mode, &mut out),
            Node::Choice(choice) => {
                emit_choice(&mut out, choice, container_path, next_choice_index, context)?;
                next_choice_index += 1;
            }
        }
    }

    Ok(out)
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
    container_path: &str,
    choice_index: usize,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    out.push(json!("ev"));
    out.push(json!("str"));
    out.push(json!(format!("^{}", choice.label)));
    out.push(json!("/str"));
    out.push(json!("/ev"));

    let branch_name = format!("c-{choice_index}");
    let branch_path = format!("{container_path}.{branch_name}");
    out.push(json!({
        "*": branch_path,
        "flg": 20
    }));

    let mut branch_nodes = choice.body.clone();
    if choice.style == ChoiceStyle::EchoLabelOnSelect {
        branch_nodes.insert(0, Node::Newline);
        branch_nodes.insert(0, Node::Text(choice.label.clone()));
    }

    let branch_container = emit_nodes(&branch_nodes, &branch_path, context)?;
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
            }));
        }
    }
}

fn emit_conditional(
    condition: &Condition,
    branch: &[Node],
    container_path: &str,
    conditional_index: usize,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let mut out = Vec::new();

    out.push(json!("ev"));

    match condition {
        Condition::Bool(value) => out.push(json!(value)),
        Condition::FunctionCall(name) => {
            let mut call = BTreeMap::new();
            call.insert(format!("{name}()"), Value::String(name.clone()));
            out.push(serde_json::to_value(call).map_err(|error| {
                CompilerError::InvalidSource(format!("failed to serialize function call: {error}"))
            })?);
        }
    }

    out.push(json!("/ev"));

    let mut branch_content = emit_nodes(branch, container_path, context)?;
    branch_content.push(json!({"->": format!("{container_path}.{}", conditional_index + 1)}));

    let mut named = Map::new();
    named.insert("b".to_owned(), branch_content.into_json_array(None, None)?);

    out.push(Value::Array(vec![
        json!({"->": ".^.b", "c": true}),
        Value::Object(named),
    ]));
    out.push(json!("nop"));

    Ok(Value::Array(out))
}
