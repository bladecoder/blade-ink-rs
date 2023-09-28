use bladeink::story::Story;

mod common;

#[test]
fn list_basic_operations_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/basic-operations.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("b, d\na, b, c, e\nb, c\nfalse\ntrue\ntrue\n", &story.continue_maximally()?);

    Ok(())
}

#[test]
fn list_mixed_items_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/list-mixed-items.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("a, y, c\n", &story.continue_maximally()?);

    Ok(())
}

#[test]
fn more_list_operations_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/more-list-operations.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("1\nl\nn\nl, m\nn\n", &story.continue_maximally()?);

    Ok(())
}

#[test]
fn empty_list_origin_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/empty-list-origin.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("a, b\n", &story.continue_maximally()?);

    Ok(())
}

#[test]
fn list_save_load_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/list-save-load.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    //TODO

    Ok(())
}

#[test]
fn empty_list_origin_after_assinment_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/empty-list-origin-after-assignment.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("a, b, c\n", &story.continue_maximally()?);

    Ok(())
}

#[test]
fn list_range_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/list-range.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("Pound, Pizza, Euro, Pasta, Dollar, Curry, Paella\nEuro, Pasta, Dollar, Curry\nTwo, Three, Four, Five, Six\nPizza, Pasta\n", &story.continue_maximally()?);

    Ok(())
}

#[test]
fn list_bug_adding_element_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/bug-adding-element.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("", &story.continue_maximally()?);

    story.choose_choice_index(0);
    assert_eq!("a\n", &story.continue_maximally()?);

    story.choose_choice_index(1);
    assert_eq!("OK\n", &story.continue_maximally()?);

    Ok(())
}

#[test]
fn more_list_operations2_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/more-list-operations2.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("a1, b1, c1\na1\na1, b2\ncount:2\nmax:c2\nmin:a1\ntrue\ntrue\nfalse\nempty\na2\na2, b2, c2\nrange:a1, b2\na1\nsubtract:a1, c1\nrandom:c2\n", &story.continue_maximally()?);

    Ok(())
}
