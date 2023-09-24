use bladeink::story::Story;


mod common;

#[test]
fn no_choice_test() -> Result<(), String>  {
    let mut errors:Vec<String> = Vec::new();

    let text = common::run_story("examples/inkfiles/choices/no-choice-text.ink.json", None, &mut errors)?;

    assert_eq!(0, errors.len());
    assert_eq!("Hello world!\nHello back!\n", common::join_text(&text));

    Ok(())
}

#[test]
fn one_test() -> Result<(), String>  {
    let mut errors:Vec<String> = Vec::new();

    let text = common::run_story("examples/inkfiles/choices/one.ink.json", None, &mut errors)?;

    assert_eq!(0, errors.len());
    assert_eq!("Hello world!\nHello back!\nHello back!\n", common::join_text(&text));

    Ok(())
}

#[test]
fn multi_choice_test() -> Result<(), String>  {
    let mut errors:Vec<String> = Vec::new();

    let text = common::run_story("examples/inkfiles/choices/multi-choice.ink.json", Some(vec![0]), &mut errors)?;

    assert_eq!(0, errors.len());
    assert_eq!("Hello, world!\nHello back!\nGoodbye\nHello back!\nNice to hear from you\n", common::join_text(&text));

    // Select second choice
    let text = common::run_story("examples/inkfiles/choices/multi-choice.ink.json", Some(vec![1]), &mut errors)?;

    assert_eq!(0, errors.len());
    assert_eq!("Hello, world!\nHello back!\nGoodbye\nGoodbye\nSee you later\n", common::join_text(&text));

    Ok(())
}

#[test]
fn single_choice1_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/single-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("Hello, world!", text[0]);

    Ok(())
}

#[test]
fn single_choic2_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/single-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("Hello back!", text[0]);
    assert_eq!("Nice to hear from you", text[1]);

    Ok(())
}

#[test]
fn suppress_choice_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/suppress-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Hello back!", story.get_current_choices().get(0).unwrap().text);
    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("Nice to hear from you.", text[0]);


    Ok(())
}

#[test]
fn mixed_choice_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/mixed-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Hello back!", story.get_current_choices().get(0).unwrap().text);
    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("Hello right back to you!", text[0]);
    assert_eq!("Nice to hear from you.", text[1]);


    Ok(())
}

#[test]
fn varying_choice_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/varying-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, story.get_current_choices().len());
    assert_eq!("The man with the briefcase?", story.get_current_choices()[0].text);


    Ok(())
}

#[test]
fn sticky_choice_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/sticky-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, story.get_current_choices().len());

    Ok(())
}

#[test]
fn fallback_choice_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/fallback-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());

    Ok(())
}

#[test]
fn fallback_choice2_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/fallback-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(true, common::is_ended(&story));

    Ok(())
}

#[test]
fn conditional_choice_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/conditional-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!(4, story.get_current_choices().len());
    
    Ok(())
}

#[test]
fn label_flow_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/label-flow.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, story.get_current_choices().len());
    assert_eq!("\'Having a nice day?\'",story.get_current_choices()[0].text);

    Ok(())
}

#[test]
fn label_flow2_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/label-flow.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(1);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, story.get_current_choices().len());
    assert_eq!("Shove him aside",story.get_current_choices()[1].text);

    Ok(())
}

#[test]
fn label_scope_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/label-scope.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, story.get_current_choices().len());
    assert_eq!("Found gatherpoint",story.get_current_choices()[0].text);

    Ok(())
}

#[test]
fn divert_choice_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/choices/divert-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("You pull a face, and the soldier comes at you! You shove the guard to one side, but he comes back swinging.", text[0]);
    
    assert_eq!(1, story.get_current_choices().len());
    assert_eq!("Grapple and fight",story.get_current_choices()[0].text);

    Ok(())
}