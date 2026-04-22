use std::{collections::HashMap, rc::Rc};

use bladeink::{
    CommandType, Container, Divert, NativeFunctionCall, NativeOp, PushPopType, RTObject,
};

use crate::{
    error::CompilerError,
    runtime_export::{
        ExportState, PathFixupSource, Scope, command, export_nodes_with_paths, export_weave,
        has_weave_content, native, rt_int, unwrap_weave_root_container,
    },
};

use super::{ParsedNode, ParsedNodeKind, Story};

#[derive(Debug, Clone)]
pub struct SequenceNodeSpec {
    pub sequence_type: u8,
    pub elements: Vec<Vec<ParsedNode>>,
}

impl SequenceNodeSpec {
    pub fn build(self) -> ParsedNode {
        let element_children = self
            .elements
            .into_iter()
            .map(|nodes| ParsedNode::new(ParsedNodeKind::Text).with_text("").with_children(nodes))
            .collect();
        let mut node = ParsedNode::new(ParsedNodeKind::Sequence);
        node.sequence_type = self.sequence_type;
        node.set_children(element_children);
        node
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SequenceNode<'a> {
    node: &'a ParsedNode,
}

impl<'a> SequenceNode<'a> {
    pub fn from_node(node: &'a ParsedNode) -> Option<Self> {
        (node.kind() == ParsedNodeKind::Sequence).then_some(Self { node })
    }

    pub fn sequence_type(self) -> u8 {
        self.node.sequence_type
    }

    pub fn elements(self) -> &'a [ParsedNode] {
        self.node.children()
    }

    pub(crate) fn export_runtime(
        self,
        state: &ExportState,
        scope: Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        container_path: Option<&str>,
    ) -> Result<Rc<dyn RTObject>, CompilerError> {
        use super::SequenceType;

        let seq_type = self.sequence_type();
        let once = (seq_type & SequenceType::Once as u8) != 0;
        let cycle = (seq_type & SequenceType::Cycle as u8) != 0;
        let stopping = (seq_type & SequenceType::Stopping as u8) != 0;
        let shuffle = (seq_type & SequenceType::Shuffle as u8) != 0;
        let stopping = stopping || (!once && !cycle && !shuffle);

        let elements = self.elements();
        let num_elements = elements.len();
        let empty_named_paths = HashMap::new();
        let named_paths = named_paths.unwrap_or(&empty_named_paths);
        let seq_branch_count = if once { num_elements + 1 } else { num_elements };

        let mut seq_items: Vec<Rc<dyn RTObject>> = Vec::new();
        seq_items.push(command(CommandType::EvalStart));
        seq_items.push(command(CommandType::VisitIndex));

        if stopping || once {
            seq_items.push(rt_int(seq_branch_count as i32 - 1));
            seq_items.push(Rc::new(NativeFunctionCall::new(NativeOp::Min)));
        } else if cycle {
            seq_items.push(rt_int(num_elements as i32));
            seq_items.push(Rc::new(NativeFunctionCall::new(NativeOp::Mod)));
        }

        let mut post_shuffle_no_op: Option<Rc<dyn RTObject>> = None;
        if shuffle {
            if once || stopping {
                let last_idx = if stopping {
                    num_elements as i32 - 1
                } else {
                    num_elements as i32
                };
                let skip_shuffle_divert = Rc::new(Divert::new(
                    false,
                    PushPopType::Tunnel,
                    false,
                    0,
                    true,
                    None,
                    None,
                ));
                let post_shuffle_nop = command(CommandType::NoOp);
                state.add_runtime_target_fixup(
                    PathFixupSource::Divert(skip_shuffle_divert.clone()),
                    post_shuffle_nop.clone(),
                );
                seq_items.push(command(CommandType::Duplicate));
                seq_items.push(rt_int(last_idx));
                seq_items.push(native(NativeOp::Equal));
                seq_items.push(skip_shuffle_divert);
                post_shuffle_no_op = Some(post_shuffle_nop);
            }

            let element_count_to_shuffle = if stopping {
                num_elements.saturating_sub(1)
            } else {
                num_elements
            };
            seq_items.push(rt_int(element_count_to_shuffle as i32));
            seq_items.push(command(CommandType::SequenceShuffleIndex));
            if let Some(post_shuffle_nop) = post_shuffle_no_op.clone() {
                seq_items.push(post_shuffle_nop);
            }
        }

        seq_items.push(command(CommandType::EvalEnd));

        let mut branch_diverts = Vec::new();
        for el_index in 0..seq_branch_count {
            seq_items.push(command(CommandType::EvalStart));
            seq_items.push(command(CommandType::Duplicate));
            seq_items.push(rt_int(el_index as i32));
            seq_items.push(Rc::new(NativeFunctionCall::new(NativeOp::Equal)));
            seq_items.push(command(CommandType::EvalEnd));

            let branch_divert = Rc::new(Divert::new(
                false,
                PushPopType::Function,
                false,
                0,
                true,
                None,
                None,
            ));
            branch_diverts.push(branch_divert.clone());
            seq_items.push(branch_divert);
        }

        let post_sequence_nop = command(CommandType::NoOp);
        seq_items.push(post_sequence_nop.clone());

        let mut named_branches: HashMap<String, Rc<Container>> = HashMap::new();
        for el_index in 0..seq_branch_count {
            let (branch_content, branch_named, branch_flags) = if el_index < num_elements {
                let element_nodes = elements[el_index].children();
                if has_weave_content(element_nodes) {
                    let branch_path_prefix = container_path
                        .map(|path| format!("{path}.s{el_index}"))
                        .unwrap_or_else(|| ".^".to_owned());
                    let weave = unwrap_weave_root_container(&export_weave(
                        state,
                        &branch_path_prefix,
                        element_nodes,
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
                            element_nodes,
                            scope,
                            story,
                            Some(named_paths),
                            container_path.map(|path| format!("{path}.s{el_index}")).as_deref(),
                            0,
                        )?,
                        HashMap::new(),
                        0,
                    )
                }
            } else {
                (Vec::new(), HashMap::new(), 0)
            };

            let back_divert = Rc::new(Divert::new(
                false,
                PushPopType::Function,
                false,
                0,
                false,
                None,
                None,
            ));
            state.add_runtime_target_fixup(
                PathFixupSource::Divert(back_divert.clone()),
                post_sequence_nop.clone(),
            );

            let mut branch_items: Vec<Rc<dyn RTObject>> = Vec::new();
            branch_items.push(command(CommandType::PopEvaluatedValue));
            branch_items.extend(branch_content);
            branch_items.push(back_divert);

            let branch_name = format!("s{}", el_index);
            let branch_container =
                Container::new(Some(branch_name.clone()), branch_flags, branch_items, branch_named);
            state.add_runtime_target_fixup(
                PathFixupSource::Divert(branch_diverts[el_index].clone()),
                branch_container.clone(),
            );
            named_branches.insert(branch_name, branch_container);
        }

        Ok(Container::new(None, 5, seq_items, named_branches))
    }
}

impl ParsedNode {
    pub fn as_sequence(&self) -> Option<SequenceNode<'_>> {
        SequenceNode::from_node(self)
    }
}
