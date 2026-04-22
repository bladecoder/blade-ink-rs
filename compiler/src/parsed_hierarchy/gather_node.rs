use crate::error::CompilerError;
use std::collections::HashSet;

use super::{ParsedNode, ParsedNodeKind};

#[derive(Debug, Clone)]
pub struct GatherNodeSpec {
    pub(crate) source_node: Option<ParsedNode>,
    pub indentation_depth: usize,
    pub identifier: Option<String>,
    pub content: Vec<ParsedNode>,
}

impl GatherNodeSpec {
    pub fn from_node(node: &ParsedNode) -> Option<Self> {
        let gather = GatherNode::from_node(node)?;
        Some(Self {
            source_node: Some(node.clone()),
            indentation_depth: gather.indentation_depth(),
            identifier: gather.identifier().map(ToOwned::to_owned),
            content: gather.content().to_vec(),
        })
    }

    pub fn build(self) -> ParsedNode {
        let mut node = ParsedNode::new(if self.identifier.is_some() {
            ParsedNodeKind::GatherLabel
        } else {
            ParsedNodeKind::GatherPoint
        });
        node.indentation_depth = self.indentation_depth;
        if let Some(identifier) = self.identifier {
            node = node.with_name(identifier);
        }
        if !self.content.is_empty() {
            node = node.with_children(self.content);
        }
        node
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub fn indentation_depth(&self) -> usize {
        self.indentation_depth
    }

    pub fn content(&self) -> &[ParsedNode] {
        &self.content
    }

    pub fn is_label(&self) -> bool {
        self.identifier.is_some()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GatherNode<'a> {
    node: &'a ParsedNode,
}

impl<'a> GatherNode<'a> {
    pub fn from_node(node: &'a ParsedNode) -> Option<Self> {
        matches!(node.kind(), ParsedNodeKind::GatherPoint | ParsedNodeKind::GatherLabel)
            .then_some(Self { node })
    }

    pub fn identifier(self) -> Option<&'a str> {
        self.node.name()
    }

    pub fn indentation_depth(self) -> usize {
        self.node.indentation_depth
    }

    pub fn content(self) -> &'a [ParsedNode] {
        self.node.children()
    }

    pub fn is_label(self) -> bool {
        self.node.kind() == ParsedNodeKind::GatherLabel
    }

    pub fn collect_named_label(self, names: &mut HashSet<String>) -> Result<(), CompilerError> {
        if let Some(name) = self.identifier()
            && !names.insert(name.to_owned())
        {
            return Err(CompilerError::invalid_source(format!(
                "A label with the same name '{}' already exists in this scope",
                name
            )));
        }

        Ok(())
    }

    pub fn validate_scope_label(self, names: &mut HashSet<String>) -> Result<(), CompilerError> {
        if let Some(name) = self.identifier()
            && !names.insert(name.to_owned())
        {
            return Err(CompilerError::invalid_source(format!(
                "A gather label with the same name '{}' already exists in this scope",
                name
            )));
        }

        Ok(())
    }
}

impl ParsedNode {
    pub fn as_gather(&self) -> Option<GatherNode<'_>> {
        GatherNode::from_node(self)
    }
}
