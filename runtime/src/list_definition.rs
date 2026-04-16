use std::collections::HashMap;

use crate::ink_list_item::InkListItem;

#[derive(Clone)]
pub struct ListDefinition {
    name: String,
    items: Option<HashMap<InkListItem, i32>>,
    item_name_to_values: HashMap<String, i32>,
}

impl ListDefinition {
    pub fn new(name: String, items: HashMap<String, i32>) -> Self {
        Self {
            name,
            items: None,
            item_name_to_values: items,
        }
    }

    pub fn get_items(&mut self) -> &HashMap<InkListItem, i32> {
        if self.items.is_none() {
            let mut new_items = HashMap::new();
            for (item_name, value) in &self.item_name_to_values {
                let item = InkListItem::new(Some(self.name.clone()), item_name.clone());
                new_items.insert(item, *value);
            }
            self.items = Some(new_items);
        }

        self.items.as_ref().unwrap()
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_value_for_item(&self, item: &InkListItem) -> Option<&i32> {
        self.item_name_to_values.get(item.get_item_name())
    }

    pub fn contains_item(&self, item: &InkListItem) -> bool {
        item.get_origin_name() == Some(&self.name)
            && self.item_name_to_values.contains_key(item.get_item_name())
    }

    pub fn contains_item_with_name(&self, item_name: &str) -> bool {
        self.item_name_to_values.contains_key(item_name)
    }

    pub fn get_item_with_value(&self, val: i32) -> Option<InkListItem> {
        for (item_name, value) in &self.item_name_to_values {
            if *value == val {
                return Some(InkListItem::new(Some(self.name.clone()), item_name.clone()));
            }
        }
        None
    }
}
