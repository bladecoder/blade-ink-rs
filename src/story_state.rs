#![allow(unused_variables, dead_code)]

use std::{rc::Rc, borrow::BorrowMut, cell::RefCell, collections::VecDeque};

use crate::{pointer::Pointer, callstack::CallStack, story::Story, flow::Flow, variables_state::VariablesState, choice::Choice, object::RTObject, value::{Value, ValueType}, glue::Glue, push_pop::PushPopType, control_command::{CommandType, ControlCommand}, container::Container};

pub const INK_SAVE_STATE_VERSION: u32 = 10;
pub const MIN_COMPATIBLE_LOAD_VERSION: u32 = 8;

static DEFAULT_FLOW_NAME: &str = "DEFAULT_FLOW";

pub struct StoryState {
    pub(crate) current_flow: Flow,
    pub(crate) did_safe_exit: bool,
    output_stream_text_dirty: bool,
    output_stream_tags_dirty: bool,
    variables_state: VariablesState,
    alive_flow_names_dirty: bool,
    evaluation_stack: Vec<Rc<dyn RTObject>>,
    main_content_container: Rc<Container>,
    current_errors: Vec<String>,
    current_warnings: Vec<String>,
    current_text: Option<String>,
}

impl StoryState {
    pub fn new(main_content_container: Rc<Container>) -> StoryState {
        let current_flow = Flow::new(DEFAULT_FLOW_NAME, main_content_container.clone());
        let callstack = current_flow.callstack.clone();

        let mut state = StoryState { 
            current_flow: current_flow, 
            did_safe_exit: false,
            output_stream_text_dirty: true,
            output_stream_tags_dirty: true,
            variables_state: VariablesState::new(callstack),
            alive_flow_names_dirty: true,
            evaluation_stack: Vec::new(),
            main_content_container: main_content_container,
            current_errors: Vec::with_capacity(0),
            current_warnings: Vec::with_capacity(0),
            current_text: None,
        };

        // TODO
        // visitCounts = new HashMap<>();
        // turnIndices = new HashMap<>();
        // currentTurnIndex = -1;

        // // Seed the shuffle random numbers
        // long timeSeed = System.currentTimeMillis();

        // storySeed = new Random(timeSeed).nextInt() % 100;
        // previousRandom = 0;

        state.go_to_start();

        state
    }

    pub fn can_continue(&self) -> bool {
        !self.get_current_pointer().is_null() && !self.has_error()
    }

    pub fn has_error(&self) -> bool {
        !self.current_errors.is_empty()
    }

    pub(crate) fn get_current_pointer(&self) -> Pointer {
        self.get_callstack().borrow().get_current_element().current_pointer.clone()
    }

    pub(crate) fn get_callstack(&self) -> &Rc<RefCell<CallStack>> {
        &self.current_flow.callstack
    }

    pub(crate) fn set_did_safe_exit(&mut self, did_safe_exit: bool) {
        self.did_safe_exit = did_safe_exit;
    }

    pub(crate) fn reset_output(&mut self, objs: Option<Vec<Rc<dyn RTObject>>>) {
        self.get_output_stream_mut().clear();
        if let Some(objs) = objs {
            for o in objs {
                self.get_output_stream_mut().push(o.clone());
            }
        }
        self.output_stream_dirty();
    }

    pub(crate) fn get_variables_state(&self) -> &VariablesState {
        &self.variables_state
    }

    pub(crate) fn get_variables_state_mut(&mut self) -> &mut VariablesState {
        &mut self.variables_state
    }

    pub(crate) fn get_generated_choices(&self) -> &Vec<Rc<Choice>> {
        &self.current_flow.current_choices
    }

    pub(crate) fn is_did_safe_exit(&self) -> bool {
        self.did_safe_exit
    }

    pub(crate) fn has_warning(&self) -> bool {
        !self.current_warnings.is_empty()
    }

    pub(crate) fn get_current_errors(&self) -> &Vec<String> {
        &self.current_errors
    }

    pub(crate) fn get_current_warnings(&self) -> &Vec<String> {
        &self.current_warnings
    }

    fn get_output_stream(&self) -> &Vec<Rc<(dyn RTObject)>> {
        &self.current_flow.output_stream
    }

    fn get_output_stream_mut(&mut self) -> &mut Vec<Rc<(dyn RTObject)>> {
        &mut self.current_flow.output_stream
    }

    fn output_stream_dirty(&mut self) {
        self.output_stream_text_dirty = true;
        self.output_stream_tags_dirty = true;
    }

    pub(crate) fn in_string_evaluation(&self) -> bool {
        for e in self.get_output_stream().iter().rev() {
            if let Some(cmd) = e.as_any().downcast_ref::<ControlCommand>() {
                if cmd.command_type == CommandType::BeginString {
                    return true;
                }
            }
        }
        false
    }

    pub fn get_current_text(&mut self) -> String {
        if self.output_stream_text_dirty {
            let mut sb = String::new();
            let mut in_tag = false;

            for outputObj in self.get_output_stream() {
                let text_content = match outputObj.as_ref().as_any().downcast_ref::<Value>() {
                    Some(v) => match &v.value {
                        ValueType::String(s) => Some(s),
                        _ => None,
                    },
                    None => None,
                };

                if !in_tag && text_content.is_some() {
                    sb.push_str(&text_content.unwrap().string);
                } else {
                    if let Some(controlCommand) = outputObj.as_ref().as_any().downcast_ref::<ControlCommand>() {
                        if controlCommand.command_type == CommandType::BeginTag {
                            in_tag = true;
                        } else if controlCommand.command_type == CommandType::EndTag {
                            in_tag = false;
                        }
                    }
                }
            }

            self.current_text = Some(StoryState::clean_output_whitespace(&sb));

            self.output_stream_tags_dirty = false;
        }

        self.current_text.as_ref().unwrap().to_string()
    }

    pub(crate) fn get_current_tags(&self) -> Vec<String> {
        todo!()
    }

    pub(crate) fn output_stream_ends_in_newline(&self) -> bool {
        if !self.get_output_stream().is_empty() {
            for e in self.get_output_stream().iter().rev() {
                if let Some(cmd) = e.as_any().downcast_ref::<ControlCommand>() {
                    break;
                }
    
                if let Some(val) = e.as_any().downcast_ref::<Value>() {
                    if let ValueType::String(text) = &val.value {
                        if text.is_newline {
                            return true;
                        } else if text.is_non_whitespace() {
                            break;
                        }
                    }
                }
            }
        }
    
        false
    }

    pub(crate) fn set_current_pointer(&self, pointer: Pointer) {
        self.get_callstack().as_ref().borrow_mut().get_current_element_mut().current_pointer = pointer;
    }

    pub(crate) fn get_in_expression_evaluation(&self) -> bool {
        self.get_callstack().borrow().get_current_element().in_expression_evaluation
    }

    pub(crate) fn push_evaluation_stack(&self, content_obj: Option<Rc<dyn RTObject>>) {
        todo!()
    }

    pub(crate) fn push_to_output_stream(&mut self, obj: Option<Rc<dyn RTObject>>) {
        let text = match &obj {
            Some(obj) => {
                let obj = obj.clone();
                match obj.into_any().downcast::<Value>() {
                    Ok(v) => match &v.value {
                        ValueType::String(s) => Some(s.clone()),
                        _ => None,
                    },
                    Err(_) => None,
                }
            },
            None => None,
        };

        if let Some(s) = text {
            let list_text = StoryState::try_splitting_head_tail_whitespace(&s.string);

            if let Some(list_text) = list_text {
                for text_obj in list_text {
                    self.push_to_output_stream_individual(Rc::new(text_obj));
                }
                self.output_stream_dirty();
                return;
            }
        }

        self.push_to_output_stream_individual(obj.unwrap());
    }

    pub(crate) fn increment_visit_count_for_container(&self, container: &crate::container::Container) {
        todo!()
    }

    pub(crate) fn record_turn_index_visit_to_container(&self, container: &crate::container::Container) {
        todo!()
    }

    fn try_splitting_head_tail_whitespace(text: &str) -> Option<Vec<Value>> {
        let mut head_first_newline_idx = -1;
        let mut head_last_newline_idx = -1;
        for (i, c) in text.chars().enumerate() {
            if c == '\n' {
                if head_first_newline_idx == -1 {
                    head_first_newline_idx = i as i32;
                }
                head_last_newline_idx = i as i32;
            } else if c == ' ' || c == '\t' {
                continue;
            } else {
                break;
            }
        }

        let mut tail_last_newline_idx = -1;
        let mut tail_first_newline_idx = -1;
        for (i, c) in text.chars().rev().enumerate() {
            let reversed_i = text.len() as i32 - i as i32 - 1;
            if c == '\n' {
                if tail_last_newline_idx == -1 {
                    tail_last_newline_idx = reversed_i;
                }
                tail_first_newline_idx = reversed_i;
            } else if c == ' ' || c == '\t' {
                continue;
            } else {
                break;
            }
        }

        if head_first_newline_idx == -1 && tail_last_newline_idx == -1 {
            return None;
        }

        let mut list_texts = Vec::new();
        let mut inner_str_start = 0;
        let mut inner_str_end = text.len();

        if head_first_newline_idx != -1 {
            if head_first_newline_idx > 0 {
                let leading_spaces = Value::new_string(&text[0..head_first_newline_idx as usize]);
                list_texts.push(leading_spaces);
            }
            list_texts.push(Value::new_string("\n"));
            inner_str_start = head_last_newline_idx + 1;
        }

        if tail_last_newline_idx != -1 {
            inner_str_end = tail_first_newline_idx as usize;
        }

        if inner_str_end > inner_str_start as usize {
            let inner_str_text = &text[inner_str_start as usize..inner_str_end];
            list_texts.push(Value::new_string(inner_str_text));
        }

        if tail_last_newline_idx != -1 && tail_first_newline_idx > head_last_newline_idx {
            list_texts.push(Value::new_string("\n"));
            if tail_last_newline_idx < text.len() as i32 - 1 {
                let num_spaces = (text.len() as i32 - tail_last_newline_idx) - 1;
                let trailing_spaces = Value::new_string(
                    &text[(tail_last_newline_idx + 1) as usize..(num_spaces + tail_last_newline_idx + 1) as usize],
                );
                list_texts.push(trailing_spaces);
            }
        }

        Some(list_texts)
    }

    fn push_to_output_stream_individual(&mut self, obj: Rc<dyn RTObject>) {
        let glue = obj.clone().into_any().downcast::<Glue>();
        let text = obj.clone().into_any().downcast::<Value>();
        let mut include_in_output = true;
    
        // New glue, so chomp away any whitespace from the end of the stream
        if let Ok(_) = glue {
            self.trim_newlines_from_output_stream();
            include_in_output = true;
        }

        // New text: do we really want to append it, if it's whitespace?
        // Two different reasons for whitespace to be thrown away:
        // - Function start/end trimming
        // - User-defined glue: <>
        // We also need to know when to stop trimming when there's non-whitespace.
        else if let Ok(text) = text {
            let mut function_trim_index = -1;
            let cs = self.get_callstack().borrow();
            let curr_el = cs.get_current_element();
            if curr_el.push_pop_type == PushPopType::Function {
                function_trim_index = curr_el.function_start_in_output_stream as i32;
            }
    
            let mut glue_trim_index = -1;
            for (i, o) in self.get_output_stream().iter().rev().enumerate() {
                if let Some(c) = o.as_ref().as_any().downcast_ref::<ControlCommand>() {
                    if c.command_type == CommandType::BeginString {
                        if i as i32 >= function_trim_index {
                            function_trim_index = -1;
                        }

                        break;
                    }
                } else if let Some(_) = o.as_ref().as_any().downcast_ref::<Glue>() {
                    glue_trim_index = i as i32;
                    break;
                }
            }
    
            let mut trim_index = -1;
            if glue_trim_index != -1 && function_trim_index != -1 {
                trim_index = function_trim_index.min(glue_trim_index);
            } else if glue_trim_index != -1 {
                trim_index = glue_trim_index;
            } else {
                trim_index = function_trim_index;
            }
    
            if trim_index != -1 {
                if let ValueType::String(t) = &text.value {
                    if t.is_newline {
                        include_in_output = false;
                    } else if t.is_non_whitespace() {
                        if glue_trim_index > -1 {
                            self.remove_existing_glue();
                        }
    
                        if function_trim_index > -1 {
                            let mut cs = self.get_callstack().as_ref().borrow_mut();
                            let callstack_elements = cs.get_elements_mut();
                            for i in (0..callstack_elements.len()).rev() {
                                if let Some(el) = callstack_elements.get_mut(i) {
                                    if el.push_pop_type == PushPopType::Function {
                                        el.function_start_in_output_stream = -1;
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            } else if let ValueType::String(t) = &text.value {
                if t.is_newline {
                    if self.output_stream_ends_in_newline() || !self.output_stream_contains_content() {
                        include_in_output = false;
                    }
                }
            }
        }
    
        if include_in_output {
            self.get_output_stream_mut().push(obj);
            self.output_stream_dirty();
        }
    }

    fn trim_newlines_from_output_stream(&self) {
        todo!()
    }

    fn remove_existing_glue(&self) {
        todo!()
    }

    fn output_stream_contains_content(&self) -> bool {
        todo!()
    }

    pub(crate) fn set_previous_pointer(&self, p: Pointer) {
        self.get_callstack().as_ref().borrow_mut().get_current_thread_mut().previous_pointer = p.clone();
    }

    pub(crate) fn try_exit_function_evaluation_from_game(&self) {
        todo!()
    }

    pub(crate) fn pop_callstack(&self, function: PushPopType) {
        todo!()
    }

    fn go_to_start(&self) {
        self.get_callstack().as_ref().borrow_mut().get_current_element_mut().current_pointer = Pointer::start_of(self.main_content_container.clone())
    }

    pub(crate) fn get_current_choices(&self) -> Option<&Vec<Rc<Choice>>> {
        // If we can continue generating text content rather than choices,
        // then we reflect the choice list as being empty, since choices
        // should always come at the end.
        if self.can_continue() {
            return None;
        }

        Some(&self.current_flow.current_choices)
    }

    fn clean_output_whitespace(input_str: &str) -> String {
        let mut result = String::with_capacity(input_str.len());
        let mut current_whitespace_start = -1;
        let mut start_of_line = 0;
    
        for (i, c) in input_str.chars().enumerate() {
            let is_inline_whitespace = c == ' ' || c == '\t';
    
            if is_inline_whitespace && current_whitespace_start == -1 {
                current_whitespace_start = i as i32;
            }
    
            if !is_inline_whitespace {
                if c != '\n' && current_whitespace_start > 0 && current_whitespace_start != start_of_line {
                    result.push(' ');
                }
                current_whitespace_start = -1;
            }
    
            if c == '\n' {
                start_of_line = i as i32 + 1;
            }
    
            if !is_inline_whitespace {
                result.push(c);
            }
        }
    
        result
    }
    

}