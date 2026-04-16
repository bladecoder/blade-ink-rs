use std::collections::BTreeMap;

use bladeink::story::INK_VERSION_CURRENT;
use serde_json::{json, Map, Value};

use crate::error::CompilerError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    Bool(bool),
    FunctionCall(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Text(String),
    Newline,
    Glue,
    Divert(String),
    Conditional {
        condition: Condition,
        branch: Vec<Node>,
    },
    ReturnBool(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flow {
    pub name: String,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParsedStory {
    root: Vec<Node>,
    flows: Vec<Flow>,
}

impl ParsedStory {
    pub fn new(root: Vec<Node>, flows: Vec<Flow>) -> Self {
        Self { root, flows }
    }

    pub fn to_json_value(&self) -> Result<Value, CompilerError> {
        let mut root_content = emit_nodes(&self.root, "0")?;
        root_content.push(json!(["done", {"#n": "g-0"}]));
        root_content.push(Value::Null);

        let named_content = if self.flows.is_empty() {
            Value::Null
        } else {
            let mut named = Map::new();

            for flow in &self.flows {
                let mut flow_content = emit_nodes(&flow.nodes, &flow.name)?;
                flow_content.push(Value::Null);
                named.insert(flow.name.clone(), Value::Array(flow_content));
            }

            Value::Object(named)
        };

        Ok(json!({
            "inkVersion": INK_VERSION_CURRENT,
            "root": [root_content, "done", named_content],
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

fn emit_nodes(nodes: &[Node], flow_name: &str) -> Result<Vec<Value>, CompilerError> {
    let mut out = Vec::new();

    for node in nodes {
        match node {
            Node::Text(text) => out.push(json!(format!("^{text}"))),
            Node::Newline => out.push(json!("\n")),
            Node::Glue => out.push(json!("<>")),
            Node::Divert(target) => {
                if target == "END" {
                    out.push(json!("end"));
                } else if target == "DONE" {
                    out.push(json!("done"));
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
            Node::Conditional { condition, branch } => {
                emit_conditional(&mut out, condition, branch, flow_name)?;
            }
        }
    }

    Ok(out)
}

fn emit_conditional(
    out: &mut Vec<Value>,
    condition: &Condition,
    branch: &[Node],
    flow_name: &str,
) -> Result<(), CompilerError> {
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

    let nop_index = out.len() + 1;
    let jump_target = format!("{flow_name}.{nop_index}");

    let mut branch_content = emit_nodes(branch, flow_name)?;
    branch_content.push(json!({"->": jump_target}));
    branch_content.push(Value::Null);

    let mut named = Map::new();
    named.insert("b".to_owned(), Value::Array(branch_content));

    out.push(Value::Array(vec![
        json!({"->": ".^.b", "c": true}),
        Value::Object(named),
    ]));
    out.push(json!("nop"));

    Ok(())
}
