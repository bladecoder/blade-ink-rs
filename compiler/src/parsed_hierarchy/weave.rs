use super::{ContentList, ObjectKind, ParsedObject};

#[derive(Debug, Clone)]
pub struct Choice {
    object: ParsedObject,
    identifier: Option<String>,
    indentation_depth: usize,
    has_weave_style_inline_brackets: bool,
    once_only: bool,
    is_invisible_default: bool,
    start_content: Option<ContentList>,
    choice_only_content: Option<ContentList>,
    inner_content: ContentList,
}

impl Choice {
    pub fn new(
        indentation_depth: usize,
        once_only: bool,
        identifier: Option<String>,
        start_content: Option<ContentList>,
        choice_only_content: Option<ContentList>,
        inner_content: ContentList,
    ) -> Self {
        let mut choice = Self {
            object: ParsedObject::new(ObjectKind::Choice),
            identifier,
            indentation_depth,
            has_weave_style_inline_brackets: choice_only_content.is_some(),
            once_only,
            is_invisible_default: false,
            start_content,
            choice_only_content,
            inner_content,
        };
        choice.set_content_parents();
        choice
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub fn indentation_depth(&self) -> usize {
        self.indentation_depth
    }

    pub fn has_weave_style_inline_brackets(&self) -> bool {
        self.has_weave_style_inline_brackets
    }

    pub fn once_only(&self) -> bool {
        self.once_only
    }

    pub fn is_invisible_default(&self) -> bool {
        self.is_invisible_default
    }

    pub fn set_invisible_default(&mut self, value: bool) {
        self.is_invisible_default = value;
    }

    pub fn start_content(&self) -> Option<&ContentList> {
        self.start_content.as_ref()
    }

    pub fn choice_only_content(&self) -> Option<&ContentList> {
        self.choice_only_content.as_ref()
    }

    pub fn inner_content(&self) -> &ContentList {
        &self.inner_content
    }

    fn set_content_parents(&mut self) {
        if let Some(start) = self.start_content.as_mut() {
            start.object_mut().set_parent(&self.object);
            self.object.add_content_ref(start.object().reference());
        }
        if let Some(choice_only) = self.choice_only_content.as_mut() {
            choice_only.object_mut().set_parent(&self.object);
            self.object
                .add_content_ref(choice_only.object().reference());
        }
        self.inner_content.object_mut().set_parent(&self.object);
        self.object
            .add_content_ref(self.inner_content.object().reference());
    }
}

#[derive(Debug, Clone)]
pub struct Gather {
    object: ParsedObject,
    identifier: Option<String>,
    indentation_depth: usize,
    content: Option<ContentList>,
}

impl Gather {
    pub fn new(
        indentation_depth: usize,
        identifier: Option<String>,
        mut content: Option<ContentList>,
    ) -> Self {
        let mut object = ParsedObject::new(ObjectKind::Gather);
        if let Some(content) = content.as_mut() {
            content.object_mut().set_parent(&object);
            object.add_content_ref(content.object().reference());
        }
        Self {
            object,
            identifier,
            indentation_depth,
            content,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub fn indentation_depth(&self) -> usize {
        self.indentation_depth
    }

    pub fn content(&self) -> Option<&ContentList> {
        self.content.as_ref()
    }
}

#[derive(Debug, Clone)]
pub enum WeaveElement {
    Content(ContentList),
    Choice(Choice),
    Gather(Gather),
    NestedWeave(Weave),
}

#[derive(Debug, Clone)]
pub struct Weave {
    object: ParsedObject,
    base_indentation_index: usize,
    elements: Vec<WeaveElement>,
}

impl Weave {
    pub fn new(base_indentation_index: usize) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::Weave),
            base_indentation_index,
            elements: Vec::new(),
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn base_indentation_index(&self) -> usize {
        self.base_indentation_index
    }

    pub fn elements(&self) -> &[WeaveElement] {
        &self.elements
    }

    pub fn push(&mut self, mut element: WeaveElement) {
        let parent = self.object.reference();
        let child_ref = match &mut element {
            WeaveElement::Content(content) => {
                content.object_mut().set_parent_ref(parent);
                content.object().reference()
            }
            WeaveElement::Choice(choice) => {
                choice.object_mut().set_parent_ref(parent);
                choice.object().reference()
            }
            WeaveElement::Gather(gather) => {
                gather.object_mut().set_parent_ref(parent);
                gather.object().reference()
            }
            WeaveElement::NestedWeave(weave) => {
                weave.object_mut().set_parent_ref(parent);
                weave.object().reference()
            }
        };
        self.object.add_content_ref(child_ref);
        self.elements.push(element);
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, Gather, Weave, WeaveElement};
    use crate::parsed_hierarchy::ContentList;

    #[test]
    fn choice_sets_parent_on_content_lists() {
        let mut start = ContentList::new();
        start.push_text("start");
        let mut inner = ContentList::new();
        inner.push_text("inner");

        let choice = Choice::new(1, true, None, Some(start), None, inner);

        assert_eq!(
            Some(choice.object().id()),
            choice
                .start_content()
                .map(|c| c.object().parent_id())
                .flatten()
        );
        assert_eq!(
            Some(choice.object().id()),
            choice.inner_content().object().parent_id()
        );
        assert!(choice.once_only());
    }

    #[test]
    fn gather_sets_parent_on_optional_content() {
        let mut content = ContentList::new();
        content.push_text("join");
        let gather = Gather::new(1, None, Some(content));
        assert_eq!(
            Some(gather.object().id()),
            gather.content().map(|c| c.object().parent_id()).flatten()
        );
    }

    #[test]
    fn weave_sets_parent_on_inserted_elements() {
        let mut weave = Weave::new(0);
        weave.push(WeaveElement::Gather(Gather::new(1, None, None)));
        assert_eq!(1, weave.elements().len());
    }
}
