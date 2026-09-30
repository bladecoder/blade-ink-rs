//! Serde JSON tokens to owned arena records, without a runtime-object tree.

use super::*;
use crate::story::{INK_VERSION_CURRENT, INK_VERSION_MINIMUM_COMPATIBLE};
use serde_json::{Map, Value as JsonValue};

pub(super) fn load_from_reader(
    reader: impl Read,
    observer: &mut impl LoadObserver,
) -> Result<(i32, FlatStoryData), StoryError> {
    let json: JsonValue = serde_json::from_reader(reader)
        .map_err(|_| StoryError::BadJson("Story not in JSON format.".to_owned()))?;
    observer.record(LoadPhase::JsonDecoded);
    let version = json
        .get("inkVersion")
        .and_then(JsonValue::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| StoryError::BadJson("ink version number not found".to_owned()))?;
    if version > INK_VERSION_CURRENT {
        return Err(StoryError::BadJson(
            "Version of ink used to build story was newer than the current version of the engine"
                .to_owned(),
        ));
    }
    if version < INK_VERSION_MINIMUM_COMPATIBLE {
        return Err(StoryError::BadJson(
            "Version of ink used to build story is too old to be loaded by this version of the engine"
                .to_owned(),
        ));
    }

    let definitions = json
        .get("listDefs")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| StoryError::BadJson("List Definitions node for ink not found".to_owned()))?;
    let mut list_definitions = Vec::with_capacity(definitions.len());
    for (name, items_json) in definitions {
        let items_json = items_json
            .as_object()
            .ok_or_else(|| StoryError::BadJson("list definition must be an object".to_owned()))?;
        let mut items = Vec::with_capacity(items_json.len());
        for (item, value) in items_json {
            items.push((item.clone(), integer(value, "list item")?));
        }
        items.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        list_definitions.push(StaticListDefinition {
            name: name.clone(),
            items,
        });
    }
    list_definitions.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    let mut data = FlatStoryData {
        nodes: Vec::new(),
        children: Vec::new(),
        named: Vec::new(),
        list_definitions,
    };
    let root = json
        .get("root")
        .ok_or_else(|| StoryError::BadJson("Root node for ink not found".to_owned()))?;
    let root_id = data.parse_node(root, None, None, None)?;
    if root_id != data.root() || data.container_id(root_id).is_none() {
        return Err(StoryError::BadJson(
            "Root node for ink is not a container".to_owned(),
        ));
    }
    drop(json);
    observer.record(LoadPhase::ArenaBuilt);
    data.resolve_static_targets()?;
    observer.record(LoadPhase::TargetsResolved);
    Ok((version, data))
}

fn integer(value: &JsonValue, field: &str) -> Result<i32, StoryError> {
    value
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .ok_or_else(|| StoryError::BadJson(format!("{field} must be an integer")))
}

fn string<'a>(value: &'a JsonValue, field: &str) -> Result<&'a str, StoryError> {
    value
        .as_str()
        .ok_or_else(|| StoryError::BadJson(format!("{field} must be a string")))
}

fn path(value: &str) -> StaticPath {
    StaticPath::from_text(value)
}

impl FlatStoryData {
    fn parse_node(
        &mut self,
        token: &JsonValue,
        name: Option<String>,
        parent: Option<NodeId>,
        child_index: Option<u32>,
    ) -> Result<NodeId, StoryError> {
        let kind =
            match token {
                JsonValue::Null => {
                    return Err(StoryError::BadJson(
                        "null is not a runtime object".to_owned(),
                    ));
                }
                JsonValue::Bool(value) => NodeKind::Value(StaticValue::Bool(*value)),
                JsonValue::Number(value) => {
                    if let Some(number) = value.as_i64() {
                        let number = i32::try_from(number).map_err(|_| {
                            StoryError::BadJson("integer is outside the supported range".to_owned())
                        })?;
                        NodeKind::Value(StaticValue::Int(number))
                    } else {
                        NodeKind::Value(StaticValue::Float(value.as_f64().ok_or_else(|| {
                            StoryError::BadJson("invalid numeric value".to_owned())
                        })? as f32))
                    }
                }
                JsonValue::String(value) => parse_string(value)?,
                JsonValue::Object(object) => parse_object(object)?,
                JsonValue::Array(_) => NodeKind::Container(ContainerRecord {
                    name,
                    count_flags: 0,
                    children: 0..0,
                    named: 0..0,
                }),
            };
        let id = NodeId(
            u32::try_from(self.nodes.len())
                .map_err(|_| StoryError::BadJson("story contains too many nodes".to_owned()))?,
        );
        self.nodes.push(NodeRecord {
            parent,
            child_index,
            kind,
        });
        if let JsonValue::Array(array) = token {
            self.parse_container(id, array)?;
        }
        Ok(id)
    }

    fn parse_container(&mut self, id: NodeId, array: &[JsonValue]) -> Result<(), StoryError> {
        let (terminator, ordered) = array
            .split_last()
            .ok_or_else(|| StoryError::BadJson("container array is empty".to_owned()))?;
        let mut child_ids = Vec::with_capacity(ordered.len());
        let mut named = HashMap::<String, NodeId>::new();
        for (index, token) in ordered.iter().enumerate() {
            let child_index = u32::try_from(index).map_err(|_| {
                StoryError::BadJson("container contains too many children".to_owned())
            })?;
            let child = self.parse_node(token, None, Some(id), Some(child_index))?;
            child_ids.push(child);
            if let NodeKind::Container(record) = &self.nodes[child.index()].kind
                && let Some(name) = record.name.as_ref().filter(|name| !name.is_empty())
            {
                named.insert(name.clone(), child);
            }
        }
        let child_start = self.children.len();
        self.children.extend(child_ids);
        let child_end = self.children.len();

        let mut flags = 0;
        let mut override_name = None;
        if let Some(object) = terminator.as_object() {
            for (key, token) in object {
                match key.as_str() {
                    "#f" => flags = integer(token, "container flags")?,
                    "#n" => override_name = Some(string(token, "container name")?.to_owned()),
                    _ => {
                        if named.contains_key(key) {
                            continue;
                        }
                        let child = self.parse_node(token, Some(key.clone()), Some(id), None)?;
                        if self.container_id(child).is_none() {
                            return Err(StoryError::BadJson(
                                "named content must be a container".to_owned(),
                            ));
                        }
                        named.insert(key.clone(), child);
                    }
                }
            }
        } else if !terminator.is_null() {
            return Err(StoryError::BadJson(
                "container terminator must be an object or null".to_owned(),
            ));
        }
        let mut named: Vec<_> = named
            .into_iter()
            .map(|(name, node)| NamedChild { name, node })
            .collect();
        named.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        let named_start = self.named.len();
        self.named.extend(named);
        let named_end = self.named.len();
        let NodeKind::Container(record) = &mut self.nodes[id.index()].kind else {
            unreachable!("array was registered as a container")
        };
        record.name = override_name.or_else(|| record.name.take());
        record.count_flags = flags;
        record.children = child_start..child_end;
        record.named = named_start..named_end;
        Ok(())
    }
}

fn parse_string(value: &str) -> Result<NodeKind, StoryError> {
    if let Some(string) = value.strip_prefix('^') {
        return Ok(NodeKind::Value(StaticValue::String(string.to_owned())));
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

fn parse_object(object: &Map<String, JsonValue>) -> Result<NodeKind, StoryError> {
    if let Some(target) = object.get("^->") {
        return Ok(NodeKind::Value(StaticValue::DivertTarget {
            path: path(string(target, "divert target")?),
            target: None,
        }));
    }
    if let Some(variable) = object.get("^var") {
        return Ok(NodeKind::Value(StaticValue::VariablePointer {
            name: string(variable, "variable pointer")?.to_owned(),
            context_index: object
                .get("ci")
                .map_or(Ok(-1), |value| integer(value, "ci"))?,
        }));
    }
    let divert = [
        ("->", false, PushPopType::Function, false),
        ("f()", true, PushPopType::Function, false),
        ("->t->", true, PushPopType::Tunnel, false),
        ("x()", false, PushPopType::Function, true),
    ];
    for (key, pushes_to_stack, stack_push_type, external) in divert {
        if let Some(target) = object.get(key) {
            let target = string(target, "divert destination")?;
            let variable = object.contains_key("var");
            let external_args = object
                .get("exArgs")
                .map_or(Ok(0), |value| integer(value, "exArgs"))?;
            return Ok(NodeKind::Divert(DivertRecord {
                path: (!variable).then(|| path(target)),
                target: None,
                variable_name: variable.then(|| target.to_owned()),
                external_args: usize::try_from(external_args)
                    .map_err(|_| StoryError::BadJson("exArgs cannot be negative".to_owned()))?,
                conditional: object.contains_key("c"),
                external,
                pushes_to_stack,
                stack_push_type,
            }));
        }
    }
    if let Some(target) = object.get("*") {
        return Ok(NodeKind::ChoicePoint(ChoiceRecord {
            path: Some(path(string(target, "choice target")?)),
            target: None,
            flags: object
                .get("flg")
                .map_or(Ok(0), |value| integer(value, "flg"))?,
        }));
    }
    if let Some(name) = object.get("VAR?") {
        return Ok(NodeKind::VariableReference(VariableReferenceRecord {
            name: string(name, "variable reference")?.to_owned(),
            count_path: None,
            count_target: None,
        }));
    }
    if let Some(target) = object.get("CNT?") {
        let target = string(target, "read count target")?;
        return Ok(NodeKind::VariableReference(VariableReferenceRecord {
            name: String::new(),
            count_path: Some(path(target)),
            count_target: None,
        }));
    }
    if let Some(name) = object.get("VAR=").or_else(|| object.get("temp=")) {
        return Ok(NodeKind::VariableAssignment {
            name: string(name, "variable assignment")?.to_owned(),
            global: object.contains_key("VAR="),
            new_declaration: !object.contains_key("re"),
        });
    }
    if let Some(text) = object.get("#") {
        return Ok(NodeKind::Tag(string(text, "tag")?.to_owned()));
    }
    if let Some(items_json) = object.get("list") {
        let items_json = items_json
            .as_object()
            .ok_or_else(|| StoryError::BadJson("list must be an object".to_owned()))?;
        let mut items = Vec::with_capacity(items_json.len());
        for (name, value) in items_json {
            items.push((
                InkListItem::from_full_name(name),
                integer(value, "list item")?,
            ));
        }
        items.sort_unstable_by(|left, right| {
            left.0
                .get_origin_name()
                .cmp(&right.0.get_origin_name())
                .then_with(|| left.0.get_item_name().cmp(right.0.get_item_name()))
        });
        let mut origin_names = Vec::new();
        if items.is_empty() {
            if let Some(origins) = object.get("origins") {
                for origin in origins
                    .as_array()
                    .ok_or_else(|| StoryError::BadJson("origins must be an array".to_owned()))?
                {
                    origin_names.push(string(origin, "list origin")?.to_owned());
                }
            }
        } else {
            for (item, _) in &items {
                if let Some(origin) = item.get_origin_name() {
                    origin_names.push(origin.clone());
                }
            }
        }
        origin_names.sort_unstable();
        origin_names.dedup();
        return Ok(NodeKind::Value(StaticValue::List(StaticList {
            items,
            origin_names,
        })));
    }
    Err(StoryError::BadJson("unknown runtime object".to_owned()))
}
