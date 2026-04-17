#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    Bool(bool),
    FunctionCall(String),
    Expression(Expression),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Equal,
    And,
    Greater,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    Variable(String),
    DivertTarget(String),
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum DynamicStringPart {
    Text(String),
    Expression(Expression),
    Sequence(Sequence),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DynamicString {
    pub parts: Vec<DynamicStringPart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignMode {
    Set,
    AddAssign,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalVariable {
    pub name: String,
    pub initial_value: Expression,
}

/// A `LIST name = item1, (item2), ...` declaration.
/// Items marked with `()` are the initially-selected values.
#[derive(Debug, Clone, PartialEq)]
pub struct ListDeclaration {
    pub name: String,
    /// All items in order: (item_name, value_number, initially_selected)
    pub items: Vec<(String, u32, bool)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Divert {
    pub target: String,
    pub arguments: Vec<Expression>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub display_text: String,
    pub selected_text: Option<String>,
    pub body: Vec<Node>,
    pub start_text: String,
    pub choice_only_text: String,
    pub conditions: Vec<Condition>,
    pub label: Option<String>,
    pub once_only: bool,
    pub is_invisible_default: bool,
    pub has_start_content: bool,
    pub has_choice_only_content: bool,
    pub start_tags: Vec<DynamicString>,
    pub choice_only_tags: Vec<DynamicString>,
    pub selected_tags: Vec<DynamicString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceMode {
    Stopping,
    Once,
    Cycle,
    Shuffle,
    ShuffleOnce,
    ShuffleStopping,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub mode: SequenceMode,
    pub branches: Vec<Vec<Node>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Text(String),
    OutputExpression(Expression),
    Newline,
    Tag(DynamicString),
    Glue,
    Sequence(Sequence),
    Divert(Divert),
    TunnelDivert(String),
    TunnelReturn,
    Conditional {
        condition: Condition,
        when_true: Vec<Node>,
        when_false: Option<Vec<Node>>,
    },
    ReturnBool(bool),
    Assignment {
        variable_name: String,
        expression: Expression,
        mode: AssignMode,
    },
    Choice(Choice),
    /// A gather point label `- (label)` — signals the emitter to name the next g-N container.
    GatherLabel(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Flow {
    pub name: String,
    pub parameters: Vec<String>,
    pub nodes: Vec<Node>,
    pub children: Vec<Flow>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ParsedStory {
    pub(crate) globals: Vec<GlobalVariable>,
    pub(crate) list_declarations: Vec<ListDeclaration>,
    pub(crate) root: Vec<Node>,
    pub(crate) flows: Vec<Flow>,
}

impl ParsedStory {
    pub fn new(globals: Vec<GlobalVariable>, root: Vec<Node>, flows: Vec<Flow>) -> Self {
        Self {
            globals,
            list_declarations: Vec::new(),
            root,
            flows,
        }
    }

    pub fn globals(&self) -> &[GlobalVariable] {
        &self.globals
    }

    pub fn list_declarations(&self) -> &[ListDeclaration] {
        &self.list_declarations
    }

    pub fn root(&self) -> &[Node] {
        &self.root
    }

    pub fn flows(&self) -> &[Flow] {
        &self.flows
    }
}
