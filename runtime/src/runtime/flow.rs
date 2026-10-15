//! Flows, variables, lists and function calls.

use super::*;

impl Runtime {
    pub(crate) fn switch_flow(&mut self, name: &str) {
        if name == self.current_flow_name {
            return;
        }
        let root = self.data.container_id(self.data.root()).unwrap();
        let next = self
            .named_flows
            .remove(name)
            .unwrap_or_else(|| RuntimeFlow::new(root));
        let previous = RuntimeFlow {
            callstack: core::mem::replace(&mut self.callstack, next.callstack),
            output: core::mem::replace(&mut self.output, next.output),
            choices: core::mem::replace(&mut self.choices, next.choices),
            current_tags: core::mem::replace(&mut self.current_tags, next.current_tags),
        };
        self.variables.set_callstack(self.callstack.clone());
        let previous_name = core::mem::replace(&mut self.current_flow_name, name.to_owned());
        self.named_flows.insert(previous_name, previous);
    }

    pub(crate) fn remove_flow(&mut self, name: &str) -> Result<(), StoryError> {
        if name == "DEFAULT_FLOW" {
            return Err(StoryError::BadArgument(
                "Cannot destroy default flow".to_owned(),
            ));
        }
        if name == self.current_flow_name {
            self.switch_to_default_flow();
        }
        self.named_flows.remove(name);
        Ok(())
    }

    pub(crate) fn switch_to_default_flow(&mut self) {
        self.switch_flow("DEFAULT_FLOW");
    }

    pub(crate) fn choose_path_string(
        &mut self,
        path: &str,
        increment_turn: bool,
    ) -> Result<(), StoryError> {
        self.choose_path(&Path::new_with_components_string(Some(path)))?;
        self.choices.clear();
        if increment_turn {
            self.current_turn_index += 1;
        }
        Ok(())
    }

    pub(crate) fn choose_path_string_with_arguments(
        &mut self,
        path: &str,
        reset_call_stack: bool,
        args: Option<&Vec<ValueType>>,
    ) -> Result<(), StoryError> {
        if reset_call_stack {
            self.callstack.borrow_mut().reset();
            self.choices.clear();
        } else if self.callstack.borrow().current_element().push_pop_type == PushPopType::Function {
            return Err(StoryError::InvalidStoryState(format!(
                "Story was running a function when you called ChoosePathString({path})"
            )));
        }
        if let Some(args) = args {
            for value in args {
                if matches!(
                    value,
                    ValueType::DivertTarget(_) | ValueType::VariablePointer(_)
                ) {
                    return Err(StoryError::InvalidStoryState(
                        "ink arguments when calling EvaluateFunction / ChoosePathStringWithParameters must be int, float, string, bool or InkList.".to_owned()
                    ));
                }
                self.push_evaluation(Rc::new(Value::new_value_type(value.clone())));
            }
        }
        self.choose_path_string(path, true)
    }

    pub(crate) fn get_variable(&self, name: &str) -> Option<ValueType> {
        self.variables.get(name)
    }

    pub(crate) fn get_global_variables(&self) -> Vec<String> {
        self.variables.get_global_variables()
    }

    pub(crate) fn global_variable_exists(&self, name: &str) -> bool {
        self.variables.global_variable_exists_with_name(name)
    }

    pub(crate) fn list_from_origin(&self, origin_name: &str) -> Result<InkList, StoryError> {
        let definition = self
            .data
            .list_definition_index(origin_name)
            .ok_or_else(|| {
                StoryError::BadArgument(format!("List origin '{origin_name}' does not exist."))
            })?;
        let list = InkList::new();
        list.set_initial_origin_names(vec![origin_name.to_owned()]);
        let values: HashMap<_, _> = (0..self
            .data
            .list_definition_item_count(definition)
            .unwrap_or(0))
            .filter_map(|index| self.data.list_definition_item_at(definition, index))
            .map(|(name, value)| (name.to_owned(), value))
            .collect();
        list.origins
            .borrow_mut()
            .push(ListDefinition::new(origin_name.to_owned(), values));
        Ok(list)
    }

    pub(crate) fn list_from_item(&self, full_item_name: &str) -> Result<InkList, StoryError> {
        let item = InkListItem::from_full_name(full_item_name);
        let origin_name = item.get_origin_name().ok_or_else(|| {
            StoryError::BadArgument(format!(
                "List item '{full_item_name}' must use the 'origin.item' format."
            ))
        })?;
        let mut list = self.list_from_origin(origin_name)?;
        let value = self
            .data
            .list_item(full_item_name)
            .map(|(_, value)| value)
            .ok_or_else(|| {
                StoryError::BadArgument(format!("List item '{full_item_name}' does not exist."))
            })?;
        list.items.insert(item, value);
        Ok(list)
    }

    pub(crate) fn tags_for_content_at_path(&self, path: &str) -> Result<Vec<String>, StoryError> {
        let path = Path::new_with_components_string(Some(path));
        let mut container = self
            .data
            .resolve_path(self.data.root(), &path)
            .ok_or_else(|| {
                StoryError::BadArgument(format!("story path '{path}' does not resolve"))
            })?;
        if self.data.container_id(container).is_none() {
            return Err(StoryError::BadArgument(format!(
                "story path '{path}' is not a container"
            )));
        }
        while let Some(first) = self.data.child_at(container, 0) {
            if self.data.container_id(first).is_some() {
                container = first;
            } else {
                break;
            }
        }
        let mut tags = Vec::new();
        let mut in_tag = false;
        for index in 0..self.data.child_count(container).unwrap_or(0) {
            let id = self.data.child_at(container, index).unwrap();
            match self.data.node_view(id).unwrap().kind {
                NodeKindView::ControlCommand(CommandType::BeginTag) => in_tag = true,
                NodeKindView::ControlCommand(CommandType::EndTag) => in_tag = false,
                NodeKindView::Value(ValueView::String(text)) if in_tag => {
                    tags.push(text.to_owned())
                }
                NodeKindView::Value(_) if in_tag => return Err(StoryError::InvalidStoryState(
                    "Tag contained non-text content. Only plain text is allowed when using globalTags or TagsAtContentPath. If you want to evaluate dynamic content, you need to use story.Continue()".to_owned()
                )),
                _ if !in_tag => break,
                _ => {}
            }
        }
        Ok(tags)
    }

    pub(crate) fn evaluate_function(
        &mut self,
        name: &str,
        args: Option<&Vec<ValueType>>,
        text_output: &mut String,
    ) -> Result<Option<ValueType>, StoryError> {
        if name.trim().is_empty() {
            return Err(StoryError::InvalidStoryState(
                "Function is empty or white space.".to_owned(),
            ));
        }
        let target = self
            .data
            .named_child(self.data.root(), name)
            .and_then(|id| self.data.container_id(id))
            .ok_or_else(|| StoryError::BadArgument(format!("Function doesn't exist: '{name}'")))?;
        let output_before = core::mem::take(&mut self.output);
        let tags_before = core::mem::take(&mut self.current_tags);
        self.callstack.borrow_mut().push(
            PushPopType::FunctionEvaluationFromGame,
            self.evaluation_stack.len(),
            0,
        );
        self.callstack
            .borrow_mut()
            .set_current_pointer(ContentPointer::start_of(target));
        if let Some(args) = args {
            for value in args {
                if matches!(
                    value,
                    ValueType::DivertTarget(_) | ValueType::VariablePointer(_)
                ) {
                    return Err(StoryError::InvalidStoryState(
                        "ink arguments when calling EvaluateFunction / ChoosePathStringWithParameters must be int, float, string, bool or InkList.".to_owned()
                    ));
                }
                self.push_evaluation(Rc::new(Value::new_value_type(value.clone())));
            }
        }
        let mut changes = HashMap::new();
        while self.can_continue() {
            text_output.push_str(&self.cont()?);
            changes.extend(self.take_changed_variables());
        }
        self.changed_variables = changes;
        self.output = output_before;
        self.current_tags = tags_before;
        let height = self
            .callstack
            .borrow()
            .current_element()
            .evaluation_stack_height_when_pushed;
        let result = self.evaluation_stack.pop();
        self.evaluation_stack.truncate(height);
        self.callstack
            .borrow_mut()
            .pop(Some(PushPopType::FunctionEvaluationFromGame))?;
        Ok(result.and_then(|value| {
            value
                .as_any()
                .downcast_ref::<Value>()
                .map(|value| match &value.value {
                    ValueType::DivertTarget(path) => ValueType::new::<&str>(&path.to_string()),
                    other => other.clone(),
                })
        }))
    }

    pub(crate) fn set_variable(&mut self, name: &str, value: &ValueType) -> Result<(), StoryError> {
        if self.variables.set(name, value.clone())? {
            self.changed_variables
                .insert(name.to_owned(), value.clone());
        }
        Ok(())
    }

    pub(crate) fn take_changed_variables(&mut self) -> HashMap<String, ValueType> {
        core::mem::take(&mut self.changed_variables)
    }
}
