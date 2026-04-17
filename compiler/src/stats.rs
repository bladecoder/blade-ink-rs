//! Story statistics computed from the parsed AST.
//!
//! Mirrors the `Stats` class from the blade-ink-java reference implementation.

use crate::ast::{Node, ParsedStory};

/// Statistics about an Ink story, computed by walking the parsed AST.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    /// Total word count across all text nodes.
    pub words: usize,
    /// Number of knots (top-level named flows that are not functions).
    pub knots: usize,
    /// Number of stitches (child flows inside knots).
    pub stitches: usize,
    /// Number of functions (`=== function name ===`).
    pub functions: usize,
    /// Number of choice points.
    pub choices: usize,
    /// Number of named gather points.
    pub gathers: usize,
    /// Number of diverts (`->`).
    pub diverts: usize,
}

impl Stats {
    /// Walk the fully-parsed story AST and compute statistics.
    pub fn generate(story: &ParsedStory) -> Self {
        let mut stats = Stats::default();

        // Count words in root nodes
        count_words_in_nodes(story.root(), &mut stats.words);

        // Walk top-level flows (knots / functions)
        for flow in story.flows() {
            if flow.is_function {
                stats.functions += 1;
            } else {
                stats.knots += 1;
            }

            // Count stitches (direct children of a knot that are not functions)
            for child in &flow.children {
                if !child.is_function {
                    stats.stitches += 1;
                }
                count_words_in_nodes(&child.nodes, &mut stats.words);
                count_nodes_in_slice(&child.nodes, &mut stats);
            }

            count_words_in_nodes(&flow.nodes, &mut stats.words);
            count_nodes_in_slice(&flow.nodes, &mut stats);
        }

        // Count in root nodes
        count_nodes_in_slice(story.root(), &mut stats);

        stats
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

fn count_words_in_nodes(nodes: &[Node], count: &mut usize) {
    for node in nodes {
        match node {
            Node::Text(t) => count_words_in_str(t, count),
            Node::Choice(c) => {
                count_words_in_str(&c.display_text, count);
                count_words_in_nodes(&c.body, count);
            }
            Node::Conditional {
                when_true,
                when_false,
                ..
            } => {
                count_words_in_nodes(when_true, count);
                if let Some(wf) = when_false {
                    count_words_in_nodes(wf, count);
                }
            }
            Node::SwitchConditional { branches, .. } => {
                for (_, body) in branches {
                    count_words_in_nodes(body, count);
                }
            }
            Node::Sequence(seq) => {
                for branch in &seq.branches {
                    count_words_in_nodes(branch, count);
                }
            }
            _ => {}
        }
    }
}

fn count_nodes_in_slice(nodes: &[Node], stats: &mut Stats) {
    for node in nodes {
        match node {
            Node::Choice(c) => {
                stats.choices += 1;
                count_nodes_in_slice(&c.body, stats);
            }
            Node::GatherLabel(_) => {
                stats.gathers += 1;
            }
            Node::Divert(_) | Node::TunnelDivert { .. } => {
                stats.diverts += 1;
            }
            Node::Conditional {
                when_true,
                when_false,
                ..
            } => {
                count_nodes_in_slice(when_true, stats);
                if let Some(wf) = when_false {
                    count_nodes_in_slice(wf, stats);
                }
            }
            Node::SwitchConditional { branches, .. } => {
                for (_, body) in branches {
                    count_nodes_in_slice(body, stats);
                }
            }
            Node::Sequence(seq) => {
                for branch in &seq.branches {
                    count_nodes_in_slice(branch, stats);
                }
            }
            _ => {}
        }
    }
}
