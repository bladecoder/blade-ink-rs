use std::{collections::HashMap, collections::HashSet, rc::Rc};

use bladeink::{CommandType, Container, RTObject};

use crate::error::CompilerError;
use crate::runtime_export::{
    ExportState, Scope, command, divert_object, export_nodes_with_paths, export_weave,
};

use super::{ObjectKind, ParsedNode, ParsedObject, Story, ValidationScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowLevel {
    Story,
    Knot,
    Stitch,
    WeavePoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowArgument {
    pub identifier: String,
    pub is_by_reference: bool,
    pub is_divert_target: bool,
}

#[derive(Debug, Clone)]
pub struct FlowBase {
    object: ParsedObject,
    flow_level: FlowLevel,
    identifier: Option<String>,
    arguments: Vec<FlowArgument>,
    is_function: bool,
}

impl FlowBase {
    pub fn new(
        flow_level: FlowLevel,
        identifier: Option<String>,
        arguments: Vec<FlowArgument>,
        is_function: bool,
    ) -> Self {
        let kind = match flow_level {
            FlowLevel::Story => ObjectKind::Story,
            FlowLevel::Knot => ObjectKind::Knot,
            FlowLevel::Stitch => ObjectKind::Stitch,
            FlowLevel::WeavePoint => ObjectKind::FlowBase,
        };

        Self {
            object: ParsedObject::new(kind),
            flow_level,
            identifier,
            arguments,
            is_function,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn flow_level(&self) -> FlowLevel {
        self.flow_level
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub fn arguments(&self) -> &[FlowArgument] {
        &self.arguments
    }

    pub fn has_parameters(&self) -> bool {
        !self.arguments.is_empty()
    }

    pub fn is_function(&self) -> bool {
        self.is_function
    }
}

impl super::ParsedFlow {
    pub(crate) fn child_flow_names(&self) -> HashSet<String> {
        self.children()
            .iter()
            .filter_map(|child| child.flow().identifier().map(ToOwned::to_owned))
            .collect()
    }

    pub(crate) fn collect_temp_vars(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        for node in self.content() {
            node.collect_temp_vars(&mut names);
        }
        names
    }

    pub(crate) fn collect_named_labels(&self) -> Result<HashSet<String>, CompilerError> {
        let mut names = HashSet::new();
        for node in self.content() {
            node.collect_named_labels(&mut names)?;
        }
        Ok(names)
    }

    pub(crate) fn uses_turn_or_read_count(&self) -> bool {
        self.content().iter().any(ParsedNode::uses_turn_or_read_count)
            || self.children().iter().any(super::ParsedFlow::uses_turn_or_read_count)
    }

    pub(super) fn validate(
        &self,
        top_level_flow_names: &HashSet<String>,
        sibling_flow_names: &HashSet<String>,
        all_flow_names: &HashSet<String>,
        global_vars: &HashSet<String>,
        const_names: &HashSet<String>,
        story: &super::Story,
    ) -> Result<(), CompilerError> {
        let flow_name = self.flow().identifier().unwrap_or_default().to_owned();
        let child_flow_names = self.child_flow_names();

        for child in self.children() {
            if let Some(child_name) = child.flow().identifier()
                && global_vars.contains(child_name)
            {
                return Err(CompilerError::invalid_source(format!(
                    "Flow '{}' collides with existing var '{}'",
                    child_name, child_name
                )));
            }
        }

        let mut arg_names = HashSet::new();
        let mut typed_divert_args = HashSet::new();
        for argument in self.flow().arguments() {
            if !arg_names.insert(argument.identifier.clone()) {
                return Err(CompilerError::invalid_source(format!(
                    "Multiple arguments with the same name: '{}'",
                    argument.identifier
                )));
            }

            if global_vars.contains(&argument.identifier)
                || const_names.contains(&argument.identifier)
                || all_flow_names.contains(&argument.identifier)
            {
                return Err(CompilerError::invalid_source(format!(
                    "Argument '{}' is already used by a var or flow",
                    argument.identifier
                )));
            }

            if argument.is_divert_target {
                typed_divert_args.insert(argument.identifier.clone());
            }
        }

        let temp_vars = self.collect_temp_vars();
        for temp in &temp_vars {
            if arg_names.contains(temp) {
                return Err(CompilerError::invalid_source(format!(
                    "Variable '{}' already exists as a parameter",
                    temp
                )));
            }
            if all_flow_names.contains(temp) {
                return Err(CompilerError::invalid_source(format!(
                    "Variable '{}' already exists as a flow or function name",
                    temp
                )));
            }
        }

        let mut visible_vars = global_vars.clone();
        visible_vars.extend(const_names.iter().cloned());
        visible_vars.extend(arg_names.iter().cloned());
        visible_vars.extend(temp_vars.iter().cloned());

        let mut divert_target_vars = global_vars.clone();
        divert_target_vars.extend(typed_divert_args.iter().cloned());

        let mut sibling_names = top_level_flow_names.clone();
        sibling_names.extend(sibling_flow_names.iter().cloned());
        sibling_names.insert(flow_name);

        let scope = ValidationScope {
            visible_vars,
            divert_target_vars,
            top_level_flow_names: top_level_flow_names.clone(),
            sibling_flow_names: sibling_names,
            local_labels: self.collect_named_labels()?,
            all_flow_names: all_flow_names.clone(),
        };

        ParsedNode::validate_list(self.content(), &scope, story)?;

        for child in self.children() {
            child.validate(
                top_level_flow_names,
                &child_flow_names,
                all_flow_names,
                global_vars,
                const_names,
                story,
            )?;
        }

        Ok(())
    }

    pub(crate) fn export_runtime(
        &self,
        state: &ExportState,
        story: &Story,
        full_path: &str,
    ) -> Result<Rc<Container>, CompilerError> {
        let mut flow_named_paths = HashMap::new();
        if let Some(name) = self.flow().identifier() {
            flow_named_paths.insert(name.to_owned(), full_path.to_owned());
        }
        for child in self.children() {
            if let Some(child_name) = child.flow().identifier() {
                flow_named_paths.insert(child_name.to_owned(), format!("{full_path}.{child_name}"));
            }
        }

        let mut content = if self.content().is_empty() && !self.children().is_empty() {
            vec![divert_object(&format!(
                "{}.{}",
                full_path,
                self.children()[0].flow().identifier().unwrap_or_default()
            ))]
        } else if crate::runtime_export::has_weave_content(self.content()) {
            let weave_root_index = self.flow().arguments().len();
            vec![export_weave(
                state,
                &format!("{full_path}.{weave_root_index}"),
                self.content(),
                Scope::Flow(self),
                story,
                false,
                &flow_named_paths,
            )? as Rc<dyn RTObject>]
        } else {
            let flow_content_index_offset = self.flow().arguments().len();
            export_nodes_with_paths(
                state,
                self.content(),
                Scope::Flow(self),
                story,
                Some(&flow_named_paths),
                Some(full_path),
                flow_content_index_offset,
            )?
        };

        if self.flow().has_parameters() {
            let mut assignments = Vec::new();
            for argument in self.flow().arguments().iter().rev() {
                assignments.push(crate::runtime_export::variable_assignment(
                    &argument.identifier,
                    false,
                    true,
                ));
            }
            assignments.extend(content);
            content = assignments;
        }

        if !self.content().is_empty() && !crate::runtime_export::has_terminal(self.content()) {
            if !self.flow().is_function() {
                content.push(command(CommandType::Done));
            }
        }

        let mut named = HashMap::new();
        for child in self.children() {
            let child_name = child.flow().identifier().unwrap_or_default().to_owned();
            named.insert(
                child_name.clone(),
                child.export_runtime(state, story, &format!("{full_path}.{child_name}"))?,
            );
        }

        Ok(Container::new(
            Some(self.flow().identifier().unwrap_or_default().to_owned()),
            story.flow_count_flags(),
            content,
            named,
        ))
    }
}

#[derive(Debug, Clone)]
pub struct Knot {
    flow: FlowBase,
}

impl Knot {
    pub fn new(
        identifier: impl Into<String>,
        arguments: Vec<FlowArgument>,
        is_function: bool,
    ) -> Self {
        Self {
            flow: FlowBase::new(
                FlowLevel::Knot,
                Some(identifier.into()),
                arguments,
                is_function,
            ),
        }
    }

    pub fn flow(&self) -> &FlowBase {
        &self.flow
    }

    pub fn flow_mut(&mut self) -> &mut FlowBase {
        &mut self.flow
    }
}

#[derive(Debug, Clone)]
pub struct Stitch {
    flow: FlowBase,
}

impl Stitch {
    pub fn new(identifier: impl Into<String>, arguments: Vec<FlowArgument>) -> Self {
        Self {
            flow: FlowBase::new(FlowLevel::Stitch, Some(identifier.into()), arguments, false),
        }
    }

    pub fn flow(&self) -> &FlowBase {
        &self.flow
    }

    pub fn flow_mut(&mut self) -> &mut FlowBase {
        &mut self.flow
    }
}
