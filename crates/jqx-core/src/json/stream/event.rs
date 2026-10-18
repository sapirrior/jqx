//! Streaming JSON token and event definitions.
//!
//! Defines `Event` variants representing JSON document tokens and the `EventSource`
//! trait allowing pull-based streaming consumption and zero-allocation subtree skipping.

use crate::json::error::JsonError;
use crate::json::model::Number;
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq)]
pub enum Scalar<'a> {
    Null,
    Bool(bool),
    Number(Number<'a>),
    String(Cow<'a, str>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event<'a> {
    StartObject,
    EndObject,
    StartArray,
    EndArray,
    Key(Cow<'a, str>),
    Scalar(Scalar<'a>),
}

pub trait EventSource<'a> {
    /// Reads and returns the next streaming event from the input source.
    ///
    /// # Errors
    ///
    /// Returns [`JsonError`] if the input violates RFC 8259 syntax,
    /// encounters unexpected EOF, or exceeds configured depth/token limits.
    fn next_event(&mut self) -> Result<Option<Event<'a>>, JsonError>;

    /// Fast-forwards past the current composite or scalar value without allocating.
    ///
    /// # Errors
    ///
    /// Returns [`JsonError`] if the skipped value has invalid structural boundaries
    /// or encounters unexpected EOF.
    fn skip_value(&mut self) -> Result<(), JsonError>;

    fn offset(&self) -> u64;
}
