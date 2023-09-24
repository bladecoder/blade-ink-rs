#[derive(PartialEq, Clone, Copy, Eq, Hash, Debug)]
pub enum PushPopType {
    Tunnel,
    Function,
    FunctionEvaluationFromGame
}