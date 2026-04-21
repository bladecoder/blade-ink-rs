use crate::parsed_hierarchy::{Content, ContentList, FlowBase, FlowLevel, Story as ParsedStory};

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
