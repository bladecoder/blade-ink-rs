use std::{collections::HashMap, rc::Rc, cell::RefCell};

use crate::{
    container::Container,
    object::{self, RTObject}, control_command::{CommandType, ControlCommand}, value::Value, object_enum::ObjectEnum, glue::Glue,
};

pub fn jtoken_to_runtime_object(token: &serde_json::Value) -> Result<Rc<dyn RTObject>, String> {
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
        }

        serde_json::Value::String(value) => {
            let str = value;
            // String value
            let first_char = str.chars().next().unwrap();
            if first_char == '^' {return Ok(Rc::new(Value::new_string(&str[1..])));}     
            else if first_char == '\n' && str.len() == 1 {return Ok(Rc::new(Value::new_string("\n")));}

            // Glue
            if "<>".eq(str) {
                return  Ok(Rc::new(Glue::new()));
            }

            if let Some(control_command) = create_control_command(str) {
                return Ok(Rc::new(control_command));
            }

            /* TODO 

            // Native functions
            // "^" conflicts with the way to identify strings, so now
            // we know it's not a string, we can convert back to the proper
            // symbol for the operator.
            if ("L^".eq(str)) {str = "^";}
            if NativeFunctionCall.callExistsWithName(str) {return NativeFunctionCall.callWithName(str);}

            // Pop
            if ("->->".eq(str)) {return CommandType.popTunnel();}
            else if ("~ret".eq(str)) {return CommandType.popFunction();}

            // Void
            if ("void".eq(str)) {return new Void();}
            */

            Err("Failed to convert token to runtime RTObject: ".to_string() + &token.to_string())
        },
        serde_json::Value::Array(value) => Ok(jarray_to_container(value)?),
        serde_json::Value::Object(_) => todo!(),
    }
}

fn jarray_to_container(jarray: &Vec<serde_json::Value>) -> Result<Rc<Container>, String> {
    // Final object in the array is always a combination of
    //  - named content
    //  - a "#f" key with the countFlags
    // (if either exists at all, otherwise null)
    let terminating_obj = jarray[jarray.len() - 1].as_object();
    let mut name: Option<String> = None;
    let mut flags = 0;

    if let Some(terminating_obj) = terminating_obj {
        let named_only_content: HashMap<String, Box<dyn RTObject>> =
            HashMap::with_capacity(terminating_obj.len());

        for (k, v) in terminating_obj {
            match k.as_str() {
                "#f" => flags = v.as_i64().unwrap().try_into().unwrap(),
                "#n" => name = Some(v.as_str().unwrap().to_string()),
                _ => {
                    let named_content_item = jtoken_to_runtime_object(v);
                    /* TODO
                    let namedSubContainer = named_content_item as Container;
                    if namedSubContainer {
                        namedSubContainer.name = k;
                    }
                    
                    namedOnlyContent[k] = namedContentItem;
                    */
                }
            }
        }

        // TODO container.namedOnlyContent = namedOnlyContent;
    }

    let container = Container::new(name, flags, jarray_to_runtime_obj_list(jarray, true)?);
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
        let runtime_obj = jtoken_to_runtime_object(jtok);
        list.push(runtime_obj?);
    }

    Ok(list)
}

fn create_control_command(name: &str) -> Option<ControlCommand> {
    match name {
        "ev" => Some(ControlCommand::new(CommandType::EvalStart)),
        "out" => Some(ControlCommand::new(CommandType::EvalOutput)),
        "/ev" => Some(ControlCommand::new(CommandType::EvalEnd)),
        "du" => Some(ControlCommand::new(CommandType::Duplicate)),
        "pop" => Some(ControlCommand::new(CommandType::PopEvaluatedValue)),
        "~ret" => Some(ControlCommand::new(CommandType::PopFunction)),
        "->->" => Some(ControlCommand::new(CommandType::PopTunnel)),
        "str" => Some(ControlCommand::new(CommandType::BeginString)),
        "/str" => Some(ControlCommand::new(CommandType::EndString)),
        "nop" => Some(ControlCommand::new(CommandType::NoOp)),
        "choiceCnt" => Some(ControlCommand::new(CommandType::ChoiceCount)),
        "turn" => Some(ControlCommand::new(CommandType::Turns)),
        "turns" => Some(ControlCommand::new(CommandType::TurnsSince)),
        "readc" => Some(ControlCommand::new(CommandType::ReadCount)),
        "rnd" => Some(ControlCommand::new(CommandType::Random)),
        "srnd" => Some(ControlCommand::new(CommandType::SeedRandom)),
        "visit" => Some(ControlCommand::new(CommandType::VisitIndex)),
        "seq" => Some(ControlCommand::new(CommandType::SequenceShuffleIndex)),
        "thread" => Some(ControlCommand::new(CommandType::StartThread)),
        "done" => Some(ControlCommand::new(CommandType::Done)),
        "end" => Some(ControlCommand::new(CommandType::End)),
        "listInt" => Some(ControlCommand::new(CommandType::ListFromInt)),
        "range" => Some(ControlCommand::new(CommandType::ListRange)),
        "lrnd" => Some(ControlCommand::new(CommandType::ListRandom,)),
        "#" => Some(ControlCommand::new(CommandType::BeginTag)),
        "/#" => Some(ControlCommand::new(CommandType::EndTag)),
        _ => None,
    }

}

#[cfg(test)]
mod tests {

    #[test]
    fn simple_test() {}
}
