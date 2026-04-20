use crate::{
    parsed_hierarchy::{ContentList, DebugMetadata},
    string_parser::{
        CharacterRange, CharacterSet, CommentEliminator, ParseSuccess, StringParser,
        StringParserStateElement,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceMarker {
    pub indentation_depth: usize,
    pub once_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceClause {
    pub start_text: String,
    pub choice_only_text: Option<String>,
    pub inner_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatherMarker {
    pub indentation_depth: usize,
    pub identifier: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InkParser {
    source_name: Option<String>,
    parser: StringParser,
    parsing_string_expression: bool,
    parsing_choice: bool,
}

impl InkParser {
    pub fn new(input: impl Into<String>, source_name: Option<String>) -> Self {
        let processed = CommentEliminator::process(input.into());
        Self {
            source_name,
            parser: StringParser::new(processed),
            parsing_string_expression: false,
            parsing_choice: false,
        }
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source_name.as_deref()
    }

    pub fn parser(&self) -> &StringParser {
        &self.parser
    }

    pub fn parser_mut(&mut self) -> &mut StringParser {
        &mut self.parser
    }

    pub fn end_of_line(&mut self) -> Option<ParseSuccess> {
        self.newline().or_else(|| self.end_of_file())
    }

    pub fn newline(&mut self) -> Option<ParseSuccess> {
        let _ = self.whitespace();
        self.parser.parse_newline().map(|_| ParseSuccess)
    }

    pub fn end_of_file(&mut self) -> Option<ParseSuccess> {
        let _ = self.whitespace();
        self.parser.end_of_input().then_some(ParseSuccess)
    }

    pub fn multiline_whitespace(&mut self) -> Option<ParseSuccess> {
        let mut count = 0;
        while self.newline().is_some() {
            count += 1;
        }
        (count > 0).then_some(ParseSuccess)
    }

    pub fn whitespace(&mut self) -> Option<ParseSuccess> {
        self.parser
            .parse_characters_from_string(" \t", true, -1)
            .map(|_| ParseSuccess)
    }

    pub fn any_whitespace(&mut self) -> Option<ParseSuccess> {
        let mut found = false;
        loop {
            if self.whitespace().is_some() || self.multiline_whitespace().is_some() {
                found = true;
            } else {
                break;
            }
        }
        found.then_some(ParseSuccess)
    }

    pub fn spaced<T>(
        &mut self,
        mut rule: impl FnMut(&mut StringParser) -> Option<T>,
    ) -> Option<T> {
        let _ = self.whitespace();
        let result = self.parser.parse_object(|parser| rule(parser))?;
        let _ = self.whitespace();
        Some(result)
    }

    pub fn multispaced<T>(
        &mut self,
        mut rule: impl FnMut(&mut StringParser) -> Option<T>,
    ) -> Option<T> {
        let _ = self.any_whitespace();
        let result = self.parser.parse_object(|parser| rule(parser))?;
        let _ = self.any_whitespace();
        Some(result)
    }

    pub fn create_debug_metadata(
        &self,
        start: &StringParserStateElement,
        end: &StringParserStateElement,
    ) -> DebugMetadata {
        DebugMetadata {
            start_line_number: start.line_index() + 1,
            end_line_number: end.line_index() + 1,
            start_character_number: start.character_in_line_index() + 1,
            end_character_number: end.character_in_line_index() + 1,
            file_name: self.source_name.clone(),
        }
    }

    pub fn parse_identifier(&mut self) -> Option<String> {
        let first = self
            .parser
            .parse_characters_from_char_set(&self.identifier_character_set(), true, 1)?;
        let rest = self
            .parser
            .parse_characters_from_char_set(&self.identifier_character_set(), true, -1)
            .unwrap_or_default();
        Some(format!("{first}{rest}"))
    }

    pub fn bracketed_name(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let identifier = self.parse_identifier();
        if identifier.is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        if self.parser.parse_string(")").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.parser.succeed_rule(rule_id, identifier)
    }

    pub fn choice_marker(&mut self) -> Option<ChoiceMarker> {
        let _ = self.whitespace();
        let marker = self.parser.parse_single_character()?;
        let once_only = match marker {
            '*' => true,
            '+' => false,
            _ => return None,
        };

        let mut indentation_depth = 1;
        loop {
            if self
                .parser
                .parse_object(|parser| {
                    let _ = parser.parse_characters_from_string(" \t", true, -1);
                    match parser.parse_single_character() {
                        Some('*') | Some('+') => Some(ParseSuccess),
                        _ => None,
                    }
                })
                .is_some()
            {
                indentation_depth += 1;
            } else {
                break;
            }
        }

        Some(ChoiceMarker {
            indentation_depth,
            once_only,
        })
    }

    pub fn gather_marker(&mut self) -> Option<GatherMarker> {
        let _ = self.whitespace();
        let mut indentation_depth = 0;

        loop {
            if self
                .parser
                .parse_object(|parser| {
                    let _ = parser.parse_characters_from_string(" \t", true, -1);
                    if parser.parse_string("->").is_some() {
                        return None;
                    }
                    parser.parse_string("-").map(|_| ParseSuccess)
                })
                .is_some()
            {
                indentation_depth += 1;
                let _ = self.whitespace();
            } else {
                break;
            }
        }

        if indentation_depth == 0 {
            return None;
        }

        let identifier = self.bracketed_name();
        let _ = self.whitespace();

        Some(GatherMarker {
            indentation_depth,
            identifier,
        })
    }

    pub fn split_choice_clause(text: &str) -> ChoiceClause {
        let mut start_text = text.to_owned();
        let mut choice_only_text = None;
        let mut inner_text = String::new();

        if let Some(open) = text.find('[')
            && let Some(close) = text[open + 1..].find(']')
        {
            let close = open + 1 + close;
            start_text = text[..open].to_owned();
            choice_only_text = Some(text[open + 1..close].to_owned());
            inner_text = text[close + 1..].to_owned();
        }

        ChoiceClause {
            start_text,
            choice_only_text,
            inner_text,
        }
    }

    pub fn content_text(&mut self) -> Option<String> {
        self.content_text_allowing_escape_char()
    }

    pub fn content_text_allowing_escape_char(&mut self) -> Option<String> {
        let mut out = String::new();

        loop {
            let part = self.content_text_no_escape();
            let got_escape = self.parser.parse_string("\\").is_some();
            let had_part = part.is_some();

            if let Some(part) = part {
                out.push_str(&part);
            }

            if got_escape {
                if let Some(ch) = self.parser.parse_single_character() {
                    out.push(ch);
                }
            }

            if !had_part && !got_escape {
                break;
            }
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    pub fn line_of_mixed_text(&mut self) -> Option<ContentList> {
        let _ = self.whitespace();
        let text = self.content_text()?;
        let mut list = ContentList::new();
        list.push_text(text);
        list.trim_trailing_whitespace();
        list.push_text("\n");
        let _ = self.end_of_line();
        Some(list)
    }

    fn content_text_no_escape(&mut self) -> Option<String> {
        let pause_characters = CharacterSet::from("-<");

        let mut end_characters = CharacterSet::from("{}|\n\r\\#");
        if self.parsing_choice {
            end_characters = end_characters.add_characters("[]".chars());
        }
        if self.parsing_string_expression {
            end_characters = end_characters.add_characters("\"".chars());
        }

        self.parser.parse_until(
            |parser| {
                parser
                    .parse_string("->")
                    .or_else(|| parser.parse_string("<-"))
                    .or_else(|| parser.parse_string("<>"))
                    .or_else(|| {
                        if parser.parse_newline().is_some() {
                            Some(String::new())
                        } else {
                            None
                        }
                    })
            },
            Some(&pause_characters),
            Some(&end_characters),
        )
    }

    fn identifier_character_set(&self) -> CharacterSet {
        let mut set = CharacterSet::from("0123456789_");
        for mut range in Self::list_all_character_ranges() {
            set = set.add_characters(range.to_character_set().into_iter());
        }
        set
    }

    fn list_all_character_ranges() -> Vec<CharacterRange> {
        vec![
            CharacterRange::define('\u{0041}', '\u{007A}', "[\\]^_`".chars()),
            CharacterRange::define('\u{0100}', '\u{017F}', [].into_iter()),
            CharacterRange::define('\u{0180}', '\u{024F}', [].into_iter()),
            CharacterRange::define(
                '\u{0370}',
                '\u{03FF}',
                "\u{0374}\u{0375}\u{0378}\u{0387}\u{038B}\u{038D}\u{03A2}"
                    .chars()
                    .chain('\u{0378}'..='\u{0385}'),
            ),
            CharacterRange::define(
                '\u{0400}',
                '\u{04FF}',
                '\u{0482}'..='\u{0489}',
            ),
            CharacterRange::define(
                '\u{0530}',
                '\u{058F}',
                "\u{0530}".chars()
                    .chain('\u{0557}'..='\u{0560}')
                    .chain('\u{0588}'..='\u{058E}'),
            ),
            CharacterRange::define('\u{0590}', '\u{05FF}', [].into_iter()),
            CharacterRange::define('\u{0600}', '\u{06FF}', [].into_iter()),
            CharacterRange::define('\u{AC00}', '\u{D7AF}', [].into_iter()),
            CharacterRange::define('\u{0080}', '\u{00FF}', [].into_iter()),
            CharacterRange::define('\u{4E00}', '\u{9FFF}', [].into_iter()),
            CharacterRange::define('\u{3041}', '\u{3096}', [].into_iter()),
            CharacterRange::define('\u{30A0}', '\u{30FC}', [].into_iter()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::InkParser;
    use crate::{
        parsed_hierarchy::Content,
        string_parser::{ParseSuccess, StringParserStateElement},
    };

    #[test]
    fn ink_parser_preprocesses_comments_and_line_endings() {
        let parser = InkParser::new("A\r\n/*x*/\r\nB // tail\r\n", Some("test.ink".to_owned()));
        assert_eq!("A\n\nB \n", parser.parser().input_string());
        assert_eq!(Some("test.ink"), parser.source_name());
    }

    #[test]
    fn whitespace_helpers_match_reference_shape() {
        let mut parser = InkParser::new(" \t\n\nx", None);
        assert_eq!(Some(ParseSuccess), parser.whitespace());
        assert_eq!(Some(ParseSuccess), parser.multiline_whitespace());
        assert_eq!(Some('x'), parser.parser().current_character());
    }

    #[test]
    fn create_debug_metadata_is_one_based() {
        let parser = InkParser::new("abc", Some("a.ink".to_owned()));
        let start = StringParserStateElement::default();
        let end = StringParserStateElement::default();
        let metadata = parser.create_debug_metadata(&start, &end);
        assert_eq!(1, metadata.start_line_number);
        assert_eq!(1, metadata.start_character_number);
        assert_eq!(Some("a.ink".to_owned()), metadata.file_name);
    }

    #[test]
    fn identifier_supports_ascii_and_unicode_ranges() {
        let mut parser = InkParser::new("héllo_世界", None);
        assert_eq!(Some("héllo_世界".to_owned()), parser.parse_identifier());
    }

    #[test]
    fn content_text_honours_escape_and_stops_before_divert() {
        let mut parser = InkParser::new(r#"hello \-> world -> target"#, None);
        assert_eq!(Some("hello -> world ".to_owned()), parser.content_text());
        assert_eq!(Some("->".to_owned()), parser.parser_mut().parse_string("->"));
    }

    #[test]
    fn line_of_mixed_text_trims_inline_whitespace_and_appends_newline() {
        let mut parser = InkParser::new("hello \t\n", None);
        let list = parser.line_of_mixed_text().expect("expected text line");
        let pieces: Vec<&str> = list
            .content()
            .iter()
            .map(|item| match item {
                Content::Text(text) => text.text(),
            })
            .collect();
        assert_eq!(vec!["hello", "\n"], pieces);
    }

    #[test]
    fn bracketed_name_parses_choice_and_gather_labels() {
        let mut parser = InkParser::new("(route_name)", None);
        assert_eq!(Some("route_name".to_owned()), parser.bracketed_name());
    }

    #[test]
    fn choice_marker_supports_sticky_and_nested_depth() {
        let mut parser = InkParser::new(" + + option", None);
        assert_eq!(
            Some(super::ChoiceMarker {
                indentation_depth: 2,
                once_only: false
            }),
            parser.choice_marker()
        );
    }

    #[test]
    fn gather_marker_stops_before_arrow_and_reads_optional_name() {
        let mut parser = InkParser::new(" - (join) continue", None);
        assert_eq!(
            Some(super::GatherMarker {
                indentation_depth: 1,
                identifier: Some("join".to_owned())
            }),
            parser.gather_marker()
        );

        let mut divert = InkParser::new("-> target", None);
        assert_eq!(None, divert.gather_marker());
    }

    #[test]
    fn split_choice_clause_supports_weave_style_inline_brackets() {
        let clause = InkParser::split_choice_clause("\"I am tired[.]\" he said");
        assert_eq!("\"I am tired", clause.start_text);
        assert_eq!(Some(".".to_owned()), clause.choice_only_text);
        assert_eq!("\" he said", clause.inner_text);
    }
}
