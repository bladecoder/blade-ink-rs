//! This is a Rust port of inkle's [Ink](https://github.com/inkle/ink), a scripting language for writing interactive narrative.
//! `bink` is fully compatible with the reference version and supports all the language features.
//!
//! To know more about the Ink language, you can check [the oficial documentation](https://github.com/inkle/ink/blob/master/Documentation/WritingWithInk.md).
//!
//! Here it is a quick example that uses the basic features to play an Ink story using the `bink` crate.
//!
//! ```no_run
//! // story is the entry point of the `bink` lib.
//! // json_string is a string with all the contents of the .ink.json file.
//! let mut story = Story::new(json_string)?;
//!
//! let mut end = false;
//!
//! while !end {
//!     while story.can_continue() {
//!         let line = story.cont()?;
//!
//!         println!("{}", line);
//!     }
//!
//!     let choices = story.get_current_choices();
//!     if !choices.is_empty() {
//!         // read_input is a method that you should implement
//!         // to get the choice selected by the user.
//!         let choice_idx = read_input(&choices)?;
//!         // set the option selected by the user
//!         story.choose_choice_index(choice_idx)?;
//!     } else {
//!        end = true;
//!     }
//! }
//! ```
//!
//! The `bink` library support all the **Ink** language features, including threads, multi-flows, variable set/get from code, variable observing, external functions,
//! tags on choices, etc. Examples of uses of all these features will be added to this documentation in the future, but meanwhile, all the examples can be found in the `lib/tests` folder in the source code of this crate.

mod callstack;
pub mod choice;
mod choice_point;
mod container;
mod control_command;
mod divert;
mod flow;
mod glue;
mod ink_list;
mod ink_list_item;
mod json_read;
mod json_write;
mod list_definition;
mod list_definitions_origin;
mod native_function_call;
mod object;
mod path;
mod pointer;
mod push_pop;
mod search_result;
mod state_patch;
pub mod story;
pub mod story_callbacks;
pub mod story_error;
mod story_state;
mod tag;
mod value;
pub mod value_type;
mod variable_assigment;
mod variable_reference;
mod variables_state;
mod void;
