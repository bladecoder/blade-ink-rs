use std::fmt;

use strum::Display;

use crate::object::{RTObject, Object};

#[derive(PartialEq, Display, Clone, Copy)]
pub enum CommandType {
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

const EVAL_START_NAME: &str = "ev";
const EVAL_OUTPUT_NAME: &str = "out";
const EVAL_END_NAME: &str = "/ev";
const DUPLICATE_NAME: &str = "du";
const POP_EVALUATED_VALUE_NAME: &str = "pop";
const POP_FUNCTION_NAME: &str = "~ret";
const POP_TUNNEL_NAME: &str = "->->";
const BEGIN_STRING_NAME: &str = "str";
const END_STRING_NAME: &str = "/str";
const NO_OP_NAME: &str = "nop";
const CHOICE_COUNT_NAME: &str = "choiceCnt";
const TURNS_NAME: &str = "turn";
const TURNS_SINCE_NAME: &str = "turns";
const READ_COUNT_NAME: &str = "readc";
const RANDOM_NAME: &str = "rnd";
const SEED_RANDOM_NAME: &str = "srnd";
const VISIT_INDEX_NAME: &str = "visit";
const SEQUENCE_SHUFFLE_INDEX_NAME: &str = "seq";
const START_THREAD_NAME: &str = "thread";
const DONE_NAME: &str = "done";
const END_NAME: &str = "end";
const LIST_FROM_INT_NAME: &str = "listInt";
const LIST_RANGE_NAME: &str = "range";
const LIST_RANDOM_NAME: &str = "lrnd";
const BEGIN_TAG_NAME: &str = "#";
const END_TAG_NAME: &str = "/#";

pub struct ControlCommand {
    obj: Object,
    pub command_type: CommandType
}

impl ControlCommand {

    pub fn new_from_name(name: &str) -> Option<Self> {
        match name {
            EVAL_START_NAME => Some(Self::new(CommandType::EvalStart)),
            EVAL_OUTPUT_NAME => Some(Self::new(CommandType::EvalOutput)),
            EVAL_END_NAME => Some(Self::new(CommandType::EvalEnd)),
            DUPLICATE_NAME => Some(Self::new(CommandType::Duplicate)),
            POP_EVALUATED_VALUE_NAME => Some(Self::new(CommandType::PopEvaluatedValue)),
            POP_FUNCTION_NAME => Some(Self::new(CommandType::PopFunction)),
            POP_TUNNEL_NAME => Some(Self::new(CommandType::PopTunnel)),
            BEGIN_STRING_NAME => Some(Self::new(CommandType::BeginString)),
            END_STRING_NAME => Some(Self::new(CommandType::EndString)),
            NO_OP_NAME => Some(Self::new(CommandType::NoOp)),
            CHOICE_COUNT_NAME => Some(Self::new(CommandType::ChoiceCount)),
            TURNS_NAME => Some(Self::new(CommandType::Turns)),
            TURNS_SINCE_NAME => Some(Self::new(CommandType::TurnsSince)),
            READ_COUNT_NAME => Some(Self::new(CommandType::ReadCount)),
            RANDOM_NAME => Some(Self::new(CommandType::Random)),
            SEED_RANDOM_NAME => Some(Self::new(CommandType::SeedRandom)),
            VISIT_INDEX_NAME => Some(Self::new(CommandType::VisitIndex)),
            SEQUENCE_SHUFFLE_INDEX_NAME => Some(Self::new(CommandType::SequenceShuffleIndex)),
            START_THREAD_NAME => Some(Self::new(CommandType::StartThread)),
            DONE_NAME => Some(Self::new(CommandType::Done)),
            END_NAME => Some(Self::new(CommandType::End)),
            LIST_FROM_INT_NAME => Some(Self::new(CommandType::ListFromInt)),
            LIST_RANGE_NAME => Some(Self::new(CommandType::ListRange)),
            LIST_RANDOM_NAME => Some(Self::new(CommandType::ListRandom,)),
            BEGIN_TAG_NAME => Some(Self::new(CommandType::BeginTag)),
            END_TAG_NAME => Some(Self::new(CommandType::EndTag)),
            _ => None,
        }

    }

    pub fn get_name(c: CommandType) -> String {
        match c {
            CommandType::EvalStart => EVAL_START_NAME.to_owned(),
            CommandType::EvalOutput => EVAL_OUTPUT_NAME.to_owned(),
            CommandType::EvalEnd => EVAL_END_NAME.to_owned(),
            CommandType::Duplicate => DUPLICATE_NAME.to_owned(),
            CommandType::PopEvaluatedValue => POP_EVALUATED_VALUE_NAME.to_owned(),
            CommandType::PopFunction => POP_FUNCTION_NAME.to_owned(),
            CommandType::PopTunnel => POP_TUNNEL_NAME.to_owned(),
            CommandType::BeginString => BEGIN_STRING_NAME.to_owned(),
            CommandType::EndString => END_STRING_NAME.to_owned(),
            CommandType::NoOp => NO_OP_NAME.to_owned(),
            CommandType::ChoiceCount => CHOICE_COUNT_NAME.to_owned(),
            CommandType::Turns => TURNS_NAME.to_owned(),
            CommandType::TurnsSince => TURNS_SINCE_NAME.to_owned(),
            CommandType::ReadCount => READ_COUNT_NAME.to_owned(),
            CommandType::Random => RANDOM_NAME.to_owned(),
            CommandType::SeedRandom => SEED_RANDOM_NAME.to_owned(),
            CommandType::VisitIndex => VISIT_INDEX_NAME.to_owned(),
            CommandType::SequenceShuffleIndex => SEQUENCE_SHUFFLE_INDEX_NAME.to_owned(),
            CommandType::StartThread => START_THREAD_NAME.to_owned(),
            CommandType::Done => DONE_NAME.to_owned(),
            CommandType::End => END_NAME.to_owned(),
            CommandType::ListFromInt => LIST_FROM_INT_NAME.to_owned(),
            CommandType::ListRange => LIST_RANGE_NAME.to_owned(),
            CommandType::ListRandom => LIST_RANDOM_NAME.to_owned(),
            CommandType::BeginTag => BEGIN_TAG_NAME.to_owned(),
            CommandType::EndTag => END_TAG_NAME.to_owned(),
        }
    }

    pub fn new(command_type: CommandType) -> Self {
        Self {obj: Object::new(), command_type}
    }
}

impl RTObject for ControlCommand {
    fn get_object(&self) -> &Object {
        &self.obj
     }
}

impl fmt::Display for ControlCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.command_type)
    }
}


