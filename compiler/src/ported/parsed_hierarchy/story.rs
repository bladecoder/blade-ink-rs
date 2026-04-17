use std::{collections::HashMap, rc::Rc};

use bladeink::compiler_support::{Container, RTObject};

use crate::error::CompilerError;

use super::{choice::Choice, content_list::ContentList};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Story {
    pub leading_content: ContentList,
    pub choices: Vec<Choice>,
    pub continuation_content: ContentList,
    pub named_flows: Vec<NamedFlow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedFlow {
    pub name: String,
    pub story: Box<Story>,
}

pub type TopLevelNamedContainers = HashMap<String, Rc<Container>>;

impl Story {
    pub fn from_legacy(
        story: &crate::parsed_hierarchy::ParsedStory,
    ) -> Result<Self, CompilerError> {
        if !story.globals().is_empty() {
            return Err(CompilerError::UnsupportedFeature(
                "ported story adapter does not support globals yet".to_owned(),
            ));
        }

        let mut leading_nodes = Vec::new();
        let mut continuation_nodes = Vec::new();
        let mut choices = Vec::new();
        let mut seen_choice = false;
        let mut after_choices = false;

        for node in story.root() {
            match node {
                crate::parsed_hierarchy::Node::Choice(choice) => {
                    if after_choices {
                        return Err(CompilerError::UnsupportedFeature(
                            "ported story adapter does not support choices after root continuation content"
                                .to_owned(),
                        ));
                    }
                    seen_choice = true;
                    choices.push(Choice::from_legacy(choice)?);
                }
                _ if seen_choice => {
                    after_choices = true;
                    continuation_nodes.push(node.clone());
                }
                _ => leading_nodes.push(node.clone()),
            }
        }

        let continuation_content = normalize_invisible_default_continuation(
            &mut choices,
            ContentList::from_legacy_nodes(&continuation_nodes)?,
        );

        Ok(Self {
            leading_content: ContentList::from_legacy_nodes(&leading_nodes)?,
            choices,
            continuation_content,
            named_flows: story
                .flows()
                .iter()
                .map(named_flow_from_legacy)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub fn to_root_container(&self) -> Result<Rc<Container>, CompilerError> {
        Ok(self.to_runtime_story_root()?.0)
    }

    pub fn to_runtime_story_root(
        &self,
    ) -> Result<(Rc<Container>, TopLevelNamedContainers), CompilerError> {
        let main_content = self.build_scope_container("0", None)?;
        let mut top_level_named = TopLevelNamedContainers::new();
        for flow in &self.named_flows {
            top_level_named.insert(flow.name.clone(), flow.to_runtime_container()?);
        }

        Ok((main_content, top_level_named))
    }

    fn build_scope_container(
        &self,
        scope_path: &str,
        name: Option<String>,
    ) -> Result<Rc<Container>, CompilerError> {
        let mut content = self.leading_content.to_runtime()?;
        let mut named = HashMap::new();

        let continuation_name = "g-0";
        for (choice_index, choice) in self.choices.iter().enumerate() {
            let runtime = choice.generate_runtime(
                scope_path,
                content.len(),
                choice_index,
                continuation_name,
            )?;
            content.push(runtime.outer as Rc<dyn RTObject>);
            named.insert(format!("c-{choice_index}"), runtime.inner);
        }

        let mut continuation_content = self.continuation_content.to_runtime()?;
        if !continuation_is_terminal(&self.continuation_content) {
            continuation_content.push(Rc::new(bladeink::compiler_support::ControlCommand::new(
                bladeink::compiler_support::CommandType::Done,
            )) as Rc<dyn RTObject>);
        }
        let continuation = Container::new(
            Some(continuation_name.to_owned()),
            0,
            continuation_content,
            HashMap::new(),
        );
        named.insert(continuation_name.to_owned(), continuation);

        Ok(Container::new(name, 0, content, named))
    }
}

impl NamedFlow {
    fn to_runtime_container(&self) -> Result<Rc<Container>, CompilerError> {
        let main_content = self
            .story
            .build_scope_container(&format!("{}.0", self.name), None)?;
        Ok(Container::new(
            Some(self.name.clone()),
            0,
            vec![main_content as Rc<dyn RTObject>],
            HashMap::new(),
        ))
    }
}

fn named_flow_from_legacy(
    flow: &crate::parsed_hierarchy::Flow,
) -> Result<NamedFlow, CompilerError> {
    if !flow.parameters.is_empty() {
        return Err(CompilerError::UnsupportedFeature(
            "ported story adapter does not support flow parameters yet".to_owned(),
        ));
    }

    if !flow.children.is_empty() {
        return Err(CompilerError::UnsupportedFeature(
            "ported story adapter does not support nested child flows yet".to_owned(),
        ));
    }

    let mut leading_nodes = Vec::new();
    let mut continuation_nodes = Vec::new();
    let mut choices = Vec::new();
    let mut seen_choice = false;
    let mut after_choices = false;

    for node in &flow.nodes {
        match node {
            crate::parsed_hierarchy::Node::Choice(choice) => {
                if after_choices {
                    return Err(CompilerError::UnsupportedFeature(
                        "ported flow adapter does not support choices after continuation content"
                            .to_owned(),
                    ));
                }
                seen_choice = true;
                choices.push(Choice::from_legacy(choice)?);
            }
            _ if seen_choice => {
                after_choices = true;
                continuation_nodes.push(node.clone());
            }
            _ => leading_nodes.push(node.clone()),
        }
    }

    Ok(NamedFlow {
        name: flow.name.clone(),
        story: Box::new(Story {
            leading_content: ContentList::from_legacy_nodes(&leading_nodes)?,
            continuation_content: normalize_invisible_default_continuation(
                &mut choices,
                ContentList::from_legacy_nodes(&continuation_nodes)?,
            ),
            choices,
            named_flows: Vec::new(),
        }),
    })
}

fn normalize_invisible_default_continuation(
    choices: &mut [Choice],
    continuation_content: ContentList,
) -> ContentList {
    if !continuation_content.items.is_empty() {
        return continuation_content;
    }

    let Some(last_choice) = choices.last_mut() else {
        return continuation_content;
    };

    if !last_choice.is_invisible_default || !last_choice.once_only {
        return continuation_content;
    }

    if last_choice.inner_content.items.is_empty() {
        return continuation_content;
    }

    let moved = std::mem::take(&mut last_choice.inner_content);
    last_choice.inner_content = ContentList::new(vec![super::content_list::ContentItem::Newline]);
    moved
}

fn continuation_is_terminal(content: &ContentList) -> bool {
    matches!(
        content.items.last(),
        Some(
            super::content_list::ContentItem::End
                | super::content_list::ContentItem::Done
                | super::content_list::ContentItem::Divert(_)
        )
    )
}
