fn emit_global_declarations(
    globals: &[GlobalVariable],
    list_decls: &[ListDeclaration],
) -> Result<EmittedContainer, CompilerError> {
    let mut container = EmittedContainer::default();
    container.push(json!("ev"));

    // Emit list declarations before regular var declarations
    // (inklecate emits LIST decls before VAR decls in global decl)
    for list_decl in list_decls {
        // Emit the initial value: only the items that are marked as selected
        let mut selected = Map::new();
        for (item_name, value, initially_selected) in &list_decl.items {
            if *initially_selected {
                let key = format!("{}.{}", list_decl.name, item_name);
                selected.insert(key, json!(value));
            }
        }
        if selected.is_empty() {
            // No selected items → include origins so the runtime knows which list this belongs to
            container.push(json!({ "list": Value::Object(selected), "origins": [list_decl.name] }));
        } else {
            container.push(json!({ "list": Value::Object(selected) }));
        }
        container.push(json!({ "VAR=": list_decl.name }));
    }

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
        let target = joined_path(&scope.path, &flow.children[0].name);
        container.push(json!({"->": target}));
    }

    for child in &flow.children {
        container.insert_named(
            child.name.clone(),
            emit_nested_flow(child, &scope, context)?,
        );
    }

    container.into_json_array(None, Some(flow_count_flags(&scope.path, context)))
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
        let target = joined_path(&scope.path, &flow.children[0].name);
        container.push(json!({"->": target}));
    }

    for child in &flow.children {
        container.insert_named(
            child.name.clone(),
            emit_nested_flow(child, &scope, context)?,
        );
    }

    container.into_json_array(None, Some(flow_count_flags(&scope.path, context)))
}

fn flow_count_flags(path: &str, context: &EmitContext) -> i32 {
    let mut flags = context
        .flow_count_flags
        .get(path)
        .copied()
        .unwrap_or_default();
    if context.count_all_visits {
        flags |= COUNT_VISITS;
    }
    flags
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

/// Pre-scan all nodes (including continuations) to collect every choice label
/// as an absolute path. This allows cross-block label references (e.g., {greet}
/// in a second choice block referencing a label from the first block).
fn collect_all_choice_labels(nodes: &[Node], scope: &EmitScope) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    collect_choice_labels_recursive(nodes, scope, &mut labels, &mut 0);
    labels
}

fn collect_story_choice_labels(story: &ParsedStory) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    let root_scope = EmitScope::root(story.flows());

    collect_qualified_choice_labels_for_scope(story.root(), &root_scope, &mut labels);
    for flow in story.flows() {
        collect_qualified_choice_labels_for_flow(flow, &root_scope, &mut labels);
    }

    labels
}

fn collect_qualified_choice_labels_for_flow(
    flow: &Flow,
    parent_scope: &EmitScope,
    labels: &mut BTreeMap<String, String>,
) {
    let scope = parent_scope.child_flow(flow);
    collect_qualified_choice_labels_for_scope(&flow.nodes, &scope, labels);

    for child in &flow.children {
        collect_qualified_choice_labels_for_flow(child, &scope, labels);
    }
}

fn collect_qualified_choice_labels_for_scope(
    nodes: &[Node],
    scope: &EmitScope,
    labels: &mut BTreeMap<String, String>,
) {
    for (label, path) in collect_all_choice_labels(nodes, scope) {
        if scope.path == "0" {
            labels.insert(label, path);
        } else {
            labels.insert(format!("{}.{}", scope.path, label), path);
        }
    }
}

fn collect_choice_labels_recursive(
    nodes: &[Node],
    scope: &EmitScope,
    labels: &mut BTreeMap<String, String>,
    choice_index: &mut usize,
) {
    let mut i = 0;
    while i < nodes.len() {
        match &nodes[i] {
            Node::Choice(choice) => {
                let branch_index = *choice_index;
                if let Some(label) = &choice.label {
                    labels.insert(label.clone(), format!("{}.c-{branch_index}", scope.path));
                }
                let branch_scope = scope.choice_branch(&format!("c-{branch_index}"));
                let mut nested_choice_index = 0;
                collect_choice_labels_recursive(
                    &choice.body,
                    &branch_scope,
                    labels,
                    &mut nested_choice_index,
                );
                let level = choice.nesting_level;
                *choice_index += 1;
                i += 1;

                // Find where the choice block ends (non-Choice node or different nesting level)
                let block_start_ci = *choice_index - 1; // index of first choice in block
                // collect any remaining adjacent choices at the same nesting level
                while i < nodes.len() {
                    match &nodes[i] {
                        Node::Choice(c) if c.nesting_level == level => {
                            let branch_index = *choice_index;
                            if let Some(label) = &c.label {
                                labels.insert(
                                    label.clone(),
                                    format!("{}.c-{branch_index}", scope.path),
                                );
                            }
                            let branch_scope = scope.choice_branch(&format!("c-{branch_index}"));
                            let mut nested_choice_index = 0;
                            collect_choice_labels_recursive(
                                &c.body,
                                &branch_scope,
                                labels,
                                &mut nested_choice_index,
                            );
                            *choice_index += 1;
                            i += 1;
                        }
                        _ => break,
                    }
                }
                // Recurse into the continuation as g-<first_choice_in_block>
                let continuation = &nodes[i..];
                if !continuation.is_empty() {
                    let g_name = format!("g-{}", block_start_ci);
                    let (child_scope, continuation) =
                        if let Some(Node::GatherLabel(label)) = continuation.first() {
                            labels.insert(label.clone(), format!("{}.{}", scope.path, label));
                            (scope.choice_branch(label), &continuation[1..])
                        } else {
                            (scope.choice_branch(&g_name), continuation)
                        };
                    let mut child_index = 0;
                    collect_choice_labels_recursive(
                        continuation,
                        &child_scope,
                        labels,
                        &mut child_index,
                    );
                }
                return; // choice block consumes the rest via continuation
            }
            Node::GatherLabel(label) => {
                // Standalone gather label (loop-back point): record its path so
                // DivertTarget expressions can resolve it. Remaining nodes are
                // emitted inside the label's sub-scope. Generated g-N continuations
                // are hoisted back to the parent by emit_nodes_with_continuation,
                // so mirror that path rewrite while collecting nested labels.
                labels.insert(label.clone(), format!("{}.{}", scope.path, label));
                let sub_scope = scope.choice_branch(label);
                let mut child_index = 0;
                let mut nested_labels = BTreeMap::new();
                collect_choice_labels_recursive(
                    &nodes[i + 1..],
                    &sub_scope,
                    &mut nested_labels,
                    &mut child_index,
                );
                let nested_prefix = format!("{}.", sub_scope.path);
                for (nested_label, path) in nested_labels {
                    let path = path
                        .strip_prefix(&nested_prefix)
                        .and_then(|suffix| {
                            let first = suffix.split('.').next()?;
                            first
                                .strip_prefix("g-")?
                                .parse::<usize>()
                                .ok()
                                .map(|_| format!("{}.{}", scope.path, suffix))
                        })
                        .unwrap_or(path);
                    labels.insert(nested_label, path);
                }
                return; // remaining nodes consumed by sub-scope recursion
            }
            _ => {
                i += 1;
            }
        }
    }
}
