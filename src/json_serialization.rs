use std::collections::HashMap;

use serde_json::Value;

use crate::{
    container::Container,
    ink_value::{BoolValue, FloatValue, IntValue, StringValue},
    rt_object::{self, RTObject}, control_command::{ControlCommand, self},
};

pub fn jtoken_to_runtime_object(token: &Value) -> Result<Box<dyn RTObject>, String> {
    match token {
        Value::Null =>  Ok(Box::new(rt_object::Null)),
        Value::Bool(value) => Ok(BoolValue::new(value.clone())),
        Value::Number(_) => {
            if token.is_i64() {
                Ok(IntValue::new(token.as_i64().unwrap().try_into().unwrap()))
            } else {
                let val: f32 = token.as_f64().unwrap() as f32;
                Ok(FloatValue::new(val))
            }
        }

        Value::String(value) => {
            let str = value;
            // String value
            let firstChar = str.chars().next().unwrap();
            if firstChar == '^' {return Ok(StringValue::new(str[1..].to_string()));}     
            else if firstChar == '\n' && str.len() == 1 {return Ok(StringValue::new("\n".to_string()));}

            // Glue
            // TODO if "<>".eq(str) {return new Glue();}

            if let Some(controlCommand) = create_control_command(str) {
                return Ok(Box::new(controlCommand));
            }

            /* TODO 

            // Native functions
            // "^" conflicts with the way to identify strings, so now
            // we know it's not a string, we can convert back to the proper
            // symbol for the operator.
            if ("L^".eq(str)) {str = "^";}
            if NativeFunctionCall.callExistsWithName(str) {return NativeFunctionCall.callWithName(str);}

            // Pop
            if ("->->".eq(str)) {return ControlCommand.popTunnel();}
            else if ("~ret".eq(str)) {return ControlCommand.popFunction();}

            // Void
            if ("void".eq(str)) {return new Void();}
            */

            Err("Failed to convert token to runtime RTObject: ".to_string() + &token.to_string())
        },
        Value::Array(value) => Ok(jarray_to_container(value)?),
        Value::Object(_) => todo!(),
    }
}

fn jarray_to_container(jarray: &Vec<Value>) -> Result<Box<Container>, String> {
    let container_content = jarray_to_runtime_obj_list(jarray, true);

    // Final object in the array is always a combination of
    //  - named content
    //  - a "#f" key with the countFlags
    // (if either exists at all, otherwise null)
    let terminatingObj = jarray[jarray.len() - 1].as_object();
    let mut name: Option<String> = None;
    let mut flags = 0;

    if terminatingObj.is_some() {
        let terminatingObj = terminatingObj.unwrap();
        let namedOnlyContent: HashMap<String, Box<dyn RTObject>> =
            HashMap::with_capacity(terminatingObj.len());

        for (k, v) in terminatingObj {
            match k.as_str() {
                "#f" => flags = v.as_i64().unwrap().try_into().unwrap(),
                "#n" => name = Some(v.as_str().unwrap().to_string()),
                _ => {
                    let namedContentItem = jtoken_to_runtime_object(v);
                    /* TODO
                    let namedSubContainer = namedContentItem as Container;
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

    Ok(Container::new(container_content?, name, flags))
}

fn jarray_to_runtime_obj_list(jarray: &Vec<Value>, skip_last: bool) -> Result<Vec<Box<dyn RTObject>>, String> {
    let mut count = jarray.len();

    if skip_last {
        count -= 1;
    }

    let mut list: Vec<Box<dyn RTObject>> = Vec::with_capacity(jarray.len());

    for i in 0..count {
        let jtok = &jarray[i];
        let runtime_obj = jtoken_to_runtime_object(&jtok);
        list.push(runtime_obj?);
    }

    Ok(list)
}

fn create_control_command(name: &str ) -> Option<ControlCommand> {
    let result = match name {
        "ev" => Some(ControlCommand::EvalStart),
        "out" => Some(ControlCommand::EvalOutput),
        "/ev" => Some(ControlCommand::EvalEnd),
        "du" => Some(ControlCommand::Duplicate),
        "pop" => Some(ControlCommand::PopEvaluatedValue),
        "~ret" => Some(ControlCommand::PopFunction),
        "->->" => Some(ControlCommand::PopTunnel),
        "str" => Some(ControlCommand::BeginString),
        "/str" => Some(ControlCommand::EndString),
        "nop" => Some(ControlCommand::NoOp),
        "choiceCnt" => Some(ControlCommand::ChoiceCount),
        "turn" => Some(ControlCommand::Turns),
        "turns" => Some(ControlCommand::TurnsSince),
        "readc" => Some(ControlCommand::ReadCount),
        "rnd" => Some(ControlCommand::Random),
        "srnd" => Some(ControlCommand::SeedRandom),
        "visit" => Some(ControlCommand::VisitIndex),
        "seq" => Some(ControlCommand::SequenceShuffleIndex),
        "thread" => Some(ControlCommand::StartThread),
        "done" => Some(ControlCommand::Done),
        "end" => Some(ControlCommand::End),
        "listInt" => Some(ControlCommand::ListFromInt),
        "range" => Some(ControlCommand::ListRange),
        "lrnd" => Some(ControlCommand::ListRandom),
        "#" => Some(ControlCommand::BeginTag),
        "/#" => Some(ControlCommand::EndTag),
        _ => None,
    };

    result
}

#[cfg(test)]
mod tests {

    #[test]
    fn simple_test() {}
}
