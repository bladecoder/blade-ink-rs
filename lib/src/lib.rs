//! This is a Rust port of inkle's [Ink](https://github.com/inkle/ink), a scripting language for writing interactive narrative.
//! `bink` is fully compatible with the original version.

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
