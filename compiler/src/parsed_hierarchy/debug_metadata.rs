#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DebugMetadata {
    pub start_line_number: usize,
    pub end_line_number: usize,
    pub start_character_number: usize,
    pub end_character_number: usize,
    pub file_name: Option<String>,
}

impl From<DebugMetadata> for bladeink::DebugMetadata {
    fn from(value: DebugMetadata) -> Self {
        Self {
            start_line_number: value.start_line_number,
            end_line_number: value.end_line_number,
            start_character_number: value.start_character_number,
            end_character_number: value.end_character_number,
            file_name: value.file_name,
        }
    }
}

impl From<&DebugMetadata> for bladeink::DebugMetadata {
    fn from(value: &DebugMetadata) -> Self {
        Self {
            start_line_number: value.start_line_number,
            end_line_number: value.end_line_number,
            start_character_number: value.start_character_number,
            end_character_number: value.end_character_number,
            file_name: value.file_name.clone(),
        }
    }
}
