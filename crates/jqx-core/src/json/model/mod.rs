//! Core JSON in-memory data structures.
//!
//! Exposes `Value`, `Number`, and `Object` representing standard JSON types
//! alongside the domain-specific `Absent` variant and zero-copy string slices.

pub mod number;
pub mod object;
pub mod parse;
pub mod value;

pub use number::Number;
pub use object::Object;
pub use value::Value;
