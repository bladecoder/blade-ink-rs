use std::{collections::HashMap, rc::Rc};

use crate::{pointer::{Pointer, self}, object::RTObject, push_pop::PushPopType, story::Story, container::Container, value::Value};

pub struct Element {
    pub current_pointer: Pointer,
    pub in_expression_evaluation: bool,
    pub temporary_variables: HashMap<String, Rc<Value>>,
    pub push_pop_type: PushPopType,
    pub evaluation_stack_height_when_pushed: usize,
    pub function_start_in_output_stream: i32,
}

impl Element {
    fn new(push_pop_type: PushPopType, pointer: Pointer, in_expression_evaluation: bool) -> Element {
        Element {
            current_pointer: pointer,
            in_expression_evaluation: in_expression_evaluation,
            temporary_variables: HashMap::new(),
            push_pop_type: push_pop_type,
            evaluation_stack_height_when_pushed:0,
            function_start_in_output_stream: 0
        }
    }

    fn copy(&self) -> Element {
        let mut copy = Element::new(self.push_pop_type, self.current_pointer.clone(), self.in_expression_evaluation);
        copy.temporary_variables = self.temporary_variables.clone();
        copy.evaluation_stack_height_when_pushed = self.evaluation_stack_height_when_pushed;
        copy.function_start_in_output_stream = self.function_start_in_output_stream;
        
        copy
    }
}

pub struct Thread {
    pub callstack: Vec<Element>,
    pub previous_pointer: Pointer,
    thread_index: usize
}

impl Thread {
    fn new() -> Thread {
        Thread {
            callstack: Vec::new(),
            previous_pointer: pointer::NULL.clone(),
            thread_index: 0,
        }
    }

    pub fn copy(&self) -> Thread {
        let mut copy = Thread::new();
        copy.thread_index = self.thread_index;
        
        for e in self.callstack.iter() {
            copy.callstack.push(e.copy());
        }

        copy.previous_pointer = self.previous_pointer.clone();
        
        copy
    }
}

pub struct CallStack {
    thread_counter: usize,
    start_of_root: Pointer,
    threads: Vec<Thread>
}

impl CallStack {
    pub fn new(main_content_container: Rc<Container>) -> CallStack {
        let mut cs = CallStack {
            thread_counter: 0,
            start_of_root: Pointer::start_of(main_content_container),
            threads: Vec::new(),
        };

        cs.reset();

        cs
    }

    pub fn new_from(to_copy: &CallStack) -> CallStack {
        let mut cs = CallStack {
            thread_counter: to_copy.thread_counter,
            start_of_root: to_copy.start_of_root.clone(),
            threads: Vec::new(),
        };

        for  other_thread in &to_copy.threads {
            cs.threads.push(other_thread.copy());
        }

        cs
    }

    pub fn get_current_element(&self) -> &Element {
        let thread = self.threads.last().unwrap();
        let cs = &thread.callstack;
        cs.last().unwrap()
    }

    pub fn get_current_element_mut(&mut self) -> &mut Element {
        let thread = self.threads.last_mut().unwrap();
        let cs = &mut thread.callstack;
        cs.last_mut().unwrap()
    }

    pub fn get_current_element_index(&self) -> i32 {
        self.get_callstack().len() as i32 - 1
    }

    pub fn reset(&mut self) {
        self.threads.clear();
        self.threads.push(Thread::new());
        self.threads[0].callstack.push(Element::new(PushPopType::Tunnel, self.start_of_root.clone(), false));
    }

    pub fn can_pop_thread(&self) -> bool {
        return self.threads.len() > 1 && !self.element_is_evaluate_from_game();
    }

    pub fn pop_thread(&mut self) -> Result<(), String> {
        if self.can_pop_thread() {
            self.threads.remove(self.threads.len() - 1);
            Ok(())
        } else {
            Err("Can't pop thread".to_owned())
        }
    }

    pub fn push_thread(&mut self) {
        let mut newThread = self.get_current_thread().copy();
        self.thread_counter += 1;
        newThread.thread_index = self.thread_counter;
        self.threads.push(newThread);
    }

    pub fn can_pop(&self) -> bool {
        self.get_callstack().len() > 1
    }

    pub fn can_pop_type(&self, t: Option<PushPopType>) -> bool {
        if !self.can_pop() {
            return false;
        }

        if t.is_none() { return true; }

        self.get_current_element().push_pop_type == t.unwrap()
    }

    pub fn pop(&mut self, t: Option<PushPopType>) {
        if self.can_pop_type(t) {
            let l = self.get_callstack().len() - 1;
            self.get_callstack_mut().remove(l);
            return;
        } else {
            panic!("Mismatched push/pop in Callstack");
        }
    }

    pub fn element_is_evaluate_from_game(&self) -> bool {
        self.get_current_element().push_pop_type == PushPopType::FunctionEvaluationFromGame
    }

    pub fn get_elements(&self) -> &Vec<Element> {
        self.get_callstack()
    }

    pub fn get_elements_mut(&mut self) -> &mut Vec<Element> {
        self.get_callstack_mut()
    }

    pub fn get_callstack(&self) -> &Vec<Element> {
        &self.get_current_thread().callstack
    }

    pub fn get_callstack_mut(&mut self) -> &mut Vec<Element> {
        &mut self.get_current_thread_mut().callstack
    }

    pub fn get_current_thread(&self) -> &Thread {
        self.threads.last().unwrap()
    }

    pub fn get_current_thread_mut(&mut self) -> &mut Thread {
        self.threads.last_mut().unwrap()
    }

    pub fn set_current_thread(&mut self, value: Thread) {
        // Debug.Assert (threads.Count == 1, "Shouldn't be directly setting the
        // current thread when we have a stack of them");
        self.threads.clear();
        self.threads.push(value);
    }

    pub fn fork_thread(&mut self) -> Thread {
        let mut forked_thread = self.get_current_thread().copy();
        self.thread_counter += 1;
        forked_thread.thread_index = self.thread_counter;
        forked_thread
    }

    pub fn set_temporary_variable(
        &mut self,
        name: String,
        value: Rc<Value>,
        declare_new: bool,
        mut context_index: i32,
    ) -> Result<(), String> {
        if context_index == -1 {
            context_index = self.get_current_element_index() + 1;
        }

        let context_element = self.get_callstack_mut().get_mut((context_index - 1) as usize).unwrap();

        if !declare_new && !context_element.temporary_variables.contains_key(&name) {
            return Err(format!("Could not find temporary variable to set: {}", name));
        }

        let old_value = context_element.temporary_variables.get(&name).cloned();

        if let Some(old_value) = &old_value {
            Value::retain_list_origins_for_assignment(old_value.as_ref(), value.as_ref());
        }

        context_element.temporary_variables.insert(name, value);

        Ok(())
    }

    pub fn context_for_variable_named(&self, name: &str) -> usize {
        // Check if the current temporary context contains the variable.
        if self.get_current_element().temporary_variables.contains_key(name) {
            return (self.get_current_element_index() + 1) as usize;
        }
    
        // Otherwise, it's a global variable.
        0
    }

    pub fn get_temporary_variable_with_name(&self, name: &str, context_index: i32) -> Option<Rc<Value>> {
        let mut context_index = context_index;
        if context_index == -1 {
            context_index = self.get_current_element_index() + 1;
        }

        let context_element = self.get_callstack().get((context_index - 1)as usize);
        let var_value = context_element.unwrap().temporary_variables.get(name);

        var_value.cloned()
    }

    pub fn push( &mut self, t: PushPopType, external_evaluation_stack_height: usize, output_stream_length_with_pushed: i32) {
        // When pushing to callstack, maintain the current content path, but
        // jump
        // out of expressions by default
        let mut element =  Element::new(t, self.get_current_element().current_pointer.clone(), false);

        element.evaluation_stack_height_when_pushed = external_evaluation_stack_height;
        element.function_start_in_output_stream = output_stream_length_with_pushed;

        self.get_callstack_mut().push(element);
    }
}