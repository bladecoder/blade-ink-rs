use crate::error::CompilerError;
use std::{collections::HashMap, rc::Rc};

use bladeink::{ChoicePoint, CommandType, Divert, Path, PushPopType};

use crate::runtime_export::{
    ExportState, PathFixupSource, PendingContainer, Scope, command, export_condition_expression,
    export_nodes, rt_value, variable_assignment as runtime_variable_assignment, variable_divert,
};
use std::collections::HashSet;

use super::{ParsedExpression, ParsedNode, ParsedNodeKind, Story};

#[derive(Debug, Clone)]
pub struct ChoiceNodeSpec {
    pub(crate) source_node: Option<ParsedNode>,
    pub indentation_depth: usize,
    pub once_only: bool,
    pub identifier: Option<String>,
    pub condition: Option<ParsedExpression>,
    pub start_content: Vec<ParsedNode>,
    pub choice_only_content: Vec<ParsedNode>,
    pub inner_content: Vec<ParsedNode>,
    pub is_invisible_default: bool,
}

impl ChoiceNodeSpec {
    pub fn from_node(node: &ParsedNode) -> Option<Self> {
        let choice = ChoiceNode::from_node(node)?;
        Some(Self {
            source_node: Some(node.clone()),
            indentation_depth: choice.indentation_depth(),
            once_only: choice.once_only(),
            identifier: choice.identifier().map(ToOwned::to_owned),
            condition: choice.condition().cloned(),
            start_content: choice.start_content().to_vec(),
            choice_only_content: choice.choice_only_content().to_vec(),
            inner_content: choice.inner_content().to_vec(),
            is_invisible_default: choice.is_invisible_default(),
        })
    }

    pub fn build(self) -> ParsedNode {
        let mut node = ParsedNode::new(ParsedNodeKind::Choice);
        node.indentation_depth = self.indentation_depth;
        node.once_only = self.once_only;
        node.is_invisible_default = self.is_invisible_default;
        node.start_content = self.start_content;
        node.choice_only_content = self.choice_only_content;
        if let Some(condition) = self.condition {
            node = node.with_condition(condition);
        }
        if let Some(identifier) = self.identifier {
            node = node.with_name(identifier);
        }
        if !self.inner_content.is_empty() {
            node = node.with_children(self.inner_content);
        }
        node
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub fn indentation_depth(&self) -> usize {
        self.indentation_depth
    }

    pub fn once_only(&self) -> bool {
        self.once_only
    }

    pub fn is_invisible_default(&self) -> bool {
        self.is_invisible_default
    }

    pub fn condition(&self) -> Option<&ParsedExpression> {
        self.condition.as_ref()
    }

    pub fn start_content(&self) -> &[ParsedNode] {
        &self.start_content
    }

    pub fn choice_only_content(&self) -> &[ParsedNode] {
        &self.choice_only_content
    }

    pub fn inner_content(&self) -> &[ParsedNode] {
        &self.inner_content
    }

    pub(crate) fn export_runtime(
        &self,
        state: &ExportState,
        choice_index: usize,
        path_prefix: &str,
        current: Rc<PendingContainer>,
        choice_container: Rc<PendingContainer>,
        scope: Scope<'_>,
        story: &Story,
        named_paths: &HashMap<String, String>,
    ) -> Result<(), CompilerError> {
        let choice_key = format!("c-{choice_index}");
        let relative_start_choice = !self.start_content().is_empty() && current.path.starts_with('.');
        let has_start = !self.start_content().is_empty();
        let has_choice_only = !self.choice_only_content().is_empty();
        let has_condition = self.condition().is_some();
        let flags = (has_condition as i32)
            + ((has_start as i32) * 2)
            + ((has_choice_only as i32) * 4)
            + ((self.is_invisible_default() as i32) * 8)
            + ((self.once_only() as i32) * 16);

        if has_start {
            let sub_index = current.next_content_index();
            let (outer_path, r1_path, r2_path) = if relative_start_choice {
                let outer = format!(".^.^.{sub_index}");
                (
                    outer.clone(),
                    format!("{}.{}.$r1", current.path, sub_index),
                    format!("{}.$r2", choice_container.path),
                )
            } else {
                let outer = format!("{}.{}", current.path, sub_index);
                (
                    outer.clone(),
                    format!("{outer}.$r1"),
                    format!("{}.{}.$r2", path_prefix, choice_key),
                )
            };
            let outer = PendingContainer::new(state, &outer_path, None, 0);
            if let Some(source_node) = &self.source_node {
                state.add_parsed_runtime_fixup(
                    source_node.object().runtime_cache_handle(),
                    outer.id(),
                    crate::runtime_export::ParsedRuntimeFixupFlags {
                        runtime_object: true,
                        runtime_path_target: false,
                        container_for_counting: false,
                    },
                );
                state.add_parsed_runtime_fixup(
                    source_node.object().runtime_cache_handle(),
                    choice_container.id(),
                    crate::runtime_export::ParsedRuntimeFixupFlags {
                        runtime_object: false,
                        runtime_path_target: true,
                        container_for_counting: true,
                    },
                );
            }
            outer.push_object(command(CommandType::EvalStart));
            outer.push_object(rt_value(Path::new_with_components_string(Some(&r1_path))));
            outer.push_object(runtime_variable_assignment("$r", false, true));
            outer.push_object(command(CommandType::BeginString));
            let divert_to_start_outer = Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                false,
                None,
                None,
            ));
            outer.push_object(divert_to_start_outer.clone());
            outer.push_container(PendingContainer::new(state, r1_path.clone(), Some("$r1".to_owned()), 0));
            outer.push_object(command(CommandType::EndString));

            if has_choice_only {
                outer.push_object(command(CommandType::BeginString));
                for item in export_nodes(state, self.choice_only_content(), scope, story)? {
                    outer.push_object(item);
                }
                outer.push_object(command(CommandType::EndString));
            }

            if let Some(condition) = self.condition() {
                export_condition_expression(condition, story, named_paths, &mut outer.content.borrow_mut())?;
            }

            outer.push_object(command(CommandType::EvalEnd));
            let choice_point = Rc::new(ChoicePoint::new(flags, ""));
            state.add_pending_target_fixup(
                PathFixupSource::ChoicePoint(choice_point.clone()),
                choice_container.id(),
            );
            outer.push_object(choice_point);

            let start_container =
                PendingContainer::new(state, format!("{outer_path}.s"), Some("s".to_owned()), 0);
            state.add_pending_target_fixup(
                PathFixupSource::Divert(divert_to_start_outer),
                start_container.id(),
            );
            for item in export_nodes(state, self.start_content(), scope, story)? {
                start_container.push_object(item);
            }
            start_container.push_object(variable_divert("$r"));
            outer.add_named("s", start_container.clone());

            current.push_container(outer);

            choice_container.push_object(command(CommandType::EvalStart));
            choice_container.push_object(rt_value(Path::new_with_components_string(Some(&r2_path))));
            choice_container.push_object(command(CommandType::EvalEnd));
            choice_container.push_object(runtime_variable_assignment("$r", false, true));
            let divert_to_start_inner = Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                false,
                None,
                None,
            ));
            state.add_pending_target_fixup(
                PathFixupSource::Divert(divert_to_start_inner.clone()),
                start_container.id(),
            );
            choice_container.push_object(divert_to_start_inner);
            choice_container.push_container(PendingContainer::new(
                state,
                r2_path,
                Some("$r2".to_owned()),
                0,
            ));
        } else {
            if let Some(source_node) = &self.source_node {
                state.add_parsed_runtime_fixup(
                    source_node.object().runtime_cache_handle(),
                    choice_container.id(),
                    crate::runtime_export::ParsedRuntimeFixupFlags {
                        runtime_object: true,
                        runtime_path_target: true,
                        container_for_counting: true,
                    },
                );
            }
            current.push_object(command(CommandType::EvalStart));
            if has_choice_only {
                current.push_object(command(CommandType::BeginString));
                for item in export_nodes(state, self.choice_only_content(), scope, story)? {
                    current.push_object(item);
                }
                current.push_object(command(CommandType::EndString));
            }
            if let Some(condition) = self.condition() {
                export_condition_expression(condition, story, named_paths, &mut current.content.borrow_mut())?;
            }
            current.push_object(command(CommandType::EvalEnd));
            let choice_point = Rc::new(ChoicePoint::new(flags, ""));
            state.add_pending_target_fixup(
                PathFixupSource::ChoicePoint(choice_point.clone()),
                choice_container.id(),
            );
            current.push_object(choice_point);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChoiceNode<'a> {
    node: &'a ParsedNode,
}

impl<'a> ChoiceNode<'a> {
    pub fn from_node(node: &'a ParsedNode) -> Option<Self> {
        (node.kind() == ParsedNodeKind::Choice).then_some(Self { node })
    }

    pub fn identifier(self) -> Option<&'a str> {
        self.node.name()
    }

    pub fn indentation_depth(self) -> usize {
        self.node.indentation_depth
    }

    pub fn once_only(self) -> bool {
        self.node.once_only
    }

    pub fn is_invisible_default(self) -> bool {
        self.node.is_invisible_default
    }

    pub fn condition(self) -> Option<&'a ParsedExpression> {
        self.node.condition()
    }

    pub fn start_content(self) -> &'a [ParsedNode] {
        &self.node.start_content
    }

    pub fn choice_only_content(self) -> &'a [ParsedNode] {
        &self.node.choice_only_content
    }

    pub fn inner_content(self) -> &'a [ParsedNode] {
        self.node.children()
    }

    pub fn has_weave_style_inline_brackets(self) -> bool {
        !self.choice_only_content().is_empty()
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
}

impl ParsedNode {
    pub fn as_choice(&self) -> Option<ChoiceNode<'_>> {
        ChoiceNode::from_node(self)
    }
}
