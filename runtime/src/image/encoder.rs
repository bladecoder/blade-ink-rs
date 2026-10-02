//! Host-side encoding of the immutable Ink story arena.

use super::{FORMAT_VERSION, HEADER_SIZE, MAGIC, NODE_WORDS, NONE, checksum};

use std::collections::BTreeMap;

use crate::{
    control_command::ControlCommand,
    io::Read,
    native_function_call::NativeFunctionCall,
    push_pop::PushPopType,
    story::error::StoryError,
    story_content::{
        ContainerId, NodeId, NodeKind, StaticList, StaticPath, StaticValue, StoryData,
    },
};

/// Compile an already compiled Ink JSON document into a flash story image.
/// The result contains no pointers or Rust struct layouts.
pub fn compile_json_to_image(reader: impl Read) -> Result<Vec<u8>, StoryError> {
    let (ink_version, story) = StoryData::from_json_reader(reader)?;
    let story = canonicalize(story)?;
    Encoder::new().encode(ink_version, &story)
}

/// JSON object property order can change the IDs assigned to named-only
/// containers. Reorder those IDs before encoding so both JSON readers and
/// different property orders produce the same image.
fn canonicalize(mut story: StoryData) -> Result<StoryData, StoryError> {
    let mut order = Vec::with_capacity(story.nodes.len());
    let mut seen = vec![false; story.nodes.len()];
    let mut pending = vec![NodeId(0)];
    while let Some(id) = pending.pop() {
        let index = id.index();
        if index >= seen.len() || seen[index] {
            continue;
        }
        seen[index] = true;
        order.push(id);
        if let NodeKind::Container(container) = &story.nodes[index].kind {
            for child in story.named[container.named.clone()].iter().rev() {
                pending.push(child.node);
            }
            for child in story.children[container.children.clone()].iter().rev() {
                pending.push(*child);
            }
        }
    }
    if order.len() != story.nodes.len() {
        return Err(StoryError::BadJson(
            "story contains unreachable nodes".to_owned(),
        ));
    }

    let mut new_id = vec![NodeId(0); order.len()];
    for (index, old) in order.iter().enumerate() {
        new_id[old.index()] = NodeId(count(index)?);
    }
    let mut old_nodes: Vec<_> = std::mem::take(&mut story.nodes)
        .into_iter()
        .map(Some)
        .collect();
    let old_children = std::mem::take(&mut story.children);
    let old_named = std::mem::take(&mut story.named);
    for old in order {
        let mut node = old_nodes[old.index()].take().unwrap();
        node.parent = node.parent.map(|id| new_id[id.index()]);
        match &mut node.kind {
            NodeKind::Container(container) => {
                let start = story.children.len();
                for id in &old_children[container.children.clone()] {
                    story.children.push(new_id[id.index()]);
                }
                container.children = start..story.children.len();
                let start = story.named.len();
                for entry in &old_named[container.named.clone()] {
                    story.named.push(crate::story_content::NamedChild {
                        name: entry.name.clone(),
                        node: new_id[entry.node.index()],
                    });
                }
                container.named = start..story.named.len();
            }
            NodeKind::ChoicePoint(choice) => {
                choice.target = choice
                    .target
                    .map(|id| ContainerId(new_id[id.node().index()]));
            }
            NodeKind::Divert(divert) => {
                divert.target = divert.target.map(|id| new_id[id.index()]);
            }
            NodeKind::Value(StaticValue::DivertTarget { target, .. }) => {
                *target = target.map(|id| new_id[id.index()]);
            }
            NodeKind::VariableReference(reference) => {
                reference.count_target = reference
                    .count_target
                    .map(|id| ContainerId(new_id[id.node().index()]));
            }
            _ => {}
        }
        story.nodes.push(node);
    }
    Ok(story)
}

struct Encoder {
    strings: Vec<u8>,
    string_offsets: BTreeMap<String, (u32, u32)>,
    payload: Vec<u8>,
}

impl Encoder {
    fn new() -> Self {
        Self {
            strings: Vec::new(),
            string_offsets: BTreeMap::new(),
            payload: Vec::new(),
        }
    }

    fn encode(mut self, ink_version: i32, story: &StoryData) -> Result<Vec<u8>, StoryError> {
        let mut nodes = Vec::with_capacity(story.nodes.len() * NODE_WORDS * 4);
        for node in &story.nodes {
            let mut fields = [NONE; 7];
            let tag = self.encode_kind(&node.kind, &mut fields)?;
            word(&mut nodes, opt_id(node.parent));
            word(&mut nodes, node.child_index.unwrap_or(NONE));
            word(&mut nodes, tag);
            for field in fields {
                word(&mut nodes, field);
            }
        }

        let mut children = Vec::with_capacity(story.children.len() * 4);
        for child in &story.children {
            word(&mut children, child.0);
        }

        let mut named = Vec::with_capacity(story.named.len() * 12);
        for child in &story.named {
            let (offset, len) = self.intern(&child.name)?;
            word(&mut named, offset);
            word(&mut named, len);
            word(&mut named, child.node.0);
        }

        let mut definitions = Vec::with_capacity(story.list_definitions.len() * 16);
        let mut definition_items = Vec::new();
        for definition in &story.list_definitions {
            let (offset, len) = self.intern(&definition.name)?;
            word(&mut definitions, offset);
            word(&mut definitions, len);
            word(&mut definitions, count(definition_items.len() / 12)?);
            word(&mut definitions, count(definition.items.len())?);
            for (name, value) in &definition.items {
                let (offset, len) = self.intern(name)?;
                word(&mut definition_items, offset);
                word(&mut definition_items, len);
                word(&mut definition_items, *value as u32);
            }
        }

        // Save-state paths always name containers; indexed child pointers add
        // their final numeric component. Sorting makes lookup deterministic.
        let mut paths = BTreeMap::new();
        for (index, node) in story.nodes.iter().enumerate() {
            if matches!(node.kind, NodeKind::Container(_)) {
                let id = NodeId(count(index)?);
                let path = story.canonical_path_text(id).ok_or_else(|| {
                    StoryError::BadJson("container path could not be encoded".to_owned())
                })?;
                if paths.insert(path, id).is_some() {
                    return Err(StoryError::BadJson(
                        "multiple containers have the same canonical path".to_owned(),
                    ));
                }
            }
        }
        let mut path_index = Vec::with_capacity(paths.len() * 12);
        for (path, id) in paths {
            let (offset, len) = self.intern(&path)?;
            word(&mut path_index, offset);
            word(&mut path_index, len);
            word(&mut path_index, id.0);
        }

        let mut image =
            Vec::with_capacity(HEADER_SIZE + nodes.len() + children.len() + named.len());
        image.extend_from_slice(MAGIC);
        word(&mut image, FORMAT_VERSION);
        word(&mut image, ink_version as u32);
        word(&mut image, 0); // total length, filled after appending sections
        section(&mut image, HEADER_SIZE, story.nodes.len())?;
        section(&mut image, HEADER_SIZE + nodes.len(), story.children.len())?;
        section(
            &mut image,
            HEADER_SIZE + nodes.len() + children.len(),
            story.named.len(),
        )?;
        let mut next = HEADER_SIZE + nodes.len() + children.len() + named.len();
        section(&mut image, next, story.list_definitions.len())?;
        next += definitions.len();
        section(&mut image, next, definition_items.len() / 12)?;
        next += definition_items.len();
        section(&mut image, next, path_index.len() / 12)?;
        next += path_index.len();
        section(&mut image, next, self.payload.len())?;
        next += self.payload.len();
        section(&mut image, next, self.strings.len())?;
        word(&mut image, 0); // checksum, filled after appending sections
        debug_assert_eq!(image.len(), HEADER_SIZE);
        image.extend_from_slice(&nodes);
        image.extend_from_slice(&children);
        image.extend_from_slice(&named);
        image.extend_from_slice(&definitions);
        image.extend_from_slice(&definition_items);
        image.extend_from_slice(&path_index);
        image.extend_from_slice(&self.payload);
        image.extend_from_slice(&self.strings);
        let total = count(image.len())?;
        image[16..20].copy_from_slice(&total.to_le_bytes());
        let crc = checksum(&image);
        image[84..88].copy_from_slice(&crc.to_le_bytes());
        Ok(image)
    }

    fn encode_kind(&mut self, kind: &NodeKind, fields: &mut [u32; 7]) -> Result<u32, StoryError> {
        let tag = match kind {
            NodeKind::Container(container) => {
                self.optional_str(container.name.as_deref(), fields, 0)?;
                fields[2] = container.count_flags as u32;
                fields[3] = count(container.children.start)?;
                fields[4] = count(container.children.len())?;
                fields[5] = count(container.named.start)?;
                fields[6] = count(container.named.len())?;
                1
            }
            NodeKind::ChoicePoint(choice) => {
                self.optional_path(choice.path.as_ref(), fields, 0)?;
                fields[2] = choice.target.map_or(NONE, |target| target.node().0);
                fields[3] = choice.flags as u32;
                2
            }
            NodeKind::ControlCommand(command) => {
                self.required_str(&ControlCommand::get_name(*command), fields, 0)?;
                3
            }
            NodeKind::Divert(divert) => {
                self.optional_path(divert.path.as_ref(), fields, 0)?;
                fields[2] = opt_id(divert.target);
                self.optional_str(divert.variable_name.as_deref(), fields, 3)?;
                fields[5] = count(divert.external_args)?;
                fields[6] = u32::from(divert.conditional)
                    | (u32::from(divert.external) << 1)
                    | (u32::from(divert.pushes_to_stack) << 2)
                    | (push_type(divert.stack_push_type) << 3);
                4
            }
            NodeKind::Glue => 5,
            NodeKind::NativeFunction(op) => {
                self.required_str(&NativeFunctionCall::get_name(*op), fields, 0)?;
                6
            }
            NodeKind::Tag(text) => {
                self.required_str(text, fields, 0)?;
                7
            }
            NodeKind::Value(value) => self.encode_value(value, fields)?,
            NodeKind::VariableAssignment {
                name,
                global,
                new_declaration,
            } => {
                self.required_str(name, fields, 0)?;
                fields[2] = u32::from(*global) | (u32::from(*new_declaration) << 1);
                16
            }
            NodeKind::VariableReference(reference) => {
                self.required_str(&reference.name, fields, 0)?;
                self.optional_path(reference.count_path.as_ref(), fields, 2)?;
                fields[4] = reference
                    .count_target
                    .map_or(NONE, |target| target.node().0);
                17
            }
            NodeKind::Void => 18,
        };
        Ok(tag)
    }

    fn encode_value(
        &mut self,
        value: &StaticValue,
        fields: &mut [u32; 7],
    ) -> Result<u32, StoryError> {
        let tag = match value {
            StaticValue::Bool(value) => {
                fields[0] = u32::from(*value);
                8
            }
            StaticValue::Int(value) => {
                fields[0] = *value as u32;
                9
            }
            StaticValue::Float(value) => {
                fields[0] = value.to_bits();
                10
            }
            StaticValue::String(value) => {
                self.required_str(value, fields, 0)?;
                11
            }
            StaticValue::List(list) => {
                self.encode_list(list, fields)?;
                12
            }
            StaticValue::DivertTarget { path, target } => {
                self.required_path(path, fields, 0)?;
                fields[2] = opt_id(*target);
                13
            }
            StaticValue::VariablePointer {
                name,
                context_index,
            } => {
                self.required_str(name, fields, 0)?;
                fields[2] = *context_index as u32;
                14
            }
        };
        Ok(tag)
    }

    fn encode_list(&mut self, list: &StaticList, fields: &mut [u32; 7]) -> Result<(), StoryError> {
        let mut bytes = Vec::new();
        word(&mut bytes, count(list.items.len())?);
        word(&mut bytes, count(list.origin_names.len())?);
        for (item, value) in &list.items {
            let (origin, origin_len) =
                self.optional_intern(item.get_origin_name().map(String::as_str))?;
            let (name, name_len) = self.intern(item.get_item_name())?;
            word(&mut bytes, origin);
            word(&mut bytes, origin_len);
            word(&mut bytes, name);
            word(&mut bytes, name_len);
            word(&mut bytes, *value as u32);
        }
        for origin in &list.origin_names {
            let (offset, len) = self.intern(origin)?;
            word(&mut bytes, offset);
            word(&mut bytes, len);
        }
        fields[0] = count(self.payload.len())?;
        fields[1] = count(bytes.len())?;
        self.payload.extend_from_slice(&bytes);
        Ok(())
    }

    fn required_path(
        &mut self,
        path: &StaticPath,
        fields: &mut [u32; 7],
        at: usize,
    ) -> Result<(), StoryError> {
        let mut text = String::new();
        if path.relative {
            text.push('.');
        }
        for (index, component) in path.components.iter().enumerate() {
            if index != 0 {
                text.push('.');
            }
            text.push_str(&component.to_string());
        }
        self.required_str(&text, fields, at)
    }

    fn optional_path(
        &mut self,
        path: Option<&StaticPath>,
        fields: &mut [u32; 7],
        at: usize,
    ) -> Result<(), StoryError> {
        if let Some(path) = path {
            self.required_path(path, fields, at)?;
        }
        Ok(())
    }

    fn required_str(
        &mut self,
        text: &str,
        fields: &mut [u32; 7],
        at: usize,
    ) -> Result<(), StoryError> {
        let (offset, len) = self.intern(text)?;
        fields[at] = offset;
        fields[at + 1] = len;
        Ok(())
    }

    fn optional_str(
        &mut self,
        text: Option<&str>,
        fields: &mut [u32; 7],
        at: usize,
    ) -> Result<(), StoryError> {
        if let Some(text) = text {
            self.required_str(text, fields, at)?;
        }
        Ok(())
    }

    fn optional_intern(&mut self, text: Option<&str>) -> Result<(u32, u32), StoryError> {
        text.map_or(Ok((NONE, NONE)), |text| self.intern(text))
    }

    fn intern(&mut self, text: &str) -> Result<(u32, u32), StoryError> {
        if let Some(&location) = self.string_offsets.get(text) {
            return Ok(location);
        }
        let location = (count(self.strings.len())?, count(text.len())?);
        self.strings.extend_from_slice(text.as_bytes());
        self.string_offsets.insert(text.to_owned(), location);
        Ok(location)
    }
}

fn section(header: &mut Vec<u8>, offset: usize, count_or_len: usize) -> Result<(), StoryError> {
    word(header, count(offset)?);
    word(header, count(count_or_len)?);
    Ok(())
}

fn count(value: usize) -> Result<u32, StoryError> {
    u32::try_from(value)
        .map_err(|_| StoryError::BadJson("story image exceeds the 4 GiB format limit".to_owned()))
}

fn opt_id(id: Option<NodeId>) -> u32 {
    id.map_or(NONE, |id| id.0)
}

fn push_type(kind: PushPopType) -> u32 {
    match kind {
        PushPopType::Tunnel => 0,
        PushPopType::Function => 1,
        PushPopType::FunctionEvaluationFromGame => 2,
    }
}

fn word(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        control_command::CommandType,
        ink_list_item::InkListItem,
        native_function_call::Op,
        story_content::{
            ChoiceRecord, ContainerRecord, DivertRecord, NamedChild, NodeRecord,
            StaticListDefinition, VariableReferenceRecord,
        },
    };

    fn read_word(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn encodes_every_node_variant_with_stable_sections() {
        let kinds = vec![
            NodeKind::ChoicePoint(ChoiceRecord {
                path: None,
                target: None,
                flags: 3,
            }),
            NodeKind::ControlCommand(CommandType::EvalStart),
            NodeKind::Divert(DivertRecord {
                path: Some(StaticPath::from_text(".relative")),
                target: None,
                variable_name: Some("destination".to_owned()),
                external_args: 0,
                conditional: true,
                external: false,
                pushes_to_stack: false,
                stack_push_type: PushPopType::Tunnel,
            }),
            NodeKind::Glue,
            NodeKind::NativeFunction(Op::Add),
            NodeKind::Tag("tag".to_owned()),
            NodeKind::Value(StaticValue::Bool(true)),
            NodeKind::Value(StaticValue::Int(-2)),
            NodeKind::Value(StaticValue::Float(1.5)),
            NodeKind::Value(StaticValue::String("text".to_owned())),
            NodeKind::Value(StaticValue::List(StaticList {
                items: vec![(InkListItem::from_full_name("L.item"), 7)],
                origin_names: vec!["L".to_owned()],
            })),
            NodeKind::Value(StaticValue::DivertTarget {
                path: StaticPath::from_text(".relative"),
                target: Some(NodeId(0)),
            }),
            NodeKind::Value(StaticValue::VariablePointer {
                name: "v".to_owned(),
                context_index: -1,
            }),
            NodeKind::VariableAssignment {
                name: "v".to_owned(),
                global: true,
                new_declaration: false,
            },
            NodeKind::VariableReference(VariableReferenceRecord {
                name: "v".to_owned(),
                count_path: None,
                count_target: None,
            }),
            NodeKind::Void,
        ];
        let mut story = StoryData {
            nodes: vec![NodeRecord {
                parent: None,
                child_index: None,
                kind: NodeKind::Container(ContainerRecord {
                    name: None,
                    count_flags: 0,
                    children: 0..kinds.len(),
                    named: 0..1,
                }),
            }],
            children: Vec::new(),
            named: vec![NamedChild {
                name: "root".to_owned(),
                node: NodeId(0),
            }],
            list_definitions: vec![StaticListDefinition {
                name: "L".to_owned(),
                items: vec![("item".to_owned(), 7)],
            }],
        };
        for (index, kind) in kinds.into_iter().enumerate() {
            let id = NodeId(u32::try_from(index + 1).unwrap());
            story.children.push(id);
            story.nodes.push(NodeRecord {
                parent: Some(NodeId(0)),
                child_index: Some(u32::try_from(index).unwrap()),
                kind,
            });
        }
        let image = Encoder::new().encode(21, &story).unwrap();
        assert_eq!(&image[..8], MAGIC);
        assert_eq!(read_word(&image, 8), FORMAT_VERSION);
        assert_eq!(read_word(&image, 16) as usize, image.len());
        assert_eq!(read_word(&image, 20), HEADER_SIZE as u32);
        assert_eq!(read_word(&image, 24), story.nodes.len() as u32);
        let node_offset = read_word(&image, 20) as usize;
        let tags: Vec<u32> = (0..story.nodes.len())
            .map(|index| read_word(&image, node_offset + index * 40 + 8))
            .collect();
        assert_eq!(
            tags,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18]
        );
        let string_offset = read_word(&image, 76) as usize;
        let strings = &image[string_offset..];
        assert!(
            strings
                .windows(b".relative".len())
                .any(|bytes| bytes == b".relative")
        );
        assert_eq!(read_word(&image, 64), 1); // root path index
        assert_eq!(read_word(&image, 84), checksum(&image));
    }

    #[test]
    fn json_compilation_is_deterministic_and_rejects_bad_input() {
        let json = br#"{"inkVersion":21,"root":["^Hi","done",null],"listDefs":{}}"#;
        let first = compile_json_to_image(json.as_slice()).unwrap();
        assert_eq!(first, compile_json_to_image(json.as_slice()).unwrap());
        assert!(compile_json_to_image(b"not JSON".as_slice()).is_err());
        let unresolved = br#"{"inkVersion":21,"root":[{"->":"missing"},null],"listDefs":{}}"#;
        assert!(compile_json_to_image(unresolved.as_slice()).is_err());
    }

    #[test]
    fn named_container_property_order_does_not_change_image() {
        let first =
            br##"{"inkVersion":21,"root":[{"z":["^Z",null],"a":["^A",null]}],"listDefs":{}}"##;
        let second =
            br##"{"listDefs":{},"root":[{"a":["^A",null],"z":["^Z",null]}],"inkVersion":21}"##;
        assert_eq!(
            compile_json_to_image(first.as_slice()).unwrap(),
            compile_json_to_image(second.as_slice()).unwrap()
        );
    }
}
