use crate::error::CompilerError;
use bladeink::{CommandType, Glue, RTObject, Void};
use bladeink::{Container, Path};
use std::collections::HashSet;
use std::{collections::HashMap, rc::Rc};

use super::{
    AssignmentNode, ChoiceNode, ConditionalNode, DivertNode, DivertTarget, FlowArgument,
    FlowBase, FlowLevel, FunctionCall, GatherNode, GenerateIntoContainer, ObjectKind,
    ParsedObject, ParsedObjectRef, ParsedPath, Story, ValidationScope, VariableReference,
};

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
    AuthorWarning,
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
            Self::AuthorWarning => ObjectKind::AuthorWarning,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ParsedExpression {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
    StringExpression(Vec<ParsedNode>),
    Variable(VariableReference),
    DivertTarget(DivertTarget),
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
    FunctionCall(FunctionCall),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedAssignmentMode {
    Set,
    GlobalDecl,
    TempSet,
    AddAssign,
    SubtractAssign,
}

#[derive(Debug, Clone)]
pub struct ParsedNode {
    object: ParsedObject,
    kind: ParsedNodeKind,
    text: Option<String>,
    name: Option<String>,
    assignment_mode: Option<ParsedAssignmentMode>,
    assignment_target: Option<String>,
    target: Option<ParsedPath>,
    resolved_target: Option<ParsedObjectRef>,
    arguments: Vec<ParsedExpression>,
    expression: Option<ParsedExpression>,
    condition: Option<ParsedExpression>,
    children: Vec<ParsedNode>,
    // Choice / Gather specific
    pub indentation_depth: usize,
    pub once_only: bool,
    pub is_invisible_default: bool,
    pub start_content: Vec<ParsedNode>,
    pub choice_only_content: Vec<ParsedNode>,
    // Sequence specific
    pub sequence_type: u8,
    // Conditional branch specific
    pub is_else: bool,
    pub is_inline: bool,
    pub is_true_branch: bool,
    pub matching_equality: bool,
}

impl ParsedNode {
    pub fn new(kind: ParsedNodeKind) -> Self {
        Self {
            object: ParsedObject::new(kind.object_kind()),
            kind,
            text: None,
            name: None,
            assignment_mode: None,
            assignment_target: None,
            target: None,
            resolved_target: None,
            arguments: Vec::new(),
            expression: None,
            condition: None,
            children: Vec::new(),
            indentation_depth: 0,
            once_only: true,
            is_invisible_default: false,
            start_content: Vec::new(),
            choice_only_content: Vec::new(),
            sequence_type: 0,
            is_else: false,
            is_inline: false,
            is_true_branch: false,
            matching_equality: false,
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

    pub fn assignment_mode(&self) -> Option<ParsedAssignmentMode> {
        self.assignment_mode
    }

    pub fn assignment_target(&self) -> Option<&str> {
        self.assignment_target.as_deref()
    }

    pub fn target(&self) -> Option<&str> {
        self.target.as_ref().map(ParsedPath::as_str)
    }

    pub fn target_path(&self) -> Option<&ParsedPath> {
        self.target.as_ref()
    }

    pub fn arguments(&self) -> &[ParsedExpression] {
        &self.arguments
    }

    pub fn resolved_target(&self) -> Option<ParsedObjectRef> {
        self.resolved_target
    }

    pub fn expression(&self) -> Option<&ParsedExpression> {
        self.expression.as_ref()
    }

    pub fn children(&self) -> &[ParsedNode] {
        &self.children
    }

    pub fn start_content(&self) -> &[ParsedNode] {
        &self.start_content
    }

    pub fn choice_only_content(&self) -> &[ParsedNode] {
        &self.choice_only_content
    }

    pub fn condition(&self) -> Option<&ParsedExpression> {
        self.condition.as_ref()
    }

    pub fn runtime_object(&self) -> Option<Rc<dyn RTObject>> {
        self.object.runtime_object()
    }

    pub fn runtime_path(&self) -> Option<Path> {
        self.object.runtime_path()
    }

    pub fn container_for_counting(&self) -> Option<Rc<Container>> {
        self.object.container_for_counting()
    }

    pub fn sequence_type(&self) -> u8 {
        self.sequence_type
    }

    pub fn is_else_branch(&self) -> bool {
        self.is_else
    }

    pub fn is_inline_conditional(&self) -> bool {
        self.is_inline
    }

    pub fn is_true_branch(&self) -> bool {
        self.is_true_branch
    }

    pub fn matches_switch_value(&self) -> bool {
        self.matching_equality
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_assignment(mut self, mode: ParsedAssignmentMode, target: impl Into<String>) -> Self {
        self.assignment_mode = Some(mode);
        self.assignment_target = Some(target.into());
        self
    }

    pub fn with_target(mut self, target: impl Into<ParsedPath>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_arguments(mut self, arguments: Vec<ParsedExpression>) -> Self {
        self.arguments = arguments;
        self
    }

    pub fn set_resolved_target(&mut self, resolved_target: ParsedObjectRef) {
        self.resolved_target = Some(resolved_target);
    }

    pub fn with_expression(mut self, expression: ParsedExpression) -> Self {
        self.expression = Some(expression);
        self
    }

    pub fn with_condition(mut self, condition: ParsedExpression) -> Self {
        self.condition = Some(condition);
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

    pub fn resolve_references(&mut self) {
        for child in &mut self.start_content {
            child.resolve_references();
        }
        for child in &mut self.choice_only_content {
            child.resolve_references();
        }
        for child in &mut self.children {
            child.resolve_references();
        }
    }

    pub(crate) fn resolve_targets(&mut self, story: &Story) {
        if let Some(expression) = self.expression.as_mut() {
            expression.resolve_targets(story);
        }
        if let Some(condition) = self.condition.as_mut() {
            condition.resolve_targets(story);
        }

        if matches!(
            self.kind(),
            ParsedNodeKind::Divert
                | ParsedNodeKind::TunnelDivert
                | ParsedNodeKind::TunnelOnwardsWithTarget
                | ParsedNodeKind::ThreadDivert
        ) && let Some(target) = self.target()
            && target != "END"
            && target != "DONE"
            && let Some(resolved) = story.resolve_target_ref(target)
        {
            self.set_resolved_target(resolved);
        }

        for child in &mut self.start_content {
            child.resolve_targets(story);
        }
        for child in &mut self.choice_only_content {
            child.resolve_targets(story);
        }
        for child in &mut self.children {
            child.resolve_targets(story);
        }
    }

    pub(crate) fn mark_count_target(&mut self, target: ParsedObjectRef, count_turns: bool) -> bool {
        if self.object().reference() == target {
            if count_turns {
                self.object().mark_turn_index_should_be_counted();
            } else {
                self.object().mark_visits_should_be_counted();
            }
            return true;
        }

        for child in &mut self.start_content {
            if child.mark_count_target(target, count_turns) {
                return true;
            }
        }
        for child in &mut self.choice_only_content {
            if child.mark_count_target(target, count_turns) {
                return true;
            }
        }
        for child in &mut self.children {
            if child.mark_count_target(target, count_turns) {
                return true;
            }
        }

        false
    }

    pub(super) fn validate(&self, scope: &ValidationScope, story: &Story) -> Result<(), CompilerError> {
        if let Some(expression) = self.expression() {
            expression.validate(scope, story)?;
        }
        if let Some(condition) = self.condition() {
            condition.validate(scope, story)?;
        }

        for child in &self.start_content {
            child.validate(scope, story)?;
        }
        for child in &self.choice_only_content {
            child.validate(scope, story)?;
        }

        if let Some(divert) = DivertNode::from_node(self) {
            divert.validate(scope, story)?;
        }

        if let Some(conditional) = ConditionalNode::from_node(self)
            && self
                .children()
                .iter()
                .all(|child| child.kind() == ParsedNodeKind::Conditional)
        {
            conditional.validate(scope, story)?;
            return Ok(());
        }

        ParsedNode::validate_list(self.children(), scope, story)
    }

    pub(crate) fn validate_list(
        nodes: &[ParsedNode],
        scope: &ValidationScope,
        story: &Story,
    ) -> Result<(), CompilerError> {
        let mut seen_gather_labels = HashSet::new();
        for node in nodes {
            if let Some(gather) = GatherNode::from_node(node) {
                gather.validate_scope_label(&mut seen_gather_labels)?;
            }
        }

        for node in nodes {
            node.validate(scope, story)?;
        }

        Ok(())
    }

    pub(crate) fn contains_choice_content(&self) -> bool {
        self.children().iter().any(|child| {
            child.kind() == ParsedNodeKind::Choice
                || child.contains_choice_content()
                || child.start_content.iter().any(ParsedNode::contains_choice_content)
                || child.choice_only_content.iter().any(ParsedNode::contains_choice_content)
        })
    }

    pub(crate) fn collect_named_labels(&self, names: &mut HashSet<String>) -> Result<(), CompilerError> {
        if let Some(choice) = ChoiceNode::from_node(self) {
            choice.collect_named_label(names)?;
        }
        if let Some(gather) = GatherNode::from_node(self) {
            gather.collect_named_label(names)?;
        }

        for child in &self.start_content {
            child.collect_named_labels(names)?;
        }
        for child in &self.choice_only_content {
            child.collect_named_labels(names)?;
        }
        for child in self.children() {
            child.collect_named_labels(names)?;
        }

        Ok(())
    }

    pub(crate) fn collect_temp_vars(&self, names: &mut HashSet<String>) {
        if let Some(assignment) = AssignmentNode::from_node(self) {
            assignment.collect_temp_var(names);
        }

        for child in &self.start_content {
            child.collect_temp_vars(names);
        }
        for child in &self.choice_only_content {
            child.collect_temp_vars(names);
        }
        for child in self.children() {
            child.collect_temp_vars(names);
        }
    }

    pub(crate) fn collect_global_declared_vars(&self, names: &mut HashSet<String>) {
        if let Some(assignment) = AssignmentNode::from_node(self) {
            assignment.collect_global_declared_var(names);
        }

        for child in &self.start_content {
            child.collect_global_declared_vars(names);
        }
        for child in &self.choice_only_content {
            child.collect_global_declared_vars(names);
        }
        for child in self.children() {
            child.collect_global_declared_vars(names);
        }
    }

    pub(crate) fn export_runtime_nodes(
        state: &crate::runtime_export::ExportState,
        nodes: &[ParsedNode],
        scope: crate::runtime_export::Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        container_path: Option<&str>,
        content_index_offset: usize,
    ) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
        let mut content = Vec::new();

        for node in nodes {
            node.generate_into_container(
                state,
                scope,
                story,
                named_paths,
                container_path,
                content_index_offset,
                &mut content,
            )?;
        }

        Ok(content)
    }

    pub(crate) fn export_runtime(
        &self,
        state: &crate::runtime_export::ExportState,
        scope: crate::runtime_export::Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        _container_path: Option<&str>,
        node_path: Option<&str>,
        content_index_offset: usize,
        content: &mut Vec<Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError> {
        match self.kind() {
            ParsedNodeKind::Text => {
                if let Some(text) = self.text()
                    && !text.is_empty()
                {
                    content.push(crate::runtime_export::rt_value(text));
                }
            }
            ParsedNodeKind::Newline => content.push(crate::runtime_export::rt_value("\n")),
            ParsedNodeKind::Glue => content.push(Rc::new(Glue::new())),
            ParsedNodeKind::Divert
            | ParsedNodeKind::TunnelDivert
            | ParsedNodeKind::TunnelOnwardsWithTarget
            | ParsedNodeKind::ThreadDivert => {
                self.as_divert()
                    .ok_or_else(|| CompilerError::unsupported_feature("runtime export divert shape"))?
                    .export_runtime(state, scope, story, named_paths, content)?;
            }
            ParsedNodeKind::TunnelReturn => {
                content.push(crate::runtime_export::command(CommandType::EvalStart));
                content.push(Rc::new(Void::new()));
                content.push(crate::runtime_export::command(CommandType::EvalEnd));
                content.push(crate::runtime_export::command(CommandType::PopTunnel));
            }
            ParsedNodeKind::ReturnExpression => {
                let expression = self.expression().ok_or_else(|| {
                    CompilerError::unsupported_feature("runtime export return expression missing expression")
                })?;
                content.push(crate::runtime_export::command(CommandType::EvalStart));
                crate::runtime_export::export_expression(expression, story, content)?;
                content.push(crate::runtime_export::command(CommandType::EvalEnd));
                content.push(crate::runtime_export::command(CommandType::PopFunction));
            }
            ParsedNodeKind::ReturnVoid => {
                content.push(crate::runtime_export::command(CommandType::EvalStart));
                content.push(Rc::new(Void::new()));
                content.push(crate::runtime_export::command(CommandType::EvalEnd));
                content.push(crate::runtime_export::command(CommandType::PopFunction));
            }
            ParsedNodeKind::VoidCall => {
                let expression = self.expression().ok_or_else(|| {
                    CompilerError::unsupported_feature("runtime export void call missing expression")
                })?;
                content.push(crate::runtime_export::command(CommandType::EvalStart));
                crate::runtime_export::export_expression(expression, story, content)?;
                content.push(crate::runtime_export::command(CommandType::PopEvaluatedValue));
                content.push(crate::runtime_export::command(CommandType::EvalEnd));
                content.push(crate::runtime_export::rt_value("\n"));
            }
            ParsedNodeKind::Conditional | ParsedNodeKind::SwitchConditional => {
                let conditional = ConditionalNode::from_node(self).ok_or_else(|| {
                    CompilerError::unsupported_feature("runtime export conditional shape")
                })?;
                if crate::runtime_export::conditional_is_simple(conditional) {
                    conditional.append_simple_runtime(
                        state,
                        scope,
                        story,
                        named_paths,
                        node_path,
                        content_index_offset,
                        content,
                    )?;
                } else {
                    content.push(conditional.export_runtime(
                        state,
                        scope,
                        story,
                        named_paths,
                        node_path,
                    )?);
                }
            }
            ParsedNodeKind::OutputExpression => {
                let expression = self.expression().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export output expression missing expression",
                    )
                })?;
                content.push(crate::runtime_export::command(CommandType::EvalStart));
                crate::runtime_export::export_output_expression(
                    expression,
                    scope,
                    story,
                    named_paths,
                    content,
                )?;
                content.push(crate::runtime_export::command(CommandType::EvalOutput));
                content.push(crate::runtime_export::command(CommandType::EvalEnd));
            }
            ParsedNodeKind::Assignment => {
                self.as_assignment()
                    .ok_or_else(|| CompilerError::unsupported_feature("runtime export assignment shape"))?
                    .export_runtime(scope, story, content)?;
            }
            ParsedNodeKind::Tag => {
                content.push(crate::runtime_export::command(CommandType::BeginTag));
                content.extend(ParsedNode::export_runtime_nodes(
                    state,
                    self.children(),
                    scope,
                    story,
                    named_paths,
                    None,
                    0,
                )?);
                content.push(crate::runtime_export::command(CommandType::EndTag));
            }
            ParsedNodeKind::AuthorWarning => {}
            ParsedNodeKind::Sequence => {
                let sequence = self.as_sequence().ok_or_else(|| {
                    CompilerError::unsupported_feature("runtime export sequence shape")
                })?;
                content.push(sequence.export_runtime(state, scope, story, named_paths, node_path)?);
            }
            ParsedNodeKind::Choice
            | ParsedNodeKind::GatherPoint
            | ParsedNodeKind::GatherLabel
            | ParsedNodeKind::ReturnBool => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support {:?} yet",
                    self.kind()
                )))
            }
        }

        Ok(())
    }
}

impl ParsedExpression {
    pub fn variable(name: impl Into<String>) -> Self {
        Self::Variable(VariableReference::new(ParsedPath::from(name.into())))
    }

    pub fn divert_target(target: impl Into<String>) -> Self {
        Self::DivertTarget(DivertTarget::new(ParsedPath::from(target.into())))
    }

    pub fn function_call(name: impl Into<String>, arguments: Vec<ParsedExpression>) -> Self {
        Self::FunctionCall(FunctionCall::new(ParsedPath::from(name.into()), arguments))
    }

    pub fn variable_name(&self) -> Option<&str> {
        match self {
            ParsedExpression::Variable(reference) => Some(reference.path().as_str()),
            _ => None,
        }
    }

    pub fn divert_target_name(&self) -> Option<&str> {
        match self {
            ParsedExpression::DivertTarget(target) => Some(target.target_path().as_str()),
            _ => None,
        }
    }

    pub fn function_call_name(&self) -> Option<&str> {
        match self {
            ParsedExpression::FunctionCall(call) => Some(call.path().as_str()),
            _ => None,
        }
    }

    pub fn function_call_arguments(&self) -> Option<&[ParsedExpression]> {
        match self {
            ParsedExpression::FunctionCall(call) => Some(call.arguments()),
            _ => None,
        }
    }

    pub fn resolved_target(&self) -> Option<ParsedObjectRef> {
        match self {
            ParsedExpression::DivertTarget(target) => target.resolved_target(),
            ParsedExpression::FunctionCall(call) => call.resolved_target(),
            _ => None,
        }
    }

    pub fn resolved_count_target(&self) -> Option<ParsedObjectRef> {
        match self {
            ParsedExpression::Variable(reference) => reference.resolved_count_target(),
            _ => None,
        }
    }

    pub fn resolve_targets(&mut self, story: &Story) {
        match self {
            ParsedExpression::Variable(reference) => reference.resolve_targets(story),
            ParsedExpression::DivertTarget(target) => target.resolve_targets(story),
            ParsedExpression::Unary { expression, .. } => expression.resolve_targets(story),
            ParsedExpression::Binary { left, right, .. } => {
                left.resolve_targets(story);
                right.resolve_targets(story);
            }
            ParsedExpression::FunctionCall(call) => call.resolve_targets(story),
            ParsedExpression::StringExpression(nodes) => {
                for node in nodes {
                    node.resolve_targets(story);
                }
            }
            ParsedExpression::Bool(_)
            | ParsedExpression::Int(_)
            | ParsedExpression::Float(_)
            | ParsedExpression::String(_)
            | ParsedExpression::ListItems(_)
            | ParsedExpression::EmptyList => {}
        }
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

    pub fn resolve_references(&mut self) {
        for node in &mut self.content {
            node.resolve_references();
        }
        for child in &mut self.children {
            child.resolve_references();
        }
    }

    pub(crate) fn resolve_targets(&mut self, story: &Story) {
        for node in &mut self.content {
            node.resolve_targets(story);
        }
        for child in &mut self.children {
            child.resolve_targets(story);
        }
    }

    pub(crate) fn mark_count_target(&mut self, target: ParsedObjectRef, count_turns: bool) -> bool {
        if self.object().reference() == target {
            if count_turns {
                self.object().mark_turn_index_should_be_counted();
            } else {
                self.object().mark_visits_should_be_counted();
            }
            return true;
        }

        for node in &mut self.content {
            if node.mark_count_target(target, count_turns) {
                return true;
            }
        }
        for child in &mut self.children {
            if child.mark_count_target(target, count_turns) {
                return true;
            }
        }
        false
    }
}

impl ParsedExpression {
    pub(super) fn validate(&self, scope: &ValidationScope, story: &Story) -> Result<(), CompilerError> {
        match self {
            ParsedExpression::Variable(reference) => {
                VariableReference::validate_name(reference.path().as_str(), scope, story)?;
            }
            ParsedExpression::DivertTarget(target) => {
                DivertTarget::validate_explicit_target(target.target_path().as_str(), scope, story)?;
            }
            ParsedExpression::Unary { expression, .. } => {
                expression.validate(scope, story)?;
            }
            ParsedExpression::Binary { left, right, .. } => {
                left.validate(scope, story)?;
                right.validate(scope, story)?;
            }
            ParsedExpression::FunctionCall(call) => call.validate(scope, story)?,
            ParsedExpression::StringExpression(nodes) => {
                ParsedNode::validate_list(nodes, scope, story)?;
            }
            ParsedExpression::Bool(_)
            | ParsedExpression::Int(_)
            | ParsedExpression::Float(_)
            | ParsedExpression::String(_)
            | ParsedExpression::ListItems(_)
            | ParsedExpression::EmptyList => {}
        }

        Ok(())
    }
}
