use crate::{
    error::CompilerError,
    file_handler::{DefaultFileHandler, FileHandler},
    parsed_hierarchy::{SequenceType, Story},
    string_parser::{CommentEliminator, StringParser},
};
use std::{cell::RefCell, collections::HashSet, path::Path, rc::Rc};

mod author_warning;
mod basics;
mod choices;
mod conditional;
mod content;
mod declarations;
mod divert;
mod expressions;
mod flow;
mod gather;
mod include;
mod logic;
mod sequences;
mod shared;
mod statements;
mod tags;
mod text;
mod command_line;
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




    // ─────────────────────────────────────────────────────────────────────────
    // Statement parsing
    // ─────────────────────────────────────────────────────────────────────────

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
