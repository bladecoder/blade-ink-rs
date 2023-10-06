#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct InkListItem {
    origin_name: Option<String>,
    item_name: String,
}

impl InkListItem {
    pub fn new(origin_name: Option<String>, item_name: String) -> Self {
        Self {
            origin_name,
            item_name,
        }
    }

    pub fn from_full_name(full_name: &str) -> Self {
        let name_parts: Vec<&str> = full_name.split('.').collect();
        let origin_name = if name_parts.len() > 1 {
            Some(name_parts[0].to_string())
        } else {
            None
        };
        let item_name = name_parts.last().unwrap_or(&"").to_string();
        Self {
            origin_name,
            item_name,
        }
    }

    pub fn get_null() -> Self {
        Self {
            origin_name: None,
            item_name: String::new(),
        }
    }

    pub fn get_origin_name(&self) -> Option<&String> {
        self.origin_name.as_ref()
    }

    pub fn get_item_name(&self) -> &str {
        &self.item_name
    }

    pub fn get_full_name(&self) -> String {
        let origin = self.origin_name.as_deref().unwrap_or("?");
        format!("{}.{}", origin, self.item_name)
    }

    pub fn is_null(&self) -> bool {
        self.origin_name.is_none() && self.item_name.is_empty()
    }
}

impl std::fmt::Display for InkListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_full_name())
    }
}
