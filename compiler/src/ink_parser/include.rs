use std::path::Path;

use crate::file_handler::FileHandler;

use super::{InkParser, ParseSection, StatementLevel};

impl<'fh> InkParser<'fh> {
    pub fn include_statement_filename(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let Some(keyword) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if keyword != "INCLUDE" {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let Some(filename) = self
            .parser
            .parse_until_characters_from_string("\n\r", -1)
            .map(|value| value.trim_end_matches([' ', '\t']).to_owned())
        else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(rule_id, Some(filename))
    }

    pub(super) fn try_parse_include_statement_line(&mut self) -> Option<ParseSection> {
        let rule_id = self.parser.begin_rule();
        let Some(filename) = self.include_statement_filename() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.end_of_line();
        self.parser.succeed_rule(rule_id, Some(()));

        let resolved_filename = self
            .source_name
            .as_deref()
            .and_then(|source_name| Path::new(source_name).parent())
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| parent.join(&filename).to_string_lossy().into_owned())
            .unwrap_or_else(|| filename.clone());
        let full_filename = self.file_handler.resolve_ink_filename(&resolved_filename);
        if self.open_filenames.borrow().contains(&full_filename) {
            return Some(ParseSection::default());
        }

        self.open_filenames
            .borrow_mut()
            .insert(full_filename.clone());

        let included = self.file_handler.load_ink_file_contents(&full_filename).ok()?;
        let mut parser = InkParser::new_with_file_handler(
            included,
            Some(resolved_filename),
            self.file_handler.clone(),
            self.open_filenames.clone(),
        );
        let parsed = parser.statements_at_level(StatementLevel::Top);

        self.open_filenames.borrow_mut().remove(&full_filename);
        Some(parsed)
    }
}
