//! Frontend pipeline for the native Rust compiler port.
//!
//! This module will host the Rust equivalents of the Java compiler frontend:
//! `CommentEliminator`, `StringParser`, `StringParserState`, and `InkParser`.

pub mod comment_eliminator;
pub mod ink_parser;
pub mod string_parser;
pub mod string_parser_state;
