#[derive(Clone)]
pub struct Line<'a> {
    pub content: &'a str,
    pub indent: usize,
    pub had_newline: bool,
}

pub enum Header {
    Knot {
        name: String,
        parameters: Vec<String>,
        ref_parameters: Vec<String>,
        divert_parameters: Vec<String>,
    },
    Function {
        name: String,
        parameters: Vec<String>,
        ref_parameters: Vec<String>,
        divert_parameters: Vec<String>,
    },
    Stitch {
        name: String,
        parameters: Vec<String>,
        ref_parameters: Vec<String>,
        divert_parameters: Vec<String>,
    },
}

#[derive(Default)]
struct FlowBuilder {
    name: String,
    is_function: bool,
    is_root_stitch: bool,
    parameters: Vec<String>,
    ref_parameters: Vec<String>,
    divert_parameters: Vec<String>,
    nodes: Vec<Node>,
    children: Vec<Flow>,
}

pub struct Parser<'a> {
    source: &'a str,
}

pub enum ParsedStatement {
    Global(GlobalVariable),
    Const(GlobalVariable),
    List(ListDeclaration),
    ExternalFunction(String),
    Nodes(Vec<Node>),
}
