use super::{CommandLineInput, InkParser};

impl<'fh> InkParser<'fh> {
    pub fn command_line_user_input(&mut self) -> Option<CommandLineInput> {
        let _ = self.whitespace();

        if self.parser.parse_string("help").is_some() {
            return Some(CommandLineInput {
                is_help: true,
                ..Default::default()
            });
        }

        if self.parser.parse_string("exit").is_some() || self.parser.parse_string("quit").is_some() {
            return Some(CommandLineInput {
                is_exit: true,
                ..Default::default()
            });
        }

        self.debug_source()
            .or_else(|| self.debug_path_lookup())
            .or_else(|| self.user_choice_number())
            .or_else(|| self.user_immediate_mode_statement())
    }

    fn debug_source(&mut self) -> Option<CommandLineInput> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        if self.parser.parse_string("DebugSource").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let Some(offset) = self.parser.parse_int() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.whitespace();
        if self.parser.parse_string(")").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(
            rule_id,
            Some(CommandLineInput {
                debug_source: Some(offset),
                ..Default::default()
            }),
        )
    }

    fn debug_path_lookup(&mut self) -> Option<CommandLineInput> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        if self.parser.parse_string("DebugPath").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        if self.whitespace().is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let Some(path) = self.parser.parse_characters_from_char_set(
            &self.runtime_path_character_set(),
            true,
            -1,
        ) else {
            return self.parser.fail_rule(rule_id);
        };

        self.parser.succeed_rule(
            rule_id,
            Some(CommandLineInput {
                debug_path_lookup: Some(path),
                ..Default::default()
            }),
        )
    }

    fn user_choice_number(&mut self) -> Option<CommandLineInput> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let Some(number) = self.parser.parse_int() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.whitespace();
        if self.end_of_line().is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(
            rule_id,
            Some(CommandLineInput {
                choice_input: Some(number),
                ..Default::default()
            }),
        )
    }

    fn user_immediate_mode_statement(&mut self) -> Option<CommandLineInput> {
        let _ = self.whitespace();
        let statement = self
            .parser
            .parse_until_characters_from_string("\n\r", -1)?
            .trim()
            .to_owned();
        (!statement.is_empty()).then_some(CommandLineInput {
            user_immediate_mode_statement: Some(statement),
            ..Default::default()
        })
    }
}
