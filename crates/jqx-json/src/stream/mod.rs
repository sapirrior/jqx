//! Streaming JSON parser and tokenizer modules.
//!
//! Exposes event-driven parsing types and the pull-based `Tokenizer`
//! that powers streaming queries with constant memory overhead.

pub mod event;
pub mod tokenizer;

pub use event::{Event, EventSource, Scalar};
pub use tokenizer::Tokenizer;
