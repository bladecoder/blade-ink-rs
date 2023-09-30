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

pub struct ControlCommand {
    obj: Object,
    pub command_type: CommandType
}

impl ControlCommand {

    pub fn new_from_name(name: &str) -> Option<Self> {
        match name {
            "ev" => Some(Self::new(CommandType::EvalStart)),
            "out" => Some(Self::new(CommandType::EvalOutput)),
            "/ev" => Some(Self::new(CommandType::EvalEnd)),
            "du" => Some(Self::new(CommandType::Duplicate)),
            "pop" => Some(Self::new(CommandType::PopEvaluatedValue)),
            "~ret" => Some(Self::new(CommandType::PopFunction)),
            "->->" => Some(Self::new(CommandType::PopTunnel)),
            "str" => Some(Self::new(CommandType::BeginString)),
            "/str" => Some(Self::new(CommandType::EndString)),
            "nop" => Some(Self::new(CommandType::NoOp)),
            "choiceCnt" => Some(Self::new(CommandType::ChoiceCount)),
            "turn" => Some(Self::new(CommandType::Turns)),
            "turns" => Some(Self::new(CommandType::TurnsSince)),
            "readc" => Some(Self::new(CommandType::ReadCount)),
            "rnd" => Some(Self::new(CommandType::Random)),
            "srnd" => Some(Self::new(CommandType::SeedRandom)),
            "visit" => Some(Self::new(CommandType::VisitIndex)),
            "seq" => Some(Self::new(CommandType::SequenceShuffleIndex)),
            "thread" => Some(Self::new(CommandType::StartThread)),
            "done" => Some(Self::new(CommandType::Done)),
            "end" => Some(Self::new(CommandType::End)),
            "listInt" => Some(Self::new(CommandType::ListFromInt)),
            "range" => Some(Self::new(CommandType::ListRange)),
            "lrnd" => Some(Self::new(CommandType::ListRandom,)),
            "#" => Some(Self::new(CommandType::BeginTag)),
            "/#" => Some(Self::new(CommandType::EndTag)),
            _ => None,
        }

    }

    pub fn get_name(c: CommandType) -> String {
        match c {
            CommandType::EvalStart => "ev".to_owned(),
            CommandType::EvalOutput => "out".to_owned(),
            CommandType::EvalEnd => "/ev".to_owned(),
            CommandType::Duplicate => "du".to_owned(),
            CommandType::PopEvaluatedValue => "pop".to_owned(),
            CommandType::PopFunction => "~ret".to_owned(),
            CommandType::PopTunnel => "->->".to_owned(),
            CommandType::BeginString => "str".to_owned(),
            CommandType::EndString => "/str".to_owned(),
            CommandType::NoOp => "nop".to_owned(),
            CommandType::ChoiceCount => "choiceCnt".to_owned(),
            CommandType::Turns => "turn".to_owned(),
            CommandType::TurnsSince => "turns".to_owned(),
            CommandType::ReadCount => "readc".to_owned(),
            CommandType::Random => "rnd".to_owned(),
            CommandType::SeedRandom => "srnd".to_owned(),
            CommandType::VisitIndex => "visit".to_owned(),
            CommandType::SequenceShuffleIndex => "seq".to_owned(),
            CommandType::StartThread => "thread".to_owned(),
            CommandType::Done => "done".to_owned(),
            CommandType::End => "end".to_owned(),
            CommandType::ListFromInt => "listInt".to_owned(),
            CommandType::ListRange => "range".to_owned(),
            CommandType::ListRandom => "lrnd".to_owned(),
            CommandType::BeginTag => "#".to_owned(),
            CommandType::EndTag => "/#".to_owned(),
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
        write!(f, "{}", self.command_type.to_string())
    }
}


