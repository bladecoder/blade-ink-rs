use std::{any::Any, fmt, rc::Rc};

use strum::Display;

use crate::{object::{RTObject, Object}, container::Container};

#[derive(Display)]
#[derive(PartialEq)]
pub enum CommandType {
    NotSet,
    EvalStart,
    EvalOutput,
    EvalEnd,
    Duplicate,
    PopEvaluatedValue,
    PopFunction,
    PopTunnel,
    BeginString,
    EndString,
    NoOp,
    ChoiceCount,
    Turns,
    TurnsSince,
    ReadCount,
    Random,
    SeedRandom,
    VisitIndex,
    SequenceShuffleIndex,
    StartThread,
    Done,
    End,
    ListFromInt,
    ListRange,
    ListRandom,
    BeginTag,
    EndTag
}

pub(crate) struct ControlCommand {
    obj: Object,
    pub command_type: CommandType
}

impl ControlCommand {
    pub(crate) fn new(command_type: CommandType) -> Self {
        ControlCommand {obj: Object::new(), command_type}
    }
}

impl RTObject for ControlCommand {
    fn get_object(&self) -> &Object {
        &self.obj
     }
}

impl fmt::Display for ControlCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.command_type.to_string())
    }
}


