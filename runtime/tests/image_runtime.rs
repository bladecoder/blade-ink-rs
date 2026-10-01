#![cfg(feature = "binary-image")]

use bladeink::story::Story;

const IMAGE: &[u8] = include_bytes!("fixtures/image_smoke.inkb");
#[cfg(any(feature = "serde-json-parser", feature = "stream-json-parser"))]
const JSON: &str = include_str!("../../conformance-tests/inkfiles/test1.ink.json");

#[cfg(any(feature = "serde-json-parser", feature = "stream-json-parser"))]
#[test]
fn embedded_image_runs_without_json_parser_at_construction() {
    let mut image = Story::new_from_image_with_seed(IMAGE, 7).unwrap();
    let mut json = Story::new_with_seed(JSON, 7).unwrap();
    for _ in 0..100 {
        assert_eq!(image.can_continue(), json.can_continue());
        if image.can_continue() {
            assert_eq!(image.cont().unwrap(), json.cont().unwrap());
        } else {
            let image_choices = image.get_current_choices();
            let json_choices = json.get_current_choices();
            assert_eq!(image_choices.len(), json_choices.len());
            for (left, right) in image_choices.iter().zip(json_choices.iter()) {
                assert_eq!(left.text, right.text);
            }
            if image_choices.is_empty() {
                return;
            }
            image.choose_choice_index(0).unwrap();
            json.choose_choice_index(0).unwrap();
        }
    }
    panic!("story did not finish");
}

#[cfg(not(any(feature = "serde-json-parser", feature = "stream-json-parser")))]
#[test]
fn embedded_image_runs_and_restores_state_with_binary_image_only() {
    let mut story = Story::new_from_image_with_seed(IMAGE, 7).unwrap();
    let mut text = String::new();
    while story.can_continue() {
        text.push_str(&story.cont().unwrap());
    }
    assert!(text.contains("Test conditional choices"));
    let choices = story.get_current_choices();
    assert!(!choices.is_empty());
    let saved = story.save_state().unwrap();
    let mut restored = Story::new_from_image_with_seed(IMAGE, 7).unwrap();
    restored.load_state_from_reader(saved.as_bytes()).unwrap();
    assert_eq!(choices.len(), restored.get_current_choices().len());
    restored.choose_choice_index(0).unwrap();
    assert!(restored.cont().unwrap().contains("one"));
}

#[test]
fn image_loader_rejects_truncated_corrupt_and_incompatible_bytes() {
    for cut in [0, 7, 87, 88, IMAGE.len() - 1] {
        assert!(Story::new_from_image_with_seed(&IMAGE[..cut], 1).is_err());
    }
    let mut bad = IMAGE.to_vec();
    bad[8..12].copy_from_slice(&99_u32.to_le_bytes());
    assert!(Story::new_from_image_with_seed(Box::leak(bad.into_boxed_slice()), 1).is_err());

    let mut bad = IMAGE.to_vec();
    bad[88 + 8..88 + 12].copy_from_slice(&255_u32.to_le_bytes());
    assert!(Story::new_from_image_with_seed(Box::leak(bad.into_boxed_slice()), 1).is_err());

    let mut bad = IMAGE.to_vec();
    let strings = u32::from_le_bytes(bad[76..80].try_into().unwrap()) as usize;
    bad[strings] = 0xff;
    assert!(Story::new_from_image_with_seed(Box::leak(bad.into_boxed_slice()), 1).is_err());
}

#[cfg(all(
    feature = "std",
    any(feature = "serde-json-parser", feature = "stream-json-parser")
))]
#[test]
fn image_reads_list_definitions_and_list_values() {
    let json = include_str!("../../conformance-tests/inkfiles/lists/basic-operations.ink.json");
    let bytes = bladeink::image::compile_json_to_image(json.as_bytes()).unwrap();
    let mut story =
        Story::new_from_image_with_seed(Box::leak(bytes.into_boxed_slice()), 1).unwrap();
    let reference = Story::new_with_seed(json, 1).unwrap();
    assert_eq!(
        story.list_from_origin("list").unwrap().get_origin_names(),
        reference
            .list_from_origin("list")
            .unwrap()
            .get_origin_names()
    );
    assert_eq!(
        story.list_from_item("list.a").unwrap().items,
        reference.list_from_item("list.a").unwrap().items
    );
    for _ in 0..20 {
        if !story.can_continue() {
            break;
        }
        story.cont().unwrap();
    }
}
