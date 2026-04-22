use std::collections::HashSet;

#[derive(Clone)]
pub(crate) struct ValidationScope {
    pub(crate) visible_vars: HashSet<String>,
    pub(crate) divert_target_vars: HashSet<String>,
    pub(crate) top_level_flow_names: HashSet<String>,
    pub(crate) sibling_flow_names: HashSet<String>,
    pub(crate) local_labels: HashSet<String>,
    pub(crate) all_flow_names: HashSet<String>,
}
