pub mod choice;
pub mod conditional;
pub mod expression;
pub mod inline;
pub mod sequence;

use crate::{
    ast::{AssignMode, Expression, Flow, GlobalVariable, ListDeclaration, Node, ParsedStory},
    error::CompilerError,
};

use self::{
    choice::parse_choice,
    conditional::{looks_like_conditional, parse_conditional},
    expression::{parse_bool, parse_call_like, parse_expression, parse_path_identifier},
    inline::{
        parse_divert, parse_divert_line, parse_dynamic_string, split_inline_divert,
        tokenize_inline_content,
    },
    sequence::{looks_like_sequence, parse_sequence},
};

include!("types.rs");
include!("story.rs");
include!("lines.rs");
include!("statement.rs");
include!("declarations.rs");
include!("header.rs");
