use std::path::Path;

use crate::{
    bootstrap::{legacy_root::parse_story_with_includes, parser::Parser as LegacyParser},
    error::CompilerError,
    parsed_hierarchy::{
        ContentList, DebugMetadata, ExternalDeclaration, FlowArgument, FlowDecl, List,
        ListDefinition, ListElementDefinition, Number, NumberValue, Return as ParsedReturn,
        SequenceType, Story, Tag, VariableAssignment,
    },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceTypeAnnotation {
    pub flags: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DivertPrototype {
    pub target: Vec<String>,
    pub arguments: Vec<String>,
    pub is_thread: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandLineInput {
    pub is_help: bool,
    pub is_exit: bool,
    pub choice_input: Option<i32>,
    pub debug_source: Option<i32>,
    pub debug_path_lookup: Option<String>,
    pub user_immediate_mode_statement: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InkParser {
    source_name: Option<String>,
    parser: StringParser,
    parsing_string_expression: bool,
    parsing_choice: bool,
    tag_active: bool,
}

impl InkParser {
    pub fn new(input: impl Into<String>, source_name: Option<String>) -> Self {
        let processed = CommentEliminator::process(input.into());
        Self {
            source_name,
            parser: StringParser::new(processed),
            parsing_string_expression: false,
            parsing_choice: false,
            tag_active: false,
        }
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source_name.as_deref()
    }

    pub fn parse_story(&self, count_all_visits: bool) -> Result<Story, CompilerError> {
        let legacy_story = LegacyParser::new(self.parser.input_string()).parse()?;
        let mut story = Story::new(
            self.parser.input_string(),
            self.source_name.clone(),
            count_all_visits,
        );
        story.populate_from_legacy(&legacy_story);
        Ok(story)
    }

    pub fn parse_story_with_file_handler<F>(
        &self,
        count_all_visits: bool,
        file_handler: F,
    ) -> Result<Story, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        let source_name = self.source_name.as_deref().unwrap_or("<source>");
        let legacy_story = parse_story_with_includes(
            self.parser.input_string(),
            &file_handler,
            Path::new(""),
            source_name,
            0,
        )?;
        let mut story = Story::new(
            self.parser.input_string(),
            self.source_name.clone(),
            count_all_visits,
        );
        story.populate_from_legacy(&legacy_story);
        Ok(story)
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

    pub fn spaced<T>(&mut self, mut rule: impl FnMut(&mut StringParser) -> Option<T>) -> Option<T> {
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
            .map(|_| {
                Number::new(NumberValue::Float(
                    self.parser.parse_float().expect("peeked float"),
                ))
            })
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

    pub fn start_tag(&mut self) -> Option<Tag> {
        let _ = self.whitespace();
        self.parser.parse_string("#")?;
        let was_active = self.tag_active;
        self.tag_active = true;
        let _ = self.whitespace();
        Some(Tag::new(true, self.parsing_choice || was_active))
    }

    pub fn end_tag_if_necessary(&mut self) -> Option<Tag> {
        if !self.tag_active {
            return None;
        }

        self.tag_active = false;
        Some(Tag::new(false, self.parsing_choice))
    }

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
                Some(ParseSuccess)
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

            let argument = self
                .parser
                .parse_until_characters_from_string(",)", -1)?
                .trim()
                .to_owned();
            arguments.push(argument);
            let _ = self.whitespace();

            if self.parser.parse_string(")").is_some() {
                break;
            }
            self.parser.parse_string(",")?;
        }

        self.parser.succeed_rule(rule_id, Some(arguments))
    }

    pub fn divert_identifier_with_arguments(&mut self) -> Option<DivertPrototype> {
        let _ = self.whitespace();
        let target = self.dot_separated_divert_path_components()?;
        let _ = self.whitespace();
        let arguments = self
            .expression_function_call_arguments()
            .unwrap_or_default();
        let _ = self.whitespace();

        Some(DivertPrototype {
            target,
            arguments,
            is_thread: false,
        })
    }

    pub fn start_thread(&mut self) -> Option<DivertPrototype> {
        let _ = self.whitespace();
        self.parse_thread_arrow()?;
        let _ = self.whitespace();
        let mut divert = self.divert_identifier_with_arguments()?;
        divert.is_thread = true;
        Some(divert)
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

            let argument = self.flow_decl_argument()?;
            arguments.push(argument);
            let _ = self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }
            self.parser.parse_string(",")?;
        }

        self.parser.succeed_rule(rule_id, Some(arguments))
    }

    pub fn knot_declaration(&mut self) -> Option<FlowDecl> {
        let rule_id = self.parser.begin_rule();
        let equals = self.parser.parse_characters_from_string("=", true, -1)?;
        if equals.len() <= 1 {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let identifier = self.parse_identifier()?;
        let (is_function, name) = if identifier == "function" {
            let _ = self.whitespace();
            (true, self.parse_identifier()?)
        } else {
            (false, identifier)
        };

        let _ = self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();
        let _ = self.whitespace();
        let _ = self.parser.parse_characters_from_string("=", true, -1);

        self.parser.succeed_rule(
            rule_id,
            Some(FlowDecl {
                name,
                arguments,
                is_function,
            }),
        )
    }

    pub fn stitch_declaration(&mut self) -> Option<FlowDecl> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        self.parser.parse_string("=")?;
        if self.parser.parse_string("=").is_some() {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let is_function = self.parser.parse_string("function").is_some();
        if is_function {
            let _ = self.whitespace();
        }
        let name = self.parse_identifier()?;
        let _ = self.whitespace();
        let arguments = self.bracketed_knot_decl_arguments().unwrap_or_default();

        self.parser.succeed_rule(
            rule_id,
            Some(FlowDecl {
                name,
                arguments,
                is_function,
            }),
        )
    }

    pub fn external_declaration(&mut self) -> Option<ExternalDeclaration> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let keyword = self.parse_identifier()?;
        if keyword != "EXTERNAL" {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let name = self.parse_identifier()?;
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
        let keyword = self.parse_identifier()?;
        if keyword != "LIST" {
            return self.parser.fail_rule(rule_id);
        }

        let _ = self.whitespace();
        let var_name = self.parse_identifier()?;
        let _ = self.whitespace();
        if self.parser.parse_string("=").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();

        let mut definition = self.list_definition()?;
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
        let name = self.parse_identifier()?;
        let _ = self.whitespace();

        if in_initial_list && self.parser.parse_string(")").is_some() {
            needs_to_close_paren = false;
            let _ = self.whitespace();
        }

        let mut explicit_value = None;
        if self.parser.parse_string("=").is_some() {
            let _ = self.whitespace();
            let number = self.parse_number_literal()?;
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
            Some(ListElementDefinition::new(
                name,
                in_initial_list,
                explicit_value,
            )),
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
            let path = self.dot_separated_divert_path_components()?;
            items.push(path.join("."));
            let _ = self.whitespace();

            if self.parser.parse_string(")").is_some() {
                break;
            }
            self.parser.parse_string(",")?;
            let _ = self.whitespace();
        }

        self.parser
            .succeed_rule(rule_id, Some(List::new(Some(items))))
    }

    pub fn command_line_user_input(&mut self) -> Option<CommandLineInput> {
        let _ = self.whitespace();

        if self.parser.parse_string("help").is_some() {
            return Some(CommandLineInput {
                is_help: true,
                ..Default::default()
            });
        }

        if self.parser.parse_string("exit").is_some() || self.parser.parse_string("quit").is_some()
        {
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
        self.parser.parse_string("(")?;
        let _ = self.whitespace();
        let offset = self.parser.parse_int()?;
        let _ = self.whitespace();
        self.parser.parse_string(")")?;

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
        self.whitespace()?;
        let path = self.parser.parse_characters_from_char_set(
            &self.runtime_path_character_set(),
            true,
            -1,
        )?;

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
        let number = self.parser.parse_int()?;
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

    pub fn include_statement_filename(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        let _ = self.whitespace();
        let keyword = self.parse_identifier()?;
        if keyword != "INCLUDE" {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let filename = self
            .parser
            .parse_until_characters_from_string("\n\r", -1)?
            .trim_end_matches([' ', '\t'])
            .to_owned();
        self.parser.succeed_rule(rule_id, Some(filename))
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

        if out.is_empty() { None } else { Some(out) }
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

    fn runtime_path_character_set(&self) -> CharacterSet {
        self.identifier_character_set()
            .add_characters(['-', '.'].into_iter())
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

#[cfg(test)]
mod tests {
    use super::InkParser;
    use crate::{
        parsed_hierarchy::{Content, NumberValue},
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
        assert_eq!(
            Some("->".to_owned()),
            parser.parser_mut().parse_string("->")
        );
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

    #[test]
    fn number_and_bool_literals_match_reference_shapes() {
        let mut ints = InkParser::new("12", None);
        assert!(matches!(
            ints.parse_number_literal().map(|n| n.value().clone()),
            Some(NumberValue::Int(12))
        ));

        let mut floats = InkParser::new("12.5", None);
        assert!(matches!(
            floats.parse_number_literal().map(|n| n.value().clone()),
            Some(NumberValue::Float(value)) if (value - 12.5).abs() < f32::EPSILON
        ));

        let mut bools = InkParser::new("true", None);
        assert!(matches!(
            bools.parse_bool_literal().map(|n| n.value().clone()),
            Some(NumberValue::Bool(true))
        ));
    }

    #[test]
    fn sequence_type_annotations_support_symbol_and_word_forms() {
        let mut symbols = InkParser::new("!~", None);
        assert_eq!(
            Some((super::SequenceType::Once as u8) | (super::SequenceType::Shuffle as u8)),
            symbols.sequence_type_symbol_annotation().map(|a| a.flags)
        );

        let mut words = InkParser::new("once shuffle:", None);
        assert_eq!(
            Some((super::SequenceType::Once as u8) | (super::SequenceType::Shuffle as u8)),
            words.sequence_type_word_annotation().map(|a| a.flags)
        );
    }

    #[test]
    fn start_and_end_tag_track_active_state() {
        let mut parser = InkParser::new("# tag", None);
        let start = parser.start_tag().expect("start tag");
        assert!(start.is_start());
        let end = parser.end_tag_if_necessary().expect("end tag");
        assert!(!end.is_start());
        assert!(parser.end_tag_if_necessary().is_none());
    }

    #[test]
    fn parses_divert_shapes_and_thread_starts() {
        let mut parser = InkParser::new("knot.stitch(arg, 2)", None);
        let divert = parser
            .divert_identifier_with_arguments()
            .expect("divert with args");
        assert_eq!(vec!["knot".to_owned(), "stitch".to_owned()], divert.target);
        assert_eq!(vec!["arg".to_owned(), "2".to_owned()], divert.arguments);

        let mut thread = InkParser::new("<- worker()", None);
        let divert = thread.start_thread().expect("thread");
        assert!(divert.is_thread);
    }

    #[test]
    fn parses_flow_decl_arguments_and_headers() {
        let mut arg = InkParser::new("ref -> target", None);
        let parsed = arg.flow_decl_argument().expect("arg");
        assert!(parsed.is_by_reference);
        assert!(parsed.is_divert_target);
        assert_eq!("target", parsed.identifier);

        let mut knot = InkParser::new("=== function my_knot(ref x, -> target) ===", None);
        let parsed = knot.knot_declaration().expect("knot");
        assert!(parsed.is_function);
        assert_eq!("my_knot", parsed.name);
        assert_eq!(2, parsed.arguments.len());

        let mut stitch = InkParser::new("= place(ref x)", None);
        let parsed = stitch.stitch_declaration().expect("stitch");
        assert_eq!("place", parsed.name);
        assert_eq!(1, parsed.arguments.len());
    }

    #[test]
    fn parses_external_include_and_return_headers() {
        let mut external = InkParser::new("EXTERNAL my_func(x, y)", None);
        let declaration = external.external_declaration().expect("external");
        assert_eq!("my_func", declaration.name());
        assert_eq!(
            &["x".to_owned(), "y".to_owned()],
            declaration.argument_names()
        );

        let mut include = InkParser::new("INCLUDE path/to/file.ink\n", None);
        assert_eq!(
            Some("path/to/file.ink".to_owned()),
            include.include_statement_filename()
        );

        let mut ret = InkParser::new("return\n", None);
        assert!(ret.return_statement().is_some());
    }

    #[test]
    fn parses_list_definitions_and_literals() {
        let mut definition = InkParser::new("a, (b = 5), c", None);
        let list_definition = definition.list_definition().expect("list definition");
        let items = list_definition.item_definitions();
        assert_eq!("a", items[0].name());
        assert_eq!(1, items[0].series_value());
        assert_eq!("b", items[1].name());
        assert!(items[1].in_initial_list());
        assert_eq!(Some(5), items[1].explicit_value());
        assert_eq!(6, items[2].series_value());

        let mut literal = InkParser::new("(alpha, things.beta)", None);
        let list = literal.list_expression().expect("list literal");
        assert_eq!(
            Some(&["alpha".to_owned(), "things.beta".to_owned()][..]),
            list.item_identifier_list()
        );

        let mut empty = InkParser::new("()", None);
        assert!(empty.list_expression().expect("empty list").is_empty());
    }

    #[test]
    fn parses_command_line_input_shapes() {
        let mut help = InkParser::new("help", None);
        assert!(help.command_line_user_input().expect("help").is_help);

        let mut choice = InkParser::new("3\n", None);
        assert_eq!(
            Some(3),
            choice
                .command_line_user_input()
                .expect("choice")
                .choice_input
        );

        let mut debug_source = InkParser::new("DebugSource(12)", None);
        assert_eq!(
            Some(12),
            debug_source
                .command_line_user_input()
                .expect("debug source")
                .debug_source
        );

        let mut debug_path = InkParser::new("DebugPath knot.0.c-0", None);
        assert_eq!(
            Some("knot.0.c-0".to_owned()),
            debug_path
                .command_line_user_input()
                .expect("debug path")
                .debug_path_lookup
        );
    }
}
