use bink::{story::Story, story_error::StoryError};

mod common;

#[test]
fn tags_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/tags/tags.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;

    let global_tags = story.get_global_tags()?;
    assert_eq!(2, global_tags.len());
    assert_eq!("author: Joe", global_tags[0]);
    assert_eq!("title: My Great Story", global_tags[1]);

    assert_eq!("This is the content\n", story.cont()?);

    let current_tags = story.get_current_tags()?;
    assert_eq!(2, current_tags.len());
    assert_eq!("author: Joe", current_tags[0]);
    assert_eq!("title: My Great Story", current_tags[1]);

    let current_tags = story.tags_for_content_at_path("knot")?;
    assert_eq!(1, current_tags.len());
    assert_eq!("knot tag", current_tags[0]);

    let current_tags = story.tags_for_content_at_path("knot.stitch")?;
    assert_eq!(1, current_tags.len());
    assert_eq!("stitch tag", current_tags[0]);

    story.choose_path_string("knot", false, None)?;
    assert_eq!("Knot content\n", story.cont()?);
    let current_tags = story.get_current_tags()?;
    assert_eq!(1, current_tags.len());
    assert_eq!("knot tag", current_tags[0]);

    assert_eq!("", story.cont()?);
    let current_tags = story.get_current_tags()?;
    assert_eq!("end of knot tag", current_tags[0]);

    Ok(())
}

#[test]
fn tags_in_seq_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/tags/tagsInSeq.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("A red sequence.\n", story.cont()?);
    let current_tags = story.get_current_tags()?;
    assert_eq!(1, current_tags.len());
    assert_eq!("red", current_tags[0]);

    assert_eq!("A white sequence.\n", story.cont()?);
    let current_tags = story.get_current_tags()?;
    assert_eq!(1, current_tags.len());
    assert_eq!("white", current_tags[0]);

    Ok(())
}

#[test]
fn tags_in_choice_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/tags/tagsInChoice.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;

    story.cont()?;
    let current_tags = story.get_current_tags()?;
    assert_eq!(0, current_tags.len());
    assert_eq!(1, story.get_current_choices().len());
    assert_eq!(2, story.get_current_choices()[0].tags.len());
    assert_eq!("one", story.get_current_choices()[0].tags[0]);
    assert_eq!("two", story.get_current_choices()[0].tags[1]);

    story.choose_choice_index(0)?;

    assert_eq!("one three", story.cont()?);
    let current_tags = story.get_current_tags()?;
    assert_eq!(2, current_tags.len());
    assert_eq!("one", current_tags[0]);
    assert_eq!("three", current_tags[1]);


    Ok(())
}

#[test]
fn tags_dynamic_content_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/tags/tagsDynamicContent.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("tag\n", story.cont()?);
    let current_tags = story.get_current_tags()?;
    assert_eq!(1, current_tags.len());
    assert_eq!("pic8red.jpg", current_tags[0]);

    Ok(())
}

