use crate::{
    error::CompilerError,
    file_handler::{DefaultFileHandler, FileHandler},
    parsed_hierarchy::{
        ConstDeclaration, ContentList, ExternalDeclaration, FlowArgument,
        FlowDecl, List, ListDefinition, ListElementDefinition, Number, NumberValue,
        ParsedNode, ParsedNodeKind, Return as ParsedReturn, SequenceType, Story, Tag,
        VariableAssignment,
    },
    string_parser::{
        CharacterRange, CharacterSet, CommentEliminator, ParseSuccess, StringParser,
    },
};
use std::{cell::RefCell, collections::HashSet, path::Path, rc::Rc};

mod author_warning;
mod choices;
mod conditional;
mod content;
mod divert;
mod expressions;
mod flow;
mod gather;
mod include;
mod logic;
mod sequences;
mod shared;
mod statements;
mod whitespace;

use self::shared::{
    ParseSection, build_story_from_section, merge_parse_section, parse_divert_argument_expressions,
    parse_inner_logic_string, parsed_expression_to_expression_node,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StatementLevel {
    InnerBlock,
    Stitch,
    Knot,
    Top,
}

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

#[derive(Clone)]
enum ParserFileHandler<'fh> {
    Owned(DefaultFileHandler),
    Borrowed(&'fh dyn FileHandler),
}

impl FileHandler for ParserFileHandler<'_> {
    fn resolve_ink_filename(&self, include_name: &str) -> String {
        match self {
            Self::Owned(handler) => handler.resolve_ink_filename(include_name),
            Self::Borrowed(handler) => handler.resolve_ink_filename(include_name),
        }
    }

    fn load_ink_file_contents(&self, full_filename: &str) -> Result<String, CompilerError> {
        match self {
            Self::Owned(handler) => handler.load_ink_file_contents(full_filename),
            Self::Borrowed(handler) => handler.load_ink_file_contents(full_filename),
        }
    }
}

pub struct InkParser<'fh> {
    source_name: Option<String>,
    parser: StringParser,
    parsing_string_expression: bool,
    parsing_choice: bool,
    tag_active: bool,
    file_handler: ParserFileHandler<'fh>,
    open_filenames: Rc<RefCell<HashSet<String>>>,
}

impl<'fh> InkParser<'fh> {
    pub fn new(input: impl Into<String>, source_name: Option<String>) -> InkParser<'static> {
        let processed = CommentEliminator::process(input.into());
        let working_dir = source_name
            .as_deref()
            .and_then(|name| Path::new(name).parent())
            .map(|path| path.to_path_buf());
        let file_handler = ParserFileHandler::Owned(DefaultFileHandler::new(working_dir));
        let open_filenames = Rc::new(RefCell::new(HashSet::new()));
        if let Some(source_name) = source_name.as_deref() {
            open_filenames
                .borrow_mut()
                .insert(file_handler.resolve_ink_filename(source_name));
        }
        InkParser {
            source_name,
            parser: StringParser::new(processed),
            parsing_string_expression: false,
            parsing_choice: false,
            tag_active: false,
            file_handler,
            open_filenames,
        }
    }

    fn new_with_file_handler(
        input: impl Into<String>,
        source_name: Option<String>,
        file_handler: ParserFileHandler<'fh>,
        open_filenames: Rc<RefCell<HashSet<String>>>,
    ) -> Self {
        Self {
            source_name,
            parser: StringParser::new(CommentEliminator::process(input.into())),
            parsing_string_expression: false,
            parsing_choice: false,
            tag_active: false,
            file_handler,
            open_filenames,
        }
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source_name.as_deref()
    }

    pub fn parse_story(mut self, count_all_visits: bool) -> Result<Story, CompilerError> {
        let source = self.parser.input_string().to_owned();
        let source_name = self.source_name.clone();
        let parsed = self.statements_at_level(StatementLevel::Top);
        Ok(build_story_from_section(source, source_name, count_all_visits, parsed))
    }

    pub fn parse_story_with_file_handler<F>(
        self,
        count_all_visits: bool,
        file_handler: F,
    ) -> Result<Story, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        let file_handler_impl = ClosureFileHandler { load: &file_handler };
        let open_filenames = Rc::new(RefCell::new(HashSet::new()));
        if let Some(source_name) = self.source_name.as_deref() {
            open_filenames
                .borrow_mut()
                .insert(file_handler_impl.resolve_ink_filename(source_name));
        }
        let mut parser = InkParser::new_with_file_handler(
            self.parser.input_string(),
            self.source_name.clone(),
            ParserFileHandler::Borrowed(&file_handler_impl),
            open_filenames,
        );
        let source = parser.parser.input_string().to_owned();
        let source_name = parser.source_name.clone();
        let parsed = parser.statements_at_level(StatementLevel::Top);
        Ok(build_story_from_section(source, source_name, count_all_visits, parsed))
    }
}

struct ClosureFileHandler<'a, F> {
    load: &'a F,
}

impl<F> FileHandler for ClosureFileHandler<'_, F>
where
    F: Fn(&str) -> Result<String, CompilerError>,
{
    fn resolve_ink_filename(&self, include_name: &str) -> String {
        include_name.to_owned()
    }

    fn load_ink_file_contents(&self, full_filename: &str) -> Result<String, CompilerError> {
        (self.load)(full_filename)
    }
}

impl<'fh> InkParser<'fh> {

    pub fn parser(&self) -> &StringParser {
        &self.parser
    }

    pub fn parser_mut(&mut self) -> &mut StringParser {
        &mut self.parser
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


    fn try_parse_external_declaration_line(&mut self) -> Option<ExternalDeclaration> {
        let rule_id = self.parser.begin_rule();
        let Some(declaration) = self.external_declaration() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = self.end_of_line();
        self.parser.succeed_rule(rule_id, Some(declaration))
    }

    fn try_parse_list_statement(&mut self) -> Option<ListDefinition> {
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

    fn try_parse_global_declaration_statement(
        &mut self,
    ) -> Option<(VariableAssignment, (String, crate::parsed_hierarchy::ParsedExpression))> {
        let rule_id = self.parser.begin_rule();
        let Some(node) = self.try_parse_variable_declaration_line() else {
            return self.parser.fail_rule(rule_id);
        };
        let Some(encoded) = node.name() else {
            return self.parser.fail_rule(rule_id);
        };
        let Some((mode, name)) = encoded.split_once(':') else {
            return self.parser.fail_rule(rule_id);
        };
        if mode != "GlobalDecl" {
            return self.parser.fail_rule(rule_id);
        }
        let Some(expression) = node.expression() else {
            return self.parser.fail_rule(rule_id);
        };
        let mut declaration = VariableAssignment::new(name, None);
        declaration.set_global_declaration(true);
        self.parser
            .succeed_rule(rule_id, Some((declaration, (name.to_owned(), expression.clone()))))
    }

    fn try_parse_variable_declaration_line(&mut self) -> Option<ParsedNode> {
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
                    .with_name(format!("GlobalDecl:{name}"))
                    .with_expression(expression),
            ),
        )
    }

    fn try_parse_const_declaration_statement(&mut self) -> Option<ConstDeclaration> {
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

    // ─────────────────────────────────────────────────────────────────────────
    // Statement parsing
    // ─────────────────────────────────────────────────────────────────────────

    fn peek_stitch_start(&mut self) -> bool {
        self.parser
            .peek(|p| {
                let _ = p.parse_characters_from_string(" \t", true, -1);
                p.parse_string("=")?;
                // A second `=` means it's a knot, not a stitch
                if p.parse_string("=").is_some() {
                    return None;
                }
                Some(())
            })
            .is_some()
    }


    fn temp_declaration_or_assignment(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        let default_mode = if self.parse_temp_keyword() {
            self.whitespace();
            "TempSet"
        } else {
            "Set"
        };

        let name = self.parse_identifier()?;
        self.whitespace();

        let (mode, expression_override) = if self.parser.parse_string("+=").is_some() {
            ("AddAssign", None)
        } else if self.parser.parse_string("-=").is_some() {
            ("SubtractAssign", None)
        } else if self.parser.parse_string("++").is_some() {
            ("AddAssign", Some(crate::parsed_hierarchy::ParsedExpression::Int(1)))
        } else if self.parser.parse_string("--").is_some() {
            (
                "SubtractAssign",
                Some(crate::parsed_hierarchy::ParsedExpression::Int(1)),
            )
        } else if self.parser.parse_string("=").is_some() {
            (default_mode, None)
        } else {
            return self.parser.fail_rule(rule_id);
        };

        self.whitespace();

        let expression = if let Some(expression) = expression_override {
            expression
        } else {
            self.whitespace();
            self.expression_until_top_level_terminators("\n\r")?
        };

        self.parser.succeed_rule(
            rule_id,
            Some(
                ParsedNode::new(ParsedNodeKind::Assignment)
                    .with_name(format!("{mode}:{name}"))
                    .with_expression(expression),
            ),
        )
    }

    fn parse_temp_keyword(&mut self) -> bool {
        let rule_id = self.parser.begin_rule();
        if self.parse_identifier().as_deref() == Some("temp") {
            self.parser.succeed_rule(rule_id, Some(ParseSuccess)).is_some()
        } else {
            let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
            false
        }
    }


    fn single_divert_node(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        let Some(node) = self.try_parse_inline_divert() else {
            return self.parser.fail_rule(rule_id);
        };

        if matches!(
            node.kind(),
            ParsedNodeKind::ThreadDivert
                | ParsedNodeKind::TunnelDivert
                | ParsedNodeKind::TunnelReturn
                | ParsedNodeKind::TunnelOnwardsWithTarget
        ) {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(rule_id, Some(node))
    }

    /// Parse a line of mixed content: text, `<>` glue, optional trailing divert.
    ///
    /// Mirrors `LineOfMixedTextAndLogic` / `MixedTextAndLogic` in the C# parser,
    /// restricted to the subset we currently handle.
    fn try_parse_mixed_line(&mut self) -> Option<Vec<ParsedNode>> {
        self.whitespace();

        let mut nodes = self.mixed_text_and_logic()?;
        let line_is_pure_tag = !nodes.is_empty() && nodes.iter().all(|node| node.kind() == ParsedNodeKind::Tag);
        let line_ends_with_divert = nodes.last().is_some_and(|node| {
            matches!(
                node.kind(),
                ParsedNodeKind::Divert
                    | ParsedNodeKind::TunnelDivert
                    | ParsedNodeKind::TunnelReturn
                    | ParsedNodeKind::TunnelOnwardsWithTarget
            )
        });
        if !line_ends_with_divert {
            Self::trim_trailing_whitespace_nodes(&mut nodes);
        }
        if !line_is_pure_tag {
            nodes.push(ParsedNode::new(ParsedNodeKind::Newline));
        }
        let _ = self.end_of_line();

        Some(nodes)
    }

    fn mixed_text_and_logic(&mut self) -> Option<Vec<ParsedNode>> {
        let mut nodes: Vec<ParsedNode> = Vec::new();

        // Interleave: text … inline-expressions … glue …
        loop {
            if let Some(text) = self.content_text() {
                if !text.is_empty() {
                    nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(text));
                }
            }

            // Handle inline {…} expressions/sequences
            if self.parser.peek(|parser| parser.parse_string("{")).is_some() {
                let mut expr_nodes = self.parse_braced_inline_content("\n\r")?;
                nodes.append(&mut expr_nodes);
                continue;
            }

            if self.parser.peek(|parser| parser.parse_string("#")).is_some() {
                if let Some(tag) = self.parse_tag_content("\n\r") {
                    nodes.push(tag);
                    continue;
                }
            }

            if self.parser.parse_string("<>").is_some() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Glue));
                continue; // look for more text after glue
            }

            break;
        }

        if !self.parsing_choice && let Some(divert) = self.try_parse_inline_divert() {
            Self::append_trailing_space(&mut nodes);
            nodes.push(divert);
        }

        (!nodes.is_empty()).then_some(nodes)
    }

    /// Try to parse an inline `-> target` (does not consume a trailing newline).
    fn try_parse_inline_divert(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();

        if self.parser.parse_string("->->").is_some() {
            self.whitespace();
            let node = if let Some(divert) = self.divert_identifier_with_arguments() {
                let arguments = parse_divert_argument_expressions(&divert.arguments)?;
                ParsedNode::new(ParsedNodeKind::TunnelOnwardsWithTarget)
                    .with_target(divert.target.join("."))
                    .with_arguments(arguments)
            } else {
                ParsedNode::new(ParsedNodeKind::TunnelReturn)
            };
            return self.parser.succeed_rule(rule_id, Some(node));
        }

        if self.parser.parse_string("->").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let Some(divert) = self.divert_identifier_with_arguments() else {
            // Bare `->` with no target (gather-style)
            return self.parser.succeed_rule(rule_id, None);
        };

        let target = divert.target.join(".");
        let arguments = parse_divert_argument_expressions(&divert.arguments)?;

        let node = if self.parser.parse_string("->").is_some() {
            ParsedNode::new(ParsedNodeKind::TunnelDivert)
                .with_target(target)
                .with_arguments(arguments)
        } else {
            ParsedNode::new(ParsedNodeKind::Divert)
                .with_target(target)
                .with_arguments(arguments)
        };
        self.parser.succeed_rule(rule_id, Some(node))
    }

    // ── Text-node helpers ──────────────────────────────────────────────────────

    /// Trim trailing inline whitespace from the last `Text` node.
    /// If the node becomes empty it is removed (and the previous node is also trimmed
    /// recursively, matching C# `TrimEndWhitespace`).
    fn trim_trailing_whitespace_nodes(nodes: &mut Vec<ParsedNode>) {
        if let Some(last) = nodes.last_mut() {
            if last.kind() == ParsedNodeKind::Text {
                let trimmed = last
                    .text()
                    .unwrap_or("")
                    .trim_end_matches([' ', '\t'])
                    .to_owned();
                if trimmed.is_empty() {
                    nodes.pop();
                    // Recurse – there may be more trailing whitespace-only nodes
                    Self::trim_trailing_whitespace_nodes(nodes);
                } else {
                    *last = ParsedNode::new(ParsedNodeKind::Text).with_text(trimmed);
                }
            }
        }
    }

    /// Trim trailing whitespace from the last `Text` node and append a single
    /// space.  Used before a trailing divert (`TrimEndWhitespace(terminateWithSpace:true)`
    /// in C#).
    fn append_trailing_space(nodes: &mut Vec<ParsedNode>) {
        if let Some(last) = nodes.last_mut() {
            if last.kind() == ParsedNodeKind::Text {
                let trimmed = last
                    .text()
                    .unwrap_or("")
                    .trim_end_matches([' ', '\t'])
                    .to_owned();
                *last = ParsedNode::new(ParsedNodeKind::Text).with_text(format!("{trimmed} "));
                return;
            }
        }

        if !nodes.is_empty() {
            nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(" "));
        }
    }



    fn parse_until_top_level_terminator_set(&mut self, terminators: &str) -> Option<String> {
        let mut result = String::new();
        let mut paren_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut string_open = false;

        while let Some(ch) = self.parser.peek(|parser| parser.parse_single_character()) {
            if terminators.contains(ch)
                && !string_open
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0
            {
                return Some(result);
            }

            let ch = self.parser.parse_single_character()?;
            match ch {
                '"' => string_open = !string_open,
                '(' if !string_open => paren_depth += 1,
                ')' if !string_open => paren_depth = paren_depth.saturating_sub(1),
                '{' if !string_open => brace_depth += 1,
                '}' if !string_open => brace_depth = brace_depth.saturating_sub(1),
                '[' if !string_open => bracket_depth += 1,
                ']' if !string_open => bracket_depth = bracket_depth.saturating_sub(1),
                _ => {}
            }
            result.push(ch);
        }

        Some(result)
    }


}

#[cfg(test)]
mod tests {
    use super::InkParser;
    use crate::{
        parsed_hierarchy::{Content, NumberValue, ParsedNodeKind},
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

    #[test]
    fn expression_parser_honours_binary_precedence() {
        let mut parser = InkParser::new("1 + 2 * 3", None);
        let expr = parser.expression().expect("expression");

        match expr {
            crate::parsed_hierarchy::ParsedExpression::Binary { left, operator, right } => {
                assert_eq!("Add", operator);
                assert!(matches!(*left, crate::parsed_hierarchy::ParsedExpression::Int(1)));
                match *right {
                    crate::parsed_hierarchy::ParsedExpression::Binary { left, operator, right } => {
                        assert_eq!("Multiply", operator);
                        assert!(matches!(*left, crate::parsed_hierarchy::ParsedExpression::Int(2)));
                        assert!(matches!(*right, crate::parsed_hierarchy::ParsedExpression::Int(3)));
                    }
                    other => panic!("unexpected rhs: {other:?}"),
                }
            }
            other => panic!("unexpected expression: {other:?}"),
        }
    }

    #[test]
    fn expression_parser_parses_list_items() {
        let mut parser = InkParser::new("(apples, basket.oranges)", None);
        let expr = parser.expression().expect("list expression");
        match expr {
            crate::parsed_hierarchy::ParsedExpression::ListItems(items) => {
                assert_eq!(vec!["apples".to_owned(), "basket.oranges".to_owned()], items);
            }
            other => panic!("unexpected expression: {other:?}"),
        }
    }

    #[test]
    fn expression_parser_parses_string_expression_nodes() {
        let mut parser = InkParser::new(r#""hello {name}""#, None);
        let expr = parser.expression().expect("string expression");
        match expr {
            crate::parsed_hierarchy::ParsedExpression::StringExpression(nodes) => {
                assert!(nodes.iter().any(|node| node.kind() == ParsedNodeKind::Text));
                assert!(nodes.iter().any(|node| node.kind() == ParsedNodeKind::OutputExpression));
            }
            other => panic!("unexpected expression: {other:?}"),
        }
    }

    #[test]
    fn author_warning_line_parses_todo_statement() {
        let mut parser = InkParser::new("TODO: fix this\n", None);
        let node = parser.try_parse_author_warning_line().expect("todo statement");
        assert_eq!(ParsedNodeKind::AuthorWarning, node.kind());
        assert_eq!(Some("fix this"), node.text());
    }

    #[test]
    fn multiline_conditional_with_else_keeps_all_branches() {
        let ink = r#"
VAR x = -2
VAR y = 3
{
    - x == 0:
        ~ y = 0
    - x > 0:
        ~ y = x - 1
    - else:
        ~ y = x + 1
}
The value is {y}. -> END
"#;
        let parsed = InkParser::new(ink, None).parse_story(true).expect("parse story");
        let conditional = parsed
            .root_nodes()
            .iter()
            .find(|node| node.kind() == ParsedNodeKind::Conditional)
            .expect("conditional root node");
        assert_eq!(3, conditional.children().len());
        assert!(conditional.children()[2].is_else);
    }

    #[test]
    fn multiline_conditional_text_keeps_all_branches() {
        let ink = r#"
VAR x = -2
{
    - x == 0:
      This is text 1.
    - x > 0:
      This is text 2.
    - else:
      This is text 3.
}
+ [The Choice.] -> to_end
=== to_end
This is the end. -> END
"#;
        let parsed = InkParser::new(ink, None).parse_story(true).expect("parse story");
        let conditional = parsed
            .root_nodes()
            .iter()
            .find(|node| node.kind() == ParsedNodeKind::Conditional)
            .expect("conditional root node");
        assert_eq!(3, conditional.children().len());
        assert!(conditional.children()[2].is_else);
    }

    #[test]
    fn conditional_choice_story_keeps_expected_choice_conditions() {
        let ink = r#"
Test conditional choices
* { true } { false } not displayed
* { true } { true } { true and true }  one
* { false } not displayed
* { true } two
* { true } { true } three
* { true } four
"#;
        let parsed = InkParser::new(ink, None).parse_story(true).expect("parse story");
        let choices: Vec<_> = parsed
            .root_nodes()
            .iter()
            .filter(|node| node.kind() == ParsedNodeKind::Choice)
            .collect();
        assert!(matches!(
            choices[0].condition(),
            Some(crate::parsed_hierarchy::ParsedExpression::Binary { .. })
        ));
        assert_eq!(Some("not displayed"), choices[0].start_content[0].text());
        assert_eq!(6, choices.len());
    }
}
