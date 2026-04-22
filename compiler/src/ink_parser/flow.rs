use super::{InkParser, StatementLevel};
use crate::parsed_hierarchy::{FlowLevel, ParsedFlow};

impl<'fh> InkParser<'fh> {
    pub(super) fn try_parse_knot(&mut self) -> Option<ParsedFlow> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        match self.parser.parse_characters_from_string("=", true, -1) {
            Some(ref s) if s.len() > 1 => {}
            _ => return self.parser.fail_rule(rule_id),
        }

        self.whitespace();

        let Some(ident) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };

        let (is_function, name) = if ident == "function" {
            self.whitespace();
            let Some(fn_name) = self.parse_identifier() else {
                return self.parser.fail_rule(rule_id);
            };
            (true, fn_name)
        } else {
            (false, ident)
        };

        self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();
        self.whitespace();
        let _ = self.parser.parse_characters_from_string("=", true, -1);
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        let parsed = self.statements_at_level(StatementLevel::Knot);

        Some(ParsedFlow::new(
            name,
            FlowLevel::Knot,
            arguments,
            is_function,
            parsed.nodes,
            parsed.flows,
        ))
    }

    pub(super) fn try_parse_stitch(&mut self) -> Option<ParsedFlow> {
        if !self.peek_stitch_start() {
            return None;
        }

        let rule_id = self.parser.begin_rule();
        self.whitespace();

        let Some(_) = self.parser.parse_string("=") else {
            return self.parser.fail_rule(rule_id);
        };
        if self.parser.parse_string("=").is_some() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let mut is_function = false;
        let kw_rule = self.parser.begin_rule();
        if let Some(kw) = self.parse_identifier() {
            if kw == "function" {
                self.parser.succeed_rule(kw_rule, Some(()));
                self.whitespace();
                is_function = true;
            } else {
                self.parser.fail_rule::<()>(kw_rule);
            }
        } else {
            self.parser.cancel_rule(kw_rule);
        }

        let Some(name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };

        self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        let parsed = self.statements_at_level(StatementLevel::Stitch);

        Some(ParsedFlow::new(
            name,
            FlowLevel::Stitch,
            arguments,
            is_function,
            parsed.nodes,
            Vec::new(),
        ))
    }
}
