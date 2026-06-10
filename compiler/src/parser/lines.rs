pub fn split_lines(source: &str) -> Vec<Line<'_>> {
    let mut lines: Vec<Line<'_>> = source
        .split_inclusive('\n')
        .map(|line| {
            let content = line.strip_suffix('\n').unwrap_or(line);
            // Strip trailing inline comments (// ...) but not inside strings
            let content = strip_inline_comment(content);
            Line {
                content,
                indent: count_indent_columns(content),
                had_newline: line.ends_with('\n'),
            }
        })
        .collect();

    if !source.ends_with('\n')
        && let Some(last_line) = lines.last_mut()
        && !last_line.content.is_empty()
    {
        last_line.had_newline = true;
    }

    lines
}

fn count_indent_columns(content: &str) -> usize {
    let mut columns = 0usize;

    for ch in content.chars() {
        match ch {
            ' ' => columns += 1,
            '\t' => {
                const TAB_STOP: usize = 4;
                let offset = columns % TAB_STOP;
                columns += if offset == 0 {
                    TAB_STOP
                } else {
                    TAB_STOP - offset
                };
            }
            _ => break,
        }
    }

    columns
}

/// Strip all `/* ... */` block comments from a string.
fn strip_block_comments(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume '*'
            // skip until we find '*/'
            while let Some(c2) = chars.next() {
                if c2 == '*' && chars.peek() == Some(&'/') {
                    chars.next(); // consume '/'
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Strip trailing `// comment` from a line, respecting string literals.
fn strip_inline_comment(line: &str) -> &str {
    let mut in_string = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_string = !in_string,
            b'/' if !in_string && i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                return line[..i].trim_end();
            }
            _ => {}
        }
        i += 1;
    }
    line
}

