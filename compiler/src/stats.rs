use crate::{
    bootstrap::ast::{Node, ParsedStory as BootstrapParsedStory},
    parsed_hierarchy::{Content, ContentList, FlowBase, FlowLevel, Story as ParsedStory},
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    pub words: usize,
    pub knots: usize,
    pub stitches: usize,
    pub functions: usize,
    pub choices: usize,
    pub gathers: usize,
    pub diverts: usize,
}

impl Stats {
    pub fn generate(story: &BootstrapParsedStory) -> Self {
        let mut stats = Stats::default();

        count_words_in_bootstrap_nodes(story.root(), &mut stats.words);
        count_bootstrap_nodes(story.root(), &mut stats);

        for flow in story.flows() {
            if flow.is_function {
                stats.functions += 1;
            } else {
                stats.knots += 1;
            }

            count_words_in_bootstrap_nodes(&flow.nodes, &mut stats.words);
            count_bootstrap_nodes(&flow.nodes, &mut stats);

            for child in &flow.children {
                if child.is_function {
                    stats.functions += 1;
                } else {
                    stats.stitches += 1;
                }
                count_words_in_bootstrap_nodes(&child.nodes, &mut stats.words);
                count_bootstrap_nodes(&child.nodes, &mut stats);
            }
        }

        stats
    }

    pub fn generate_from_parsed(story: &ParsedStory) -> Self {
        let mut stats = Stats::default();
        count_flow(story.flow(), &mut stats);
        count_words_in_content_list(story.root_content(), &mut stats.words);
        stats
    }
}

fn count_flow(flow: &FlowBase, stats: &mut Stats) {
    match flow.flow_level() {
        FlowLevel::Story => {}
        FlowLevel::Knot if flow.is_function() => stats.functions += 1,
        FlowLevel::Knot => stats.knots += 1,
        FlowLevel::Stitch if flow.is_function() => stats.functions += 1,
        FlowLevel::Stitch => stats.stitches += 1,
        FlowLevel::WeavePoint => {}
    }
}

fn count_words_in_str(text: &str, count: &mut usize) {
    let mut was_whitespace = true;
    for c in text.chars() {
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
            was_whitespace = true;
        } else if was_whitespace {
            *count += 1;
            was_whitespace = false;
        }
    }
}

fn count_words_in_content_list(content: &ContentList, count: &mut usize) {
    for item in content.content() {
        match item {
            Content::Text(text) => count_words_in_str(text.text(), count),
        }
    }
}

fn count_words_in_bootstrap_nodes(nodes: &[Node], count: &mut usize) {
    for node in nodes {
        match node {
            Node::Text(text) => count_words_in_str(text, count),
            Node::Choice(choice) => {
                count_words_in_str(&choice.display_text, count);
                count_words_in_bootstrap_nodes(&choice.body, count);
            }
            Node::Conditional {
                when_true,
                when_false,
                ..
            } => {
                count_words_in_bootstrap_nodes(when_true, count);
                if let Some(when_false) = when_false {
                    count_words_in_bootstrap_nodes(when_false, count);
                }
            }
            Node::SwitchConditional { branches, .. } => {
                for (_, body) in branches {
                    count_words_in_bootstrap_nodes(body, count);
                }
            }
            Node::Sequence(sequence) => {
                for branch in &sequence.branches {
                    count_words_in_bootstrap_nodes(branch, count);
                }
            }
            _ => {}
        }
    }
}

fn count_bootstrap_nodes(nodes: &[Node], stats: &mut Stats) {
    for node in nodes {
        match node {
            Node::Choice(choice) => {
                stats.choices += 1;
                count_bootstrap_nodes(&choice.body, stats);
            }
            Node::GatherLabel(_) => stats.gathers += 1,
            Node::Divert(_) | Node::TunnelDivert { .. } => stats.diverts += 1,
            Node::Conditional {
                when_true,
                when_false,
                ..
            } => {
                count_bootstrap_nodes(when_true, stats);
                if let Some(when_false) = when_false {
                    count_bootstrap_nodes(when_false, stats);
                }
            }
            Node::SwitchConditional { branches, .. } => {
                for (_, body) in branches {
                    count_bootstrap_nodes(body, stats);
                }
            }
            Node::Sequence(sequence) => {
                for branch in &sequence.branches {
                    count_bootstrap_nodes(branch, stats);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Stats;
    use crate::parsed_hierarchy::Story;

    #[test]
    fn parsed_stats_count_words_from_root_content() {
        let mut story = Story::new("", None, true);
        story.root_content_mut().push_text("one two\nthree");
        let stats = Stats::generate_from_parsed(&story);
        assert_eq!(3, stats.words);
    }
}
