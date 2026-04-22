use std::{collections::HashMap, rc::Rc};

use bladeink::{CommandType, Path, RTObject, VariableReference as RuntimeVariableReference};

use crate::{
    error::CompilerError,
    runtime_export::{
        Scope, command, export_divert_arguments, export_divert_by_kind, export_divert_conditional,
        resolve_target, resolve_variable_divert_name, rt_value,
    },
};

use super::{
    DivertTarget, FunctionCall, ParsedExpression, ParsedNode, ParsedNodeKind, Story,
    ValidationScope,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivertNodeKind {
    Normal,
    Tunnel,
    TunnelOnwards,
    Thread,
}

impl DivertNodeKind {
    fn parsed_kind(self) -> ParsedNodeKind {
        match self {
            Self::Normal => ParsedNodeKind::Divert,
            Self::Tunnel => ParsedNodeKind::TunnelDivert,
            Self::TunnelOnwards => ParsedNodeKind::TunnelOnwardsWithTarget,
            Self::Thread => ParsedNodeKind::ThreadDivert,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DivertNode<'a> {
    node: &'a ParsedNode,
    kind: DivertNodeKind,
}

impl<'a> DivertNode<'a> {
    pub fn from_node(node: &'a ParsedNode) -> Option<Self> {
        let kind = match node.kind() {
            ParsedNodeKind::Divert => DivertNodeKind::Normal,
            ParsedNodeKind::TunnelDivert => DivertNodeKind::Tunnel,
            ParsedNodeKind::TunnelOnwardsWithTarget => DivertNodeKind::TunnelOnwards,
            ParsedNodeKind::ThreadDivert => DivertNodeKind::Thread,
            _ => return None,
        };
        Some(Self { node, kind })
    }

    pub fn kind(self) -> DivertNodeKind {
        self.kind
    }

    pub fn target(self) -> Option<&'a str> {
        self.node.target()
    }

    pub fn arguments(self) -> &'a [ParsedExpression] {
        self.node.arguments()
    }

    pub fn condition(self) -> Option<&'a ParsedExpression> {
        self.node.condition()
    }

    pub(crate) fn validate(self, scope: &ValidationScope, story: &Story) -> Result<(), CompilerError> {
        if let Some(target) = self.target() {
            DivertTarget::validate_target_name(target, scope, story)?;
            FunctionCall::validate_call_arguments(target, self.arguments(), scope, story)?;
        }

        Ok(())
    }

    pub(crate) fn export_runtime(
        self,
        state: &crate::runtime_export::ExportState,
        scope: Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        content: &mut Vec<Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError> {
        let target = self.target().ok_or_else(|| {
            CompilerError::unsupported_feature("runtime export divert missing target")
        })?;

        match self.kind() {
            DivertNodeKind::Normal => {
                if let Some(condition) = self.condition() {
                    content.push(command(CommandType::EvalStart));
                    crate::runtime_export::export_expression(condition, story, content)?;
                    content.push(command(CommandType::EvalEnd));
                    content.push(export_divert_conditional(
                        state,
                        target,
                        self.node.resolved_target(),
                        scope,
                        story,
                        named_paths,
                    )?);
                } else {
                    export_divert_by_kind(
                        state,
                        target,
                        self.node.resolved_target(),
                        self.arguments(),
                        scope,
                        story,
                        named_paths,
                        crate::runtime_export::DivertKind::Normal,
                        content,
                    )?;
                }
            }
            DivertNodeKind::Tunnel => {
                export_divert_by_kind(
                    state,
                    target,
                    self.node.resolved_target(),
                    self.arguments(),
                    scope,
                    story,
                    named_paths,
                    crate::runtime_export::DivertKind::Tunnel,
                    content,
                )?;
            }
            DivertNodeKind::Thread => {
                export_divert_by_kind(
                    state,
                    target,
                    self.node.resolved_target(),
                    self.arguments(),
                    scope,
                    story,
                    named_paths,
                    crate::runtime_export::DivertKind::Thread,
                    content,
                )?;
            }
            DivertNodeKind::TunnelOnwards => {
                let variable_target = resolve_variable_divert_name(target, scope, story, named_paths);
                content.push(command(CommandType::EvalStart));
                export_divert_arguments(
                    self.arguments(),
                    target,
                    scope,
                    story,
                    named_paths,
                    content,
                )?;
                if let Some(variable_target) = variable_target {
                    content.push(Rc::new(RuntimeVariableReference::new(&variable_target)));
                } else {
                    let resolved = resolve_target(target, scope, story, named_paths);
                    content.push(rt_value(Path::new_with_components_string(Some(&resolved))));
                }
                content.push(command(CommandType::EvalEnd));
                content.push(command(CommandType::PopTunnel));
            }
        }

        Ok(())
    }
}

impl ParsedNode {
    pub fn new_divert(
        kind: DivertNodeKind,
        target: impl Into<String>,
        arguments: Vec<ParsedExpression>,
    ) -> Self {
        Self::new(kind.parsed_kind())
            .with_target(target)
            .with_arguments(arguments)
    }

    pub fn as_divert(&self) -> Option<DivertNode<'_>> {
        DivertNode::from_node(self)
    }
}
