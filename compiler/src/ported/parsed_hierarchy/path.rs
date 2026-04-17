use super::identifier::Identifier;

#[derive(Default)]
pub struct Path {
    pub components: Vec<Identifier>,
}
