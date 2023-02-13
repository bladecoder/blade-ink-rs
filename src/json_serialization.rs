use std::collections::HashMap;

use serde_json::Value;

use crate::{
    container::Container,
    ink_value::{BoolValue, FloatValue, IntValue},
    rt_object::{self, RTObject},
};

pub fn jtoken_to_runtime_object(token: &Value) -> Box<dyn RTObject> {
    match token {
        Value::Null => Box::new(rt_object::Null),
        Value::Bool(value) => BoolValue::new(value.clone()),
        Value::Number(_) => {
            if token.is_i64() {
                IntValue::new(token.as_i64().unwrap().try_into().unwrap())
            } else {
                let val: f32 = token.as_f64().unwrap() as f32;
                FloatValue::new(val)
            }
        }
        Value::String(_) => todo!(),
        Value::Array(value) => jarray_to_container(value),
        Value::Object(_) => todo!(),
    }
}

fn jarray_to_container(jarray: &Vec<Value>) -> Box<Container> {
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

    Container::new(container_content, name, flags)
}

fn jarray_to_runtime_obj_list(jarray: &Vec<Value>, skip_last: bool) -> Vec<Box<dyn RTObject>> {
    let mut count = jarray.len();

    if skip_last {
        count -= 1;
    }

    let mut list: Vec<Box<dyn RTObject>> = Vec::with_capacity(jarray.len());

    for i in 0..count {
        let jtok = &jarray[i];
        let runtime_obj = jtoken_to_runtime_object(&jtok);
        list.push(runtime_obj);
    }

    list
}

#[cfg(test)]
mod tests {

    #[test]
    fn simple_test() {}
}
