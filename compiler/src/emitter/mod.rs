use std::collections::{BTreeMap, BTreeSet};

use bladeink::story::INK_VERSION_CURRENT;
use serde_json::{Map, Value, json};

use crate::{
    ast::{
        AssignMode, BinaryOperator, Choice, Condition, Divert, DynamicString, DynamicStringPart,
        Expression, Flow, GlobalVariable, ListDeclaration, Node, ParsedStory, Sequence,
        SequenceMode,
    },
    error::CompilerError,
    inline::{parse_dynamic_string, tokenize_inline_content},
};

include!("context.rs");
include!("flow.rs");
include!("nodes.rs");
include!("choice_threaded.rs");
include!("choice_wrapped.rs");
include!("choice.rs");
include!("expression.rs");
include!("conditional.rs");
