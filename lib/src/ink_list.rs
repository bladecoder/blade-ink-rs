use core::fmt;
use std::{cell::RefCell, collections::HashMap};

use crate::{
    ink_list_item::InkListItem, list_definition::ListDefinition,
    list_definitions_origin::ListDefinitionsOrigin, story_error::StoryError, value_type::ValueType,
};

#[derive(Clone)]
pub struct InkList {
    pub items: HashMap<InkListItem, i32>,
    pub origins: RefCell<Vec<ListDefinition>>,
    // we need an origin when we only have the definition (the list has not elemetns)
    initial_origin_names: RefCell<Vec<String>>,
}

impl InkList {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            origins: RefCell::new(Vec::with_capacity(0)),
            initial_origin_names: RefCell::new(Vec::with_capacity(0)),
        }
    }

    pub fn from_single_element(single_element: (InkListItem, i32)) -> Self {
        let mut l = Self::new();
        l.items.insert(single_element.0, single_element.1);

        l
    }

    pub fn from_single_origin(
        single_origin: String,
        list_definitions: &ListDefinitionsOrigin,
    ) -> Result<Self, StoryError> {
        let l = Self::new();

        l.initial_origin_names.borrow_mut().push(single_origin);

        let def = list_definitions.get_list_definition(&l.initial_origin_names.borrow()[0]);

        if let Some(list_def) = def {
            l.origins.borrow_mut().push(list_def.clone());
        } else {
            return Err(StoryError::InvalidStoryState(format!(
                "InkList origin could not be found in story when constructing new list: {}",
                &l.initial_origin_names.borrow()[0]
            )));
        }

        Ok(l)
    }

    fn from_other_list(other_list: &InkList) -> Self {
        let mut ink_list = InkList::new();

        for (item, value) in &other_list.items {
            ink_list.items.insert(item.clone(), *value);
        }

        ink_list.initial_origin_names = other_list.initial_origin_names.clone();

        ink_list.origins = other_list.origins.clone();

        ink_list
    }

    fn get_ordered_items(&self) -> Vec<(&InkListItem, &i32)> {
        let mut ordered: Vec<_> = self.items.iter().collect();
        ordered.sort_by(|a, b| {
            if a.1 == b.1 {
                a.0.get_origin_name().cmp(&b.0.get_origin_name())
            } else {
                a.1.cmp(b.1)
            }
        });
        ordered
    }

    pub fn get_max_item(&self) -> Option<(&InkListItem, i32)> {
        let mut max: Option<(&InkListItem, i32)> = None;

        for (k, v) in &self.items {
            if max.is_none() || *v > max.as_ref().unwrap().1 {
                max = Some((k, *v));
            }
        }

        max
    }

    pub fn get_min_item(&self) -> Option<(&InkListItem, i32)> {
        let mut min: Option<(&InkListItem, i32)> = None;

        for (k, v) in &self.items {
            if min.is_none() || *v < min.as_ref().unwrap().1 {
                min = Some((k, *v));
            }
        }

        min
    }

    pub fn set_initial_origin_names(&self, initial_origin_names: Vec<String>) {
        self.initial_origin_names.replace(initial_origin_names);
    }

    pub fn get_origin_names(&self) -> Vec<String> {
        if !self.items.is_empty() {
            let mut names = Vec::new();

            for k in self.items.keys() {
                names.push(k.get_origin_name().unwrap().clone());
            }

            return names;
        }

        self.initial_origin_names.borrow().clone()
    }

    pub fn union(&self, other_list: &InkList) -> InkList {
        let mut union = InkList::from_other_list(self);

        for (key, value) in &other_list.items {
            union.items.insert(key.clone(), *value);
        }

        union
    }

    pub fn without(&self, other_list: &InkList) -> InkList {
        let mut result = InkList::from_other_list(self);

        other_list.items.iter().for_each(|(key, _)| {
            result.items.remove(key);
        });

        result
    }

    pub fn intersect(&self, other_list: &InkList) -> InkList {
        let mut intersection = InkList::new();

        for (k, v) in &self.items {
            if other_list.items.contains_key(k) {
                intersection.items.insert(k.clone(), *v);
            }
        }

        intersection
    }

    pub fn has(&self, other_list: &InkList) -> InkList {
        let mut result = InkList::new();

        for (k, v) in &self.items {
            if other_list.items.contains_key(k) {
                result.items.insert(k.clone(), *v);
            }
        }

        result
    }

    pub fn contains(&self, other_list: &InkList) -> bool {
        if other_list.items.is_empty() || self.items.is_empty() {
            return false;
        }

        for k in other_list.items.keys() {
            if !self.items.contains_key(k) {
                return false;
            }
        }

        true
    }

    pub(crate) fn get_all(&self) -> InkList {
        let mut list = InkList::new();

        for origin in self.origins.borrow_mut().iter_mut() {
            for (k, v) in origin.get_items().iter() {
                list.items.insert(k.clone(), *v);
            }
        }

        list
    }

    pub(crate) fn list_with_sub_range(
        &self,
        min_bound: &ValueType,
        max_bound: &ValueType,
    ) -> InkList {
        if self.items.is_empty() {
            return InkList::new();
        }

        let ordered = self.get_ordered_items();
        let mut min_value = 0;
        let mut max_value = i32::MAX;

        if let ValueType::Int(v) = min_bound {
            min_value = *v;
        } else if let ValueType::List(l) = min_bound {
            if !l.items.is_empty() {
                min_value = l.get_min_item().unwrap().1;
            }
        }

        if let ValueType::Int(v) = max_bound {
            max_value = *v;
        } else if let ValueType::List(l) = max_bound {
            if !l.items.is_empty() {
                max_value = l.get_max_item().unwrap().1;
            }
        }

        let mut sub_list = InkList::new();
        sub_list.set_initial_origin_names(self.initial_origin_names.borrow().clone());

        for (k, v) in ordered {
            if *v >= min_value && *v <= max_value {
                sub_list.items.insert(k.clone(), *v);
            }
        }

        sub_list
    }

    pub fn inverse(&self) -> InkList {
        let mut list = InkList::new();

        for origin in self.origins.borrow_mut().iter_mut() {
            for (k, v) in origin.get_items() {
                if !self.items.contains_key(k) {
                    list.items.insert(k.clone(), *v);
                }
            }
        }

        list
    }

    pub fn max_as_list(&self) -> InkList {
        match self.items.is_empty() {
            true => InkList::new(),
            false => {
                let item = self.get_max_item();
                InkList::from_single_element((
                    item.as_ref().unwrap().0.clone(),
                    item.as_ref().unwrap().1,
                ))
            }
        }
    }

    pub fn min_as_list(&self) -> InkList {
        match self.items.is_empty() {
            true => InkList::new(),
            false => {
                let item = self.get_min_item();
                InkList::from_single_element((
                    item.as_ref().unwrap().0.clone(),
                    item.as_ref().unwrap().1,
                ))
            }
        }
    }

    // Returns true if all the item values in the current list are greater than all
    // the item values in the passed-in list.
    pub fn greater_than(&self, other_list: &InkList) -> bool {
        if self.items.is_empty() {
            return false;
        }
        if other_list.items.is_empty() {
            return true;
        }

        // All greater
        self.get_min_item().unwrap().1 > other_list.get_max_item().unwrap().1
    }

    // Returns true if the item values in the current list overlap or are all
    // greater than the item values in the passed-in list.
    pub fn greater_than_or_equals(&self, other_list: &InkList) -> bool {
        if self.items.is_empty() {
            return false;
        }
        if other_list.items.is_empty() {
            return true;
        }

        // All greater
        self.get_min_item().unwrap().1 >= other_list.get_min_item().unwrap().1
            && self.get_max_item().unwrap().1 >= other_list.get_max_item().unwrap().1
    }

    // Returns true if all the item values in the current list are less than all the
    // item values in the passed-in list.
    pub fn less_than(&self, other_list: &InkList) -> bool {
        if other_list.items.is_empty() {
            return false;
        }
        if self.items.is_empty() {
            return true;
        }

        self.get_max_item().unwrap().1 < other_list.get_min_item().unwrap().1
    }

    // Returns true if the item values in the current list overlap or are all less
    // than the item values in the passed-in list.
    pub fn less_than_or_equals(&self, other_list: &InkList) -> bool {
        if other_list.items.is_empty() {
            return false;
        }
        if self.items.is_empty() {
            return true;
        }

        self.get_max_item().unwrap().1 <= other_list.get_max_item().unwrap().1
            && self.get_min_item().unwrap().1 <= other_list.get_min_item().unwrap().1
    }
}

impl Default for InkList {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for InkList {
    fn eq(&self, other: &Self) -> bool {
        if other.items.len() != self.items.len() {
            return false;
        }

        for key in self.items.keys() {
            if !other.items.contains_key(key) {
                return false;
            }
        }

        true
    }
}

impl fmt::Display for InkList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ordered = self.get_ordered_items();
        let mut result = String::new();

        for (i, (item, _)) in ordered.iter().enumerate() {
            if i > 0 {
                result.push_str(", ");
            }
            result.push_str(item.get_item_name());
        }

        write!(f, "{}", result)
    }
}
