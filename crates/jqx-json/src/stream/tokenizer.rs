//! Pull-based streaming JSON tokenizer.
//!
//! Parses JSON bytes in a streaming manner, tracking nesting depth with
//! an explicit stack to prevent stack overflow.
//! Supports `skip_value()` to fast-forward through branches without allocating.

use super::event::{Event, EventSource, Scalar};
use crate::error::{JsonError, JsonErrorKind};
use crate::model::Number;
use std::borrow::Cow;
use std::io::Read;

const DEFAULT_MAX_DEPTH: usize = 512;
const CHUNK_SIZE: usize = 64 * 1024; // 64 KiB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainerState {
    ObjectExpectKeyOrEnd,
    ObjectExpectColon,
    ObjectExpectCommaOrEnd,
    ArrayExpectValueOrEnd,
    ArrayExpectCommaOrEnd,
}

enum StepResult {
    Event(Event<'static>),
    Continue,
}

pub struct Tokenizer<R> {
    reader: R,
    buffer: Vec<u8>,
    cursor: usize,
    bytes_read: usize,
    global_offset: u64,
    stack: Vec<ContainerState>,
    max_depth: usize,
    eof_reached: bool,
}

impl<R: Read> Tokenizer<R> {
    pub fn new(reader: R) -> Self {
        Self::with_max_depth(reader, DEFAULT_MAX_DEPTH)
    }

    pub fn with_max_depth(reader: R, max_depth: usize) -> Self {
        Self {
            reader,
            buffer: vec![0u8; CHUNK_SIZE],
            cursor: 0,
            bytes_read: 0,
            global_offset: 0,
            stack: Vec::with_capacity(32),
            max_depth,
            eof_reached: false,
        }
    }

    fn fill_buffer(&mut self) -> Result<bool, JsonError> {
        if self.cursor < self.bytes_read {
            return Ok(true);
        }
        if self.eof_reached {
            return Ok(false);
        }
        match self.reader.read(&mut self.buffer) {
            Ok(0) => {
                self.eof_reached = true;
                Ok(false)
            }
            Ok(n) => {
                self.cursor = 0;
                self.bytes_read = n;
                Ok(true)
            }
            Err(e) => Err(JsonError::syntax(format!("I/O read error: {e}"), self.global_offset)),
        }
    }

    fn peek_byte(&mut self) -> Result<Option<u8>, JsonError> {
        if !self.fill_buffer()? {
            return Ok(None);
        }
        Ok(Some(self.buffer[self.cursor]))
    }

    fn advance(&mut self) {
        if self.cursor < self.bytes_read {
            self.cursor += 1;
            self.global_offset += 1;
        }
    }

    fn skip_whitespace(&mut self) -> Result<(), JsonError> {
        while let Some(b) = self.peek_byte()? {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => self.advance(),
                _ => break,
            }
        }
        Ok(())
    }

    fn parse_unicode_escape(&mut self) -> Result<char, JsonError> {
        let mut hex_val = 0u32;
        for _ in 0..4 {
            let Some(hb) = self.peek_byte()? else {
                return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
            };
            self.advance();
            let digit = match hb {
                b'0'..=b'9' => u32::from(hb - b'0'),
                b'a'..=b'f' => u32::from(hb - b'a' + 10),
                b'A'..=b'F' => u32::from(hb - b'A' + 10),
                _ => {
                    return Err(JsonError::syntax(
                        "invalid hex character in \\u unicode escape",
                        self.global_offset,
                    ))
                }
            };
            hex_val = (hex_val << 4) | digit;
        }
        char::from_u32(hex_val).ok_or_else(|| {
            JsonError::syntax(
                format!("invalid unicode code point: \\u{hex_val:04x}"),
                self.global_offset,
            )
        })
    }

    fn parse_string_content(&mut self) -> Result<String, JsonError> {
        let start_offset = self.global_offset;
        self.advance();
        let mut result = String::new();

        while let Some(b) = self.peek_byte()? {
            self.advance();
            match b {
                b'"' => return Ok(result),
                b'\\' => {
                    let Some(esc) = self.peek_byte()? else {
                        return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
                    };
                    self.advance();
                    match esc {
                        b'"' => result.push('"'),
                        b'\\' => result.push('\\'),
                        b'/' => result.push('/'),
                        b'b' => result.push('\x08'),
                        b'f' => result.push('\x0c'),
                        b'n' => result.push('\n'),
                        b'r' => result.push('\r'),
                        b't' => result.push('\t'),
                        b'u' => result.push(self.parse_unicode_escape()?),
                        other => {
                            return Err(JsonError::syntax(
                                format!("invalid escape character: \\{}", other as char),
                                self.global_offset,
                            ))
                        }
                    }
                }
                c if c < 0x20 => {
                    return Err(JsonError::syntax(
                        "unescaped control character in string",
                        self.global_offset,
                    ))
                }
                valid_byte => result.push(valid_byte as char),
            }
        }

        Err(JsonError::new(JsonErrorKind::UnexpectedEof, start_offset))
    }

    fn parse_number_literal(&mut self) -> Result<Number<'static>, JsonError> {
        let start_offset = self.global_offset;
        let mut num_str = String::new();

        while let Some(b) = self.peek_byte()? {
            match b {
                b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E' => {
                    num_str.push(b as char);
                    self.advance();
                }
                _ => break,
            }
        }

        if let Ok(i) = num_str.parse::<i64>() {
            return Ok(Number::Int(i));
        }

        if let Ok(f) = num_str.parse::<f64>() {
            if f.is_finite() && f.to_string() == num_str {
                return Ok(Number::Float(f));
            }
        }

        if num_str.is_empty() {
            Err(JsonError::syntax("expected number digits", start_offset))
        } else {
            Ok(Number::Raw(Cow::Owned(num_str)))
        }
    }

    fn check_literal(&mut self, expected: &[u8]) -> Result<(), JsonError> {
        for &expected_byte in expected {
            let Some(actual) = self.peek_byte()? else {
                return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
            };
            if actual != expected_byte {
                return Err(JsonError::syntax(
                    format!(
                        "unexpected byte '{}', expected '{}'",
                        actual as char, expected_byte as char
                    ),
                    self.global_offset,
                ));
            }
            self.advance();
        }
        Ok(())
    }

    fn process_container_state(&mut self, state: ContainerState) -> Result<StepResult, JsonError> {
        match state {
            ContainerState::ObjectExpectKeyOrEnd => self.step_object_key_or_end(),
            ContainerState::ObjectExpectColon => self.step_object_colon(),
            ContainerState::ObjectExpectCommaOrEnd => self.step_object_comma_or_end(),
            ContainerState::ArrayExpectValueOrEnd => self.step_array_value_or_end(),
            ContainerState::ArrayExpectCommaOrEnd => self.step_array_comma_or_end(),
        }
    }

    fn step_object_key_or_end(&mut self) -> Result<StepResult, JsonError> {
        let Some(b) = self.peek_byte()? else {
            return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
        };
        if b == b'}' {
            self.advance();
            self.stack.pop();
            return Ok(StepResult::Event(Event::EndObject));
        }
        if b == b'"' {
            let key = self.parse_string_content()?;
            if let Some(top) = self.stack.last_mut() {
                *top = ContainerState::ObjectExpectColon;
            }
            return Ok(StepResult::Event(Event::Key(Cow::Owned(key))));
        }
        Err(JsonError::syntax("expected string key or '}' in object", self.global_offset))
    }

    fn step_object_colon(&mut self) -> Result<StepResult, JsonError> {
        let Some(b) = self.peek_byte()? else {
            return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
        };
        if b != b':' {
            return Err(JsonError::syntax("expected ':' after object key", self.global_offset));
        }
        self.advance();
        if let Some(top) = self.stack.last_mut() {
            *top = ContainerState::ObjectExpectCommaOrEnd;
        }
        self.skip_whitespace()?;
        Ok(StepResult::Continue)
    }

    fn step_object_comma_or_end(&mut self) -> Result<StepResult, JsonError> {
        let Some(b) = self.peek_byte()? else {
            return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
        };
        if b == b'}' {
            self.advance();
            self.stack.pop();
            return Ok(StepResult::Event(Event::EndObject));
        }
        if b == b',' {
            self.advance();
            if let Some(top) = self.stack.last_mut() {
                *top = ContainerState::ObjectExpectKeyOrEnd;
            }
            let next = self.next_event()?.ok_or_else(|| {
                JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset)
            })?;
            return Ok(StepResult::Event(next));
        }
        Err(JsonError::syntax("expected ',' or '}' in object", self.global_offset))
    }

    fn step_array_value_or_end(&mut self) -> Result<StepResult, JsonError> {
        let Some(b) = self.peek_byte()? else {
            return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
        };
        if b == b']' {
            self.advance();
            self.stack.pop();
            return Ok(StepResult::Event(Event::EndArray));
        }
        if let Some(top) = self.stack.last_mut() {
            *top = ContainerState::ArrayExpectCommaOrEnd;
        }
        Ok(StepResult::Continue)
    }

    fn step_array_comma_or_end(&mut self) -> Result<StepResult, JsonError> {
        let Some(b) = self.peek_byte()? else {
            return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
        };
        if b == b']' {
            self.advance();
            self.stack.pop();
            return Ok(StepResult::Event(Event::EndArray));
        }
        if b == b',' {
            self.advance();
            if let Some(top) = self.stack.last_mut() {
                *top = ContainerState::ArrayExpectValueOrEnd;
            }
            let next = self.next_event()?.ok_or_else(|| {
                JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset)
            })?;
            return Ok(StepResult::Event(next));
        }
        Err(JsonError::syntax("expected ',' or ']' in array", self.global_offset))
    }

    fn parse_scalar_or_start(&mut self, b: u8) -> Result<Event<'static>, JsonError> {
        match b {
            b'{' => {
                self.advance();
                if self.stack.len() >= self.max_depth {
                    return Err(JsonError::new(JsonErrorKind::DepthLimitExceeded, self.global_offset));
                }
                self.stack.push(ContainerState::ObjectExpectKeyOrEnd);
                Ok(Event::StartObject)
            }
            b'[' => {
                self.advance();
                if self.stack.len() >= self.max_depth {
                    return Err(JsonError::new(JsonErrorKind::DepthLimitExceeded, self.global_offset));
                }
                self.stack.push(ContainerState::ArrayExpectValueOrEnd);
                Ok(Event::StartArray)
            }
            b'"' => {
                let s = self.parse_string_content()?;
                Ok(Event::Scalar(Scalar::String(Cow::Owned(s))))
            }
            b't' => {
                self.check_literal(b"true")?;
                Ok(Event::Scalar(Scalar::Bool(true)))
            }
            b'f' => {
                self.check_literal(b"false")?;
                Ok(Event::Scalar(Scalar::Bool(false)))
            }
            b'n' => {
                self.check_literal(b"null")?;
                Ok(Event::Scalar(Scalar::Null))
            }
            b'-' | b'0'..=b'9' => {
                let num = self.parse_number_literal()?;
                Ok(Event::Scalar(Scalar::Number(num)))
            }
            other => Err(JsonError::syntax(
                format!("unexpected character '{}' starting JSON value", other as char),
                self.global_offset,
            )),
        }
    }
}

impl<R: Read> EventSource<'static> for Tokenizer<R> {
    fn offset(&self) -> u64 {
        self.global_offset
    }

    fn next_event(&mut self) -> Result<Option<Event<'static>>, JsonError> {
        self.skip_whitespace()?;

        if let Some(state) = self.stack.last().copied() {
            match self.process_container_state(state)? {
                StepResult::Event(ev) => return Ok(Some(ev)),
                StepResult::Continue => {}
            }
        }

        let Some(b) = self.peek_byte()? else {
            if self.stack.is_empty() {
                return Ok(None);
            }
            return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
        };

        self.parse_scalar_or_start(b).map(Some)
    }

    fn skip_value(&mut self) -> Result<(), JsonError> {
        let mut depth = 0usize;
        loop {
            match self.next_event()? {
                Some(Event::StartObject | Event::StartArray) => {
                    depth += 1;
                }
                Some(Event::EndObject | Event::EndArray) => {
                    if depth == 0 {
                        return Ok(());
                    }
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Some(Event::Scalar(_)) => {
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Some(Event::Key(_)) => {}
                None => {
                    return Err(JsonError::new(JsonErrorKind::UnexpectedEof, self.global_offset));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depth_limit_invariant() {
        let deep_input = "[".repeat(20);
        let mut tok = Tokenizer::with_max_depth(deep_input.as_bytes(), 10);
        let mut err = None;
        loop {
            match tok.next_event() {
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        assert!(matches!(
            err,
            Some(JsonError {
                kind: JsonErrorKind::DepthLimitExceeded,
                ..
            })
        ));
    }

    #[test]
    fn test_skip_value_invariant() {
        let json = r#"{"skip_me": {"nested": [1, 2, 3, {"inner": "val"}]}, "keep_me": 42}"#;
        let mut tok = Tokenizer::new(json.as_bytes());

        assert_eq!(tok.next_event().unwrap(), Some(Event::StartObject));
        assert_eq!(
            tok.next_event().unwrap(),
            Some(Event::Key(Cow::Borrowed("skip_me")))
        );

        tok.skip_value().unwrap();

        assert_eq!(
            tok.next_event().unwrap(),
            Some(Event::Key(Cow::Borrowed("keep_me")))
        );
        assert_eq!(
            tok.next_event().unwrap(),
            Some(Event::Scalar(Scalar::Number(Number::Int(42))))
        );
        assert_eq!(tok.next_event().unwrap(), Some(Event::EndObject));
    }

    #[test]
    fn test_unicode_and_escapes() {
        let json = r#"["hello\nworld", "escaped \"quotes\"", "\u0041\u0042\u0043"]"#;
        let mut tok = Tokenizer::new(json.as_bytes());

        assert_eq!(tok.next_event().unwrap(), Some(Event::StartArray));
        assert_eq!(
            tok.next_event().unwrap(),
            Some(Event::Scalar(Scalar::String(Cow::Owned("hello\nworld".into()))))
        );
        assert_eq!(
            tok.next_event().unwrap(),
            Some(Event::Scalar(Scalar::String(Cow::Owned("escaped \"quotes\"".into()))))
        );
        assert_eq!(
            tok.next_event().unwrap(),
            Some(Event::Scalar(Scalar::String(Cow::Owned("ABC".into()))))
        );
        assert_eq!(tok.next_event().unwrap(), Some(Event::EndArray));
    }
}
