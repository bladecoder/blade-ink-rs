#![allow(dead_code)]

pub mod frontend;
pub mod parsed_hierarchy;
pub mod runtime_export;

use crate::error::CompilerError;

pub fn compile(_source: &str) -> Result<String, CompilerError> {
    Err(CompilerError::UnsupportedFeature(
        "ported compiler backend is not implemented yet".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use crate::parser::Parser;
    use bladeink::story::Story as RuntimeStory;

    use super::{
        parsed_hierarchy::{
            choice::Choice,
            content_list::{ContentItem, ContentList},
            story::Story,
        },
        runtime_export::serialize_root_container,
    };

    #[test]
    fn single_choice_story_matches_fixture_shape() {
        let story = Story {
            leading_content: ContentList::new(vec![
                ContentItem::Text("Hello, world!".to_owned()),
                ContentItem::Newline,
            ]),
            choices: vec![Choice {
                start_content: Some(ContentList::new(vec![ContentItem::Text(
                    "Hello back!".to_owned(),
                )])),
                choice_only_content: None,
                inner_content: ContentList::new(vec![
                    ContentItem::Newline,
                    ContentItem::Text("Nice to hear from you".to_owned()),
                    ContentItem::Newline,
                    ContentItem::End,
                ]),
                once_only: true,
                is_invisible_default: false,
            }],
            continuation_content: ContentList::default(),
            named_flows: Vec::new(),
        };

        let (root, named) = story.to_runtime_story_root().unwrap();
        let compiled = serialize_root_container(root.as_ref(), &named).unwrap();
        let mut runtime_story = RuntimeStory::new(&compiled).unwrap();

        let first_line = runtime_story.cont().unwrap();
        assert_eq!("Hello, world!\n", first_line);
        assert_eq!(1, runtime_story.get_current_choices().len());
        assert_eq!("Hello back!", runtime_story.get_current_choices()[0].text);

        runtime_story.choose_choice_index(0).unwrap();
        let second_line = runtime_story.cont().unwrap();
        let third_line = runtime_story.cont().unwrap();

        assert_eq!("Hello back!\n", second_line);
        assert_eq!("Nice to hear from you\n", third_line);
    }

    #[test]
    fn single_choice_legacy_story_adapts_to_ported_runtime_choice() {
        let source = include_str!("../../../conformance-tests/inkfiles/choices/single-choice.ink");
        let parsed = Parser::new(source).parse().unwrap();
        let story = Story::from_legacy(&parsed).unwrap();

        let (root, named) = story.to_runtime_story_root().unwrap();
        let compiled = serialize_root_container(root.as_ref(), &named).unwrap();
        let mut runtime_story = RuntimeStory::new(&compiled).unwrap();

        assert_eq!("Hello, world!\n", runtime_story.cont().unwrap());
        assert_eq!(1, runtime_story.get_current_choices().len());
        assert_eq!("Hello back!", runtime_story.get_current_choices()[0].text);

        runtime_story.choose_choice_index(0).unwrap();
        assert_eq!("Hello back!\n", runtime_story.cont().unwrap());
        assert_eq!("Nice to hear from you\n", runtime_story.cont().unwrap());
    }

    #[test]
    fn mixed_choice_legacy_story_adapts_to_ported_runtime_choice() {
        let source = include_str!("../../../conformance-tests/inkfiles/choices/mixed-choice.ink");
        let parsed = Parser::new(source).parse().unwrap();
        let story = Story::from_legacy(&parsed).unwrap();

        let (root, named) = story.to_runtime_story_root().unwrap();
        let compiled = serialize_root_container(root.as_ref(), &named).unwrap();
        let mut runtime_story = RuntimeStory::new(&compiled).unwrap();

        assert_eq!("Hello world!\n", runtime_story.cont().unwrap());
        assert_eq!(1, runtime_story.get_current_choices().len());
        assert_eq!("Hello back!", runtime_story.get_current_choices()[0].text);

        runtime_story.choose_choice_index(0).unwrap();
        assert_eq!("Hello right back to you!\n", runtime_story.cont().unwrap());
        assert_eq!("Nice to hear from you.\n", runtime_story.cont().unwrap());
    }

    #[test]
    fn fallback_choice_legacy_story_adapts_to_ported_runtime_choice() {
        let source =
            include_str!("../../../conformance-tests/inkfiles/choices/fallback-choice.ink");
        let parsed = Parser::new(source).parse().unwrap();
        let story = Story::from_legacy(&parsed).unwrap();

        let (root, named) = story.to_runtime_story_root().unwrap();
        let compiled = serialize_root_container(root.as_ref(), &named).unwrap();
        let mut runtime_story = RuntimeStory::new(&compiled).unwrap();

        assert_eq!(
            "You search desperately for a friendly face in the crowd.\n",
            runtime_story.cont().unwrap()
        );
        assert_eq!(2, runtime_story.get_current_choices().len());
        assert_eq!(
            "The woman in the hat?",
            runtime_story.get_current_choices()[0].text
        );
        assert_eq!(
            "The man with the briefcase?",
            runtime_story.get_current_choices()[1].text
        );

        runtime_story.choose_choice_index(0).unwrap();
        assert_eq!(
            "The woman in the hat pushes you roughly aside.\n",
            runtime_story.cont().unwrap()
        );
        assert_eq!(
            "You search desperately for a friendly face in the crowd.\n",
            runtime_story.cont().unwrap()
        );
    }

    #[test]
    fn root_fallback_choice_runtime_shape_works() {
        let story = Story {
            leading_content: ContentList::new(vec![
                ContentItem::Text(
                    "You search desperately for a friendly face in the crowd.".to_owned(),
                ),
                ContentItem::Newline,
            ]),
            choices: vec![
                Choice {
                    start_content: Some(ContentList::new(vec![ContentItem::Text(
                        "The woman in the hat".to_owned(),
                    )])),
                    choice_only_content: Some(ContentList::new(vec![ContentItem::Text(
                        "?".to_owned(),
                    )])),
                    inner_content: ContentList::new(vec![
                        ContentItem::Text(" pushes you roughly aside. ".to_owned()),
                        ContentItem::Divert("0".to_owned()),
                    ]),
                    once_only: true,
                    is_invisible_default: false,
                },
                Choice {
                    start_content: Some(ContentList::new(vec![ContentItem::Text(
                        "The man with the briefcase".to_owned(),
                    )])),
                    choice_only_content: Some(ContentList::new(vec![ContentItem::Text(
                        "?".to_owned(),
                    )])),
                    inner_content: ContentList::new(vec![
                        ContentItem::Text(" looks disgusted as you stumble past him. ".to_owned()),
                        ContentItem::Divert("0".to_owned()),
                    ]),
                    once_only: true,
                    is_invisible_default: false,
                },
                Choice {
                    start_content: None,
                    choice_only_content: None,
                    inner_content: ContentList::new(vec![]),
                    once_only: true,
                    is_invisible_default: true,
                },
            ],
            continuation_content: ContentList::new(vec![
                ContentItem::Text(
                    "But it is too late: you collapse onto the station platform. This is the end."
                        .to_owned(),
                ),
                ContentItem::Newline,
                ContentItem::End,
            ]),
            named_flows: Vec::new(),
        };

        let (root, named) = story.to_runtime_story_root().unwrap();
        let compiled = serialize_root_container(root.as_ref(), &named).unwrap();
        let mut runtime_story = RuntimeStory::new(&compiled).unwrap();

        assert_eq!(
            "You search desperately for a friendly face in the crowd.\n",
            runtime_story.cont().unwrap()
        );
        assert_eq!(2, runtime_story.get_current_choices().len());
    }
}
