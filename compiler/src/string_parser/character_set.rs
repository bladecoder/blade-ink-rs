use std::collections::HashSet;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterSet {
    chars: HashSet<char>,
}

impl CharacterSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_range(start: char, end: char) -> Self {
        Self::new().add_range(start, end)
    }

    pub fn contains(&self, value: char) -> bool {
        self.chars.contains(&value)
    }

    pub fn add(&mut self, value: char) {
        self.chars.insert(value);
    }

    pub fn add_range(mut self, start: char, end: char) -> Self {
        for ch in start..=end {
            self.chars.insert(ch);
        }
        self
    }

    pub fn add_characters(mut self, chars: impl IntoIterator<Item = char>) -> Self {
        self.chars.extend(chars);
        self
    }

    pub fn union_with(&mut self, other: &CharacterSet) {
        self.chars.extend(other.chars.iter().copied());
    }
}

impl From<&str> for CharacterSet {
    fn from(value: &str) -> Self {
        Self::new().add_characters(value.chars())
    }
}

impl IntoIterator for CharacterSet {
    type Item = char;
    type IntoIter = std::collections::hash_set::IntoIter<char>;

    fn into_iter(self) -> Self::IntoIter {
        self.chars.into_iter()
    }
}
