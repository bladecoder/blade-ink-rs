use super::{ObjectKind, ParsedObject};

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

#[derive(Debug, Clone)]
pub struct Knot {
    flow: FlowBase,
}

impl Knot {
    pub fn new(identifier: impl Into<String>, arguments: Vec<FlowArgument>, is_function: bool) -> Self {
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
