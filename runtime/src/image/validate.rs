//! Structural and reference validation of binary images.

use super::*;

impl ImageView {
    pub(super) fn validate(&self) -> Result<(), StoryError> {
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
                if section == 5 && !self.matches_canonical_path(NodeId(target), name) {
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

    fn matches_canonical_path(&self, mut id: NodeId, mut path: &str) -> bool {
        while id.0 != 0 {
            let Some(record) = self.record(id) else {
                return false;
            };
            let name = if record[2] == 1 {
                self.str_ref(record[3], record[4])
            } else {
                None
            };
            if let Some(name) = name.filter(|name| !name.is_empty()) {
                let Some(prefix) = path.strip_suffix(name) else {
                    return false;
                };
                path = prefix;
            } else {
                let (prefix, component) = path.rsplit_once('.').unwrap_or(("", path));
                let canonical_number = !component.is_empty()
                    && (component == "0" || !component.starts_with('0'))
                    && component.bytes().all(|byte| byte.is_ascii_digit())
                    && component.parse::<u32>() == Ok(record[1]);
                if !canonical_number {
                    return false;
                }
                id = NodeId(record[0]);
                path = prefix;
                continue;
            }
            id = NodeId(record[0]);
            if id.0 != 0 {
                let Some(prefix) = path.strip_suffix('.') else {
                    return false;
                };
                path = prefix;
            }
        }
        path.is_empty()
    }
}
