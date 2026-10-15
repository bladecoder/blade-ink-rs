//! Streaming JSON construction of the flat static arena.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::json::tokenizer::{JsonTokenizer, JsonValue};
use crate::{
    compat::{collections::HashMap, io::Read},
    control_command::ControlCommand,
    ink_list_item::InkListItem,
    native_function_call::NativeFunctionCall,
    push_pop::PushPopType,
    story::error::StoryError,
    story::{INK_VERSION_CURRENT, INK_VERSION_MINIMUM_COMPATIBLE},
    story_content::{
        ChoiceRecord, ContainerRecord, DivertRecord, LoadObserver, LoadPhase, NamedChild, NodeId,
        NodeKind, NodeRecord, StaticList, StaticListDefinition, StaticPath, StaticValue, StoryData,
        VariableReferenceRecord,
    },
};

fn invalid(message: &str) -> StoryError {
    StoryError::BadJson(message.to_owned())
}

fn path(text: &str) -> StaticPath {
    StaticPath::from_text(text)
}

fn string<'a>(value: &'a JsonValue, field: &str) -> Result<&'a str, StoryError> {
    value
        .as_str()
        .ok_or_else(|| StoryError::BadJson(format!("{field} must be a string")))
}

fn integer(value: &JsonValue, field: &str) -> Result<i32, StoryError> {
    value
        .as_integer()
        .ok_or_else(|| StoryError::BadJson(format!("{field} must be an integer")))
}

fn runtime_object_key(key: &str) -> bool {
    matches!(
        key,
        "^->"
            | "^var"
            | "->"
            | "f()"
            | "->t->"
            | "x()"
            | "*"
            | "VAR?"
            | "CNT?"
            | "VAR="
            | "temp="
            | "#"
            | "list"
    )
}

pub(crate) fn load_from_reader(
    reader: impl Read,
    observer: &mut impl LoadObserver,
) -> Result<(i32, StoryData), StoryError> {
    let mut tok = JsonTokenizer::new(reader);
    tok.expect('{')?;
    let mut version = None;
    let mut root = false;
    let mut data = StoryData {
        nodes: Vec::new(),
        children: Vec::new(),
        named: Vec::new(),
        list_definitions: Vec::new(),
    };
    let mut definitions = false;
    while tok.peek()? != '}' {
        let key = tok.read_obj_key()?;
        match key.as_str() {
            "inkVersion" => {
                if version.is_some() {
                    return Err(invalid("duplicate inkVersion"));
                }
                version = Some(
                    tok.read_number()?
                        .as_integer()
                        .ok_or_else(|| invalid("inkVersion must be an integer"))?,
                );
            }
            "root" => {
                if root {
                    return Err(invalid("duplicate root"));
                }
                root = true;
                let token = tok.read_value()?;
                if token != JsonValue::Array {
                    return Err(invalid("Root node for ink is not a container"));
                }
                let id = read_node(&mut tok, &mut data, token, None, None, None)?;
                debug_assert_eq!(id, data.root());
            }
            "listDefs" => {
                if definitions {
                    return Err(invalid("duplicate listDefs"));
                }
                definitions = true;
                data.list_definitions = read_list_definitions(&mut tok)?;
            }
            _ => tok.skip_value()?,
        }
        if tok.peek()? != '}' {
            tok.expect(',')?;
        }
    }
    tok.expect('}')?;
    tok.expect_eof()?;
    let version = version.ok_or_else(|| invalid("ink version number not found"))?;
    if version > INK_VERSION_CURRENT {
        return Err(invalid(
            "Version of ink used to build story was newer than the current version of the engine",
        ));
    }
    if version < INK_VERSION_MINIMUM_COMPATIBLE {
        return Err(invalid(
            "Version of ink used to build story is too old to be loaded by this version of the engine",
        ));
    }
    if !root {
        return Err(invalid("Root node for ink not found"));
    }
    if !definitions {
        return Err(invalid("List Definitions node for ink not found"));
    }
    observer.record(LoadPhase::StreamParsedAndBuilt);
    data.resolve_static_targets()?;
    observer.record(LoadPhase::TargetsResolved);
    Ok((version, data))
}

fn read_list_definitions<R: Read>(
    tok: &mut JsonTokenizer<R>,
) -> Result<Vec<StaticListDefinition>, StoryError> {
    tok.expect('{')?;
    let mut definitions = Vec::new();
    while tok.peek()? != '}' {
        let name = tok.read_obj_key()?;
        tok.expect('{')?;
        let mut items = Vec::new();
        while tok.peek()? != '}' {
            let item = tok.read_obj_key()?;
            let value = tok
                .read_number()?
                .as_integer()
                .ok_or_else(|| invalid("list item must be an integer"))?;
            items.push((item, value));
            if tok.peek()? != '}' {
                tok.expect(',')?;
            }
        }
        tok.expect('}')?;
        items.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        definitions.push(StaticListDefinition { name, items });
        if tok.peek()? != '}' {
            tok.expect(',')?;
        }
    }
    tok.expect('}')?;
    definitions.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    Ok(definitions)
}

fn read_node<R: Read>(
    tok: &mut JsonTokenizer<R>,
    data: &mut StoryData,
    token: JsonValue,
    name: Option<String>,
    parent: Option<NodeId>,
    child_index: Option<u32>,
) -> Result<NodeId, StoryError> {
    let kind = match token {
        JsonValue::Null => return Err(invalid("null is not a runtime object")),
        JsonValue::Boolean(value) => NodeKind::Value(StaticValue::Bool(value)),
        JsonValue::Number(value) => {
            if let Some(value) = value.as_integer() {
                NodeKind::Value(StaticValue::Int(value))
            } else {
                NodeKind::Value(StaticValue::Float(value.as_float()))
            }
        }
        JsonValue::String(value) => read_string(&value)?,
        JsonValue::Object => {
            let first = tok.read_obj_key()?;
            if !runtime_object_key(&first) {
                return Err(invalid("container terminator outside a container"));
            }
            read_object(tok, first)?
        }
        JsonValue::Array => NodeKind::Container(ContainerRecord {
            name,
            count_flags: 0,
            children: 0..0,
            named: 0..0,
        }),
    };
    let id = NodeId(
        u32::try_from(data.nodes.len()).map_err(|_| invalid("story contains too many nodes"))?,
    );
    let is_container = matches!(kind, NodeKind::Container(_));
    data.nodes.push(NodeRecord {
        parent,
        child_index,
        kind,
    });
    if is_container {
        read_container(tok, data, id)?;
    }
    Ok(id)
}

fn read_container<R: Read>(
    tok: &mut JsonTokenizer<R>,
    data: &mut StoryData,
    id: NodeId,
) -> Result<(), StoryError> {
    let mut children = Vec::new();
    let mut named = HashMap::<String, NodeId>::new();
    let mut flags = 0;
    let mut override_name = None;
    let mut found_terminator = false;
    while tok.peek()? != ']' {
        let token = tok.read_value()?;
        if token == JsonValue::Null {
            found_terminator = true;
        } else if token == JsonValue::Object {
            if tok.peek()? == '}' {
                tok.expect('}')?;
                found_terminator = true;
            } else {
                let key = tok.read_obj_key()?;
                if runtime_object_key(&key) {
                    let index = u32::try_from(children.len())
                        .map_err(|_| invalid("container contains too many children"))?;
                    let kind = read_object(tok, key)?;
                    let child = NodeId(
                        u32::try_from(data.nodes.len())
                            .map_err(|_| invalid("story contains too many nodes"))?,
                    );
                    data.nodes.push(NodeRecord {
                        parent: Some(id),
                        child_index: Some(index),
                        kind,
                    });
                    children.push(child);
                } else {
                    found_terminator = true;
                    read_terminator(
                        tok,
                        data,
                        id,
                        key,
                        &mut named,
                        &mut flags,
                        &mut override_name,
                    )?;
                }
            }
        } else {
            let index = u32::try_from(children.len())
                .map_err(|_| invalid("container contains too many children"))?;
            let child = read_node(tok, data, token, None, Some(id), Some(index))?;
            children.push(child);
            if let NodeKind::Container(record) = &data.nodes[child.index()].kind
                && let Some(name) = record.name.as_ref().filter(|name| !name.is_empty())
            {
                named.insert(name.clone(), child);
            }
        }
        if found_terminator {
            if tok.peek()? != ']' {
                return Err(invalid("container terminator must be last"));
            }
            break;
        }
        if tok.peek()? != ']' {
            tok.expect(',')?;
        }
    }
    if !found_terminator {
        return Err(invalid("container has no terminator"));
    }
    tok.expect(']')?;
    let child_start = data.children.len();
    data.children.extend(children);
    let child_end = data.children.len();
    let mut named: Vec<_> = named
        .into_iter()
        .map(|(name, node)| NamedChild { name, node })
        .collect();
    named.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    let named_start = data.named.len();
    data.named.extend(named);
    let named_end = data.named.len();
    let NodeKind::Container(record) = &mut data.nodes[id.index()].kind else {
        unreachable!()
    };
    record.name = override_name.or_else(|| record.name.take());
    record.count_flags = flags;
    record.children = child_start..child_end;
    record.named = named_start..named_end;
    Ok(())
}

fn read_terminator<R: Read>(
    tok: &mut JsonTokenizer<R>,
    data: &mut StoryData,
    parent: NodeId,
    first_key: String,
    named: &mut HashMap<String, NodeId>,
    flags: &mut i32,
    override_name: &mut Option<String>,
) -> Result<(), StoryError> {
    let mut key = first_key;
    loop {
        let token = tok.read_value()?;
        match key.as_str() {
            "#f" => *flags = integer(&token, "#f")?,
            "#n" => *override_name = Some(string(&token, "#n")?.to_owned()),
            _ => {
                let node_len = data.nodes.len();
                let child_len = data.children.len();
                let named_len = data.named.len();
                let child = read_node(tok, data, token, Some(key.clone()), Some(parent), None)?;
                if data.container_id(child).is_none() {
                    return Err(invalid("named content must be a container"));
                }
                if named.contains_key(&key) {
                    data.nodes.truncate(node_len);
                    data.children.truncate(child_len);
                    data.named.truncate(named_len);
                } else {
                    named.insert(key.clone(), child);
                }
            }
        }
        if tok.peek()? == '}' {
            tok.expect('}')?;
            break;
        }
        tok.expect(',')?;
        key = tok.read_obj_key()?;
    }
    Ok(())
}

fn read_string(value: &str) -> Result<NodeKind, StoryError> {
    if let Some(text) = value.strip_prefix('^') {
        return Ok(NodeKind::Value(StaticValue::String(text.to_owned())));
    }
    if value == "\n" {
        return Ok(NodeKind::Value(StaticValue::String("\n".to_owned())));
    }
    if value == "<>" {
        return Ok(NodeKind::Glue);
    }
    if let Some(command) = ControlCommand::new_from_name(value) {
        return Ok(NodeKind::ControlCommand(command.command_type));
    }
    let op = if value == "L^" { "^" } else { value };
    if let Some(function) = NativeFunctionCall::new_from_name(op) {
        return Ok(NodeKind::NativeFunction(function.op));
    }
    if value == "void" {
        return Ok(NodeKind::Void);
    }
    Err(StoryError::BadJson(format!(
        "unknown runtime string: {value}"
    )))
}

enum Field {
    Scalar(JsonValue),
    List(Vec<(String, i32)>),
    Origins(Vec<String>),
}

fn read_object<R: Read>(
    tok: &mut JsonTokenizer<R>,
    first_key: String,
) -> Result<NodeKind, StoryError> {
    let mut fields = HashMap::<String, Field>::new();
    let mut key = first_key;
    loop {
        let value = tok.read_value()?;
        let field = match (key.as_str(), value) {
            ("list", JsonValue::Object) => {
                let mut items = Vec::new();
                while tok.peek()? != '}' {
                    let item = tok.read_obj_key()?;
                    let value = tok
                        .read_number()?
                        .as_integer()
                        .ok_or_else(|| invalid("list item must be an integer"))?;
                    items.push((item, value));
                    if tok.peek()? != '}' {
                        tok.expect(',')?;
                    }
                }
                tok.expect('}')?;
                Field::List(items)
            }
            ("origins", JsonValue::Array) => {
                let mut origins = Vec::new();
                while tok.peek()? != ']' {
                    origins.push(tok.read_string()?);
                    if tok.peek()? != ']' {
                        tok.expect(',')?;
                    }
                }
                tok.expect(']')?;
                Field::Origins(origins)
            }
            (_, JsonValue::Object | JsonValue::Array) => {
                return Err(invalid("nested runtime object field"));
            }
            (_, value) => Field::Scalar(value),
        };
        fields.insert(key, field);
        if tok.peek()? == '}' {
            tok.expect('}')?;
            break;
        }
        tok.expect(',')?;
        key = tok.read_obj_key()?;
    }
    let scalar = |key: &str| match fields.get(key) {
        Some(Field::Scalar(value)) => Some(value),
        _ => None,
    };
    if let Some(value) = scalar("^->") {
        return Ok(NodeKind::Value(StaticValue::DivertTarget {
            path: path(string(value, "^->")?),
            target: None,
        }));
    }
    if let Some(value) = scalar("^var") {
        return Ok(NodeKind::Value(StaticValue::VariablePointer {
            name: string(value, "^var")?.to_owned(),
            context_index: scalar("ci").map_or(Ok(-1), |value| integer(value, "ci"))?,
        }));
    }
    for (key, pushes_to_stack, stack_push_type, external) in [
        ("->", false, PushPopType::Function, false),
        ("f()", true, PushPopType::Function, false),
        ("->t->", true, PushPopType::Tunnel, false),
        ("x()", false, PushPopType::Function, true),
    ] {
        if let Some(value) = scalar(key) {
            let target = string(value, key)?;
            let variable = fields.contains_key("var");
            let external_args = scalar("exArgs").map_or(Ok(0), |value| integer(value, "exArgs"))?;
            return Ok(NodeKind::Divert(DivertRecord {
                path: (!variable).then(|| path(target)),
                target: None,
                variable_name: variable.then(|| target.to_owned()),
                external_args: usize::try_from(external_args)
                    .map_err(|_| invalid("exArgs cannot be negative"))?,
                conditional: fields.contains_key("c"),
                external,
                pushes_to_stack,
                stack_push_type,
            }));
        }
    }
    if let Some(value) = scalar("*") {
        return Ok(NodeKind::ChoicePoint(ChoiceRecord {
            path: Some(path(string(value, "*")?)),
            target: None,
            flags: scalar("flg").map_or(Ok(0), |value| integer(value, "flg"))?,
        }));
    }
    if let Some(value) = scalar("VAR?") {
        return Ok(NodeKind::VariableReference(VariableReferenceRecord {
            name: string(value, "VAR?")?.to_owned(),
            count_path: None,
            count_target: None,
        }));
    }
    if let Some(value) = scalar("CNT?") {
        let target = string(value, "CNT?")?;
        return Ok(NodeKind::VariableReference(VariableReferenceRecord {
            name: String::new(),
            count_path: Some(path(target)),
            count_target: None,
        }));
    }
    if let Some(value) = scalar("VAR=").or_else(|| scalar("temp=")) {
        return Ok(NodeKind::VariableAssignment {
            name: string(value, "variable assignment")?.to_owned(),
            global: fields.contains_key("VAR="),
            new_declaration: !fields.contains_key("re"),
        });
    }
    if let Some(value) = scalar("#") {
        return Ok(NodeKind::Tag(string(value, "#")?.to_owned()));
    }
    if let Some(Field::List(raw_items)) = fields.get("list") {
        let mut items: Vec<_> = raw_items
            .iter()
            .map(|(name, value)| (InkListItem::from_full_name(name), *value))
            .collect();
        items.sort_unstable_by(|left, right| {
            left.0
                .get_origin_name()
                .cmp(&right.0.get_origin_name())
                .then_with(|| left.0.get_item_name().cmp(right.0.get_item_name()))
        });
        let mut origin_names = if items.is_empty() {
            match fields.get("origins") {
                Some(Field::Origins(names)) => names.clone(),
                _ => Vec::new(),
            }
        } else {
            items
                .iter()
                .filter_map(|(item, _)| item.get_origin_name().cloned())
                .collect()
        };
        origin_names.sort_unstable();
        origin_names.dedup();
        return Ok(NodeKind::Value(StaticValue::List(StaticList {
            items,
            origin_names,
        })));
    }
    Err(invalid("unknown runtime object"))
}
