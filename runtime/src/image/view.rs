//! Read-only node views over image sections.

use super::*;

impl StaticStoryView for ImageView {
    fn node_count(&self) -> usize {
        self.node_count()
    }

    fn node_view(&self, id: NodeId) -> Option<NodeView<'_>> {
        let record = self.record(id)?;
        let f = &record[3..];
        let optional_id = |id| (id != NONE).then_some(NodeId(id));
        let optional_container = |id| optional_id(id).map(ContainerId);
        let string = |at| self.str_ref(f[at], f[at + 1]);
        let path = |at| string(at).map(PathView::Image);
        let kind = match record[2] {
            1 => NodeKindView::Container {
                count_flags: f[2] as i32,
                name: string(0),
            },
            2 => NodeKindView::ChoicePoint {
                flags: f[3] as i32,
                target: optional_container(f[2]),
            },
            3 => NodeKindView::ControlCommand(
                ControlCommand::new_from_name(string(0)?)?.command_type,
            ),
            4 => NodeKindView::Divert {
                path: path(0),
                target: optional_id(f[2]),
                variable_name: string(3),
                external_args: f[5] as usize,
                conditional: f[6] & 1 != 0,
                external: f[6] & 2 != 0,
                pushes_to_stack: f[6] & 4 != 0,
                stack_push_type: match f[6] >> 3 {
                    0 => PushPopType::Tunnel,
                    1 => PushPopType::Function,
                    2 => PushPopType::FunctionEvaluationFromGame,
                    _ => return None,
                },
            },
            5 => NodeKindView::Glue,
            6 => NodeKindView::NativeFunction(NativeFunctionCall::new_from_name(string(0)?)?.op),
            7 => NodeKindView::Tag(string(0)?),
            8 => NodeKindView::Value(ValueView::Bool(f[0] != 0)),
            9 => NodeKindView::Value(ValueView::Int(f[0] as i32)),
            10 => NodeKindView::Value(ValueView::Float(f32::from_bits(f[0]))),
            11 => NodeKindView::Value(ValueView::String(string(0)?)),
            12 => {
                let start = f[0] as usize;
                let end = start.checked_add(f[1] as usize)?;
                NodeKindView::Value(ValueView::List(ListView::Image {
                    payload: self.section(6).get(start..end)?,
                    strings: self.section(7),
                }))
            }
            13 => NodeKindView::Value(ValueView::DivertTarget(PathView::Image(string(0)?))),
            14 => NodeKindView::Value(ValueView::VariablePointer {
                name: string(0)?,
                context_index: f[2] as i32,
            }),
            16 => NodeKindView::VariableAssignment {
                name: string(0)?,
                global: f[2] & 1 != 0,
                new_declaration: f[2] & 2 != 0,
            },
            17 => NodeKindView::VariableReference {
                name: string(0)?,
                count_target: optional_container(f[4]),
            },
            18 => NodeKindView::Void,
            _ => return None,
        };
        Some(NodeView {
            parent: optional_id(record[0]),
            child_index: (record[1] != NONE).then_some(record[1]),
            kind,
        })
    }

    fn child_count(&self, id: NodeId) -> Option<usize> {
        let record = self.record(id)?;
        (record[2] == 1).then_some(record[7] as usize)
    }

    fn child_at(&self, id: NodeId, index: usize) -> Option<NodeId> {
        ImageView::child_at(self, id, index)
    }

    fn named_child_count(&self, id: NodeId) -> Option<usize> {
        let record = self.record(id)?;
        (record[2] == 1).then_some(record[9] as usize)
    }

    fn named_child_at(&self, id: NodeId, index: usize) -> Option<NamedChildView<'_>> {
        let (name, node) = self.named_at(id, index)?;
        Some(NamedChildView { name, node })
    }

    fn list_item(&self, name: &str) -> Option<(InkListItem, i32)> {
        if name.trim().is_empty() {
            return None;
        }
        let (qualified_origin, item_name) = match name.split_once('.') {
            Some((origin, item)) => (Some(origin), item),
            None => (None, name),
        };
        let mut found = None;
        for index in 0..self.sections[3].count {
            let (origin, start, count) = self.definition(index)?;
            if qualified_origin.is_some_and(|qualified| qualified != origin) {
                continue;
            }
            let mut low = start;
            let mut high = start + count;
            while low < high {
                let middle = low + (high - low) / 2;
                let (candidate, value) = self.definition_item(middle)?;
                match candidate.cmp(item_name) {
                    core::cmp::Ordering::Less => low = middle + 1,
                    core::cmp::Ordering::Greater => high = middle,
                    core::cmp::Ordering::Equal => {
                        found = Some((
                            InkListItem::new(Some(origin.to_owned()), item_name.to_owned()),
                            value,
                        ));
                        break;
                    }
                }
            }
        }
        found
    }

    fn list_definition_count(&self) -> usize {
        self.sections[3].count
    }

    fn list_definition_name(&self, index: usize) -> Option<&str> {
        self.definition(index).map(|(name, _, _)| name)
    }

    fn list_definition_item_count(&self, index: usize) -> Option<usize> {
        self.definition(index).map(|(_, _, count)| count)
    }

    fn list_definition_item_at(&self, index: usize, item: usize) -> Option<(&str, i32)> {
        let (_, start, count) = self.definition(index)?;
        if item >= count {
            return None;
        }
        self.definition_item(start + item)
    }
}
