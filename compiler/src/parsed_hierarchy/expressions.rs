use super::{ContentList, List, ObjectKind, ParsedObject};

#[derive(Debug, Clone, PartialEq)]
pub enum NumberValue {
    Int(i32),
    Float(f32),
    Bool(bool),
}

#[derive(Debug, Clone)]
pub struct Expression {
    object: ParsedObject,
    output_when_complete: bool,
}

impl Expression {
    pub fn new(kind: ObjectKind) -> Self {
        Self {
            object: ParsedObject::new(kind),
            output_when_complete: false,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn output_when_complete(&self) -> bool {
        self.output_when_complete
    }

    pub fn set_output_when_complete(&mut self, value: bool) {
        self.output_when_complete = value;
    }
}

#[derive(Debug, Clone)]
pub struct Number {
    expression: Expression,
    value: NumberValue,
}

impl Number {
    pub fn new(value: NumberValue) -> Self {
        Self {
            expression: Expression::new(ObjectKind::Number),
            value,
        }
    }

    pub fn expression(&self) -> &Expression {
        &self.expression
    }

    pub fn value(&self) -> &NumberValue {
        &self.value
    }
}

#[derive(Debug, Clone)]
pub struct StringExpression {
    expression: Expression,
    content: ContentList,
}

impl StringExpression {
    pub fn new(mut content: ContentList) -> Self {
        let expression = Expression::new(ObjectKind::StringExpression);
        content.object_mut().set_parent_id(expression.object().id());
        Self {
            expression,
            content,
        }
    }

    pub fn expression(&self) -> &Expression {
        &self.expression
    }

    pub fn content(&self) -> &ContentList {
        &self.content
    }

    pub fn is_single_string(&self) -> bool {
        let [super::Content::Text(text)] = self.content.content() else {
            return false;
        };
        !text.text().contains('\n')
    }
}

#[derive(Debug, Clone)]
pub struct VariableReference {
    expression: Expression,
    path: Vec<String>,
}

impl VariableReference {
    pub fn new(path: Vec<String>) -> Self {
        Self {
            expression: Expression::new(ObjectKind::VariableReference),
            path,
        }
    }

    pub fn expression(&self) -> &Expression {
        &self.expression
    }

    pub fn path(&self) -> &[String] {
        &self.path
    }

    pub fn name(&self) -> String {
        self.path.join(".")
    }
}

#[derive(Debug, Clone)]
pub struct VariableAssignment {
    object: ParsedObject,
    variable_name: String,
    expression: Option<ExpressionNode>,
    is_global_declaration: bool,
    is_new_temporary_declaration: bool,
}

impl VariableAssignment {
    pub fn new(variable_name: impl Into<String>, expression: Option<ExpressionNode>) -> Self {
        let object = ParsedObject::new(ObjectKind::VariableAssignment);
        Self {
            object,
            variable_name: variable_name.into(),
            expression,
            is_global_declaration: false,
            is_new_temporary_declaration: false,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn variable_name(&self) -> &str {
        &self.variable_name
    }

    pub fn expression(&self) -> Option<&ExpressionNode> {
        self.expression.as_ref()
    }

    pub fn is_global_declaration(&self) -> bool {
        self.is_global_declaration
    }

    pub fn set_global_declaration(&mut self, value: bool) {
        self.is_global_declaration = value;
    }

    pub fn is_new_temporary_declaration(&self) -> bool {
        self.is_new_temporary_declaration
    }

    pub fn set_new_temporary_declaration(&mut self, value: bool) {
        self.is_new_temporary_declaration = value;
    }
}

#[derive(Debug, Clone)]
pub struct ConditionalSingleBranch {
    object: ParsedObject,
    own_expression: Option<ExpressionNode>,
    content: ContentList,
    is_true_branch: bool,
    is_else: bool,
    is_inline: bool,
}

impl ConditionalSingleBranch {
    pub fn new(own_expression: Option<ExpressionNode>, mut content: ContentList) -> Self {
        let object = ParsedObject::new(ObjectKind::ConditionalBranch);
        content.object_mut().set_parent_id(object.id());
        Self {
            object,
            own_expression,
            content,
            is_true_branch: false,
            is_else: false,
            is_inline: false,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn own_expression(&self) -> Option<&ExpressionNode> {
        self.own_expression.as_ref()
    }

    pub fn content(&self) -> &ContentList {
        &self.content
    }

    pub fn is_true_branch(&self) -> bool {
        self.is_true_branch
    }

    pub fn set_true_branch(&mut self, value: bool) {
        self.is_true_branch = value;
    }

    pub fn is_else(&self) -> bool {
        self.is_else
    }

    pub fn set_else(&mut self, value: bool) {
        self.is_else = value;
    }

    pub fn is_inline(&self) -> bool {
        self.is_inline
    }

    pub fn set_inline(&mut self, value: bool) {
        self.is_inline = value;
    }
}

#[derive(Debug, Clone)]
pub struct Conditional {
    object: ParsedObject,
    initial_condition: Option<ExpressionNode>,
    branches: Vec<ConditionalSingleBranch>,
}

impl Conditional {
    pub fn new(
        initial_condition: Option<ExpressionNode>,
        branches: Vec<ConditionalSingleBranch>,
    ) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::Conditional),
            initial_condition,
            branches,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn initial_condition(&self) -> Option<&ExpressionNode> {
        self.initial_condition.as_ref()
    }

    pub fn branches(&self) -> &[ConditionalSingleBranch] {
        &self.branches
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceType {
    Stopping = 1,
    Cycle = 2,
    Shuffle = 4,
    Once = 8,
}

#[derive(Debug, Clone)]
pub struct Sequence {
    object: ParsedObject,
    sequence_type: u8,
    elements: Vec<ContentList>,
}

impl Sequence {
    pub fn new(sequence_type: u8, elements: Vec<ContentList>) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::Sequence),
            sequence_type,
            elements,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn sequence_type(&self) -> u8 {
        self.sequence_type
    }

    pub fn elements(&self) -> &[ContentList] {
        &self.elements
    }
}

#[derive(Debug, Clone)]
pub struct Tag {
    object: ParsedObject,
    is_start: bool,
    in_choice: bool,
}

impl Tag {
    pub fn new(is_start: bool, in_choice: bool) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::Tag),
            is_start,
            in_choice,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn is_start(&self) -> bool {
        self.is_start
    }

    pub fn in_choice(&self) -> bool {
        self.in_choice
    }
}

#[derive(Debug, Clone)]
pub struct AuthorWarning {
    object: ParsedObject,
    message: String,
}

impl AuthorWarning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::AuthorWarning),
            message: message.into(),
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone)]
pub struct ConstDeclaration {
    object: ParsedObject,
    name: String,
    expression: ExpressionNode,
}

impl ConstDeclaration {
    pub fn new(name: impl Into<String>, expression: ExpressionNode) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::ConstDeclaration),
            name: name.into(),
            expression,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn expression(&self) -> &ExpressionNode {
        &self.expression
    }
}

#[derive(Debug, Clone)]
pub struct ExternalDeclaration {
    object: ParsedObject,
    name: String,
    argument_names: Vec<String>,
}

impl ExternalDeclaration {
    pub fn new(name: impl Into<String>, argument_names: Vec<String>) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::ExternalDeclaration),
            name: name.into(),
            argument_names,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn argument_names(&self) -> &[String] {
        &self.argument_names
    }
}

#[derive(Debug, Clone)]
pub enum ExpressionNode {
    Number(Number),
    StringExpression(StringExpression),
    VariableReference(VariableReference),
    List(List),
}

impl ExpressionNode {
    pub fn object(&self) -> &ParsedObject {
        match self {
            Self::Number(number) => number.expression().object(),
            Self::StringExpression(string) => string.expression().object(),
            Self::VariableReference(var_ref) => var_ref.expression().object(),
            Self::List(list) => list.expression().object(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Conditional, ConditionalSingleBranch, ConstDeclaration, ExpressionNode, Number,
        NumberValue, Sequence, StringExpression, Tag, VariableReference,
    };
    use crate::parsed_hierarchy::ContentList;

    #[test]
    fn string_expression_reports_single_string_shape() {
        let mut content = ContentList::new();
        content.push_text("hello");
        let string_expression = StringExpression::new(content);
        assert!(string_expression.is_single_string());
    }

    #[test]
    fn variable_reference_joins_path_segments() {
        let variable = VariableReference::new(vec!["knot".to_owned(), "stitch".to_owned()]);
        assert_eq!("knot.stitch", variable.name());
    }

    #[test]
    fn conditional_keeps_branches() {
        let mut content = ContentList::new();
        content.push_text("branch");
        let mut branch = ConditionalSingleBranch::new(None, content);
        branch.set_else(true);
        let conditional = Conditional::new(None, vec![branch]);
        assert_eq!(1, conditional.branches().len());
        assert!(conditional.branches()[0].is_else());
    }

    #[test]
    fn sequence_keeps_type_mask() {
        let sequence = Sequence::new(1 | 4, vec![ContentList::new(), ContentList::new()]);
        assert_eq!(5, sequence.sequence_type());
        assert_eq!(2, sequence.elements().len());
    }

    #[test]
    fn tag_tracks_start_and_choice_context() {
        let tag = Tag::new(true, true);
        assert!(tag.is_start());
        assert!(tag.in_choice());
    }

    #[test]
    fn const_declaration_holds_expression() {
        let constant = ConstDeclaration::new(
            "x",
            ExpressionNode::Number(Number::new(NumberValue::Int(3))),
        );
        assert_eq!("x", constant.name());
    }
}
