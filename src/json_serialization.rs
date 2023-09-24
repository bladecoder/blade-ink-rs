use std::{collections::HashMap, rc::Rc};

use serde_json::Map;

use crate::{
    container::Container,
    object::{self, RTObject}, control_command::{CommandType, ControlCommand}, value::Value, glue::Glue, path::Path, choice_point::ChoicePoint, choice::Choice, push_pop::PushPopType, divert::Divert, variable_assigment::VariableAssignment, void::Void, variable_reference::VariableReference, native_function_call::{self, NativeFunctionCall},
};

pub fn jtoken_to_runtime_object(token: &serde_json::Value, name: Option<String>) -> Result<Rc<dyn RTObject>, String> {
    match token {
        serde_json::Value::Null =>  Ok(Rc::new(object::Null::new())),
        serde_json::Value::Bool(value) => Ok(Rc::new(Value::new_bool(value.to_owned()))),
        serde_json::Value::Number(_) => {
            if token.is_i64() {
                let val:i32 = token.as_i64().unwrap().try_into().unwrap();
                Ok(Rc::new(Value::new_int(val)))
            } else {
                let val: f32 = token.as_f64().unwrap() as f32;
                Ok(Rc::new(Value::new_float(val)))
            }
        },

        serde_json::Value::String(value) => {
            let str = value.as_str();
            // String value
            let first_char = str.chars().next().unwrap();
            if first_char == '^' {return Ok(Rc::new(Value::new_string(&str[1..])));}     
            else if first_char == '\n' && str.len() == 1 {return Ok(Rc::new(Value::new_string("\n")));}

            // Glue
            if "<>".eq(str) {
                return  Ok(Rc::new(Glue::new()));
            }

            if let Some(control_command) = ControlCommand::new_from_name(str) {
                return Ok(Rc::new(control_command));
            }

            // Native functions
            // "^" conflicts with the way to identify strings, so now
            // we know it's not a string, we can convert back to the proper
            // symbol for the operator.
            let mut call_str = str;
            if "L^".eq(str) {call_str = &"^";}
            if let Some(native_function_call) = NativeFunctionCall::new_from_name(call_str) {
                return Ok(Rc::new(native_function_call));
            }
            
            // Void
            if "void".eq(str) {return Ok(Rc::new(Void::new()));}


            return Err(format!("Failed to convert token to runtime RTObject: {}", &token.to_string()));
        },
        serde_json::Value::Array(value) => Ok(jarray_to_container(value, name)?),
        serde_json::Value::Object(obj) => {
            // Divert target value to path
            let prop_value = obj.get("^->");

            if prop_value.is_some() {
                return Ok(Rc::new(Value::new_divert_target(Path::new_with_components_string(prop_value.unwrap().as_str()))));
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

                let var_ptr = Rc::new(Value::new_variable_pointer( variable_name, contex_index));
                
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
                    if prop_value.is_some() {
                        external_args = prop_value.unwrap().as_i64().unwrap() as i32;
                    }
                }

                return Ok(Rc::new(Divert::new(pushes_to_stack, div_push_type, external, external_args, conditional, var_divert_name, target_path.as_deref())));
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

                return Ok(Rc::new(ChoicePoint::new(flags as i32, path_string_on_choice)));
            }

            // // Variable reference
            let prop_value = obj.get("VAR?");
            if let Some(name) = prop_value {
                return Ok(Rc::new(VariableReference::new(name.as_str().unwrap())));
            }

            let prop_value = obj.get("CNT?");
            if let Some(v) = prop_value {
                return Ok(Rc::new(VariableReference::from_path_for_count(v.as_str().unwrap())));
            }


            // // Variable assignment
            let mut is_var_ass = false;
            let mut is_global_var = false;

            let mut prop_value = obj.get("VAR=");
            match prop_value {
                Some(_) => {
                    is_var_ass = true;
                    is_global_var = true;
                },
                None => {
                    prop_value = obj.get("temp=");
                    if let Some(_) = prop_value {
                        is_var_ass = true;
                        is_global_var = false;
                    }
                }
            }

            if is_var_ass {
                let var_name = prop_value.unwrap().as_str().unwrap();
                let prop_value = obj.get("re");
                let is_new_decl = prop_value.is_none();

                let var_ass = Rc::new(VariableAssignment::new(var_name, is_new_decl, is_global_var));
                return Ok(var_ass);
            }

            // // Legacy Tag
            // prop_value = obj.get("#");
            // if (prop_value != null) {
            //     return new Tag((String) prop_value);
            // }

            // // List value
            // prop_value = obj.get("list");

            // if (prop_value != null) {
            //     HashMap<String, Object> listContent = (HashMap<String, Object>) prop_value;
            //     InkList rawList = new InkList();

            //     prop_value = obj.get("origins");

            //     if (prop_value != null) {
            //         List<String> namesAsObjs = (List<String>) prop_value;

            //         rawList.setInitialOriginNames(namesAsObjs);
            //     }

            //     for (Entry<String, Object> nameToVal : listContent.entrySet()) {
            //         InkListItem item = new InkListItem(nameToVal.getKey());
            //         int val = (int) nameToVal.getValue();
            //         rawList.put(item, val);
            //     }

            //     return new ListValue(rawList);
            // }

            // Used when serialising save state only
            if obj.get("originalChoicePath").is_some() {
                return jobject_to_choice(obj);
            }

            return Err(format!("Failed to convert token to runtime RTObject: {}", &token.to_string()));
        },
    }

}

fn jarray_to_container(jarray: &Vec<serde_json::Value>, name: Option<String>) -> Result<Rc<dyn RTObject>, String> {
    // Final object in the array is always a combination of
    //  - named content
    //  - a "#f" key with the countFlags
    // (if either exists at all, otherwise null)
    let terminating_obj = jarray[jarray.len() - 1].as_object();
    let mut name: Option<String> = name;
    let mut flags = 0;

    let mut named_only_content: HashMap<String, Rc<Container>> =
            HashMap::new();

    if let Some(terminating_obj) = terminating_obj {
        for (k, v) in terminating_obj {
            match k.as_str() {
                "#f" => flags = v.as_i64().unwrap().try_into().unwrap(),
                "#n" => name = Some(v.as_str().unwrap().to_string()),
                k => {
                    let named_content_item = jtoken_to_runtime_object(v, Some(k.to_string())).unwrap();
                    
                    let named_sub_container = named_content_item.into_any().downcast::<Container>().unwrap();

                    named_only_content.insert(k.to_string(), named_sub_container);
                }
            }
        }

        // TODO container.namedOnlyContent = namedOnlyContent;
    }

    let container = Container::new(name, flags, jarray_to_runtime_obj_list(jarray, true)?, named_only_content);
    Ok(container)
}

fn jarray_to_runtime_obj_list(jarray: &Vec<serde_json::Value>, skip_last: bool) -> Result<Vec<Rc<dyn RTObject>>, String> {
    let mut count = jarray.len();

    if skip_last {
        count -= 1;
    }

    let mut list: Vec<Rc<dyn RTObject>> = Vec::with_capacity(jarray.len());

    for i in 0..count {
        let jtok = &jarray[i];
        let runtime_obj = jtoken_to_runtime_object(jtok, None);
        list.push(runtime_obj?);
    }

    Ok(list)
}

fn jobject_to_choice(obj: &Map<String, serde_json::Value>) -> Result<Rc<dyn RTObject>, String>  {
    let text = obj.get("text").unwrap().as_str().unwrap();
    let index = obj.get("index").unwrap().as_u64().unwrap() as usize;
    let source_path = obj.get("originalChoicePath").unwrap().as_str().unwrap();
    let original_thread_index = obj.get("originalThreadIndex").unwrap().as_i64().unwrap() as usize;
    let path_string_on_choice = obj.get("targetPath").unwrap().as_str().unwrap();

    return Ok(Rc::new(Choice::new_from_json(path_string_on_choice, source_path.to_string(),  text, index, original_thread_index)));
}
