#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DebugMetadata {
    pub start_line_number: usize,
    pub end_line_number: usize,
    pub start_character_number: usize,
    pub end_character_number: usize,
    pub file_name: Option<String>,
}
