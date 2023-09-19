use std::{collections::HashMap, rc::Rc};

use crate::{pointer::{Pointer, self}, object::RTObject, push_pop::PushPopType, story::Story, container::Container};

pub(crate) struct Element {
    pub current_pointer: Pointer,
    pub in_expression_evaluation: bool,
    pub temporary_variables: HashMap<String, Rc<dyn RTObject>>,
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

pub(crate) struct Thread {
    pub(crate) callstack: Vec<Element>,
    pub(crate) previous_pointer: Pointer,
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

    pub(crate) fn copy(&self) -> Thread {
        let mut copy = Thread::new();
        copy.thread_index = self.thread_index;
        
        for e in self.callstack.iter() {
            copy.callstack.push(e.copy());
        }

        copy.previous_pointer = self.previous_pointer.clone();
        
        copy
    }
}

pub(crate) struct CallStack {
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

    pub(crate) fn get_current_element(&self) -> &Element {
        let thread = self.threads.last().unwrap();
        let cs = &thread.callstack;
        cs.last().unwrap()
    }

    pub(crate) fn get_current_element_mut(&mut self) -> &mut Element {
        let thread = self.threads.last_mut().unwrap();
        let cs = &mut thread.callstack;
        cs.last_mut().unwrap()
    }

    fn reset(&mut self) {
        self.threads.clear();
        self.threads.push(Thread::new());
        self.threads[0].callstack.push(Element::new(PushPopType::Tunnel, self.start_of_root.clone(), false));
    }

    pub(crate) fn can_pop_thread(&self) -> bool {
        return self.threads.len() > 1 && !self.element_is_evaluate_from_game();
    }

    pub(crate) fn pop_thread(&mut self) -> Result<(), String> {
        if self.can_pop_thread() {
            self.threads.remove(self.threads.len() - 1);
            Ok(())
        } else {
            Err("Can't pop thread".to_string())
        }
    }

    pub(crate) fn can_pop(&self) -> bool {
        todo!()
    }

    pub(crate) fn can_pop_type(&self, t: PushPopType) -> bool {
        todo!()
    }

    pub(crate) fn element_is_evaluate_from_game(&self) -> bool {
        self.get_current_element().push_pop_type == PushPopType::FunctionEvaluationFromGame
    }

    pub(crate) fn get_elements(&self) -> &Vec<Element> {
        self.get_callstack()
    }

    pub(crate) fn get_elements_mut(&mut self) -> &mut Vec<Element> {
        self.get_callstack_mut()
    }

    pub(crate) fn get_callstack(&self) -> &Vec<Element> {
        &self.get_current_thread().callstack
    }

    pub(crate) fn get_callstack_mut(&mut self) -> &mut Vec<Element> {
        &mut self.get_current_thread_mut().callstack
    }

    pub(crate) fn get_current_thread(&self) -> &Thread {
        self.threads.last().unwrap()
    }

    pub(crate) fn get_current_thread_mut(&mut self) -> &mut Thread {
        self.threads.last_mut().unwrap()
    }

    pub(crate) fn set_current_thread(&mut self, value: Thread) {
        // Debug.Assert (threads.Count == 1, "Shouldn't be directly setting the
        // current thread when we have a stack of them");
        self.threads.clear();
        self.threads.push(value);
    }

    pub(crate) fn fork_thread(&mut self) -> Thread {
        let mut forked_thread = self.get_current_thread().copy();
        self.thread_counter += 1;
        forked_thread.thread_index = self.thread_counter;
        forked_thread
    }
}