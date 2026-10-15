//! Output text normalization shared by continuation and saved-state loading.

#[allow(unused_imports)]
use crate::prelude::*;

pub(crate) fn clean_output_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut current_whitespace_start = -1;
    let mut start_of_line = 0;

    for (index, character) in input.chars().enumerate() {
        let is_inline_whitespace = character == ' ' || character == '\t';

        if is_inline_whitespace && current_whitespace_start == -1 {
            current_whitespace_start = index as i32;
        }

        if !is_inline_whitespace {
            if character != '\n'
                && current_whitespace_start > 0
                && current_whitespace_start != start_of_line
            {
                output.push(' ');
            }
            current_whitespace_start = -1;
        }

        if character == '\n' {
            start_of_line = index as i32 + 1;
        }

        if !is_inline_whitespace {
            output.push(character);
        }
    }

    output
}
