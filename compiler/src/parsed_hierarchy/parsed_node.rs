use super::{FlowArgument, FlowBase, FlowLevel, ObjectKind, ParsedObject, ParsedObjectRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedNodeKind {
    Text,
    OutputExpression,
    Newline,
    Tag,
    Glue,
    Sequence,
    Divert,
    TunnelDivert,
    TunnelReturn,
    TunnelOnwardsWithTarget,
    Conditional,
    SwitchConditional,
    ThreadDivert,
    ReturnBool,
    ReturnExpression,
    ReturnVoid,
    Assignment,
    Choice,
    GatherPoint,
    GatherLabel,
    VoidCall,
}

impl ParsedNodeKind {
    pub fn object_kind(self) -> ObjectKind {
        match self {
            Self::Text => ObjectKind::Text,
            Self::OutputExpression => ObjectKind::Expression,
            Self::Newline => ObjectKind::Text,
            Self::Tag => ObjectKind::Tag,
            Self::Glue => ObjectKind::Text,
            Self::Sequence => ObjectKind::Sequence,
            Self::Divert | Self::TunnelDivert | Self::ThreadDivert => ObjectKind::DivertTarget,
            Self::TunnelReturn | Self::TunnelOnwardsWithTarget => ObjectKind::TunnelOnwards,
            Self::Conditional | Self::SwitchConditional => ObjectKind::Conditional,
            Self::ReturnBool | Self::ReturnExpression | Self::ReturnVoid => ObjectKind::Return,
            Self::Assignment => ObjectKind::VariableAssignment,
            Self::Choice => ObjectKind::Choice,
            Self::GatherPoint | Self::GatherLabel => ObjectKind::Gather,
            Self::VoidCall => ObjectKind::FunctionCall,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedExpression {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
    Variable(String),
    DivertTarget(String),
    ListItems(Vec<String>),
    EmptyList,
    Unary {
        operator: String,
        expression: Box<ParsedExpression>,
    },
    Binary {
        left: Box<ParsedExpression>,
        operator: String,
        right: Box<ParsedExpression>,
    },
    FunctionCall {
        name: String,
        arguments: Vec<ParsedExpression>,
    },
}

#[derive(Debug, Clone)]
pub struct ParsedNode {
    object: ParsedObject,
    kind: ParsedNodeKind,
    text: Option<String>,
    name: Option<String>,
    target: Option<String>,
    arguments: Vec<ParsedExpression>,
    expression: Option<ParsedExpression>,
    children: Vec<ParsedNode>,
    // Choice / Gather specific
    pub indentation_depth: usize,
    pub once_only: bool,
    pub is_invisible_default: bool,
    pub start_content: Vec<ParsedNode>,
    pub choice_only_content: Vec<ParsedNode>,
}

impl ParsedNode {
    pub fn new(kind: ParsedNodeKind) -> Self {
        Self {
            object: ParsedObject::new(kind.object_kind()),
            kind,
            text: None,
            name: None,
            target: None,
            arguments: Vec::new(),
            expression: None,
            children: Vec::new(),
            indentation_depth: 0,
            once_only: true,
            is_invisible_default: false,
            start_content: Vec::new(),
            choice_only_content: Vec::new(),
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn kind(&self) -> ParsedNodeKind {
        self.kind
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub fn arguments(&self) -> &[ParsedExpression] {
        &self.arguments
    }

    pub fn expression(&self) -> Option<&ParsedExpression> {
        self.expression.as_ref()
    }

    pub fn children(&self) -> &[ParsedNode] {
        &self.children
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_arguments(mut self, arguments: Vec<ParsedExpression>) -> Self {
        self.arguments = arguments;
        self
    }

    pub fn with_expression(mut self, expression: ParsedExpression) -> Self {
        self.expression = Some(expression);
        self
    }

    pub fn with_children(mut self, children: Vec<ParsedNode>) -> Self {
        self.set_children(children);
        self
    }

    pub fn set_children(&mut self, mut children: Vec<ParsedNode>) {
        self.object = ParsedObject::new(self.kind.object_kind());
        for child in &mut children {
            child.object_mut().set_parent(&self.object);
            self.object.add_content_ref(child.object().reference());
        }
        self.children = children;
    }
}

#[derive(Debug, Clone)]
pub struct ParsedFlow {
    flow: FlowBase,
    content: Vec<ParsedNode>,
    children: Vec<ParsedFlow>,
}

impl ParsedFlow {
    pub fn new(
        identifier: impl Into<String>,
        flow_level: FlowLevel,
        arguments: Vec<FlowArgument>,
        is_function: bool,
        mut content: Vec<ParsedNode>,
        mut children: Vec<ParsedFlow>,
    ) -> Self {
        let mut flow = FlowBase::new(flow_level, Some(identifier.into()), arguments, is_function);

        for node in &mut content {
            node.object_mut().set_parent(flow.object());
            flow.object_mut().add_content_ref(node.object().reference());
        }

        for child in &mut children {
            child.object_mut().set_parent(flow.object());
            flow.object_mut()
                .add_content_ref(child.object().reference());
        }

        Self {
            flow,
            content,
            children,
        }
    }

    pub fn flow(&self) -> &FlowBase {
        &self.flow
    }

    pub fn object(&self) -> &ParsedObject {
        self.flow.object()
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        self.flow.object_mut()
    }

    pub fn content(&self) -> &[ParsedNode] {
        &self.content
    }

    pub fn children(&self) -> &[ParsedFlow] {
        &self.children
    }

    pub fn reference(&self) -> ParsedObjectRef {
        self.object().reference()
    }
}
