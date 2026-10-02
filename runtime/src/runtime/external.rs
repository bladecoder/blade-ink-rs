//! External functions and output handling.

use super::*;

impl Runtime {
    pub(crate) fn bind_external_function<F>(
        &mut self,
        name: &str,
        function: F,
        lookahead_safe: bool,
    ) -> Result<(), StoryError>
    where
        F: ExternalFunction + 'static,
    {
        let mut externals = self.externals.borrow_mut();
        if externals.contains_key(name) {
            return Err(StoryError::BadArgument(format!(
                "Function '{name}' has already been bound."
            )));
        }
        externals.insert(
            name.to_owned(),
            BoundExternal {
                function: Box::new(function),
                lookahead_safe,
            },
        );
        Ok(())
    }

    pub(crate) fn set_allow_external_fallbacks(&mut self, allowed: bool) {
        self.allow_external_fallbacks = allowed;
    }

    pub(crate) fn unbind_external_function(&mut self, name: &str) -> Result<(), StoryError> {
        if self.externals.borrow_mut().remove(name).is_none() {
            return Err(StoryError::BadArgument(format!(
                "Function '{name}' has not been bound."
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_external_bindings(&self) -> Result<(), StoryError> {
        let bound = self.externals.borrow();
        let mut missing = crate::compat::collections::HashSet::new();
        for index in 0..self.data.node_count() {
            let Some(node_id) = u32::try_from(index).ok().map(NodeId) else {
                break;
            };
            let Some(node) = self.data.node_view(node_id) else {
                continue;
            };
            let NodeKindView::Divert { path, external, .. } = node.kind else {
                continue;
            };
            if !external {
                continue;
            }
            let Some(path) = path else {
                continue;
            };
            let name = path.to_runtime().to_string();
            let has_fallback = self.allow_external_fallbacks
                && self
                    .data
                    .find_named_child(self.data.root(), &name)
                    .is_some();
            if !bound.contains_key(&name) && !has_fallback {
                missing.insert(name);
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        let count = missing.len();
        let mut names: Vec<_> = missing.into_iter().collect();
        names.sort();
        Err(StoryError::InvalidStoryState(format!(
            "ERROR: Missing function binding for external{}: '{}' {}",
            if count > 1 { "s" } else { "" },
            names.join(", "),
            if self.allow_external_fallbacks {
                ", and no fallback ink function found."
            } else {
                " (ink fallbacks disabled)"
            }
        )))
    }

    pub(super) fn call_external(&mut self, name: &str, count: usize) -> Result<(), StoryError> {
        if self.lookahead_active
            && self
                .externals
                .borrow()
                .get(name)
                .is_some_and(|def| !def.lookahead_safe)
        {
            self.lookahead_unsafe_external = true;
            return Ok(());
        }
        if !self.externals.borrow().contains_key(name) {
            if self.allow_external_fallbacks {
                if let Some(target) = self
                    .data
                    .named_child(self.data.root(), name)
                    .and_then(|id| self.data.container_id(id))
                {
                    self.callstack.borrow_mut().push(
                        PushPopType::Function,
                        self.evaluation_stack.len(),
                        self.string_stack.last().unwrap_or(&self.output).len() as i32,
                    );
                    self.diverted = ContentPointer::start_of(target);
                    return Ok(());
                }
                return Err(StoryError::InvalidStoryState(format!(
                    "Trying to call EXTERNAL function '{name}' which has not been bound, and fallback ink function could not be found."
                )));
            }
            return Err(StoryError::InvalidStoryState(format!(
                "Trying to call EXTERNAL function '{name}' which has not been bound (and ink fallbacks disabled)."
            )));
        }
        let mut args = Vec::with_capacity(count);
        for _ in 0..count {
            let popped = self.pop_value()?;
            let value = popped.as_any().downcast_ref::<Value>().ok_or_else(|| {
                StoryError::InvalidStoryState(format!(
                    "Trying to call EXTERNAL function '{name}' with arguments which are not values."
                ))
            })?;
            args.push(value.value.clone());
        }
        args.reverse();
        let result: ExternalFunctionResult = self
            .externals
            .borrow_mut()
            .get_mut(name)
            .unwrap()
            .function
            .call(name, &args);
        let result = result.map_err(|error| StoryError::ExternalFunctionFailed {
            function_name: name.to_owned(),
            error,
        })?;
        match result {
            Some(value) => self.push_evaluation(Rc::new(Value::new_value_type(value))),
            None => self.evaluation_stack.push(Rc::new(Void::new())),
        }
        Ok(())
    }

    pub(super) fn push_int(&mut self, value: i32) {
        self.evaluation_stack.push(Rc::new(Value::new(value)));
    }

    pub(super) fn push_evaluation(&mut self, value: Rc<dyn RTObject>) {
        if let Some(list) = Value::get_value::<&InkList>(value.as_ref()) {
            let names = list.get_origin_names();
            let mut origins = list.origins.borrow_mut();
            origins.clear();
            for name in names {
                if let Some(definition) = self.data.list_definition_index(&name) {
                    let items: HashMap<_, _> = (0..self
                        .data
                        .list_definition_item_count(definition)
                        .unwrap_or(0))
                        .filter_map(|index| self.data.list_definition_item_at(definition, index))
                        .map(|(item, value)| (item.to_owned(), value))
                        .collect();
                    origins.push(ListDefinition::new(name, items));
                }
            }
        }
        self.evaluation_stack.push(value);
    }

    pub(super) fn append_text(&mut self, text: &str) {
        if let Some(tag) = self.tag_stack.last_mut() {
            tag.push_str(text);
            return;
        }
        if self.string_stack.is_empty()
            && text == "\n"
            && (self.output.is_empty() || self.output.ends_with('\n'))
        {
            return;
        }
        if self.glue_active {
            if text == "\n" {
                return;
            }
            if text.chars().any(|character| !character.is_whitespace()) {
                self.glue_active = false;
            }
        }
        let leading_function_whitespace = {
            let stack = self.callstack.borrow();
            let element = stack.current_element();
            element.push_pop_type == PushPopType::Function && !element.function_output_started
        };
        if leading_function_whitespace {
            if text.chars().all(char::is_whitespace) {
                return;
            }
            self.callstack
                .borrow_mut()
                .current_element_mut()
                .function_output_started = true;
        }
        self.string_stack
            .last_mut()
            .unwrap_or(&mut self.output)
            .push_str(text);
    }

    pub(super) fn trim_function_output(&mut self) {
        let (start, started) = {
            let stack = self.callstack.borrow();
            let element = stack.current_element();
            (
                element.function_start_in_output_stream.max(0) as usize,
                element.function_output_started,
            )
        };
        if started {
            let stream = self.string_stack.last_mut().unwrap_or(&mut self.output);
            while stream.len() > start && stream.ends_with(char::is_whitespace) {
                stream.pop();
            }
        }
    }

    pub(super) fn truthy(value: &dyn RTObject) -> Result<bool, StoryError> {
        let Some(value) = value.as_any().downcast_ref::<Value>() else {
            return Err(StoryError::InvalidStoryState(
                "condition expected a value".to_owned(),
            ));
        };
        match &value.value {
            ValueType::Bool(value) => Ok(*value),
            ValueType::Int(value) => Ok(*value != 0),
            ValueType::Float(value) => Ok(*value != 0.0),
            ValueType::String(StringValue { string, .. }) => Ok(!string.is_empty()),
            ValueType::List(value) => Ok(!value.items.is_empty()),
            ValueType::DivertTarget(_) | ValueType::VariablePointer(_) => Err(
                StoryError::InvalidStoryState("cannot use a target as a condition".to_owned()),
            ),
        }
    }
}
