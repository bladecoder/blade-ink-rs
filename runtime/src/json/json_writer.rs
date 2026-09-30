#[allow(unused_imports)]
use crate::prelude::*;

use crate::compat::io::{self, Write};

pub(crate) struct JsonWriter<W: Write> {
    output: W,
}

impl<W: Write> JsonWriter<W> {
    pub(crate) fn new(output: W) -> Self {
        Self { output }
    }

    pub(crate) fn raw(&mut self, value: &str) -> io::Result<()> {
        self.output.write_all(value.as_bytes())
    }

    pub(crate) fn string(&mut self, value: &str) -> io::Result<()> {
        self.output.write_all(b"\"")?;
        let mut start = 0;
        for (index, ch) in value.char_indices() {
            let escaped = match ch {
                '"' => Some("\\\""),
                '\\' => Some("\\\\"),
                '\u{08}' => Some("\\b"),
                '\u{0c}' => Some("\\f"),
                '\n' => Some("\\n"),
                '\r' => Some("\\r"),
                '\t' => Some("\\t"),
                _ => None,
            };
            if let Some(escaped) = escaped {
                self.output.write_all(&value.as_bytes()[start..index])?;
                self.output.write_all(escaped.as_bytes())?;
                start = index + ch.len_utf8();
            } else if ch <= '\u{1f}' {
                self.output.write_all(&value.as_bytes()[start..index])?;
                write!(self.output, "\\u{:04x}", ch as u32)?;
                start = index + ch.len_utf8();
            }
        }
        self.output.write_all(&value.as_bytes()[start..])?;
        self.output.write_all(b"\"")
    }

    pub(crate) fn key(&mut self, first: &mut bool, key: &str) -> io::Result<()> {
        self.separator(first)?;
        self.string(key)?;
        self.output.write_all(b":")
    }

    pub(crate) fn separator(&mut self, first: &mut bool) -> io::Result<()> {
        if *first {
            *first = false;
            Ok(())
        } else {
            self.output.write_all(b",")
        }
    }

    pub(crate) fn integer(&mut self, value: impl crate::compat::fmt::Display) -> io::Result<()> {
        write!(self.output, "{value}")
    }

    pub(crate) fn float(&mut self, value: f32) -> io::Result<()> {
        if !value.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cannot serialize a non-finite float",
            ));
        }
        write!(self.output, "{value}")
    }

    pub(crate) fn boolean(&mut self, value: bool) -> io::Result<()> {
        self.raw(if value { "true" } else { "false" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_json_strings() {
        let mut output = Vec::new();
        JsonWriter::new(&mut output)
            .string("\"\\\n\t\u{7}ñ")
            .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "\"\\\"\\\\\\n\\t\\u0007ñ\""
        );
    }
}
