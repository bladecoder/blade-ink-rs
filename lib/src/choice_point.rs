use core::fmt;

use crate::{
    container::Container,
    object::{Object, RTObject},
    path::Path,
    threadsafe::BrCell,
    threadsafe::Brc,
};

pub struct ChoicePoint {
    obj: Object,
    has_choice_only_content: bool,
    has_start_content: bool,
    is_invisible_default: bool,
    once_only: bool,
    has_condition: bool,
    path_on_choice: BrCell<Path>,
}

impl ChoicePoint {
    pub fn new(flags: i32, path_string_on_choice: &str) -> Self {
        Self {
            obj: Object::new(),
            has_choice_only_content: (flags & 4) > 0,
            has_start_content: (flags & 2) > 0,
            is_invisible_default: (flags & 8) > 0,
            once_only: (flags & 16) > 0,
            has_condition: (flags & 1) > 0,
            path_on_choice: BrCell::new(Path::new_with_components_string(Some(
                path_string_on_choice,
            ))),
        }
    }

    pub fn get_choice_target(self: &Brc<Self>) -> Option<Brc<Container>> {
        Object::resolve_path(self.clone(), &self.path_on_choice.borrow()).container()
    }

    pub fn get_flags(&self) -> i32 {
        let mut flags = 0;
        if self.has_condition() {
            flags |= 1;
        }
        if self.has_start_content() {
            flags |= 2;
        }
        if self.has_choice_only_content() {
            flags |= 4;
        }
        if self.is_invisible_default() {
            flags |= 8;
        }
        if self.once_only() {
            flags |= 16;
        }
        flags
    }

    pub fn has_choice_only_content(&self) -> bool {
        self.has_choice_only_content
    }

    pub fn has_condition(&self) -> bool {
        self.has_condition
    }

    pub fn has_start_content(&self) -> bool {
        self.has_start_content
    }

    pub fn is_invisible_default(&self) -> bool {
        self.is_invisible_default
    }

    pub fn once_only(&self) -> bool {
        self.once_only
    }

    pub fn get_path_on_choice(self: &Brc<Self>) -> Path {
        // Resolve any relative paths to global ones as we come across them
        if self.path_on_choice.borrow().is_relative() {
            if let Some(choice_target_obj) = self.get_choice_target() {
                self.path_on_choice.replace(choice_target_obj.get_path());
            }
        }

        self.path_on_choice.borrow().clone()
    }

    pub fn get_path_string_on_choice(self: &Brc<Self>) -> String {
        Object::compact_path_string(self.clone(), &self.get_path_on_choice())
    }
}

impl RTObject for ChoicePoint {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for ChoicePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // let target_line_num = self.get_debug_line_number_of_path(self.get_path_on_choice()?)?;

        // let mut target_string = self.get_path_on_choice()?.to_string();

        let target_string = self.path_on_choice.borrow().to_string();

        // if let Some(line_num) = target_line_num {
        //     target_string = format!(" line {}({})", line_num, target_string);
        // }

        write!(f, "Choice: -> {}", target_string)
    }
}
