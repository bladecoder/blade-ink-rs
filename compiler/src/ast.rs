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
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    GreaterEqual,
    And,
    Or,
    Greater,
    /// List `?` (has / contains)
    Has,
    /// List `!?` (hasn't / doesn't contain)
    Hasnt,
    /// List `^` (intersect)
    Intersect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    Variable(String),
    DivertTarget(String),
    Negate(Box<Expression>),
    Not(Box<Expression>),
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },
    /// A list literal: `(a, b, c)` — items are bare names (resolved to qualified names in emitter)
    ListItems(Vec<String>),
    /// A list literal that is already empty: `()`
    EmptyList,
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
    TempSet,
    AddAssign,
    SubtractAssign,
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
    /// Nesting level of this choice: 1 for `*`/`+`, 2 for `**`/`++`, etc.
    pub nesting_level: usize,
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
    TunnelDivert {
        target: String,
        is_variable: bool,
        args: Vec<Expression>,
    },
    TunnelReturn,
    Conditional {
        condition: Condition,
        when_true: Vec<Node>,
        when_false: Option<Vec<Node>>,
    },
    /// A switch-style conditional: `{ expr: - Case1: body - Case2: body - else: body }`
    /// Each branch is (Some(case_expr), body_nodes) for cases, or (None, body_nodes) for `else`.
    SwitchConditional {
        value: Expression,
        branches: Vec<(Option<Expression>, Vec<Node>)>,
    },
    /// A thread divert: `<- target(args)`
    ThreadDivert(Divert),
    ReturnBool(bool),
    /// Return with an arbitrary expression value
    ReturnExpr(Expression),
    /// Return void (bare `~ return` or `~return`)
    ReturnVoid,
    Assignment {
        variable_name: String,
        expression: Expression,
        mode: AssignMode,
    },
    Choice(Choice),
    /// An anonymous gather point `-` with no content and no label.
    /// Acts as a separator between choice blocks at different nesting levels.
    GatherPoint,
    /// A gather point label `- (label)` — signals the emitter to name the next g-N container.
    GatherLabel(String),
    /// A void function call statement: `~ funcname(args)`
    VoidCall {
        name: String,
        args: Vec<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Flow {
    pub name: String,
    pub is_function: bool,
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
    pub(crate) external_functions: Vec<String>,
    /// CONST declarations: name → compile-time literal value
    pub(crate) consts: std::collections::HashMap<String, Expression>,
}

impl ParsedStory {
    pub fn new(globals: Vec<GlobalVariable>, root: Vec<Node>, flows: Vec<Flow>) -> Self {
        Self {
            globals,
            list_declarations: Vec::new(),
            root,
            flows,
            external_functions: Vec::new(),
            consts: std::collections::HashMap::new(),
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
