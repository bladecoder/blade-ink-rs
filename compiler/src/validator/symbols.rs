fn is_builtin_function(name: &str) -> bool {
    matches!(
        name,
        "RANDOM"
            | "SEED_RANDOM"
            | "POW"
            | "FLOOR"
            | "CEILING"
            | "INT"
            | "FLOAT"
            | "MIN"
            | "MAX"
            | "READ_COUNT"
            | "TURNS_SINCE"
            | "CHOICE_COUNT"
            | "TURNS"
            | "LIST_VALUE"
            | "LIST_ALL"
            | "LIST_INVERT"
            | "LIST_COUNT"
            | "LIST_MIN"
            | "LIST_MAX"
            | "LIST_RANGE"
            | "LIST_RANDOM"
    )
}

/// Collect gather/choice labels and their qualified paths into `targets`.
fn collect_labels_from_nodes(nodes: &[Node], prefix: &str, targets: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::GatherLabel(label) => {
                if prefix.is_empty() {
                    targets.insert(label.clone());
                } else {
                    targets.insert(format!("{prefix}.{label}"));
                }
            }
            Node::Choice(c) => {
                if let Some(lbl) = &c.label {
                    if prefix.is_empty() {
                        targets.insert(lbl.clone());
                    } else {
                        targets.insert(format!("{prefix}.{lbl}"));
                    }
                }
                collect_labels_from_nodes(&c.body, prefix, targets);
            }
            Node::Conditional {
                when_true,
                when_false,
                ..
            } => {
                collect_labels_from_nodes(when_true, prefix, targets);
                if let Some(wf) = when_false {
                    collect_labels_from_nodes(wf, prefix, targets);
                }
            }
            Node::SwitchConditional { branches, .. } => {
                for (_, body) in branches {
                    collect_labels_from_nodes(body, prefix, targets);
                }
            }
            Node::Sequence(seq) => {
                for branch in &seq.branches {
                    collect_labels_from_nodes(branch, prefix, targets);
                }
            }
            _ => {}
        }
    }
}

/// Returns true if a choice has an explicit divert at the end of its body
/// (i.e., the choice explicitly redirects flow rather than falling through).
fn choice_has_explicit_divert(choice: &crate::ast::Choice) -> bool {
    nodes_end_with_divert(&choice.body)
}

fn nodes_end_with_divert(nodes: &[Node]) -> bool {
    nodes
        .iter()
        .rev()
        .find(|n| !matches!(n, Node::Newline))
        .is_some_and(|n| {
            matches!(
                n,
                Node::Divert(_)
                    | Node::TunnelReturn
                    | Node::TunnelOnwardsWithTarget { .. }
                    | Node::ReturnBool(_)
                    | Node::ReturnExpr(_)
            )
        })
}
