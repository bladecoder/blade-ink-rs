#[derive(PartialEq, Clone, Copy)]
pub(crate) enum PushPopType {
    Tunnel,
    Function,
    FunctionEvaluationFromGame
}