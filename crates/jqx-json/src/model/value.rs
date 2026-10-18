//! Core in-memory JSON Value AST.
//!
//! Features zero-copy borrowed strings (`Cow<'a, str>`), lossless numbers,
//! and an explicit `Absent` variant representing missing keys (distinct from `Null`).

use super::number::Number;
use super::object::Object;
use std::borrow::Cow;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    Absent,
    Null,
    Bool(bool),
    Number(Number<'a>),
    String(Cow<'a, str>),
    Array(Vec<Value<'a>>),
    Object(Object<'a>),
}

impl<'a> Value<'a> {
    #[must_use]
    pub const fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }

    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_number(&self) -> Option<&Number<'a>> {
        match self {
            Self::Number(n) => Some(n),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_ref()),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_array(&self) -> Option<&[Value<'a>]> {
        match self {
            Self::Array(arr) => Some(arr.as_slice()),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_object(&self) -> Option<&Object<'a>> {
        match self {
            Self::Object(obj) => Some(obj),
            _ => None,
        }
    }

    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Absent => "undefined",
            Self::Null => "null",
            Self::Bool(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }
}

impl fmt::Display for Value<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => Ok(()),
            Self::Null => write!(f, "null"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Number(num) => write!(f, "{num}"),
            Self::String(s) => {
                write!(f, "\"")?;
                for ch in s.chars() {
                    match ch {
                        '"' => write!(f, "\\\"")?,
                        '\\' => write!(f, "\\\\")?,
                        '\n' => write!(f, "\\n")?,
                        '\r' => write!(f, "\\r")?,
                        '\t' => write!(f, "\\t")?,
                        c if (c as u32) < 0x20 => write!(f, "\\u{:04x}", c as u32)?,
                        c => write!(f, "{c}")?,
                    }
                }
                write!(f, "\"")
            }
            Self::Array(arr) => {
                write!(f, "[")?;
                let mut first = true;
                for item in arr {
                    if item.is_absent() {
                        continue;
                    }
                    if !first {
                        write!(f, ",")?;
                    }
                    write!(f, "{item}")?;
                    first = false;
                }
                write!(f, "]")
            }
            Self::Object(obj) => {
                write!(f, "{{")?;
                let mut first = true;
                for (k, v) in obj.iter() {
                    if v.is_absent() {
                        continue;
                    }
                    if !first {
                        write!(f, ",")?;
                    }
                    write!(f, "\"{k}\":{v}")?;
                    first = false;
                }
                write!(f, "}}")
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_absent_and_null_invariants() {
        // High-risk invariant: Absent must never serialize or be confused with Null.
        let mut obj = Object::new();
        obj.insert(Cow::Borrowed("present"), Value::Null);
        obj.insert(Cow::Borrowed("missing"), Value::Absent);

        let val = Value::Object(obj);
        let serialized = format!("{val}");

        assert_eq!(serialized, r#"{"present":null}"#);
        assert!(!serialized.contains("missing"));
    }

    #[test]
    fn test_lossless_numbers_invariant() {
        // High-risk invariant: large integers and raw decimals must not lose precision.
        let large_id_str = "123456789012345678901234567890";
        let parsed = Value::from_json_str(large_id_str).unwrap();

        match parsed {
            Value::Number(Number::Raw(s)) => assert_eq!(s, large_id_str),
            other => panic!("expected Number::Raw, got {other:?}"),
        }
    }
}
