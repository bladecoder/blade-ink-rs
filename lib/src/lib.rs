//! This is a Rust port of inkle's [ink](https://github.com/inkle/ink), a scripting language for writing interactive narrative.
//! `bink` is fully compatible with the original version.

pub mod story;
pub mod story_callbacks;
pub mod value_type;
pub mod story_error;
pub mod choice;
mod json_read;
mod json_write;
mod object;
mod value;
mod container;
mod control_command;
mod story_state;
mod pointer;
mod path;
mod search_result;
mod callstack;
mod flow;
mod push_pop;
mod variables_state;
mod glue;
mod void;
mod state_patch;
mod choice_point;
mod tag;
mod divert;
mod variable_assigment;
mod variable_reference;
mod native_function_call;
mod ink_list;
mod ink_list_item;
mod list_definition;
mod list_definitions_origin;


