use super::{CharacterSet, StringParser};

#[derive(Debug, Clone)]
pub struct CommentEliminator {
    parser: StringParser,
}

impl CommentEliminator {
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            parser: StringParser::new(input),
        }
    }

    pub fn process(input: impl Into<String>) -> String {
        Self::new(input).run()
    }

    pub fn run(mut self) -> String {
        let mut out = String::new();

        while !self.parser.end_of_input() {
            if self.end_of_line_comment() {
                continue;
            }

            if let Some(block) = self.block_comment() {
                out.push_str(&block);
                continue;
            }

            if let Some(newline) = self.parser.parse_newline() {
                out.push_str(&newline);
                continue;
            }

            if let Some(text) = self.main_ink() {
                out.push_str(&text);
                continue;
            }

            if let Some(ch) = self.parser.parse_single_character() {
                out.push(ch);
            }
        }

        out
    }

    fn main_ink(&mut self) -> Option<String> {
        let stop_chars = CharacterSet::from("/\r\n");
        self.parser
            .parse_characters_from_char_set(&stop_chars, false, -1)
    }

    fn end_of_line_comment(&mut self) -> bool {
        if self.parser.parse_string("//").is_none() {
            return false;
        }

        let newline_chars = CharacterSet::from("\n\r");
        let _ = self
            .parser
            .parse_until_characters_from_string("\n\r", -1)
            .or_else(|| {
                self.parser
                    .parse_characters_from_char_set(&newline_chars, false, -1)
            });
        true
    }

    fn block_comment(&mut self) -> Option<String> {
        if self.parser.parse_string("/*").is_none() {
            return None;
        }

        let start_line = self.parser.line_index();
        let end_marker = CharacterSet::from("*");
        let _ =
            self.parser
                .parse_until(|parser| parser.parse_string("*/"), Some(&end_marker), None);

        if !self.parser.end_of_input() {
            let _ = self.parser.parse_string("*/");
        }

        Some("\n".repeat(self.parser.line_index().saturating_sub(start_line)))
    }
}
