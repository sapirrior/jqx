//! Error types for JSON parsing, serialization, and stream validation.
//!
//! Provides `JsonError` carrying byte offsets and descriptive messages
//! without heap-allocated error frameworks.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonErrorKind {
    UnexpectedEof,
    InvalidSyntax(String),
    InvalidUtf8,
    TrailingCharacters,
    DepthLimitExceeded,
    TokenLimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    pub kind: JsonErrorKind,
    pub offset: u64,
}

impl JsonError {
    #[must_use]
    pub const fn new(kind: JsonErrorKind, offset: u64) -> Self {
        Self { kind, offset }
    }

    #[must_use]
    pub fn syntax(msg: impl Into<String>, offset: u64) -> Self {
        Self {
            kind: JsonErrorKind::InvalidSyntax(msg.into()),
            offset,
        }
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            JsonErrorKind::UnexpectedEof => {
                write!(f, "unexpected end of JSON input at byte offset {}", self.offset)
            }
            JsonErrorKind::InvalidSyntax(msg) => {
                write!(f, "JSON syntax error at byte offset {}: {}", self.offset, msg)
            }
            JsonErrorKind::InvalidUtf8 => {
                write!(f, "invalid UTF-8 sequence at byte offset {}", self.offset)
            }
            JsonErrorKind::TrailingCharacters => {
                write!(f, "trailing characters after JSON value at byte offset {}", self.offset)
            }
            JsonErrorKind::DepthLimitExceeded => {
                write!(f, "JSON nesting depth limit exceeded at byte offset {}", self.offset)
            }
            JsonErrorKind::TokenLimitExceeded => {
                write!(f, "JSON token size limit exceeded at byte offset {}", self.offset)
            }
        }
    }
}

impl std::error::Error for JsonError {}
