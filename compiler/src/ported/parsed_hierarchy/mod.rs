//! Parsed hierarchy for the native Rust compiler port.
//!
//! The target architecture mirrors the Java compiler package, but follows the
//! runtime port style: modular files, explicit ownership, and Rust-native APIs.

pub mod base;
pub mod choice;
pub mod content_list;
pub mod flow_base;
pub mod identifier;
pub mod path;
pub mod story;
pub mod text;
