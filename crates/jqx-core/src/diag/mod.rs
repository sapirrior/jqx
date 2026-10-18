//! Diagnostic reporting, source spans, and Did-You-Mean suggestions.
//!
//! Provides rustc/miette-style terminal code frames and Levenshtein distance matching.

pub mod diagnostic;
pub mod span;
pub mod suggest;

pub use diagnostic::{Diagnostic, Severity};
pub use span::Span;
pub use suggest::{find_closest, levenshtein_distance};
