use super::{InkParser, parsed_expression_to_expression_node};
use crate::parsed_hierarchy::{
    ConstDeclaration, ExternalDeclaration, FlowArgument, FlowDecl, List, ListDefinition,
    ListElementDefinition, NumberValue, ParsedNode, ParsedNodeKind, Return as ParsedReturn,
    VariableAssignment, ParsedAssignmentMode,
};

impl<'fh> InkParser<'fh> {
    pub fn parse_divert_arrow(&mut self) -> Option<String> {
        self.parser.parse_string("->")
    }

    pub fn parse_thread_arrow(&mut self) -> Option<String> {
        self.parser.parse_string("<-")
    }

    pub fn parse_divert_arrow_or_tunnel_onwards(&mut self) -> Option<String> {
        let mut count = 0;
        while self.parser.parse_string("->").is_some() {
            count += 1;
        }

        match count {
            0 => None,
            1 => Some("->".to_owned()),
            2 => Some("->->".to_owned()),
            _ => Some("->->".to_owned()),
        }
    }

    pub fn dot_separated_divert_path_components(&mut self) -> Option<Vec<String>> {
        let _ = self.whitespace();
        let first = self.parse_identifier()?;
        let _ = self.whitespace();
        let mut components = vec![first];

        while self
            .parser
            .parse_object(|parser| {
                let _ = parser.parse_characters_from_string(" \t", true, -1);
                parser.parse_string(".")?;
                let _ = parser.parse_characters_from_string(" \t", true, -1);
                Some(crate::string_parser::ParseSuccess)
            })
            .is_some()
        {
            components.push(self.parse_identifier()?);
        }

        Some(components)
    }

    pub fn expression_function_call_arguments(&mut self) -> Option<Vec<String>> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let mut arguments = Vec::new();
        loop {
            let _ = self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }

            let argument = self.parse_until_top_level_terminator_set(",)")?.trim().to_owned();
            arguments.push(argument);
            let _ = self.whitespace();

            if self.parser.parse_string(")").is_some() {
                break;
            }
            self.parser.parse_string(",")?;
        }

        self.parser.succeed_rule(rule_id, Some(arguments))
    }

    pub fn flow_decl_argument(&mut self) -> Option<FlowArgument> {
        let first = self.parse_identifier();
        let _ = self.whitespace();
        let divert_arrow = self.parse_divert_arrow();
        let _ = self.whitespace();
        let second = self.parse_identifier();

        if first.is_none() && second.is_none() {
            return None;
        }

        let is_divert_target = divert_arrow.is_some();
        if matches!(first.as_deref(), Some("ref")) {
            return Some(FlowArgument {
                identifier: second.unwrap_or_default(),
                is_by_reference: true,
                is_divert_target,
            });
        }

        Some(FlowArgument {
            identifier: if is_divert_target {
                second.unwrap_or_default()
            } else {
                first.unwrap_or_default()
            },
            is_by_reference: false,
            is_divert_target,
        })
    }

    pub fn bracketed_knot_decl_arguments(&mut self) -> Option<Vec<FlowArgument>> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let mut arguments = Vec::new();
        loop {
            let _ = self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }

            let Some(argument) = self.flow_decl_argument() else {
                return self.parser.fail_rule(rule_id);
            };
            arguments.push(argument);
            let _ = self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }
            if self.parser.parse_string(",").is_none() {
                return self.parser.fail_rule(rule_id);
            }
        }

        self.parser.succeed_rule(rule_id, Some(arguments))
    }

    pub fn knot_declaration(&mut self) -> Option<FlowDecl> {
        let rule_id = self.parser.begin_rule();
        let Some(equals) = self.parser.parse_characters_from_string("=", true, -1) else {
            return self.parser.fail_rule(rule_id);
        };
        if equals.len() <= 1 {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let Some(identifier) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        let (is_function, name) = if identifier == "function" {
            let _ = self.whitespace();
            let Some(name) = self.parse_identifier() else {
                return self.parser.fail_rule(rule_id);
            };
            (true, name)
        } else {
            (false, identifier)
        };

        let _ = self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();
        let _ = self.whitespace();
        let _ = self.parser.parse_characters_from_string("=", true, -1);

        self.parser.succeed_rule(rule_id, Some(FlowDecl { name, arguments, is_function }))
    }

    pub fn stitch_declaration(&mut self) -> Option<FlowDecl> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        if self.parser.parse_string("=").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        if self.parser.parse_string("=").is_some() {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let is_function = self.parser.parse_string("function").is_some();
        if is_function {
            let _ = self.whitespace();
        }
        let Some(name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();

        self.parser.succeed_rule(rule_id, Some(FlowDecl { name, arguments, is_function }))
    }

    pub fn external_declaration(&mut self) -> Option<ExternalDeclaration> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let Some(keyword) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if keyword != "EXTERNAL" {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let Some(name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.whitespace();
        let arguments = self
            .bracketed_knot_decl_arguments()
            .unwrap_or_default()
            .into_iter()
            .map(|arg| arg.identifier)
            .collect();

        self.parser
            .succeed_rule(rule_id, Some(ExternalDeclaration::new(name, arguments)))
    }

    pub fn list_declaration(&mut self) -> Option<VariableAssignment> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let Some(keyword) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if keyword != "LIST" {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let Some(var_name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.whitespace();
        if self.parser.parse_string("=").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();

        let Some(mut definition) = self.list_definition() else {
            return self.parser.fail_rule(rule_id);
        };
        definition.set_identifier(var_name.clone());

        let assignment = VariableAssignment::new(var_name, None);
        self.parser.succeed_rule(rule_id, Some(assignment))
    }

    pub fn list_definition(&mut self) -> Option<ListDefinition> {
        let _ = self.any_whitespace();
        let mut elements = Vec::new();

        let first = self.list_element_definition()?;
        elements.push(first);

        loop {
            let checkpoint = self.parser.begin_rule();
            let _ = self.any_whitespace();
            if self.parser.parse_string(",").is_none() {
                self.parser.cancel_rule(checkpoint);
                break;
            }
            let _ = self.any_whitespace();
            let Some(element) = self.list_element_definition() else {
                return self.parser.fail_rule(checkpoint);
            };
            self.parser.succeed_rule(checkpoint, Some(()));
            elements.push(element);
        }

        Some(ListDefinition::new(elements))
    }

    pub fn list_element_definition(&mut self) -> Option<ListElementDefinition> {
        let rule_id = self.parser.begin_rule();
        let in_initial_list = self.parser.parse_string("(").is_some();
        let mut needs_to_close_paren = in_initial_list;

        let _ = self.whitespace();
        let Some(name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.whitespace();

        if in_initial_list && self.parser.parse_string(")").is_some() {
            needs_to_close_paren = false;
            let _ = self.whitespace();
        }

        let mut explicit_value = None;
        if self.parser.parse_string("=").is_some() {
            let _ = self.whitespace();
            let Some(number) = self.parse_number_literal() else {
                return self.parser.fail_rule(rule_id);
            };
            let NumberValue::Int(value) = number.value() else {
                return self.parser.fail_rule(rule_id);
            };
            explicit_value = Some(*value);

            if needs_to_close_paren {
                let _ = self.whitespace();
                if self.parser.parse_string(")").is_some() {
                    needs_to_close_paren = false;
                }
            }
        }

        if needs_to_close_paren {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(
            rule_id,
            Some(ListElementDefinition::new(name, in_initial_list, explicit_value)),
        )
    }

    pub fn list_expression(&mut self) -> Option<List> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        if self.parser.parse_string(")").is_some() {
            return self.parser.succeed_rule(rule_id, Some(List::new(None)));
        }

        let mut items = Vec::new();
        loop {
            let Some(path) = self.dot_separated_divert_path_components() else {
                return self.parser.fail_rule(rule_id);
            };
            items.push(path.join("."));
            let _ = self.whitespace();

            if self.parser.parse_string(")").is_some() {
                break;
            }
            if self.parser.parse_string(",").is_none() {
                return self.parser.fail_rule(rule_id);
            }
            let _ = self.whitespace();
        }

        self.parser.succeed_rule(rule_id, Some(List::new(Some(items))))
    }

    pub(super) fn try_parse_external_declaration_line(&mut self) -> Option<ExternalDeclaration> {
        let rule_id = self.parser.begin_rule();
        let Some(declaration) = self.external_declaration() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.end_of_line();
        self.parser.succeed_rule(rule_id, Some(declaration))
    }

    pub(super) fn try_parse_list_statement(&mut self) -> Option<ListDefinition> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let Some(keyword) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if keyword != "LIST" {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let Some(var_name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.whitespace();
        if self.parser.parse_string("=").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();

        let Some(mut definition) = self.list_definition() else {
            return self.parser.fail_rule(rule_id);
        };
        definition.set_identifier(var_name.clone());
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(definition))
    }

    pub(super) fn try_parse_global_declaration_statement(
        &mut self,
    ) -> Option<(VariableAssignment, (String, crate::parsed_hierarchy::ParsedExpression))> {
        let rule_id = self.parser.begin_rule();
        let Some(node) = self.try_parse_variable_declaration_line() else {
            return self.parser.fail_rule(rule_id);
        };
        if node.assignment_mode() != Some(ParsedAssignmentMode::GlobalDecl) {
            return self.parser.fail_rule(rule_id);
        }
        let Some(name) = node.assignment_target() else {
            return self.parser.fail_rule(rule_id);
        };
        let Some(expression) = node.expression() else {
            return self.parser.fail_rule(rule_id);
        };
        let mut declaration = VariableAssignment::new(name, None);
        declaration.set_global_declaration(true);
        self.parser
            .succeed_rule(rule_id, Some((declaration, (name.to_owned(), expression.clone()))))
    }

    pub(super) fn try_parse_variable_declaration_line(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();
        if self.parser.parse_string("VAR").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let Some(name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        if self.parser.parse_string("=").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let Some(expression) = self.expression_until_top_level_terminators("\n\r") else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.end_of_line();

        self.parser.succeed_rule(
            rule_id,
            Some(
                ParsedNode::new(ParsedNodeKind::Assignment)
                    .with_assignment(ParsedAssignmentMode::GlobalDecl, name)
                    .with_expression(expression),
            ),
        )
    }

    pub(super) fn try_parse_const_declaration_statement(&mut self) -> Option<ConstDeclaration> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();
        if self.parser.parse_string("CONST").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let Some(name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        if self.parser.parse_string("=").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let Some(expression) = self.expression_until_top_level_terminators("\n\r") else {
            return self.parser.fail_rule(rule_id);
        };
        let expression = parsed_expression_to_expression_node(expression)?;
        let _ = self.end_of_line();

        self.parser
            .succeed_rule(rule_id, Some(ConstDeclaration::new(name, expression)))
    }

    pub fn return_statement(&mut self) -> Option<ParsedReturn> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let keyword = self.parse_identifier()?;
        if keyword != "return" {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        self.parser
            .succeed_rule(rule_id, Some(ParsedReturn::new(None)))
    }
}
