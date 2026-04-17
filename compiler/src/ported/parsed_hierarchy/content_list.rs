use std::rc::Rc;

use bladeink::compiler_support::{
    CommandType, ControlCommand, Divert, PushPopType, RTObject, Value,
};

use crate::error::CompilerError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentItem {
    Text(String),
    Newline,
    Divert(String),
    End,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContentList {
    pub items: Vec<ContentItem>,
}

impl ContentList {
    pub fn new(items: Vec<ContentItem>) -> Self {
        Self { items }
    }

    pub fn push_text(&mut self, text: impl Into<String>) {
        self.items.push(ContentItem::Text(text.into()));
    }

    pub fn to_runtime(&self) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
        self.items
            .iter()
            .map(|item| match item {
                ContentItem::Text(text) => {
                    Ok(Rc::new(Value::new(text.as_str())) as Rc<dyn RTObject>)
                }
                ContentItem::Newline => Ok(Rc::new(Value::new("\n")) as Rc<dyn RTObject>),
                ContentItem::Divert(target) => Ok(Rc::new(Divert::new(
                    false,
                    PushPopType::Tunnel,
                    false,
                    0,
                    false,
                    None,
                    Some(target),
                )) as Rc<dyn RTObject>),
                ContentItem::End => {
                    Ok(Rc::new(ControlCommand::new(CommandType::End)) as Rc<dyn RTObject>)
                }
                ContentItem::Done => {
                    Ok(Rc::new(ControlCommand::new(CommandType::Done)) as Rc<dyn RTObject>)
                }
            })
            .collect()
    }

    pub fn to_runtime_with_absolute_diverts(
        &self,
        rewrite: &dyn Fn(&str) -> String,
    ) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
        self.items
            .iter()
            .map(|item| match item {
                ContentItem::Divert(target) => Ok(Rc::new(Divert::new(
                    false,
                    PushPopType::Tunnel,
                    false,
                    0,
                    false,
                    None,
                    Some(&rewrite(target)),
                )) as Rc<dyn RTObject>),
                ContentItem::Text(text) => {
                    Ok(Rc::new(Value::new(text.as_str())) as Rc<dyn RTObject>)
                }
                ContentItem::Newline => Ok(Rc::new(Value::new("\n")) as Rc<dyn RTObject>),
                ContentItem::End => {
                    Ok(Rc::new(ControlCommand::new(CommandType::End)) as Rc<dyn RTObject>)
                }
                ContentItem::Done => {
                    Ok(Rc::new(ControlCommand::new(CommandType::Done)) as Rc<dyn RTObject>)
                }
            })
            .collect()
    }

    pub fn from_legacy_nodes(
        nodes: &[crate::parsed_hierarchy::Node],
    ) -> Result<Self, CompilerError> {
        let mut items = Vec::new();
        for node in nodes {
            match node {
                crate::parsed_hierarchy::Node::Text(text) => {
                    items.push(ContentItem::Text(text.clone()))
                }
                crate::parsed_hierarchy::Node::Newline => items.push(ContentItem::Newline),
                crate::parsed_hierarchy::Node::Divert(divert) if divert.arguments.is_empty() => {
                    match divert.target.as_str() {
                        "END" => items.push(ContentItem::End),
                        "DONE" => items.push(ContentItem::Done),
                        target => items.push(ContentItem::Divert(target.to_owned())),
                    }
                }
                _ => {
                    return Err(CompilerError::UnsupportedFeature(
                        "ported choice adapter currently supports only text, newline, and simple divert content"
                            .to_owned(),
                    ))
                }
            }
        }
        Ok(Self { items })
    }
}
