use crate::{
    choice::Choice, choice_point::ChoicePoint, object::Object, path::Path, story::Story,
    story_error::StoryError, tag::Tag, value::Value,
};
use std::rc::Rc;
/// # Choices
/// Methods to get and select choices.
impl Story {
    /// Chooses the [`Choice`](crate::choice::Choice) from the
    /// `currentChoices` list with the given index. Internally, this
    /// sets the current content path to what the
    /// [`Choice`](crate::choice::Choice) points to, ready
    /// to continue story evaluation.
    pub fn choose_choice_index(&mut self, choice_index: usize) -> Result<(), StoryError> {
        let choices = self.get_current_choices();
        if choice_index >= choices.len() {
            return Err(StoryError::BadArgument("choice out of range".to_owned()));
        }

        // Replace callstack with the one from the thread at the choosing point,
        // so that we can jump into the right place in the flow.
        // This is important in case the flow was forked by a new thread, which
        // can create multiple leading edges for the story, each of
        // which has its own context.
        let choice_to_choose = choices.get(choice_index).unwrap();
        self.get_state()
            .get_callstack()
            .borrow_mut()
            .set_current_thread(choice_to_choose.get_thread_at_generation().unwrap());

        self.choose_path(&choice_to_choose.target_path, true)?;

        Ok(())
    }

    pub(crate) fn choose_path(
        &mut self,
        p: &Path,
        incrementing_turn_index: bool,
    ) -> Result<(), StoryError> {
        self.get_state_mut()
            .set_chosen_path(p, incrementing_turn_index)?;

        // Take a note of newly visited containers for read counts etc
        self.visit_changed_containers_due_to_divert();

        Ok(())
    }

    pub(crate) fn process_choice(
        &mut self,
        choice_point: &Rc<ChoicePoint>,
    ) -> Result<Option<Rc<Choice>>, StoryError> {
        let mut show_choice = true;

        // Don't create choice if choice point doesn't pass conditional
        if choice_point.has_condition() {
            let condition_value = self.get_state_mut().pop_evaluation_stack();
            if !self.is_truthy(condition_value)? {
                show_choice = false;
            }
        }

        let mut start_text = String::new();
        let mut choice_only_text = String::new();
        let mut tags: Vec<String> = Vec::with_capacity(0);

        if choice_point.has_choice_only_content() {
            choice_only_text = self.pop_choice_string_and_tags(&mut tags);
        }

        if choice_point.has_start_content() {
            start_text = self.pop_choice_string_and_tags(&mut tags);
        }

        // Don't create choice if player has already read this content
        if choice_point.once_only() {
            let visit_count = self
                .get_state_mut()
                .visit_count_for_container(choice_point.get_choice_target().as_ref().unwrap());
            if visit_count > 0 {
                show_choice = false;
            }
        }

        // We go through the full process of creating the choice above so
        // that we consume the content for it, since otherwise it'll
        // be shown on the output stream.
        if !show_choice {
            return Ok(None);
        }

        start_text.push_str(&choice_only_text);

        let choice = Rc::new(Choice::new(
            choice_point.get_path_on_choice(),
            Object::get_path(choice_point.as_ref()).to_string(),
            choice_point.is_invisible_default(),
            tags,
            self.get_state().get_callstack().borrow_mut().fork_thread(),
            start_text.trim().to_string(),
        ));

        Ok(Some(choice))
    }

    pub(crate) fn try_follow_default_invisible_choice(&mut self) -> Result<(), StoryError> {
        let all_choices = match self.get_state().get_current_choices() {
            Some(c) => c,
            None => return Ok(()),
        };

        // Is a default invisible choice the ONLY choice?
        // var invisibleChoices = allChoices.Where (c =>
        // c.choicePoint.isInvisibleDefault).ToList();
        let mut invisible_choices: Vec<Rc<Choice>> = Vec::new();
        for c in all_choices {
            if c.is_invisible_default {
                invisible_choices.push(c.clone());
            }
        }

        if invisible_choices.is_empty() || all_choices.len() > invisible_choices.len() {
            return Ok(());
        }

        let choice = &invisible_choices[0];

        // Invisible choice may have been generated on a different thread,
        // in which case we need to restore it before we continue
        self.get_state()
            .get_callstack()
            .as_ref()
            .borrow_mut()
            .set_current_thread(choice.get_thread_at_generation().unwrap().clone());

        // If there's a chance that this state will be rolled back to before
        // the invisible choice then make sure that the choice thread is
        // left intact, and it isn't re-entered in an old state.
        if self.state_snapshot_at_last_new_line.is_some() {
            let fork_thread = self
                .get_state()
                .get_callstack()
                .as_ref()
                .borrow_mut()
                .fork_thread();
            self.get_state()
                .get_callstack()
                .as_ref()
                .borrow_mut()
                .set_current_thread(fork_thread);
        }

        self.choose_path(&choice.target_path, false)
    }

    fn pop_choice_string_and_tags(&mut self, tags: &mut Vec<String>) -> String {
        let obj = self.get_state_mut().pop_evaluation_stack();
        let choice_only_str_val = Value::get_string_value(obj.as_ref()).unwrap();

        while !self.get_state().evaluation_stack.is_empty()
            && self
                .get_state()
                .peek_evaluation_stack()
                .unwrap()
                .as_any()
                .is::<Tag>()
        {
            let tag = self
                .get_state_mut()
                .pop_evaluation_stack()
                .into_any()
                .downcast::<Tag>()
                .unwrap();
            tags.insert(0, tag.get_text().clone()); // popped in reverse
                                                    // order
        }

        choice_only_str_val.string.to_string()
    }
}
