/// `serde_json::json!(f32_value)` would widen to `f64` first, introducing
/// conversion artefacts like `0.4 → 0.4000000059604645`.
fn float_to_json(value: f32) -> Value {
    let mut buf = ryu::Buffer::new();
    let s = buf.format(value);
    let n: serde_json::Number = s.parse().expect("ryu produced invalid number");
    Value::Number(n)
}

pub fn story_to_json_string(
    story: &ParsedStory,
    count_all_visits: bool,
) -> Result<String, CompilerError> {
    let json = story_to_json_value(story, count_all_visits)?;
    serde_json::to_string(&json).map_err(|error| {
        CompilerError::invalid_source(format!("failed to serialize compiled ink: {error}"))
    })
}

pub fn story_to_json_value(
    story: &ParsedStory,
    count_all_visits: bool,
) -> Result<Value, CompilerError> {
    let context = EmitContext::new(story, count_all_visits);
    let root_scope = EmitScope::root(story.flows());

    let mut root_container =
        emit_nodes_with_continuation(story.root(), &root_scope, &context, Some("0.g-0"))?;
    // Only add the default done/g-0 container if choices haven't already created a g-0 continuation.
    if !root_container.named.contains_key("g-0") {
        root_container.push(json!(["done", {"#n": "g-0"}]));
    }

    let mut named_content = Map::new();

    for flow in story.flows() {
        named_content.insert(flow.name.clone(), emit_flow(flow, &context)?);
    }

    if !story.globals().is_empty() || !story.list_declarations().is_empty() {
        named_content.insert(
            "global decl".to_owned(),
            emit_global_declarations(story.globals(), story.list_declarations())?
                .into_json_array(None, None)?,
        );
    }

    // Build listDefs
    let mut list_defs = Map::new();
    for list_decl in story.list_declarations() {
        let mut items = Map::new();
        for (item_name, value, _selected) in &list_decl.items {
            items.insert(item_name.clone(), json!(value));
        }
        list_defs.insert(list_decl.name.clone(), Value::Object(items));
    }

    // Emit keys in inklecate-compatible order: inkVersion, root, listDefs.
    let root_value = json!([
        root_container.into_json_array(None, None)?,
        "done",
        if named_content.is_empty() {
            Value::Null
        } else {
            Value::Object(named_content)
        }
    ]);

    let mut output = serde_json::Map::new();
    output.insert("inkVersion".to_owned(), json!(INK_VERSION_CURRENT));
    output.insert("root".to_owned(), root_value);
    output.insert("listDefs".to_owned(), Value::Object(list_defs));
    Ok(Value::Object(output))
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
    /// Siblings of the current flow (i.e., other stitches of the same knot).
    /// Used to resolve divert targets that are siblings.
    sibling_flow_names: BTreeSet<String>,
    choice_label_targets: BTreeMap<String, String>,
    /// Number of tokens that will be prepended (e.g. function parameters)
    /// before the content emitted by emit_nodes. This offset is added when
    /// computing absolute indices (e.g. conditional rejoin targets).
    param_offset: usize,
    /// Parameter names for the current flow (live in temp-variable frame).
    temp_param_names: BTreeSet<String>,
    /// Divert-typed parameter names for the current flow (valid `-> var` targets).
    divert_param_names: BTreeSet<String>,
    /// Number of `^` hops needed to navigate from content in this scope back to
    /// the enclosing knot/stitch container (used for relative divert paths).
    /// - 0  : root (relative paths not used)
    /// - 1  : knot body
    /// - 2  : stitch body
    /// - 3  : inside a conditional branch (b) within a knot or stitch
    relative_depth: usize,
    /// The path of the enclosing flow container (knot or stitch).
    /// For branch scopes, this is the scope.path of the parent flow scope.
    /// Used to compute the suffix for relative divert paths.
    flow_path: String,
    /// Generated continuation containers can be renamed or hoisted after emission.
    /// Flow targets emitted inside them must therefore use stable absolute paths.
    absolute_flow_targets: bool,
}

struct EmitContext {
    global_variables: BTreeSet<String>,
    top_flow_names: BTreeSet<String>,
    count_all_visits: bool,
    /// Set of LIST declaration names (to detect list-typed function calls)
    list_names: BTreeSet<String>,
    /// For each list name, a map from bare item name → (qualified name, value)
    list_items: std::collections::BTreeMap<String, Vec<(String, u32)>>,
    /// Set of EXTERNAL function declaration names
    external_functions: BTreeSet<String>,
    /// Set of flow names referenced as arguments to TURNS_SINCE()
    turns_since_targets: BTreeSet<String>,
    /// Fully-qualified authored label targets (e.g. `knot.stitch.choice`) mapped
    /// to their emitted runtime paths (e.g. `knot.stitch.c-0`).
    qualified_choice_labels: BTreeMap<String, String>,
    /// For each function, whether each parameter position is `ref`.
    function_ref_param_positions: BTreeMap<String, Vec<bool>>,
    /// Unqualified flow/stitch target names mapped to their absolute path
    /// when the name is unique across the story.
    unqualified_flow_targets: BTreeMap<String, String>,
}

fn register_unqualified_flow_target(
    shortcuts: &mut BTreeMap<String, Option<String>>,
    short_name: &str,
    absolute_path: String,
) {
    match shortcuts.get(short_name) {
        None => {
            shortcuts.insert(short_name.to_owned(), Some(absolute_path));
        }
        Some(Some(existing)) if existing == &absolute_path => {}
        _ => {
            shortcuts.insert(short_name.to_owned(), None);
        }
    }
}

fn collect_unqualified_flow_targets_recursive(
    flow: &Flow,
    absolute_path: &str,
    shortcuts: &mut BTreeMap<String, Option<String>>,
) {
    register_unqualified_flow_target(shortcuts, &flow.name, absolute_path.to_owned());

    for child in &flow.children {
        let child_path = format!("{absolute_path}.{}", child.name);
        collect_unqualified_flow_targets_recursive(child, &child_path, shortcuts);
    }
}

fn collect_unqualified_flow_targets(story: &ParsedStory) -> BTreeMap<String, String> {
    let mut shortcuts: BTreeMap<String, Option<String>> = BTreeMap::new();

    for flow in story.flows() {
        collect_unqualified_flow_targets_recursive(flow, &flow.name, &mut shortcuts);
    }

    shortcuts
        .into_iter()
        .filter_map(|(name, maybe_path)| maybe_path.map(|path| (name, path)))
        .collect()
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
        let flags = count_flags.unwrap_or_default();
        let has_flags = flags > 0;

        if !self.named.is_empty() || has_name || has_flags {
            let mut terminator = self.named;

            if has_flags {
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

fn collect_turns_since_targets_from_flows_into(flows: &[Flow], targets: &mut BTreeSet<String>) {
    for flow in flows {
        collect_turns_since_targets_from_nodes(&flow.nodes, targets);
        collect_turns_since_targets_from_flows_into(&flow.children, targets);
    }
}

fn collect_turns_since_targets_from_nodes(nodes: &[Node], targets: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::OutputExpression(expr) => {
                collect_turns_since_targets_from_expr(expr, targets);
            }
            Node::Choice(choice) => {
                for cond in &choice.conditions {
                    if let Condition::Expression(e) = cond {
                        collect_turns_since_targets_from_expr(e, targets);
                    }
                }
                collect_turns_since_targets_from_nodes(&choice.body, targets);
            }
            Node::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                if let Condition::Expression(e) = condition {
                    collect_turns_since_targets_from_expr(e, targets);
                }
                collect_turns_since_targets_from_nodes(when_true, targets);
                if let Some(nodes) = when_false {
                    collect_turns_since_targets_from_nodes(nodes, targets);
                }
            }
            Node::SwitchConditional { value, branches } => {
                collect_turns_since_targets_from_expr(value, targets);
                for (opt_expr, branch_nodes) in branches {
                    if let Some(e) = opt_expr {
                        collect_turns_since_targets_from_expr(e, targets);
                    }
                    collect_turns_since_targets_from_nodes(branch_nodes, targets);
                }
            }
            Node::Assignment { expression, .. } => {
                collect_turns_since_targets_from_expr(expression, targets);
            }
            Node::ReturnExpr(e) => {
                collect_turns_since_targets_from_expr(e, targets);
            }
            Node::VoidCall { args, .. } => {
                for arg in args {
                    if let Expression::DivertTarget(target) = arg {
                        targets.insert(target.clone());
                    } else {
                        collect_turns_since_targets_from_expr(arg, targets);
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_turns_since_targets_from_expr(expr: &Expression, targets: &mut BTreeSet<String>) {
    match expr {
        Expression::FunctionCall { args, .. } => {
            // Any DivertTarget passed as a function argument is a TURNS_SINCE candidate —
            // it may be passed directly to TURNS_SINCE or forwarded via a parameter.
            for arg in args {
                if let Expression::DivertTarget(target) = arg {
                    targets.insert(target.clone());
                } else {
                    collect_turns_since_targets_from_expr(arg, targets);
                }
            }
        }
        Expression::Negate(inner) | Expression::Not(inner) => {
            collect_turns_since_targets_from_expr(inner, targets);
        }
        Expression::Binary { left, right, .. } => {
            collect_turns_since_targets_from_expr(left, targets);
            collect_turns_since_targets_from_expr(right, targets);
        }
        _ => {}
    }
}

impl EmitContext {
    fn new(story: &ParsedStory, count_all_visits: bool) -> Self {
        let mut list_items = std::collections::BTreeMap::new();
        for list_decl in story.list_declarations() {
            let items: Vec<(String, u32)> = list_decl
                .items
                .iter()
                .map(|(name, value, _)| (format!("{}.{}", list_decl.name, name), *value))
                .collect();
            list_items.insert(list_decl.name.clone(), items);
        }
        let mut turns_since_targets = BTreeSet::new();
        collect_turns_since_targets_from_nodes(story.root(), &mut turns_since_targets);
        collect_turns_since_targets_from_flows_into(story.flows(), &mut turns_since_targets);
        let qualified_choice_labels = collect_story_choice_labels(story);
        let unqualified_flow_targets = collect_unqualified_flow_targets(story);
        let function_ref_param_positions = story
            .flows()
            .iter()
            .filter(|flow| flow.is_function)
            .map(|flow| {
                (
                    flow.name.clone(),
                    flow.parameters
                        .iter()
                        .map(|parameter| flow.ref_parameters.contains(parameter))
                        .collect(),
                )
            })
            .collect();
        Self {
            global_variables: story.globals().iter().map(|var| var.name.clone()).collect(),
            top_flow_names: story.flows().iter().map(|flow| flow.name.clone()).collect(),
            count_all_visits,
            list_names: story
                .list_declarations()
                .iter()
                .map(|l| l.name.clone())
                .collect(),
            list_items,
            external_functions: story.external_functions.iter().cloned().collect(),
            turns_since_targets,
            qualified_choice_labels,
            function_ref_param_positions,
            unqualified_flow_targets,
        }
    }

    /// Look up a bare item name (e.g. "b") across all lists.
    /// Returns the qualified name and value if found.
    fn resolve_list_item(&self, bare_name: &str) -> Option<(String, u32)> {
        // If already qualified (contains '.'), use as-is
        if bare_name.contains('.') {
            for items in self.list_items.values() {
                if let Some((qname, val)) = items.iter().find(|(q, _)| q == bare_name) {
                    return Some((qname.clone(), *val));
                }
            }
            return None;
        }
        for items in self.list_items.values() {
            if let Some((qname, val)) = items
                .iter()
                .find(|(q, _)| q.split('.').next_back() == Some(bare_name))
            {
                return Some((qname.clone(), *val));
            }
        }
        None
    }
}

impl EmitScope {
    fn root(flows: &[Flow]) -> Self {
        Self {
            path: "0".to_owned(),
            top_flow_name: None,
            child_flow_names: flows.iter().map(|flow| flow.name.clone()).collect(),
            sibling_flow_names: BTreeSet::new(),
            choice_label_targets: BTreeMap::new(),
            param_offset: 0,
            temp_param_names: BTreeSet::new(),
            divert_param_names: BTreeSet::new(),
            relative_depth: 0,
            flow_path: "0".to_owned(),
            absolute_flow_targets: false,
        }
    }

    fn child_flow(&self, child: &Flow) -> Self {
        let path = if self.path == "0" {
            child.name.clone()
        } else {
            format!("{}.{}", self.path, child.name)
        };

        // When entering a stitch (self already has a top_flow_name), pass down
        // the current child_flow_names as siblings so the stitch can resolve them.
        let siblings = if self.top_flow_name.is_some() {
            self.child_flow_names.clone()
        } else {
            BTreeSet::new()
        };

        // Depth to navigate back from content to the enclosing knot container:
        // - knot body  (path = "knot"):          1 hop
        // - stitch body (path = "knot.stitch"):  2 hops (up to stitch, up through knot's
        //                                        named-dict level to knot container)
        let is_stitch = self.top_flow_name.is_some();
        let new_depth = if is_stitch { 2 } else { 1 };

        Self {
            flow_path: path.clone(),
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
            sibling_flow_names: siblings,
            choice_label_targets: BTreeMap::new(),
            param_offset: child.parameters.len(),
            temp_param_names: child.parameters.iter().cloned().collect(),
            divert_param_names: child.divert_parameters.iter().cloned().collect(),
            relative_depth: new_depth,
            absolute_flow_targets: false,
        }
    }

    fn choice_branch(&self, branch_name: &str) -> Self {
        Self {
            path: format!("{}.{}", self.path, branch_name),
            flow_path: self.flow_path.clone(),
            top_flow_name: self.top_flow_name.clone(),
            child_flow_names: self.child_flow_names.clone(),
            sibling_flow_names: self.sibling_flow_names.clone(),
            choice_label_targets: self.choice_label_targets.clone(),
            param_offset: 0,
            temp_param_names: self.temp_param_names.clone(),
            divert_param_names: self.divert_param_names.clone(),
            relative_depth: self.relative_depth,
            absolute_flow_targets: self.absolute_flow_targets,
        }
    }

    fn continuation(&self, name: &str) -> Self {
        let mut scope = self.choice_branch(name);
        scope.absolute_flow_targets = true;
        scope
    }

    /// Create a scope for a conditional branch `b` body. Inside a conditional branch
    /// the runtime path gains two extra hops (conditional item index + `b` key), so
    /// `relative_depth` is set to 3 regardless of the parent depth.
    /// However, for nested conditionals (inside another conditional branch, depth >= 3),
    /// we cannot reliably navigate back to the flow with 3 `^`s, so we use 0 (absolute).
    fn conditional_branch(&self, branch_name: &str) -> Self {
        // Only use relative paths (depth=3) for direct conditional branches within a
        // flow body (knot depth=1 or stitch depth=2). For deeper nesting, use absolute.
        let branch_depth = if self.relative_depth == 1 || self.relative_depth == 2 {
            3
        } else {
            0
        };
        Self {
            path: format!("{}.{}", self.path, branch_name),
            flow_path: self.flow_path.clone(),
            top_flow_name: self.top_flow_name.clone(),
            child_flow_names: self.child_flow_names.clone(),
            sibling_flow_names: self.sibling_flow_names.clone(),
            choice_label_targets: self.choice_label_targets.clone(),
            param_offset: 0,
            temp_param_names: self.temp_param_names.clone(),
            divert_param_names: self.divert_param_names.clone(),
            relative_depth: branch_depth,
            absolute_flow_targets: self.absolute_flow_targets,
        }
    }

    fn with_choice_labels(&self, labels: BTreeMap<String, String>) -> Self {
        // Merge: start with existing labels and overlay the new ones
        let mut merged = self.choice_label_targets.clone();
        merged.extend(labels);
        Self {
            path: self.path.clone(),
            flow_path: self.flow_path.clone(),
            top_flow_name: self.top_flow_name.clone(),
            child_flow_names: self.child_flow_names.clone(),
            sibling_flow_names: self.sibling_flow_names.clone(),
            choice_label_targets: merged,
            param_offset: self.param_offset,
            temp_param_names: self.temp_param_names.clone(),
            divert_param_names: self.divert_param_names.clone(),
            relative_depth: self.relative_depth,
            absolute_flow_targets: self.absolute_flow_targets,
        }
    }

    fn with_relative_depth(&self, relative_depth: usize) -> Self {
        Self {
            path: self.path.clone(),
            flow_path: self.flow_path.clone(),
            top_flow_name: self.top_flow_name.clone(),
            child_flow_names: self.child_flow_names.clone(),
            sibling_flow_names: self.sibling_flow_names.clone(),
            choice_label_targets: self.choice_label_targets.clone(),
            param_offset: self.param_offset,
            temp_param_names: self.temp_param_names.clone(),
            divert_param_names: self.divert_param_names.clone(),
            relative_depth,
            absolute_flow_targets: self.absolute_flow_targets,
        }
    }

    /// Convert an absolute path like `knot.stitch` or `knot.stitch.7` to a relative path
    /// `.^...^.X` when we are currently inside that same knot/stitch.
    ///
    /// Uses `self.relative_depth` `^` hops. After those hops, the runtime reaches:
    ///
    /// - depth 1 or 2 (knot/stitch body): the enclosing KNOT container (`top_flow_name`)
    /// - depth 3 (branch scope): the enclosing FLOW container (`flow_path`)
    ///
    /// The suffix is the remainder of `absolute` after stripping the appropriate prefix.
    fn make_relative(&self, absolute: &str) -> String {
        let ups = ".^".repeat(self.relative_depth);
        let prefix = if self.relative_depth == 3 {
            // Inside a branch: 3 `^`s reaches flow_path (the enclosing knot/stitch)
            self.flow_path.as_str()
        } else {
            // In knot/stitch body: `^`s reach the knot
            self.top_flow_name.as_deref().unwrap_or(&self.path)
        };
        let suffix = absolute
            .strip_prefix(prefix)
            .unwrap_or(absolute)
            .trim_start_matches('.');
        if suffix.is_empty() {
            ups
        } else {
            format!("{ups}.{suffix}")
        }
    }

    fn resolve_divert_target(&self, target: &str, context: &EmitContext) -> String {
        if target == "END" || target == "DONE" {
            return target.to_owned();
        }

        if let Some(choice_target) = self.resolve_qualified_choice_label(target, context) {
            return choice_target;
        }

        if target.contains('.') {
            return target.to_owned();
        }

        if let Some(choice_target) = self.resolve_choice_label(target) {
            return choice_target.to_owned();
        }

        if context.global_variables.contains(target) && !context.top_flow_names.contains(target) {
            return target.to_owned();
        }

        if self.child_flow_names.contains(target) && self.top_flow_name.is_some() {
            let abs = format!("{}.{target}", self.top_flow_name.as_deref().unwrap());
            return if self.absolute_flow_targets {
                abs
            } else {
                self.make_relative(&abs)
            };
        }

        // Sibling stitch: target is a stitch of the same parent knot
        if self.sibling_flow_names.contains(target) && self.top_flow_name.is_some() {
            let abs = format!("{}.{target}", self.top_flow_name.as_deref().unwrap());
            return if self.absolute_flow_targets {
                abs
            } else {
                self.make_relative(&abs)
            };
        }

        if let Some(abs) = context.unqualified_flow_targets.get(target) {
            return abs.clone();
        }

        target.to_owned()
    }

    fn is_variable_divert(&self, target: &str, context: &EmitContext) -> bool {
        self.divert_param_names.contains(target)
            || (context.global_variables.contains(target)
                && !context.top_flow_names.contains(target))
    }

    fn resolve_choice_label(&self, label: &str) -> Option<&str> {
        self.choice_label_targets.get(label).map(String::as_str)
    }

    fn resolve_qualified_choice_label(
        &self,
        target: &str,
        context: &EmitContext,
    ) -> Option<String> {
        if let Some(resolved) = context.qualified_choice_labels.get(target) {
            return Some(resolved.clone());
        }

        let (prefix, label) = target.rsplit_once('.')?;
        let resolved = self.resolve_choice_label(label)?;
        let prefix_with_dot = format!("{prefix}.");
        if resolved == prefix || resolved.starts_with(&prefix_with_dot) {
            Some(resolved.to_owned())
        } else {
            None
        }
    }
}

