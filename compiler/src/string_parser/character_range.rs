use super::CharacterSet;

#[derive(Debug, Clone)]
pub struct CharacterRange {
    start: char,
    end: char,
    excludes: CharacterSet,
    cached: Option<CharacterSet>,
}

impl CharacterRange {
    pub fn define(
        start: char,
        end: char,
        excludes: impl IntoIterator<Item = char>,
    ) -> Self {
        Self {
            start,
            end,
            excludes: CharacterSet::new().add_characters(excludes),
            cached: None,
        }
    }

    pub fn to_character_set(&mut self) -> CharacterSet {
        if let Some(cached) = &self.cached {
            return cached.clone();
        }

        let mut set = CharacterSet::new();
        for ch in self.start..=self.end {
            if !self.excludes.contains(ch) {
                set.add(ch);
            }
        }

        self.cached = Some(set.clone());
        set
    }

    pub fn start(&self) -> char {
        self.start
    }

    pub fn end(&self) -> char {
        self.end
    }
}
