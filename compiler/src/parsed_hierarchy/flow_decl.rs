use super::FlowArgument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowDecl {
    pub name: String,
    pub arguments: Vec<FlowArgument>,
    pub is_function: bool,
}
