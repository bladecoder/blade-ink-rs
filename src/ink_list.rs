use core::fmt;
use std::collections::HashMap;

use crate::{ink_list_item::InkListItem, list_definition::ListDefinition, story::Story};

pub struct InkList {
    pub items: HashMap<InkListItem, i32>,
    origins: Vec<ListDefinition>,
    origin_names: Option<Vec<String>>,
}

impl InkList {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            origins: Vec::new(),
            origin_names: None,
        }
    }

    pub fn from_single_element(single_element: (InkListItem, i32)) -> Self {
        // let mut items = HashMap::new();
        // items.insert(single_element.0.clone(), single_element.1);

        // let mut origins = Vec::new();
        // if let Some(origin_name) = single_element.0.get_origin_name() {
        //     let def = origin_story.get_list_definitions().get_list_definition(origin_name);

        //     if let Some(list_def) = def {
        //         origins.push(list_def.clone());
        //     } else {
        //         panic!(
        //             "InkList origin could not be found in story when constructing new list: {}",
        //             origin_name
        //         );
        //     }
        // }

        // Self {
        //     items,
        //     origins,
        //     origin_names: None,
        // }

        todo!()
    }

    pub fn from_single_origin_list_name(
        single_origin_list_name: &str,
        origin_story: &Story,
    ) -> Result<Self, &'static str> {
        // let mut ink_list = InkList::new();
        // ink_list.set_initial_origin_name(single_origin_list_name, origin_story)?;
        // Ok(ink_list)

        todo!()
    }

    fn from_other_list(other_list: &InkList) -> Self {
        let mut ink_list = InkList::new();

        for (item, value) in &other_list.items {
            ink_list.items.insert(item.clone(), *value);
        }

        if let Some(names) = &other_list.origin_names {
            ink_list.origin_names = Some(names.clone());
        }

        ink_list.origins = other_list.origins.clone();

        ink_list
    }

    fn get_ordered_items(&self) -> Vec<(&InkListItem, &i32)> {
        let mut ordered: Vec<_> = self.items.iter().collect();
        ordered.sort_by(|a, b| {
            if a.1 == b.1 {
                a.0.get_origin_name()
                    .cmp(&b.0.get_origin_name())
            } else {
                a.1.cmp(b.1)
            }
        });
        ordered
    }

    pub fn get_max_item(&self) -> (Option<&InkListItem>, i32) {
        let mut max = (None, 0);

        for (k,v) in &self.items {
            if max.0.is_none() || *v > max.1 {
                max = (Some(k), *v);
            }

        }

        max
    }

    pub fn get_min_item(&self) -> (Option<&InkListItem>, i32) {
        let mut min = (None, 0);

        for (k,v) in &self.items {
            if min.0.is_none() || *v < min.1 {
                min = (Some(k), *v);
            }

        }

        min
    }

    pub fn set_initial_origin_names(&mut self, initial_origin_names: Option<Vec<String>>) {
        match &initial_origin_names {
            Some(_) => {
                self.origin_names = initial_origin_names;
            },
            None =>  self.origin_names = None,
        };
    }

    pub fn get_origin_names(&mut self) -> &Option<Vec<String>> {
        if self.items.len() > 0 {

            if self.origin_names.is_none()  && self.items.len() > 0 {
                self.origin_names = Some(Vec::new());
             } else { 
                self.origin_names.as_mut().unwrap().clear();
            }

            for k in self.items.keys() {
                self.origin_names.as_mut().unwrap().push(k.get_origin_name().unwrap().clone());
            }
        }

        return &self.origin_names;
    }

    pub fn union(&self, other_list: &InkList) -> InkList {
        let mut union = InkList::from_other_list(self);
       
       for (key, value) in &other_list.items {
            union.items.insert(key.clone(), *value);
        }

        union
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
        let mut intersection = InkList::new();
       
        for (k, v) in &self.items {
            if other_list.items.contains_key(k) {
                intersection.items.insert(k.clone(), *v);
            }
        }

        intersection
    }

    pub fn contains(&self, other_list: &InkList) -> bool {
        if other_list.items.len() == 0 || self.items.len() == 0 { return false; }

        for k in other_list.items.keys() {
            if !self.items.contains_key(k) { return false; }
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