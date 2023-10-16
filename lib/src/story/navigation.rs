use crate::{
    container::Container,
    object::{Object, RTObject},
    path::Path,
    pointer::{Pointer, self},
    push_pop::PushPopType,
    search_result::SearchResult,
    story::Story,
    story_error::StoryError,
    value_type::ValueType,
};
use std::rc::Rc;

/// # Navigation
/// Methods to access specific sections of the story.
impl Story {
    pub(crate) fn get_main_content_container(&self) -> Rc<Container> {
        match self.temporary_evaluation_container.as_ref() {
            Some(c) => c.clone(),
            None => self.main_content_container.clone(),
        }
    }

    /// Change the current position of the story to the given path. From
    /// here you can call [`cont()`](Story::cont) to evaluate the
    /// next line.
    ///
    /// The path string is a dot-separated path as used internally by the
    /// engine. These examples should work:
    ///
    /// ```ink
    ///    myKnot
    ///    myKnot.myStitch
    /// ```
    ///
    /// Note however that this won't necessarily work:
    ///
    /// ```ink
    ///    myKnot.myStitch.myLabelledChoice
    /// ```
    ///
    /// ...because of the way that content is nested within a weave
    /// structure.
    ///
    /// Usually you would reset the callstack beforehand, which means that
    /// any tunnels, threads or functions you were in at the time of
    /// calling will be discarded. This is different from the
    /// behaviour of
    /// [`choose_choice_index`](Story::choose_choice_index), which
    /// will always keep the callstack, since the choices are known to come
    /// from a correct state, and their source thread is known.
    ///
    /// You have the option of passing `false` to the `reset_callstack`
    /// parameter if you don't want this behaviour, leaving any active
    /// threads, tunnels or function calls intact.
    ///
    /// Not reseting the call stack is potentially dangerous! If you're in
    /// the middle of a tunnel, it'll redirect only the inner-most
    /// tunnel, meaning that when you tunnel-return using `->->`,
    /// it'll return to where you were before. This may be what you
    /// want though. However, if you're in the middle of a function,
    /// `choose_path_string` will throw an error.
    pub fn choose_path_string(
        &mut self,
        path: &str,
        reset_call_stack: bool,
        args: Option<&Vec<ValueType>>,
    ) -> Result<(), StoryError> {
        self.if_async_we_cant("call ChoosePathString right now")?;

        if reset_call_stack {
            self.reset_callstack()?;
        } else {
            // ChoosePathString is potentially dangerous since you can call it when the
            // stack is
            // pretty much in any state. Let's catch one of the worst offenders.
            if self
                .get_state()
                .get_callstack()
                .borrow()
                .get_current_element()
                .push_pop_type
                == PushPopType::Function
            {
                let mut func_detail = "".to_owned();
                let container = self
                    .get_state()
                    .get_callstack()
                    .borrow()
                    .get_current_element()
                    .current_pointer
                    .container
                    .clone();
                if let Some(container) = container {
                    func_detail = format!("({})", Object::get_path(container.as_ref()));
                }

                return Err(StoryError::InvalidStoryState(format!("Story was running a function {func_detail} when you called ChoosePathString({}) - this is almost certainly not what you want! Full stack trace: \n{}", path, self.get_state().get_callstack().borrow().get_callstack_trace())));
            }
        }

        self.get_state_mut()
            .pass_arguments_to_evaluation_stack(args)?;
        self.choose_path(&Path::new_with_components_string(Some(path)), true)?;

        Ok(())
    }

    /// Evaluates a function defined in ink, and gathers the (possibly
    /// multi-line) text the function produces while executing. This output
    /// text is any text written as normal content within the function,
    /// as opposed to the ink function's return value, which is specified by
    /// `~ return` in the ink.
    pub fn evaluate_function(
        &mut self,
        func_name: &str,
        args: Option<&Vec<ValueType>>,
        text_output: &mut String,
    ) -> Result<Option<ValueType>, StoryError> {
        self.if_async_we_cant("evaluate a function")?;

        if func_name.trim().is_empty() {
            return Err(StoryError::InvalidStoryState(
                "Function is empty or white space.".to_owned(),
            ));
        }

        // Get the content that we need to run
        let func_container = self.knot_container_with_name(func_name);
        if func_container.is_none() {
            let mut e = "Function doesn't exist: '".to_owned();
            e.push_str(func_name);
            e.push('\'');

            return Err(StoryError::BadArgument(e));
        }

        // Snapshot the output stream
        let output_stream_before = self.get_state().get_output_stream().clone();
        self.get_state_mut().reset_output(None);

        // State will temporarily replace the callstack in order to evaluate
        self.get_state_mut()
            .start_function_evaluation_from_game(func_container.unwrap(), args)?;

        // Evaluate the function, and collect the string output
        while self.can_continue() {
            let text = self.cont()?;

            text_output.push_str(&text);
        }

        // Restore the output stream in case this was called
        // during main story evaluation.
        self.get_state_mut()
            .reset_output(Some(output_stream_before));

        // Finish evaluation, and see whether anything was produced
        self.get_state_mut()
            .complete_function_evaluation_from_game()
    }

    pub(crate) fn visit_changed_containers_due_to_divert(&mut self) {
        let previous_pointer = self.get_state().get_previous_pointer();
        let pointer = self.get_state().get_current_pointer();

        // Unless we're pointing *directly* at a piece of content, we don't do counting
        // here. Otherwise, the main stepping function will do the counting.
        if pointer.is_null() || pointer.index == -1 {
            return;
        }

        // First, find the previously open set of containers
        self.prev_containers.clear();

        if !previous_pointer.is_null() {
            let mut prev_ancestor = None;

            if let Some(container) = previous_pointer
                .resolve()
                .and_then(|res| res.into_any().downcast::<Container>().ok())
            {
                prev_ancestor = Some(container);
            } else if previous_pointer.container.is_some() {
                prev_ancestor = previous_pointer.container.clone();
            }

            while let Some(prev_anc) = prev_ancestor {
                self.prev_containers.push(prev_anc.clone());
                prev_ancestor = prev_anc.get_object().get_parent();
            }
        }

        // If the new Object is a container itself, it will be visited
        // automatically at the next actual content step. However, we need to walk up
        // the new ancestry to see if there are more new containers
        let current_child_of_container = pointer.resolve();

        if current_child_of_container.is_none() {
            return;
        }

        let mut current_child_of_container = current_child_of_container.unwrap();

        let mut current_container_ancestor =
            current_child_of_container.get_object().get_parent();

        let mut all_children_entered_at_start = true;

        while let Some(current_container) = current_container_ancestor {
            if !self
                .prev_containers
                .iter()
                .any(|e| Rc::ptr_eq(e, &current_container))
                || current_container.counting_at_start_only
            {
                // Check whether this ancestor container is being entered at the start,
                // by checking whether the child Object is the first.
                let entering_at_start = current_container
                    .content
                    .first()
                    .map(|first_child| {
                        Rc::ptr_eq(first_child, &current_child_of_container)
                            && all_children_entered_at_start
                    })
                    .unwrap_or(false);

                // Don't count it as entering at start if we're entering randomly somewhere
                // within a container B that happens to be nested at index 0 of
                // container A. It only counts
                // if we're diverting directly to the first leaf node.
                if !entering_at_start {
                    all_children_entered_at_start = false;
                }

                // Mark a visit to this container
                self.visit_container(&current_container, entering_at_start);

                current_child_of_container = current_container.clone();
                current_container_ancestor = current_container.get_object().get_parent();
            } else {
                break;
            }
        }
    }

    pub(crate) fn pointer_at_path(
        main_content_container: &Rc<Container>,
        path: &Path,
    ) -> Result<Pointer, StoryError> {
        if path.len() == 0 {
            return Ok(pointer::NULL.clone());
        }

        let mut p = Pointer::default();
        let mut path_length_to_use = path.len() as i32;

        let result: SearchResult = if path.get_last_component().unwrap().is_index() {
            path_length_to_use -= 1;
            let result = SearchResult::from_search_result(
                &main_content_container.content_at_path(path, 0, path_length_to_use),
            );
            p.container = result.container();
            p.index = path.get_last_component().unwrap().index.unwrap() as i32;

            result
        } else {
            let result = SearchResult::from_search_result(
                &main_content_container.content_at_path(path, 0, -1),
            );
            p.container = result.container();
            p.index = -1;

            result
        };

        let main_container: Rc<dyn RTObject> = main_content_container.clone();

        if Rc::ptr_eq(&result.obj, &main_container) && path_length_to_use > 0 {
            return Err(StoryError::InvalidStoryState(format!(
                "Failed to find content at path '{}', and no approximation of it was possible.",
                path
            )));
        } else if result.approximate {
            // TODO
            // self.add_error(&format!("Failed to find content at path '{}',
            // so it was approximated to: '{}'.", path
            // , result.obj.unwrap().get_path()), true);
        }

        Ok(p)
    }

    pub(crate) fn knot_container_with_name(&self, name: &str) -> Option<Rc<Container>> {
        let named_container = self.main_content_container.named_content.get(name);

        named_container.cloned()
    }

    pub(crate) fn content_at_path(&self, path: &Path) -> SearchResult {
        self.main_content_container.content_at_path(path, 0, -1)
    }

    /// Gets the visit/read count of a particular `Container` at the given
    /// path. For a knot or stitch, that path string will be in the
    /// form:
    ///
    /// ```ink
    ///     knot
    ///     knot.stitch
    /// ```
    pub fn get_visit_count_at_path_string(&self, path_string: &str) -> Result<i32, StoryError> {
        self.get_state().visit_count_at_path_string(path_string)
    }
}
