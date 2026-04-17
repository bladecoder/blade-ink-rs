use std::{collections::HashMap, rc::Rc};

use bladeink::compiler_support::{story_json_from_container_and_named, Container};

use crate::error::CompilerError;

pub fn serialize_root_container(
    root: &Container,
    top_level_named: &HashMap<String, Rc<Container>>,
) -> Result<String, CompilerError> {
    story_json_from_container_and_named(root, top_level_named, &Default::default()).map_err(
        |error| {
            CompilerError::InvalidSource(format!(
                "failed to serialize runtime-backed compiled story: {error}"
            ))
        },
    )
}
