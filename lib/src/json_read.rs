use std::collections::HashMap;

use serde_json::Map;

use crate::{
    choice::Choice, choice_point::ChoicePoint, container::Container,
    control_command::ControlCommand, divert::Divert, glue::Glue, ink_list::InkList,
    ink_list_item::InkListItem, list_definition::ListDefinition,
    list_definitions_origin::ListDefinitionsOrigin, native_function_call::NativeFunctionCall,
    object::RTObject, path::Path, push_pop::PushPopType, story_error::StoryError, tag::Tag,
    value::Value, variable_assigment::VariableAssignment, variable_reference::VariableReference,
    void::Void, Brc,
};

pub fn jtoken_to_runtime_object(
    token: &serde_json::Value,
    name: Option<String>,
) -> Result<Brc<dyn RTObject>, StoryError> {
    match token {
        serde_json::Value::Null => Err(StoryError::BadJson(format!(
            "Failed to convert token to runtime RTObject: {}",
            token
        ))),
        serde_json::Value::Bool(value) => Ok(Brc::new(Value::new_bool(value.to_owned()))),
        serde_json::Value::Number(_) => {
            if token.is_i64() {
                let val: i32 = token.as_i64().unwrap().try_into().unwrap();
                Ok(Brc::new(Value::new_int(val)))
            } else {
                let val: f32 = token.as_f64().unwrap() as f32;
                Ok(Brc::new(Value::new_float(val)))
            }
        }

        serde_json::Value::String(value) => {
            let str = value.as_str();

            // String value
            let first_char = str.chars().next().unwrap();
            if first_char == '^' {
                return Ok(Brc::new(Value::new_string(&str[1..])));
            } else if first_char == '\n' && str.len() == 1 {
                return Ok(Brc::new(Value::new_string("\n")));
            }

            // Glue
            if "<>".eq(str) {
                return Ok(Brc::new(Glue::new()));
            }

            if let Some(control_command) = ControlCommand::new_from_name(str) {
                return Ok(Brc::new(control_command));
            }

            // Native functions
            // "^" conflicts with the way to identify strings, so now
            // we know it's not a string, we can convert back to the proper
            // symbol for the operator.
            let mut call_str = str;
            if "L^".eq(str) {
                call_str = "^";
            }
            if let Some(native_function_call) = NativeFunctionCall::new_from_name(call_str) {
                return Ok(Brc::new(native_function_call));
            }

            // Void
            if "void".eq(str) {
                return Ok(Brc::new(Void::new()));
            }

            Err(StoryError::BadJson(format!(
                "Failed to convert token to runtime RTObject: {}",
                token
            )))
        }
        serde_json::Value::Array(value) => Ok(jarray_to_container(value, name)?),
        serde_json::Value::Object(obj) => {
            // Divert target value to path
            let prop_value = obj.get("^->");

            if let Some(prop_value) = prop_value {
                return Ok(Brc::new(Value::new_divert_target(
                    Path::new_with_components_string(prop_value.as_str()),
                )));
            }

            // // VariablePointerValue
            let prop_value = obj.get("^var");

            if let Some(v) = prop_value {
                let variable_name = v.as_str().unwrap();
                let mut contex_index = -1;
                let prop_value = obj.get("ci");

                if let Some(v) = prop_value {
                    contex_index = v.as_i64().unwrap() as i32;
                }

                let var_ptr = Brc::new(Value::new_variable_pointer(variable_name, contex_index));

                return Ok(var_ptr);
            }

            // // Divert
            let mut is_divert = false;
            let mut pushes_to_stack = false;
            let mut div_push_type = PushPopType::Function;
            let mut external = false;

            let mut prop_value = obj.get("->");
            if prop_value.is_some() {
                is_divert = true;
            } else {
                prop_value = obj.get("f()");
                if prop_value.is_some() {
                    is_divert = true;
                    pushes_to_stack = true;
                    div_push_type = PushPopType::Function;
                } else {
                    prop_value = obj.get("->t->");
                    if prop_value.is_some() {
                        is_divert = true;
                        pushes_to_stack = true;
                        div_push_type = PushPopType::Tunnel;
                    } else {
                        prop_value = obj.get("x()");
                        if prop_value.is_some() {
                            is_divert = true;
                            external = true;
                            pushes_to_stack = false;
                            div_push_type = PushPopType::Function;
                        }
                    }
                }
            }

            if is_divert {
                let target = prop_value.unwrap().as_str().unwrap().to_string();

                let mut var_divert_name: Option<String> = None;
                let mut target_path: Option<String> = None;

                prop_value = obj.get("var");

                if prop_value.is_some() {
                    var_divert_name = Some(target);
                } else {
                    target_path = Some(target);
                }

                prop_value = obj.get("c");
                let conditional = prop_value.is_some();
                let mut external_args = 0;

                if external {
                    prop_value = obj.get("exArgs");
                    if let Some(prop_value) = prop_value {
                        external_args = prop_value.as_i64().unwrap() as usize;
                    }
                }

                return Ok(Brc::new(Divert::new(
                    pushes_to_stack,
                    div_push_type,
                    external,
                    external_args,
                    conditional,
                    var_divert_name,
                    target_path.as_deref(),
                )));
            }

            // Choice
            let prop_value = obj.get("*");
            if let Some(cp) = prop_value {
                let mut flags = 0;
                let path_string_on_choice = cp.as_str().unwrap();
                let prop_value = obj.get("flg");
                if let Some(f) = prop_value {
                    flags = f.as_u64().unwrap();
                }

                return Ok(Brc::new(ChoicePoint::new(
                    flags as i32,
                    path_string_on_choice,
                )));
            }

            // // Variable reference
            let prop_value = obj.get("VAR?");
            if let Some(name) = prop_value {
                return Ok(Brc::new(VariableReference::new(name.as_str().unwrap())));
            }

            let prop_value = obj.get("CNT?");
            if let Some(v) = prop_value {
                return Ok(Brc::new(VariableReference::from_path_for_count(
                    v.as_str().unwrap(),
                )));
            }

            // // Variable assignment
            let mut is_var_ass = false;
            let mut is_global_var = false;

            let mut prop_value = obj.get("VAR=");
            match prop_value {
                Some(_) => {
                    is_var_ass = true;
                    is_global_var = true;
                }
                None => {
                    prop_value = obj.get("temp=");
                    if prop_value.is_some() {
                        is_var_ass = true;
                        is_global_var = false;
                    }
                }
            }

            if is_var_ass {
                let var_name = prop_value.unwrap().as_str().unwrap();
                let prop_value = obj.get("re");
                let is_new_decl = prop_value.is_none();

                let var_ass = Brc::new(VariableAssignment::new(
                    var_name,
                    is_new_decl,
                    is_global_var,
                ));
                return Ok(var_ass);
            }

            // Legacy Tag
            prop_value = obj.get("#");
            if let Some(prop_value) = prop_value {
                return Ok(Brc::new(Tag::new(prop_value.as_str().unwrap())));
            }

            // List value
            prop_value = obj.get("list");

            if let Some(pv) = prop_value {
                let list_content = pv.as_object().unwrap();
                let mut raw_list = InkList::new();

                prop_value = obj.get("origins");

                if let Some(o) = prop_value {
                    let names_as_objs = o.as_array().unwrap();

                    let names = names_as_objs
                        .iter()
                        .map(|e| e.as_str().unwrap().to_string())
                        .collect();

                    raw_list.set_initial_origin_names(names);
                }

                for (k, v) in list_content {
                    let item = InkListItem::from_full_name(k);
                    raw_list.items.insert(item, v.as_i64().unwrap() as i32);
                }

                return Ok(Brc::new(Value::new_list(raw_list)));
            }

            // Used when serialising save state only
            if obj.get("originalChoicePath").is_some() {
                return jobject_to_choice(obj);
            }

            Err(StoryError::BadJson(format!(
                "Failed to convert token to runtime RTObject: {}",
                token
            )))
        }
    }
}

fn jarray_to_container(
    jarray: &Vec<serde_json::Value>,
    name: Option<String>,
) -> Result<Brc<dyn RTObject>, StoryError> {
    // Final object in the array is always a combination of
    //  - named content
    //  - a "#f" key with the countFlags
    // (if either exists at all, otherwise null)
    let terminating_obj = jarray[jarray.len() - 1].as_object();
    let mut name: Option<String> = name;
    let mut flags = 0;

    let mut named_only_content: HashMap<String, Brc<Container>> = HashMap::new();

    if let Some(terminating_obj) = terminating_obj {
        for (k, v) in terminating_obj {
            match k.as_str() {
                "#f" => flags = v.as_i64().unwrap().try_into().unwrap(),
                "#n" => name = Some(v.as_str().unwrap().to_string()),
                k => {
                    let named_content_item =
                        jtoken_to_runtime_object(v, Some(k.to_string())).unwrap();

                    let named_sub_container = named_content_item
                        .into_any()
                        .downcast::<Container>()
                        .unwrap();

                    named_only_content.insert(k.to_string(), named_sub_container);
                }
            }
        }
    }

    let container = Container::new(
        name,
        flags,
        jarray_to_runtime_obj_list(jarray, true)?,
        named_only_content,
    );
    Ok(container)
}

pub fn jarray_to_runtime_obj_list(
    jarray: &Vec<serde_json::Value>,
    skip_last: bool,
) -> Result<Vec<Brc<dyn RTObject>>, StoryError> {
    let mut count = jarray.len();

    if skip_last {
        count -= 1;
    }

    let mut list: Vec<Brc<dyn RTObject>> = Vec::with_capacity(jarray.len());

    for jtok in jarray.iter().take(count) {
        let runtime_obj = jtoken_to_runtime_object(jtok, None);
        list.push(runtime_obj?);
    }

    Ok(list)
}

fn jobject_to_choice(
    obj: &Map<String, serde_json::Value>,
) -> Result<Brc<dyn RTObject>, StoryError> {
    let text = obj.get("text").unwrap().as_str().unwrap();
    let index = obj.get("index").unwrap().as_u64().unwrap() as usize;
    let source_path = obj.get("originalChoicePath").unwrap().as_str().unwrap();
    let original_thread_index = obj.get("originalThreadIndex").unwrap().as_i64().unwrap() as usize;
    let path_string_on_choice = obj.get("targetPath").unwrap().as_str().unwrap();

    Ok(Brc::new(Choice::new_from_json(
        path_string_on_choice,
        source_path.to_string(),
        text,
        index,
        original_thread_index,
    )))
}

pub fn jtoken_to_list_definitions(
    def: &serde_json::Value,
) -> Result<ListDefinitionsOrigin, StoryError> {
    let mut all_defs: Vec<ListDefinition> = Vec::with_capacity(0);

    for (name, list_def_json) in def.as_object().unwrap() {
        // Cast (string, object) to (string, int) for items
        let mut items: HashMap<String, i32> = HashMap::new();
        for (k, v) in list_def_json.as_object().unwrap() {
            items.insert(k.clone(), v.as_u64().unwrap() as i32);
        }

        let def = ListDefinition::new(name.clone(), items);
        all_defs.push(def);
    }

    Ok(ListDefinitionsOrigin::new(&mut all_defs))
}

pub(crate) fn jobject_to_hashmap_values(
    jobj: &Map<String, serde_json::Value>,
) -> Result<HashMap<String, Brc<Value>>, StoryError> {
    let mut dict: HashMap<String, Brc<Value>> = HashMap::new();

    for (k, v) in jobj.iter() {
        dict.insert(
            k.clone(),
            jtoken_to_runtime_object(v, None)?
                .into_any()
                .downcast::<Value>()
                .unwrap(),
        );
    }

    Ok(dict)
}

pub(crate) fn jobject_to_int_hashmap(
    jobj: &Map<String, serde_json::Value>,
) -> Result<HashMap<String, i32>, StoryError> {
    let mut dict: HashMap<String, i32> = HashMap::new();

    for (k, v) in jobj.iter() {
        dict.insert(k.clone(), v.as_i64().unwrap() as i32);
    }

    Ok(dict)
}
