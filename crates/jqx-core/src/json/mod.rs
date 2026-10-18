//! `jqx-json`: Foundational data model, streaming tokenizer, and serializer for `jqx`.
//!
//! Provides the core `Value<'a>` AST, lossless `Number<'a>`, insertion-ordered
//! `Object<'a>`, and streaming pull tokenizer (`EventSource`) with zero third-party dependencies.

#![forbid(unsafe_code)]

pub mod error;
pub mod model;
pub mod stream;

pub use error::JsonError;
pub use model::{Number, Object, Value};
pub use stream::{Event, EventSource, Scalar, Tokenizer};
