use bladeink::story::INK_VERSION_CURRENT;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextFragment {
    Text(String),
    Newline,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParsedStory {
    fragments: Vec<TextFragment>,
}

impl ParsedStory {
    pub fn new(fragments: Vec<TextFragment>) -> Self {
        Self { fragments }
    }

    pub fn to_json_value(&self) -> Value {
        let mut root_content: Vec<Value> = Vec::with_capacity(self.fragments.len() + 2);

        for fragment in &self.fragments {
            match fragment {
                TextFragment::Text(text) => root_content.push(json!(format!("^{text}"))),
                TextFragment::Newline => root_content.push(json!("\n")),
            }
        }

        root_content.push(json!(["done", {"#n": "g-0"}]));
        root_content.push(Value::Null);

        json!({
            "inkVersion": INK_VERSION_CURRENT,
            "root": [root_content, "done", Value::Null],
            "listDefs": {}
        })
    }

    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.to_json_value())
    }
}
