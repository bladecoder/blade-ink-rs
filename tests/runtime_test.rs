use bladeink::{story::Story, value_type::ValueType};

mod common;

#[test]
fn set_and_get_variable_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/runtime/set-get-variables.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(10, story.get_variables_state().get("x").unwrap().get_int().unwrap());

    story.get_variables_state_mut().set("x", ValueType::Int(15));

    assert_eq!(15, story.get_variables_state().get("x").unwrap().get_int().unwrap());

    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("OK", text[0]);

    Ok(())
}


// TODO external functions + variable observers

