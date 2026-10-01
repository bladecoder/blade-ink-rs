//! Streaming parser for runtime values in saved Ink state.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::compat::{collections::HashMap, io::Read, rc::Rc};

use crate::{
    choice_point::ChoicePoint, container::Container, control_command::ControlCommand,
    divert::Divert, glue::Glue, ink_list::InkList, ink_list_item::InkListItem,
    native_function_call::NativeFunctionCall, object::RTObject, path::Path, push_pop::PushPopType,
    story_error::StoryError, tag::Tag, value::Value, variable_assignment::VariableAssignment,
    variable_reference::VariableReference, void::Void,
};

use super::json_tokenizer::{JsonTokenizer, JsonValue};

fn read_i32<R: Read>(tok: &mut JsonTokenizer<R>, field: &str) -> Result<i32, StoryError> {
    tok.read_number()?
        .as_integer()
        .ok_or_else(|| StoryError::BadJson(format!("{field} must be an integer")))
}

fn string_value<'a>(value: &'a JsonValue, field: &str) -> Result<&'a str, StoryError> {
    value
        .as_str()
        .ok_or_else(|| StoryError::BadJson(format!("{field} must be a string")))
}

fn integer_value(value: &JsonValue, field: &str) -> Result<i32, StoryError> {
    value
        .as_integer()
        .ok_or_else(|| StoryError::BadJson(format!("{field} must be an integer")))
}

enum ArrayElement {
    RTObject(Rc<dyn RTObject>),
    LastElement(i32, Option<String>, HashMap<String, Rc<Container>>),
    NullElement,
}

type RuntimeObjectList = Vec<Rc<dyn RTObject>>;
type RuntimeObjectListResult = Result<(RuntimeObjectList, Option<ArrayElement>), StoryError>;

pub(crate) fn read_runtime_object<R: Read>(
    tok: &mut JsonTokenizer<R>,
) -> Result<Rc<dyn RTObject>, StoryError> {
    let value = tok.read_value()?;
    match jtoken_to_runtime_object(tok, value, None)? {
        ArrayElement::RTObject(object) => Ok(object),
        ArrayElement::NullElement => Err(StoryError::BadJson(
            "null is not a runtime object".to_owned(),
        )),
        ArrayElement::LastElement(_, _, _) => Err(StoryError::BadJson(
            "container terminator outside a container".to_owned(),
        )),
    }
}

pub(crate) fn read_runtime_object_list<R: Read>(
    tok: &mut JsonTokenizer<R>,
) -> Result<Vec<Rc<dyn RTObject>>, StoryError> {
    tok.expect('[')?;
    let mut objects = Vec::new();
    while tok.peek()? != ']' {
        objects.push(read_runtime_object(tok)?);
        if tok.peek()? != ']' {
            tok.expect(',')?;
        }
    }
    tok.expect(']')?;
    Ok(objects)
}

fn jtoken_to_runtime_object<R: Read>(
    tok: &mut JsonTokenizer<R>,
    value: JsonValue,
    name: Option<String>,
) -> Result<ArrayElement, StoryError> {
    match value {
        JsonValue::Null => Ok(ArrayElement::NullElement),
        JsonValue::Boolean(value) => Ok(ArrayElement::RTObject(Rc::new(Value::new::<bool>(value)))),
        JsonValue::Number(value) => {
            if value.is_integer() {
                let val = value.as_integer().ok_or_else(|| {
                    StoryError::BadJson("integer is outside the supported range".to_owned())
                })?;
                Ok(ArrayElement::RTObject(Rc::new(Value::new::<i32>(val))))
            } else {
                let val: f32 = value.as_float();
                Ok(ArrayElement::RTObject(Rc::new(Value::new::<f32>(val))))
            }
        }
        JsonValue::String(value) => {
            let str = value.as_str();

            // String value
            let first_char = str.chars().next().ok_or_else(|| {
                StoryError::BadJson("empty string is not a runtime object".to_owned())
            })?;
            if first_char == '^' {
                return Ok(ArrayElement::RTObject(Rc::new(Value::new::<&str>(
                    &str[1..],
                ))));
            } else if first_char == '\n' && str.len() == 1 {
                return Ok(ArrayElement::RTObject(Rc::new(Value::new::<&str>("\n"))));
            }

            // Glue
            if "<>".eq(str) {
                return Ok(ArrayElement::RTObject(Rc::new(Glue::new())));
            }

            if let Some(control_command) = ControlCommand::new_from_name(str) {
                return Ok(ArrayElement::RTObject(Rc::new(control_command)));
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
                return Ok(ArrayElement::RTObject(Rc::new(native_function_call)));
            }

            // Void
            if "void".eq(str) {
                return Ok(ArrayElement::RTObject(Rc::new(Void::new())));
            }

            Err(StoryError::BadJson(format!(
                "Failed to convert token to runtime RTObject: {}",
                str
            )))
        }
        JsonValue::Array => Ok(ArrayElement::RTObject(jarray_to_container(tok, name)?)),
        JsonValue::Object => {
            let prop = tok.read_obj_key()?;
            let prop_value = tok.read_value()?;

            // Divert target value to path
            if prop == "^->" {
                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(Rc::new(Value::new::<Path>(
                    Path::new_with_components_string(prop_value.as_str()),
                ))));
            }

            // // VariablePointerValue
            if prop == "^var" {
                let variable_name = string_value(&prop_value, "^var")?;
                let mut contex_index = -1;

                if tok.peek()? == ',' {
                    tok.expect(',')?;
                    tok.expect_obj_key("ci")?;
                    contex_index = read_i32(tok, "ci")?;
                }

                let var_ptr = Rc::new(Value::new_variable_pointer(variable_name, contex_index));
                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(var_ptr));
            }

            // // Divert
            let mut is_divert = false;
            let mut pushes_to_stack = false;
            let mut div_push_type = PushPopType::Function;
            let mut external = false;

            if prop == "->" {
                is_divert = true;
            } else if prop == "f()" {
                is_divert = true;
                pushes_to_stack = true;
                div_push_type = PushPopType::Function;
            } else if prop == "->t->" {
                is_divert = true;
                pushes_to_stack = true;
                div_push_type = PushPopType::Tunnel;
            } else if prop == "x()" {
                is_divert = true;
                external = true;
                pushes_to_stack = false;
                div_push_type = PushPopType::Function;
            }

            if is_divert {
                let target = string_value(&prop_value, &prop)?.to_string();

                let mut var_divert_name: Option<String> = None;
                let mut target_path: Option<String> = None;

                let mut conditional = false;
                let mut external_args = 0;

                while tok.peek()? == ',' {
                    tok.expect(',')?;
                    let prop = tok.read_obj_key()?;
                    let prop_value = tok.read_value()?;

                    // Variable target
                    if prop == "var" {
                        var_divert_name = Some(target.clone());
                    } else if prop == "c" {
                        conditional = true;
                    } else if prop == "exArgs" {
                        external_args = usize::try_from(integer_value(&prop_value, "exArgs")?)
                            .map_err(|_| {
                                StoryError::BadJson("exArgs must be non-negative".to_owned())
                            })?;
                    }
                }

                if var_divert_name.is_none() {
                    target_path = Some(target);
                }

                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(Rc::new(Divert::new(
                    pushes_to_stack,
                    div_push_type,
                    external,
                    external_args,
                    conditional,
                    var_divert_name,
                    target_path.as_deref(),
                ))));
            }

            // Choice
            if prop == "*" {
                let mut flags = 0;
                let path_string_on_choice = string_value(&prop_value, "*")?;

                if tok.peek()? == ',' {
                    tok.expect(',')?;
                    tok.expect_obj_key("flg")?;
                    flags = read_i32(tok, "flg")?;
                }

                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(Rc::new(ChoicePoint::new(
                    flags,
                    path_string_on_choice,
                ))));
            }

            // Variable reference
            if prop == "VAR?" {
                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(Rc::new(VariableReference::new(
                    string_value(&prop_value, "VAR?")?,
                ))));
            }

            if prop == "CNT?" {
                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(Rc::new(
                    VariableReference::from_path_for_count(string_value(&prop_value, "CNT?")?),
                )));
            }

            // Variable assignment
            let mut is_var_ass = false;
            let mut is_global_var = false;

            if prop == "VAR=" {
                is_var_ass = true;
                is_global_var = true;
            } else if prop == "temp=" {
                is_var_ass = true;
                is_global_var = false;
            }

            if is_var_ass {
                let var_name = string_value(&prop_value, &prop)?;
                let mut is_new_decl = true;

                if tok.peek()? == ',' {
                    tok.expect(',')?;
                    tok.expect_obj_key("re")?;
                    let _ = tok.read_boolean()?;
                    is_new_decl = false;
                }

                let var_ass = Rc::new(VariableAssignment::new(
                    var_name,
                    is_new_decl,
                    is_global_var,
                ));
                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(var_ass));
            }

            // // Legacy Tag
            if prop == "#" {
                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(Rc::new(Tag::new(string_value(
                    &prop_value,
                    "#",
                )?))));
            }

            // List value
            if prop == "list" {
                let list_content = parse_list(tok)?;
                let mut raw_list = InkList::new();

                if tok.peek()? == ',' {
                    tok.expect(',')?;
                    tok.expect_obj_key("origins")?;

                    // read array of strings
                    tok.expect('[')?;

                    let mut names = Vec::new();
                    while tok.peek()? != ']' {
                        let name = tok.read_string()?;
                        names.push(name);

                        if tok.peek()? != ']' {
                            tok.expect(',')?;
                        }
                    }

                    tok.expect(']')?;

                    raw_list.set_initial_origin_names(names);
                }

                for (k, v) in list_content {
                    let item = InkListItem::from_full_name(k.as_str());
                    raw_list.items.insert(item, v);
                }

                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(Rc::new(Value::new::<InkList>(
                    raw_list,
                ))));
            }

            // Used when serialising save state only
            if prop == "originalChoicePath" {
                return Err(StoryError::BadJson(
                    "choice object found outside currentChoices".to_owned(),
                ));
            }

            // Last Element
            let mut flags = 0;
            let mut name: Option<String> = None;
            let mut named_only_content: HashMap<String, Rc<Container>> = HashMap::new();

            let mut p = prop.clone();
            let mut pv = prop_value;

            loop {
                if p == "#f" {
                    flags = integer_value(&pv, "#f")?;
                } else if p == "#n" {
                    name = Some(string_value(&pv, "#n")?.to_string());
                } else {
                    let named_content_item = jtoken_to_runtime_object(tok, pv, Some(p.clone()))?;

                    let named_content_item = match named_content_item {
                        ArrayElement::RTObject(rt_obj) => rt_obj,
                        _ => {
                            return Err(StoryError::BadJson(
                                "Named content is not a runtime object".to_owned(),
                            ));
                        }
                    };

                    let named_sub_container = named_content_item
                        .into_any()
                        .downcast::<Container>()
                        .map_err(|_| {
                            StoryError::BadJson(format!("named content '{p}' is not a container"))
                        })?;

                    named_only_content.insert(p, named_sub_container);
                }

                if tok.peek()? == ',' {
                    tok.expect(',')?;
                    p = tok.read_obj_key()?;
                    pv = tok.read_value()?;
                } else if tok.peek()? == '}' {
                    tok.expect('}')?;
                    return Ok(ArrayElement::LastElement(flags, name, named_only_content));
                } else {
                    break;
                }
            }

            Err(StoryError::BadJson(format!(
                "Failed to convert token to runtime RTObject: {}",
                prop
            )))
        }
    }
}

fn parse_list<R: Read>(tok: &mut JsonTokenizer<R>) -> Result<HashMap<String, i32>, StoryError> {
    let mut list_content: HashMap<String, i32> = HashMap::new();

    while tok.peek()? != '}' {
        let key = tok.read_obj_key()?;
        let value = read_i32(tok, &key)?;
        list_content.insert(key, value);

        if tok.peek()? != '}' {
            tok.expect(',')?;
        }
    }

    tok.expect('}')?;

    Ok(list_content)
}

fn jarray_to_container<R: Read>(
    tok: &mut JsonTokenizer<R>,
    name: Option<String>,
) -> Result<Rc<dyn RTObject>, StoryError> {
    let (content, named) = jarray_to_runtime_obj_list(tok)?;

    // Final object in the array is always a combination of
    //  - named content
    //  - a "#f" key with the countFlags

    // (if either exists at all, otherwise null)
    // let terminating_obj = jarray[jarray.len() - 1].as_object();
    let mut name: Option<String> = name;
    let mut flags = 0;
    let mut named_only_content: HashMap<String, Rc<Container>> = HashMap::new();

    if let Some(ArrayElement::LastElement(f, n, named_content)) = named {
        flags = f;

        if n.is_some() {
            name = n;
        }

        named_only_content = named_content;
    }

    let container = Container::new(name, flags, content, named_only_content);
    Ok(container)
}

fn jarray_to_runtime_obj_list<R: Read>(tok: &mut JsonTokenizer<R>) -> RuntimeObjectListResult {
    let mut list: RuntimeObjectList = Vec::new();
    let mut last_element: Option<ArrayElement> = None;

    while tok.peek()? != ']' {
        let val = tok.read_value()?;
        let runtime_obj = jtoken_to_runtime_object(tok, val, None)?;

        match runtime_obj {
            ArrayElement::LastElement(flags, name, named_only_content) => {
                last_element = Some(ArrayElement::LastElement(flags, name, named_only_content));
                break;
            }
            ArrayElement::RTObject(rt_obj) => list.push(rt_obj),
            ArrayElement::NullElement => {
                // Only the last element can be null
                if tok.peek()? != ']' {
                    return Err(StoryError::BadJson(
                        "Only the last element can be null".to_owned(),
                    ));
                }
            }
        }

        if tok.peek()? != ']' {
            tok.expect(',')?;
        }
    }

    tok.expect(']')?;

    Ok((list, last_element))
}
