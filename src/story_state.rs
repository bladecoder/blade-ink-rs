#![allow(unused_variables, dead_code)]

use std::{rc::Rc, cell::RefCell, collections::HashMap};

use crate::{pointer::{Pointer, self}, callstack::CallStack, flow::Flow, variables_state::VariablesState, choice::Choice, object::{RTObject, Object}, value::Value, glue::Glue, push_pop::PushPopType, control_command::{CommandType, ControlCommand}, container::Container, state_patch::StatePatch, story::Story, path::Path, void::Void, tag::Tag, list_definitions_origin::ListDefinitionsOrigin, value_type::ValueType};

use rand::Rng;

pub const INK_SAVE_STATE_VERSION: u32 = 10;
pub const MIN_COMPATIBLE_LOAD_VERSION: u32 = 8;

static DEFAULT_FLOW_NAME: &str = "DEFAULT_FLOW";

pub struct StoryState {
    pub current_flow: Flow,
    pub did_safe_exit: bool,
    output_stream_text_dirty: bool,
    output_stream_tags_dirty: bool,
    variables_state: VariablesState,
    alive_flow_names_dirty: bool,
    pub evaluation_stack: Vec<Rc<dyn RTObject>>,
    main_content_container: Rc<Container>,
    current_errors: Vec<String>,
    current_warnings: Vec<String>,
    current_text: Option<String>,
    patch: Option<StatePatch>,
    named_flows: Option<HashMap<String, Flow>>,
    pub diverted_pointer: Pointer,
    pub visit_counts: HashMap<String, usize>,
    pub turn_indices: HashMap<String, usize>,
    pub current_turn_index: i32,
    pub story_seed: i32,
    pub previous_random: i32,
    current_tags: Vec<String>,
    list_definitions: Rc<ListDefinitionsOrigin>,
}

impl StoryState {
    pub fn new(main_content_container: Rc<Container>, list_definitions: Rc<ListDefinitionsOrigin>) -> StoryState {
        let current_flow = Flow::new(DEFAULT_FLOW_NAME, main_content_container.clone());
        let callstack = current_flow.callstack.clone();

        let mut rng = rand::thread_rng();
        let story_seed = rng.gen_range(0..100);

        let state = StoryState { 
            current_flow,
            did_safe_exit: false,
            output_stream_text_dirty: true,
            output_stream_tags_dirty: true,
            variables_state: VariablesState::new(callstack, list_definitions.clone()),
            alive_flow_names_dirty: true,
            evaluation_stack: Vec::new(),
            main_content_container,
            current_errors: Vec::with_capacity(0),
            current_warnings: Vec::with_capacity(0),
            current_text: None,
            patch: None,
            named_flows: None,
            diverted_pointer: pointer::NULL.clone(),
            visit_counts: HashMap::new(),
            turn_indices: HashMap::new(),
            current_turn_index: -1,
            story_seed,
            previous_random: 0,
            current_tags: Vec::with_capacity(0),
            list_definitions,
        };

        state.go_to_start();

        state
    }

    pub fn can_continue(&self) -> bool {
        !self.get_current_pointer().is_null() && !self.has_error()
    }

    pub fn has_error(&self) -> bool {
        !self.current_errors.is_empty()
    }

    pub fn get_current_pointer(&self) -> Pointer {
        self.get_callstack().borrow().get_current_element().current_pointer.clone()
    }

    pub fn get_callstack(&self) -> &Rc<RefCell<CallStack>> {
        &self.current_flow.callstack
    }

    pub fn set_did_safe_exit(&mut self, did_safe_exit: bool) {
        self.did_safe_exit = did_safe_exit;
    }

    pub fn reset_output(&mut self, objs: Option<Vec<Rc<dyn RTObject>>>) {
        self.get_output_stream_mut().clear();
        if let Some(objs) = objs {
            for o in objs {
                self.get_output_stream_mut().push(o.clone());
            }
        }
        self.output_stream_dirty();
    }

    pub fn get_variables_state(&self) -> &VariablesState {
        &self.variables_state
    }

    pub fn get_variables_state_mut(&mut self) -> &mut VariablesState {
        &mut self.variables_state
    }

    pub fn get_generated_choices_mut(&mut self) -> &mut Vec<Rc<Choice>> {
        &mut self.current_flow.current_choices
    }

    pub fn get_generated_choices(&self) -> &Vec<Rc<Choice>> {
        &self.current_flow.current_choices
    }

    pub fn is_did_safe_exit(&self) -> bool {
        self.did_safe_exit
    }

    pub fn has_warning(&self) -> bool {
        !self.current_warnings.is_empty()
    }

    pub fn get_current_errors(&self) -> &Vec<String> {
        &self.current_errors
    }

    pub fn get_current_warnings(&self) -> &Vec<String> {
        &self.current_warnings
    }

    pub fn get_output_stream(&self) -> &Vec<Rc<(dyn RTObject)>> {
        &self.current_flow.output_stream
    }

    fn get_output_stream_mut(&mut self) -> &mut Vec<Rc<(dyn RTObject)>> {
        &mut self.current_flow.output_stream
    }

    fn output_stream_dirty(&mut self) {
        self.output_stream_text_dirty = true;
        self.output_stream_tags_dirty = true;
    }

    pub fn in_string_evaluation(&self) -> bool {
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

            for output_obj in self.get_output_stream() {
                let text_content = match output_obj.as_ref().as_any().downcast_ref::<Value>() {
                    Some(v) => match &v.value {
                        ValueType::String(s) => Some(s),
                        _ => None,
                    },
                    None => None,
                };

                if !in_tag && text_content.is_some() {
                    sb.push_str(&text_content.unwrap().string);
                } else if let Some(control_command) = output_obj.as_ref().as_any().downcast_ref::<ControlCommand>() {
                    if control_command.command_type == CommandType::BeginTag {
                        in_tag = true;
                    } else if control_command.command_type == CommandType::EndTag {
                        in_tag = false;
                    }
                }
            }

            self.current_text = Some(StoryState::clean_output_whitespace(&sb));

            self.output_stream_text_dirty = false;
        }

        self.current_text.as_ref().unwrap().to_string()
    }

    pub fn get_current_tags(&mut self) -> Vec<String> {
        if self.output_stream_tags_dirty {
            self.current_tags.clear();
    
            let mut in_tag = false;
            let mut sb = String::new();
    
            for output_obj in self.get_output_stream().clone() {
                if let Some(control_command) = output_obj.as_ref().as_any().downcast_ref::<ControlCommand>() {
                    match control_command.command_type {
                        CommandType::BeginTag => {
                            if in_tag && !sb.is_empty() {
                                let txt = Self::clean_output_whitespace(&sb);
                                self.current_tags.push(txt);
                                sb.clear();
                            }
                            in_tag = true;
                        },
                        CommandType::EndTag => {
                            if !sb.is_empty() {
                                let txt = Self::clean_output_whitespace(&sb);
                                self.current_tags.push(txt);
                                sb.clear();
                            }
                            in_tag = false;
                        },
                        _ => {},
                    }
                } else if in_tag {
                    if let Some(string_value) = Value::get_string_value(output_obj.as_ref()) {
                        sb.push_str(&string_value.string);
                    }
                    if let Some(tag) = output_obj.as_ref().as_any().downcast_ref::<Tag>() {
                        if !tag.get_text().is_empty() {
                            self.current_tags.push(tag.get_text().clone()); // tag.text has whitespace already cleaned
                        }
                    }
                }
            }
    
            if !sb.is_empty() {
                let txt = Self::clean_output_whitespace(&sb);
                self.current_tags.push(txt);
                sb.clear();
            }
    
            self.output_stream_tags_dirty = false;
        }
    
        self.current_tags.clone()
    }

    pub fn clean_output_whitespace(input_str: &str) -> String {
        let mut sb = String::with_capacity(input_str.len());
        let mut current_whitespace_start = -1;
        let mut start_of_line = 0;
    
        for (i, c) in input_str.chars().enumerate() {
            let is_inline_whitespace = c == ' ' || c == '\t';
    
            if is_inline_whitespace && current_whitespace_start == -1 {
                current_whitespace_start = i as i32;
            }
    
            if !is_inline_whitespace {
                if c != '\n' && current_whitespace_start > 0 && current_whitespace_start != start_of_line {
                    sb.push(' ');
                }
                current_whitespace_start = -1;
            }
    
            if c == '\n' {
                start_of_line = i as i32 + 1;
            }
    
            if !is_inline_whitespace {
                sb.push(c);
            }
        }
    
        sb
    }
    

    pub fn output_stream_ends_in_newline(&self) -> bool {
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

    pub fn set_current_pointer(&self, pointer: Pointer) {
        if !pointer.container.is_none() && pointer.index >= pointer.container.as_ref().unwrap().content.len() as i32 {
            panic!()
        }

        self.get_callstack().as_ref().borrow_mut().get_current_element_mut().current_pointer = pointer;
    }

    pub fn get_in_expression_evaluation(&self) -> bool {
        self.get_callstack().borrow().get_current_element().in_expression_evaluation
    }

    pub fn set_in_expression_evaluation(&self, value: bool) {
        self.get_callstack().borrow_mut().get_current_element_mut().in_expression_evaluation = value;
    }

    pub fn push_evaluation_stack(&mut self, obj: Rc<dyn RTObject>) {
    
        if let Some(list) = Value::get_list_value(obj.as_ref()) {
            let origin_names = list.get_origin_names();

            list.origins.borrow_mut().clear();

            for name in &origin_names {
                let def = self.list_definitions.get_list_definition(name).unwrap();
                if !list.origins.borrow().iter().any(|e| std::ptr::eq(e, def)){
                    list.origins.borrow_mut().push(def.clone());
                }
            }
        }
    
        println!("PUSH: {}", obj.as_ref());
        self.evaluation_stack.push(obj);
    }

    pub fn push_to_output_stream(&mut self, obj: Rc<dyn RTObject>) {
        let text = {
                let obj = obj.clone();
                match obj.into_any().downcast::<Value>() {
                    Ok(v) => match &v.value {
                        ValueType::String(s) => Some(s.clone()),
                        _ => None,
                    },
                    Err(_) => None,
                }
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

        self.push_to_output_stream_individual(obj);
    }

    pub fn increment_visit_count_for_container(&mut self, container: &Rc<Container>) {
        let has_patch = self.patch.is_some();

        if has_patch {
            let curr_count = self.visit_count_for_container(container);
            let new_count = curr_count + 1;
            self.patch.as_mut().unwrap().set_visit_count(container, new_count);
        } else {
            let mut count = 0;
            let container_path_str = container.get_path().to_string();
    
            if let Some(&existing_count) = self.visit_counts.get(&container_path_str) {
                count = existing_count;
            }
    
            count += 1;
            self.visit_counts.insert(container_path_str, count);
        }
    }

    pub fn visit_count_for_container(&mut self, container: &Rc<Container>) -> usize {
        if !container.visits_should_be_counted {
            // TODO

            // story.error(format!(
            //     "Read count for target ({:?} - on {:?}) unknown.",
            //     container.get_name(),
            //     container.get_debug_metadata()
            // ));
            return 0;
        }
    
        if let Some(patch) = &self.patch {
            if let Some(visit_count) = patch.get_visit_count(container) {
                return visit_count;
            }
        }
    
        let container_path_str = container.get_path().to_string();
    
        if let Some(&count) = self.visit_counts.get(&container_path_str) {
            return count;
        }
    
        0
    }

    pub fn record_turn_index_visit_to_container(&mut self, container: &Container) {
        if let Some(patch) = &mut self.patch {
            patch.set_turn_index(container, self.current_turn_index);
            return;
        }

        let container_path_str = Object::get_path(container).to_string();
        self.turn_indices.insert(container_path_str, self.current_turn_index as usize);
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
        let text = Value::get_string_value(obj.as_ref());
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
        else if let Some(text) = text {
            let mut function_trim_index = -1;

            { // block to release cs borrow
                let cs = self.get_callstack().borrow();
                let curr_el = cs.get_current_element();
                if curr_el.push_pop_type == PushPopType::Function {
                    function_trim_index = curr_el.function_start_in_output_stream as i32;
                }
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
                if text.is_newline {
                    include_in_output = false;
                } else if text.is_non_whitespace() {
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
            } else if text.is_newline && (self.output_stream_ends_in_newline() || !self.output_stream_contains_content()) {
                include_in_output = false;
            }
        }
    
        if include_in_output {
            self.get_output_stream_mut().push(obj);
            self.output_stream_dirty();
        }
    }

    fn trim_newlines_from_output_stream(&mut self) {
        let mut remove_whitespace_from = -1;
        let output_stream = self.get_output_stream_mut();

        // Work back from the end, and try to find the point where
        // we need to start removing content.
        // - Simply work backwards to find the first newline in a String of
        // whitespace
        // e.g. This is the content \n \n\n
        // ^---------^ whitespace to remove
        // ^--- first while loop stops here
        let mut i = output_stream.len() as i32 - 1;
        while i >= 0 {
            if let Some(obj) = output_stream.get(i as usize) {
                if obj.as_ref().as_any().is::<ControlCommand>() {
                    break;
                } else if let Some(sv) = Value::get_string_value(obj.as_ref()) {

                    if sv.is_non_whitespace() {
                        break;
                    } else if sv.is_newline {
                        remove_whitespace_from = i;
                    }
                }
            }
            i -= 1;
        }

        // Remove the whitespace
        if remove_whitespace_from >= 0 {
            i = remove_whitespace_from;
            while i < output_stream.len() as i32 {
                if let Some(text) =  Value::get_string_value(output_stream[i as usize].as_ref()) {
                    output_stream.remove(i as usize);
                } else {
                    i += 1;
                }
            }
        }

        self.output_stream_dirty();
    }

    fn remove_existing_glue(&mut self) {
        let output_stream = self.get_output_stream_mut();

        let mut i = output_stream.len() as i32 - 1;
        while i >= 0 {
            if let Some(c) = output_stream.get(i as usize) {
                if c.as_ref().as_any().is::<Glue>() {
                    output_stream.remove(i as usize);
                } else if c.as_ref().as_any().is::<ControlCommand>() {
                    break;
                }
            }
            i -= 1;
        }

        self.output_stream_dirty();
    }

    fn output_stream_contains_content(&self) -> bool {
        for content in self.get_output_stream() {
            if let Some(v) = content.as_any().downcast_ref::<Value>() {
                if let ValueType::String(_) = v.value {
                    return true;
                }
            }
        }
        
        false
    }

    pub fn set_previous_pointer(&self, p: Pointer) {
        self.get_callstack().as_ref().borrow_mut().get_current_thread_mut().previous_pointer = p.clone();
    }

    pub fn get_previous_pointer(&self) -> Pointer {
        self.get_callstack().as_ref().borrow_mut().get_current_thread_mut().previous_pointer.clone()
    }

    pub fn try_exit_function_evaluation_from_game(&mut self) -> bool {
        if self.get_callstack().borrow().get_current_element().push_pop_type == PushPopType::FunctionEvaluationFromGame {
            self.set_current_pointer(pointer::NULL.clone());
            self.did_safe_exit = true;
            return true;
        }

        return false;
    }

    pub fn pop_callstack(&mut self, t: Option<PushPopType>) {
        // Add the end of a function call, trim any whitespace from the end.
        if self.get_callstack().borrow().get_current_element().push_pop_type == PushPopType::Function {self.trim_whitespace_from_function_end();}

        self.get_callstack().borrow_mut().pop(t);
    }

    fn go_to_start(&self) {
        self.get_callstack().as_ref().borrow_mut().get_current_element_mut().current_pointer = Pointer::start_of(self.main_content_container.clone())
    }

    pub fn get_current_choices(&self) -> Option<&Vec<Rc<Choice>>> {
        // If we can continue generating text content rather than choices,
        // then we reflect the choice list as being empty, since choices
        // should always come at the end.
        if self.can_continue() {
            return None;
        }

        Some(&self.current_flow.current_choices)
    }

    pub fn copy_and_start_patching(&self) -> StoryState {
        let mut copy = StoryState::new(self.main_content_container.clone(), self.list_definitions.clone());

        copy.patch = Some(StatePatch::new(self.patch.as_ref()));

        // Hijack the new default flow to become a copy of our current one
        // If the patch is applied, then this new flow will replace the old one in
        // _namedFlows
        copy.current_flow.name = self.current_flow.name.clone();
        copy.current_flow.callstack = Rc::new(RefCell::new(CallStack::new_from(&self.current_flow.callstack.as_ref().borrow())));
        copy.current_flow.current_choices = self.current_flow.current_choices.clone();
        copy.current_flow.output_stream = self.current_flow.output_stream.clone();
        copy.output_stream_dirty();

        // The copy of the state has its own copy of the named flows dictionary,
        // except with the current flow replaced with the copy above
        // (Assuming we're in multi-flow mode at all. If we're not then
        // the above copy is simply the default flow copy and we're done)
        if let Some(named_flows) = &self.named_flows {
            let mut nf = self.named_flows.clone();
            nf.as_mut().unwrap().insert(copy.current_flow.name.to_string(), copy.current_flow.clone());
            copy.alive_flow_names_dirty = true;

            copy.named_flows = nf;
        }

        if self.has_error() {
            copy.current_errors = self.current_errors.clone();
        }

        if self.has_warning() {
            copy.current_warnings = self.current_warnings.clone();

        }

        // ref copy - exactly the same variables state!
        // we're expecting not to read it only while in patch mode
        // (though the callstack will be modified) 
        copy.variables_state = self.variables_state.clone();
        copy.variables_state.set_callstack(copy.get_callstack().clone());
        copy.variables_state.patch = copy.patch.clone();

        copy.evaluation_stack = self.evaluation_stack.clone();

        if !self.diverted_pointer.is_null() {
            copy.diverted_pointer = self.diverted_pointer.clone();
        }

        copy.set_previous_pointer(self.get_previous_pointer().clone());

        // visit counts and turn indicies will be read only, not modified
        // while in patch mode
        copy.visit_counts = self.visit_counts.clone();
        copy.turn_indices = self.turn_indices.clone();

        copy.current_turn_index = self.current_turn_index;
        copy.story_seed = self.story_seed;
        copy.previous_random = self.previous_random;

        copy.set_did_safe_exit(self.did_safe_exit);

        copy
    }

    pub fn restore_after_patch(&mut self) {
        // VariablesState was being borrowed by the patched
        // state, so restore it with our own callstack.
        // _patch will be null normally, but if you're in the
        // middle of a save, it may contain a _patch for save purpsoes.
        self.variables_state.callstack = self.get_callstack().clone();
        self.variables_state.patch = self.patch.clone(); // usually null
    }

    pub fn apply_any_patch(&mut self) {
        if self.patch.is_none() {
            return;
        }
    
        self.variables_state.apply_patch();
    
        if self.patch.is_some() {
            for (path, count) in self.patch.as_ref().unwrap().visit_counts.clone().iter() {
                self.apply_count_changes(path, *count, true);
            }
        
            for (path, index) in self.patch.as_ref().unwrap().turn_indices.clone().iter() {
                self.apply_count_changes(path, *index, false);
            }
        }
    
        self.patch = None;
    }

    fn apply_count_changes(&mut self, container: &str, new_count: usize, is_visit: bool) {
        let counts = if is_visit {&mut self.visit_counts} else {&mut self.turn_indices};

        counts.insert(container.to_string(), new_count);
    }

    pub fn pop_from_output_stream(&mut self, count: usize) {
        let len = self.get_output_stream().len();

        if count <= len {
            let start = len - count;
            self.get_output_stream_mut().drain(start..len);
        }

        self.output_stream_dirty();
    }

    pub fn pop_evaluation_stack(&mut self) -> Rc<dyn RTObject> {
        let obj = self.evaluation_stack.pop().unwrap();

        println!("POP: {}", obj.to_string());

        obj
    }

    pub fn pop_evaluation_stack_multiple(&mut self, number_of_objects: usize) -> Vec<Rc<dyn RTObject>> {
        let start = self.evaluation_stack.len() - number_of_objects;
        let obj: Vec<Rc<dyn RTObject>> = self.evaluation_stack.drain(start..).collect();
        
        obj
    }

    pub fn set_diverted_pointer(&mut self, p: Pointer) {
        self.diverted_pointer = p;
    }

    pub fn set_chosen_path(&mut self, path: &Path, incrementing_turn_index: bool) {
        // Changing direction, assume we need to clear current set of choices
        self.current_flow.current_choices.clear();

        let mut new_pointer = Story::pointer_at_path(&self.main_content_container, &path);
        if !new_pointer.is_null() && new_pointer.index == -1 {
            new_pointer.index = 0;
        }

        self.set_current_pointer(new_pointer);

        if incrementing_turn_index {
            self.current_turn_index += 1;
        }
    }

    pub(crate) fn force_end(&mut self) {
        self.get_callstack().borrow_mut().reset();

        self.current_flow.current_choices.clear();

        self.set_current_pointer(pointer::NULL.clone());
        self.set_previous_pointer(pointer::NULL.clone());

        self.set_did_safe_exit(true);
    }

    // At the end of a function call, trim any whitespace from the end.
    // We always trim the start and end of the text that a function produces.
    // The start whitespace is discard as it is generated, and the end
    // whitespace is trimmed in one go here when we pop the function.
    fn trim_whitespace_from_function_end(&mut self) {
        assert_eq!(
            self.get_callstack().borrow().get_current_element().push_pop_type,
            PushPopType::Function
        );
    
        let function_start_point = match self.get_callstack().borrow().get_current_element().function_start_in_output_stream {
            -1 => 0,
            start_point => start_point,
        };
    
        // Trim whitespace from END of function call
        let mut i = self.get_output_stream().len() as isize - 1;
        while i >= function_start_point as isize {
            if let Some(obj) = self.get_output_stream().get(i as usize) {
                if let Some(_) = obj.as_any().downcast_ref::<ControlCommand>() {
                    break;
                } 

                if let Some(txt) = Value::get_string_value(obj.as_ref()) {
                    if txt.is_newline || txt.is_inline_whitespace {
                        self.get_output_stream_mut().remove(i as usize);
                        self.output_stream_dirty();
                    } else {
                        break;
                    }
                }
            }
            i -= 1;
        }
    }

    pub fn peek_evaluation_stack(&self) -> Option<&Rc<dyn RTObject>> {
        self.evaluation_stack.last()
    }

    pub fn start_function_evaluation_from_game(&mut self, func_container: Rc<Container>, arguments: Option<&Vec<String>>) -> Result<(), String> {
        self.get_callstack().borrow_mut().push(PushPopType::FunctionEvaluationFromGame, self.evaluation_stack.len(), 0);
        self.get_callstack().borrow_mut().get_current_element_mut().current_pointer = Pointer::start_of(func_container);

        self.pass_arguments_to_evaluation_stack(arguments)?;

        Ok(())
    }

    pub fn pass_arguments_to_evaluation_stack(&mut self, arguments: Option<&Vec<String>>) -> Result<(), String> {
        // Pass arguments onto the evaluation stack
        if let Some(arguments) = arguments {
            for arg in arguments {
                // TODO

                // if (!(arguments[i] instanceof Integer
                //         || arguments[i] instanceof Float
                //         || arguments[i] instanceof String
                //         || arguments[i] instanceof Boolean
                //         || arguments[i] instanceof InkList)) {
                //     throw new Exception(
                //             "ink arguments when calling EvaluateFunction / ChoosePathStringWithParameters must be "
                //                     + "int, float, string, bool or InkList. Argument was "
                //                     + (arguments[i] == null
                //                             ? "null"
                //                             : arguments[i].getClass().getName()));
                // }

                self.push_evaluation_stack(Rc::new(Value::new_string(arg)));
            }
        }
            
        Ok(())
    }

    pub fn complete_function_evaluation_from_game(&mut self) -> Result<Option<String>, String> {
        if self.get_callstack().borrow().get_current_element().push_pop_type != PushPopType::FunctionEvaluationFromGame {
            // TODO
            // return Err(format!("Expected external function evaluation to be complete. Stack trace: {}", getCallStack().getCallStackTrace());

            return Err("Expected external function evaluation to be complete. Stack trace".to_owned());
        }

        let original_evaluation_stack_height = self.get_callstack().borrow().get_current_element().evaluation_stack_height_when_pushed;

        // Do we have a returned value?
        // Potentially pop multiple values off the stack, in case we need
        // to clean up after ourselves (e.g. caller of EvaluateFunction may
        // have passed too many arguments, and we currently have no way to check
        // for that)
        let mut returned_obj = None;
        while self.evaluation_stack.len() > original_evaluation_stack_height {
            let popped_obj = self.pop_evaluation_stack();
            if returned_obj.is_none() {
                returned_obj = Some(popped_obj);
            }
        }

        // Finally, pop the external function evaluation
        self.get_callstack().borrow_mut().pop(Some(PushPopType::FunctionEvaluationFromGame));

        // What did we get back?
        if let Some(returned_obj) = returned_obj{
            if let Some(_) = returned_obj.as_ref().as_any().downcast_ref::<Void>() { return Ok(None); }

            // Some kind of value, if not void
            if let Some(return_val) = returned_obj.as_ref().as_any().downcast_ref::<Value>() {
                // DivertTargets get returned as the string of components
                // (rather than a Path, which isn't public)
                if let ValueType::DivertTarget(p) = &return_val.value {
                    return Ok(Some(p.to_string()));
                }

                // Other types can ust have their exact object type:
                // int, float, string. VariablePointers get returned as strings.
                // TODO
                return Ok(Some(return_val.to_string()));
            }
        }   

        Ok(None)    
    }

    pub(crate) fn turns_since_for_container(&self, container: &Container) -> i32 {
        if !container.turn_index_should_be_counted {
            // story.error("TURNS_SINCE() for target (" + container.getName() + " - on " + container.getDebugMetadata()
            //         + ") unknown.");
            panic!()
        }

        let mut index = 0;

        if self.patch.is_some() && self.patch.as_ref().unwrap().get_turn_index(container).is_some() {
            index = *self.patch.as_ref().unwrap().get_turn_index(container).unwrap() as i32;
            return self.current_turn_index - index;
        }

        let container_path_str = Object::get_path(container).to_string();

        if self.turn_indices.contains_key(&container_path_str) {
            index = *self.turn_indices.get(&container_path_str).unwrap() as i32;
            self.current_turn_index - index
        } else {
            -1
        }    
    }

    pub(crate) fn switch_flow_internal(&mut self, flow_name: &str) {

        if flow_name.eq(&self.current_flow.name) {
            return;
        }

        if self.named_flows.is_none() {
            self.named_flows = Some(HashMap::new());
        }

        let named_flows = self.named_flows.as_mut().unwrap();

        // store the current flow and retrieve and remove the next flow
        let flow = named_flows.remove(flow_name);

        let mut next_flow = match flow {
            Some(f) => f,
            None => {
                self.alive_flow_names_dirty = true;
                Flow::new(flow_name, self.main_content_container.clone())
            }
        };

        std::mem::swap(&mut self.current_flow, &mut next_flow);
        named_flows.insert(next_flow.name.clone(), next_flow);

        self.variables_state.set_callstack(self.current_flow.callstack.clone());

        // Cause text to be regenerated from output stream if necessary
        self.output_stream_dirty();
    }

}