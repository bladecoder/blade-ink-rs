use std::{collections::HashMap, rc::Rc};

use crate::{list_definition::ListDefinition, value::Value, ink_list::InkList};

#[derive(Clone)]
pub struct ListDefinitionsOrigin {
    lists: HashMap<String, ListDefinition>,
    all_unambiguous_list_value_cache: HashMap<String, Rc<Value>>,
}

impl ListDefinitionsOrigin {
    pub fn new(lists: &mut Vec<ListDefinition>) -> Self {
        let mut list_definitions_origin = ListDefinitionsOrigin {
            lists: HashMap::new(),
            all_unambiguous_list_value_cache: HashMap::new(),
        };

        for list in lists {
            list_definitions_origin.lists.insert(list.get_name().to_string(), list.clone());

            for (key, val) in list.get_items() {
                let mut l = InkList::new();
                l.items.insert(key.clone(), *val);

                let list_value = Rc::new(Value::new_list(l));

                list_definitions_origin
                    .all_unambiguous_list_value_cache
                    .insert(key.get_item_name().to_string(), list_value.clone());
                list_definitions_origin
                    .all_unambiguous_list_value_cache
                    .insert(key.get_full_name().to_string(), list_value.clone());
            }
        }

        list_definitions_origin
    }

    pub fn get_list_definition(&self, name: &str) -> Option<&ListDefinition> {
        self.lists.get(name)
    }

    pub fn find_single_item_list_with_name(&self, name: &str) -> Option<&Rc<Value>> {
        self.all_unambiguous_list_value_cache.get(name)
    }
}
