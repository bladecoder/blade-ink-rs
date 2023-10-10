# Blade Ink
This is a Rust port of inkle's [ink](https://github.com/inkle/ink), a scripting language for writing interactive narrative.

`bladeink` is fully compatible with the reference version and supports all the language features.

To know more about the Ink language, you can check [the oficial documentation](https://github.com/inkle/ink/blob/master/Documentation/WritingWithInk.md).

The **Blade Ink** project contains 3 crates:

- `lib` is the `bladeink` lib crate. It is available in crates.io and it can be used in your project as a dependency.
- `cli-player` contains an implementation of a cli player (called `binkplayer`) to run .json.ink story files directly from the console.
- `clib` is a C binding of the `bladeink` library ready to be used in C or any other program that can uses C libraries.

## Using the bladeink library crate

Here it is a quick example that uses the basic features to play an Ink story using the `bladeink` crate.

```rust
// story is the entry point of the `bladeink` lib.
// json_string is a string with all the contents of the .ink.json file.
let mut story = Story::new(json_string)?;

let mut end = false;

while !end {
    while story.can_continue() {
        let line = story.cont()?;

        println!("{}", line);
    }

    let choices = story.get_current_choices();
    if !choices.is_empty() {
        // read_input is a method that you should implement
        // to get the choice selected by the user.
        let choice_idx = read_input(&choices)?;
        // set the option selected by the user
        story.choose_choice_index(choice_idx)?;
    } else {
        end = true;
    }
}
```

The `bladeink` library support all the **Ink** language features, including threads, multi-flows, variable set/get from code, variable observing, external functions, tags on choices, etc. Examples of uses of all these features can be found in the `lib/tests` folder in the source code.


## Running Ink stories with *binkplayer*

If you download the source code repository, you can run `cargo build` in the workspace root folder, the `binkplayer` binary will be compiled and found in `target/debug`. You can play any of the `.ink.json` file using it.

In the `inkfiles` folder we can found many Ink test stories to test the Ink language capabilities. And also we have **The Intercept**, a full featured story created by **Inkle** also included in the `inkfiles` folder. You can run **The Intercept** running the next command in your console.

```bash
$ target/debug/binkplayer inkfiles/TheIntercept.ink.json
```

## Using the C bindings

If you download the source code repository, you can build the C bindings using the Makefile inside the clib folder.

To create the library in the target/release folder use

```bash
 $ make clib
```

This will create the `libbinkc.so.x.x.x`, where x.x.x is the version of the library, and the `binkc.h` ready to include in your C projects.

The C bindings is a work in progress. In the current state, only the basic functionality to play an Ink story is finish.

We can find an example of use in C in the `clib/tests/binkc_test.c` file. It plays **The Intercept** story included in the `inkfiles` folder, choosing always the first option presented to the user.