# Blade Ink
This is a Rust port of Inkle's [Ink](https://github.com/inkle/ink), a scripting language for writing interactive narratives.

`bladeink` is fully compatible with the reference version and supports all its language features.

To learn more about the Ink language, you can check [the official documentation](https://github.com/inkle/ink/blob/master/Documentation/WritingWithInk.md).

## Crates

| Crate | Description |
|---|---|
| [`bladeink`](https://crates.io/crates/bladeink) | Runtime library — load and play compiled `.ink.json` stories |
| [`bladeink-compiler`](https://crates.io/crates/bladeink-compiler) | Compiler library — compile `.ink` source files into `.ink.json` |
| [`rinklecate`](https://crates.io/crates/rinklecate) | CLI tool — compile and play Ink stories from the command line |

## Using the `bladeink` runtime crate

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

The `bladeink` library supports all the **Ink** language features, including threads, multi-flows, variable set/get from code, variable observing, external functions, tags on choices, etc. Examples of uses of all these features can be found in the `conformance-tests/tests` folder in the [source code](https://github.com/bladecoder/blade-ink-rs/tree/main/conformance-tests/tests).

## Using the `bladeink-compiler` crate

The `bladeink-compiler` crate compiles `.ink` source files into the JSON format expected by the runtime.

```rust
use bladeink_compiler::compile;

let ink_source = std::fs::read_to_string("my_story.ink")?;
let json = compile(&ink_source, None, None)?;

// json is a String with the compiled .ink.json content
let mut story = bladeink::story::Story::new(&json)?;
```

## Running Ink stories with *rinklecate*

`rinklecate` is a command-line tool that mirrors the interface of the official `inklecate` tool. It can compile `.ink` source files and optionally play them directly in the terminal.

You can install it from crates.io:

```bash
cargo install rinklecate
```

### Usage

```
rinklecate <options> <ink file>
   -o <filename>   Output file name
   -c              Count all visits to knots, stitches and weave points
   -p              Play mode
   -j              Output in JSON format (for communication with tools like Inky)
   -s              Print stats about story including word count
   -v              Verbose mode — print compilation timings
   -k              Keep rinklecate running in play mode even after story is complete
   -x <directory>  Import plugins (accepted but ignored — not supported)
```

### Examples

Compile an `.ink` file to `.ink.json`:

```bash
rinklecate my_story.ink
```

Compile and play immediately:

```bash
rinklecate -p my_story.ink
```

Play an already compiled story:

```bash
rinklecate my_story.ink.json
```

In the `inkfiles` folder you can find many Ink test stories to explore the language capabilities, including **The Intercept**, a full featured story created by **Inkle**:

```bash
rinklecate -p inkfiles/TheIntercept.ink.json
```

## Using Blade Ink in C

There are C bindings available to use Blade Ink in your C projects. Check it out [here](https://github.com/bladecoder/blade-ink-ffi).
