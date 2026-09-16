//! Pull-based JSON lexer used by the streamed codecs.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::compat::io::{self, Read};

#[cfg(feature = "std")]
use crate::compat::io::BufReader;

#[cfg(not(feature = "std"))]
struct BufReader<R>(R);

#[cfg(not(feature = "std"))]
impl<R> BufReader<R> {
    fn new(reader: R) -> Self {
        Self(reader)
    }
}

#[cfg(not(feature = "std"))]
impl<R: Read> Read for BufReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Number {
    Int(i32),
    Float(f32),
}

impl Number {
    pub(super) fn as_integer(self) -> Option<i32> {
        match self {
            Self::Int(value) => Some(value),
            Self::Float(_) => None,
        }
    }

    pub(super) fn as_float(self) -> f32 {
        match self {
            Self::Int(value) => value as f32,
            Self::Float(value) => value,
        }
    }

    pub(super) fn is_integer(self) -> bool {
        matches!(self, Self::Int(_))
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum JsonValue {
    Array,
    Object,
    String(String),
    Number(Number),
    Boolean(bool),
    Null,
}

impl JsonValue {
    pub(super) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub(super) fn as_integer(&self) -> Option<i32> {
        match self {
            Self::Number(value) => value.as_integer(),
            _ => None,
        }
    }
}

pub(super) struct JsonTokenizer<R: Read> {
    reader: BufReader<R>,
    lookahead: Option<u8>,
    offset: usize,
    line: usize,
    column: usize,
}

impl<R: Read> JsonTokenizer<R> {
    pub(super) fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
            lookahead: None,
            offset: 0,
            line: 1,
            column: 1,
        }
    }

    fn invalid(&self, message: impl AsRef<str>) -> io::Error {
        #[cfg(feature = "std")]
        let error = io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} at line {}, column {} (byte {})",
                message.as_ref(),
                self.line,
                self.column,
                self.offset
            ),
        );
        #[cfg(not(feature = "std"))]
        let _ = message;
        #[cfg(not(feature = "std"))]
        let error = io::Error::new(io::ErrorKind::InvalidData, "invalid JSON input");
        error
    }

    fn read_raw(&mut self) -> io::Result<Option<u8>> {
        let byte = if let Some(byte) = self.lookahead.take() {
            Some(byte)
        } else {
            let mut byte = [0_u8; 1];
            match self.reader.read(&mut byte)? {
                0 => None,
                _ => Some(byte[0]),
            }
        };
        if let Some(byte) = byte {
            self.offset += 1;
            if byte == b'\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        Ok(byte)
    }

    fn peek_raw(&mut self) -> io::Result<Option<u8>> {
        if self.lookahead.is_none() {
            let mut byte = [0_u8; 1];
            if self.reader.read(&mut byte)? != 0 {
                self.lookahead = Some(byte[0]);
            }
        }
        Ok(self.lookahead)
    }

    fn skip_whitespace(&mut self) -> io::Result<()> {
        while matches!(self.peek_raw()?, Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.read_raw()?;
        }
        Ok(())
    }

    pub(super) fn peek(&mut self) -> io::Result<char> {
        self.skip_whitespace()?;
        self.peek_raw()?
            .map(char::from)
            .ok_or_else(|| self.invalid("unexpected end of input"))
    }

    pub(super) fn expect(&mut self, expected: char) -> io::Result<()> {
        self.skip_whitespace()?;
        let Some(found) = self.read_raw()? else {
            return Err(self.invalid(format!("expected '{expected}', found end of input")));
        };
        if found == expected as u8 {
            Ok(())
        } else {
            Err(self.invalid(format!(
                "expected '{expected}', found '{}'",
                char::from(found)
            )))
        }
    }

    fn expect_bytes(&mut self, expected: &[u8]) -> io::Result<()> {
        for expected_byte in expected {
            let found = self
                .read_raw()?
                .ok_or_else(|| self.invalid("unexpected end of input"))?;
            if found != *expected_byte {
                return Err(self.invalid("invalid JSON literal"));
            }
        }
        if matches!(
            self.peek_raw()?,
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
        ) {
            return Err(self.invalid("invalid character after JSON literal"));
        }
        Ok(())
    }

    pub(super) fn read_boolean(&mut self) -> io::Result<bool> {
        self.skip_whitespace()?;
        match self.peek_raw()? {
            Some(b't') => {
                self.expect_bytes(b"true")?;
                Ok(true)
            }
            Some(b'f') => {
                self.expect_bytes(b"false")?;
                Ok(false)
            }
            _ => Err(self.invalid("expected boolean")),
        }
    }

    pub(super) fn read_null(&mut self) -> io::Result<()> {
        self.skip_whitespace()?;
        self.expect_bytes(b"null")
    }

    fn read_hex_quad(&mut self) -> io::Result<u16> {
        let mut value = 0_u16;
        for _ in 0..4 {
            let byte = self
                .read_raw()?
                .ok_or_else(|| self.invalid("unterminated Unicode escape"))?;
            let digit = char::from(byte)
                .to_digit(16)
                .ok_or_else(|| self.invalid("invalid Unicode escape"))?;
            value = (value << 4) | digit as u16;
        }
        Ok(value)
    }

    fn read_unicode_escape(&mut self) -> io::Result<char> {
        let first = self.read_hex_quad()?;
        let scalar = if (0xD800..=0xDBFF).contains(&first) {
            if self.read_raw()? != Some(b'\\') || self.read_raw()? != Some(b'u') {
                return Err(self.invalid("high surrogate without a low surrogate"));
            }
            let second = self.read_hex_quad()?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return Err(self.invalid("invalid low surrogate"));
            }
            0x10000 + (((u32::from(first) - 0xD800) << 10) | (u32::from(second) - 0xDC00))
        } else if (0xDC00..=0xDFFF).contains(&first) {
            return Err(self.invalid("unexpected low surrogate"));
        } else {
            u32::from(first)
        };
        char::from_u32(scalar).ok_or_else(|| self.invalid("invalid Unicode scalar value"))
    }

    pub(super) fn read_string(&mut self) -> io::Result<String> {
        self.skip_whitespace()?;
        self.expect('"')?;
        let mut bytes = Vec::new();
        loop {
            let byte = self
                .read_raw()?
                .ok_or_else(|| self.invalid("unterminated string"))?;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escaped = self
                        .read_raw()?
                        .ok_or_else(|| self.invalid("unterminated escape sequence"))?;
                    match escaped {
                        b'"' | b'\\' | b'/' => bytes.push(escaped),
                        b'b' => bytes.push(0x08),
                        b'f' => bytes.push(0x0c),
                        b'n' => bytes.push(b'\n'),
                        b'r' => bytes.push(b'\r'),
                        b't' => bytes.push(b'\t'),
                        b'u' => {
                            let ch = self.read_unicode_escape()?;
                            let mut encoded = [0_u8; 4];
                            bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
                        }
                        _ => return Err(self.invalid("invalid escape sequence")),
                    }
                }
                0x00..=0x1f => return Err(self.invalid("unescaped control character in string")),
                _ => bytes.push(byte),
            }
        }
        String::from_utf8(bytes).map_err(|_| self.invalid("invalid UTF-8 in string"))
    }

    pub(super) fn read_number(&mut self) -> io::Result<Number> {
        self.skip_whitespace()?;
        let mut bytes = Vec::new();
        if self.peek_raw()? == Some(b'-') {
            bytes.push(self.read_raw()?.expect("peeked byte must be available"));
        }
        match self.peek_raw()? {
            Some(b'0') => {
                bytes.push(self.read_raw()?.expect("peeked byte must be available"));
                if matches!(self.peek_raw()?, Some(b'0'..=b'9')) {
                    return Err(self.invalid("leading zero in number"));
                }
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek_raw()?, Some(b'0'..=b'9')) {
                    bytes.push(self.read_raw()?.expect("peeked byte must be available"));
                }
            }
            _ => return Err(self.invalid("invalid number")),
        }
        let mut is_float = false;
        if self.peek_raw()? == Some(b'.') {
            is_float = true;
            bytes.push(self.read_raw()?.expect("peeked byte must be available"));
            if !matches!(self.peek_raw()?, Some(b'0'..=b'9')) {
                return Err(self.invalid("fraction requires at least one digit"));
            }
            while matches!(self.peek_raw()?, Some(b'0'..=b'9')) {
                bytes.push(self.read_raw()?.expect("peeked byte must be available"));
            }
        }
        if matches!(self.peek_raw()?, Some(b'e' | b'E')) {
            is_float = true;
            bytes.push(self.read_raw()?.expect("peeked byte must be available"));
            if matches!(self.peek_raw()?, Some(b'+' | b'-')) {
                bytes.push(self.read_raw()?.expect("peeked byte must be available"));
            }
            if !matches!(self.peek_raw()?, Some(b'0'..=b'9')) {
                return Err(self.invalid("exponent requires at least one digit"));
            }
            while matches!(self.peek_raw()?, Some(b'0'..=b'9')) {
                bytes.push(self.read_raw()?.expect("peeked byte must be available"));
            }
        }
        if !matches!(
            self.peek_raw()?,
            None | Some(b' ' | b'\n' | b'\r' | b'\t' | b',' | b']' | b'}' | b':')
        ) {
            return Err(self.invalid("invalid character after number"));
        }
        let text =
            crate::compat::str::from_utf8(&bytes).map_err(|_| self.invalid("invalid number"))?;
        if !is_float {
            return text
                .parse::<i32>()
                .map(Number::Int)
                .map_err(|_| self.invalid(format!("integer out of range: {text}")));
        }
        let value = text
            .parse::<f32>()
            .map_err(|_| self.invalid(format!("number out of range: {text}")))?;
        if value.is_finite() {
            Ok(Number::Float(value))
        } else {
            Err(self.invalid("non-finite number"))
        }
    }

    pub(super) fn read_obj_key(&mut self) -> io::Result<String> {
        let key = self.read_string()?;
        self.expect(':')?;
        Ok(key)
    }

    pub(super) fn expect_obj_key(&mut self, expected: &str) -> io::Result<()> {
        let key = self.read_obj_key()?;
        if key == expected {
            Ok(())
        } else {
            Err(self.invalid(format!("expected object key '{expected}', found '{key}'")))
        }
    }

    pub(super) fn read_value(&mut self) -> io::Result<JsonValue> {
        self.skip_whitespace()?;
        match self.peek_raw()? {
            Some(b'[') => {
                self.read_raw()?;
                Ok(JsonValue::Array)
            }
            Some(b'{') => {
                self.read_raw()?;
                Ok(JsonValue::Object)
            }
            Some(b'"') => self.read_string().map(JsonValue::String),
            Some(b't' | b'f') => self.read_boolean().map(JsonValue::Boolean),
            Some(b'n') => {
                self.read_null()?;
                Ok(JsonValue::Null)
            }
            Some(b'-' | b'0'..=b'9') => self.read_number().map(JsonValue::Number),
            Some(other) => Err(self.invalid(format!(
                "unexpected character '{}' while reading a value",
                char::from(other)
            ))),
            None => Err(self.invalid("unexpected end of input")),
        }
    }

    pub(super) fn expect_eof(&mut self) -> io::Result<()> {
        self.skip_whitespace()?;
        if self.peek_raw()?.is_none() {
            Ok(())
        } else {
            Err(self.invalid("trailing data after JSON document"))
        }
    }

    pub(super) fn skip_value(&mut self) -> io::Result<()> {
        match self.read_value()? {
            JsonValue::Array => {
                if self.peek()? != ']' {
                    loop {
                        self.skip_value()?;
                        if self.peek()? == ']' {
                            break;
                        }
                        self.expect(',')?;
                    }
                }
                self.expect(']')
            }
            JsonValue::Object => {
                if self.peek()? != '}' {
                    loop {
                        self.read_obj_key()?;
                        self.skip_value()?;
                        if self.peek()? == '}' {
                            break;
                        }
                        self.expect(',')?;
                    }
                }
                self.expect('}')
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OneByteReader<'a>(&'a [u8]);

    impl Read for OneByteReader<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() || buf.is_empty() {
                return Ok(0);
            }
            buf[0] = self.0[0];
            self.0 = &self.0[1..];
            Ok(1)
        }
    }

    #[test]
    fn decodes_strings_across_fragmented_input() {
        let input = br#""a\b\f\n\r\t\/\\\" \u00f1 \ud83d\ude00""#;
        let mut tokenizer = JsonTokenizer::new(OneByteReader(input));
        assert_eq!(
            tokenizer.read_string().unwrap(),
            "a\u{8}\u{c}\n\r\t/\\\" ñ 😀"
        );
        tokenizer.expect_eof().unwrap();
    }

    #[test]
    fn validates_number_grammar() {
        for valid in ["0", "-1", "1.5", "2e3", "-2.5E-2"] {
            JsonTokenizer::new(valid.as_bytes()).read_number().unwrap();
        }
        for invalid in ["01", "1.", "1e", "--1", "+1", "2147483648"] {
            assert!(
                JsonTokenizer::new(invalid.as_bytes())
                    .read_number()
                    .is_err()
            );
        }
    }

    #[test]
    fn rejects_invalid_strings() {
        for invalid in [r#""\x""#, r#""\ud800""#, "\"line\nfeed\""] {
            assert!(
                JsonTokenizer::new(invalid.as_bytes())
                    .read_string()
                    .is_err()
            );
        }
    }
}
