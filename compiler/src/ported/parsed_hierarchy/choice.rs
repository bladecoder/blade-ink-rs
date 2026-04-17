use std::{collections::HashMap, rc::Rc};

use bladeink::compiler_support::{
    ChoicePoint, Container, Path, PushPopType, RTObject, Value, VariableAssignment,
};

use crate::error::CompilerError;

use super::content_list::ContentList;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub start_content: Option<ContentList>,
    pub choice_only_content: Option<ContentList>,
    pub inner_content: ContentList,
    pub once_only: bool,
    pub is_invisible_default: bool,
}

pub struct ChoiceRuntime {
    pub outer: Rc<Container>,
    pub inner: Rc<Container>,
}

impl Choice {
    pub fn from_legacy(choice: &crate::parsed_hierarchy::Choice) -> Result<Self, CompilerError> {
        let start_content = if choice.start_text.is_empty() {
            None
        } else {
            Some(ContentList::new(vec![
                super::content_list::ContentItem::Text(choice.start_text.clone()),
            ]))
        };

        let choice_only_content = if choice.choice_only_text.is_empty() {
            None
        } else {
            Some(ContentList::new(vec![
                super::content_list::ContentItem::Text(choice.choice_only_text.clone()),
            ]))
        };

        let mut inner_content = ContentList::from_legacy_nodes(&choice.body)?;
        let selected_suffix = selected_suffix(choice);
        let has_selected_suffix = selected_suffix
            .as_ref()
            .map(|text| !text.is_empty())
            .unwrap_or(false);
        let ends_with_divert = matches!(
            inner_content.items.last(),
            Some(super::content_list::ContentItem::Divert(_))
        );

        if let Some(text) = selected_suffix.filter(|text| !text.is_empty()) {
            inner_content
                .items
                .insert(0, super::content_list::ContentItem::Text(text));
        }

        if choice.selected_text.is_some()
            && !inner_content.items.is_empty()
            && !matches!(
                inner_content.items.first(),
                Some(
                    super::content_list::ContentItem::Newline
                        | super::content_list::ContentItem::Divert(_)
                        | super::content_list::ContentItem::End
                        | super::content_list::ContentItem::Done
                )
            )
        {
            let insert_index = if has_selected_suffix { 1 } else { 0 };
            if !matches!(
                inner_content.items.get(insert_index),
                Some(
                    super::content_list::ContentItem::Newline
                        | super::content_list::ContentItem::Divert(_)
                        | super::content_list::ContentItem::End
                        | super::content_list::ContentItem::Done
                ) | None
            ) {
                inner_content
                    .items
                    .insert(insert_index, super::content_list::ContentItem::Newline);
            }
        }

        if has_selected_suffix
            && ends_with_divert
            && !inner_content
                .items
                .iter()
                .any(|item| matches!(item, super::content_list::ContentItem::Newline))
        {
            let divert_index = inner_content
                .items
                .iter()
                .position(|item| matches!(item, super::content_list::ContentItem::Divert(_)))
                .unwrap_or(inner_content.items.len());
            inner_content
                .items
                .insert(divert_index, super::content_list::ContentItem::Newline);
        }

        Ok(Self {
            start_content,
            choice_only_content,
            inner_content,
            once_only: choice.once_only,
            is_invisible_default: choice.is_invisible_default,
        })
    }

    pub fn generate_runtime(
        &self,
        scope_path: &str,
        root_item_index: usize,
        choice_index: usize,
        continuation_name: &str,
    ) -> Result<ChoiceRuntime, CompilerError> {
        let branch_name = format!("c-{choice_index}");
        let outer_path = format!("{scope_path}.{root_item_index}");
        let branch_path = format!("{scope_path}.{branch_name}");
        let start_label_path = format!("{outer_path}.$r1");
        let inner_label_path = format!("{branch_path}.$r2");
        let start_container_path = format!("{outer_path}.s");
        let continuation_path = format!("{scope_path}.{continuation_name}");

        let mut outer_content: Vec<Rc<dyn RTObject>> = Vec::new();
        let mut outer_named: HashMap<String, Rc<Container>> = HashMap::new();

        outer_content.push(rt_command(
            bladeink::compiler_support::CommandType::EvalStart,
        ));

        if let Some(start_content) = &self.start_content {
            outer_content.push(rt_divert_target(&start_label_path));
            outer_content.push(rt_temp_assign("$r"));
            outer_content.push(rt_command(
                bladeink::compiler_support::CommandType::BeginString,
            ));
            outer_content.push(rt_divert(&start_container_path));

            let mut start_runtime = start_content.to_runtime()?;
            start_runtime.push(rt_variable_divert("$r"));
            let start_container =
                Container::new(Some("s".to_owned()), 0, start_runtime, HashMap::new());
            outer_named.insert("s".to_owned(), start_container);

            outer_content.push(Container::new(
                Some("$r1".to_owned()),
                0,
                Vec::new(),
                HashMap::new(),
            ));
            outer_content.push(rt_command(
                bladeink::compiler_support::CommandType::EndString,
            ));
        }

        if let Some(choice_only_content) = &self.choice_only_content {
            outer_content.push(rt_command(
                bladeink::compiler_support::CommandType::BeginString,
            ));
            outer_content.extend(choice_only_content.to_runtime()?);
            outer_content.push(rt_command(
                bladeink::compiler_support::CommandType::EndString,
            ));
        }

        outer_content.push(rt_command(bladeink::compiler_support::CommandType::EvalEnd));
        outer_content
            .push(Rc::new(ChoicePoint::new(choice_flags(self), &branch_path)) as Rc<dyn RTObject>);

        let outer = Container::new(None, 0, outer_content, outer_named);

        let mut inner_content: Vec<Rc<dyn RTObject>> = Vec::new();
        if self.start_content.is_some() {
            inner_content.push(rt_command(
                bladeink::compiler_support::CommandType::EvalStart,
            ));
            inner_content.push(rt_divert_target(&inner_label_path));
            inner_content.push(rt_command(bladeink::compiler_support::CommandType::EvalEnd));
            inner_content.push(rt_temp_assign("$r"));
            inner_content.push(rt_divert(&start_container_path));
            inner_content.push(Container::new(
                Some("$r2".to_owned()),
                0,
                Vec::new(),
                HashMap::new(),
            ));
        }

        inner_content.extend(
            self.inner_content
                .to_runtime_with_absolute_diverts(&|target| match target {
                    "END" => target.to_owned(),
                    "DONE" => target.to_owned(),
                    _ => target.to_owned(),
                })?,
        );
        if !content_list_is_terminal(&self.inner_content) {
            inner_content.push(rt_divert(&continuation_path));
        }

        let inner = Container::new(Some(branch_name), 5, inner_content, HashMap::new());

        Ok(ChoiceRuntime { outer, inner })
    }
}

fn selected_suffix(choice: &crate::parsed_hierarchy::Choice) -> Option<String> {
    let selected_text = choice.selected_text.as_ref()?;
    if choice.start_text.is_empty() {
        return Some(selected_text.clone());
    }

    selected_text
        .strip_prefix(&choice.start_text)
        .map(ToOwned::to_owned)
        .or_else(|| {
            selected_text
                .strip_prefix(choice.start_text.trim_end())
                .map(|suffix| suffix.trim_start().to_owned())
        })
}

fn content_list_is_terminal(content: &ContentList) -> bool {
    matches!(
        content
            .items
            .iter()
            .rev()
            .find(|item| !matches!(item, super::content_list::ContentItem::Newline)),
        Some(
            super::content_list::ContentItem::Divert(_)
                | super::content_list::ContentItem::End
                | super::content_list::ContentItem::Done
        )
    )
}

fn choice_flags(choice: &Choice) -> i32 {
    let mut flags = 0;
    if choice.start_content.is_some() {
        flags |= 2;
    }
    if choice.choice_only_content.is_some() {
        flags |= 4;
    }
    if choice.is_invisible_default {
        flags |= 8;
    }
    if choice.once_only {
        flags |= 16;
    }
    flags
}

fn rt_command(command: bladeink::compiler_support::CommandType) -> Rc<dyn RTObject> {
    Rc::new(bladeink::compiler_support::ControlCommand::new(command)) as Rc<dyn RTObject>
}

fn rt_temp_assign(name: &str) -> Rc<dyn RTObject> {
    Rc::new(VariableAssignment::new(name, true, false)) as Rc<dyn RTObject>
}

fn rt_divert(path: &str) -> Rc<dyn RTObject> {
    Rc::new(bladeink::compiler_support::Divert::new(
        false,
        PushPopType::Tunnel,
        false,
        0,
        false,
        None,
        Some(path),
    )) as Rc<dyn RTObject>
}

fn rt_variable_divert(name: &str) -> Rc<dyn RTObject> {
    Rc::new(bladeink::compiler_support::Divert::new(
        false,
        PushPopType::Tunnel,
        false,
        0,
        false,
        Some(name.to_owned()),
        None,
    )) as Rc<dyn RTObject>
}

fn rt_divert_target(path: &str) -> Rc<dyn RTObject> {
    Rc::new(Value::new(Path::new_with_components_string(Some(path)))) as Rc<dyn RTObject>
}
