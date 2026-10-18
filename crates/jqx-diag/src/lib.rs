//! `jqx-diag`: Source spans, diagnostics, and code-frame error rendering for `jqx`.
//!
//! Provides span tracking, formatting, and Did-You-Mean suggestions with zero dependencies.

#![forbid(unsafe_code)]

pub mod diagnostic;
pub mod span;
pub mod suggest;

pub use diagnostic::{Diagnostic, Severity};
pub use span::Span;
pub use suggest::{find_closest, levenshtein_distance};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_rendering_invariant() {
        // High-risk invariant: verify diagnostic correctly computes line, column, and caret underline
        let query = "users.fliter(u => u.age > 21)";
        let span = Span::new(6, 12); // "fliter"
        let diag = Diagnostic::error("Q0012", "unknown method `fliter`", span)
            .with_suggestion("filter");

        let rendered = diag.render(query);
        assert!(rendered.contains("error[Q0012]: unknown method `fliter`"));
        assert!(rendered.contains("--> query:1:7"));
        assert!(rendered.contains("users.fliter(u => u.age > 21)"));
        assert!(rendered.contains("^^^^^^ did you mean `filter`?"));
    }

    #[test]
    fn test_levenshtein_distance_invariant() {
        // High-risk invariant: verify distance algorithm accurately identifies closest match
        let candidates = &["filter", "map", "find", "slice", "length"];
        assert_eq!(find_closest("fliter", candidates, 2), Some("filter"));
        assert_eq!(find_closest("lenght", candidates, 2), Some("length"));
        assert_eq!(find_closest("completely_wrong", candidates, 2), None);
    }
}
