//! This is a JSON parser that process the JSON in a streaming fashion. It can be used as a replacement for the Serde based parser.
//! This is useful for large JSON files that don't fit in memory hence the JSON is not loaded all at once as Serde does.
//! This parser has been used to load 'The Intercept' example story in an ESP32-s2 microcontroller with an external RAM of 2MB. With the Serde based parser, it is impossible, it does not have enogh memory to load the story.

use std::{collections::HashMap, rc::Rc};

use crate::{
    choice_point::ChoicePoint,
    container::Container,
    control_command::ControlCommand,
    divert::Divert,
    glue::Glue,
    ink_list::InkList,
    ink_list_item::InkListItem,
    list_definition::ListDefinition,
    list_definitions_origin::ListDefinitionsOrigin,
    native_function_call::NativeFunctionCall,
    object::RTObject,
    path::Path,
    push_pop::PushPopType,
    story::{INK_VERSION_CURRENT, INK_VERSION_MINIMUM_COMPATIBLE},
    story_error::StoryError,
    tag::Tag,
    value::Value,
    variable_assigment::VariableAssignment,
    variable_reference::VariableReference,
    void::Void,
};

use super::json_tokenizer::{JsonTokenizer, JsonValue};

pub fn load_from_string(
    s: &str,
) -> Result<(i32, Rc<Container>, Rc<ListDefinitionsOrigin>), StoryError> {
    let mut tok = JsonTokenizer::new_from_str(s);

    parse(&mut tok)
}

fn parse(
    tok: &mut JsonTokenizer,
) -> Result<(i32, Rc<Container>, Rc<ListDefinitionsOrigin>), StoryError> {
    tok.expect('{')?;

    let version_key = tok.read_obj_key()?;

    if version_key != "inkVersion" {
        return Err(StoryError::BadJson(
            "ink version number not found. Are you sure it's a valid .ink.json file?".to_owned(),
        ));
    }

    let version: i32 = tok.read_number().unwrap().as_integer().unwrap();

    if version > INK_VERSION_CURRENT {
        return Err(StoryError::BadJson(
            "Version of ink used to build story was newer than the current version of the engine"
                .to_owned(),
        ));
    } else if version < INK_VERSION_MINIMUM_COMPATIBLE {
        return Err(StoryError::BadJson(
            "Version of ink used to build story is too old to be loaded by this version of the engine".to_owned(),
        ));
    }

    tok.expect(',')?;

    let root_key = tok.read_obj_key()?;

    if root_key != "root" {
        return Err(StoryError::BadJson(
            "Root node for ink not found. Are you sure it's a valid .ink.json file?".to_owned(),
        ));
    }

    let root_value = tok.read_value()?;
    let main_content_container = match jtoken_to_runtime_object(tok, root_value, None)? {
        ArrayElement::RTObject(rt_obj) => rt_obj,
        _ => {
            return Err(StoryError::BadJson(
                "Root node for ink is not a container?".to_owned(),
            ))
        }
    };

    let main_content_container = main_content_container.into_any().downcast::<Container>();

    if main_content_container.is_err() {
        return Err(StoryError::BadJson(
            "Root node for ink is not a container?".to_owned(),
        ));
    };

    let main_content_container = main_content_container.unwrap(); // unwrap: checked for err above

    tok.expect(',')?;
    let list_defs_key = tok.read_obj_key()?;

    if list_defs_key != "listDefs" {
        return Err(StoryError::BadJson(
            "List Definitions node for ink not found. Are you sure it's a valid .ink.json file?"
                .to_owned(),
        ));
    }

    let list_defs = Rc::new(jtoken_to_list_definitions(tok)?);

    tok.expect('}')?;

    Ok((version, main_content_container, list_defs))
}

enum ArrayElement {
    RTObject(Rc<dyn RTObject>),
    LastElement(i32, Option<String>, HashMap<String, Rc<Container>>),
    NullElement,
}

fn jtoken_to_runtime_object(
    tok: &mut JsonTokenizer,
    value: JsonValue,
    name: Option<String>,
) -> Result<ArrayElement, StoryError> {
    match value {
        JsonValue::Null => Ok(ArrayElement::NullElement),
        JsonValue::Boolean(value) => Ok(ArrayElement::RTObject(Rc::new(Value::new_bool(value)))),
        JsonValue::Number(value) => {
            if value.is_integer() {
                let val: i32 = value.as_integer().unwrap();
                Ok(ArrayElement::RTObject(Rc::new(Value::new_int(val))))
            } else {
                let val: f32 = value.as_float().unwrap();
                Ok(ArrayElement::RTObject(Rc::new(Value::new_float(val))))
            }
        }
        JsonValue::String(value) => {
            let str = value.as_str();

            // String value
            let first_char = str.chars().next().unwrap();
            if first_char == '^' {
                return Ok(ArrayElement::RTObject(Rc::new(Value::new_string(
                    &str[1..],
                ))));
            } else if first_char == '\n' && str.len() == 1 {
                return Ok(ArrayElement::RTObject(Rc::new(Value::new_string("\n"))));
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
                return Ok(ArrayElement::RTObject(Rc::new(Value::new_divert_target(
                    Path::new_with_components_string(prop_value.as_str()),
                ))));
            }

            // // VariablePointerValue
            if prop == "^var" {
                let variable_name = prop_value.as_str().unwrap();
                let mut contex_index = -1;

                if tok.peek()? == ',' {
                    tok.expect(',')?;
                    tok.expect_obj_key("ci")?;
                    contex_index = tok.read_number().unwrap().as_integer().unwrap();
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
                let target = prop_value.as_str().unwrap().to_string();

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
                        external_args = prop_value.as_integer().unwrap() as usize;
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
                let path_string_on_choice = prop_value.as_str().unwrap();

                if tok.peek()? == ',' {
                    tok.expect(',')?;
                    tok.expect_obj_key("flg")?;
                    flags = tok.read_number().unwrap().as_integer().unwrap();
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
                    prop_value.as_str().unwrap(),
                ))));
            }

            if prop == "CNT?" {
                tok.expect('}')?;
                return Ok(ArrayElement::RTObject(Rc::new(
                    VariableReference::from_path_for_count(prop_value.as_str().unwrap()),
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
                let var_name = prop_value.as_str().unwrap();
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
                return Ok(ArrayElement::RTObject(Rc::new(Tag::new(
                    prop_value.as_str().unwrap(),
                ))));
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
                return Ok(ArrayElement::RTObject(Rc::new(Value::new_list(raw_list))));
            }

            // Used when serialising save state only
            if prop == "originalChoicePath" {
                todo!("originalChoicePath");
                // return jobject_to_choice(obj); // TODO
            }

            // Last Element
            let mut flags = 0;
            let mut name: Option<String> = None;
            let mut named_only_content: HashMap<String, Rc<Container>> = HashMap::new();

            let mut p = prop.clone();
            let mut pv = prop_value;

            loop {
                if p == "#f" {
                    flags = pv.as_integer().unwrap();
                } else if p == "#n" {
                    name = Some(pv.as_str().unwrap().to_string());
                } else {
                    let named_content_item = jtoken_to_runtime_object(tok, pv, Some(p.clone()))?;

                    let named_content_item = match named_content_item {
                        ArrayElement::RTObject(rt_obj) => rt_obj,
                        _ => {
                            return Err(StoryError::BadJson(
                                "Named content is not a runtime object".to_owned(),
                            ))
                        }
                    };

                    let named_sub_container = named_content_item
                        .into_any()
                        .downcast::<Container>()
                        .unwrap();

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

fn parse_list(tok: &mut JsonTokenizer) -> Result<HashMap<String, i32>, StoryError> {
    let mut list_content: HashMap<String, i32> = HashMap::new();

    while tok.peek()? != '}' {
        let key = tok.read_obj_key()?;
        let value = tok.read_number().unwrap().as_integer().unwrap();
        list_content.insert(key, value);

        if tok.peek()? != '}' {
            tok.expect(',')?;
        }
    }

    tok.expect('}')?;

    Ok(list_content)
}

fn jarray_to_container(
    tok: &mut JsonTokenizer,
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

fn jarray_to_runtime_obj_list(
    tok: &mut JsonTokenizer,
) -> Result<(Vec<Rc<dyn RTObject>>, Option<ArrayElement>), StoryError> {
    let mut list: Vec<Rc<dyn RTObject>> = Vec::new();
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

fn jtoken_to_list_definitions(
    tok: &mut JsonTokenizer,
) -> Result<ListDefinitionsOrigin, StoryError> {
    let mut all_defs: Vec<ListDefinition> = Vec::with_capacity(0);

    tok.expect('{')?;

    while tok.peek()? != '}' {
        let name = tok.read_obj_key()?;
        tok.expect('{')?;

        let items = parse_list(tok)?;
        let def = ListDefinition::new(name, items);
        all_defs.push(def);

        if tok.peek()? != '}' {
            tok.expect(',')?;
        }
    }

    tok.expect('}')?;

    Ok(ListDefinitionsOrigin::new(&mut all_defs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_load() {
        let s = r##"{"inkVersion":21,"root":[["^Line.","\n",["done",{"#n":"g-0"}],null],"done",null],"listDefs":{}}"##;
        let _ = load_from_string(s).unwrap();
    }

    #[test]
    fn load_list() {
        let s = r##"
        {
            "inkVersion": 21,
            "root": [
                [
                    "ev",
                    {
                        "VAR?": "A"
                    },
                    {
                        "VAR?": "B"
                    },
                    "+",
                    "LIST_ALL",
                    "out",
                    "/ev",
                    "\n",
                    [
                        "done",
                        {
                            "#f": 5,
                            "#n": "g-0"
                        }
                    ],
                    null
                ],
                "done",
                {
                    "global decl": [
                        "ev",
                        {
                            "list": {},
                            "origins": [
                                "a"
                            ]
                        },
                        {
                            "VAR=": "a"
                        },
                        {
                            "list": {},
                            "origins": [
                                "b"
                            ]
                        },
                        {
                            "VAR=": "b"
                        },
                        "/ev",
                        "end",
                        null
                    ],
                    "#f": 1
                }
            ],
            "listDefs": {
                "a": {
                    "A": 1
                },
                "b": {
                    "B": 1
                }
            }
        }
        "##;
        let _ = load_from_string(s).unwrap();
    }

    #[test]
    fn load_choice() {
        let s = r##"{"inkVersion":21,"root":[["^Hello world!","\n","ev","str","^Hello back!","/str","/ev",{"*":"0.c-0","flg":20},{"c-0":["\n","done",{"->":"0.g-0"},{"#f":5}],"g-0":["done",null]}],"done",null],"listDefs":{}}"##;
        let (_, container, _) = load_from_string(s).unwrap();
        let mut sb = String::new();
        container.build_string_of_hierarchy(&mut sb, 0, None);
        println!("{}", sb);
    }

    #[test]
    fn load_iffalse() {
        let s = r##"{"inkVersion":21,"root":[["ev",{"VAR?":"x"},0,">","/ev",[{"->":".^.b","c":true},{"b":["\n","ev",{"VAR?":"x"},1,"-","/ev",{"VAR=":"y","re":true},{"->":"0.6"},null]}],"nop","\n","^The value is ","ev",{"VAR?":"y"},"out","/ev","^. ","end","\n",["done",{"#n":"g-0"}],null],"done",{"global decl":["ev",0,{"VAR=":"x"},3,{"VAR=":"y"},"/ev","end",null]}],"listDefs":{}}"##;
        let (_, container, _) = load_from_string(s).unwrap();
        let mut sb = String::new();
        container.build_string_of_hierarchy(&mut sb, 0, None);
        println!("{}", sb);
    }
}
