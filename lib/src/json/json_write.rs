use std::{collections::HashMap, rc::Rc};

use serde_json::{json, Map};

use crate::{
    choice::Choice,
    choice_point::ChoicePoint,
    container::Container,
    control_command::ControlCommand,
    divert::Divert,
    glue::Glue,
    ink_list::InkList,
    native_function_call::NativeFunctionCall,
    object::RTObject,
    path::Path,
    push_pop::PushPopType,
    story_error::StoryError,
    tag::Tag,
    value::Value,
    value_type::{StringValue, VariablePointerValue},
    variable_assigment::VariableAssignment,
    variable_reference::VariableReference,
    void::Void,
};

pub fn write_dictionary_values(
    objs: &HashMap<String, Rc<Value>>,
) -> Result<serde_json::Value, StoryError> {
    let mut jobjs: Map<String, serde_json::Value> = Map::new();

    for (k, o) in objs {
        jobjs.insert(k.clone(), write_rtobject(o.clone())?);
    }

    Ok(serde_json::Value::Object(jobjs))
}

pub fn write_rtobject(o: Rc<dyn RTObject>) -> Result<serde_json::Value, StoryError> {
    if let Some(c) = o.as_any().downcast_ref::<Container>() {
        return write_rt_container(c, false);
    }

    if let Ok(divert) = o.clone().into_any().downcast::<Divert>() {
        let mut div_type_key = "->";

        if divert.is_external {
            div_type_key = "x()";
        } else if divert.pushes_to_stack {
            if divert.stack_push_type == PushPopType::Function {
                div_type_key = "f()";
            } else if divert.stack_push_type == PushPopType::Tunnel {
                div_type_key = "->t->";
            }
        }

        let target_str = if divert.has_variable_target() {
            divert.variable_divert_name.clone().unwrap()
        } else {
            divert.get_target_path_string().unwrap()
        };

        let mut jobj: Map<String, serde_json::Value> = Map::new();

        jobj.insert(div_type_key.to_string(), json!(target_str));

        if divert.has_variable_target() {
            jobj.insert("var".to_owned(), json!(true));
        }

        if divert.is_conditional {
            jobj.insert("c".to_owned(), json!(true));
        }

        if divert.external_args > 0 {
            jobj.insert("exArgs".to_owned(), json!(divert.external_args));
        }

        return Ok(serde_json::Value::Object(jobj));
    }

    if let Ok(cp) = o.clone().into_any().downcast::<ChoicePoint>() {
        let mut jobj: Map<String, serde_json::Value> = Map::new();
        jobj.insert(
            "*".to_owned(),
            json!(ChoicePoint::get_path_string_on_choice(&cp)),
        );
        jobj.insert("flg".to_owned(), json!(cp.get_flags()));
        return Ok(serde_json::Value::Object(jobj));
    }

    if let Some(v) = Value::get_bool_value(o.as_ref()) {
        return Ok(json!(v));
    }

    if let Some(v) = Value::get_value::<i32>(o.as_ref()) {
        return Ok(json!(v));
    }

    if let Some(v) = Value::get_value::<f32>(o.as_ref()) {
        return Ok(json!(v));
    }

    if let Some(v) = Value::get_value::<&StringValue>(o.as_ref()) {
        let mut s = String::new();

        if v.is_newline {
            s.push('\n');
        } else {
            s.push('^');
            s.push_str(&v.string);
        }

        return Ok(json!(s));
    }

    if let Some(v) = Value::get_value::<&InkList>(o.as_ref()) {
        return Ok(write_ink_list(v));
    }

    if let Some(v) = Value::get_value::<&Path>(o.as_ref()) {
        let mut jobj: Map<String, serde_json::Value> = Map::new();
        jobj.insert("^->".to_owned(), json!(v.get_components_string()));
        return Ok(serde_json::Value::Object(jobj));
    }

    if let Some(v) = Value::get_value::<&VariablePointerValue>(o.as_ref()) {
        let mut jobj: Map<String, serde_json::Value> = Map::new();
        jobj.insert("^var".to_owned(), json!(v.variable_name));
        jobj.insert("ci".to_owned(), json!(v.context_index));
        return Ok(serde_json::Value::Object(jobj));
    }

    if o.as_any().is::<Glue>() {
        return Ok(json!("<>"));
    }

    if let Some(cc) = o.as_any().downcast_ref::<ControlCommand>() {
        return Ok(json!(ControlCommand::get_name(cc.command_type)));
    }

    if let Some(f) = o.as_any().downcast_ref::<NativeFunctionCall>() {
        let mut name = NativeFunctionCall::get_name(f.op);

        // Avoid collision with ^ used to indicate a string
        if "^".eq(&name) {
            name = "L^".to_owned();
        }

        return Ok(json!(name));
    }

    if let Ok(var_ref) = o.clone().into_any().downcast::<VariableReference>() {
        let mut jobj: Map<String, serde_json::Value> = Map::new();

        let read_count_path = var_ref.get_path_string_for_count();
        if read_count_path.is_some() {
            jobj.insert("CNT?".to_owned(), json!(read_count_path));
        } else {
            jobj.insert("VAR?".to_owned(), json!(var_ref.name.clone()));
        }

        return Ok(serde_json::Value::Object(jobj));
    }

    if let Some(var_ass) = o.as_any().downcast_ref::<VariableAssignment>() {
        let mut jobj: Map<String, serde_json::Value> = Map::new();

        let key = if var_ass.is_global {
            "VAR=".to_owned()
        } else {
            "temp=".to_owned()
        };
        jobj.insert(key, json!(var_ass.variable_name));

        // Reassignment?
        if !var_ass.is_new_declaration {
            jobj.insert("re".to_owned(), json!(true));
        }

        return Ok(serde_json::Value::Object(jobj));
    }

    if o.as_any().is::<Void>() {
        return Ok(json!("void"));
    }

    if let Some(tag) = o.as_any().downcast_ref::<Tag>() {
        let mut jobj: Map<String, serde_json::Value> = Map::new();

        jobj.insert("#".to_owned(), json!(tag.get_text()));

        return Ok(serde_json::Value::Object(jobj));
    }

    if let Some(choice) = o.as_any().downcast_ref::<Choice>() {
        return Ok(write_choice(choice));
    }

    Err(StoryError::BadJson(format!(
        "Failed to write runtime object to JSON: {}",
        o
    )))
}

pub fn write_rt_container(
    container: &Container,
    without_name: bool,
) -> Result<serde_json::Value, StoryError> {
    let mut c_array: Vec<serde_json::Value> = Vec::new();

    for c in container.content.iter() {
        c_array.push(write_rtobject(c.clone())?);
    }

    // Container is always an array [...]
    // But the final element is always either:
    // - a dictionary containing the named content, as well as possibly
    // the key "#" with the count flags
    // - null, if neither of the above
    let named_only_content = &container.get_named_only_content();
    let count_flags = container.get_count_flags();
    let has_name_property = container.name.is_some() && !without_name;

    let has_terminator = !named_only_content.is_empty() || count_flags > 0 || has_name_property;

    if has_terminator {
        let mut t_obj: Map<String, serde_json::Value> = Map::new();

        for (name, c) in named_only_content {
            t_obj.insert(name.clone(), write_rt_container(c.as_ref(), true)?);
        }

        if count_flags > 0 {
            t_obj.insert("#f".to_owned(), json!(count_flags));
        }

        if has_name_property {
            t_obj.insert("#n".to_owned(), json!(container.name));
        }

        c_array.push(serde_json::Value::Object(t_obj));
    } else {
        c_array.push(serde_json::Value::Null);
    }

    Ok(serde_json::Value::Array(c_array))
}

pub fn write_ink_list(list: &InkList) -> serde_json::Value {
    let mut jobj: Map<String, serde_json::Value> = Map::new();

    let mut jlist: Map<String, serde_json::Value> = Map::new();
    for (item, v) in list.items.iter() {
        let mut name = String::new();

        match item.get_origin_name() {
            Some(n) => name.push_str(n),
            None => name.push('?'),
        }

        name.push('.');
        name.push_str(item.get_item_name());

        jlist.insert(name, json!(v));
    }

    jobj.insert("list".to_owned(), serde_json::Value::Object(jlist));

    serde_json::Value::Object(jobj)
}

pub fn write_choice(choice: &Choice) -> serde_json::Value {
    let mut jobj: Map<String, serde_json::Value> = Map::new();

    jobj.insert("text".to_owned(), json!(choice.text));
    jobj.insert("index".to_owned(), json!(*choice.index.borrow()));
    jobj.insert("originalChoicePath".to_owned(), json!(choice.source_path));
    jobj.insert(
        "originalThreadIndex".to_owned(),
        json!(choice.original_thread_index),
    );
    jobj.insert(
        "targetPath".to_owned(),
        json!(choice.target_path.to_string()),
    );

    jobj.insert("tags".to_owned(), write_choice_tags(choice));

    serde_json::Value::Object(jobj)
}

fn write_choice_tags(choice: &Choice) -> serde_json::Value {
    let mut tags: Vec<serde_json::Value> = Vec::new();
    for t in &choice.tags {
        tags.push(json!(t));
    }

    serde_json::Value::Array(tags)
}

pub(crate) fn write_list_rt_objs(
    objs: &[Rc<dyn RTObject>],
) -> Result<serde_json::Value, StoryError> {
    let mut c_array: Vec<serde_json::Value> = Vec::new();

    for o in objs {
        c_array.push(write_rtobject(o.clone())?);
    }

    Ok(serde_json::Value::Array(c_array))
}

pub(crate) fn write_int_dictionary(map: &HashMap<String, i32>) -> serde_json::Value {
    let mut jobj: Map<String, serde_json::Value> = Map::new();

    for (key, val) in map {
        jobj.insert(key.clone(), json!(*val));
    }

    serde_json::Value::Object(jobj)
}
