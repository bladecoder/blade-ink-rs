#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StringParserStateElement {
    character_index: usize,
    character_in_line_index: usize,
    line_index: usize,
    reported_error_in_scope: bool,
    unique_id: usize,
    custom_flags: u32,
}

impl StringParserStateElement {
    fn copy_from(&mut self, from: &Self, unique_id: usize) {
        self.unique_id = unique_id;
        self.character_index = from.character_index;
        self.character_in_line_index = from.character_in_line_index;
        self.line_index = from.line_index;
        self.custom_flags = from.custom_flags;
        self.reported_error_in_scope = false;
    }

    fn squash_from(&mut self, from: &Self) {
        self.character_index = from.character_index;
        self.character_in_line_index = from.character_in_line_index;
        self.line_index = from.line_index;
        self.reported_error_in_scope = from.reported_error_in_scope;
        self.custom_flags = from.custom_flags;
    }

    pub fn character_index(&self) -> usize {
        self.character_index
    }

    pub fn line_index(&self) -> usize {
        self.line_index
    }

    pub fn character_in_line_index(&self) -> usize {
        self.character_in_line_index
    }
}

#[derive(Debug, Clone)]
pub struct StringParserState {
    stack: Vec<StringParserStateElement>,
    next_unique_id: usize,
}

impl Default for StringParserState {
    fn default() -> Self {
        Self::new()
    }
}

impl StringParserState {
    pub fn new() -> Self {
        Self {
            stack: vec![StringParserStateElement::default()],
            next_unique_id: 1,
        }
    }

    pub fn line_index(&self) -> usize {
        self.current_element().line_index
    }

    pub fn set_line_index(&mut self, value: usize) {
        self.current_element_mut().line_index = value;
    }

    pub fn character_index(&self) -> usize {
        self.current_element().character_index
    }

    pub fn set_character_index(&mut self, value: usize) {
        self.current_element_mut().character_index = value;
    }

    pub fn character_in_line_index(&self) -> usize {
        self.current_element().character_in_line_index
    }

    pub fn set_character_in_line_index(&mut self, value: usize) {
        self.current_element_mut().character_in_line_index = value;
    }

    pub fn custom_flags(&self) -> u32 {
        self.current_element().custom_flags
    }

    pub fn set_custom_flags(&mut self, value: u32) {
        self.current_element_mut().custom_flags = value;
    }

    pub fn error_reported_already_in_scope(&self) -> bool {
        self.current_element().reported_error_in_scope
    }

    pub fn stack_height(&self) -> usize {
        self.stack.len()
    }

    pub fn push(&mut self) -> usize {
        assert!(
            self.stack.len() < 200,
            "Stack overflow in parser state"
        );
        let previous = self.current_element().clone();
        let mut new_element = StringParserStateElement::default();
        new_element.copy_from(&previous, self.next_unique_id);
        self.next_unique_id += 1;
        let unique_id = new_element.unique_id;
        self.stack.push(new_element);
        unique_id
    }

    pub fn pop(&mut self, expected_rule_id: usize) {
        assert!(
            self.stack.len() > 1,
            "Attempting to remove final stack element is illegal! Mismatched Begin/Succeed/Fail?"
        );
        assert_eq!(
            self.current_element().unique_id,
            expected_rule_id,
            "Mismatched rule IDs - do you have mismatched Begin/Succeed/Fail?"
        );
        self.stack.pop();
    }

    pub fn peek(&self, expected_rule_id: usize) -> &StringParserStateElement {
        assert_eq!(
            self.current_element().unique_id,
            expected_rule_id,
            "Mismatched rule IDs - do you have mismatched Begin/Succeed/Fail?"
        );
        self.current_element()
    }

    pub fn peek_penultimate(&self) -> Option<&StringParserStateElement> {
        if self.stack.len() >= 2 {
            self.stack.get(self.stack.len() - 2)
        } else {
            None
        }
    }

    pub fn squash(&mut self) {
        assert!(
            self.stack.len() >= 2,
            "Attempting to remove final stack element is illegal! Mismatched Begin/Succeed/Fail?"
        );
        let last = self.stack.pop().expect("stack length checked above");
        self.current_element_mut().squash_from(&last);
    }

    pub fn note_error_reported(&mut self) {
        for element in &mut self.stack {
            element.reported_error_in_scope = true;
        }
    }

    fn current_element(&self) -> &StringParserStateElement {
        self.stack.last().expect("parser state stack must never be empty")
    }

    fn current_element_mut(&mut self) -> &mut StringParserStateElement {
        self.stack
            .last_mut()
            .expect("parser state stack must never be empty")
    }
}

#[cfg(test)]
mod tests {
    use super::StringParserState;

    #[test]
    fn push_pop_roundtrip() {
        let mut state = StringParserState::new();
        state.set_character_index(3);
        let rule = state.push();
        state.set_character_index(8);
        state.pop(rule);
        assert_eq!(3, state.character_index());
    }

    #[test]
    fn squash_keeps_latest_cursor() {
        let mut state = StringParserState::new();
        let _rule = state.push();
        state.set_character_index(9);
        state.set_line_index(2);
        state.squash();
        assert_eq!(9, state.character_index());
        assert_eq!(2, state.line_index());
        assert_eq!(1, state.stack_height());
    }
}
