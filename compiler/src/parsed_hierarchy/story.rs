use super::{
    Content, ContentList, ExternalDeclaration, FlowArgument, FlowBase, FlowLevel, ListDefinition,
    ParsedExpression, ParsedFlow, ParsedNode, ParsedObject, ParsedObjectIndex,
    ValidationScope, VariableAssignment, ConstDeclaration,
};
use crate::error::CompilerError;
use bladeink::{
    story::Story as RuntimeStory, CommandType, InkList, InkListItem, ListDefinition as RuntimeListDefinition,
    RTObject, Container, Path,
};
use std::{collections::HashMap, collections::HashSet, rc::Rc};

#[derive(Debug, Clone)]
pub struct Story {
    flow: FlowBase,
    source: String,
    source_filename: Option<String>,
    pub count_all_visits: bool,
    root_content: ContentList,
    pub(crate) global_declarations: Vec<VariableAssignment>,
    pub(crate) global_initializers: Vec<(String, ParsedExpression)>,
    pub(crate) list_definitions: Vec<ListDefinition>,
    pub(crate) external_declarations: Vec<ExternalDeclaration>,
    pub(crate) const_declarations: Vec<ConstDeclaration>,
    pub(crate) root_nodes: Vec<ParsedNode>,
    pub(crate) flows: Vec<ParsedFlow>,
}

impl Story {
    pub fn new(source: &str, source_filename: Option<String>, count_all_visits: bool) -> Self {
        let mut flow = FlowBase::new(FlowLevel::Story, None, Vec::<FlowArgument>::new(), false);
        let mut root_content = ContentList::new();
        root_content.object_mut().set_parent(flow.object());
        flow.object_mut()
            .add_content_ref(root_content.object().reference());
        Self {
            flow,
            source: source.to_owned(),
            source_filename,
            count_all_visits,
            root_content,
            global_declarations: Vec::new(),
            global_initializers: Vec::new(),
            list_definitions: Vec::new(),
            external_declarations: Vec::new(),
            const_declarations: Vec::new(),
            root_nodes: Vec::new(),
            flows: Vec::new(),
        }
    }

    pub fn object(&self) -> &ParsedObject {
        self.flow.object()
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        self.flow.object_mut()
    }

    pub fn flow(&self) -> &FlowBase {
        &self.flow
    }

    pub fn runtime_object(&self) -> Option<Rc<dyn RTObject>> {
        self.object().runtime_object()
    }

    pub fn runtime_path(&self) -> Option<Path> {
        self.object().runtime_path()
    }

    pub fn container_for_counting(&self) -> Option<Rc<Container>> {
        self.object().container_for_counting()
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn source_filename(&self) -> Option<&str> {
        self.source_filename.as_deref()
    }

    pub fn root_content(&self) -> &ContentList {
        &self.root_content
    }

    pub fn root_content_mut(&mut self) -> &mut ContentList {
        &mut self.root_content
    }

    pub fn global_declarations(&self) -> &[VariableAssignment] {
        &self.global_declarations
    }

    pub fn global_initializers(&self) -> &[(String, ParsedExpression)] {
        &self.global_initializers
    }

    pub fn list_definitions(&self) -> &[ListDefinition] {
        &self.list_definitions
    }

    pub fn external_declarations(&self) -> &[ExternalDeclaration] {
        &self.external_declarations
    }

    pub fn const_declarations(&self) -> &[ConstDeclaration] {
        &self.const_declarations
    }

    pub fn const_declaration(&self, name: &str) -> Option<&ConstDeclaration> {
        self.const_declarations
            .iter()
            .find(|declaration| declaration.name() == name)
    }

    pub fn resolve_list_item(&self, item_name: &str) -> Option<(String, i32)> {
        if let Some((list_name, item_name)) = item_name.split_once('.') {
            let definition = self
                .list_definitions
                .iter()
                .find(|definition| definition.identifier() == Some(list_name))?;
            let item = definition.item_named(item_name)?;
            return Some((item.full_name(list_name), item.series_value()));
        }

        for definition in &self.list_definitions {
            let Some(list_name) = definition.identifier() else {
                continue;
            };
            if let Some(item) = definition.item_named(item_name) {
                return Some((item.full_name(list_name), item.series_value()));
            }
        }

        None
    }

    pub fn root_nodes(&self) -> &[ParsedNode] {
        &self.root_nodes
    }

    pub fn parsed_flows(&self) -> &[ParsedFlow] {
        &self.flows
    }

    pub fn resolve_references(&mut self) -> Result<(), CompilerError> {
        self.rebuild_parse_tree_refs();
        for node in &mut self.root_nodes {
            node.resolve_references();
        }
        for flow in &mut self.flows {
            flow.resolve_references();
        }

        self.validate_empty_diverts()?;

        let global_vars = self.collect_declared_variable_names();
        let const_names = self.collect_const_names();
        let flow_names = self.collect_all_flow_names();
        let top_level_flow_names = self.collect_top_level_flow_names();

        for flow in self.parsed_flows() {
            flow.validate(
                &top_level_flow_names,
                &top_level_flow_names,
                &flow_names,
                &global_vars,
                &const_names,
                self,
            )?;
        }

        let root_scope = self.root_validation_scope(
            &global_vars,
            &const_names,
            &flow_names,
            &top_level_flow_names,
        )?;
        self.validate_root_scope(&flow_names, &root_scope)?;
        self.resolve_parsed_targets();
        let resolved_story = self.clone();
        self.apply_counting_marks(&resolved_story);

        Ok(())
    }

    pub fn export_runtime(&self) -> Result<RuntimeStory, CompilerError> {
        let state = crate::runtime_export::ExportState::new();
        let list_defs = self.export_runtime_list_defs();
        let mut named_content = HashMap::new();

        for flow in self.parsed_flows() {
            let name = flow.flow().identifier().unwrap_or_default().to_owned();
            named_content.insert(name.clone(), flow.export_runtime(&state, self, &name)?);
        }

        if let Some(global_decl) = self.export_global_decl_runtime()? {
            named_content.insert("global decl".to_owned(), global_decl);
        }

        let inner_root = crate::runtime_export::export_weave(
            &state,
            "0",
            self.root_nodes(),
            crate::runtime_export::Scope::Root,
            self,
            true,
            &HashMap::new(),
        )?;
        let root = Container::new(
            None,
            self.object().count_flags(self.count_all_visits, false),
            vec![inner_root, crate::runtime_export::command(CommandType::Done)],
            named_content,
        );
        self.object().set_runtime_object(root.clone());
        self.object().set_container_for_counting(root.clone());

        state.apply_path_fixups();

        RuntimeStory::from_compiled(root, list_defs)
            .map_err(|error| CompilerError::invalid_source(error.to_string()))
    }

    pub(crate) fn flow_count_flags_for(&self, object: &ParsedObject) -> i32 {
        object.count_flags(self.count_all_visits, false)
    }

    pub(crate) fn weave_count_flags_for(&self, object: &ParsedObject) -> i32 {
        object.count_flags(self.count_all_visits, true)
    }

    pub(crate) fn find_flow_by_name(&self, name: &str) -> Option<&ParsedFlow> {
        Self::find_flow_by_name_in(&self.flows, name)
    }

    pub(crate) fn find_flow_by_path(&self, path: &str) -> Option<&ParsedFlow> {
        let mut parts = path.split('.');
        let first = parts.next()?;
        let root = self
            .flows
            .iter()
            .find(|flow| flow.flow().identifier() == Some(first))?;
        let rest: Vec<&str> = parts.collect();
        root.find_child_flow_by_path(&rest)
    }

    fn find_flow_by_name_in<'a>(flows: &'a [ParsedFlow], name: &str) -> Option<&'a ParsedFlow> {
        for flow in flows {
            if flow.flow().identifier() == Some(name) {
                return Some(flow);
            }
            if let Some(found) = Self::find_flow_by_name_in(flow.children(), name) {
                return Some(found);
            }
        }

        None
    }

    pub(crate) fn has_flow_path(&self, target: &str) -> bool {
        self.find_flow_by_path(target).is_some()
    }

    pub(crate) fn runtime_target_cache_for_path(
        &self,
        path: &str,
    ) -> Option<std::rc::Rc<super::ParsedRuntimeCache>> {
        self.find_flow_by_path(path)
            .map(|flow| flow.object().runtime_cache_handle())
    }

    pub(crate) fn runtime_target_cache_for_ref(
        &self,
        target: super::ParsedObjectRef,
    ) -> Option<std::rc::Rc<super::ParsedRuntimeCache>> {
        if self.object().reference() == target {
            return Some(self.object().runtime_cache_handle());
        }

        for node in &self.root_nodes {
            if let Some(found) = runtime_target_cache_in_node(node, target) {
                return Some(found);
            }
        }
        for flow in &self.flows {
            if let Some(found) = runtime_target_cache_in_flow(flow, target) {
                return Some(found);
            }
        }

        None
    }

    pub(crate) fn resolve_target_ref(&self, target: &str) -> Option<super::ParsedObjectRef> {
        self.find_flow_by_path(target)
            .map(|flow| flow.object().reference())
            .or_else(|| self.resolve_explicit_named_label_ref(target))
            .or_else(|| self.resolve_named_label_ref(target))
    }

    pub(crate) fn has_named_label(&self, target: &str) -> bool {
        self.root_nodes.iter().any(|node| Self::node_has_named_label(node, target))
            || self
                .flows
                .iter()
                .any(|flow| Self::flow_has_named_label(flow, target))
    }

    fn flow_has_named_label(flow: &ParsedFlow, target: &str) -> bool {
        flow.content().iter().any(|node| Self::node_has_named_label(node, target))
            || flow.children().iter().any(|child| Self::flow_has_named_label(child, target))
    }

    fn node_has_named_label(node: &ParsedNode, target: &str) -> bool {
        node.name() == Some(target)
            || node
                .start_content()
                .iter()
                .any(|child| Self::node_has_named_label(child, target))
            || node
                .choice_only_content()
                .iter()
                .any(|child| Self::node_has_named_label(child, target))
            || node.children().iter().any(|child| Self::node_has_named_label(child, target))
    }

    fn resolve_named_label_ref(&self, target: &str) -> Option<super::ParsedObjectRef> {
        self.root_nodes
            .iter()
            .find_map(|node| resolve_named_label_ref_in_node(node, target))
            .or_else(|| {
                self.flows
                    .iter()
                    .find_map(|flow| resolve_named_label_ref_in_flow(flow, target))
            })
    }

    fn resolve_explicit_named_label_ref(&self, target: &str) -> Option<super::ParsedObjectRef> {
        let mut parts: Vec<&str> = target.split('.').collect();
        if parts.len() < 2 {
            return None;
        }

        let label = parts.pop()?;
        let flow_path = parts.join(".");
        let flow = self.find_flow_by_path(&flow_path)?;
        resolve_named_label_ref_in_flow(flow, label)
    }

    pub(crate) fn collect_const_names(&self) -> HashSet<String> {
        self.const_declarations
            .iter()
            .map(|declaration| declaration.name().to_owned())
            .collect()
    }

    pub(crate) fn collect_declared_variable_names(&self) -> HashSet<String> {
        let mut names: HashSet<String> = self
            .global_initializers
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        for node in &self.root_nodes {
            node.collect_global_declared_vars(&mut names);
        }
        for flow in &self.flows {
            collect_global_declared_vars_in_flow(flow, &mut names);
        }
        names
    }

    pub(crate) fn collect_top_level_flow_names(&self) -> HashSet<String> {
        self.flows
            .iter()
            .filter_map(|flow| flow.flow().identifier().map(ToOwned::to_owned))
            .collect()
    }

    pub(crate) fn collect_all_flow_names(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        collect_all_flow_names_into(&self.flows, &mut names);
        names
    }

    pub(crate) fn root_validation_scope(
        &self,
        global_vars: &HashSet<String>,
        const_names: &HashSet<String>,
        flow_names: &HashSet<String>,
        top_level_flow_names: &HashSet<String>,
    ) -> Result<ValidationScope, CompilerError> {
        let root_temp_vars = self.collect_root_temp_vars();
        let mut visible_vars = global_vars.clone();
        visible_vars.extend(const_names.iter().cloned());
        visible_vars.extend(root_temp_vars.iter().cloned());

        Ok(ValidationScope {
            visible_vars,
            divert_target_vars: global_vars.clone(),
            top_level_flow_names: top_level_flow_names.clone(),
            sibling_flow_names: top_level_flow_names.clone(),
            local_labels: self.collect_root_named_labels()?,
            all_flow_names: flow_names.clone(),
        })
    }

    pub(crate) fn collect_root_temp_vars(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        for node in &self.root_nodes {
            node.collect_temp_vars(&mut names);
        }
        names
    }

    pub(crate) fn collect_root_named_labels(&self) -> Result<HashSet<String>, CompilerError> {
        let mut names = HashSet::new();
        for node in &self.root_nodes {
            node.collect_named_labels(&mut names)?;
        }
        Ok(names)
    }

    pub(crate) fn validate_root_scope(
        &self,
        flow_names: &HashSet<String>,
        root_scope: &ValidationScope,
    ) -> Result<(), CompilerError> {
        let root_temp_vars = self.collect_root_temp_vars();
        for temp in &root_temp_vars {
            if flow_names.contains(temp) {
                return Err(CompilerError::invalid_source(format!(
                    "Variable '{}' already exists as a flow or function name",
                    temp
                )));
            }
        }

        if !self.root_nodes.iter().any(|node| {
            matches!(
                node.kind(),
                super::ParsedNodeKind::Choice
                    | super::ParsedNodeKind::GatherPoint
                    | super::ParsedNodeKind::GatherLabel
            )
        }) {
            for node in self.root_nodes() {
                if node
                    .as_conditional()
                    .is_some_and(|conditional| conditional.requires_weave_context())
                {
                    return Err(CompilerError::invalid_source(
                        "Nested choice inside a top-level conditional requires a weave context",
                    ));
                }
            }
        }

        ParsedNode::validate_list(self.root_nodes(), root_scope, self)
    }

    pub fn object_index(&self) -> ParsedObjectIndex {
        let mut index = ParsedObjectIndex::new();
        index.register(self.object());
        self.register_content_list(&mut index, &self.root_content);
        for node in &self.root_nodes {
            self.register_parsed_node(&mut index, node);
        }
        for flow in &self.flows {
            self.register_parsed_flow(&mut index, flow);
        }
        index
    }

    fn register_content_list(&self, index: &mut ParsedObjectIndex, content_list: &ContentList) {
        index.register(content_list.object());
        for content in content_list.content() {
            match content {
                Content::Text(text) => index.register(text.object()),
            }
        }
    }

    pub(crate) fn rebuild_parse_tree_refs(&mut self) {
        let story_ref = self.object().reference();
        for node in &mut self.root_nodes {
            node.object_mut().set_parent_ref(story_ref);
            self.flow
                .object_mut()
                .add_content_ref(node.object().reference());
        }
        for flow in &mut self.flows {
            flow.object_mut().set_parent_ref(story_ref);
            self.flow
                .object_mut()
                .add_content_ref(flow.object().reference());
        }
    }

    fn register_parsed_node(&self, index: &mut ParsedObjectIndex, node: &ParsedNode) {
        index.register(node.object());
        for child in node.start_content() {
            self.register_parsed_node(index, child);
        }
        for child in node.choice_only_content() {
            self.register_parsed_node(index, child);
        }
        for child in node.children() {
            self.register_parsed_node(index, child);
        }
    }

    fn register_parsed_flow(&self, index: &mut ParsedObjectIndex, flow: &ParsedFlow) {
        index.register(flow.object());
        for node in flow.content() {
            self.register_parsed_node(index, node);
        }
        for child in flow.children() {
            self.register_parsed_flow(index, child);
        }
    }

    fn validate_empty_diverts(&self) -> Result<(), CompilerError> {
        for (index, line) in self.source().lines().enumerate() {
            if line.trim() == "->" {
                return Err(CompilerError::invalid_source(
                    "Empty diverts (->) are only valid on choices",
                )
                .with_line(index + 1));
            }
        }

        Ok(())
    }

    fn resolve_parsed_targets(&mut self) {
        let story_ref = self.clone();
        for node in &mut self.root_nodes {
            node.resolve_targets(&story_ref);
        }
        for flow in &mut self.flows {
            flow.resolve_targets(&story_ref);
        }
    }

    fn apply_counting_marks(&mut self, resolved_story: &Story) {
        for node in &resolved_story.root_nodes {
            apply_counting_marks_in_node(node, self);
        }
        for flow in &resolved_story.flows {
            apply_counting_marks_in_flow(flow, self);
        }
    }

    pub(crate) fn mark_count_target(
        &mut self,
        target: super::ParsedObjectRef,
        count_turns: bool,
    ) {
        if self.object().reference() == target {
            if count_turns {
                self.object().mark_turn_index_should_be_counted();
            } else {
                self.object().mark_visits_should_be_counted();
            }
            return;
        }

        for node in &mut self.root_nodes {
            if node.mark_count_target(target, count_turns) {
                return;
            }
        }
        for flow in &mut self.flows {
            if flow.mark_count_target(target, count_turns) {
                return;
            }
        }
    }

    fn export_runtime_list_defs(&self) -> Vec<RuntimeListDefinition> {
        self.list_definitions()
            .iter()
            .filter_map(|definition| {
                let name = definition.identifier()?.to_owned();
                let items = definition
                    .item_definitions()
                    .iter()
                    .map(|item| (item.name().to_owned(), item.series_value()))
                    .collect();
                Some(RuntimeListDefinition::new(name, items))
            })
            .collect()
    }

    fn export_global_decl_runtime(&self) -> Result<Option<Rc<Container>>, CompilerError> {
        if self.global_initializers().is_empty() && self.list_definitions().is_empty() {
            return Ok(None);
        }

        let mut content: Vec<Rc<dyn RTObject>> = vec![crate::runtime_export::command(CommandType::EvalStart)];

        for list in self.list_definitions() {
            content.push(list_value_from_definition(list));
            if let Some(name) = list.identifier() {
                content.push(crate::runtime_export::variable_assignment(name, true, true));
            }
        }

        for (name, expression) in self.global_initializers() {
            crate::runtime_export::export_expression(expression, self, &mut content)?;
            content.push(crate::runtime_export::variable_assignment(name, true, true));
        }

        content.push(crate::runtime_export::command(CommandType::EvalEnd));
        content.push(crate::runtime_export::command(CommandType::End));
        Ok(Some(Container::new(
            Some("global decl".to_owned()),
            0,
            content,
            HashMap::new(),
        )))
    }
}

fn list_value_from_definition(definition: &ListDefinition) -> Rc<dyn RTObject> {
    let mut list = InkList::new();
    if let Some(name) = definition.identifier() {
        list.set_initial_origin_names(vec![name.to_owned()]);
        for item in definition.item_definitions() {
            if item.in_initial_list() {
                list.items.insert(
                    InkListItem::new(Some(name.to_owned()), item.name().to_owned()),
                    item.series_value(),
                );
            }
        }
    }
    crate::runtime_export::rt_value(list)
}

fn collect_all_flow_names_into(flows: &[ParsedFlow], names: &mut HashSet<String>) {
    for flow in flows {
        if let Some(name) = flow.flow().identifier() {
            names.insert(name.to_owned());
        }
        collect_all_flow_names_into(flow.children(), names);
    }
}

fn collect_global_declared_vars_in_flow(flow: &ParsedFlow, names: &mut HashSet<String>) {
    for node in flow.content() {
        node.collect_global_declared_vars(names);
    }
    for child in flow.children() {
        collect_global_declared_vars_in_flow(child, names);
    }
}

fn apply_counting_marks_in_flow(flow: &ParsedFlow, story: &mut Story) {
    for node in flow.content() {
        apply_counting_marks_in_node(node, story);
    }
    for child in flow.children() {
        apply_counting_marks_in_flow(child, story);
    }
}

fn apply_counting_marks_in_node(node: &ParsedNode, story: &mut Story) {
    if let Some(expression) = node.expression() {
        apply_counting_marks_in_expression(expression, story);
    }
    if let Some(condition) = node.condition() {
        apply_counting_marks_in_expression(condition, story);
    }
    for child in node.start_content() {
        apply_counting_marks_in_node(child, story);
    }
    for child in node.choice_only_content() {
        apply_counting_marks_in_node(child, story);
    }
    for child in node.children() {
        apply_counting_marks_in_node(child, story);
    }
}

fn apply_counting_marks_in_expression(expression: &ParsedExpression, story: &mut Story) {
    match expression {
        ParsedExpression::Variable {
            resolved_count_target,
            ..
        } => {
            if let Some(target) = resolved_count_target {
                story.mark_count_target(*target, false);
            }
        }
        ParsedExpression::DivertTarget { .. } => {}
        ParsedExpression::Unary { expression, .. } => apply_counting_marks_in_expression(expression, story),
        ParsedExpression::Binary { left, right, .. } => {
            apply_counting_marks_in_expression(left, story);
            apply_counting_marks_in_expression(right, story);
        }
        ParsedExpression::FunctionCall {
            path,
            arguments,
            ..
        } => {
            if matches!(path.as_str(), "TURNS_SINCE" | "READ_COUNT")
                && let Some(argument) = arguments.first()
            {
                if let Some(target) = argument.resolved_target().or_else(|| argument.resolved_count_target()) {
                    story.mark_count_target(target, path.as_str() == "TURNS_SINCE");
                }
            }
            for argument in arguments {
                apply_counting_marks_in_expression(argument, story);
            }
        }
        ParsedExpression::StringExpression(nodes) => {
            for node in nodes {
                apply_counting_marks_in_node(node, story);
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


fn runtime_target_cache_in_flow(
    flow: &ParsedFlow,
    target: super::ParsedObjectRef,
) -> Option<std::rc::Rc<super::ParsedRuntimeCache>> {
    if flow.object().reference() == target {
        return Some(flow.object().runtime_cache_handle());
    }
    for node in flow.content() {
        if let Some(found) = runtime_target_cache_in_node(node, target) {
            return Some(found);
        }
    }
    for child in flow.children() {
        if let Some(found) = runtime_target_cache_in_flow(child, target) {
            return Some(found);
        }
    }
    None
}

fn runtime_target_cache_in_node(
    node: &ParsedNode,
    target: super::ParsedObjectRef,
) -> Option<std::rc::Rc<super::ParsedRuntimeCache>> {
    if node.object().reference() == target {
        return Some(node.object().runtime_cache_handle());
    }
    for child in node.start_content() {
        if let Some(found) = runtime_target_cache_in_node(child, target) {
            return Some(found);
        }
    }
    for child in node.choice_only_content() {
        if let Some(found) = runtime_target_cache_in_node(child, target) {
            return Some(found);
        }
    }
    for child in node.children() {
        if let Some(found) = runtime_target_cache_in_node(child, target) {
            return Some(found);
        }
    }
    None
}

fn resolve_named_label_ref_in_flow(
    flow: &ParsedFlow,
    target: &str,
) -> Option<super::ParsedObjectRef> {
    flow.content()
        .iter()
        .find_map(|node| resolve_named_label_ref_in_node(node, target))
        .or_else(|| {
            flow.children()
                .iter()
                .find_map(|child| resolve_named_label_ref_in_flow(child, target))
        })
}

fn resolve_named_label_ref_in_node(
    node: &ParsedNode,
    target: &str,
) -> Option<super::ParsedObjectRef> {
    if node.name() == Some(target) {
        return Some(node.object().reference());
    }
    for child in node.start_content() {
        if let Some(found) = resolve_named_label_ref_in_node(child, target) {
            return Some(found);
        }
    }
    for child in node.choice_only_content() {
        if let Some(found) = resolve_named_label_ref_in_node(child, target) {
            return Some(found);
        }
    }
    for child in node.children() {
        if let Some(found) = resolve_named_label_ref_in_node(child, target) {
            return Some(found);
        }
    }
    None
}


#[cfg(test)]
mod tests {
    use super::Story;
    use crate::parsed_hierarchy::{Content, ContentList, DebugMetadata};

    #[test]
    fn story_sets_root_content_parent() {
        let story = Story::new("hello", Some("main.ink".to_owned()), true);
        assert_eq!(
            Some(story.object().id()),
            story.root_content().object().parent_id()
        );
    }

    #[test]
    fn content_list_trims_trailing_inline_whitespace() {
        let mut list = ContentList::new();
        list.push_text("hello \t");
        list.trim_trailing_whitespace();
        let text = match &list.content()[0] {
            Content::Text(text) => text.text(),
        };
        assert_eq!("hello", text);
    }

    #[test]
    fn object_can_hold_debug_metadata() {
        let mut story = Story::new("hello", None, true);
        story.object_mut().set_debug_metadata(DebugMetadata {
            start_line_number: 1,
            end_line_number: 1,
            start_character_number: 1,
            end_character_number: 5,
            file_name: Some("main.ink".to_owned()),
        });
        assert!(story.object().has_own_debug_metadata());
    }

    #[test]
    fn story_object_index_resolves_root_content_tree() {
        let mut story = Story::new("hello", None, true);
        story.root_content_mut().push_text("hello");
        let index = story.object_index();
        let text = story
            .object()
            .find(&index, crate::parsed_hierarchy::ObjectKind::Text);
        assert!(text.is_some());
        assert_eq!(
            Some(story.object().reference()),
            index.story_for(text.unwrap())
        );
    }
}
