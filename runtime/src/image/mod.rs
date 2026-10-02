//! Read-only view of a versioned story image.
//!
//! The image owns no runtime nodes: every lookup reads its fields from the
//! caller's static byte slice. See `docs/flash-story-image.md`.

#[cfg(feature = "std")]
mod encoder;
#[cfg(feature = "std")]
pub use encoder::compile_json_to_image;

#[allow(unused_imports)]
use crate::prelude::*;
use crate::{
    control_command::ControlCommand,
    ink_list_item::InkListItem,
    native_function_call::NativeFunctionCall,
    push_pop::PushPopType,
    story::error::StoryError,
    story::{INK_VERSION_CURRENT, INK_VERSION_MINIMUM_COMPATIBLE},
    story_content::{
        ContainerId, ListView, NamedChildView, NodeId, NodeKindView, NodeView, PathView,
        StaticStoryView, ValueView,
    },
};

mod validate;
mod view;

/// Version of the on-flash record layout.
pub const FORMAT_VERSION: u32 = 2;
/// Image signature.
pub const MAGIC: &[u8; 8] = b"BLINKIMG";
const NONE: u32 = u32::MAX;
const HEADER_SIZE: usize = 88;
const NODE_WORDS: usize = 10;
const SECTION_SIZES: [usize; 8] = [40, 4, 12, 16, 12, 12, 1, 1];
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0; 256];
    let mut index = 0;
    while index < table.len() {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
};

fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in bytes[..84].iter().chain(bytes[88..].iter()) {
        crc = (crc >> 8) ^ CRC32_TABLE[((crc as u8) ^ byte) as usize];
    }
    !crc
}

#[derive(Clone, Copy)]
struct Section {
    offset: usize,
    count: usize,
}

/// View over bytes embedded in flash. Only the explicit validated constructor
/// checks the full graph; both constructors retain just the section bounds.
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
        Ok(Self {
            bytes,
            sections,
            ink_version,
        })
    }

    pub(crate) fn new_validated(bytes: &'static [u8]) -> Result<Self, StoryError> {
        let view = Self::new(bytes)?;
        if word(bytes, 84) != Some(checksum(bytes)) {
            return Err(invalid("image checksum does not match"));
        }
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
        assert!(ImageView::new_validated(IMAGE).is_ok());
        let mut bad = IMAGE.to_vec();
        let first_child = word(&bad, 28).unwrap() as usize;
        bad[first_child..first_child + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(ImageView::new_validated(refreshed(bad)).is_err());

        let mut bad = IMAGE.to_vec();
        let node = word(&bad, 20).unwrap() as usize;
        bad[node + 8..node + 12].copy_from_slice(&99_u32.to_le_bytes());
        assert!(ImageView::new_validated(refreshed(bad)).is_err());
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
                std::panic::catch_unwind(|| ImageView::new_validated(bytes)).is_ok(),
                "byte {index}"
            );
        }
    }
}
