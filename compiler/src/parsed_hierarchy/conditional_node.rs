use crate::error::CompilerError;
use std::{collections::HashMap, rc::Rc};

use bladeink::{
    CommandType, Container, Divert, NativeOp, PushPopType, RTObject,
};

use crate::runtime_export::{
    ExportState, PathFixupSource, Scope, command, export_condition_expression_runtime,
    export_nodes_with_paths, export_weave, has_weave_content, native, rt_value,
    unwrap_weave_root_container,
};

use super::{ParsedExpression, ParsedNode, ParsedNodeKind, Story, ValidationScope};

#[derive(Debug, Clone)]
pub struct ConditionalBranchSpec {
    pub condition: Option<ParsedExpression>,
    pub content: Vec<ParsedNode>,
    pub is_else: bool,
    pub is_inline: bool,
    pub is_true_branch: bool,
    pub matching_equality: bool,
}

impl ConditionalBranchSpec {
    pub fn from_nodes(nodes: Vec<ParsedNode>) -> Self {
        Self {
            condition: None,
            content: nodes,
            is_else: false,
            is_inline: true,
            is_true_branch: false,
            matching_equality: false,
        }
    }

    fn build(self) -> ParsedNode {
        let mut node = ParsedNode::new(ParsedNodeKind::Conditional);
        node.is_inline = self.is_inline;
        node.is_else = self.is_else;
        node.is_true_branch = self.is_true_branch;
        node.matching_equality = self.matching_equality;
        if let Some(condition) = self.condition {
            node = node.with_condition(condition);
        }
        node.set_children(self.content);
        node
    }
}

#[derive(Debug, Clone)]
pub struct ConditionalNodeSpec {
    pub initial_condition: Option<ParsedExpression>,
    pub branches: Vec<ConditionalBranchSpec>,
}

impl ConditionalNodeSpec {
    pub fn build(self) -> ParsedNode {
        let mut saw_branch_condition = false;
        let mut children = Vec::new();

        for (idx, branch) in self.branches.into_iter().enumerate() {
            let mut branch_node = branch.build();

            if self.initial_condition.is_some() {
                if branch_node.condition().is_some() {
                    branch_node.matching_equality = true;
                    saw_branch_condition = true;
                } else if saw_branch_condition && branch_node.is_else {
                    branch_node.matching_equality = true;
                } else if idx == 0 {
                    branch_node.is_true_branch = true;
                } else {
                    branch_node.is_else = true;
                }
            } else if branch_node.is_else {
                branch_node.is_else = true;
            }

            children.push(branch_node);
        }

        let mut node = ParsedNode::new(if self.initial_condition.is_some() {
            ParsedNodeKind::SwitchConditional
        } else {
            ParsedNodeKind::Conditional
        });
        if let Some(condition) = self.initial_condition {
            node = node.with_condition(condition);
        }
        node.set_children(children);
        node
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConditionalNode<'a> {
    node: &'a ParsedNode,
}

impl<'a> ConditionalNode<'a> {
    pub fn from_node(node: &'a ParsedNode) -> Option<Self> {
        matches!(node.kind(), ParsedNodeKind::Conditional | ParsedNodeKind::SwitchConditional)
            .then_some(Self { node })
    }

    pub fn is_switch(self) -> bool {
        self.node.kind() == ParsedNodeKind::SwitchConditional
    }

    pub fn initial_condition(self) -> Option<&'a ParsedExpression> {
        self.node.condition()
    }

    pub fn branches(self) -> impl Iterator<Item = ConditionalBranchNode<'a>> {
        self.node.children().iter().filter_map(ConditionalBranchNode::from_node)
    }

    pub(crate) fn validate(self, scope: &ValidationScope, story: &Story) -> Result<(), CompilerError> {
        for branch in self.branches() {
            ParsedNode::validate_list(branch.content(), scope, story)?;
        }

        Ok(())
    }

    pub fn requires_weave_context(self) -> bool {
        self.node.contains_choice_content() && !self.branches().any(ConditionalBranchNode::is_else)
    }

    pub(crate) fn append_simple_runtime(
        self,
        state: &ExportState,
        scope: Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        parent_container_path: Option<&str>,
        content_index_offset: usize,
        content: &mut Vec<Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError> {
        let empty_named_paths = HashMap::new();
        let named_paths = named_paths.unwrap_or(&empty_named_paths);
        let is_switch = self.is_switch();

        if let Some(initial) = self.initial_condition() {
            content.push(command(CommandType::EvalStart));
            export_condition_expression_runtime(initial, scope, story, named_paths, content)?;
            content.push(command(CommandType::EvalEnd));
        }

        let needs_final_pop = is_switch
            && self.branches().next().is_some_and(|branch| branch.condition().is_some())
            && !self.branches().last().is_some_and(ConditionalBranchNode::is_else);
        let rejoin_nop = command(CommandType::NoOp);

        for branch in self.branches() {
            let duplicates_stack_value = branch.matching_equality() && !branch.is_else();
            let mut branch_control: Vec<Rc<dyn RTObject>> = Vec::new();
            let mut branch_nodes = branch.content().to_vec();
            if !branch.is_inline() && has_weave_content(branch.content()) {
                branch_nodes.insert(0, ParsedNode::new(ParsedNodeKind::Newline));
            }
            if duplicates_stack_value {
                branch_control.push(command(CommandType::Duplicate));
            }
            if !branch.is_true_branch() && !branch.is_else() {
                if branch.condition().is_some() {
                    branch_control.push(command(CommandType::EvalStart));
                }
                if let Some(condition) = branch.condition() {
                    export_condition_expression_runtime(
                        condition,
                        scope,
                        story,
                        named_paths,
                        &mut branch_control,
                    )?;
                }
                if branch.matching_equality() {
                    branch_control.push(native(NativeOp::Equal));
                }
                if branch.condition().is_some() {
                    branch_control.push(command(CommandType::EvalEnd));
                }
            }
            let branch_divert = Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                !branch.is_else(),
                None,
                None,
            ));
            branch_control.push(branch_divert.clone());

            let (mut branch_content, branch_named) = if has_weave_content(branch.content()) {
                let branch_container_index = content_index_offset + content.len();
                let branch_path = parent_container_path
                    .map(|path| format!("{path}.{branch_container_index}.b"))
                    .unwrap_or_else(|| "b".to_owned());
                let weave = unwrap_weave_root_container(&export_weave(
                    state,
                    &branch_path,
                    &branch_nodes,
                    scope,
                    story,
                    false,
                    named_paths,
                )?);
                (weave.content.clone(), weave.named_content.clone())
            } else {
                (
                    export_nodes_with_paths(
                        state,
                        branch.content(),
                        scope,
                        story,
                        Some(named_paths),
                        None,
                        0,
                    )?,
                    HashMap::new(),
                )
            };
            if !branch.is_inline() && !has_weave_content(branch.content()) {
                branch_content.insert(0, rt_value("\n"));
            }
            if duplicates_stack_value || (branch.is_else() && branch.matching_equality()) {
                branch_content.insert(0, command(CommandType::PopEvaluatedValue));
            }
            let return_divert = Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                false,
                None,
                None,
            ));
            state.add_runtime_target_fixup(
                PathFixupSource::Divert(return_divert.clone()),
                rejoin_nop.clone(),
            );
            branch_content.push(return_divert);

            let branch_b = Container::new(Some("b".to_owned()), 0, branch_content, branch_named);
            state.add_runtime_target_fixup(
                PathFixupSource::Divert(branch_divert),
                branch_b.clone(),
            );
            let mut branch_named = HashMap::new();
            branch_named.insert("b".to_owned(), branch_b);
            content.push(Container::new(None, 0, branch_control, branch_named) as Rc<dyn RTObject>);
        }

        if needs_final_pop {
            content.push(command(CommandType::PopEvaluatedValue));
        }
        content.push(rejoin_nop);
        Ok(())
    }

    pub(crate) fn export_runtime(
        self,
        state: &ExportState,
        scope: Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        container_path: Option<&str>,
    ) -> Result<Rc<Container>, CompilerError> {
        let mut content: Vec<Rc<dyn RTObject>> = Vec::new();
        let is_switch = self.is_switch();
        let empty_named_paths = HashMap::new();
        let named_paths = named_paths.unwrap_or(&empty_named_paths);

        if let Some(initial) = self.initial_condition() {
            content.push(command(CommandType::EvalStart));
            export_condition_expression_runtime(initial, scope, story, named_paths, &mut content)?;
            content.push(command(CommandType::EvalEnd));
        }

        let branch_container_base_index = content.len();
        let mut branch_specs: Vec<(
            bool,
            bool,
            Vec<Rc<dyn RTObject>>,
            HashMap<String, Rc<Container>>,
            i32,
        )> = Vec::new();

        for (idx, branch) in self.branches().enumerate() {
            let duplicates_stack_value = branch.matching_equality() && !branch.is_else();
            let mut branch_nodes = branch.content().to_vec();
            if !branch.is_inline() {
                branch_nodes.insert(0, ParsedNode::new(ParsedNodeKind::Newline));
            }

            let branch_b_path =
                container_path.map(|path| format!("{path}.{}.b", branch_container_base_index + idx));

            let (mut branch_content, branch_named, branch_flags) = if has_weave_content(&branch_nodes) {
                let branch_path_prefix = branch_b_path.clone().unwrap_or_else(|| {
                    if branch
                        .content()
                        .iter()
                        .any(|node| node.kind() == ParsedNodeKind::Choice && !node.start_content().is_empty())
                    {
                        ".^.^".to_owned()
                    } else {
                        ".^".to_owned()
                    }
                });
                let weave = unwrap_weave_root_container(&export_weave(
                    state,
                    &branch_path_prefix,
                    &branch_nodes,
                    scope,
                    story,
                    false,
                    named_paths,
                )?);
                (weave.content.clone(), weave.named_content.clone(), weave.get_count_flags())
            } else {
                (
                    export_nodes_with_paths(
                        state,
                        &branch_nodes,
                        scope,
                        story,
                        Some(named_paths),
                        branch_b_path.as_deref(),
                        0,
                    )?,
                    HashMap::new(),
                    0,
                )
            };
            if duplicates_stack_value || (branch.is_else() && branch.matching_equality()) {
                branch_content.insert(0, command(CommandType::PopEvaluatedValue));
            }

            branch_specs.push((
                !branch.is_else(),
                branch.matching_equality() && !branch.is_else(),
                branch_content,
                branch_named,
                branch_flags,
            ));
        }

        let needs_final_pop = is_switch
            && self.branches().next().is_some_and(|branch| branch.condition().is_some())
            && !self.branches().last().is_some_and(ConditionalBranchNode::is_else);
        let rejoin_nop = command(CommandType::NoOp);

        for (idx, branch) in self.branches().enumerate() {
            let (is_conditional, duplicates_stack_value, mut branch_content, branch_named, branch_flags) =
                branch_specs[idx].clone();

            let mut branch_control: Vec<Rc<dyn RTObject>> = Vec::new();
            if duplicates_stack_value {
                branch_control.push(command(CommandType::Duplicate));
            }

            if !branch.is_true_branch() && !branch.is_else() {
                if branch.condition().is_some() {
                    branch_control.push(command(CommandType::EvalStart));
                }
                if let Some(condition) = branch.condition() {
                    export_condition_expression_runtime(
                        condition,
                        scope,
                        story,
                        named_paths,
                        &mut branch_control,
                    )?;
                }
                if branch.matching_equality() {
                    branch_control.push(native(NativeOp::Equal));
                }
                if branch.condition().is_some() {
                    branch_control.push(command(CommandType::EvalEnd));
                }
            }

            let branch_divert = Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                is_conditional,
                None,
                None,
            ));
            branch_control.push(branch_divert.clone());

            let return_divert = Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                false,
                None,
                None,
            ));
            state.add_runtime_target_fixup(
                PathFixupSource::Divert(return_divert.clone()),
                rejoin_nop.clone(),
            );
            branch_content.push(return_divert);

            let branch_b = Container::new(Some("b".to_owned()), branch_flags, branch_content, branch_named);
            state.add_runtime_target_fixup(
                PathFixupSource::Divert(branch_divert),
                branch_b.clone(),
            );
            let mut branch_named_content = HashMap::new();
            branch_named_content.insert("b".to_owned(), branch_b);
            let branch_container = Container::new(None, 0, branch_control, branch_named_content);
            content.push(branch_container as Rc<dyn RTObject>);
        }

        if needs_final_pop {
            content.push(command(CommandType::PopEvaluatedValue));
        }

        content.push(rejoin_nop);
        let container = Container::new(None, 0, content, HashMap::new());
        self.node.object().set_runtime_object(container.clone());
        self.node.object().set_container_for_counting(container.clone());
        Ok(container)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConditionalBranchNode<'a> {
    node: &'a ParsedNode,
}

impl<'a> ConditionalBranchNode<'a> {
    pub fn from_node(node: &'a ParsedNode) -> Option<Self> {
        (node.kind() == ParsedNodeKind::Conditional).then_some(Self { node })
    }

    pub fn condition(self) -> Option<&'a ParsedExpression> {
        self.node.condition()
    }

    pub fn content(self) -> &'a [ParsedNode] {
        self.node.children()
    }

    pub fn is_else(self) -> bool {
        self.node.is_else
    }

    pub fn is_inline(self) -> bool {
        self.node.is_inline
    }

    pub fn is_true_branch(self) -> bool {
        self.node.is_true_branch
    }

    pub fn matching_equality(self) -> bool {
        self.node.matching_equality
    }
}

impl ParsedNode {
    pub fn as_conditional(&self) -> Option<ConditionalNode<'_>> {
        ConditionalNode::from_node(self)
    }
}
