#[derive(PartialEq, Clone, Copy, Eq, Hash, Debug)]
pub enum PushPopType {
    Tunnel,
    Function,
    FunctionEvaluationFromGame
}

impl PushPopType {
    pub(crate) fn from_value(value: usize) -> PushPopType {
        match value {
            0 => PushPopType::Tunnel,
            1 => PushPopType::Function,
            2 => PushPopType::FunctionEvaluationFromGame,
            _ => panic!("Unexpected PushPopType value")
        }
    }
}