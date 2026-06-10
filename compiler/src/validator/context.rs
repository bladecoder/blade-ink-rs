pub fn validate(story: &ParsedStory) -> Result<(), CompilerError> {
    let ctx = ValidationContext::build(story);
    ctx.validate_story(story)
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

struct ValidationContext {
    /// All valid divert targets: knot names, "knot.stitch", "knot.stitch.label",
    /// gather labels at root scope, etc.
    valid_targets: BTreeSet<String>,
    /// Top-level flow names (knots + functions).
    flow_names: BTreeSet<String>,
    /// Function flow names.
    function_names: BTreeSet<String>,
    /// Declared EXTERNAL function names.
    external_functions: BTreeSet<String>,
    /// Global VAR names.
    global_var_names: BTreeSet<String>,
    /// CONST names.
    #[allow(dead_code)]
    const_names: BTreeSet<String>,
}

impl ValidationContext {
    fn build(story: &ParsedStory) -> Self {
        let mut valid_targets: BTreeSet<String> = BTreeSet::new();
        let mut flow_names: BTreeSet<String> = BTreeSet::new();
        let mut function_names: BTreeSet<String> = BTreeSet::new();
        let mut global_var_names: BTreeSet<String> = BTreeSet::new();
        let mut const_names: BTreeSet<String> = BTreeSet::new();

        // Reserved targets always valid
        for t in &["END", "DONE", "->->"] {
            valid_targets.insert(t.to_string());
        }

        // Global VARs can hold divert values and be used as divert targets
        for g in story.globals() {
            valid_targets.insert(g.name.clone());
            global_var_names.insert(g.name.clone());
        }

        // CONSTs
        for name in story.consts.keys() {
            const_names.insert(name.clone());
        }

        // Collect gather/choice labels from root nodes
        collect_labels_from_nodes(story.root(), "", &mut valid_targets);

        // Collect knot/stitch names, their labels, and parameters (params can be divert targets)
        for flow in story.flows() {
            flow_names.insert(flow.name.clone());
            if flow.is_function {
                function_names.insert(flow.name.clone());
            }
            valid_targets.insert(flow.name.clone());
            // Parameters can be used as divert targets inside the flow
            for param in &flow.parameters {
                valid_targets.insert(param.clone());
            }
            // Gather labels within the knot's own nodes
            collect_labels_from_nodes(&flow.nodes, &flow.name, &mut valid_targets);
            for stitch in &flow.children {
                let qualified = format!("{}.{}", flow.name, stitch.name);
                valid_targets.insert(qualified.clone());
                for param in &stitch.parameters {
                    valid_targets.insert(param.clone());
                }
                collect_labels_from_nodes(&stitch.nodes, &qualified, &mut valid_targets);
            }
        }

        Self {
            valid_targets,
            flow_names,
            function_names,
            external_functions: story.external_functions.iter().cloned().collect(),
            global_var_names,
            const_names,
        }
    }

    fn validate_story(&self, story: &ParsedStory) -> Result<(), CompilerError> {
        // Validate root nodes
        let empty_params = BTreeSet::new();
        self.validate_temp_names(story.root(), &empty_params)?;
        self.validate_nodes_diverts(story.root(), "")?;
        self.validate_nodes_function_calls(story.root())?;
        self.validate_no_choice_in_conditional(story.root())?;

        // Validate each flow
        for flow in story.flows() {
            // Validate that function purity rules are respected
            self.validate_function_purity(flow)?;

            // Validate argument name collisions
            self.validate_flow_parameter_names(flow)?;

            // Collect all temps defined in this flow's own nodes (includes nested nodes)
            let flow_params: BTreeSet<String> = flow.parameters.iter().cloned().collect();
            let flow_divert_params: BTreeSet<String> =
                flow.divert_parameters.iter().cloned().collect();
            let flow_temps = collect_temps_from_nodes(&flow.nodes);
            let flow_scope = ScopeInfo {
                forbidden: BTreeSet::new(),
            };

            // Validate temp naming collisions with function names in this flow
            self.validate_temp_names(&flow.nodes, &flow_params)?;

            self.validate_nodes_diverts(&flow.nodes, &flow.name)?;
            self.validate_nodes_function_calls(&flow.nodes)?;
            self.validate_no_choice_in_conditional(&flow.nodes)?;
            self.validate_nodes_variable_divert_targets(
                &flow.nodes,
                &flow_params,
                &flow_divert_params,
            )?;
            self.validate_nodes_vars(&flow.nodes, &flow_scope)?;

            // Validate stitch names don't collide with VAR names
            for stitch in &flow.children {
                self.validate_stitch_name(stitch)?;

                let stitch_params: BTreeSet<String> = stitch.parameters.iter().cloned().collect();
                let stitch_divert_params: BTreeSet<String> =
                    stitch.divert_parameters.iter().cloned().collect();
                let stitch_temps = collect_temps_from_nodes(&stitch.nodes);
                // Forbidden: parent knot's temps and params that are NOT also defined in the stitch
                let mut forbidden = BTreeSet::new();
                for name in flow_temps.iter().chain(flow_params.iter()) {
                    if !stitch_params.contains(name) && !stitch_temps.contains(name) {
                        forbidden.insert(name.clone());
                    }
                }
                // A stitch can only see its own params/temps, NOT the parent knot's
                let stitch_scope = ScopeInfo { forbidden };

                // Validate temp naming collisions in stitch
                self.validate_temp_names(&stitch.nodes, &stitch_params)?;

                let qualified = format!("{}.{}", flow.name, stitch.name);
                self.validate_nodes_diverts(&stitch.nodes, &qualified)?;
                self.validate_nodes_function_calls(&stitch.nodes)?;
                self.validate_nodes_variable_divert_targets(
                    &stitch.nodes,
                    &stitch_params,
                    &stitch_divert_params,
                )?;
                self.validate_nodes_vars(&stitch.nodes, &stitch_scope)?;

                // Validate duplicate gather labels within stitch
                self.validate_no_duplicate_gather_labels(&stitch.nodes)?;
            }

            // Validate duplicate gather labels within knot
            self.validate_no_duplicate_gather_labels(&flow.nodes)?;
        }

        // Validate duplicate gather labels in root
        self.validate_no_duplicate_gather_labels(story.root())?;

        Ok(())
    }
}
