mod character_range;
mod character_set;
mod comment_eliminator;
mod state;

use std::fmt;

pub use character_range::CharacterRange;
pub use character_set::CharacterSet;
pub use comment_eliminator::CommentEliminator;
pub use state::{StringParserState, StringParserStateElement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseSuccess;

pub type ParseRule<T> = dyn FnMut(&mut StringParser) -> Option<T>;

pub struct StringParser {
    chars: Vec<char>,
    input_string: String,
    state: StringParserState,
    had_error: bool,
}

impl fmt::Debug for StringParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StringParser")
            .field("input_string", &self.input_string)
            .field("index", &self.index())
            .field("line_index", &self.line_index())
            .field("character_in_line_index", &self.character_in_line_index())
            .field("stack_height", &self.state.stack_height())
            .field("had_error", &self.had_error)
            .finish()
    }
}

impl Clone for StringParser {
    fn clone(&self) -> Self {
        Self {
            chars: self.chars.clone(),
            input_string: self.input_string.clone(),
            state: self.state.clone(),
            had_error: self.had_error,
        }
    }
}

impl StringParser {
    pub fn new(input: impl Into<String>) -> Self {
        let input_string = input.into();
        let chars = input_string.chars().collect();
        Self {
            chars,
            input_string,
            state: StringParserState::new(),
            had_error: false,
        }
    }

    pub fn with_preprocessed_input(input: impl Into<String>) -> Self {
        let input_string = CommentEliminator::process(input.into());
        Self::new(input_string)
    }

    pub fn input_string(&self) -> &str {
        &self.input_string
    }

    pub fn state(&self) -> &StringParserState {
        &self.state
    }

    pub fn current_character(&self) -> Option<char> {
        self.chars.get(self.index()).copied()
    }

    pub fn end_of_input(&self) -> bool {
        self.index() >= self.chars.len()
    }

    pub fn remaining_length(&self) -> usize {
        self.chars.len().saturating_sub(self.index())
    }

    pub fn remaining_string(&self) -> String {
        self.chars[self.index()..].iter().collect()
    }

    pub fn line_remainder(&mut self) -> Option<String> {
        self.peek(|parser| parser.parse_until_characters_from_string("\n\r", -1))
    }

    pub fn line_index(&self) -> usize {
        self.state.line_index()
    }

    pub fn set_line_index(&mut self, value: usize) {
        self.state.set_line_index(value);
    }

    pub fn character_in_line_index(&self) -> usize {
        self.state.character_in_line_index()
    }

    pub fn set_character_in_line_index(&mut self, value: usize) {
        self.state.set_character_in_line_index(value);
    }

    pub fn index(&self) -> usize {
        self.state.character_index()
    }

    pub fn set_index(&mut self, value: usize) {
        self.state.set_character_index(value);
    }

    pub fn set_flag(&mut self, flag: u32, value: bool) {
        let custom_flags = self.state.custom_flags();
        if value {
            self.state.set_custom_flags(custom_flags | flag);
        } else {
            self.state.set_custom_flags(custom_flags & !flag);
        }
    }

    pub fn get_flag(&self, flag: u32) -> bool {
        (self.state.custom_flags() & flag) != 0
    }

    pub fn begin_rule(&mut self) -> usize {
        self.state.push()
    }

    pub fn fail_rule<T>(&mut self, expected_rule_id: usize) -> Option<T> {
        self.state.pop(expected_rule_id);
        None
    }

    pub fn cancel_rule(&mut self, expected_rule_id: usize) {
        self.state.pop(expected_rule_id);
    }

    pub fn succeed_rule<T>(&mut self, expected_rule_id: usize, result: Option<T>) -> Option<T> {
        let _ = self.state.peek(expected_rule_id);
        self.state.squash();
        result
    }

    pub fn parse_object<T>(
        &mut self,
        mut rule: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<T> {
        let rule_id = self.begin_rule();
        let stack_height_before = self.state.stack_height();
        let result = rule(self);
        assert_eq!(
            stack_height_before,
            self.state.stack_height(),
            "Mismatched Begin/Fail/Succeed rules"
        );

        if result.is_none() {
            self.fail_rule(rule_id)
        } else {
            self.succeed_rule(rule_id, result)
        }
    }

    pub fn one_of<T>(&mut self, rules: &mut [Box<ParseRule<T>>]) -> Option<T> {
        for rule in rules {
            if let Some(result) = self.parse_object(|parser| rule(parser)) {
                return Some(result);
            }
        }

        None
    }

    pub fn one_or_more<T>(
        &mut self,
        mut rule: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<Vec<T>> {
        let mut results = Vec::new();
        while let Some(result) = self.parse_object(|parser| rule(parser)) {
            results.push(result);
        }

        if results.is_empty() {
            None
        } else {
            Some(results)
        }
    }

    pub fn interleave<TA, TB>(
        &mut self,
        mut rule_a: impl FnMut(&mut Self) -> Option<TA>,
        mut rule_b: impl FnMut(&mut Self) -> Option<TB>,
    ) -> Vec<Interleaved<TA, TB>> {
        let mut results = Vec::new();

        if let Some(a) = self.parse_object(|parser| rule_a(parser)) {
            results.push(Interleaved::A(a));
        }

        loop {
            let Some(b) = self.parse_object(|parser| rule_b(parser)) else {
                break;
            };
            results.push(Interleaved::B(b));

            let Some(a) = self.parse_object(|parser| rule_a(parser)) else {
                break;
            };
            results.push(Interleaved::A(a));

            if self.remaining_length() == 0 {
                break;
            }
        }

        results
    }

    pub fn parse_string(&mut self, value: &str) -> Option<String> {
        if value.chars().count() > self.remaining_length() {
            return None;
        }

        let rule_id = self.begin_rule();
        let mut i = self.index();
        let mut cli = self.character_in_line_index();
        let mut li = self.line_index();

        for ch in value.chars() {
            if self.chars.get(i).copied() != Some(ch) {
                return self.fail_rule(rule_id);
            }
            if ch == '\n' {
                li += 1;
                cli = 0;
            } else {
                cli += 1;
            }
            i += 1;
        }

        self.set_index(i);
        self.set_line_index(li);
        self.set_character_in_line_index(cli);

        self.succeed_rule(rule_id, Some(value.to_owned()))
    }

    pub fn parse_single_character(&mut self) -> Option<char> {
        let ch = self.current_character()?;
        if ch == '\n' {
            self.set_line_index(self.line_index() + 1);
            self.set_character_in_line_index(0);
        } else {
            self.set_character_in_line_index(self.character_in_line_index() + 1);
        }
        self.set_index(self.index() + 1);
        Some(ch)
    }

    pub fn parse_until_characters_from_string(
        &mut self,
        chars: &str,
        max_count: isize,
    ) -> Option<String> {
        self.parse_characters_from_char_set(
            &CharacterSet::from(chars),
            false,
            max_count,
        )
    }

    pub fn parse_characters_from_string(
        &mut self,
        chars: &str,
        should_include_chars: bool,
        max_count: isize,
    ) -> Option<String> {
        self.parse_characters_from_char_set(
            &CharacterSet::from(chars),
            should_include_chars,
            max_count,
        )
    }

    pub fn parse_characters_from_char_set(
        &mut self,
        char_set: &CharacterSet,
        should_include_chars: bool,
        max_count: isize,
    ) -> Option<String> {
        let max_count = if max_count < 0 {
            usize::MAX
        } else {
            max_count as usize
        };

        let start_index = self.index();
        let mut i = self.index();
        let mut cli = self.character_in_line_index();
        let mut li = self.line_index();
        let mut count = 0;

        while i < self.chars.len()
            && char_set.contains(self.chars[i]) == should_include_chars
            && count < max_count
        {
            if self.chars[i] == '\n' {
                li += 1;
                cli = 0;
            } else {
                cli += 1;
            }
            i += 1;
            count += 1;
        }

        self.set_index(i);
        self.set_line_index(li);
        self.set_character_in_line_index(cli);

        if i > start_index {
            Some(self.chars[start_index..i].iter().collect())
        } else {
            None
        }
    }

    pub fn peek<T>(&mut self, mut rule: impl FnMut(&mut Self) -> Option<T>) -> Option<T> {
        let rule_id = self.begin_rule();
        let result = rule(self);
        self.cancel_rule(rule_id);
        result
    }

    pub fn parse_until<T>(
        &mut self,
        mut stop_rule: impl FnMut(&mut Self) -> Option<T>,
        pause_characters: Option<&CharacterSet>,
        end_characters: Option<&CharacterSet>,
    ) -> Option<String> {
        let rule_id = self.begin_rule();

        let mut pause_and_end = CharacterSet::new();
        if let Some(chars) = pause_characters {
            pause_and_end.union_with(chars);
        }
        if let Some(chars) = end_characters {
            pause_and_end.union_with(chars);
        }

        let mut parsed = String::new();

        loop {
            if let Some(partial) =
                self.parse_characters_from_char_set(&pause_and_end, false, -1)
            {
                parsed.push_str(&partial);
            }

            if self.peek(|parser| stop_rule(parser)).is_some() {
                break;
            }

            if self.end_of_input() {
                break;
            }

            let pause_character = self.current_character()?;
            if pause_characters.is_some_and(|chars| chars.contains(pause_character)) {
                parsed.push(pause_character);
                let _ = self.parse_single_character();
                continue;
            }

            break;
        }

        if parsed.is_empty() {
            self.fail_rule(rule_id)
        } else {
            self.succeed_rule(rule_id, Some(parsed))
        }
    }

    pub fn parse_int(&mut self) -> Option<i32> {
        let old_index = self.index();
        let old_cli = self.character_in_line_index();

        let negative = self.parse_string("-").is_some();
        let _ = self.parse_characters_from_string(" \t", true, -1);
        let parsed = self.parse_characters_from_char_set(&Self::numbers_character_set(), true, -1)?;

        let parsed_int = parsed.parse::<i32>().ok()?;
        Some(if negative { -parsed_int } else { parsed_int })
            .or_else(|| {
                self.set_index(old_index);
                self.set_character_in_line_index(old_cli);
                None
            })
    }

    pub fn parse_float(&mut self) -> Option<f32> {
        let old_index = self.index();
        let old_cli = self.character_in_line_index();

        let leading_int = self.parse_int()?;
        if self.parse_string(".").is_some() {
            let after_decimal =
                self.parse_characters_from_char_set(&Self::numbers_character_set(), true, -1);
            let value = format!("{leading_int}.{}", after_decimal.unwrap_or_default());
            return value.parse::<f32>().ok();
        }

        self.set_index(old_index);
        self.set_character_in_line_index(old_cli);
        None
    }

    pub fn parse_newline(&mut self) -> Option<String> {
        let rule_id = self.begin_rule();
        let _ = self.parse_string("\r");
        if self.parse_string("\n").is_none() {
            self.fail_rule(rule_id)
        } else {
            self.succeed_rule(rule_id, Some("\n".to_owned()))
        }
    }

    pub fn had_error(&self) -> bool {
        self.had_error
    }

    pub fn set_had_error(&mut self, value: bool) {
        self.had_error = value;
    }

    pub fn numbers_character_set() -> CharacterSet {
        CharacterSet::from("0123456789")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Interleaved<TA, TB> {
    A(TA),
    B(TB),
}

#[cfg(test)]
mod tests {
    use super::{CharacterSet, CommentEliminator, Interleaved, StringParser};

    #[test]
    fn state_tracks_nested_rules() {
        let mut parser = StringParser::new("abc");
        let root_stack = parser.state().stack_height();
        let rule = parser.begin_rule();
        assert_eq!(root_stack + 1, parser.state().stack_height());
        assert_eq!(Some("a".to_owned()), parser.parse_string("a"));
        parser.cancel_rule(rule);
        assert_eq!(root_stack, parser.state().stack_height());
        assert_eq!(0, parser.index());
    }

    #[test]
    fn parse_string_updates_line_tracking() {
        let mut parser = StringParser::new("a\nb");
        assert_eq!(Some("a\n".to_owned()), parser.parse_string("a\n"));
        assert_eq!(2, parser.index());
        assert_eq!(1, parser.line_index());
        assert_eq!(0, parser.character_in_line_index());
        assert_eq!(Some('b'), parser.current_character());
    }

    #[test]
    fn parse_until_respects_pause_and_stop_rule() {
        let mut parser = StringParser::new("alpha/*comment*/omega");
        let pause = CharacterSet::from("/");
        let stop = |parser: &mut StringParser| parser.parse_string("/*");
        assert_eq!(
            Some("alpha".to_owned()),
            parser.parse_until(stop, Some(&pause), None)
        );
        assert_eq!(Some("/*".to_owned()), parser.parse_string("/*"));
    }

    #[test]
    fn interleave_collects_alternating_results() {
        let mut parser = StringParser::new("a,b,c");
        let results = parser.interleave(
            |parser| parser.parse_characters_from_char_set(&CharacterSet::from("abc"), true, 1),
            |parser| parser.parse_string(","),
        );
        assert_eq!(
            vec![
                Interleaved::A("a".to_owned()),
                Interleaved::B(",".to_owned()),
                Interleaved::A("b".to_owned()),
                Interleaved::B(",".to_owned()),
                Interleaved::A("c".to_owned()),
            ],
            results
        );
    }

    #[test]
    fn comment_eliminator_preserves_line_numbers() {
        let input = "A\r\n/* one\r\ntwo */\r\nB // tail\r\n";
        let processed = CommentEliminator::process(input);
        assert_eq!("A\n\n\nB \n", processed);
    }
}
