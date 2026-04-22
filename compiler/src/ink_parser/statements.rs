use super::{InkParser, ParseSection, StatementLevel, merge_parse_section};
use crate::string_parser::ParseSuccess;

impl<'fh> InkParser<'fh> {
    pub(super) fn statements_at_level(&mut self, level: StatementLevel) -> ParseSection {
        let mut parsed = ParseSection::default();

        loop {
            let loop_start = self.parser.index();
            self.multiline_whitespace();
            if self.parser.end_of_input() {
                break;
            }

            if self.statements_break_for_level(level) {
                break;
            }

            if self.statement_at_level(level, &mut parsed) {
                if self.parser.index() == loop_start {
                    if std::env::var_os("BLADEINK_TRACE_PARSER_STALL").is_some() {
                        let line = self.parser.line_index() + 1;
                        eprintln!(
                            "TRACE parser_stall level={level:?} line={line} source={:?}",
                            self.source_name
                        );
                    }
                    self.skip_line();
                }
                continue;
            }

            self.skip_line();
        }

        parsed
    }

    fn statement_at_level(&mut self, level: StatementLevel, parsed: &mut ParseSection) -> bool {
        match self.peek_next_statement_character() {
            Some('=') => {
                if level >= StatementLevel::Top && let Some(knot) = self.try_parse_knot() {
                    parsed.flows.push(knot);
                    return true;
                }

                if level >= StatementLevel::Knot && let Some(stitch) = self.try_parse_stitch() {
                    parsed.flows.push(stitch);
                    return true;
                }

                return false;
            }
            Some('*' | '+') => {
                if let Some(choice) = self.try_parse_choice() {
                    parsed.nodes.push(choice);
                    return true;
                }
            }
            Some('~') => {
                if let Some(logic) = self.try_parse_logic_line() {
                    parsed.nodes.push(logic);
                    return true;
                }
                return false;
            }
            Some('-') => {
                if let Some(mut line) = self.try_parse_divert_line() {
                    parsed.nodes.append(&mut line);
                    return true;
                }

                if level > StatementLevel::InnerBlock && let Some(gather) = self.try_parse_gather() {
                    parsed.nodes.push(gather);
                    return true;
                }
            }
            Some(ch) if ch.is_alphabetic() => {
                match self.peek_statement_keyword().as_deref() {
                    Some("VAR") => {
                        if level == StatementLevel::Top {
                            if let Some((declaration, initializer)) = self.try_parse_global_declaration_statement() {
                                parsed.global_declarations.push(declaration);
                                parsed.global_initializers.push(initializer);
                                return true;
                            }
                        } else if let Some(variable_declaration) = self.try_parse_variable_declaration_line() {
                            parsed.nodes.push(variable_declaration);
                            return true;
                        }
                    }
                    Some("CONST") => {
                        if level == StatementLevel::Top
                            && let Some(const_declaration) = self.try_parse_const_declaration_statement()
                        {
                            parsed.const_declarations.push(const_declaration);
                            return true;
                        }
                    }
                    Some("LIST") => {
                        if let Some(definition) = self.try_parse_list_statement() {
                            parsed.list_definitions.push(definition);
                            return true;
                        }
                    }
                    Some("EXTERNAL") => {
                        if let Some(external) = self.try_parse_external_declaration_line() {
                            parsed.external_declarations.push(external);
                            return true;
                        }
                    }
                    Some("INCLUDE") => {
                        if let Some(included) = self.try_parse_include_statement_line() {
                            merge_parse_section(parsed, included);
                            return true;
                        }
                    }
                    Some("TODO") => {
                        if let Some(author_warning) = self.try_parse_author_warning_line() {
                            parsed.nodes.push(author_warning);
                            return true;
                        }
                    }
                    _ => {
                        if let Some(mut line) = self.try_parse_mixed_line() {
                            parsed.nodes.append(&mut line);
                            return true;
                        }
                        return false;
                    }
                }
            }
            _ => {}
        }

        if let Some(mut line) = self.try_parse_divert_line() {
            parsed.nodes.append(&mut line);
            return true;
        }

        if level >= StatementLevel::Top && let Some(knot) = self.try_parse_knot() {
            parsed.flows.push(knot);
            return true;
        }

        if let Some(choice) = self.try_parse_choice() {
            parsed.nodes.push(choice);
            return true;
        }

        if let Some(author_warning) = self.try_parse_author_warning_line() {
            parsed.nodes.push(author_warning);
            return true;
        }

        if level > StatementLevel::InnerBlock && let Some(gather) = self.try_parse_gather() {
            parsed.nodes.push(gather);
            return true;
        }

        if level >= StatementLevel::Knot && let Some(stitch) = self.try_parse_stitch() {
            parsed.flows.push(stitch);
            return true;
        }

        if let Some(definition) = self.try_parse_list_statement() {
            parsed.list_definitions.push(definition);
            return true;
        }

        if level == StatementLevel::Top
            && let Some(const_declaration) = self.try_parse_const_declaration_statement()
        {
            parsed.const_declarations.push(const_declaration);
            return true;
        }

        if level == StatementLevel::Top
            && let Some((declaration, initializer)) = self.try_parse_global_declaration_statement()
        {
            parsed.global_declarations.push(declaration);
            parsed.global_initializers.push(initializer);
            return true;
        }

        if level != StatementLevel::Top
            && let Some(variable_declaration) = self.try_parse_variable_declaration_line()
        {
            parsed.nodes.push(variable_declaration);
            return true;
        }

        if let Some(external) = self.try_parse_external_declaration_line() {
            parsed.external_declarations.push(external);
            return true;
        }

        if let Some(included) = self.try_parse_include_statement_line() {
            merge_parse_section(parsed, included);
            return true;
        }

        if let Some(logic) = self.try_parse_logic_line() {
            parsed.nodes.push(logic);
            return true;
        }

        if let Some(mut line) = self.try_parse_mixed_line() {
            parsed.nodes.append(&mut line);
            return true;
        }

        false
    }

    fn peek_next_statement_character(&mut self) -> Option<char> {
        self.parser.peek(|parser| {
            let _ = parser.parse_characters_from_string(" \t", true, -1);
            parser.parse_single_character()
        })
    }

    fn peek_statement_keyword(&mut self) -> Option<String> {
        self.parser.peek(|parser| {
            let _ = parser.parse_characters_from_string(" \t", true, -1);
            let mut word = String::new();
            while let Some(ch) = parser.current_character() {
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    word.push(parser.parse_single_character()?);
                } else {
                    break;
                }
            }
            (!word.is_empty()).then_some(word)
        })
    }

    fn statements_break_for_level(&mut self, level: StatementLevel) -> bool {
        self.parser
            .peek(|parser| {
                let _ = parser.parse_characters_from_string(" \t", true, -1);

                if level <= StatementLevel::Knot {
                    let checkpoint = parser.begin_rule();
                    let knot_start = parser
                        .parse_characters_from_string("=", true, -1)
                        .is_some_and(|eq| eq.len() > 1);
                    let _ = parser.fail_rule::<ParseSuccess>(checkpoint);
                    if knot_start {
                        return Some(ParseSuccess);
                    }
                }

                if level <= StatementLevel::Stitch {
                    let checkpoint = parser.begin_rule();
                    let stitch_start = if parser.parse_string("=").is_some() {
                        parser.parse_string("=").is_none()
                    } else {
                        false
                    };
                    let _ = parser.fail_rule::<ParseSuccess>(checkpoint);
                    if stitch_start {
                        return Some(ParseSuccess);
                    }
                }

                if level <= StatementLevel::InnerBlock {
                    let checkpoint = parser.begin_rule();
                    let break_on_dash = if parser.parse_string("->").is_none() {
                        parser.parse_single_character() == Some('-')
                    } else {
                        false
                    };
                    let _ = parser.fail_rule::<ParseSuccess>(checkpoint);
                    if break_on_dash || parser.parse_string("}").is_some() {
                        return Some(ParseSuccess);
                    }
                }

                None
            })
            .is_some()
    }

    fn parse_dash_not_arrow(&mut self) -> Option<ParseSuccess> {
        let rule_id = self.parser.begin_rule();

        if self.parser.parse_string("->").is_none() && self.parser.parse_single_character() == Some('-') {
            return self.parser.succeed_rule(rule_id, Some(ParseSuccess));
        }

        self.parser.fail_rule(rule_id)
    }

    fn peek_knot_start(&mut self) -> bool {
        self.parser
            .peek(|p| {
                let _ = p.parse_characters_from_string(" \t", true, -1);
                let eq = p.parse_characters_from_string("=", true, -1)?;
                if eq.len() > 1 { Some(()) } else { None }
            })
            .is_some()
    }
}
