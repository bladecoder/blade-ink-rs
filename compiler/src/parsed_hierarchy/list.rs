use std::collections::BTreeMap;

use super::{Expression, ObjectKind, ParsedObject};

#[derive(Debug, Clone)]
pub struct List {
    expression: Expression,
    item_identifier_list: Option<Vec<String>>,
}

impl List {
    pub fn new(item_identifier_list: Option<Vec<String>>) -> Self {
        Self {
            expression: Expression::new(ObjectKind::List),
            item_identifier_list,
        }
    }

    pub fn expression(&self) -> &Expression {
        &self.expression
    }

    pub fn item_identifier_list(&self) -> Option<&[String]> {
        self.item_identifier_list.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.item_identifier_list
            .as_ref()
            .is_none_or(|items| items.is_empty())
    }
}

#[derive(Debug, Clone)]
pub struct ListDefinition {
    object: ParsedObject,
    identifier: Option<String>,
    item_definitions: Vec<ListElementDefinition>,
}

impl ListDefinition {
    pub fn new(mut item_definitions: Vec<ListElementDefinition>) -> Self {
        let object = ParsedObject::new(ObjectKind::ListDefinition);

        let mut current_value = 1;
        for item in &mut item_definitions {
            if let Some(explicit_value) = item.explicit_value {
                current_value = explicit_value;
            }
            item.series_value = current_value;
            item.object.set_parent_id(object.id());
            current_value += 1;
        }

        Self {
            object,
            identifier: None,
            item_definitions,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub fn set_identifier(&mut self, identifier: impl Into<String>) {
        self.identifier = Some(identifier.into());
    }

    pub fn item_definitions(&self) -> &[ListElementDefinition] {
        &self.item_definitions
    }

    pub fn item_named(&self, item_name: &str) -> Option<&ListElementDefinition> {
        self.item_definitions
            .iter()
            .find(|item| item.name() == item_name)
    }

    pub fn runtime_definition_items(&self) -> BTreeMap<String, i32> {
        self.item_definitions
            .iter()
            .map(|item| (item.name().to_owned(), item.series_value()))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ListElementDefinition {
    object: ParsedObject,
    identifier: String,
    explicit_value: Option<i32>,
    series_value: i32,
    in_initial_list: bool,
}

impl ListElementDefinition {
    pub fn new(
        identifier: impl Into<String>,
        in_initial_list: bool,
        explicit_value: Option<i32>,
    ) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::ListElementDefinition),
            identifier: identifier.into(),
            explicit_value,
            series_value: 0,
            in_initial_list,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn name(&self) -> &str {
        &self.identifier
    }

    pub fn explicit_value(&self) -> Option<i32> {
        self.explicit_value
    }

    pub fn series_value(&self) -> i32 {
        self.series_value
    }

    pub fn in_initial_list(&self) -> bool {
        self.in_initial_list
    }

    pub fn full_name(&self, parent_list_name: &str) -> String {
        format!("{parent_list_name}.{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::{List, ListDefinition, ListElementDefinition};

    #[test]
    fn list_expression_tracks_empty_and_items() {
        assert!(List::new(None).is_empty());

        let list = List::new(Some(vec!["A".to_owned(), "B".to_owned()]));
        assert_eq!(
            Some(&["A".to_owned(), "B".to_owned()][..]),
            list.item_identifier_list()
        );
    }

    #[test]
    fn list_definition_assigns_series_values_like_reference() {
        let mut definition = ListDefinition::new(vec![
            ListElementDefinition::new("a", false, None),
            ListElementDefinition::new("b", true, Some(5)),
            ListElementDefinition::new("c", false, None),
        ]);
        definition.set_identifier("things");

        let items = definition.item_definitions();
        assert_eq!(1, items[0].series_value());
        assert_eq!(5, items[1].series_value());
        assert_eq!(6, items[2].series_value());
        assert_eq!("things.c", items[2].full_name("things"));
        assert_eq!(
            Some("b"),
            definition.item_named("b").map(|item| item.name())
        );
    }
}
