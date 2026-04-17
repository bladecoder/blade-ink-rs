use std::{collections::HashMap, rc::Rc};

use serde_json::{json, Map, Value as JsonValue};

pub use crate::{
    choice_point::ChoicePoint,
    container::Container,
    control_command::{CommandType, ControlCommand},
    divert::Divert,
    glue::Glue,
    ink_list::InkList,
    list_definition::ListDefinition,
    list_definitions_origin::ListDefinitionsOrigin,
    native_function_call::NativeFunctionCall,
    object::{Object, RTObject},
    path::{Component, Path},
    push_pop::PushPopType,
    tag::Tag,
    value::Value,
    value_type::{StringValue, VariablePointerValue},
    variable_assigment::VariableAssignment,
    variable_reference::VariableReference,
    void::Void,
};

use crate::{json::json_write, story::INK_VERSION_CURRENT, story_error::StoryError};

pub fn empty_list_definitions() -> Rc<ListDefinitionsOrigin> {
    Rc::new(ListDefinitionsOrigin::new(&mut Vec::new()))
}

pub fn list_definitions_from_raw(
    raw_lists: HashMap<String, HashMap<String, i32>>,
) -> Rc<ListDefinitionsOrigin> {
    let mut lists = raw_lists
        .into_iter()
        .map(|(name, items)| ListDefinition::new(name, items))
        .collect();
    Rc::new(ListDefinitionsOrigin::new(&mut lists))
}

pub fn story_json_from_container(
    root_container: &Container,
    list_defs: &HashMap<String, HashMap<String, i32>>,
) -> Result<String, StoryError> {
    story_json_from_container_and_named(root_container, &HashMap::new(), list_defs)
}

pub fn story_json_from_container_and_named(
    root_container: &Container,
    top_level_named: &HashMap<String, Rc<Container>>,
    list_defs: &HashMap<String, HashMap<String, i32>>,
) -> Result<String, StoryError> {
    let mut list_defs_json = Map::new();
    for (list_name, items) in list_defs {
        let mut item_json = Map::new();
        for (item_name, value) in items {
            item_json.insert(item_name.clone(), json!(value));
        }
        list_defs_json.insert(list_name.clone(), JsonValue::Object(item_json));
    }

    let root = json_write::write_rt_container(root_container, false)?;
    let mut named_json = Map::new();
    for (name, container) in top_level_named {
        named_json.insert(
            name.clone(),
            json_write::write_rt_container(container.as_ref(), false)?,
        );
    }
    let story_json = json!({
        "inkVersion": INK_VERSION_CURRENT,
        "root": [root, "done", JsonValue::Null],
        "listDefs": JsonValue::Object(list_defs_json),
    });
    let mut story_json = story_json.as_object().unwrap().clone();
    let root_value = story_json.get_mut("root").unwrap().as_array_mut().unwrap();
    root_value[2] = if named_json.is_empty() {
        JsonValue::Null
    } else {
        JsonValue::Object(named_json)
    };

    Ok(JsonValue::Object(story_json).to_string())
}
