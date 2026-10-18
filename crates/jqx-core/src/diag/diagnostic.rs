//! Diagnostic data structures and terminal rendering.
//!
//! Produces rustc/miette-style code frames with source lines, caret underlines,
//! and Did-You-Mean suggestions.

use crate::diag::span::Span;
use std::fmt::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Hint => write!(f, "hint"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub severity: Severity,
    pub label: Option<String>,
    pub suggestion: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
            severity: Severity::Error,
            label: None,
            suggestion: None,
        }
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    #[must_use]
    pub fn render(&self, source: &str) -> String {
        let (line_idx, col_idx, line_str) = find_line_and_col(source, self.span.start);
        let line_num = line_idx + 1;
        let col_num = col_idx + 1;

        let underline_len = if self.span.is_empty() {
            1
        } else {
            self.span.len()
        };

        let mut out = String::new();
        let _ = writeln!(out, "{}[{}]: {}", self.severity, self.code, self.message);
        let _ = writeln!(out, " --> query:{line_num}:{col_num}");
        let _ = writeln!(out, "  |");
        let _ = writeln!(out, "{line_num:2} | {line_str}");

        let padding = " ".repeat(col_idx);
        let underline = "^".repeat(underline_len);

        if let Some(suggestion) = &self.suggestion {
            let _ = writeln!(out, "  | {padding}{underline} did you mean `{suggestion}`?");
        } else if let Some(label) = &self.label {
            let _ = writeln!(out, "  | {padding}{underline} {label}");
        } else {
            let _ = writeln!(out, "  | {padding}{underline}");
        }

        out
    }
}

fn find_line_and_col(source: &str, target_offset: usize) -> (usize, usize, &str) {
    let mut current_offset = 0;

    for (line_idx, line) in source.split('\n').enumerate() {
        let line_len = line.len() + 1; // including '\n'
        if target_offset < current_offset + line_len || current_offset + line_len > source.len() {
            let col = target_offset.saturating_sub(current_offset);
            return (line_idx, col, line);
        }
        current_offset += line_len;
    }

    (0, 0, source)
}
