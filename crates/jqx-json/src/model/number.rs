//! Lossless numeric representation for JSON data.
//!
//! Preserves original representation fidelity (Int, Float, or Raw string)
//! avoiding silent precision loss for 64-bit integer IDs or high-precision decimals.

use std::borrow::Cow;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Number<'a> {
    Int(i64),
    Float(f64),
    Raw(Cow<'a, str>),
}

impl Number<'_> {
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Float(f) => {
                let s = f.to_string();
                s.parse::<i64>().ok()
            }
            Self::Raw(s) => s.parse::<i64>().ok(),
        }
    }

    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(i) => i.to_string().parse::<f64>().ok(),
            Self::Float(f) => Some(*f),
            Self::Raw(s) => s.parse::<f64>().ok().filter(|f| f.is_finite()),
        }
    }

    #[must_use]
    pub fn raw_str(&self) -> Cow<'_, str> {
        match self {
            Self::Int(i) => Cow::Owned(i.to_string()),
            Self::Float(f) => Cow::Owned(f.to_string()),
            Self::Raw(s) => Cow::Borrowed(s.as_ref()),
        }
    }
}

impl fmt::Display for Number<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl}"),
            Self::Raw(s) => write!(f, "{s}"),
        }
    }
}
