#[derive(Clone, Debug, Eq, PartialEq)]
struct RuntimePath {
    components: Vec<String>,
    relative: bool,
}

impl RuntimePath {
    fn absolute(path: &str) -> Self {
        Self::parse(path, false)
    }

    fn parse(path: &str, relative: bool) -> Self {
        let path = path.strip_prefix('.').unwrap_or(path);
        let components = if path.is_empty() {
            Vec::new()
        } else {
            path.split('.').map(str::to_owned).collect()
        };
        Self {
            components,
            relative,
        }
    }

    fn appended(&self, component: impl Into<String>) -> Self {
        let mut path = self.clone();
        path.components.push(component.into());
        path.relative = false;
        path
    }

    fn resolve_from(&self, origin: &Self) -> Self {
        if !self.relative {
            return self.clone();
        }

        let upward_moves = self
            .components
            .iter()
            .take_while(|component| component.as_str() == "^")
            .count();
        let retained = origin.components.len().saturating_sub(upward_moves);
        let mut components = origin.components[..retained].to_vec();
        components.extend(self.components[upward_moves..].iter().cloned());
        Self {
            components,
            relative: false,
        }
    }

    fn relative_to(&self, origin: &Self) -> Self {
        let shared = self
            .components
            .iter()
            .zip(&origin.components)
            .take_while(|(target, source)| target == source)
            .count();

        if shared == 0 {
            return self.clone();
        }

        let upward_moves = origin.components.len().saturating_sub(shared);
        let mut components = vec!["^".to_owned(); upward_moves];
        components.extend(self.components[shared..].iter().cloned());
        Self {
            components,
            relative: true,
        }
    }

    fn compact_from(&self, origin: &Self) -> String {
        let absolute = self.resolve_from(origin);
        let absolute_string = absolute.to_string();
        let relative_string = absolute.relative_to(origin).to_string();
        if relative_string.len() < absolute_string.len() {
            relative_string
        } else {
            absolute_string
        }
    }
}

impl std::fmt::Display for RuntimePath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.relative {
            formatter.write_str(".")?;
        }
        formatter.write_str(&self.components.join("."))
    }
}

fn joined_path(parent: &str, component: impl std::fmt::Display) -> String {
    if parent.is_empty() {
        component.to_string()
    } else {
        format!("{parent}.{component}")
    }
}

fn embedded_container_name(value: &Value) -> Option<&str> {
    value
        .as_array()?
        .last()?
        .as_object()?
        .get("#n")?
        .as_str()
}

fn compact_path_fields(value: &mut Value, origin: &RuntimePath) {
    let Value::Object(map) = value else {
        return;
    };
    let variable_target = map.get("var").and_then(Value::as_bool) == Some(true);

    for field in ["->", "f()", "->t->", "*", "CNT?"] {
        if variable_target && matches!(field, "->" | "f()" | "->t->") {
            continue;
        }
        let Some(target) = map.get_mut(field) else {
            continue;
        };
        let Some(target_path) = target.as_str() else {
            continue;
        };
        let parsed = RuntimePath::parse(target_path, target_path.starts_with('.'));
        *target = Value::String(parsed.compact_from(origin));
    }
}

fn compact_container_paths(value: &mut Value, container_path: &RuntimePath) {
    let Value::Array(values) = value else {
        return;
    };
    let Some((terminator, content)) = values.split_last_mut() else {
        return;
    };

    for (index, child) in content.iter_mut().enumerate() {
        let child_path = container_path.appended(
            embedded_container_name(child)
                .map(str::to_owned)
                .unwrap_or_else(|| index.to_string()),
        );
        if child.is_array() {
            compact_container_paths(child, &child_path);
        } else {
            compact_path_fields(child, &child_path);
        }
    }

    let Value::Object(named) = terminator else {
        return;
    };
    for (name, child) in named {
        if name == "#f" || name == "#n" || !child.is_array() {
            continue;
        }
        compact_container_paths(child, &container_path.appended(name.clone()));
    }
}

fn compact_story_paths(root: &mut Value) {
    compact_container_paths(root, &RuntimePath::absolute(""));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_path_matches_reference_algorithm() {
        let target = RuntimePath::absolute("knot.stitch.c-0");
        assert_eq!(
            ".^.c-0",
            target.compact_from(&RuntimePath::absolute("knot.stitch.4"))
        );
        assert_eq!(
            "0.c-0",
            RuntimePath::absolute("0.c-0").compact_from(&RuntimePath::absolute("0.4"))
        );
        assert_eq!(
            ".^.^.g-0",
            RuntimePath::absolute("knot.2.g-0")
                .compact_from(&RuntimePath::absolute("knot.2.c-0.8"))
        );
    }

    #[test]
    fn relative_paths_are_resolved_before_compaction() {
        let origin = RuntimePath::absolute("knot.2.c-0.8");
        let target = RuntimePath::parse(".^.^.g-0", true);
        assert_eq!(".^.^.g-0", target.compact_from(&origin));
    }

    #[test]
    fn hierarchy_walk_uses_named_and_indexed_container_paths() {
        let mut container = json!([
            [
                [
                    {"^->": "long_knot_name.0.loop.0.$r1"},
                    {"*": "long_knot_name.0.loop.c-0", "flg": 0},
                    null
                ],
                {
                    "c-0": [
                        {"->": "long_knot_name.0.g-0"},
                        {"->": "$r", "var": true},
                        null
                    ],
                    "#n": "loop"
                }
            ],
            {
                "g-0": ["done", null]
            }
        ]);

        compact_container_paths(
            &mut container,
            &RuntimePath::absolute("long_knot_name.0"),
        );

        assert_eq!(
            "long_knot_name.0.loop.0.$r1",
            container[0][0][0]["^->"].as_str().unwrap()
        );
        assert_eq!(
            ".^.^.c-0",
            container[0][0][1]["*"].as_str().unwrap()
        );
        assert_eq!(
            ".^.^.^.g-0",
            container[0][1]["c-0"][0]["->"].as_str().unwrap()
        );
        assert_eq!(
            "$r",
            container[0][1]["c-0"][1]["->"].as_str().unwrap()
        );
    }
}
