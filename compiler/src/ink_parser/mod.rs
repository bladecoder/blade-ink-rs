use crate::{
    error::CompilerError,
    file_handler::{DefaultFileHandler, FileHandler},
    parsed_hierarchy::{
        ContentList, DebugMetadata, ExternalDeclaration, FlowArgument, FlowDecl, FlowLevel, List,
        ListDefinition, ListElementDefinition, Number, NumberValue, ParsedFlow, ParsedNode,
        ParsedNodeKind, Return as ParsedReturn, SequenceType, Story, Tag, VariableAssignment,
    },
    string_parser::{
        CharacterRange, CharacterSet, CommentEliminator, ParseSuccess, StringParser,
        StringParserStateElement,
    },
};
use std::{cell::RefCell, collections::HashSet, path::Path, rc::Rc};

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
pub struct InkParser {
    source_name: Option<String>,
    parser: StringParser,
    parsing_string_expression: bool,
    parsing_choice: bool,
    tag_active: bool,
    file_handler: Rc<dyn FileHandler>,
    open_filenames: Rc<RefCell<HashSet<String>>>,
}

#[derive(Default)]
struct ParseSection {
    nodes: Vec<ParsedNode>,
    flows: Vec<ParsedFlow>,
    global_declarations: Vec<VariableAssignment>,
    global_initializers: Vec<(String, crate::parsed_hierarchy::ParsedExpression)>,
    list_definitions: Vec<ListDefinition>,
    external_declarations: Vec<ExternalDeclaration>,
}

impl InkParser {
    pub fn new(input: impl Into<String>, source_name: Option<String>) -> Self {
        let processed = CommentEliminator::process(input.into());
        let working_dir = source_name
            .as_deref()
            .and_then(|name| Path::new(name).parent())
            .map(|path| path.to_path_buf());
        let file_handler: Rc<dyn FileHandler> = Rc::new(DefaultFileHandler::new(working_dir));
        let open_filenames = Rc::new(RefCell::new(HashSet::new()));
        if let Some(source_name) = source_name.as_deref() {
            open_filenames
                .borrow_mut()
                .insert(file_handler.resolve_ink_filename(source_name));
        }
        Self {
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
        file_handler: Rc<dyn FileHandler>,
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
        F: Fn(&str) -> Result<String, CompilerError> + 'static,
    {
        let file_handler: Rc<dyn FileHandler> = Rc::new(ClosureFileHandler { load: file_handler });
        let open_filenames = Rc::new(RefCell::new(HashSet::new()));
        if let Some(source_name) = self.source_name.as_deref() {
            open_filenames
                .borrow_mut()
                .insert(file_handler.resolve_ink_filename(source_name));
        }
        let mut parser = InkParser::new_with_file_handler(
            self.parser.input_string(),
            self.source_name.clone(),
            file_handler,
            open_filenames,
        );
        let source = parser.parser.input_string().to_owned();
        let source_name = parser.source_name.clone();
        let parsed = parser.statements_at_level(StatementLevel::Top);
        Ok(build_story_from_section(source, source_name, count_all_visits, parsed))
    }
}

struct ClosureFileHandler<F> {
    load: F,
}

impl<F> FileHandler for ClosureFileHandler<F>
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

impl InkParser {

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

    fn try_parse_author_warning_line(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        let identifier = self.parse_identifier()?;
        if identifier != "TODO" {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let _ = self.parser.parse_string(":");
        self.whitespace();
        let message = self
            .parser
            .parse_until_characters_from_string("\n\r", -1)
            .unwrap_or_default();
        let _ = self.end_of_line();

        self.parser.succeed_rule(
            rule_id,
            Some(ParsedNode::new(ParsedNodeKind::AuthorWarning).with_text(message)),
        )
    }

    fn try_parse_include_statement_line(&mut self) -> Option<ParseSection> {
        let rule_id = self.parser.begin_rule();
        let filename = self.include_statement_filename()?;
        let _ = self.end_of_line();
        self.parser.succeed_rule(rule_id, Some(()));

        let full_filename = self.file_handler.resolve_ink_filename(&filename);
        if self.open_filenames.borrow().contains(&full_filename) {
            return Some(ParseSection::default());
        }

        self.open_filenames
            .borrow_mut()
            .insert(full_filename.clone());

        let included = self.file_handler.load_ink_file_contents(&full_filename).ok()?;
        let mut parser = InkParser::new_with_file_handler(
            included,
            Some(filename),
            self.file_handler.clone(),
            self.open_filenames.clone(),
        );
        let parsed = parser.statements_at_level(StatementLevel::Top);

        self.open_filenames.borrow_mut().remove(&full_filename);
        Some(parsed)
    }

    fn try_parse_external_declaration_line(&mut self) -> Option<ExternalDeclaration> {
        let rule_id = self.parser.begin_rule();
        let declaration = self.external_declaration()?;
        let _ = self.end_of_line();
        self.parser.succeed_rule(rule_id, Some(declaration))
    }

    fn try_parse_list_statement(&mut self) -> Option<ListDefinition> {
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
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(definition))
    }

    fn try_parse_global_declaration_statement(
        &mut self,
    ) -> Option<(VariableAssignment, (String, crate::parsed_hierarchy::ParsedExpression))> {
        let rule_id = self.parser.begin_rule();
        let node = self.try_parse_variable_declaration_line()?;
        let encoded = node.name()?;
        let (mode, name) = encoded.split_once(':')?;
        if mode != "GlobalDecl" {
            return self.parser.fail_rule(rule_id);
        }
        let expression = node.expression()?.clone();
        let mut declaration = VariableAssignment::new(name, None);
        declaration.set_global_declaration(true);
        self.parser
            .succeed_rule(rule_id, Some((declaration, (name.to_owned(), expression))))
    }

    fn try_parse_variable_declaration_line(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();
        if self.parser.parse_string("VAR").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let name = self.parse_identifier()?;
        self.whitespace();
        if self.parser.parse_string("=").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let expression = self.expression_until_top_level_terminators("\n\r")?;
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

    fn statements_at_level(&mut self, level: StatementLevel) -> ParseSection {
        let mut parsed = ParseSection::default();

        loop {
            self.multiline_whitespace();
            if self.parser.end_of_input() {
                break;
            }

            if self.statements_break_for_level(level) {
                break;
            }

            if self.statement_at_level(level, &mut parsed) {
                continue;
            }

            // Nothing matched – skip to next line
            self.skip_line();
        }

        parsed
    }

    fn statement_at_level(&mut self, level: StatementLevel, parsed: &mut ParseSection) -> bool {
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

        if let Some((declaration, initializer)) = self.try_parse_global_declaration_statement() {
            parsed.global_declarations.push(declaration);
            parsed.global_initializers.push(initializer);
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

    fn statements_break_for_level(&mut self, level: StatementLevel) -> bool {
        self.parser
            .peek(|parser| {
                let mut parser = Self {
                    source_name: self.source_name.clone(),
                    parser: parser.clone(),
                    parsing_string_expression: self.parsing_string_expression,
                    parsing_choice: self.parsing_choice,
                    tag_active: self.tag_active,
                    file_handler: self.file_handler.clone(),
                    open_filenames: self.open_filenames.clone(),
                };
                let _ = parser.whitespace();

                if level <= StatementLevel::Knot && parser.peek_knot_start() {
                    return Some(ParseSuccess);
                }

                if level <= StatementLevel::Stitch && parser.peek_stitch_start() {
                    return Some(ParseSuccess);
                }

                if level <= StatementLevel::InnerBlock {
                    if parser.parse_dash_not_arrow().is_some() {
                        return Some(ParseSuccess);
                    }
                    if parser.parser.parse_string("}").is_some() {
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

    // ── Peek helpers ──────────────────────────────────────────────────────────

    /// Returns true if the next non-whitespace content looks like a knot header (`==…`).
    fn peek_knot_start(&mut self) -> bool {
        self.parser
            .peek(|p| {
                let _ = p.parse_characters_from_string(" \t", true, -1);
                let eq = p.parse_characters_from_string("=", true, -1)?;
                if eq.len() > 1 { Some(()) } else { None }
            })
            .is_some()
    }

    /// Returns true if the next non-whitespace content looks like a stitch header (`= name`).
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

    // ── Flow definitions ──────────────────────────────────────────────────────

    /// Try to parse a knot (or function) definition starting with `==`.
    fn try_parse_knot(&mut self) -> Option<ParsedFlow> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        // Must start with at least `==`
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
        // Optional trailing `===`
        let _ = self.parser.parse_characters_from_string("=", true, -1);

        // Consume the rest of the header line
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        // Parse the knot body
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

    /// Try to parse a stitch definition starting with a single `=`.
    fn try_parse_stitch(&mut self) -> Option<ParsedFlow> {
        // Peek first to avoid leaving orphaned begin_rule on the stack when failing
        if !self.peek_stitch_start() {
            return None;
        }

        let rule_id = self.parser.begin_rule();
        self.whitespace();

        // Single `=`
        let Some(_) = self.parser.parse_string("=") else {
            return self.parser.fail_rule(rule_id);
        };
        // Reject `==` (which is a knot)
        if self.parser.parse_string("=").is_some() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        // Optional `function` keyword
        let mut is_function = false;
        let kw_rule = self.parser.begin_rule();
        if let Some(kw) = self.parse_identifier() {
            if kw == "function" {
                self.parser.succeed_rule(kw_rule, Some(()));
                self.whitespace();
                is_function = true;
            } else {
                // Not `function` – restore and re-parse as the stitch name below
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
        // Consume the rest of the header line
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        let parsed = self.statements_at_level(StatementLevel::Stitch);

        Some(ParsedFlow::new(
            name,
            FlowLevel::Stitch,
            arguments,
            is_function,
            parsed.nodes,
            Vec::new(), // stitches have no children
        ))
    }

    // ── Statement parsers ─────────────────────────────────────────────────────

    /// Parse a line that is *only* a divert: `-> target`.
    fn try_parse_divert_line(&mut self) -> Option<Vec<ParsedNode>> {
        let rule_id = self.parser.begin_rule();

        self.whitespace();

        if self.parser.parse_string("->->").is_some() {
            self.whitespace();
            let node = if let Some(parts) = self.dot_separated_divert_path_components() {
                ParsedNode::new(ParsedNodeKind::TunnelOnwardsWithTarget)
                    .with_target(parts.join("."))
            } else {
                ParsedNode::new(ParsedNodeKind::TunnelReturn)
            };
            let _ = self.end_of_line();
            return self.parser.succeed_rule(rule_id, Some(vec![node]));
        }

        if self.parser.parse_string("->").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let target_components = self.dot_separated_divert_path_components();
        let _ = self.end_of_line();

        let nodes = match target_components {
            Some(parts) => {
                let target = parts.join(".");
                let _ = self.expression_function_call_arguments(); // optional args
                self.whitespace();
                if self.parser.parse_string("->").is_some() {
                    vec![ParsedNode::new(ParsedNodeKind::TunnelDivert).with_target(target)]
                } else {
                    vec![ParsedNode::new(ParsedNodeKind::Divert).with_target(target)]
                }
            }
            None => vec![], // bare `->` (gather-style, no target)
        };

        let _ = self.end_of_line();
        self.parser.succeed_rule(rule_id, Some(nodes))
    }

    fn try_parse_logic_line(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();
        if self.parser.parse_string("~").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.whitespace();

        if let Some(return_node) = self.try_parse_return_after_tilde() {
            let _ = self.end_of_line();
            return self.parser.succeed_rule(rule_id, Some(return_node));
        }

        if let Some(assignment_node) = self.temp_declaration_or_assignment() {
            let _ = self.end_of_line();
            return self.parser.succeed_rule(rule_id, Some(assignment_node));
        }

        let expression = self.expression_until_top_level_terminators("\n\r")?;
        let _ = self.end_of_line();
        match expression {
            crate::parsed_hierarchy::ParsedExpression::FunctionCall { .. } => self
                .parser
                .succeed_rule(rule_id, Some(ParsedNode::new(ParsedNodeKind::VoidCall).with_expression(expression))),
            _ => self.parser.fail_rule(rule_id),
        }
    }

    fn try_parse_return_after_tilde(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        if let Some(keyword) = self.parse_identifier()
            && keyword == "return"
        {
            self.whitespace();
            let expression = self.expression_until_top_level_terminators("\n\r");
            return if expression.is_none() {
                self.parser
                    .succeed_rule(rule_id, Some(ParsedNode::new(ParsedNodeKind::ReturnVoid)))
            } else {
                self.parser.succeed_rule(
                    rule_id,
                    Some(
                        ParsedNode::new(ParsedNodeKind::ReturnExpression)
                        .with_expression(expression?),
                    ),
                )
            };
        }

        self.parser.fail_rule(rule_id)
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

    fn expression_until_top_level_terminators(
        &mut self,
        terminators: &str,
    ) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let text = self.parse_until_top_level_terminator_set(terminators)?;
        let mut parser = InkParser::new(text.trim(), None);
        let expression = parser.expression()?;
        let _ = parser.whitespace();
        if !parser.parser.end_of_input() {
            return None;
        }
        Some(expression)
    }

    fn expression(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        self.expression_with_min_precedence(0)
    }

    fn expression_with_min_precedence(
        &mut self,
        minimum_precedence: i32,
    ) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        self.whitespace();

        let mut expr = self.expression_unary()?;
        self.whitespace();

        loop {
            let rule_id = self.parser.begin_rule();
            let Some(op) = self.parse_infix_operator() else {
                let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
                break;
            };

            if op.precedence <= minimum_precedence {
                let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
                break;
            }

            self.whitespace();
            let Some(right) = self.expression_with_min_precedence(op.precedence) else {
                let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
                return None;
            };

            expr = crate::parsed_hierarchy::ParsedExpression::Binary {
                left: Box::new(expr),
                operator: op.kind.to_owned(),
                right: Box::new(right),
            };
            let _ = self.parser.succeed_rule(rule_id, Some(ParseSuccess));
            self.whitespace();
        }

        Some(expr)
    }

    fn expression_unary(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();

        if let Some(divert_target) = self.expression_divert_target() {
            return self.parser.succeed_rule(rule_id, Some(divert_target));
        }

        let prefix_op: Option<String> = if self.parser.parse_string("-").is_some() {
            Some("-".to_owned())
        } else if self.parser.parse_string("!").is_some() {
            Some("!".to_owned())
        } else {
            self.expression_not_keyword()
        };

        self.whitespace();

        let mut expr = self
            .expression_list()
            .or_else(|| self.expression_paren())
            .or_else(|| self.expression_function_call())
            .or_else(|| self.expression_variable_name())
            .or_else(|| self.expression_literal());

        if expr.is_none() && prefix_op.is_some() {
            expr = self.expression_unary();
        }

        let Some(mut expr) = expr else {
            return self.parser.fail_rule(rule_id);
        };

        if let Some(prefix_op) = prefix_op {
            expr = crate::parsed_hierarchy::ParsedExpression::Unary {
                operator: prefix_op,
                expression: Box::new(expr),
            };
        }

        self.parser.succeed_rule(rule_id, Some(expr))
    }

    fn parse_infix_operator(&mut self) -> Option<InfixOperator> {
        for op in infix_operators() {
            let rule_id = self.parser.begin_rule();
            if self.parser.parse_string(op.token).is_some() {
                if op.require_whitespace_after && self.whitespace().is_none() {
                    let _ = self.parser.fail_rule::<InfixOperator>(rule_id);
                    continue;
                }
                return self.parser.succeed_rule(rule_id, Some(*op));
            }
            let _ = self.parser.fail_rule::<InfixOperator>(rule_id);
        }

        None
    }

    fn expression_not_keyword(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        if self.parse_identifier().as_deref() == Some("not") {
            self.parser.succeed_rule(rule_id, Some("!".to_owned()))
        } else {
            self.parser.fail_rule(rule_id)
        }
    }

    fn expression_paren(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let Some(expression) = self.expression_until_top_level_terminators(")") else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        if self.parser.parse_string(")").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(rule_id, Some(expression))
    }

    fn expression_list(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let mut members = Vec::new();
        if self.parser.parse_string(")").is_some() {
            return self
                .parser
                .succeed_rule(rule_id, Some(crate::parsed_hierarchy::ParsedExpression::EmptyList));
        }

        loop {
            let Some(member) = self.list_member() else {
                return self.parser.fail_rule(rule_id);
            };
            members.push(member);

            self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }

            if self.parser.parse_string(",").is_none() {
                return self.parser.fail_rule(rule_id);
            }
            self.whitespace();
        }

        self.parser.succeed_rule(
            rule_id,
            Some(crate::parsed_hierarchy::ParsedExpression::ListItems(members)),
        )
    }

    fn list_member(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        let Some(first) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };

        let mut name = first;
        let checkpoint = self.parser.begin_rule();
        if self.parser.parse_string(".").is_some() {
            let Some(second) = self.parse_identifier() else {
                return self.parser.fail_rule(rule_id);
            };
            let _ = self.parser.succeed_rule(checkpoint, Some(ParseSuccess));
            name.push('.');
            name.push_str(&second);
        } else {
            let _ = self.parser.fail_rule::<ParseSuccess>(checkpoint);
        }

        self.whitespace();
        self.parser.succeed_rule(rule_id, Some(name))
    }

    fn expression_divert_target(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        let Some(node) = self.single_divert_node() else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        let Some(target) = node.target() else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(
            rule_id,
            Some(crate::parsed_hierarchy::ParsedExpression::DivertTarget(target.to_owned())),
        )
    }

    fn expression_literal(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let expr = self
            .expression_int()
            .or_else(|| self.expression_float())
            .or_else(|| self.expression_bool())
            .or_else(|| self.expression_string());
        let Some(expr) = expr else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(rule_id, Some(expr))
    }

    fn expression_int(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(value) = self.parser.parse_int() else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(
            rule_id,
            Some(crate::parsed_hierarchy::ParsedExpression::Int(value)),
        )
    }

    fn expression_float(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(value) = self.parser.parse_float() else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(
            rule_id,
            Some(crate::parsed_hierarchy::ParsedExpression::Float(value)),
        )
    }

    fn expression_bool(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(identifier) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        let expr = match identifier.as_str() {
            "true" => crate::parsed_hierarchy::ParsedExpression::Bool(true),
            "false" => crate::parsed_hierarchy::ParsedExpression::Bool(false),
            _ => return self.parser.fail_rule(rule_id),
        };
        self.parser.succeed_rule(rule_id, Some(expr))
    }

    fn expression_string(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("\"").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        if self.parser.peek(|parser| parser.parse_string("\"")).is_some() {
            self.parser.parse_string("\"")?;
            return self.parser.succeed_rule(
                rule_id,
                Some(crate::parsed_hierarchy::ParsedExpression::String(String::new())),
            );
        }

        let was_parsing_string = self.parsing_string_expression;
        self.parsing_string_expression = true;
        let text_and_logic = self.string_text_and_logic();
        self.parsing_string_expression = was_parsing_string;

        if self.parser.parse_string("\"").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        if text_and_logic.iter().any(|node| {
            matches!(
                node.kind(),
                ParsedNodeKind::Divert
                    | ParsedNodeKind::TunnelDivert
                    | ParsedNodeKind::TunnelReturn
                    | ParsedNodeKind::TunnelOnwardsWithTarget
            )
        }) {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(
            rule_id,
            Some(crate::parsed_hierarchy::ParsedExpression::StringExpression(text_and_logic)),
        )
    }

    fn string_text_and_logic(&mut self) -> Vec<ParsedNode> {
        let mut nodes = Vec::new();

        loop {
            if self.parser.peek(|parser| parser.parse_string("\"")).is_some() {
                break;
            }

            if self.parser.peek(|parser| parser.parse_string("{")).is_some() {
                self.parser.parse_string("{");
                let Some(content) = self.parse_balanced_brace_body() else {
                    break;
                };
                if let Some(mut expression_nodes) = parse_inner_logic_string(
                    &content,
                    "\"",
                    self.parsing_choice,
                    false,
                ) {
                    nodes.append(&mut expression_nodes);
                    continue;
                }
                break;
            }

            if self.parser.parse_string("<>").is_some() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Glue));
                continue;
            }

            let Some(text) = self.content_text_allowing_escape_char() else {
                break;
            };
            if !text.is_empty() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(text));
            }
        }

        nodes
    }

    fn expression_function_call(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        let Some(arguments) = self.expression_function_call_arguments_parsed() else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(
            rule_id,
            Some(crate::parsed_hierarchy::ParsedExpression::FunctionCall { name, arguments }),
        )
    }

    fn expression_function_call_arguments_parsed(
        &mut self,
    ) -> Option<Vec<crate::parsed_hierarchy::ParsedExpression>> {
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

            let Some(argument) = self.expression_until_top_level_terminators(",)") else {
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

    fn expression_variable_name(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(first) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if is_reserved_keyword(&first) || is_number_only_identifier(&first) {
            return self.parser.fail_rule(rule_id);
        }

        let mut path = vec![first];
        loop {
            let checkpoint = self.parser.begin_rule();
            let _ = self.whitespace();
            if self.parser.parse_string(".").is_none() {
                let _ = self.parser.fail_rule::<ParseSuccess>(checkpoint);
                break;
            }
            let _ = self.whitespace();
            let Some(next) = self.parse_identifier() else {
                return self.parser.fail_rule(rule_id);
            };
            self.parser.succeed_rule(checkpoint, Some(ParseSuccess));
            path.push(next);
        }

        self.parser.succeed_rule(
            rule_id,
            Some(crate::parsed_hierarchy::ParsedExpression::Variable(path.join("."))),
        )
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
        nodes.push(ParsedNode::new(ParsedNodeKind::Newline));
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
            let node = if let Some(parts) = self.dot_separated_divert_path_components() {
                ParsedNode::new(ParsedNodeKind::TunnelOnwardsWithTarget)
                    .with_target(parts.join("."))
            } else {
                ParsedNode::new(ParsedNodeKind::TunnelReturn)
            };
            return self.parser.succeed_rule(rule_id, Some(node));
        }

        if self.parser.parse_string("->").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let Some(parts) = self.dot_separated_divert_path_components() else {
            // Bare `->` with no target (gather-style)
            return self.parser.succeed_rule(rule_id, None);
        };

        let target = parts.join(".");
        let _ = self.expression_function_call_arguments(); // optional args
        self.whitespace();

        let node = if self.parser.parse_string("->").is_some() {
            ParsedNode::new(ParsedNodeKind::TunnelDivert).with_target(target)
        } else {
            ParsedNode::new(ParsedNodeKind::Divert).with_target(target)
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
            }
        }
    }

    /// Try to parse a choice line: `* text` / `+ text` / `* [text]` / etc.
    ///
    /// Returns a `ParsedNode::Choice` with:
    /// - `indentation_depth`: number of `*`/`+` markers
    /// - `once_only`: true for `*`, false for `+`
    /// - `start_content`: text before `[` (shown in list and after choosing)
    /// - `choice_only_content`: text inside `[…]` (shown only in list)
    /// - `children`: inner content nodes (trailing same-line divert etc.)
    /// - `name`: optional label `(identifier)` after the markers
    fn try_parse_choice(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();

        // Optional leading horizontal whitespace
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        // Detect the first marker character: `*` or `+`
        let first_char = self.parser.peek(|p| p.parse_single_character())?;
        let is_choice = first_char == '*' || first_char == '+';
        if !is_choice {
            return self.parser.fail_rule(rule_id);
        }

        // Count consecutive identical markers (depth)
        let once_only = first_char == '*';
        let marker_char = if once_only { "*" } else { "+" };
        let markers = self
            .parser
            .parse_characters_from_string(marker_char, true, -1)?;
        let depth = markers.len();

        // Optional horizontal whitespace after markers
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        // Optional label `(identifier)` before the content
        let label = self.try_parse_inline_label();

        let _ = self.whitespace();

        // Reference parser allows an optional newline immediately after a label.
        if label.is_some() {
            let _ = self.newline();
        }

        let choice_condition = self.parse_choice_condition();

        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        let was_parsing_choice = self.parsing_choice;
        self.parsing_choice = true;

        // Start content: text before `[` or end-of-line
        let start_nodes = self.parse_choice_start_content();

        // Choice-only content: text inside `[…]`
        let mut choice_only_nodes = self.parse_choice_only_content();

        // Inner content: everything after `]` on the same line (may be text + optional divert)
        let inner_nodes = self.parse_choice_inner_content();

        if should_close_quoted_choice(&start_nodes, &choice_only_nodes, &inner_nodes) {
            append_text_suffix(&mut choice_only_nodes, "'");
        }

        self.parsing_choice = was_parsing_choice;

        // Consume the rest of the line
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        let mut node = ParsedNode::new(ParsedNodeKind::Choice);
        node.indentation_depth = depth;
        node.once_only = once_only;
        node.is_invisible_default = start_nodes.is_empty() && choice_only_nodes.is_empty();
        node.start_content = if choice_only_nodes.is_empty() {
            trim_end_whitespace_nodes(start_nodes)
        } else {
            start_nodes
        };
        node.choice_only_content = choice_only_nodes;
        if let Some(condition) = choice_condition {
            node = node.with_condition(condition);
        }
        if let Some(label_name) = label {
            node = node.with_name(label_name);
        }
        // Inner content becomes `children`
        if !inner_nodes.is_empty() {
            node = node.with_children(inner_nodes);
        }
        Some(node)
    }

    /// Parse the start content of a choice (text before `[` or end-of-line).
    fn parse_choice_start_content(&mut self) -> Vec<ParsedNode> {
        self.parse_inline_content_until("[\n\r")
    }

    /// Parse choice-only content inside `[…]`.
    fn parse_choice_only_content(&mut self) -> Vec<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("[").is_none() {
            self.parser.cancel_rule(rule_id);
            return Vec::new();
        }
        let nodes = self.parse_inline_content_until("]");
        // Consume closing `]`; if missing, treat as empty choice-only
        let _ = self.parser.parse_string("]");
        self.parser.succeed_rule(rule_id, Some(()));
        nodes
    }

    /// Parse any inner content after `]` on the same choice line (text + optional divert).
    fn parse_choice_inner_content(&mut self) -> Vec<ParsedNode> {
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);
        let mut nodes = trim_end_whitespace_nodes(self.parse_inline_content_until("\n\r"));
        if let Some(divert) = self.try_parse_inline_divert() {
            if !nodes.is_empty() {
                Self::append_trailing_space(&mut nodes);
            }
            nodes.push(divert);
        }
        nodes
    }

    fn parse_choice_condition(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let first = self.choice_single_condition()?;
        let mut conditions = vec![first];

        loop {
            let rule_id = self.parser.begin_rule();
            if self.choice_conditions_space().is_none() {
                self.parser.cancel_rule(rule_id);
                break;
            }

            let Some(condition) = self.choice_single_condition() else {
                return self.parser.fail_rule(rule_id);
            };
            self.parser.succeed_rule(rule_id, Some(()));
            conditions.push(condition);
        }

        let mut iter = conditions.into_iter();
        let first = iter.next()?;
        Some(iter.fold(first, |left, right| crate::parsed_hierarchy::ParsedExpression::Binary {
            left: Box::new(left),
            operator: "And".to_owned(),
            right: Box::new(right),
        }))
    }

    fn choice_conditions_space(&mut self) -> Option<ParseSuccess> {
        let rule_id = self.parser.begin_rule();
        let mut found = false;
        if self.newline().is_some() {
            found = true;
        }
        if self.whitespace().is_some() {
            found = true;
        }

        if found {
            self.parser.succeed_rule(rule_id, Some(ParseSuccess))
        } else {
            self.parser.fail_rule(rule_id)
        }
    }

    fn choice_single_condition(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("{").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let Some(condition_text) = self.parser.parse_until_characters_from_string("}", -1) else {
            return None;
        };
        if self.parser.parse_string("}").is_none() {
            return None;
        }

        let mut parser = InkParser::new(condition_text.trim(), None);
        let Some(condition) = parser.expression() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = parser.whitespace();
        if !parser.parser.end_of_input() {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(rule_id, Some(condition))
    }

    fn parse_inline_content_until(&mut self, terminators: &str) -> Vec<ParsedNode> {
        let mut nodes = Vec::new();

        loop {
            if self
                .parser
                .peek(|parser| {
                    parser
                        .current_character()
                        .filter(|ch| terminators.contains(*ch))
                        .map(|_| ())
                })
                .is_some()
            {
                break;
            }

            if self.parser.peek(|parser| parser.parse_string("{")).is_some() {
                let Some(mut expression_nodes) = self.parse_braced_inline_content(terminators) else {
                    break;
                };
                nodes.append(&mut expression_nodes);
                continue;
            }

            if self.parser.parse_string("<>").is_some() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Glue));
                continue;
            }

            let Some(text) = self.content_text_allowing_escape_char() else {
                break;
            };
            if !text.is_empty() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(text));
            }
        }
        nodes
    }

    fn parse_braced_inline_content(&mut self, terminators: &str) -> Option<Vec<ParsedNode>> {
        let rule_id = self.parser.begin_rule();
        self.parser.parse_string("{")?;
        let content = self.parse_balanced_brace_body()?;
        self.parser.succeed_rule(rule_id, Some(()));

        parse_inner_logic_string(
            &content,
            terminators,
            self.parsing_choice,
            self.parsing_string_expression,
        )
    }

    /// Try to parse a gather line: `-` (not `->`) optionally followed by content.
    ///
    /// Returns a `ParsedNode::GatherPoint` with:
    /// - `indentation_depth`: number of `-` markers
    /// - `name`: optional label `(identifier)` after the markers
    /// - `children`: optional content nodes on the same line
    /// Try to parse a multiline sequence block:
    ///
    ///   { [type:] \n
    ///     - element1 content \n
    ///     - element2 content \n
    ///   }
    ///
    /// Returns a `Sequence` ParsedNode.
    fn parse_balanced_brace_body(&mut self) -> Option<String> {
        let mut depth = 1usize;
        let mut string_open = false;
        let mut content = String::new();

        while let Some(ch) = self.parser.parse_single_character() {
            match ch {
                '"' => {
                    string_open = !string_open;
                    content.push(ch);
                }
                '{' if !string_open => {
                    depth += 1;
                    content.push(ch);
                }
                '}' if !string_open => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(content);
                    }
                    content.push(ch);
                }
                _ => content.push(ch),
            }
        }

        None
    }

    fn inner_logic_nodes(&mut self, terminators: &str) -> Option<Vec<ParsedNode>> {
        let _ = self.whitespace();

        if let Some(explicit_seq_type) = self.try_parse_sequence_type_annotation() {
            let elements = self.inner_sequence_objects(terminators)?;
            return Some(vec![build_sequence_node(explicit_seq_type, elements)]);
        }

        if let Some(initial_query_expression) = self.condition_expression_text() {
            return self.inner_conditional_nodes(Some(initial_query_expression), terminators);
        }

        for rule in [
            InnerLogicRule::Conditional,
            InnerLogicRule::Sequence,
            InnerLogicRule::Expression,
        ] {
            let rule_id = self.parser.begin_rule();
            let result = match rule {
                InnerLogicRule::Conditional => self.inner_conditional_nodes(None, terminators),
                InnerLogicRule::Sequence => self.inner_sequence_nodes(terminators, SequenceType::Stopping as u8),
                InnerLogicRule::Expression => self.inner_expression_nodes(),
            };

            if let Some(nodes) = result {
                let _ = self.whitespace();
                if self.parser.end_of_input() {
                    return self.parser.succeed_rule(rule_id, Some(nodes));
                }
            }

            let _ = self.parser.fail_rule::<Vec<ParsedNode>>(rule_id);
        }

        None
    }

    fn inner_expression_nodes(&mut self) -> Option<Vec<ParsedNode>> {
        let expr = self.expression_until_top_level_terminators("")?;
        Some(vec![ParsedNode::new(ParsedNodeKind::OutputExpression).with_expression(expr)])
    }

    fn inner_sequence_nodes(&mut self, terminators: &str, default_flags: u8) -> Option<Vec<ParsedNode>> {
        let elements = self.inner_sequence_objects(terminators)?;
        if elements.len() <= 1 {
            return None;
        }
        Some(vec![build_sequence_node(default_flags, elements)])
    }

    fn inner_sequence_objects(&mut self, terminators: &str) -> Option<Vec<Vec<ParsedNode>>> {
        let multiline = self.newline().is_some();
        if multiline {
            self.inner_multiline_sequence_objects()
        } else {
            self.inner_inline_sequence_objects(terminators)
        }
    }

    fn inner_inline_sequence_objects(&mut self, terminators: &str) -> Option<Vec<Vec<ParsedNode>>> {
        let mut result = Vec::new();
        let mut just_had_content = false;

        loop {
            let content = self.parse_inline_content_until_with_brace_break(&format!("|}}{terminators}"));
            if !content.is_empty() {
                result.push(content);
                just_had_content = true;
            }

            if self.parser.parse_string("|").is_some() {
                if !just_had_content {
                    result.push(Vec::new());
                }
                just_had_content = false;
                continue;
            }

            break;
        }

        if !just_had_content {
            result.push(Vec::new());
        }

        (!result.is_empty()).then_some(result)
    }

    fn inner_multiline_sequence_objects(&mut self) -> Option<Vec<Vec<ParsedNode>>> {
        self.multiline_whitespace();

        let mut result = Vec::new();
        while let Some(content) = self.single_multiline_sequence_element() {
            result.push(content);
        }

        (!result.is_empty()).then_some(result)
    }

    fn single_multiline_sequence_element(&mut self) -> Option<Vec<ParsedNode>> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        if self.parser.peek(|parser| parser.parse_string("->")).is_some() {
            return self.parser.fail_rule(rule_id);
        }

        if self.parser.parse_string("-").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let mut content = self.statements_at_level(StatementLevel::InnerBlock).nodes;
        if content.is_empty() {
            self.multiline_whitespace();
        } else {
            content.insert(0, ParsedNode::new(ParsedNodeKind::Newline));
        }

        self.parser.succeed_rule(rule_id, Some(content))
    }

    fn inner_conditional_nodes(
        &mut self,
        initial_query_expression: Option<crate::parsed_hierarchy::ParsedExpression>,
        terminators: &str,
    ) -> Option<Vec<ParsedNode>> {
        let can_be_inline = initial_query_expression.is_some();
        let is_inline = self.newline().is_none();
        if is_inline && !can_be_inline {
            return None;
        }

        let mut alternatives = if is_inline {
            self.inline_conditional_branches(terminators)?
        } else {
            self.multiline_conditional_branches()?
        };

        for branch in &mut alternatives {
            branch.is_inline = is_inline;
        }

        Some(vec![build_conditional_node(initial_query_expression, alternatives)])
    }

    fn inline_conditional_branches(&mut self, terminators: &str) -> Option<Vec<ConditionalBranchSpec>> {
        let branches = self.inner_inline_sequence_objects(terminators)?;
        if branches.is_empty() || branches.len() > 2 {
            return None;
        }

        let mut result = Vec::new();
        let mut true_branch = ConditionalBranchSpec::from_nodes(branches[0].clone());
        true_branch.is_true_branch = true;
        result.push(true_branch);

        if branches.len() > 1 {
            let mut else_branch = ConditionalBranchSpec::from_nodes(branches[1].clone());
            else_branch.is_else = true;
            result.push(else_branch);
        }

        Some(result)
    }

    fn multiline_conditional_branches(&mut self) -> Option<Vec<ConditionalBranchSpec>> {
        self.multiline_whitespace();

        let mut branches = Vec::new();
        while let Some(branch) = self.single_multiline_condition() {
            branches.push(branch);
        }

        self.multiline_whitespace();
        (!branches.is_empty()).then_some(branches)
    }

    fn single_multiline_condition(&mut self) -> Option<ConditionalBranchSpec> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        if self.parser.peek(|parser| parser.parse_string("->")).is_some() {
            return self.parser.fail_rule(rule_id);
        }

        if self.parser.parse_string("-").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let is_else = self.else_expression();
        let condition = if is_else {
            None
        } else {
            self.condition_expression_text()
        };

        let mut content = self.statements_at_level(StatementLevel::InnerBlock).nodes;
        if condition.is_none() && content.is_empty() {
            content.push(ParsedNode::new(ParsedNodeKind::Text).with_text(""));
        }

        self.multiline_whitespace();

        self.parser.succeed_rule(
            rule_id,
            Some(ConditionalBranchSpec {
                condition,
                content: nodes_to_block_text(&content),
                is_else,
                is_inline: false,
                is_true_branch: false,
                matching_equality: false,
            }),
        )
    }

    fn condition_expression_text(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(expr) = self.expression_until_top_level_terminators(":") else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        if self.parser.parse_string(":").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.parser.succeed_rule(rule_id, Some(expr))
    }

    fn else_expression(&mut self) -> bool {
        let rule_id = self.parser.begin_rule();
        if self.parse_identifier().as_deref() != Some("else") {
            let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
            return false;
        }

        self.whitespace();
        if self.parser.parse_string(":").is_none() {
            let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
            return false;
        }

        self.parser.succeed_rule(rule_id, Some(ParseSuccess)).is_some()
    }

    fn parse_inline_content_until_with_brace_break(&mut self, terminators: &str) -> Vec<ParsedNode> {
        self.parse_inline_content_until(terminators)
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

        if terminators.is_empty() {
            Some(result)
        } else {
            None
        }
    }

    fn try_parse_sequence_type_annotation(&mut self) -> Option<u8> {
        let rule_id = self.parser.begin_rule();
        let mut combined = 0u8;
        let mut matched_any = false;
        loop {
            self.whitespace();
            let matched = if self.parser.parse_string("stopping").is_some() {
                combined |= SequenceType::Stopping as u8;
                true
            } else if self.parser.parse_string("cycle").is_some() {
                combined |= SequenceType::Cycle as u8;
                true
            } else if self.parser.parse_string("shuffle").is_some() {
                combined |= SequenceType::Shuffle as u8;
                true
            } else if self.parser.parse_string("once").is_some() {
                combined |= SequenceType::Once as u8;
                true
            } else {
                false
            };

            if !matched {
                break;
            }
            matched_any = true;
        }

        if matched_any {
            self.whitespace();
            if self.parser.parse_string(":").is_some() {
                return self.parser.succeed_rule(rule_id, Some(combined));
            }
        }
        let _ = self.parser.fail_rule::<u8>(rule_id);

        let rule_id = self.parser.begin_rule();

        let mut symbol_combined = 0u8;
        let mut saw_symbol = false;
        loop {
            let matched = if self.parser.parse_string("!").is_some() {
                symbol_combined |= SequenceType::Once as u8;
                true
            } else if self.parser.parse_string("&").is_some() {
                symbol_combined |= SequenceType::Cycle as u8;
                true
            } else if self.parser.parse_string("~").is_some() {
                symbol_combined |= SequenceType::Shuffle as u8;
                true
            } else if self.parser.parse_string("$").is_some() {
                symbol_combined |= SequenceType::Stopping as u8;
                true
            } else {
                false
            };
            if !matched {
                break;
            }
            saw_symbol = true;
        }

        if saw_symbol {
            self.parser.succeed_rule(rule_id, Some(symbol_combined))
        } else {
            self.parser.fail_rule(rule_id)
        }
    }

    fn try_parse_gather(&mut self) -> Option<ParsedNode> {        let rule_id = self.parser.begin_rule();
        let marker = self.gather_marker();
        let Some(marker) = marker else {
            return self.parser.fail_rule(rule_id);
        };
        let depth = marker.indentation_depth;
        let label = marker.identifier;

        // Optional horizontal whitespace
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        // Optional content on the same line (text + optional divert)
        let mut content_nodes = Vec::new();
        let mut line_nodes: Vec<ParsedNode> = Vec::new();
        if let Some(text) = self.content_text() {
            let trimmed = text.trim_end_matches([' ', '\t']).to_owned();
            if !trimmed.is_empty() {
                line_nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(trimmed));
            }
        }
        if let Some(divert) = self.try_parse_inline_divert() {
            if !line_nodes.is_empty() {
                Self::append_trailing_space(&mut line_nodes);
            }
            line_nodes.push(divert);
        }
        if !line_nodes.is_empty() {
            line_nodes.push(ParsedNode::new(ParsedNodeKind::Newline));
            content_nodes.append(&mut line_nodes);
        }
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        let mut node = ParsedNode::new(if label.is_some() {
            ParsedNodeKind::GatherLabel
        } else {
            ParsedNodeKind::GatherPoint
        });
        node.indentation_depth = depth;
        if let Some(label_name) = label {
            node = node.with_name(label_name);
        }
        if !content_nodes.is_empty() {
            node = node.with_children(content_nodes);
        }
        Some(node)
    }

    /// Try to parse a parenthesised inline label: `(identifier)`.
    fn try_parse_inline_label(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let Some(ident) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if self.parser.parse_string(")").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.parser.succeed_rule(rule_id, Some(ident))
    }

    /// Skip to the end of the current line (fallback for unrecognised content).
    fn skip_line(&mut self) {
        let _ = self
            .parser
            .parse_until_characters_from_string("\n\r", -1);
        let _ = self.parser.parse_newline();
    }
}

fn trim_end_whitespace_nodes(mut nodes: Vec<ParsedNode>) -> Vec<ParsedNode> {
    if let Some(last) = nodes.last_mut()
        && last.kind() == ParsedNodeKind::Text
    {
        let trimmed = last
            .text()
            .unwrap_or("")
            .trim_end_matches([' ', '\t'])
            .to_owned();
        if trimmed.is_empty() {
            nodes.pop();
        } else {
            *last = ParsedNode::new(ParsedNodeKind::Text).with_text(trimmed);
        }
    }
    nodes
}

fn should_close_quoted_choice(
    start_nodes: &[ParsedNode],
    choice_only_nodes: &[ParsedNode],
    inner_nodes: &[ParsedNode],
) -> bool {
    if choice_only_nodes.is_empty() {
        return false;
    }

    let starts_with_quote = start_nodes
        .first()
        .and_then(|node| node.text())
        .is_some_and(|text| text.starts_with('\''));
    let inner_starts_with_comma_quote = inner_nodes
        .first()
        .and_then(|node| node.text())
        .is_some_and(|text| text.starts_with(",'"));

    starts_with_quote && inner_starts_with_comma_quote
}

fn append_text_suffix(nodes: &mut [ParsedNode], suffix: &str) {
    if let Some(last) = nodes.last_mut()
        && last.kind() == ParsedNodeKind::Text
    {
        let mut text = last.text().unwrap_or("").to_owned();
        text.push_str(suffix);
        *last = ParsedNode::new(ParsedNodeKind::Text).with_text(text);
    }
}

fn parse_inner_logic_string(
    input: &str,
    terminators: &str,
    parsing_choice: bool,
    parsing_string_expression: bool,
) -> Option<Vec<ParsedNode>> {
    let mut parser = InkParser::new(input, None);
    parser.parsing_choice = parsing_choice;
    parser.parsing_string_expression = parsing_string_expression;
    let nodes = parser.inner_logic_nodes(terminators)?;
    let _ = parser.whitespace();
    if !parser.parser.end_of_input() {
        return None;
    }
    Some(nodes)
}

#[derive(Clone, Copy)]
enum InnerLogicRule {
    Conditional,
    Sequence,
    Expression,
}

#[derive(Clone, Copy)]
struct InfixOperator {
    token: &'static str,
    kind: &'static str,
    precedence: i32,
    require_whitespace_after: bool,
}

#[derive(Clone)]
struct ConditionalBranchSpec {
    condition: Option<crate::parsed_hierarchy::ParsedExpression>,
    content: String,
    is_else: bool,
    is_inline: bool,
    is_true_branch: bool,
    matching_equality: bool,
}

impl ConditionalBranchSpec {
    fn from_nodes(nodes: Vec<ParsedNode>) -> Self {
        Self {
            condition: None,
            content: nodes_to_block_text(&nodes),
            is_else: false,
            is_inline: true,
            is_true_branch: false,
            matching_equality: false,
        }
    }
}

fn build_sequence_node(sequence_type: u8, elements: Vec<Vec<ParsedNode>>) -> ParsedNode {
    let element_children = elements
        .into_iter()
        .map(|nodes| ParsedNode::new(ParsedNodeKind::Text).with_text("").with_children(nodes))
        .collect();
    let mut seq_node = ParsedNode::new(ParsedNodeKind::Sequence);
    seq_node.sequence_type = sequence_type;
    seq_node.set_children(element_children);
    seq_node
}

fn build_conditional_node(
    initial_condition: Option<crate::parsed_hierarchy::ParsedExpression>,
    mut branches: Vec<ConditionalBranchSpec>,
) -> ParsedNode {
    let mut saw_branch_condition = false;
    let mut children = Vec::new();

    for (idx, branch) in branches.iter_mut().enumerate() {
        let mut branch_node = ParsedNode::new(ParsedNodeKind::Conditional);
        branch_node.is_inline = branch.is_inline;
        branch_node.is_else = branch.is_else;
        branch_node.is_true_branch = branch.is_true_branch;
        branch_node.matching_equality = branch.matching_equality;
        if let Some(condition) = branch.condition.clone() {
            branch_node = branch_node.with_condition(condition);
        }
        branch_node.set_children(parse_nested_statement_block(&branch.content));

        if initial_condition.is_some() {
            if branch.condition.is_some() {
                branch_node.matching_equality = true;
                saw_branch_condition = true;
            } else if saw_branch_condition && branch.is_else {
                branch_node.matching_equality = true;
            } else if idx == 0 {
                branch_node.is_true_branch = true;
            } else {
                branch_node.is_else = true;
            }
        } else if branch.is_else {
            branch_node.is_else = true;
        }

        children.push(branch_node);
    }

    let mut node = ParsedNode::new(if initial_condition.is_some() {
        ParsedNodeKind::SwitchConditional
    } else {
        ParsedNodeKind::Conditional
    });
    if let Some(condition) = initial_condition {
        node = node.with_condition(condition);
    }
    node.set_children(children);
    node
}

fn nodes_to_block_text(nodes: &[ParsedNode]) -> String {
    let mut text = String::new();
    for node in nodes {
        match node.kind() {
            ParsedNodeKind::Text => text.push_str(node.text().unwrap_or("")),
            ParsedNodeKind::Newline => text.push('\n'),
            _ => {}
        }
    }
    text
}

fn is_reserved_keyword(name: &str) -> bool {
    matches!(
        name,
        "true" | "false" | "not" | "return" | "else" | "VAR" | "CONST" | "temp" | "LIST" | "function"
    )
}

fn is_number_only_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_digit())
}

fn infix_operators() -> &'static [InfixOperator] {
    &[
        InfixOperator { token: "&&", kind: "And", precedence: 1, require_whitespace_after: false },
        InfixOperator { token: "||", kind: "Or", precedence: 1, require_whitespace_after: false },
        InfixOperator { token: "and", kind: "And", precedence: 1, require_whitespace_after: true },
        InfixOperator { token: "or", kind: "Or", precedence: 1, require_whitespace_after: true },
        InfixOperator { token: "==", kind: "Equal", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: ">=", kind: "GreaterEqual", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: "<=", kind: "LessEqual", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: "<", kind: "Less", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: ">", kind: "Greater", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: "!=", kind: "NotEqual", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: "!?", kind: "Hasnt", precedence: 3, require_whitespace_after: false },
        InfixOperator { token: "?", kind: "Has", precedence: 3, require_whitespace_after: false },
        InfixOperator { token: "has", kind: "Has", precedence: 3, require_whitespace_after: true },
        InfixOperator { token: "hasnt", kind: "Hasnt", precedence: 3, require_whitespace_after: true },
        InfixOperator { token: "^", kind: "Intersect", precedence: 3, require_whitespace_after: false },
        InfixOperator { token: "+", kind: "Add", precedence: 4, require_whitespace_after: false },
        InfixOperator { token: "-", kind: "Subtract", precedence: 5, require_whitespace_after: false },
        InfixOperator { token: "*", kind: "Multiply", precedence: 6, require_whitespace_after: false },
        InfixOperator { token: "/", kind: "Divide", precedence: 7, require_whitespace_after: false },
        InfixOperator { token: "%", kind: "Modulo", precedence: 8, require_whitespace_after: false },
        InfixOperator { token: "mod", kind: "Modulo", precedence: 8, require_whitespace_after: true },
    ]
}

fn parse_nested_statement_block(content: &str) -> Vec<ParsedNode> {
    let mut parser = InkParser::new(content, None);
    let parsed = parser.statements_at_level(StatementLevel::InnerBlock);
    parsed.nodes
}

fn merge_parse_section(into: &mut ParseSection, mut other: ParseSection) {
    into.nodes.append(&mut other.nodes);
    into.flows.append(&mut other.flows);
    into.global_declarations.append(&mut other.global_declarations);
    into.global_initializers.append(&mut other.global_initializers);
    into.list_definitions.append(&mut other.list_definitions);
    into.external_declarations.append(&mut other.external_declarations);
}

fn build_story_from_section(
    source: String,
    source_name: Option<String>,
    count_all_visits: bool,
    parsed: ParseSection,
) -> Story {
    let mut story = Story::new(&source, source_name, count_all_visits);
    story.root_nodes = parsed.nodes;
    story.flows = parsed.flows;
    story.global_declarations = parsed.global_declarations;
    story.global_initializers = parsed.global_initializers;
    story.list_definitions = parsed.list_definitions;
    story.external_declarations = parsed.external_declarations;
    story.rebuild_parse_tree_refs();
    story
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
}
