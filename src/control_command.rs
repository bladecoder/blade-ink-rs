use std::any::Any;

use crate::rt_object::RTObject;

pub enum ControlCommand {
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

impl ControlCommand {

}

impl RTObject for ControlCommand {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

