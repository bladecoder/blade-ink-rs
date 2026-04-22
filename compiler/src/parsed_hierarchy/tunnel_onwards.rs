use super::{ObjectKind, ParsedObject};

#[derive(Debug, Clone)]
pub struct TunnelOnwards {
    object: ParsedObject,
    divert_after: Option<String>,
}

impl TunnelOnwards {
    pub fn new(divert_after: Option<String>) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::TunnelOnwards),
            divert_after,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn divert_after(&self) -> Option<&str> {
        self.divert_after.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::TunnelOnwards;

    #[test]
    fn tunnel_onwards_can_hold_override_target() {
        let tunnel = TunnelOnwards::new(Some("next".to_owned()));
        assert_eq!(Some("next"), tunnel.divert_after());
    }
}
