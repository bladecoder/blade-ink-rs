use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

#[test]
fn fun_basic_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/func-basic.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 4.4.", text[0]);

    Ok(())
}

#[test]
fn fun_none_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/func-none.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 3.8.", text[0]);

    Ok(())
}

#[test]
fn fun_inline_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/func-inline.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 4.4.", text[0]);

    Ok(())
}

#[test]
fn setvar_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/setvar-func.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value is 6.", text[0]);

    Ok(())
}

#[test]
fn complex_func1_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/complex-func1.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The values are 6 and 10.", text[0]);

    Ok(())
}

#[test]
fn complex_func2_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/complex-func2.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The values are -1 and 0 and 1.", text[0]);

    Ok(())
}

#[test]
fn complex_func3_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/complex-func3.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!(
        "\"I will pay you 120 reales if you get the goods to their destination. The goods will take up 20 cargo spaces.\"",
        text[0]
    );

    Ok(())
}

#[test]
fn rnd() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/rnd-func.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(4, text.len());
    assert_eq!("Rolling dice 1: 1.", text[0]);
    assert_eq!("Rolling dice 2: 4.", text[1]);
    assert_eq!("Rolling dice 3: 4.", text[2]);
    assert_eq!("Rolling dice 4: 1.", text[3]);

    Ok(())
}

#[test]
fn evaluating_function_variable_state_bug_test() -> Result<(), StoryError> {
    let ink_source =
        common::get_file_string("inkfiles/function/evaluating-function-variablestate-bug.ink")
            .unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("Start\n", story.cont()?);
    assert_eq!("In tunnel.\n", story.cont()?);

    let mut output = String::new();
    let result = story.evaluate_function("function_to_evaluate", None, &mut output);

    assert_eq!("RIGHT", result?.unwrap().get::<&str>().unwrap());

    assert_eq!("End\n", story.cont()?);

    Ok(())
}

// TestFactorialByReference (Tests.cs:906)
#[test]
fn factorial_by_reference_test() -> Result<(), StoryError> {
    let ink = r#"
VAR result = 0
~ factorialByRef(result, 5)
{ result }

== function factorialByRef(ref r, n) ==
{ r == 0:
    ~ r = 1
}
{ n > 1:
    ~ r = r * n
    ~ factorialByRef(r, n-1)
}
~ return
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("120\n", story.continue_maximally()?);

    Ok(())
}

// TestFactorialRecursive (Tests.cs:930)
#[test]
fn factorial_recursive_test() -> Result<(), StoryError> {
    let ink = r#"
{ factorial(5) }

== function factorial(n) ==
{ n == 1:
    ~ return 1
- else:
    ~ return (n * factorial(n-1))
}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("120\n", story.continue_maximally()?);

    Ok(())
}

// TestFunctionCallRestrictions (Tests.cs:949)
#[test]
fn function_call_restrictions_test() {
    let call_knot_as_function = r#"
~ aKnot()

== function myFunc ==
~ return

== aKnot ==
-> END
"#;
    let err = Compiler::new().compile(call_knot_as_function).unwrap_err();
    assert!(
        err.message().contains("function") || err.message().contains("call"),
        "expected function-call restriction error, got: {}",
        err.message()
    );

    let divert_to_function = r#"
-> myFunc

== function myFunc ==
~ return
"#;
    let err = Compiler::new().compile(divert_to_function).unwrap_err();
    assert!(
        err.message().contains("function") || err.message().contains("divert"),
        "expected divert-to-function restriction error, got: {}",
        err.message()
    );
}

// TestNestedPassByReference (Tests.cs:1404)
#[test]
fn nested_pass_by_reference_test() -> Result<(), StoryError> {
    let ink = r#"
VAR globalVal = 5

{globalVal}

~ squaresquare(globalVal)

{globalVal}

== function squaresquare(ref x) ==
 {square(x)} {square(x)}
 ~ return

== function square(ref x) ==
 ~ x = x * x
 ~ return
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("5\n625\n", story.continue_maximally()?);

    Ok(())
}

// TestFloorCeilingAndCasts (Tests.cs:3605)
#[test]
fn floor_ceiling_and_casts_test() -> Result<(), StoryError> {
    let ink = r#"
{FLOOR(1.2)}
{INT(1.2)}
{CEILING(1.2)}
{CEILING(1.2) / 3}
{INT(CEILING(1.2)) / 3}
{FLOOR(1)}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("1\n1\n2\n0.6666667\n0\n1\n", &story.continue_maximally()?);
    Ok(())
}

// TestUsingFunctionAndIncrementTogether (Tests.cs:3653)
#[test]
fn using_function_and_increment_together_test() -> Result<(), StoryError> {
    let ink = r#"
VAR x = 5
~ x += one()
    
=== function one()
~ return 1
"#;
    // Ensure it just compiles and runs without error
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    story.continue_maximally()?;
    Ok(())
}

// TestKnotStitchGatherCounts (Tests.cs:3670)
#[test]
fn knot_stitch_gather_counts_test() -> Result<(), StoryError> {
    let ink = r#"
VAR knotCount = 0
VAR stitchCount = 0

-> gather_count_test ->

~ knotCount = 0
-> knot_count_test ->

~ knotCount = 0
-> knot_count_test ->

-> stitch_count_test ->

== gather_count_test ==
VAR gatherCount = 0
- (loop)
~ gatherCount++
{gatherCount} {loop}
{gatherCount<3:->loop}
->->

== knot_count_test ==
~ knotCount++
{knotCount} {knot_count_test}
{knotCount<3:->knot_count_test}
->->

== stitch_count_test ==
~ stitchCount = 0
-> stitch ->
~ stitchCount = 0
-> stitch ->
->->

= stitch
~ stitchCount++
{stitchCount} {stitch}
{stitchCount<3:->stitch}
->->
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!(
        "1 1\n2 2\n3 3\n1 1\n2 1\n3 1\n1 2\n2 2\n3 2\n1 1\n2 1\n3 1\n1 2\n2 2\n3 2\n",
        &story.continue_maximally()?
    );
    Ok(())
}

// TestPrintNum (Tests.cs)
#[test]
fn print_num_test() -> Result<(), StoryError> {
    let ink = r#"
. {print_num(4)} .
. {print_num(15)} .
. {print_num(37)} .
. {print_num(101)} .
. {print_num(222)} .
. {print_num(1234)} .

=== function print_num(x) ===
{
    - x >= 1000:
        {print_num(x / 1000)} thousand { x mod 1000 > 0:{print_num(x mod 1000)}}
    - x >= 100:
        {print_num(x / 100)} hundred { x mod 100 > 0:and {print_num(x mod 100)}}
    - x == 0:
        zero
    - else:
        { x >= 20:
            { x / 10:
                - 2: twenty
                - 3: thirty
                - 4: forty
                - 5: fifty
                - 6: sixty
                - 7: seventy
                - 8: eighty
                - 9: ninety
            }
            { x mod 10 > 0:<>-<>}
        }
        { x < 10 || x > 20:
            { x mod 10:
                - 1: one
                - 2: two
                - 3: three
                - 4: four
                - 5: five
                - 6: six
                - 7: seven
                - 8: eight
                - 9: nine
            }
        - else:
            { x:
                - 10: ten
                - 11: eleven
                - 12: twelve
                - 13: thirteen
                - 14: fourteen
                - 15: fifteen
                - 16: sixteen
                - 17: seventeen
                - 18: eighteen
                - 19: nineteen
            }
        }
}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!(
        ". four .\n. fifteen .\n. thirty-seven .\n. one hundred and one .\n. two hundred and twenty-two .\n. one thousand two hundred and thirty-four .\n",
        &story.continue_maximally()?
    );
    Ok(())
}

// TestLiteralUnary (Tests.cs)
#[test]
fn literal_unary_test() -> Result<(), StoryError> {
    let ink = r#"
VAR negativeLiteral = -1
VAR negativeLiteral2 = not not false
VAR negativeLiteral3 = !(0)

{negativeLiteral}
{negativeLiteral2}
{negativeLiteral3}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("-1\nfalse\ntrue\n", &story.continue_maximally()?);
    Ok(())
}

// TestLogicLinesWithNewlines (Tests.cs)
#[test]
fn logic_lines_with_newlines_test() -> Result<(), StoryError> {
    let ink = r#"
~ func ()
text 2

~temp tempVar = func ()
text 2

== function func ()
	text1
	~ return true
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!(
        "text1\ntext 2\ntext1\ntext 2\n",
        &story.continue_maximally()?
    );
    Ok(())
}
