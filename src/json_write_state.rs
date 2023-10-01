use std::{collections::HashMap, rc::Rc};

use serde_json::{Map, json};

use crate::{
    container::Container,
    object::RTObject, value::Value, glue::Glue, choice_point::ChoicePoint, push_pop::PushPopType, divert::Divert, ink_list::InkList, control_command::ControlCommand, native_function_call::NativeFunctionCall, variable_reference::VariableReference, variable_assigment::VariableAssignment, tag::Tag, void::Void, choice::Choice,
};

pub fn write_dictionary_values(objs: &HashMap<String, Rc<Value>>) -> serde_json::Value {
    let mut jobjs: Map<String, serde_json::Value> = Map::new();

    for (k,o) in objs {
        jobjs.insert(k.clone(), write_rtobject(o.clone()));
    }

    serde_json::Value::Object(jobjs)
}

pub fn write_rtobject(o: Rc<dyn RTObject>) -> serde_json::Value {
    if let Some(c) = o.as_any().downcast_ref::<Container>() {
        return write_rt_container(c, false);
    }

    if let Some(divert) = o.as_any().downcast_ref::<Divert>() {
        let mut div_type_key = "->";

        if divert.is_external { div_type_key = "x()"; }
        else if divert.pushes_to_stack {
            if divert.stack_push_type == PushPopType::Function {div_type_key = "f()";}
            else if divert.stack_push_type == PushPopType::Tunnel {div_type_key = "->t->";}
        }

        let target_str =
            if divert.has_variable_target() {divert.variable_divert_name.clone().unwrap()}
            else {divert.get_target_path_string().unwrap()};

        let mut jobj: Map<String, serde_json::Value> = Map::new();

        jobj.insert(div_type_key.to_string(), json!(target_str));

        if divert.has_variable_target() {jobj.insert("var".to_owned(), json!(true));}

        if divert.is_conditional {jobj.insert("c".to_owned(), json!(true));}

        if divert.external_args > 0 {jobj.insert("exArgs".to_owned(), json!(divert.external_args));}

        return serde_json::Value::Object(jobj);
    }

    if let Ok(cp) = o.clone().into_any().downcast::<ChoicePoint>() {
        let mut jobj: Map<String, serde_json::Value> = Map::new();
        jobj.insert("*".to_owned(), json!(ChoicePoint::get_path_string_on_choice(&cp)));
        jobj.insert("flg".to_owned(), json!(cp.get_flags()));
        return serde_json::Value::Object(jobj);
    }

    if let Some(v) = Value::get_bool_value(o.as_ref()) {
        return json!(v);
    }

    if let Some(v) = Value::get_int_value(o.as_ref()) {
        return json!(v);
    }

    if let Some(v) = Value::get_float_value(o.as_ref()) {
        return json!(v);
    }

    if let Some(v) = Value::get_string_value(o.as_ref()) {
        let mut s = String::new();

        if v.is_newline {
            s.push('\n');
        } else {
            s.push('^');
            s.push_str(&v.string);
        }

        return json!(s);
    }

    if let Some(v) = Value::get_list_value(o.as_ref()) {
        return write_ink_list(v);
    }

    if let Some(v) = Value::get_divert_target_value(o.as_ref()) {
        let mut jobj: Map<String, serde_json::Value> = Map::new();
        jobj.insert("^->".to_owned(), json!(v.get_components_string()));
        return serde_json::Value::Object(jobj);
    }

    if let Some(v) = Value::get_variable_pointer_value(o.as_ref()) {
        let mut jobj: Map<String, serde_json::Value> = Map::new();
        jobj.insert("^var".to_owned(), json!(v.variable_name));
        jobj.insert("ci".to_owned(), json!(v.context_index));
        return serde_json::Value::Object(jobj);
    }

    if o.as_any().is::<Glue>() {
        return json!("<>")
    }

    if let Some(cc) = o.as_any().downcast_ref::<ControlCommand>() {
        return json!(ControlCommand::get_name(cc.command_type));
    }

    if let Some(f) = o.as_any().downcast_ref::<NativeFunctionCall>() {
        let mut name = NativeFunctionCall::get_name(f.op);

        // Avoid collision with ^ used to indicate a string
        if "^".eq(&name) {name = "L^".to_owned();}

        return json!(name);
    }

    if let Ok(var_ref) = o.clone().into_any().downcast::<VariableReference>() {

        let mut jobj: Map<String, serde_json::Value> = Map::new();

        let read_count_path = var_ref.get_path_string_for_count();
        if read_count_path.is_some() {
            jobj.insert("CNT?".to_owned(), json!(read_count_path));
        } else {
            jobj.insert("VAR?".to_owned(), json!(var_ref.name.clone()));
        }

        return serde_json::Value::Object(jobj);
    }

    if let Some(var_ass) = o.as_any().downcast_ref::<VariableAssignment>() {
        let mut jobj: Map<String, serde_json::Value> = Map::new();
        
        let key = if var_ass.is_global {"VAR=".to_owned()} else {"temp=".to_owned()};
        jobj.insert(key, json!(var_ass.variable_name));

        // Reassignment?
        if !var_ass.is_new_declaration {jobj.insert("re".to_owned(), json!(true));}
        
        return serde_json::Value::Object(jobj);
    }

    if o.as_any().is::<Void>() {
        return json!("void")
    }

    if let Some(tag) = o.as_any().downcast_ref::<Tag>() {
        let mut jobj: Map<String, serde_json::Value> = Map::new();

        jobj.insert("#".to_owned(), json!(tag.get_text()));
        
        return serde_json::Value::Object(jobj);
    }

    if let Some(choice) = o.as_any().downcast_ref::<Choice>() {
        return write_choice(choice);
    }

    panic!("Failed to write runtime object to JSON: {}", o.to_string());
}

pub fn write_rt_container(container: &Container, without_name: bool) -> serde_json::Value {
    let mut c_array: Vec<serde_json::Value> = Vec::new();

    for c in container.content.iter() {
        c_array.push(write_rtobject(c.clone()));
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
            t_obj.insert(name.clone(), write_rt_container(c.as_ref(), true));
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

    serde_json::Value::Array(c_array)
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
    jobj.insert("index".to_owned(), json!(choice.index));
    jobj.insert("originalChoicePath".to_owned(), json!(choice.source_path));
    jobj.insert("originalThreadIndex".to_owned(), json!(choice.original_thread_index));
    jobj.insert("targetPath".to_owned(), json!(choice.target_path.to_string()));

    serde_json::Value::Object(jobj)
}

pub(crate) fn write_list_rt_objs(objs: &[Rc<dyn RTObject>]) -> serde_json::Value {
    let mut c_array: Vec<serde_json::Value> = Vec::new();
    
    for o in objs {
        c_array.push(write_rtobject(o.clone()));
    }

    serde_json::Value::Array(c_array)
}

pub(crate) fn write_int_dictionary(map: &HashMap<String, i32>) -> serde_json::Value {
    let mut jobj: Map<String, serde_json::Value> = Map::new();

    for (key, val) in map {
        jobj.insert(key.clone(), json!(*val));
    }

    serde_json::Value::Object(jobj)
}
