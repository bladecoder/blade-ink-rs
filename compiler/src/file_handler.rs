use std::{fs, path::PathBuf};

use crate::error::CompilerError;

pub trait FileHandler {
    fn resolve_ink_filename(&self, include_name: &str) -> String;

    fn load_ink_file_contents(&self, full_filename: &str) -> Result<String, CompilerError>;
}

#[derive(Debug, Default, Clone)]
pub struct DefaultFileHandler {
    working_dir: Option<PathBuf>,
}

impl DefaultFileHandler {
    pub fn new(working_dir: Option<PathBuf>) -> Self {
        Self { working_dir }
    }
}

impl FileHandler for DefaultFileHandler {
    fn resolve_ink_filename(&self, include_name: &str) -> String {
        let base = self
            .working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        base.join(include_name).to_string_lossy().into_owned()
    }

    fn load_ink_file_contents(&self, full_filename: &str) -> Result<String, CompilerError> {
        fs::read_to_string(full_filename).map_err(|error| {
            CompilerError::invalid_source(format!(
                "Failed to read included file '{}': {}",
                full_filename, error
            ))
        })
    }
}
