use super::{ChoiceClause, ChoiceMarker, GatherMarker, InkParser, SequenceType, SequenceTypeAnnotation};
use crate::{
    parsed_hierarchy::{Number, NumberValue},
    string_parser::{CharacterRange, CharacterSet, ParseSuccess},
};

impl<'fh> InkParser<'fh> {
    pub fn parse_identifier(&mut self) -> Option<String> {
        let first = self.parser.parse_characters_from_char_set(
            &self.identifier_character_set(),
            true,
            1,
        )?;
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

    pub fn parse_number_literal(&mut self) -> Option<Number> {
        self.parser
            .peek(|parser| parser.parse_float())
            .map(|_| Number::new(NumberValue::Float(self.parser.parse_float().expect("peeked float"))))
            .or_else(|| {
                self.parser
                    .parse_int()
                    .map(|value| Number::new(NumberValue::Int(value)))
            })
    }

    pub fn parse_bool_literal(&mut self) -> Option<Number> {
        let identifier_characters = self.identifier_character_set();
        self.parser.parse_object(|parser| {
            match parser.parse_characters_from_char_set(&identifier_characters, true, -1) {
                Some(word) if word == "true" => Some(Number::new(NumberValue::Bool(true))),
                Some(word) if word == "false" => Some(Number::new(NumberValue::Bool(false))),
                _ => None,
            }
        })
    }

    pub fn sequence_type_symbol_annotation(&mut self) -> Option<SequenceTypeAnnotation> {
        let annotations = self
            .parser
            .parse_characters_from_string("!&~$ ", true, -1)?;

        let mut flags = 0u8;
        for ch in annotations.chars() {
            match ch {
                '!' => flags |= SequenceType::Once as u8,
                '&' => flags |= SequenceType::Cycle as u8,
                '~' => flags |= SequenceType::Shuffle as u8,
                '$' => flags |= SequenceType::Stopping as u8,
                ' ' => {}
                _ => return None,
            }
        }

        (flags != 0).then_some(SequenceTypeAnnotation { flags })
    }

    pub fn sequence_type_word_annotation(&mut self) -> Option<SequenceTypeAnnotation> {
        let rule_id = self.parser.begin_rule();
        let mut flags = 0u8;
        let mut parsed_any = false;

        loop {
            let checkpoint = self.parser.begin_rule();
            let Some(word) = self.parse_identifier() else {
                self.parser.cancel_rule(checkpoint);
                break;
            };

            let flag = match word.as_str() {
                "once" => SequenceType::Once as u8,
                "cycle" => SequenceType::Cycle as u8,
                "shuffle" => SequenceType::Shuffle as u8,
                "stopping" => SequenceType::Stopping as u8,
                _ => {
                    self.parser.fail_rule::<()>(checkpoint);
                    break;
                }
            };
            self.parser.succeed_rule(checkpoint, Some(()));
            parsed_any = true;
            flags |= flag;
            let _ = self.whitespace();
        }

        if !parsed_any || self.parser.parse_string(":").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.parser
            .succeed_rule(rule_id, Some(SequenceTypeAnnotation { flags }))
    }

    pub(super) fn identifier_character_set(&self) -> CharacterSet {
        let mut set = CharacterSet::from("0123456789_");
        for mut range in Self::list_all_character_ranges() {
            set = set.add_characters(range.to_character_set().into_iter());
        }
        set
    }

    pub(super) fn runtime_path_character_set(&self) -> CharacterSet {
        self.identifier_character_set()
            .add_characters(['-', '.'].into_iter())
    }

    pub(super) fn list_all_character_ranges() -> Vec<CharacterRange> {
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
            CharacterRange::define('\u{0400}', '\u{04FF}', '\u{0482}'..='\u{0489}'),
            CharacterRange::define(
                '\u{0530}',
                '\u{058F}',
                "\u{0530}"
                    .chars()
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
