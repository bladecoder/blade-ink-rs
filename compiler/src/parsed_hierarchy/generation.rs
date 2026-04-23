use std::{collections::HashMap, rc::Rc};

use bladeink::RTObject;

use crate::{error::CompilerError, runtime_export::{ExportState, Scope}};

use bladeink::Container;

use super::{DivertTarget, FunctionCall, ParsedFlow, ParsedNode, Story, VariableReference};

#[allow(dead_code)]
pub(crate) trait GenerateRuntimeObject {
    fn generate_runtime_object(
        &self,
        state: &ExportState,
        scope: Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        path_hint: Option<&str>,
    ) -> Result<Rc<dyn RTObject>, CompilerError>;
}

pub(crate) trait GenerateIntoContainer {
    fn generate_into_container(
        &self,
        state: &ExportState,
        scope: Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        container_path: Option<&str>,
        content_index_offset: usize,
        content: &mut Vec<Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError>;
}

impl GenerateIntoContainer for ParsedNode {
    fn generate_into_container(
        &self,
        state: &ExportState,
        scope: Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        container_path: Option<&str>,
        content_index_offset: usize,
        content: &mut Vec<Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError> {
        let node_path = container_path.map(|path| format!("{path}.{}", content_index_offset + content.len()));
        self.export_runtime(
            state,
            scope,
            story,
            named_paths,
            container_path,
            node_path.as_deref(),
            content_index_offset,
            content,
        )
    }
}

impl GenerateRuntimeObject for ParsedNode {
    fn generate_runtime_object(
        &self,
        state: &ExportState,
        scope: Scope<'_>,
        story: &Story,
        named_paths: Option<&HashMap<String, String>>,
        path_hint: Option<&str>,
    ) -> Result<Rc<dyn RTObject>, CompilerError> {
        let mut content = Vec::new();
        self.generate_into_container(state, scope, story, named_paths, path_hint, 0, &mut content)?;
        match content.len() {
            0 => Err(CompilerError::unsupported_feature("node generated no runtime object")),
            1 => Ok(content.remove(0)),
            _ => Ok(Container::new(None, 0, content, HashMap::new())),
        }
    }
}

impl GenerateRuntimeObject for ParsedFlow {
    fn generate_runtime_object(
        &self,
        state: &ExportState,
        _scope: Scope<'_>,
        story: &Story,
        _named_paths: Option<&HashMap<String, String>>,
        path_hint: Option<&str>,
    ) -> Result<Rc<dyn RTObject>, CompilerError> {
        let full_path = path_hint.unwrap_or(self.flow().identifier().unwrap_or_default());
        Ok(self.export_runtime(state, story, full_path)?)
    }
}

impl GenerateRuntimeObject for Story {
    fn generate_runtime_object(
        &self,
        _state: &ExportState,
        _scope: Scope<'_>,
        _story: &Story,
        _named_paths: Option<&HashMap<String, String>>,
        _path_hint: Option<&str>,
    ) -> Result<Rc<dyn RTObject>, CompilerError> {
        self.runtime_object()
            .ok_or_else(|| CompilerError::unsupported_feature("story runtime object not generated yet"))
    }
}

impl GenerateRuntimeObject for VariableReference {
    fn generate_runtime_object(
        &self,
        _state: &ExportState,
        _scope: Scope<'_>,
        _story: &Story,
        _named_paths: Option<&HashMap<String, String>>,
        _path_hint: Option<&str>,
    ) -> Result<Rc<dyn RTObject>, CompilerError> {
        Ok(self.runtime_object())
    }
}

impl GenerateRuntimeObject for DivertTarget {
    fn generate_runtime_object(
        &self,
        _state: &ExportState,
        _scope: Scope<'_>,
        story: &Story,
        _named_paths: Option<&HashMap<String, String>>,
        _path_hint: Option<&str>,
    ) -> Result<Rc<dyn RTObject>, CompilerError> {
        let path = self
            .resolved_target()
            .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
            .and_then(|cache| cache.runtime_path())
            .map(|path| path.to_string())
            .or_else(|| self.target_path().runtime_path(story))
            .unwrap_or_else(|| self.target_path().as_str().to_owned());
        Ok(Rc::new(bladeink::Value::new(bladeink::Path::new_with_components_string(Some(&path)))))
    }
}

impl GenerateIntoContainer for FunctionCall {
    fn generate_into_container(
        &self,
        _state: &ExportState,
        _scope: Scope<'_>,
        story: &Story,
        _named_paths: Option<&HashMap<String, String>>,
        _container_path: Option<&str>,
        _content_index_offset: usize,
        content: &mut Vec<Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError> {
        self.export_runtime(story, content)
    }
}
