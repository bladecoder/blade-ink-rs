use bladeink::story::Story;

pub fn next_all(story: &mut Story, text: &mut Vec<String>) -> Result<(), String> {
    while story.can_continue() {
        let line = story.cont()?;
        print!("{line}");

        if !line.trim().is_empty() {
            text.push(line.trim().to_string());
        }
    }

    // if story.has_error() {
    //     Err(TestUtils.joinText(story.get_current_errors()));
    // }

    Ok(())
}