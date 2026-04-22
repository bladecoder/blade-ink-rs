use std::{cell::{Cell, RefCell}, collections::HashMap, rc::Rc};

use bladeink::{
    ChoicePoint, CommandType, Container, ControlCommand, Divert, Glue, InkList, InkListItem,
    NativeFunctionCall, NativeOp, Path, PushPopType, RTObject, Value, path_of,
    VariableAssignment, VariableReference,
};

use crate::{
    error::CompilerError,
    parsed_hierarchy::{
        ChoiceNode, ConditionalNode, GatherNode, ParsedExpression, ParsedFlow, ParsedNode,
        ParsedNodeKind, ParsedRuntimeCache, Story, StructuredWeave, StructuredWeaveEntry,
        StructuredWeaveEntryKind,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PendingContainerId(usize);

#[derive(Clone)]
pub(crate) enum PathFixupSource {
    Divert(Rc<Divert>),
    ChoicePoint(Rc<ChoicePoint>),
}

#[derive(Clone)]
enum PathFixupTarget {
    PendingContainer(PendingContainerId),
    RuntimeObject(*const dyn RTObject),
    ParsedRuntimeCache(Rc<ParsedRuntimeCache>),
}

#[derive(Clone)]
struct PathFixup {
    source: PathFixupSource,
    target: PathFixupTarget,
}

#[derive(Clone, Copy)]
pub(crate) struct ParsedRuntimeFixupFlags {
    pub runtime_object: bool,
    pub runtime_path_target: bool,
    pub container_for_counting: bool,
}

#[derive(Clone)]
struct ParsedRuntimeFixup {
    cache: Rc<ParsedRuntimeCache>,
    target: PendingContainerId,
    flags: ParsedRuntimeFixupFlags,
}

pub(crate) struct ExportState {
    next_pending_container_id: Cell<usize>,
    pending_containers: RefCell<HashMap<PendingContainerId, Rc<Container>>>,
    runtime_objects: RefCell<HashMap<*const dyn RTObject, Rc<dyn RTObject>>>,
    path_fixups: RefCell<Vec<PathFixup>>,
    parsed_runtime_fixups: RefCell<Vec<ParsedRuntimeFixup>>,
    parsed_runtime_targets_by_path: RefCell<HashMap<String, Rc<ParsedRuntimeCache>>>,
}

impl ExportState {
    pub(crate) fn new() -> Self {
        Self {
            next_pending_container_id: Cell::new(0),
            pending_containers: RefCell::new(HashMap::new()),
            runtime_objects: RefCell::new(HashMap::new()),
            path_fixups: RefCell::new(Vec::new()),
            parsed_runtime_fixups: RefCell::new(Vec::new()),
            parsed_runtime_targets_by_path: RefCell::new(HashMap::new()),
        }
    }

    fn next_pending_container_id(&self) -> PendingContainerId {
        let id = PendingContainerId(self.next_pending_container_id.get());
        self.next_pending_container_id.set(id.0 + 1);
        id
    }

    fn register_pending_container(&self, id: PendingContainerId, container: Rc<Container>) {
        self.pending_containers.borrow_mut().insert(id, container);
    }

    fn register_runtime_object(&self, object: Rc<dyn RTObject>) -> *const dyn RTObject {
        let key = Rc::as_ptr(&object);
        self.runtime_objects.borrow_mut().insert(key, object);
        key
    }

    pub(crate) fn add_pending_target_fixup(&self, source: PathFixupSource, target: PendingContainerId) {
        self.path_fixups.borrow_mut().push(PathFixup {
            source,
            target: PathFixupTarget::PendingContainer(target),
        });
    }

    pub(crate) fn add_runtime_target_fixup(&self, source: PathFixupSource, target: Rc<dyn RTObject>) {
        let key = self.register_runtime_object(target);
        self.path_fixups.borrow_mut().push(PathFixup {
            source,
            target: PathFixupTarget::RuntimeObject(key),
        });
    }

    pub(crate) fn add_parsed_runtime_target_fixup(
        &self,
        source: PathFixupSource,
        target: Rc<ParsedRuntimeCache>,
    ) {
        self.path_fixups.borrow_mut().push(PathFixup {
            source,
            target: PathFixupTarget::ParsedRuntimeCache(target),
        });
    }

    pub(crate) fn add_parsed_runtime_fixup(
        &self,
        cache: Rc<ParsedRuntimeCache>,
        target: PendingContainerId,
        flags: ParsedRuntimeFixupFlags,
    ) {
        self.parsed_runtime_fixups.borrow_mut().push(ParsedRuntimeFixup {
            cache,
            target,
            flags,
        });
    }

    pub(crate) fn register_parsed_runtime_target_path(
        &self,
        path: impl Into<String>,
        cache: Rc<ParsedRuntimeCache>,
    ) {
        self.parsed_runtime_targets_by_path
            .borrow_mut()
            .insert(path.into(), cache);
    }

    pub(crate) fn parsed_runtime_target_cache_for_path(
        &self,
        path: &str,
    ) -> Option<Rc<ParsedRuntimeCache>> {
        self.parsed_runtime_targets_by_path.borrow().get(path).cloned()
    }

    pub(crate) fn apply_path_fixups(&self) {
        for fixup in self.path_fixups.borrow().iter() {
            let target: Rc<dyn RTObject> = match &fixup.target {
                PathFixupTarget::PendingContainer(id) => self
                    .pending_containers
                    .borrow()
                    .get(id)
                    .cloned()
                    .expect("registered pending container") as Rc<dyn RTObject>,
                PathFixupTarget::RuntimeObject(key) => self
                    .runtime_objects
                    .borrow()
                    .get(key)
                    .cloned()
                    .expect("registered runtime object"),
                PathFixupTarget::ParsedRuntimeCache(cache) => cache
                    .runtime_path_target()
                    .or_else(|| cache.runtime_object())
                    .expect("parsed runtime cache target"),
            };
            let path = path_of(target.as_ref());
            match &fixup.source {
                PathFixupSource::Divert(divert) => divert.set_target_path(path),
                PathFixupSource::ChoicePoint(choice_point) => choice_point.set_path_on_choice(path),
            }
        }

        for fixup in self.parsed_runtime_fixups.borrow().iter() {
            let target = self
                .pending_containers
                .borrow()
                .get(&fixup.target)
                .cloned()
                .expect("registered pending container");
            if fixup.flags.runtime_object {
                fixup.cache.set_runtime_object(target.clone());
            }
            if fixup.flags.runtime_path_target {
                fixup.cache.set_runtime_path_target(target.clone());
            }
            if fixup.flags.container_for_counting {
                fixup.cache.set_container_for_counting(target);
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Scope<'a> {
    Root,
    Flow(&'a ParsedFlow),
}

pub(crate) fn export_nodes(
    state: &ExportState,
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
    export_nodes_with_paths(state, nodes, scope, story, None, None, 0)
}

pub(crate) fn export_nodes_with_paths(
    state: &ExportState,
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    container_path: Option<&str>,
    content_index_offset: usize,
) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
    ParsedNode::export_runtime_nodes(
        state,
        nodes,
        scope,
        story,
        named_paths,
        container_path,
        content_index_offset,
    )
}

#[derive(Clone)]
pub(crate) enum PendingItem {
    Object(Rc<dyn RTObject>),
    Container(Rc<PendingContainer>),
}

pub(crate) struct PendingContainer {
    id: PendingContainerId,
    pub(crate) path: String,
    name: Option<String>,
    pub(crate) count_flags: i32,
    pub(crate) content: RefCell<Vec<PendingItem>>,
    pub(crate) named: RefCell<Vec<(String, Rc<PendingContainer>)>>,
}

impl PendingContainer {
    pub(crate) fn new(
        state: &ExportState,
        path: impl Into<String>,
        name: Option<String>,
        count_flags: i32,
    ) -> Rc<Self> {
        Rc::new(Self {
            id: state.next_pending_container_id(),
            path: path.into(),
            name,
            count_flags,
            content: RefCell::new(Vec::new()),
            named: RefCell::new(Vec::new()),
        })
    }

    pub(crate) fn push_object(&self, object: Rc<dyn RTObject>) {
        self.content.borrow_mut().push(PendingItem::Object(object));
    }

    pub(crate) fn push_container(&self, container: Rc<PendingContainer>) {
        self.content.borrow_mut().push(PendingItem::Container(container));
    }

    pub(crate) fn add_named(&self, key: impl Into<String>, container: Rc<PendingContainer>) {
        self.named.borrow_mut().push((key.into(), container));
    }

    pub(crate) fn next_content_index(&self) -> usize {
        self.content.borrow().len()
    }

    pub(crate) fn id(&self) -> PendingContainerId {
        self.id
    }

    pub(crate) fn finalize(&self, state: &ExportState) -> Rc<Container> {
        let mut content = Vec::new();
        for item in self.content.borrow().iter() {
            match item {
                PendingItem::Object(object) => content.push(object.clone()),
                PendingItem::Container(container) => content.push(container.finalize(state)),
            }
        }

        let mut named = HashMap::new();
        for (key, container) in self.named.borrow().iter() {
            named.insert(key.clone(), container.finalize(state));
        }

        let finalized = Container::new(self.name.clone(), self.count_flags, content, named);
        state.register_pending_container(self.id, finalized.clone());
        finalized
    }
}

pub(crate) fn export_weave(
    state: &ExportState,
    path_prefix: &str,
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
    is_root: bool,
    inherited_named_paths: &HashMap<String, String>,
) -> Result<Rc<Container>, CompilerError> {
    let weave = StructuredWeave::from_nodes(nodes).expect("weave structure");
    Ok(weave.build_runtime_pending(
        state,
        path_prefix,
        scope,
        story,
        is_root,
        inherited_named_paths,
    )?
    .finalize(state))
}

pub(crate) fn export_condition_expression(
    expression: &ParsedExpression,
    story: &Story,
    named_paths: &HashMap<String, String>,
    content: &mut Vec<PendingItem>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Variable { name, .. } => {
            if let Some(path) = resolve_count_path(name, named_paths) {
                content.push(PendingItem::Object(Rc::new(VariableReference::from_path_for_count(
                    &path,
                ))));
            } else {
                let mut tmp = Vec::new();
                export_expression(expression, story, &mut tmp)?;
                content.extend(tmp.into_iter().map(PendingItem::Object));
            }
        }
        ParsedExpression::Unary { operator, expression } => {
            export_condition_expression(expression, story, named_paths, content)?;
            let op = match operator.as_str() {
                "!" => NativeOp::Not,
                "-" => NativeOp::Negate,
                other => {
                    return Err(CompilerError::unsupported_feature(format!(
                        "runtime export does not support unary operator '{other}'"
                    )))
                }
            };
            content.push(PendingItem::Object(native(op)));
        }
        ParsedExpression::Binary {
            left,
            operator,
            right,
        } => {
            export_condition_expression(left, story, named_paths, content)?;
            export_condition_expression(right, story, named_paths, content)?;
            content.push(PendingItem::Object(native(operator_token(operator)?)));
        }
        _ => {
            let mut tmp = Vec::new();
            export_expression(expression, story, &mut tmp)?;
            content.extend(tmp.into_iter().map(PendingItem::Object));
        }
    }

    Ok(())
}

pub(crate) fn export_output_expression(
    expression: &ParsedExpression,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Variable { name, .. } => {
            if let Some(path) = resolve_output_count_path(name, scope, named_paths) {
                content.push(Rc::new(VariableReference::from_path_for_count(&path)));
            } else {
                export_expression_scoped(expression, scope, story, named_paths, content)?;
            }
        }
        _ => export_expression_scoped(expression, scope, story, named_paths, content)?,
    }

    Ok(())
}

fn export_expression_scoped(
    expression: &ParsedExpression,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Bool(value) => content.push(rt_value(*value)),
        ParsedExpression::Int(value) => content.push(rt_value(*value)),
        ParsedExpression::Float(value) => content.push(rt_value(*value)),
        ParsedExpression::String(value) => {
            content.push(command(CommandType::BeginString));
            content.push(rt_value(value.as_str()));
            content.push(command(CommandType::EndString));
        }
        ParsedExpression::StringExpression(nodes) => {
            content.push(command(CommandType::BeginString));
            export_string_expression(nodes, story, content)?;
            content.push(command(CommandType::EndString));
        }
        ParsedExpression::Variable { name, .. } => {
            if let Some(constant) = story.const_declaration(name) {
                export_expression_node(constant.expression(), story, content)?;
            } else {
                content.push(Rc::new(VariableReference::new(name)));
            }
        }
        ParsedExpression::DivertTarget {
            target,
            resolved_target,
        } => {
            let resolved = resolved_target
                .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
                .and_then(|cache| cache.runtime_path())
                .map(|path| path.to_string())
                .unwrap_or_else(|| resolve_target(target, scope, story, named_paths));
            content.push(rt_value(Path::new_with_components_string(Some(&resolved))));
        }
        ParsedExpression::ListItems(items) => {
            let mut list = InkList::new();
            for item in items {
                insert_resolved_list_item(story, item, &mut list)?;
            }
            content.push(rt_value(list));
        }
        ParsedExpression::EmptyList => content.push(rt_value(InkList::new())),
        ParsedExpression::Unary { operator, expression } => match operator.as_str() {
            "-" => match expression.as_ref() {
                ParsedExpression::Int(value) => content.push(rt_value(-value)),
                ParsedExpression::Float(value) => content.push(rt_value(-value)),
                other => {
                    export_expression_scoped(other, scope, story, named_paths, content)?;
                    content.push(native(NativeOp::Negate));
                }
            },
            "!" => {
                export_expression_scoped(expression, scope, story, named_paths, content)?;
                content.push(native(NativeOp::Not));
            }
            _ => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support unary operator '{operator}'"
                )));
            }
        },
        ParsedExpression::Binary {
            left,
            operator,
            right,
        } => {
            export_expression_scoped(left, scope, story, named_paths, content)?;
            export_expression_scoped(right, scope, story, named_paths, content)?;
            content.push(native(operator_token(operator)?));
        }
        ParsedExpression::FunctionCall {
            name, arguments, ..
        } => {
            if story
                .list_definitions()
                .iter()
                .any(|list| list.identifier() == Some(name.as_str()))
            {
                match arguments.as_slice() {
                    [] => {
                        let list = InkList::new();
                        list.set_initial_origin_names(vec![name.to_owned()]);
                        content.push(rt_value(list));
                    }
                    [argument] => {
                        content.push(rt_value(name.as_str()));
                        export_expression_scoped(argument, scope, story, named_paths, content)?;
                        content.push(command(CommandType::ListFromInt));
                    }
                    _ => {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export does not support list call '{name}' with {} arguments",
                            arguments.len()
                        )));
                    }
                }
            } else if let Some(command_type) = builtin_command(name) {
                for argument in arguments {
                    if matches!(name.as_str(), "TURNS_SINCE" | "READ_COUNT") {
                        export_count_argument_expression(argument, story, content)?;
                    } else {
                        export_expression_scoped(argument, scope, story, named_paths, content)?;
                    }
                }
                content.push(command(command_type));
            } else if let Some(native_op) = native_function(name) {
                for argument in arguments {
                    export_expression_scoped(argument, scope, story, named_paths, content)?;
                }
                content.push(native(native_op));
            } else {
                export_expression(expression, story, content)?;
            }
        }
    }

    Ok(())
}

pub(crate) fn register_named_path(
    named_paths: &mut HashMap<String, String>,
    scope: Scope<'_>,
    name: &str,
    runtime_path: &str,
) {
    named_paths.insert(name.to_owned(), runtime_path.to_owned());

    if let Some(alias) = weave_label_alias(runtime_path, name) {
        named_paths.insert(alias, runtime_path.to_owned());
    }

    if let Scope::Flow(flow) = scope
        && let Some(flow_name) = flow.flow().identifier()
    {
        named_paths.insert(format!("{flow_name}.{name}"), runtime_path.to_owned());
    }
}

fn weave_label_alias(runtime_path: &str, name: &str) -> Option<String> {
    runtime_path
        .strip_suffix(&format!(".0.{name}"))
        .map(|prefix| format!("{prefix}.{name}"))
}

fn resolve_count_path(name: &str, named_paths: &HashMap<String, String>) -> Option<String> {
    named_paths.get(name).cloned().or_else(|| {
        let mut parts: Vec<&str> = name.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        let last = parts.pop()?;
        Some(format!("{}.0.{last}", parts.join(".")))
    })
}

fn resolve_output_count_path(
    name: &str,
    scope: Scope<'_>,
    named_paths: Option<&HashMap<String, String>>,
) -> Option<String> {
    if let Some(named_paths) = named_paths
        && let Some(path) = resolve_count_path(name, named_paths)
    {
        return Some(path);
    }

    let Scope::Flow(flow) = scope else {
        return None;
    };

    if flow.flow().identifier() == Some(name) {
        return Some(".^".to_owned());
    }

    None
}

pub(crate) fn collect_current_level_named_paths(
    path_prefix: &str,
    entries: &[StructuredWeaveEntry],
) -> HashMap<String, String> {
    let mut current_path = path_prefix.to_owned();
    let mut has_seen_choice_in_section = false;
    let mut choice_count = 0usize;
    let mut gather_count = 0usize;
    let mut result = HashMap::new();

    for entry in entries {
        match entry.kind() {
            StructuredWeaveEntryKind::Choice(choice) => {
                if let Some(name) = choice.identifier() {
                    let choice_path = format!("{}.c-{choice_count}", current_path);
                    result.insert(name.to_owned(), choice_path.clone());
                    if let Some(alias) = weave_label_alias(&choice_path, name) {
                        result.insert(alias, choice_path);
                    }
                }
                has_seen_choice_in_section = true;
                choice_count += 1;
            }
            StructuredWeaveEntryKind::Gather(gather) => {
                let is_named_gather = gather.identifier().is_some();
                let gather_name = gather
                    .identifier()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("g-{gather_count}"));
                let gather_path = if !has_seen_choice_in_section {
                    format!("{}.{}", current_path, gather_name)
                } else {
                    format!("{path_prefix}.{}", gather_name)
                };
                if gather.identifier().is_some() {
                    result.insert(gather_name.clone(), gather_path.clone());
                    if let Some(alias) = weave_label_alias(&gather_path, &gather_name) {
                        result.insert(alias, gather_path.clone());
                    }
                }
                current_path = gather_path;
                has_seen_choice_in_section = false;
                if !is_named_gather {
                    gather_count += 1;
                }
            }
            StructuredWeaveEntryKind::Content(_) => {}
        }
    }

    result
}

pub(crate) fn collect_loose_choice_ends(container: &Rc<PendingContainer>) -> Vec<Rc<PendingContainer>> {
    let mut result = Vec::new();

    if container.count_flags == 5 && !pending_container_has_terminal(container) {
        result.push(container.clone());
    }

    for (_, named) in container.named.borrow().iter() {
        result.extend(collect_loose_choice_ends(named));
    }

    for item in container.content.borrow().iter() {
        if let PendingItem::Container(child) = item {
            result.extend(collect_loose_choice_ends(child));
        }
    }

    result
}

fn pending_container_has_terminal(container: &Rc<PendingContainer>) -> bool {
    let content = container.content.borrow();
    let Some(last) = content.last() else {
        return false;
    };

    match last {
        PendingItem::Object(object) => {
            if object.as_any().is::<Divert>() {
                return true;
            }
            if let Some(command) = object.as_any().downcast_ref::<ControlCommand>() {
                return matches!(command.command_type, CommandType::End | CommandType::Done);
            }
            false
        }
        PendingItem::Container(_) => false,
    }
}

pub(crate) fn variable_divert(name: &str) -> Rc<dyn RTObject> {
    Rc::new(Divert::new(
        false,
        PushPopType::Tunnel,
        false,
        0,
        false,
        Some(name.to_owned()),
        None,
    ))
}

pub(crate) fn has_weave_content(nodes: &[ParsedNode]) -> bool {
    nodes.iter().any(|node| weave_depth(node).is_some())
}

fn weave_depth(node: &ParsedNode) -> Option<usize> {
    if let Some(choice) = ChoiceNode::from_node(node) {
        return Some(choice.indentation_depth());
    }

    GatherNode::from_node(node).map(GatherNode::indentation_depth)
}

#[derive(Clone, Copy)]
pub(crate) enum DivertKind {
    Normal,
    Tunnel,
    Thread,
}

pub(crate) fn export_divert_by_kind(
    state: &ExportState,
    target: &str,
    resolved_target_ref: Option<crate::parsed_hierarchy::ParsedObjectRef>,
    arguments: &[ParsedExpression],
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    kind: DivertKind,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    if matches!(kind, DivertKind::Normal) {
        match target {
            "END" => {
                content.push(command(CommandType::End));
                return Ok(());
            }
            "DONE" => {
                content.push(command(CommandType::Done));
                return Ok(());
            }
            _ => {}
        }
    }

    let variable_target = resolve_variable_divert_name(target, scope, story, named_paths);
    let resolved = variable_target
        .clone()
        .unwrap_or_else(|| resolve_target(target, scope, story, named_paths));
    let resolved_target_cache = resolved_target_ref
        .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
        .or_else(|| resolved_runtime_target_cache(state, &resolved, story));

    if variable_target.is_none()
        && story
            .parsed_flows()
            .iter()
            .any(|flow| flow.flow().identifier() == Some(resolved.as_str()) && flow.flow().is_function())
    {
        return Err(CompilerError::unsupported_feature(format!(
            "cannot divert to function '{resolved}'"
        )));
    }

    if variable_target.is_some() && !arguments.is_empty() {
        return Err(CompilerError::unsupported_feature(format!(
            "can't store arguments in a divert target variable '{resolved}'"
        )));
    }

    if !arguments.is_empty() {
        content.push(command(CommandType::EvalStart));
        export_divert_arguments(arguments, &resolved, scope, story, named_paths, content)?;
        content.push(command(CommandType::EvalEnd));
    }

    if matches!(kind, DivertKind::Thread) {
        content.push(command(CommandType::StartThread));
    }

    let divert = if let Some(ref variable_name) = variable_target {
        Rc::new(Divert::new(
            matches!(kind, DivertKind::Tunnel),
            PushPopType::Tunnel,
            false,
            0,
            false,
                Some(variable_name.clone()),
                None,
            )) as Rc<dyn RTObject>
    } else {
        match kind {
            DivertKind::Normal => Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                false,
                None,
                resolved_target_cache.is_none().then_some(resolved.as_str()),
            )) as Rc<dyn RTObject>,
            DivertKind::Tunnel => Rc::new(Divert::new(
                true,
                PushPopType::Tunnel,
                false,
                0,
                false,
                None,
                resolved_target_cache.is_none().then_some(resolved.as_str()),
            )) as Rc<dyn RTObject>,
            DivertKind::Thread => Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                false,
                None,
                resolved_target_cache.is_none().then_some(resolved.as_str()),
            )) as Rc<dyn RTObject>,
        }
    };

    if variable_target.is_none()
        && let Some(cache) = resolved_target_cache
        && let Ok(divert_runtime) = divert.clone().into_any().downcast::<Divert>()
    {
        state.add_parsed_runtime_target_fixup(PathFixupSource::Divert(divert_runtime), cache);
    }

    content.push(divert);
    Ok(())
}

pub(crate) fn export_divert_conditional(
    state: &ExportState,
    target: &str,
    resolved_target_ref: Option<crate::parsed_hierarchy::ParsedObjectRef>,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Result<Rc<dyn RTObject>, CompilerError> {
    match target {
        "END" => Ok(Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            true,
            None,
            Some("END"),
        ))),
        "DONE" => Ok(Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            true,
            None,
            Some("DONE"),
        ))),
        _ => {
            let resolved = resolve_target(target, scope, story, named_paths);
            let resolved_target_cache = resolved_target_ref
                .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
                .or_else(|| resolved_runtime_target_cache(state, &resolved, story));
            let divert = Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                true,
                None,
                resolved_target_cache.is_none().then_some(resolved.as_str()),
            ));
            if let Some(cache) = resolved_target_cache {
                state.add_parsed_runtime_target_fixup(PathFixupSource::Divert(divert.clone()), cache);
            }
            Ok(divert)
        }
    }
}

pub(crate) fn export_divert_arguments(
    arguments: &[ParsedExpression],
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    let expected_arguments = find_flow_by_path(story.parsed_flows(), target).map(|flow| flow.flow().arguments());

    for (index, argument) in arguments.iter().enumerate() {
        let expected_argument = expected_arguments.and_then(|args| args.get(index));
        if expected_argument.is_some_and(|arg| arg.is_by_reference) {
            let ParsedExpression::Variable { name: var_name, .. } = argument else {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export divert to '{target}' requires variable arguments for by-reference parameters"
                )));
            };
            content.push(Rc::new(Value::new_variable_pointer(var_name, -1)));
        } else {
            export_divert_argument_expression(argument, scope, story, named_paths, content)?;
        }
    }

    Ok(())
}

fn export_divert_argument_expression(
    argument: &ParsedExpression,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match argument {
        ParsedExpression::DivertTarget {
            target,
            resolved_target,
        } => {
            let resolved = resolved_target
                .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
                .and_then(|cache| cache.runtime_path())
                .map(|path| path.to_string())
                .unwrap_or_else(|| resolve_target(target, scope, story, named_paths));
            content.push(rt_value(Path::new_with_components_string(Some(&resolved))));
            Ok(())
        }
        _ => export_expression(argument, story, content),
    }
}

fn find_flow_by_path<'a>(flows: &'a [ParsedFlow], path: &str) -> Option<&'a ParsedFlow> {
    let mut parts = path.split('.');
    let first = parts.next()?;
    let mut current = flows.iter().find(|flow| flow.flow().identifier() == Some(first))?;

    for part in parts {
        current = current
            .children()
            .iter()
            .find(|flow| flow.flow().identifier() == Some(part))?;
    }

    Some(current)
}

fn is_global_variable_divert(target: &str, story: &Story) -> bool {
    story
        .global_initializers()
        .iter()
        .any(|(name, _)| name == target)
        && !story
            .parsed_flows()
            .iter()
            .any(|flow| flow.flow().identifier() == Some(target))
}

pub(crate) fn resolve_variable_divert_name(
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Option<String> {
    if target.contains('.')
        || named_paths.is_some_and(|paths| paths.contains_key(target))
    {
        return None;
    }

    if is_global_variable_divert(target, story) {
        return Some(target.to_owned());
    }

    let Scope::Flow(flow) = scope else {
        return None;
    };

    flow.flow()
        .arguments()
        .iter()
        .any(|arg| arg.identifier == target && arg.is_divert_target)
        .then_some(target.to_owned())
}

pub(crate) fn resolve_target(
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> String {
    if let Some(named_paths) = named_paths
        && let Some(path) = named_paths.get(target)
    {
        return path.clone();
    }

    if target.contains('.') {
        if let Some(path) = resolve_explicit_weave_target(target, story) {
            return path;
        }
        if let Some(expanded) = expand_weave_point_path(target, story, named_paths) {
            return expanded;
        }
        return target.to_owned();
    }

    let Scope::Flow(flow) = scope else {
        if let Some(resolved) = resolve_unique_nested_flow_target(target, story) {
            return resolved;
        }
        return target.to_owned();
    };

    if flow
        .children()
        .iter()
        .any(|child| child.flow().identifier() == Some(target))
    {
        return format!(
            "{}.{target}",
            flow.flow().identifier().unwrap_or_default()
        );
    }

    if let Some(resolved) = resolve_sibling_or_ancestor_flow_target(target, flow, story) {
        return resolved;
    }

    if story
        .parsed_flows()
        .iter()
        .any(|candidate| candidate.flow().identifier() == Some(target))
    {
        return target.to_owned();
    }

    target.to_owned()
}

fn resolved_runtime_target_cache(
    state: &ExportState,
    target: &str,
    story: &Story,
) -> Option<Rc<ParsedRuntimeCache>> {
    state
        .parsed_runtime_target_cache_for_path(target)
        .or_else(|| story.runtime_target_cache_for_path(target))
}

fn resolved_count_target_path(
    story: &Story,
    target: crate::parsed_hierarchy::ParsedObjectRef,
) -> Option<String> {
    story
        .runtime_target_cache_for_ref(target)
        .and_then(|cache| cache.container_for_counting())
        .map(|container| path_of(container.as_ref()).to_string())
}

fn resolve_unique_nested_flow_target(target: &str, story: &Story) -> Option<String> {
    let mut matches = Vec::new();
    collect_nested_flow_target_paths(story.parsed_flows(), target, None, &mut matches);
    (matches.len() == 1).then(|| matches.pop().unwrap())
}

fn collect_nested_flow_target_paths(
    flows: &[ParsedFlow],
    target: &str,
    prefix: Option<&str>,
    matches: &mut Vec<String>,
) {
    for flow in flows {
        let Some(flow_name) = flow.flow().identifier() else {
            continue;
        };
        let path = prefix
            .map(|prefix| format!("{prefix}.{flow_name}"))
            .unwrap_or_else(|| flow_name.to_owned());

        if flow
            .children()
            .iter()
            .any(|child| child.flow().identifier() == Some(target))
        {
            matches.push(format!("{path}.{target}"));
        }

        collect_nested_flow_target_paths(flow.children(), target, Some(&path), matches);
    }
}

fn resolve_sibling_or_ancestor_flow_target(
    target: &str,
    current_flow: &ParsedFlow,
    story: &Story,
) -> Option<String> {
    resolve_sibling_or_ancestor_flow_target_in(
        story.parsed_flows(),
        current_flow.object().id(),
        target,
        None,
    )
}

fn resolve_sibling_or_ancestor_flow_target_in(
    flows: &[ParsedFlow],
    current_flow_id: usize,
    target: &str,
    prefix: Option<&str>,
) -> Option<String> {
    for flow in flows {
        let flow_name = flow.flow().identifier()?;
        let path = prefix
            .map(|prefix| format!("{prefix}.{flow_name}"))
            .unwrap_or_else(|| flow_name.to_owned());

        if flow.children().iter().any(|child| child.object().id() == current_flow_id)
            && flow
                .children()
                .iter()
                .any(|child| child.flow().identifier() == Some(target))
        {
            return Some(format!("{path}.{target}"));
        }

        if let Some(found) = resolve_sibling_or_ancestor_flow_target_in(
            flow.children(),
            current_flow_id,
            target,
            Some(&path),
        ) {
            return Some(found);
        }
    }

    None
}

fn resolve_explicit_weave_target(target: &str, story: &Story) -> Option<String> {
    let mut parts: Vec<&str> = target.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    let weave_name = parts.pop()?;
    let mut flows = story.parsed_flows();
    let mut prefix = String::new();
    let mut current_flow: Option<&ParsedFlow> = None;

    for flow_name in parts {
        let flow = flows.iter().find(|flow| flow.flow().identifier() == Some(flow_name))?;
        if !prefix.is_empty() {
            prefix.push('.');
        }
        prefix.push_str(flow_name);
        current_flow = Some(flow);
        flows = flow.children();
    }

    let flow = current_flow?;
    let path_prefix = format!("{prefix}.0");
    let named_paths = StructuredWeave::from_nodes(flow.content())
        .map(|weave| collect_current_level_named_paths(&path_prefix, weave.entries()))
        .unwrap_or_default();
    named_paths.get(weave_name).cloned()
}

fn expand_weave_point_path(
    target: &str,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Option<String> {
    if let Some(named_paths) = named_paths
        && let Some(path) = named_paths.get(target)
    {
        return Some(path.clone());
    }

    let mut parts: Vec<&str> = target.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let last = parts.pop()?;
    let candidate = format!("{}.0.{last}", parts.join("."));
    if has_named_content_path(story.parsed_flows(), &candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn has_named_content_path(flows: &[ParsedFlow], path: &str) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return false;
    }

    for flow in flows {
        if flow.flow().identifier() == Some(parts[0]) {
            return has_named_content_path_in_flow(flow, &parts[1..]);
        }
    }

    false
}

fn has_named_content_path_in_flow(flow: &ParsedFlow, parts: &[&str]) -> bool {
    if parts.is_empty() {
        return true;
    }

    if parts[0] == "0" {
        return has_named_content_path_in_nodes(flow.content(), &parts[1..]);
    }

    for child in flow.children() {
        if child.flow().identifier() == Some(parts[0]) {
            return has_named_content_path_in_flow(child, &parts[1..]);
        }
    }

    false
}

fn has_named_content_path_in_nodes(nodes: &[ParsedNode], parts: &[&str]) -> bool {
    if parts.is_empty() {
        return true;
    }

    let target = parts[0];
    for node in nodes {
        if ChoiceNode::from_node(node)
            .and_then(|choice| choice.identifier())
            .is_some_and(|name| name == target)
        {
            return true;
        }
        if GatherNode::from_node(node)
            .and_then(|gather| gather.identifier())
            .is_some_and(|name| name == target)
        {
            return true;
        }
        if GatherNode::from_node(node).is_some_and(|gather| !gather.is_label()) {
            continue;
        }
    }

    false
}

pub(crate) fn unwrap_weave_root_container(container: &Rc<Container>) -> Rc<Container> {
    if !container.has_valid_name()
        && container.named_content.is_empty()
        && container.content.len() == 1
        && let Ok(child) = container.content[0].clone().into_any().downcast::<Container>()
        && !child.has_valid_name()
    {
        return child;
    }

    container.clone()
}

pub(crate) fn conditional_is_simple(node: ConditionalNode<'_>) -> bool {
    node.branches().all(|branch| {
        branch.content().iter().all(|child| {
            !matches!(child.kind(), ParsedNodeKind::Conditional | ParsedNodeKind::SwitchConditional)
        })
    })
}

pub(crate) fn export_condition_expression_runtime(
    expression: &ParsedExpression,
    scope: Scope<'_>,
    story: &Story,
    named_paths: &HashMap<String, String>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Variable {
            name,
            resolved_count_target,
        } => {
            if let Some(path) = resolved_count_target
                .and_then(|target_ref| resolved_count_target_path(story, target_ref))
                .or_else(|| resolve_condition_count_path(name, scope, story, named_paths))
            {
                content.push(Rc::new(VariableReference::from_path_for_count(&path)));
            } else {
                export_expression(expression, story, content)?;
            }
        }
        ParsedExpression::Unary { operator, expression } => {
            export_condition_expression_runtime(expression, scope, story, named_paths, content)?;
            let op = match operator.as_str() {
                "!" => NativeOp::Not,
                "-" => NativeOp::Negate,
                other => {
                    return Err(CompilerError::unsupported_feature(format!(
                        "runtime export does not support unary operator '{other}'"
                    )))
                }
            };
            content.push(native(op));
        }
        ParsedExpression::Binary { left, operator, right } => {
            export_condition_expression_runtime(left, scope, story, named_paths, content)?;
            export_condition_expression_runtime(right, scope, story, named_paths, content)?;
            content.push(native(operator_token(operator)?));
        }
        _ => export_expression(expression, story, content)?,
    }

    Ok(())
}

fn resolve_condition_count_path(
    name: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: &HashMap<String, String>,
) -> Option<String> {
    if let Some(path) = resolve_count_path(name, named_paths) {
        return Some(path);
    }

    if story
        .parsed_flows()
        .iter()
        .any(|flow| flow.flow().identifier() == Some(name))
    {
        return Some(name.to_owned());
    }

    let Scope::Flow(flow) = scope else {
        return None;
    };

    if flow
        .children()
        .iter()
        .any(|child| child.flow().identifier() == Some(name))
    {
        return Some(format!("{}.{}", flow.flow().identifier().unwrap_or_default(), name));
    }

    None
}

pub(crate) fn variable_is_temporary_in_scope(name: &str, scope: Scope<'_>) -> bool {
    let Scope::Flow(flow) = scope else {
        return false;
    };

    flow.flow().arguments().iter().any(|arg| arg.identifier == name)
}

pub(crate) fn expression_contains_function_call(expression: &ParsedExpression) -> bool {
    match expression {
        ParsedExpression::FunctionCall { .. } => true,
        ParsedExpression::Unary { expression, .. } => expression_contains_function_call(expression),
        ParsedExpression::Binary { left, right, .. } => {
            expression_contains_function_call(left) || expression_contains_function_call(right)
        }
        _ => false,
    }
}

fn export_expression_node(
    expression: &crate::parsed_hierarchy::ExpressionNode,
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    use crate::parsed_hierarchy::ExpressionNode;

    match expression {
        ExpressionNode::Number(number) => content.push(number.runtime_object()),
        ExpressionNode::StringExpression(string) => {
            content.push(command(CommandType::BeginString));
            for item in string.content().content() {
                let crate::parsed_hierarchy::Content::Text(text) = item;
                content.push(text.runtime_object());
            }
            content.push(command(CommandType::EndString));
        }
        ExpressionNode::VariableReference(reference) => {
            let name = reference.name();
            if let Some(constant) = story.const_declaration(&name) {
                export_expression_node(constant.expression(), story, content)?;
            } else {
                content.push(reference.runtime_object());
            }
        }
        ExpressionNode::List(list) => {
            let mut runtime_list = InkList::new();
            if let Some(list_items) = list.item_identifier_list() {
                for item in list_items {
                    insert_resolved_list_item(story, item, &mut runtime_list)?;
                }
            }
            content.push(rt_value(runtime_list));
        }
    }
    Ok(())
}

pub(crate) fn export_expression(
    expression: &ParsedExpression,
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Bool(value) => content.push(rt_value(*value)),
        ParsedExpression::Int(value) => content.push(rt_value(*value)),
        ParsedExpression::Float(value) => content.push(rt_value(*value)),
        ParsedExpression::String(value) => {
            content.push(command(CommandType::BeginString));
            content.push(rt_value(value.as_str()));
            content.push(command(CommandType::EndString));
        }
        ParsedExpression::StringExpression(nodes) => {
            content.push(command(CommandType::BeginString));
            export_string_expression(nodes, story, content)?;
            content.push(command(CommandType::EndString));
        }
        ParsedExpression::Variable { name, .. } => {
            if let Some(constant) = story.const_declaration(name) {
                export_expression_node(constant.expression(), story, content)?;
            } else {
                content.push(Rc::new(VariableReference::new(name)));
            }
        }
        ParsedExpression::DivertTarget {
            target,
            resolved_target,
        } => {
            let resolved_path = resolved_target
                .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
                .or_else(|| story.runtime_target_cache_for_path(target))
                .and_then(|cache| cache.runtime_path())
                .map(|path| path.to_string())
                .unwrap_or_else(|| target.to_owned());
            content.push(rt_value(Path::new_with_components_string(Some(&resolved_path))));
        }
        ParsedExpression::ListItems(items) => {
            let mut list = InkList::new();
            for item in items {
                insert_resolved_list_item(story, item, &mut list)?;
            }
            content.push(rt_value(list));
        }
        ParsedExpression::EmptyList => content.push(rt_value(InkList::new())),
        ParsedExpression::Unary { operator, expression } => match operator.as_str() {
            "-" => match expression.as_ref() {
                ParsedExpression::Int(value) => content.push(rt_value(-value)),
                ParsedExpression::Float(value) => content.push(rt_value(-value)),
                other => {
                    export_expression(other, story, content)?;
                    content.push(native(NativeOp::Negate));
                }
            },
            "!" => {
                export_expression(expression, story, content)?;
                content.push(native(NativeOp::Not));
            }
            _ => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support unary operator '{operator}'"
                )));
            }
        },
        ParsedExpression::Binary {
            left,
            operator,
            right,
        } => {
            export_expression(left, story, content)?;
            export_expression(right, story, content)?;
            content.push(native(operator_token(operator)?));
        }
        ParsedExpression::FunctionCall {
            name,
            arguments,
            resolved_target,
        } => {
            if story
                .list_definitions()
                .iter()
                .any(|list| list.identifier() == Some(name.as_str()))
            {
                match arguments.as_slice() {
                    [] => {
                        let list = InkList::new();
                        list.set_initial_origin_names(vec![name.to_owned()]);
                        content.push(rt_value(list));
                    }
                    [argument] => {
                        content.push(rt_value(name.as_str()));
                        export_expression(argument, story, content)?;
                        content.push(command(CommandType::ListFromInt));
                    }
                    _ => {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export does not support list call '{name}' with {} arguments",
                            arguments.len()
                        )));
                    }
                }
            } else if let Some(command_type) = builtin_command(name) {
                for argument in arguments {
                    if matches!(name.as_str(), "TURNS_SINCE" | "READ_COUNT") {
                        export_count_argument_expression(argument, story, content)?;
                    } else {
                        export_expression(argument, story, content)?;
                    }
                }
                content.push(command(command_type));
            } else if let Some(native_op) = native_function(name) {
                for argument in arguments {
                    export_expression(argument, story, content)?;
                }
                content.push(native(native_op));
            } else if let Some(function_flow) = story.parsed_flows().iter().find(|flow| {
                flow.flow().identifier() == Some(name.as_str()) && flow.flow().is_function()
            }) {
                if arguments.is_empty()
                    && let Some(text) = simple_function_text(function_flow)
                {
                    content.push(rt_value(text.as_str()));
                } else {
                    let params = function_flow.flow().arguments();
                    if params.len() != arguments.len() {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export function call '{name}' has {} arguments but expected {}",
                            arguments.len(),
                            params.len()
                        )));
                    }

                    for (argument, parameter) in arguments.iter().zip(params.iter()) {
                        if parameter.is_by_reference {
                            let ParsedExpression::Variable { name: var_name, .. } = argument else {
                                return Err(CompilerError::unsupported_feature(format!(
                                    "runtime export by-reference function call '{name}' requires variable arguments"
                                )));
                            };
                            content.push(Rc::new(Value::new_variable_pointer(var_name, -1)));
                        } else {
                            export_expression(argument, story, content)?;
                        }
                    }

                    let target_path = resolved_target
                        .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
                        .and_then(|cache| cache.runtime_path())
                        .or_else(|| function_flow.runtime_path())
                        .map(|path| path.to_string())
                        .unwrap_or_else(|| name.to_owned());

                    content.push(Rc::new(Divert::new(
                        true,
                        PushPopType::Function,
                        false,
                        0,
                        false,
                        None,
                        Some(&target_path),
                    )));
                }
            } else {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support function call '{name}'"
                )));
            }
        }
    }

    Ok(())
}

fn export_string_expression(
    nodes: &[ParsedNode],
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    for node in nodes {
        match node.kind() {
            ParsedNodeKind::Text => {
                if let Some(text) = node.text() {
                    content.push(rt_value(text));
                }
            }
            ParsedNodeKind::Newline => content.push(rt_value("\n")),
            ParsedNodeKind::Glue => content.push(Rc::new(Glue::new())),
            ParsedNodeKind::OutputExpression => {
                let expression = node.expression().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export string expression missing expression",
                    )
                })?;
                content.push(command(CommandType::EvalStart));
                export_expression(expression, story, content)?;
                content.push(command(CommandType::EvalOutput));
                content.push(command(CommandType::EvalEnd));
            }
            other => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support {:?} inside string expressions yet",
                    other
                )))
            }
        }
    }

    Ok(())
}

fn insert_resolved_list_item(
    story: &Story,
    item: &str,
    list: &mut InkList,
) -> Result<(), CompilerError> {
    if let Some((qualified, value)) = story.resolve_list_item(item) {
        list.items.insert(InkListItem::from_full_name(&qualified), value);
        Ok(())
    } else {
        Err(CompilerError::unsupported_feature(format!(
            "runtime export cannot resolve list item '{item}'"
        )))
    }
}

fn export_count_argument_expression(
    argument: &ParsedExpression,
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match argument {
        ParsedExpression::Variable {
            name: _,
            resolved_count_target,
        } => {
            if let Some(path) = resolved_count_target
                .and_then(|target_ref| resolved_count_target_path(story, target_ref))
            {
                content.push(Rc::new(VariableReference::from_path_for_count(&path)));
                Ok(())
            } else {
                export_expression(argument, story, content)
            }
        }
        ParsedExpression::DivertTarget {
            target,
            resolved_target,
        } => {
            let resolved_path = resolved_target
                .and_then(|target_ref| resolved_count_target_path(story, target_ref))
                .unwrap_or_else(|| target.to_owned());
            content.push(Rc::new(VariableReference::from_path_for_count(&resolved_path)));
            Ok(())
        }
        _ => export_expression(argument, story, content),
    }
}

fn simple_function_text(flow: &ParsedFlow) -> Option<String> {
    let mut text = String::new();

    for node in flow.content() {
        match node.kind() {
            ParsedNodeKind::Text => text.push_str(node.text()?),
            ParsedNodeKind::Newline => {}
            _ => return None,
        }
    }

    Some(text)
}

fn operator_token(operator: &str) -> Result<NativeOp, CompilerError> {
    match operator {
        "Add" => Ok(NativeOp::Add),
        "Subtract" => Ok(NativeOp::Subtract),
        "Multiply" => Ok(NativeOp::Multiply),
        "Divide" => Ok(NativeOp::Divide),
        "Modulo" => Ok(NativeOp::Mod),
        "Equal" => Ok(NativeOp::Equal),
        "NotEqual" => Ok(NativeOp::NotEquals),
        "Less" => Ok(NativeOp::Less),
        "LessEqual" => Ok(NativeOp::LessThanOrEquals),
        "Greater" => Ok(NativeOp::Greater),
        "GreaterEqual" => Ok(NativeOp::GreaterThanOrEquals),
        "And" => Ok(NativeOp::And),
        "Or" => Ok(NativeOp::Or),
        "Has" => Ok(NativeOp::Has),
        "Hasnt" => Ok(NativeOp::Hasnt),
        "Intersect" => Ok(NativeOp::Intersect),
        other => Err(CompilerError::unsupported_feature(format!(
            "runtime export does not support binary operator '{other}'"
        ))),
    }
}

fn native_function(name: &str) -> Option<NativeOp> {
    match name {
        "MIN" => Some(NativeOp::Min),
        "MAX" => Some(NativeOp::Max),
        "POW" => Some(NativeOp::Pow),
        "FLOOR" => Some(NativeOp::Floor),
        "CEILING" => Some(NativeOp::Ceiling),
        "INT" => Some(NativeOp::Int),
        "FLOAT" => Some(NativeOp::Float),
        "LIST_VALUE" => Some(NativeOp::ValueOfList),
        "LIST_COUNT" => Some(NativeOp::Count),
        "LIST_MIN" => Some(NativeOp::ListMin),
        "LIST_MAX" => Some(NativeOp::ListMax),
        "LIST_ALL" => Some(NativeOp::All),
        "LIST_INVERT" => Some(NativeOp::Invert),
        _ => None,
    }
}

fn builtin_command(name: &str) -> Option<CommandType> {
    match name {
        "CHOICE_COUNT" => Some(CommandType::ChoiceCount),
        "TURNS" => Some(CommandType::Turns),
        "TURNS_SINCE" => Some(CommandType::TurnsSince),
        "READ_COUNT" => Some(CommandType::ReadCount),
        "RANDOM" => Some(CommandType::Random),
        "SEED_RANDOM" => Some(CommandType::SeedRandom),
        "LIST_RANGE" => Some(CommandType::ListRange),
        "LIST_RANDOM" => Some(CommandType::ListRandom),
        _ => None,
    }
}

pub(crate) fn has_terminal(nodes: &[ParsedNode]) -> bool {
    let last = nodes
        .iter()
        .rev()
        .find(|node| node.kind() != ParsedNodeKind::Newline);

    matches!(
        last.map(|node| node.kind()),
        Some(ParsedNodeKind::Divert | ParsedNodeKind::TunnelReturn)
    )
}

pub(crate) fn command(command_type: CommandType) -> Rc<dyn RTObject> {
    Rc::new(ControlCommand::new(command_type))
}

pub(crate) fn rt_value<T: Into<Value>>(value: T) -> Rc<dyn RTObject> {
    Rc::new(Value::new(value))
}

pub(crate) fn rt_int(value: i32) -> Rc<dyn RTObject> {
    Rc::new(Value::new::<i32>(value))
}

pub(crate) fn native(op: NativeOp) -> Rc<dyn RTObject> {
    Rc::new(NativeFunctionCall::new(op))
}

pub(crate) fn variable_assignment(name: &str, is_global: bool, is_new_declaration: bool) -> Rc<dyn RTObject> {
    Rc::new(VariableAssignment::new(
        name,
        is_new_declaration,
        is_global,
    ))
}

pub(crate) fn divert_object(target: &str) -> Rc<dyn RTObject> {
    Rc::new(Divert::new(
        false,
        PushPopType::Tunnel,
        false,
        0,
        false,
        None,
        Some(target),
    ))
}
