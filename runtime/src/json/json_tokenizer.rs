//! Tokenizer for the streamed JSON parser.
use std::io::{self, Read};

#[derive(Debug)]
pub(super) enum Number {
    Int(i32),
    Float(f32),
}

impl Number {
    pub(super) fn as_integer(&self) -> Option<i32> {
        match self {
            Number::Int(n) => Some(*n),
            Number::Float(n) => Some(*n as i32),
        }
    }

    pub(super) fn as_float(&self) -> Option<f32> {
        match self {
            Number::Int(n) => Some(*n as f32),
            Number::Float(n) => Some(*n),
        }
    }

    pub(super) fn is_integer(&self) -> bool {
        match self {
            Number::Int(_) => true,
            Number::Float(_) => false,
        }
    }
}

#[derive(Debug)]
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
            JsonValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub(super) fn as_integer(&self) -> Option<i32> {
        match self {
            JsonValue::Number(n) => n.as_integer(),
            _ => None,
        }
    }
}

pub(super) struct JsonTokenizer<'a> {
    json: &'a [u8],
    lookahead: Option<char>,
    skip_whitespaces: bool,
}

impl<'a> JsonTokenizer<'a> {
    pub(super) fn new_from_str(s: &'a str) -> JsonTokenizer<'a> {
        JsonTokenizer {
            json: s.as_bytes(),
            lookahead: None,
            skip_whitespaces: true,
        }
    }

    pub(super) fn read(&mut self) -> io::Result<char> {
        let c = match self.lookahead {
            Some(c) => {
                self.lookahead = None;
                c
            }
            None => self.read_no_lookahead()?,
        };

        Ok(c)
    }

    fn read_no_lookahead(&mut self) -> io::Result<char> {
        let c = loop {
            let c = self.read_utf8_char()?;

            if !self.skip_whitespaces || !c.is_whitespace() {
                break c;
            }
        };

        Ok(c)
    }

    fn read_utf8_char(&mut self) -> io::Result<char> {
        let mut temp_buf = [0; 1];
        let mut utf8_char = Vec::new();

        // Read bytes until a valid UTF-8 character is formed
        loop {
            self.json.read_exact(&mut temp_buf)?;
            utf8_char.push(temp_buf[0]);

            if let Ok(utf8_str) = std::str::from_utf8(&utf8_char) {
                if let Some(ch) = utf8_str.chars().next() {
                    return Ok(ch);
                }
            }

            // If we have read 4 bytes and still not a valid character, return an error
            if utf8_char.len() >= 4 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid UTF-8 sequence",
                ));
            }
        }
    }

    pub(super) fn peek(&mut self) -> io::Result<char> {
        match self.lookahead {
            Some(c) => Ok(c),
            None => {
                let c = self.read_no_lookahead()?;
                self.lookahead = Some(c);
                Ok(c)
            }
        }
    }

    pub(super) fn read_boolean(&mut self) -> io::Result<bool> {
        let string = self.read_until_separator()?;

        match string.trim() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid boolean format",
            )),
        }
    }

    pub(super) fn read_null(&mut self) -> io::Result<()> {
        let string = self.read_until_separator()?;

        if string.trim() == "null" {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid null format",
            ))
        }
    }

    pub(super) fn read_number(&mut self) -> io::Result<Number> {
        let number_str = self.read_until_separator()?;
        let number_str = number_str.trim();

        // Check if the number is an integer
        if let Ok(num) = number_str.parse::<i32>() {
            return Ok(Number::Int(num));
        }

        // Convert the accumulated string to a f32
        match number_str.parse::<f32>() {
            Ok(num) => Ok(Number::Float(num)),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid number format: '{}'", number_str),
            )),
        }
    }

    pub(super) fn read_string(&mut self) -> io::Result<String> {
        let mut result = String::new();
        let mut escape = false;

        self.expect('"')?;
        self.skip_whitespaces = false;

        while let Ok(c) = self.read() {
            if escape {
                // Handle escape sequences
                match c {
                    '\\' => result.push('\\'),
                    '"' => result.push('"'),
                    'n' => result.push('\n'),
                    // 't' => result.push('\t'),
                    // 'r' => result.push('\r'),
                    // Add other escape sequences as needed
                    // _ => result.push(c), // Push the character as is if unknown escape
                    _ => {}
                }
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                self.skip_whitespaces = true;
                break; // End of the quoted string
            } else {
                result.push(c);
            }
        }

        if !escape {
            Ok(result)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unterminated string",
            ))
        }
    }

    fn read_until_separator(&mut self) -> io::Result<String> {
        let mut result = String::new();

        self.skip_whitespaces = false;

        while !self.next_is_separator() {
            let c = self.read()?;
            result.push(c);
        }

        self.skip_whitespaces = true;

        Ok(result)
    }

    fn next_is_separator(&mut self) -> bool {
        match self.peek() {
            Ok(c) => c == ',' || c == '}' || c == ']',
            Err(_) => true,
        }
    }

    pub(super) fn expect(&mut self, c: char) -> io::Result<()> {
        while let Ok(c2) = self.read() {
            if !c2.is_whitespace() {
                if c2 == c {
                    return Ok(());
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Expected '{}', found '{}'", c, c2),
                    ));
                }
            }
        }

        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Unexpected end of file",
        ))
    }

    pub(super) fn read_obj_key(&mut self) -> io::Result<String> {
        let s = self.read_string();
        self.expect(':')?;
        s
    }

    pub(super) fn expect_obj_key(&mut self, expected: &str) -> io::Result<()> {
        let s = self.read_string()?;
        if s != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Expected '{}', found '{}'", expected, s),
            ));
        }
        let _ = self.expect(':');
        Ok(())
    }

    pub(super) fn read_value(&mut self) -> io::Result<JsonValue> {
        //self.skip_whitespaces()?;
        match self.peek()? {
            '[' => {
                self.read()?;
                Ok(JsonValue::Array)
            }
            '{' => {
                self.read()?;
                Ok(JsonValue::Object)
            }
            '"' => {
                let s = self.read_string()?;
                Ok(JsonValue::String(s))
            }
            't' | 'f' => {
                let b = self.read_boolean()?;
                Ok(JsonValue::Boolean(b))
            }
            'n' => {
                self.read_null()?;
                Ok(JsonValue::Null)
            }
            _ => {
                let n = self.read_number()?;
                Ok(JsonValue::Number(n))
            }
        }
    }
}
