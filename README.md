# Blade Ink
This is a Rust port of Inkle's [Ink](https://github.com/inkle/ink), a scripting language for writing interactive narrative.

`bladeink` is fully compatible with the reference version and supports all its language features.

To know more about the Ink language, you can check [the official documentation](https://github.com/inkle/ink/blob/master/Documentation/WritingWithInk.md).

## Using the bladeink library crate

Here is a quick example that uses basic features to play an Ink story using the `bladeink` crate.

```rust
// story is the entry point of the `bladeink` lib.
// json_string is a string with all the contents of the .ink.json file.
let mut story = Story::new(json_string)?;

loop {
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
        break;
    }
}
```

The `bladeink` library supports all the **Ink** language features, including threads, multi-flows, variable set/get from code, variable observing, external functions, tags on choices, etc. Examples of uses of all these features can be found in the `lib/tests` folder in the [source code](https://github.com/bladecoder/blade-ink-rs/tree/main/lib/tests).


## Running Ink stories with *binkplayer*

The Blade Ink project includes a program to run Ink stories in your terminal.

You can install it from crates.io:

```bash
$ cargo install binkplayer
$ binkplayer <your_story.ink.json>
```

Or, if you download the source code repository, you can run `cargo build` in the workspace root folder, the `binkplayer` binary will be compiled and found in `target/debug`. You can play any `.ink.json` (Ink compiled files).

In the `inkfiles` folder you can find many Ink test stories to test the Ink language capabilities. And also we have **The Intercept**, a full featured story created by **Inkle**, also included in the `inkfiles` folder. You can run **The Intercept** running the next command in your console.

```bash
$ target/debug/binkplayer inkfiles/TheIntercept.ink.json
```

## Using Blade Ink in C

There are available C bindings to use Blade Ink in your C projects. Check it out [here](https://github.com/bladecoder/blade-ink-ffi).
