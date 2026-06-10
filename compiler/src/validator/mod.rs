//! Semantic validation passes over the parsed AST.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{AssignMode, Condition, Divert, Expression, Flow, Node, ParsedStory},
    error::CompilerError,
};

include!("context.rs");
include!("structure.rs");
include!("diverts.rs");
include!("variables.rs");
include!("symbols.rs");
