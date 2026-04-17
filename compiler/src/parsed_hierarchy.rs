use std::collections::{BTreeMap, BTreeSet};

use bladeink::story::INK_VERSION_CURRENT;
use serde_json::{json, Map, Value};

use crate::error::CompilerError;

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
    globals: Vec<GlobalVariable>,
    root: Vec<Node>,
    flows: Vec<Flow>,
}

#[derive(Debug, Default)]
struct EmittedContainer {
    content: Vec<Value>,
    named: Map<String, Value>,
}

#[derive(Clone)]
struct EmitScope {
    path: String,
    top_flow_name: Option<String>,
    child_flow_names: BTreeSet<String>,
    choice_label_targets: BTreeMap<String, String>,
}

struct EmitContext {
    global_variables: BTreeSet<String>,
    top_flow_names: BTreeSet<String>,
}

impl EmittedContainer {
    fn push(&mut self, value: Value) {
        self.content.push(value);
    }

    fn insert_named(&mut self, name: String, value: Value) {
        self.named.insert(name, value);
    }

    fn into_json_array(
        self,
        name: Option<&str>,
        count_flags: Option<i32>,
    ) -> Result<Value, CompilerError> {
        let mut values = self.content;
        let has_name = name.is_some();
        let has_flags = count_flags.unwrap_or_default() > 0;

        if !self.named.is_empty() || has_name || has_flags {
            let mut terminator = self.named;

            if let Some(flags) = count_flags.filter(|flags| *flags > 0) {
                terminator.insert("#f".to_owned(), json!(flags));
            }

            if let Some(name) = name {
                terminator.insert("#n".to_owned(), json!(name));
            }

            values.push(Value::Object(terminator));
        } else {
            values.push(Value::Null);
        }

        Ok(Value::Array(values))
    }
}

impl ParsedStory {
    pub fn new(globals: Vec<GlobalVariable>, root: Vec<Node>, flows: Vec<Flow>) -> Self {
        Self {
            globals,
            root,
            flows,
        }
    }

    pub fn to_json_value(&self) -> Result<Value, CompilerError> {
        let context = EmitContext::new(self);
        let root_scope = EmitScope::root(&self.flows);

        let mut root_container = emit_nodes(&self.root, &root_scope, &context)?;
        root_container.push(json!(["done", {"#n": "g-0"}]));

        let mut named_content = Map::new();

        for flow in &self.flows {
            named_content.insert(flow.name.clone(), emit_flow(flow, &context)?);
        }

        if !self.globals.is_empty() {
            named_content.insert(
                "global decl".to_owned(),
                emit_global_declarations(&self.globals)?.into_json_array(None, None)?,
            );
        }

        Ok(json!({
            "inkVersion": INK_VERSION_CURRENT,
            "root": [
                root_container.into_json_array(None, None)?,
                "done",
                if named_content.is_empty() {
                    Value::Null
                } else {
                    Value::Object(named_content)
                }
            ],
            "listDefs": {}
        }))
    }

    pub fn to_json_string(&self) -> Result<String, CompilerError> {
        let json = self.to_json_value()?;
        serde_json::to_string(&json).map_err(|error| {
            CompilerError::InvalidSource(format!("failed to serialize compiled ink: {error}"))
        })
    }

    #[allow(dead_code)]
    pub(crate) fn globals(&self) -> &[GlobalVariable] {
        &self.globals
    }

    #[allow(dead_code)]
    pub(crate) fn root(&self) -> &[Node] {
        &self.root
    }

    #[allow(dead_code)]
    pub(crate) fn flows(&self) -> &[Flow] {
        &self.flows
    }
}

impl EmitContext {
    fn new(story: &ParsedStory) -> Self {
        Self {
            global_variables: story.globals.iter().map(|var| var.name.clone()).collect(),
            top_flow_names: story.flows.iter().map(|flow| flow.name.clone()).collect(),
        }
    }
}

impl EmitScope {
    fn root(flows: &[Flow]) -> Self {
        Self {
            path: "0".to_owned(),
            top_flow_name: None,
            child_flow_names: flows.iter().map(|flow| flow.name.clone()).collect(),
            choice_label_targets: BTreeMap::new(),
        }
    }

    fn child_flow(&self, child: &Flow) -> Self {
        let path = if self.path == "0" {
            child.name.clone()
        } else {
            format!("{}.{}", self.path, child.name)
        };

        Self {
            path,
            top_flow_name: self
                .top_flow_name
                .clone()
                .or_else(|| Some(child.name.clone())),
            child_flow_names: child
                .children
                .iter()
                .map(|nested| nested.name.clone())
                .collect(),
            choice_label_targets: BTreeMap::new(),
        }
    }

    fn choice_branch(&self, branch_name: &str) -> Self {
        Self {
            path: format!("{}.{}", self.path, branch_name),
            top_flow_name: self.top_flow_name.clone(),
            child_flow_names: self.child_flow_names.clone(),
            choice_label_targets: self.choice_label_targets.clone(),
        }
    }

    fn with_choice_labels(&self, labels: BTreeMap<String, String>) -> Self {
        Self {
            path: self.path.clone(),
            top_flow_name: self.top_flow_name.clone(),
            child_flow_names: self.child_flow_names.clone(),
            choice_label_targets: labels,
        }
    }

    fn resolve_divert_target(&self, target: &str, context: &EmitContext) -> String {
        if target == "END" || target == "DONE" || target.contains('.') {
            return target.to_owned();
        }

        if let Some(choice_target) = self.resolve_choice_label(target) {
            return format!(".^.{}", choice_target);
        }

        if context.global_variables.contains(target) && !context.top_flow_names.contains(target) {
            return target.to_owned();
        }

        if self.child_flow_names.contains(target) {
            if let Some(top_flow_name) = &self.top_flow_name {
                return format!("{top_flow_name}.{target}");
            }
        }

        target.to_owned()
    }

    fn is_variable_divert(&self, target: &str, context: &EmitContext) -> bool {
        context.global_variables.contains(target) && !context.top_flow_names.contains(target)
    }

    fn resolve_choice_label(&self, label: &str) -> Option<&str> {
        self.choice_label_targets.get(label).map(String::as_str)
    }
}

fn emit_global_declarations(globals: &[GlobalVariable]) -> Result<EmittedContainer, CompilerError> {
    let mut container = EmittedContainer::default();
    container.push(json!("ev"));

    for global in globals {
        emit_expression(&global.initial_value, &mut container.content);
        container.push(json!({ "VAR=": global.name }));
    }

    container.push(json!("/ev"));
    container.push(json!("end"));

    Ok(container)
}

fn emit_flow(flow: &Flow, context: &EmitContext) -> Result<Value, CompilerError> {
    let parent_scope = EmitScope::root(&[]);
    let scope = parent_scope.child_flow(flow);
    let mut container = emit_nodes(&flow.nodes, &scope, context)?;

    prepend_parameters(&mut container, &flow.parameters);

    if container.content.is_empty() && !flow.children.is_empty() {
        let target = if let Some(top_flow_name) = &scope.top_flow_name {
            format!("{top_flow_name}.{}", flow.children[0].name)
        } else {
            flow.children[0].name.clone()
        };
        container.push(json!({"->": target}));
    }

    for child in &flow.children {
        container.insert_named(
            child.name.clone(),
            emit_nested_flow(child, &scope, context)?,
        );
    }

    container.into_json_array(None, Some(1))
}

fn emit_nested_flow(
    flow: &Flow,
    parent_scope: &EmitScope,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let scope = parent_scope.child_flow(flow);
    let mut container = emit_nodes(&flow.nodes, &scope, context)?;

    prepend_parameters(&mut container, &flow.parameters);

    if container.content.is_empty() && !flow.children.is_empty() {
        let target = if let Some(top_flow_name) = &scope.top_flow_name {
            format!("{top_flow_name}.{}", flow.children[0].name)
        } else {
            flow.children[0].name.clone()
        };
        container.push(json!({"->": target}));
    }

    for child in &flow.children {
        container.insert_named(
            child.name.clone(),
            emit_nested_flow(child, &scope, context)?,
        );
    }

    container.into_json_array(None, Some(1))
}

fn prepend_parameters(container: &mut EmittedContainer, parameters: &[String]) {
    if parameters.is_empty() {
        return;
    }

    let mut prefix: Vec<Value> = parameters
        .iter()
        .rev()
        .map(|parameter| json!({"temp=": parameter}))
        .collect();
    prefix.append(&mut container.content);
    container.content = prefix;
}

fn emit_nodes(
    nodes: &[Node],
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<EmittedContainer, CompilerError> {
    let mut out = EmittedContainer::default();
    let mut next_choice_index = 0;

    let mut index = 0;
    while index < nodes.len() {
        match &nodes[index] {
            Node::Text(text) => out.push(json!(format!("^{text}"))),
            Node::OutputExpression(expression) => {
                out.push(json!("ev"));
                emit_expression(expression, &mut out.content);
                out.push(json!("out"));
                out.push(json!("/ev"));
            }
            Node::Newline => out.push(json!("\n")),
            Node::Tag(tag) => emit_tag(tag, &mut out.content, scope, context)?,
            Node::Glue => out.push(json!("<>")),
            Node::Sequence(_) => {
                let block_start = index;
                while index < nodes.len() && matches!(nodes[index], Node::Sequence(_)) {
                    index += 1;
                }
                emit_sequence_block(
                    &mut out,
                    &nodes[block_start..index],
                    scope,
                    next_choice_index,
                    context,
                )?;
                continue;
            }
            Node::Divert(divert) => emit_divert(&mut out, divert, scope, context),
            Node::TunnelDivert(target) => out.push(json!({"->t->": target})),
            Node::TunnelReturn => {
                out.push(json!("ev"));
                out.push(json!("void"));
                out.push(json!("/ev"));
                out.push(json!("->->"));
            }
            Node::ReturnBool(value) => {
                out.push(json!("ev"));
                out.push(json!(value));
                out.push(json!("/ev"));
                out.push(json!("~ret"));
            }
            Node::Conditional {
                condition,
                when_true,
                when_false,
            } => out.content.extend(emit_conditional(
                condition,
                when_true,
                when_false.as_deref(),
                scope,
                out.content.len(),
                context,
            )?),
            Node::Assignment {
                variable_name,
                expression,
                mode,
            } => emit_assignment(variable_name, expression, mode, &mut out),
            Node::Choice(_) => {
                let block_start = index;
                while index < nodes.len() && matches!(nodes[index], Node::Choice(_)) {
                    index += 1;
                }
                let continuation = &nodes[index..];
                emit_choice_block(
                    &mut out,
                    &nodes[block_start..index],
                    continuation,
                    scope,
                    &mut next_choice_index,
                    context,
                )?;
                break;
            }
        }

        index += 1;
    }

    Ok(out)
}

fn emit_choice_block(
    out: &mut EmittedContainer,
    choices: &[Node],
    continuation: &[Node],
    scope: &EmitScope,
    next_choice_index: &mut usize,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    let mut choice_labels = BTreeMap::new();
    for (offset, choice) in choices.iter().enumerate() {
        let Node::Choice(choice) = choice else {
            continue;
        };
        if let Some(label) = &choice.label {
            choice_labels.insert(label.clone(), format!("c-{}", *next_choice_index + offset));
        }
    }
    let block_scope = scope.with_choice_labels(choice_labels);

    let continuation_name = if continuation.is_empty() {
        None
    } else {
        let name = format!("g-{}", *next_choice_index);
        let continuation_scope = block_scope.choice_branch(&name);
        let continuation_value =
            emit_nodes(continuation, &continuation_scope, context)?.into_json_array(None, None)?;
        out.insert_named(name.clone(), continuation_value);
        Some((name.clone(), format!(".^.{}", name)))
    };

    for choice in choices {
        let Node::Choice(choice) = choice else {
            continue;
        };
        emit_choice(
            out,
            choice,
            &block_scope,
            *next_choice_index,
            continuation_name.as_ref().map(|(_, path)| path.as_str()),
            context,
        )?;
        *next_choice_index += 1;
    }

    Ok(())
}

fn emit_sequence_block(
    out: &mut EmittedContainer,
    sequences: &[Node],
    scope: &EmitScope,
    _next_index: usize,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    for sequence in sequences {
        let Node::Sequence(sequence) = sequence else {
            continue;
        };
        out.push(emit_sequence(sequence, scope, context)?);
    }

    Ok(())
}

fn emit_sequence(
    sequence: &Sequence,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let has_once_fallthrough = matches!(
        sequence.mode,
        SequenceMode::Once | SequenceMode::ShuffleOnce
    );
    let authored_branch_count = sequence.branches.len();
    let branch_count = authored_branch_count + usize::from(has_once_fallthrough);
    let max_index = branch_count.saturating_sub(1) as i32;
    let mut out = vec![json!("ev"), json!("visit")];

    match sequence.mode {
        SequenceMode::Stopping | SequenceMode::Once => {
            out.push(json!(max_index));
            out.push(json!("MIN"));
        }
        SequenceMode::Cycle => {
            out.push(json!(branch_count as i32));
            out.push(json!("%"));
        }
        SequenceMode::Shuffle => {
            out.push(json!(branch_count as i32));
            out.push(json!("seq"));
        }
        SequenceMode::ShuffleOnce | SequenceMode::ShuffleStopping => {
            out.push(json!(max_index));
            out.push(json!("MIN"));
            out.push(json!("du"));
            out.push(json!(max_index));
            out.push(json!("=="));
            out.push(json!({"->": ".^.10", "c": true}));
            out.push(json!(max_index));
            out.push(json!("seq"));
            out.push(json!("nop"));
        }
    }
    out.push(json!("/ev"));

    for (index, _) in sequence.branches.iter().enumerate() {
        out.push(json!("ev"));
        out.push(json!("du"));
        out.push(json!(index as i32));
        out.push(json!("=="));
        out.push(json!("/ev"));
        out.push(json!({"->": format!(".^.s{index}"), "c": true}));
    }
    let rejoin_index = out.len();
    out.push(json!("nop"));

    let mut named = Map::new();
    for (index, branch) in sequence.branches.iter().enumerate() {
        let branch_scope = scope.choice_branch(&format!("s{index}"));
        let mut branch_container = emit_nodes(branch, &branch_scope, context)?;
        branch_container.content.insert(0, json!("pop"));
        branch_container.push(json!({"->": format!(".^.^.{rejoin_index}")}));
        named.insert(
            format!("s{index}"),
            branch_container.into_json_array(None, None)?,
        );
    }
    if has_once_fallthrough {
        let mut branch_container = EmittedContainer::default();
        branch_container.push(json!("pop"));
        branch_container.push(json!({"->": format!(".^.^.{rejoin_index}")}));
        named.insert(
            format!("s{authored_branch_count}"),
            branch_container.into_json_array(None, None)?,
        );
    }
    named.insert("#f".to_owned(), json!(5));

    out.push(Value::Object(named));
    Ok(Value::Array(out))
}

fn emit_divert(
    out: &mut EmittedContainer,
    divert: &Divert,
    scope: &EmitScope,
    context: &EmitContext,
) {
    let resolved_target = scope.resolve_divert_target(&divert.target, context);

    if resolved_target == "END" {
        out.push(json!("end"));
        return;
    }

    if resolved_target == "DONE" {
        out.push(json!("done"));
        return;
    }

    if !divert.arguments.is_empty() {
        out.push(json!("ev"));
        for argument in &divert.arguments {
            emit_expression(argument, &mut out.content);
        }
        out.push(json!("/ev"));
    }

    if scope.is_variable_divert(&resolved_target, context) {
        out.push(json!({"->": resolved_target, "var": true}));
    } else {
        out.push(json!({"->": resolved_target}));
    }
}

fn emit_assignment(
    variable_name: &str,
    expression: &Expression,
    mode: &AssignMode,
    out: &mut EmittedContainer,
) {
    match mode {
        AssignMode::Set => {
            out.push(json!("ev"));
            emit_expression(expression, &mut out.content);
            out.push(json!("/ev"));
            out.push(json!({"VAR=": variable_name, "re": true}));
        }
        AssignMode::AddAssign => {
            out.push(json!("ev"));
            emit_expression(
                &Expression::Variable(variable_name.to_owned()),
                &mut out.content,
            );
            emit_expression(expression, &mut out.content);
            out.push(json!("+"));
            out.push(json!({"VAR=": variable_name, "re": true}));
            out.push(json!("/ev"));
        }
    }
}

fn emit_choice(
    out: &mut EmittedContainer,
    choice: &Choice,
    scope: &EmitScope,
    choice_index: usize,
    continuation_path: Option<&str>,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    out.push(json!("ev"));
    emit_choice_text_segment(
        &choice.start_text,
        &choice.start_tags,
        &mut out.content,
        scope,
        context,
    )?;
    emit_choice_text_segment(
        &choice.choice_only_text,
        &choice.choice_only_tags,
        &mut out.content,
        scope,
        context,
    )?;
    for (index, condition) in choice.conditions.iter().enumerate() {
        emit_condition(condition, &mut out.content, scope, context)?;
        if index > 0 {
            out.push(json!("&&"));
        }
    }
    out.push(json!("/ev"));

    let branch_name = format!("c-{choice_index}");
    let branch_scope = scope.choice_branch(&branch_name);
    out.push(json!({"*": format!(".^.{}", branch_name), "flg": choice_flags(choice)}));

    let mut branch_nodes = Vec::new();
    let mut body_already_emitted = false;
    if let Some(selected_text) = &choice.selected_text {
        let recovered_inline_divert = if choice.body.is_empty() {
            recover_selected_text_inline_divert(selected_text)
        } else {
            None
        };

        if choice.has_choice_only_content
            && !choice.has_start_content
            && matches!(choice.body.as_slice(), [Node::Divert(_)])
        {
            branch_nodes.push(Node::Text(format!(" {selected_text}")));
            branch_nodes.extend(choice.body.clone());
            branch_nodes.push(Node::Newline);
            body_already_emitted = true;
        } else if let Some((text, target)) = recovered_inline_divert {
            if !text.is_empty() {
                branch_nodes.push(Node::Text(text));
            }
            branch_nodes.push(Node::Divert(Divert {
                target,
                arguments: Vec::new(),
            }));
            body_already_emitted = true;
        } else {
            branch_nodes.push(Node::Text(selected_text.clone()));
        }
        branch_nodes.extend(choice.selected_tags.iter().cloned().map(Node::Tag));
        if !matches!(choice.body.as_slice(), [Node::Divert(_)] | []) {
            branch_nodes.push(Node::Newline);
        }
    } else if choice.has_choice_only_content
        && !choice.has_start_content
        && matches!(choice.body.as_slice(), [Node::Divert(_)])
    {
        branch_nodes.push(Node::Text(" ".to_owned()));
        branch_nodes.extend(choice.body.clone());
        branch_nodes.push(Node::Newline);
    } else if choice.has_choice_only_content
        && !choice.has_start_content
        && branch_nodes.is_empty()
        && !choice.body.is_empty()
    {
        branch_nodes.push(Node::Text(" ".to_owned()));
        branch_nodes.extend(choice.body.clone());
        body_already_emitted = true;
    }
    if !body_already_emitted {
        branch_nodes.extend(choice.body.clone());
    }

    let mut branch_container = emit_nodes(&branch_nodes, &branch_scope, context)?;
    if let Some(path) = continuation_path.filter(|_| !branch_has_terminal_content(&branch_nodes)) {
        branch_container.push(json!({"->": path}));
    }
    out.insert_named(
        branch_name,
        branch_container.into_json_array(None, Some(5))?,
    );

    Ok(())
}

fn emit_expression(expression: &Expression, out: &mut Vec<Value>) {
    match expression {
        Expression::Bool(value) => out.push(json!(value)),
        Expression::Int(value) => out.push(json!(value)),
        Expression::Float(value) => out.push(json!(value)),
        Expression::Str(value) => {
            out.push(json!("str"));
            out.push(json!(format!("^{value}")));
            out.push(json!("/str"));
        }
        Expression::Variable(name) => out.push(json!({"VAR?": name})),
        Expression::DivertTarget(target) => out.push(json!({"^->": target})),
        Expression::Binary {
            left,
            operator,
            right,
        } => {
            emit_expression(left, out);
            emit_expression(right, out);
            out.push(json!(match operator {
                BinaryOperator::Add => "+",
                BinaryOperator::Subtract => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::Equal => "==",
                BinaryOperator::And => "&&",
                BinaryOperator::Greater => ">",
            }));
        }
    }
}

fn emit_dynamic_string(
    dynamic: &DynamicString,
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    emit_dynamic_string_parts(&dynamic.parts, out, scope, context)
}

fn emit_dynamic_string_parts(
    parts: &[DynamicStringPart],
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    if parts.is_empty() {
        return Ok(());
    }

    match &parts[0] {
        DynamicStringPart::Text(text) => {
            if !text.is_empty() {
                out.push(json!(format!("^{text}")));
            }
            emit_dynamic_string_parts(&parts[1..], out, scope, context)
        }
        DynamicStringPart::Expression(expression) => {
            out.push(json!("ev"));
            emit_expression(expression, out);
            out.push(json!("out"));
            out.push(json!("/ev"));
            emit_dynamic_string_parts(&parts[1..], out, scope, context)
        }
        DynamicStringPart::Sequence(sequence) => {
            out.push(emit_sequence(sequence, scope, context)?);
            emit_dynamic_string_parts(&parts[1..], out, scope, context)
        }
    }
}

fn emit_tag(
    tag: &DynamicString,
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    out.push(json!("#"));
    emit_dynamic_string(tag, out, scope, context)?;
    out.push(json!("/#"));
    Ok(())
}

fn emit_choice_text_segment(
    text: &str,
    tags: &[DynamicString],
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    if text.is_empty() && tags.is_empty() {
        return Ok(());
    }

    out.push(json!("str"));
    if !text.is_empty() {
        out.push(json!(format!("^{text}")));
    }
    for tag in tags {
        emit_tag(tag, out, scope, context)?;
    }
    out.push(json!("/str"));
    Ok(())
}

fn emit_condition(
    condition: &Condition,
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    match condition {
        Condition::Bool(value) => out.push(json!(value)),
        Condition::FunctionCall(name) => {
            let mut call = BTreeMap::new();
            call.insert(format!("{name}()"), Value::String(name.clone()));
            out.push(serde_json::to_value(call).map_err(|error| {
                CompilerError::InvalidSource(format!("failed to serialize function call: {error}"))
            })?);
        }
        Condition::Expression(Expression::Variable(name))
            if scope.resolve_choice_label(name).is_some() =>
        {
            out.push(json!({"CNT?": format!(".^.{}", scope.resolve_choice_label(name).unwrap())}));
        }
        Condition::Expression(Expression::Variable(name))
            if context.top_flow_names.contains(name) || scope.child_flow_names.contains(name) =>
        {
            out.push(json!({"CNT?": scope.resolve_divert_target(name, context)}));
        }
        Condition::Expression(expression) => emit_expression(expression, out),
    }

    Ok(())
}

fn branch_has_terminal_content(nodes: &[Node]) -> bool {
    nodes
        .iter()
        .rev()
        .find(|node| !matches!(node, Node::Newline))
        .is_some_and(node_is_terminal)
}

fn node_is_terminal(node: &Node) -> bool {
    match node {
        Node::Divert(_) | Node::TunnelReturn | Node::ReturnBool(_) => true,
        Node::Choice(choice) => branch_has_terminal_content(&choice.body),
        _ => false,
    }
}

fn recover_selected_text_inline_divert(selected_text: &str) -> Option<(String, String)> {
    let (text, target) = selected_text.rsplit_once("->")?;
    let target = target.trim();
    if target.is_empty() || target.contains(' ') {
        return None;
    }

    Some((text.trim_end().to_owned(), target.to_owned()))
}

fn choice_flags(choice: &Choice) -> i32 {
    let mut flags = 0;
    if !choice.conditions.is_empty() {
        flags |= 1;
    }
    if choice.has_start_content {
        flags |= 2;
    }
    if choice.has_choice_only_content {
        flags |= 4;
    }
    if choice.is_invisible_default {
        flags |= 8;
    }
    if choice.once_only {
        flags |= 16;
    }
    flags
}

fn emit_conditional(
    condition: &Condition,
    when_true: &[Node],
    when_false: Option<&[Node]>,
    scope: &EmitScope,
    conditional_index: usize,
    context: &EmitContext,
) -> Result<Vec<Value>, CompilerError> {
    let mut out = Vec::new();
    let branches = flatten_conditional_branches(condition, when_true, when_false);
    let rejoin_target = format!("{}.{}", scope.path, conditional_index + branches.len());

    for (branch_index, (branch_condition, branch_nodes)) in branches.iter().enumerate() {
        out.push(emit_conditional_branch(
            *branch_condition,
            branch_nodes,
            scope,
            branch_index,
            &rejoin_target,
            context,
        )?);
    }

    out.push(json!("nop"));

    Ok(out)
}

fn flatten_conditional_branches<'a>(
    condition: &'a Condition,
    when_true: &'a [Node],
    when_false: Option<&'a [Node]>,
) -> Vec<(Option<&'a Condition>, &'a [Node])> {
    let mut branches = vec![(Some(condition), when_true)];
    let mut current_false = when_false;

    while let Some(nodes) = current_false {
        if let [Node::Conditional {
            condition,
            when_true,
            when_false,
        }] = nodes
        {
            branches.push((Some(condition), when_true));
            current_false = when_false.as_deref();
        } else {
            branches.push((None, nodes));
            break;
        }
    }

    branches
}

fn emit_conditional_branch(
    condition: Option<&Condition>,
    branch: &[Node],
    scope: &EmitScope,
    branch_index: usize,
    rejoin_target: &str,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let mut out = Vec::new();

    if let Some(condition) = condition {
        out.push(json!("ev"));
        emit_condition(condition, &mut out, scope, context)?;
        out.push(json!("/ev"));
    }

    let branch_scope = scope.choice_branch(&format!("cond-{branch_index}"));
    let mut branch_content = emit_nodes(branch, &branch_scope, context)?;
    branch_content.push(json!({"->": rejoin_target}));

    let mut named = Map::new();
    named.insert("b".to_owned(), branch_content.into_json_array(None, None)?);

    if condition.is_some() {
        out.push(json!({"->": ".^.b", "c": true}));
        out.push(Value::Object(named));
    } else {
        out.push(json!({"->": ".^.b"}));
        out.push(Value::Object(named));
    }

    Ok(Value::Array(out))
}
