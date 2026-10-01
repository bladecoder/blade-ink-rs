# Running Ink stories from a binary image

`bladeink` can execute compiled Ink from a read-only binary image embedded in a program. The image is generated on the host from `.ink` or compiled `.ink.json` and is read through offsets at runtime. The default constructor checks the header and section bounds, then runs without parsing story JSON or rebuilding static nodes in RAM. A separate constructor validates the entire image. This is useful for a `no_std + alloc` application whose story bytes are mapped to flash.

The format belongs to `bladeink`. It is not compatible with InkCpp `.bin` files. The existing C++ player in `ink-tts-esp32` cannot consume these images without changing its runtime.

## Storage and execution

The public `Story` type uses one interpreter for two kinds of static content:

```text
compiled .ink.json ──JSON reader──> owned flat arena in RAM ──┐
                                                              ├──> interpreter by node ID
compiled .inkb ──header check────> view of static bytes ──────┘          │
                                                                         ▼
                                                       mutable state in RAM
```

Every static node has a 32-bit `NodeId`. A container also has a `ContainerId`; ordered child ranges, named child entries, parent IDs and child indexes preserve the Ink hierarchy. The interpreter tracks a pointer as a container ID and child index. Static divert, choice and count targets are resolved to IDs when the content is built. A divert whose destination comes from a variable is resolved while the story runs.

`StaticStoryView` gives the interpreter read-only access to nodes, operands, children, names, paths and list definitions. `StoryContent::Arena` owns records built by a JSON reader. `StoryContent::Image` keeps only a `&'static [u8]` and checked section bounds; it decodes a borrowed node view when requested. Fetching a static instruction does not allocate a persistent object for that node. The arena and image run through the same execution logic.

Variables, choices, call stacks and threads, flows, output, evaluation values, random state, visit and turn counts, and other run state remain in RAM. Some dynamic values and public API objects still use `Rc`; static image nodes do not have an `Rc` each. One `Rc` shares the whole `StoryContent` with the runtime. This separation is why the story bytes can stay in flash while the state changes.

### Lazy work and caches

The legacy `Path` type still caches `components_string` in a `OnceCell`. The flat interpreter normally follows prelinked IDs, so it does not construct a textual path for each divert. It builds path text when an API call or a save operation needs it. A bounded, sparse path cache keyed by node ID exists, but the normal interpreter does not currently use it; repeated path API calls can recompute a path. The public `Rc<Choice>` objects are created only when choices are requested and are cached on the `Story` instance until the relevant state changes. Neither cache is stored in the image.

Visit and turn counts use `ContainerId` keys during execution. Saving converts IDs to Ink paths; loading resolves those paths back to IDs. Saves retain the Ink JSON state format, including choices, flows and threads, and can be exchanged between JSON-backed and image-backed instances of the same story.

## Choosing a runtime and features

`bladeink::story::Story` is the flat interpreter. `LegacyStory` keeps the earlier object-tree implementation for compatibility checks. The selected constructor determines the static storage:

```rust
use bladeink::story::{LegacyStory, Story};

let json_story = Story::new_with_seed(compiled_json, 42)?;
let old_story = LegacyStory::new_with_seed(compiled_json, 42)?;
let image_story = Story::new_from_image_with_seed(IMAGE, 42)?;
```

`stream-json-parser` selects a JSON reader, independently of `Story` versus `LegacyStory`. With default features, `std` and `serde-json-parser` are enabled. The available image APIs are:

| API | Availability | Purpose |
| --- | --- | --- |
| `Story::new_from_image_with_seed(&'static [u8], i32)` | `binary-image`, including `no_std` | Open a trusted image with a fixed seed; check header and section bounds only. |
| `Story::new_from_image(&'static [u8])` | `binary-image` and `std` | Open a trusted image with a generated seed. |
| `Story::new_from_image_validated_with_seed(&'static [u8], i32)` | `binary-image`, including `no_std` | Fully validate an image with a fixed seed. |
| `Story::new_from_image_validated(&'static [u8])` | `binary-image` and `std` | Fully validate an image with a generated seed. |
| `bladeink::image::compile_json_to_image(reader)` | `binary-image` and `std` | Generate image bytes from compiled Ink JSON on the host. |
| `rinklecate --image -o output.inkb input.ink` | `rinklecate` | Compile Ink and write an image. |
| `rinklecate --image -o output.inkb input.ink.json` | `rinklecate` | Convert compiled JSON directly. |
| `rinklecate input.inkb` | `rinklecate` | Validate and play a binary image interactively. |

A target that only opens images can depend on `bladeink` with `default-features = false, features = ["binary-image"]`. This leaves out the **story** JSON parsers; the JSON state codec remains available for saving and restoring a game. `no_std` still needs an allocator. If JSON loading is also required, enable `stream-json-parser` or use the default Serde reader under `std`. `rinklecate` detects `.ink`, `.json` and `.inkb` inputs by extension; binary file input uses the fully validated image constructor. `rinklecate --image` cannot be combined with play (`-p`) or statistics (`-s`) modes.

## Generating and embedding an image

For an already compiled story:

```sh
cargo run --release -p rinklecate -- --image -o "$PWD/story.inkb" story.ink.json
```

When the input resides in another directory, use an absolute output path: `rinklecate` resolves a relative output path against the input directory.

A Rust application can generate the image in `build.rs`. Cargo resolver 2 builds the generator for the host with `std`, while the target dependency can use only `binary-image`:

```toml
# Cargo.toml excerpt (supply the application's usual package fields)
[package]
resolver = "2"

[dependencies]
bladeink = { path = "../blade-ink-rs/runtime", default-features = false, features = ["binary-image"] }

[build-dependencies]
bladeink = { path = "../blade-ink-rs/runtime", features = ["binary-image"] }
```

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=story.ink.json");
    let source = std::fs::File::open("story.ink.json")?;
    let image = bladeink::image::compile_json_to_image(source)?;
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("story.inkb"), image)?;
    Ok(())
}
```

```rust
// Application code
use bladeink::{story::Story, story_error::StoryError};

static IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/story.inkb"));

fn open_story() -> Result<Story, StoryError> {
    Story::new_from_image_with_seed(IMAGE, 42)
}
```

Alternatively, generate a tracked `.inkb` file ahead of time and point `include_bytes!` at it. A minimal application using the build script above compiled for `thumbv7em-none-eabihf` with the host generator and the `no_std` image runtime. On ESP32, inspect the linker map to confirm the embedded array is placed in mapped flash. The build and host measurements here do not verify placement in a particular firmware.

## Image validation and binary format

Image format version **2** starts with an 88-byte header. Integers are 32-bit little endian. The offsets below are absolute from the start of the image; string and payload offsets inside a record are relative to their respective sections. `0xffffffff` denotes an absent optional field. The file contains no absolute pointers or serialized Rust struct layouts.

| Header byte offset | Content |
| ---: | --- |
| 0 | Eight-byte ASCII magic `BLINKIMG` |
| 8 | Format version `2` |
| 12 | Ink version as `i32` |
| 16 | Total byte length |
| 20 | Node section offset and record count |
| 28 | Ordered child ID section offset and count |
| 36 | Named child section offset and count |
| 44 | List definition section offset and count |
| 52 | List definition item section offset and count |
| 60 | Container path index offset and count |
| 68 | Payload section offset and byte length |
| 76 | String section offset and byte length |
| 84 | CRC-32 of the image excluding these four bytes |

The sections are contiguous in header order. Node, ordered child, named child, list definition, list item and path index records are respectively 40, 4, 12, 16, 12 and 12 bytes. A node ID is its index in the node section; ID `0` is the root. Every node has ten words: `parent`, `child_index`, `tag`, then `f0` through `f6`. `parent` and `child_index` can be absent. A string reference is a pair `(offset, byte length)` into the UTF-8 string section. Ink path text retains the leading `.` for relative paths. Resolved static targets are node IDs.

| Tag | Node kind | Fields |
| ---: | --- | --- |
| 1 | Container | `f0:f1` optional name; `f2` flags; `f3:f4` ordered child range; `f5:f6` named child range |
| 2 | Choice point | `f0:f1` optional path; `f2` target ID; `f3` flags |
| 3 | Ink control command | `f0:f1` command name |
| 4 | Divert | `f0:f1` optional path; `f2` target ID; `f3:f4` optional variable name; `f5` external argument count; `f6` flags |
| 5 | Glue | No fields |
| 6 | Native function | `f0:f1` operation name |
| 7 | Tag | `f0:f1` text |
| 8 | Boolean | `f0` is 0 or 1 |
| 9 | Integer | `f0` interpreted as `i32` |
| 10 | Float | `f0` contains IEEE-754 `f32` bits |
| 11 | Text | `f0:f1` text |
| 12 | Ink list | `f0:f1` payload offset and byte length |
| 13 | Divert target value | `f0:f1` path; `f2` resolved ID |
| 14 | Variable pointer | `f0:f1` name; `f2` context index |
| 16 | Variable assignment | `f0:f1` name; `f2` flags |
| 17 | Variable reference | `f0:f1` name; `f2:f3` optional count path; `f4` count target ID |
| 18 | Void | No fields |

For diverts, flag bits 0, 1 and 2 indicate conditional, external and stack-pushing behavior; bits from 3 encode `PushPopType` (tunnel 0, function 1, function evaluation from game 2). Assignment flag bits 0 and 1 mean global and new declaration. Unused fields contain `0xffffffff`.

- **Ordered children:** one node ID (`u32`) per entry. Containers point to a range in this section.
- **Named children:** `(name offset, name length, node ID)`, sorted by name within each container for binary search.
- **List definitions:** `(name offset, name length, first item, item count)`, sorted by name. Their items are `(name offset, name length, i32 value)`, also sorted by name.
- **Container paths:** `(path offset, path length, container ID)`, sorted by path text, including the root path `""`. These support public navigation and saved-state pointers; pointers to ordered children append a numeric component.
- **Ink list payloads:** `n_items`, `n_origins`, then `n_items` tuples `(origin offset, origin length, item offset, item length, i32 value)` and `n_origins` pairs `(offset, length)`.
- **Strings:** concatenated UTF-8 bytes without terminators. Repeated strings can share the same first offset.

Generation is deterministic for the same compiled story. The encoder rejects malformed JSON, unresolved static references and sizes that exceed 32-bit fields. The default image constructor checks the signature, format and Ink versions, total size, contiguous section bounds and the presence of a root record. It does not check the CRC or the contents of nodes and indexes. Use it only for a trusted image generated as part of the build or otherwise verified before embedding: malformed records may cause an error or panic later in execution.

The `*_validated*` constructors additionally check CRC, UTF-8, node kinds, graph relationships, targets, lists and indexes before execution. Their validator uses temporary scratch allocations, then retains the same byte slice and section bounds as the default constructor. CRC-32 uses the reflected IEEE polynomial `0xedb88320`, initial value `0xffffffff` and final XOR `0xffffffff`. A compile-time 256-entry CRC table trades up to about 1 KiB of program data for faster checking when this path is linked; path index validation compares components directly against ancestry without allocating path strings. `Story::reset_state` reuses the image without reopening or revalidating it.

## Generated stories for `ink-tts-esp32`

The images below were made from compiled JSON in `../ink-tts-esp32`; they are not replacements for the InkCpp binaries currently embedded by that C++ firmware.

| Story | JSON source size | Nodes | Image size | SHA-256 of source JSON |
| --- | ---: | ---: | ---: | --- |
| English [`story_en.inkb`](../assets/ink-tts-esp32/story_en.inkb) | 155,237 B | 7,301 | 455,127 B | `e5bffa83643dff441a1c79b9a74a97b73568dd518ad69f753def183c03fdd371` |
| Spanish [`story_es.inkb`](../assets/ink-tts-esp32/story_es.inkb) | 158,039 B | 7,291 | 458,057 B | `d945608cfea23549d19278135ff22d0d60263695f4d06cc1c07de024c745a1b7` |

Regenerate them from this repository's root:

```sh
cargo run --release -p rinklecate -- --image -o "$PWD/assets/ink-tts-esp32/story_en.inkb" ../ink-tts-esp32/story.ink.json
cargo run --release -p rinklecate -- --image -o "$PWD/assets/ink-tts-esp32/story_es.inkb" ../ink-tts-esp32/story_es.ink.json
```

Both generated images were reproduced byte for byte from those source files. The `generated_ink_tts_images_open_and_restore` test opens each image with only `binary-image`, advances to the first choice, then saves and restores the state.

## Compatibility and checks

The JSON and image backends use the same seed and choices in the differential tests. The image suite opens all 121 compiled JSON fixtures, compares up to 200 observable steps per fixture, and completes a playthrough of *The Intercept*. Focused cases cover relative paths, variable diverts, dynamic text and tags, lists, functions, globals, flows and cross-backend save restoration. The earlier tree implementation remains available as `LegacyStory` for comparison with the flat interpreter.

Useful commands from the repository root:

```sh
cargo test
cargo test -p bladeink --features stream-json-parser
cargo test -p bladeink --no-default-features --features binary-image
cargo test -p conformance-tests --features binary-image
cargo check -p bladeink --lib --no-default-features --features binary-image --target thumbv7em-none-eabihf
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The streaming JSON reader uses a fixed 512-byte buffer under `no_std` to reduce calls to a reader that can supply blocks. This affects JSON story loading when enabled and the streaming saved-state codec. It does not remove the need for an allocator or imply a speedup when reading an already embedded byte slice.

## Host memory and timing measurements

Measurements below count requested live heap bytes, peak requested heap bytes and successful allocator calls. They exclude allocator bookkeeping, fragmentation, stack, and story input bytes. They were taken in `release` mode on an x86_64 host; they are neither ESP32-S3 PSRAM measurements nor proof that a complete firmware fits in 4 MB of flash and 2 MB of PSRAM.

### JSON interpreter comparison

[`flat_story_cost.rs`](../runtime/examples/flat_story_cost.rs) compares the old tree with the flat interpreter on *The Intercept*. The JSON is embedded in the host executable and excluded from heap accounting. The route chooses option 0 for up to 20 decisions. Representative results from runs after warmup:

| JSON reader | Interpreter | Retained after open | Peak while opening | Open | Route |
| --- | --- | ---: | ---: | ---: | ---: |
| Serde | Legacy tree | 1,830,419 B | 3,819,439 B | ~3.7 ms | ~1.6 ms |
| Serde | Flat arena | 986,175 B | 3,114,107 B | ~4.1 ms | ~0.51 ms |
| Streaming | Legacy tree | 1,868,607 B | 1,875,343 B | ~3.2 ms | ~1.5 ms |
| Streaming | Flat arena | 971,839 B | 1,115,431 B | ~4.2 ms | ~0.51 ms |

The flat arena retains roughly half the tree's heap in this comparison. It still resides in RAM and must be built after parsing JSON. With `load-profile`, the example can show separate JSON decode, arena construction, target linking and runtime initialization times. These older samples varied enough that they do not establish a precise constructor speedup.

### Binary image comparison

[`image_story_cost.rs`](../runtime/examples/image_story_cost.rs) measures the English and Spanish stories on an x86_64 Intel Core i5-10310U with Rust 1.95.0. It loads the JSON and image before measuring and leaks the host image buffer solely to model a static slice; that allocation is excluded. It alternates JSON and image construction seven times and reports medians from the six passes after the first. The measured route chooses option 0 through 20 decisions and produces 57 lines for each story. The example first compares text, choice text and tags on that route.

```sh
cargo run --release -p bladeink --example image_story_cost --features binary-image -- ../ink-tts-esp32/story.ink.json assets/ink-tts-esp32/story_en.inkb
cargo run --release -p bladeink --example image_story_cost --features binary-image -- ../ink-tts-esp32/story_es.ink.json assets/ink-tts-esp32/story_es.inkb
```

| Story | Backend | Retained after open | Open peak | Open calls | Open time | Retained after route | Route peak | Route calls | Route time |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| English | JSON | 986,231 B | 3,114,107 B | 24,965 | 3,514 µs | 984,027 B | 996,210 B | 5,520 | 649 µs |
| English | Trusted image | 12,804 B | 14,852 B | 150 | 35 µs | 10,706 B | 22,920 B | 5,532 | 973 µs |
| English | Validated image | 12,804 B | 14,852 B | 159 | 1,935 µs | 10,706 B | 22,920 B | 5,532 | 950 µs |
| Spanish | JSON | 989,867 B | 3,072,793 B | 24,716 | 3,690 µs | 987,379 B | 999,705 B | 5,501 | 663 µs |
| Spanish | Trusted image | 12,804 B | 14,852 B | 150 | 35 µs | 10,422 B | 22,779 B | 5,513 | 1,026 µs |
| Spanish | Validated image | 12,804 B | 14,852 B | 159 | 2,036 µs | 10,422 B | 22,779 B | 5,513 | 1,023 µs |

The retained image figure includes initial mutable state and fixed interpreter structures, but no copy of the static node records. Both constructors retain and peak at the same measured heap size. The trusted path avoids the full scan and opens in about 35 µs on this host; the validated path opens in about 2 ms. The generated images occupy more storage than the source JSON, and this route ran more slowly than the JSON-backed interpreter. Check these tradeoffs in the target firmware before sizing flash or PSRAM.

For a controlled node-count check, the benchmark builds two stories with the same mutable state and 1 versus 2,000 text nodes. Their images are 233 B and 88,189 B, while each `Story` retains 868 B after opening. Static node count therefore did not increase retained heap in this check. Variables, lists, choices and other mutable state can still increase it.

## Code map

| File | Role |
| --- | --- |
| [`flat_story.rs`](../runtime/src/flat_story.rs) | IDs, arena, borrowed node view, shared storage interface and path logic. |
| [`image/mod.rs`](../runtime/src/image/mod.rs) | Image validation and read-only offset view. |
| [`image/encoder.rs`](../runtime/src/image/encoder.rs) | Deterministic host encoder. |
| [`flat_runtime.rs`](../runtime/src/flat_runtime.rs) and [`flat_callstack.rs`](../runtime/src/flat_callstack.rs) | Interpreter, mutable state and call stack by ID. |
| [`flat_story_player.rs`](../runtime/src/flat_story_player.rs) | Public `Story` API, observers, lazy choice cache and asynchronous continuation. |
| [`state_stream.rs`](../runtime/src/flat_runtime/state_stream.rs) | Streaming Ink JSON state codec. |
| [`image_equivalence.rs`](../conformance-tests/tests/image_equivalence.rs) | JSON/image differential tests. |
