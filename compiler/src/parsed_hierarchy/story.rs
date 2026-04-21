use super::{
    Content, ContentList, ExternalDeclaration, FlowArgument, FlowBase, FlowLevel, ListDefinition,
    ParsedExpression, ParsedFlow, ParsedNode, ParsedObject, ParsedObjectIndex, VariableAssignment,
    ConstDeclaration,
};

#[derive(Debug, Clone)]
pub struct Story {
    flow: FlowBase,
    source: String,
    source_filename: Option<String>,
    pub count_all_visits: bool,
    root_content: ContentList,
    pub(crate) global_declarations: Vec<VariableAssignment>,
    pub(crate) global_initializers: Vec<(String, ParsedExpression)>,
    pub(crate) list_definitions: Vec<ListDefinition>,
    pub(crate) external_declarations: Vec<ExternalDeclaration>,
    pub(crate) const_declarations: Vec<ConstDeclaration>,
    pub(crate) root_nodes: Vec<ParsedNode>,
    pub(crate) flows: Vec<ParsedFlow>,
}

impl Story {
    pub fn new(source: &str, source_filename: Option<String>, count_all_visits: bool) -> Self {
        let mut flow = FlowBase::new(FlowLevel::Story, None, Vec::<FlowArgument>::new(), false);
        let mut root_content = ContentList::new();
        root_content.object_mut().set_parent(flow.object());
        flow.object_mut()
            .add_content_ref(root_content.object().reference());
        Self {
            flow,
            source: source.to_owned(),
            source_filename,
            count_all_visits,
            root_content,
            global_declarations: Vec::new(),
            global_initializers: Vec::new(),
            list_definitions: Vec::new(),
            external_declarations: Vec::new(),
            const_declarations: Vec::new(),
            root_nodes: Vec::new(),
            flows: Vec::new(),
        }
    }

    pub fn object(&self) -> &ParsedObject {
        self.flow.object()
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        self.flow.object_mut()
    }

    pub fn flow(&self) -> &FlowBase {
        &self.flow
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn source_filename(&self) -> Option<&str> {
        self.source_filename.as_deref()
    }

    pub fn root_content(&self) -> &ContentList {
        &self.root_content
    }

    pub fn root_content_mut(&mut self) -> &mut ContentList {
        &mut self.root_content
    }

    pub fn global_declarations(&self) -> &[VariableAssignment] {
        &self.global_declarations
    }

    pub fn global_initializers(&self) -> &[(String, ParsedExpression)] {
        &self.global_initializers
    }

    pub fn list_definitions(&self) -> &[ListDefinition] {
        &self.list_definitions
    }

    pub fn external_declarations(&self) -> &[ExternalDeclaration] {
        &self.external_declarations
    }

    pub fn const_declarations(&self) -> &[ConstDeclaration] {
        &self.const_declarations
    }

    pub fn const_declaration(&self, name: &str) -> Option<&ConstDeclaration> {
        self.const_declarations
            .iter()
            .find(|declaration| declaration.name() == name)
    }

    pub fn resolve_list_item(&self, item_name: &str) -> Option<(String, i32)> {
        if let Some((list_name, item_name)) = item_name.split_once('.') {
            let definition = self
                .list_definitions
                .iter()
                .find(|definition| definition.identifier() == Some(list_name))?;
            let item = definition.item_named(item_name)?;
            return Some((item.full_name(list_name), item.series_value()));
        }

        for definition in &self.list_definitions {
            let Some(list_name) = definition.identifier() else {
                continue;
            };
            if let Some(item) = definition.item_named(item_name) {
                return Some((item.full_name(list_name), item.series_value()));
            }
        }

        None
    }

    pub fn root_nodes(&self) -> &[ParsedNode] {
        &self.root_nodes
    }

    pub fn parsed_flows(&self) -> &[ParsedFlow] {
        &self.flows
    }

    pub fn object_index(&self) -> ParsedObjectIndex {
        let mut index = ParsedObjectIndex::new();
        index.register(self.object());
        self.register_content_list(&mut index, &self.root_content);
        for node in &self.root_nodes {
            self.register_parsed_node(&mut index, node);
        }
        for flow in &self.flows {
            self.register_parsed_flow(&mut index, flow);
        }
        index
    }

    fn register_content_list(&self, index: &mut ParsedObjectIndex, content_list: &ContentList) {
        index.register(content_list.object());
        for content in content_list.content() {
            match content {
                Content::Text(text) => index.register(text.object()),
            }
        }
    }

    pub(crate) fn rebuild_parse_tree_refs(&mut self) {
        let story_ref = self.object().reference();
        for node in &mut self.root_nodes {
            node.object_mut().set_parent_ref(story_ref);
            self.flow
                .object_mut()
                .add_content_ref(node.object().reference());
        }
        for flow in &mut self.flows {
            flow.object_mut().set_parent_ref(story_ref);
            self.flow
                .object_mut()
                .add_content_ref(flow.object().reference());
        }
    }

    fn register_parsed_node(&self, index: &mut ParsedObjectIndex, node: &ParsedNode) {
        index.register(node.object());
        for child in node.children() {
            self.register_parsed_node(index, child);
        }
    }

    fn register_parsed_flow(&self, index: &mut ParsedObjectIndex, flow: &ParsedFlow) {
        index.register(flow.object());
        for node in flow.content() {
            self.register_parsed_node(index, node);
        }
        for child in flow.children() {
            self.register_parsed_flow(index, child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Story;
    use crate::parsed_hierarchy::{Content, ContentList, DebugMetadata};

    #[test]
    fn story_sets_root_content_parent() {
        let story = Story::new("hello", Some("main.ink".to_owned()), true);
        assert_eq!(
            Some(story.object().id()),
            story.root_content().object().parent_id()
        );
    }

    #[test]
    fn content_list_trims_trailing_inline_whitespace() {
        let mut list = ContentList::new();
        list.push_text("hello \t");
        list.trim_trailing_whitespace();
        let text = match &list.content()[0] {
            Content::Text(text) => text.text(),
        };
        assert_eq!("hello", text);
    }

    #[test]
    fn object_can_hold_debug_metadata() {
        let mut story = Story::new("hello", None, true);
        story.object_mut().set_debug_metadata(DebugMetadata {
            start_line_number: 1,
            end_line_number: 1,
            start_character_number: 1,
            end_character_number: 5,
            file_name: Some("main.ink".to_owned()),
        });
        assert!(story.object().has_own_debug_metadata());
    }

    #[test]
    fn story_object_index_resolves_root_content_tree() {
        let mut story = Story::new("hello", None, true);
        story.root_content_mut().push_text("hello");
        let index = story.object_index();
        let text = story
            .object()
            .find(&index, crate::parsed_hierarchy::ObjectKind::Text);
        assert!(text.is_some());
        assert_eq!(
            Some(story.object().reference()),
            index.story_for(text.unwrap())
        );
    }
}
