//! Read-only view of a versioned story image.
//!
//! The image owns no runtime nodes: every lookup reads its fields from the
//! caller's static byte slice. See `docs/flash-story-image-format.md`.

#[cfg(feature = "std")]
mod encoder;
#[cfg(feature = "std")]
pub use encoder::compile_json_to_image;

#[allow(unused_imports)]
use crate::prelude::*;
use crate::{
    control_command::ControlCommand,
    flat_story::{
        ContainerId, ListView, NamedChildView, NodeId, NodeKindView, NodeView, PathView,
        StaticStoryView, ValueView,
    },
    ink_list_item::InkListItem,
    native_function_call::NativeFunctionCall,
    push_pop::PushPopType,
    story::{INK_VERSION_CURRENT, INK_VERSION_MINIMUM_COMPATIBLE},
    story_error::StoryError,
};

/// Version of the on-flash record layout.
pub const FORMAT_VERSION: u32 = 2;
/// Image signature.
pub const MAGIC: &[u8; 8] = b"BLINKIMG";
const NONE: u32 = u32::MAX;
const HEADER_SIZE: usize = 88;
const NODE_WORDS: usize = 10;
const SECTION_SIZES: [usize; 8] = [40, 4, 12, 16, 12, 12, 1, 1];

fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in bytes[..84].iter().chain(bytes[88..].iter()) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[derive(Clone, Copy)]
struct Section {
    offset: usize,
    count: usize,
}

/// Validated view over bytes embedded in flash. Construction uses temporary
/// scratch data to verify graph references, then retains only section bounds.
pub(crate) struct ImageView {
    bytes: &'static [u8],
    sections: [Section; 8],
    pub(crate) ink_version: i32,
}

fn invalid(reason: &str) -> StoryError {
    StoryError::BadImage(reason.to_owned())
}

fn word(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

impl ImageView {
    pub(crate) fn new(bytes: &'static [u8]) -> Result<Self, StoryError> {
        if bytes.len() < HEADER_SIZE || bytes.get(..8) != Some(MAGIC.as_slice()) {
            return Err(invalid("invalid or truncated image header"));
        }
        if word(bytes, 8) != Some(FORMAT_VERSION) {
            return Err(invalid("unsupported image format version"));
        }
        let ink_version = word(bytes, 12).ok_or_else(|| invalid("missing Ink version"))? as i32;
        if !(INK_VERSION_MINIMUM_COMPATIBLE..=INK_VERSION_CURRENT).contains(&ink_version) {
            return Err(invalid("unsupported Ink version"));
        }
        if word(bytes, 16).map(|value| value as usize) != Some(bytes.len()) {
            return Err(invalid("image length is invalid"));
        }
        if word(bytes, 84) != Some(checksum(bytes)) {
            return Err(invalid("image checksum does not match"));
        }
        let mut sections = [Section {
            offset: 0,
            count: 0,
        }; 8];
        let mut expected = HEADER_SIZE;
        for (index, section) in sections.iter_mut().enumerate() {
            let position = 20 + index * 8;
            let offset =
                word(bytes, position).ok_or_else(|| invalid("missing section offset"))? as usize;
            let count = word(bytes, position + 4)
                .ok_or_else(|| invalid("missing section length"))? as usize;
            let length = count
                .checked_mul(SECTION_SIZES[index])
                .ok_or_else(|| invalid("section length overflow"))?;
            if offset != expected {
                return Err(invalid("section offsets are not contiguous"));
            }
            expected = expected
                .checked_add(length)
                .ok_or_else(|| invalid("section end overflow"))?;
            if expected > bytes.len() {
                return Err(invalid("section extends past image end"));
            }
            *section = Section { offset, count };
        }
        if expected != bytes.len() || sections[0].count == 0 {
            return Err(invalid("image has trailing bytes or no root node"));
        }
        let view = Self {
            bytes,
            sections,
            ink_version,
        };
        view.validate()?;
        Ok(view)
    }

    fn section(&self, index: usize) -> &'static [u8] {
        let section = self.sections[index];
        let end = section.offset + section.count * SECTION_SIZES[index];
        &self.bytes[section.offset..end]
    }

    fn record(&self, id: NodeId) -> Option<[u32; NODE_WORDS]> {
        let mut values = [0; NODE_WORDS];
        let offset = id.index().checked_mul(NODE_WORDS * 4)?;
        let bytes = self.section(0);
        for (index, value) in values.iter_mut().enumerate() {
            *value = word(bytes, offset + index * 4)?;
        }
        Some(values)
    }

    pub(crate) fn node_count(&self) -> usize {
        self.sections[0].count
    }

    fn str_ref(&self, offset: u32, len: u32) -> Option<&'static str> {
        if offset == NONE || len == NONE {
            return None;
        }
        let bytes = self.section(7);
        let start = offset as usize;
        let end = start.checked_add(len as usize)?;
        core::str::from_utf8(bytes.get(start..end)?).ok()
    }

    fn optional_str(&self, offset: u32, len: u32) -> Result<Option<&'static str>, StoryError> {
        if offset == NONE && len == NONE {
            return Ok(None);
        }
        self.str_ref(offset, len)
            .map(Some)
            .ok_or_else(|| invalid("invalid UTF-8 string reference"))
    }

    fn require_str(&self, offset: u32, len: u32) -> Result<&'static str, StoryError> {
        self.str_ref(offset, len)
            .ok_or_else(|| invalid("missing or invalid UTF-8 string"))
    }

    fn range(&self, section: usize, start: u32, count: u32) -> Result<(), StoryError> {
        let end = (start as usize)
            .checked_add(count as usize)
            .ok_or_else(|| invalid("index range overflow"))?;
        if end > self.sections[section].count {
            return Err(invalid("index range extends past section"));
        }
        Ok(())
    }

    fn valid_node(&self, id: u32) -> bool {
        (id as usize) < self.node_count()
    }

    fn require_node(&self, id: u32) -> Result<(), StoryError> {
        if self.valid_node(id) {
            Ok(())
        } else {
            Err(invalid("node ID is out of range"))
        }
    }

    fn optional_node(&self, id: u32) -> Result<(), StoryError> {
        if id == NONE {
            Ok(())
        } else {
            self.require_node(id)
        }
    }

    fn require_container(&self, id: u32) -> Result<(), StoryError> {
        self.require_node(id)?;
        if self.record(NodeId(id)).map(|record| record[2]) == Some(1) {
            Ok(())
        } else {
            Err(invalid("target is not a container"))
        }
    }

    fn optional_container(&self, id: u32) -> Result<(), StoryError> {
        if id == NONE {
            Ok(())
        } else {
            self.require_container(id)
        }
    }

    fn validate(&self) -> Result<(), StoryError> {
        let root = self
            .record(NodeId(0))
            .ok_or_else(|| invalid("missing root node"))?;
        if root[0] != NONE || root[1] != NONE || root[2] != 1 {
            return Err(invalid("root must be a parentless container"));
        }
        let mut ordered_end = 0_usize;
        let mut named_end = 0_usize;
        let mut payload_end = 0_usize;
        let mut container_count = 0_usize;
        for index in 0..self.node_count() {
            let record = self
                .record(NodeId(index as u32))
                .ok_or_else(|| invalid("truncated node record"))?;
            let fields = &record[3..];
            if index != 0 {
                self.require_container(record[0])?;
                if record[0] as usize == index {
                    return Err(invalid("node cannot be its own parent"));
                }
            }
            match record[2] {
                1 => {
                    container_count += 1;
                    self.optional_str(fields[0], fields[1])?;
                    self.range(1, fields[3], fields[4])?;
                    self.range(2, fields[5], fields[6])?;
                    if fields[3] as usize != ordered_end || fields[5] as usize != named_end {
                        return Err(invalid("container child ranges are not contiguous"));
                    }
                    ordered_end += fields[4] as usize;
                    named_end += fields[6] as usize;
                }
                2 => {
                    self.optional_str(fields[0], fields[1])?;
                    self.require_container(fields[2])?;
                }
                3 => {
                    let name = self.require_str(fields[0], fields[1])?;
                    if ControlCommand::new_from_name(name).is_none() {
                        return Err(invalid("unknown Ink command"));
                    }
                }
                6 => {
                    let name = self.require_str(fields[0], fields[1])?;
                    if NativeFunctionCall::new_from_name(name).is_none() {
                        return Err(invalid("unknown native operation"));
                    }
                }
                7 | 11 | 14 | 16 => {
                    self.require_str(fields[0], fields[1])?;
                    if record[2] == 16 && fields[2] & !3 != 0 {
                        return Err(invalid("invalid assignment flags"));
                    }
                }
                4 => {
                    self.optional_str(fields[0], fields[1])?;
                    self.optional_node(fields[2])?;
                    self.optional_str(fields[3], fields[4])?;
                    if fields[6] & !0x1f != 0 || (fields[6] >> 3) > 2 {
                        return Err(invalid("invalid divert flags"));
                    }
                }
                5 | 18 => {}
                8 if fields[0] <= 1 => {}
                9 | 10 => {}
                12 => {
                    if fields[0] as usize != payload_end {
                        return Err(invalid("list payloads are not contiguous"));
                    }
                    self.validate_list(fields[0], fields[1])?;
                    payload_end += fields[1] as usize;
                }
                13 => {
                    self.require_str(fields[0], fields[1])?;
                    self.require_node(fields[2])?;
                }
                17 => {
                    self.require_str(fields[0], fields[1])?;
                    self.optional_str(fields[2], fields[3])?;
                    self.optional_container(fields[4])?;
                }
                _ => return Err(invalid("unknown or malformed node tag")),
            }
        }
        if ordered_end != self.sections[1].count
            || named_end != self.sections[2].count
            || payload_end != self.sections[6].count
            || container_count != self.sections[5].count
        {
            return Err(invalid("image sections have unreferenced entries"));
        }
        self.validate_relationships()?;
        self.validate_indexes()?;
        Ok(())
    }

    fn validate_list(&self, offset: u32, len: u32) -> Result<(), StoryError> {
        let start = offset as usize;
        let end = start
            .checked_add(len as usize)
            .ok_or_else(|| invalid("list payload overflow"))?;
        let payload = self
            .section(6)
            .get(start..end)
            .ok_or_else(|| invalid("list payload is out of range"))?;
        let items = word(payload, 0).ok_or_else(|| invalid("list payload is truncated"))? as usize;
        let origins =
            word(payload, 4).ok_or_else(|| invalid("list payload is truncated"))? as usize;
        let expected = items
            .checked_mul(20)
            .and_then(|size| {
                origins
                    .checked_mul(8)
                    .and_then(|tail| size.checked_add(tail))
            })
            .and_then(|size| size.checked_add(8))
            .ok_or_else(|| invalid("list payload length overflow"))?;
        if expected != payload.len() {
            return Err(invalid("list payload length is invalid"));
        }
        for index in 0..items {
            let at = 8 + index * 20;
            self.optional_str(word(payload, at).unwrap(), word(payload, at + 4).unwrap())?;
            self.require_str(
                word(payload, at + 8).unwrap(),
                word(payload, at + 12).unwrap(),
            )?;
        }
        for index in 0..origins {
            let at = 8 + items * 20 + index * 8;
            self.require_str(word(payload, at).unwrap(), word(payload, at + 4).unwrap())?;
        }
        Ok(())
    }

    fn validate_relationships(&self) -> Result<(), StoryError> {
        let mut seen = vec![false; self.node_count()];
        let mut pending = vec![NodeId(0)];
        while let Some(id) = pending.pop() {
            if seen[id.index()] {
                continue;
            }
            seen[id.index()] = true;
            let record = self.record(id).unwrap();
            if record[2] != 1 {
                continue;
            }
            let fields = &record[3..];
            for index in 0..fields[4] {
                let child = self
                    .child_at(id, index as usize)
                    .ok_or_else(|| invalid("invalid child ID"))?;
                let child_record = self
                    .record(child)
                    .ok_or_else(|| invalid("invalid child ID"))?;
                if child_record[0] != id.0 || child_record[1] != index {
                    return Err(invalid("ordered child has inconsistent parent or index"));
                }
                pending.push(child);
            }
            let mut previous = None;
            for index in 0..fields[6] {
                let (name, child) = self
                    .named_at(id, index as usize)
                    .ok_or_else(|| invalid("invalid named child"))?;
                if previous.is_some_and(|last: &str| last >= name) {
                    return Err(invalid("named children are not sorted"));
                }
                previous = Some(name);
                self.require_container(child.0)?;
                if self.record(child).unwrap()[0] != id.0 {
                    return Err(invalid("named child has inconsistent parent"));
                }
                pending.push(child);
            }
        }
        if seen.iter().any(|visited| !visited) {
            return Err(invalid("image contains unreachable nodes"));
        }
        Ok(())
    }

    fn validate_indexes(&self) -> Result<(), StoryError> {
        for section in [2, 5] {
            let mut previous = None;
            for index in 0..self.sections[section].count {
                let bytes = self.section(section);
                let at = index * 12;
                let name =
                    self.require_str(word(bytes, at).unwrap(), word(bytes, at + 4).unwrap())?;
                let target = word(bytes, at + 8).unwrap();
                self.require_container(target)?;
                if section == 5 && previous.is_some_and(|last: &str| last >= name) {
                    return Err(invalid("path index is not sorted"));
                }
                if section == 5 && self.canonical_path_text(NodeId(target)).as_deref() != Some(name)
                {
                    return Err(invalid("path index disagrees with node ancestry"));
                }
                previous = Some(name);
            }
        }
        let mut previous = None;
        let mut item_end = 0_usize;
        for index in 0..self.sections[3].count {
            let bytes = self.section(3);
            let at = index * 16;
            let name = self.require_str(word(bytes, at).unwrap(), word(bytes, at + 4).unwrap())?;
            if previous.is_some_and(|last: &str| last >= name) {
                return Err(invalid("list definitions are not sorted"));
            }
            previous = Some(name);
            let start = word(bytes, at + 8).unwrap();
            let count = word(bytes, at + 12).unwrap();
            self.range(4, start, count)?;
            if start as usize != item_end {
                return Err(invalid("list item ranges are not contiguous"));
            }
            item_end += count as usize;
            let mut previous_item = None;
            for item in start as usize..item_end {
                let (name, _) = self
                    .definition_item(item)
                    .ok_or_else(|| invalid("invalid list-definition item"))?;
                if previous_item.is_some_and(|last: &str| last >= name) {
                    return Err(invalid("list-definition items are not sorted"));
                }
                previous_item = Some(name);
            }
        }
        if item_end != self.sections[4].count {
            return Err(invalid(
                "list-definition items contain unreferenced entries",
            ));
        }
        for index in 0..self.sections[4].count {
            let bytes = self.section(4);
            let at = index * 12;
            self.require_str(word(bytes, at).unwrap(), word(bytes, at + 4).unwrap())?;
        }
        Ok(())
    }

    pub(crate) fn child_at(&self, id: NodeId, index: usize) -> Option<NodeId> {
        let record = self.record(id)?;
        if record[2] != 1 || index >= record[7] as usize {
            return None;
        }
        let position = (record[6] as usize).checked_add(index)?.checked_mul(4)?;
        Some(NodeId(word(self.section(1), position)?))
    }

    pub(crate) fn named_at(&self, id: NodeId, index: usize) -> Option<(&'static str, NodeId)> {
        let record = self.record(id)?;
        if record[2] != 1 || index >= record[9] as usize {
            return None;
        }
        let position = (record[8] as usize).checked_add(index)?.checked_mul(12)?;
        let bytes = self.section(2);
        let name = self.str_ref(word(bytes, position)?, word(bytes, position + 4)?)?;
        Some((name, NodeId(word(bytes, position + 8)?)))
    }

    fn definition(&self, index: usize) -> Option<(&'static str, usize, usize)> {
        let bytes = self.section(3);
        let at = index.checked_mul(16)?;
        let name = self.str_ref(word(bytes, at)?, word(bytes, at + 4)?)?;
        Some((
            name,
            word(bytes, at + 8)? as usize,
            word(bytes, at + 12)? as usize,
        ))
    }

    fn definition_item(&self, index: usize) -> Option<(&'static str, i32)> {
        let bytes = self.section(4);
        let at = index.checked_mul(12)?;
        let name = self.str_ref(word(bytes, at)?, word(bytes, at + 4)?)?;
        Some((name, word(bytes, at + 8)? as i32))
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGE: &[u8] = include_bytes!("../../tests/fixtures/image_smoke.inkb");

    fn refreshed(mut bytes: Vec<u8>) -> &'static [u8] {
        let crc = checksum(&bytes);
        bytes[84..88].copy_from_slice(&crc.to_le_bytes());
        Box::leak(bytes.into_boxed_slice())
    }

    #[test]
    fn validates_embedded_image_and_rejects_bad_references_with_valid_crc() {
        assert!(ImageView::new(IMAGE).is_ok());
        let mut bad = IMAGE.to_vec();
        let first_child = word(&bad, 28).unwrap() as usize;
        bad[first_child..first_child + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(ImageView::new(refreshed(bad)).is_err());

        let mut bad = IMAGE.to_vec();
        let node = word(&bad, 20).unwrap() as usize;
        bad[node + 8..node + 12].copy_from_slice(&99_u32.to_le_bytes());
        assert!(ImageView::new(refreshed(bad)).is_err());
    }

    #[test]
    fn malformed_images_never_panic_during_validation() {
        let mut state = 0x1234_5678_u32;
        for _ in 0..512 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let mut bad = IMAGE.to_vec();
            let index = state as usize % bad.len();
            if (84..88).contains(&index) {
                continue;
            }
            bad[index] ^= 1 << ((state >> 16) % 8);
            let bytes = refreshed(bad);
            assert!(
                std::panic::catch_unwind(|| ImageView::new(bytes)).is_ok(),
                "byte {index}"
            );
        }
    }
}
