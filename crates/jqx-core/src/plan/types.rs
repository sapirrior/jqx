//! Execution plans and memory classifications for streaming queries.
//!
//! Classifies queries into memory tiers S0–S4 and builds prefix automata for skip-don't-parse execution.

use crate::hir::HirExpr;
use std::fmt::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryClass {
    /// Pure stream: O(depth + selected element), early exit capable.
    S0PureStream,
    /// Per-element: O(depth + 1 element), early exit capable on find/first/take.
    S1PerElement,
    /// Bounded buffer: O(k) or O(1) accumulators (e.g. at(-1), sum, count).
    S2BoundedBuffer,
    /// Materializing: O(array size) (e.g. sort, reverse, unique).
    S3Materializing,
    /// Whole document: O(document size).
    S4WholeDoc,
}

impl fmt::Display for MemoryClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::S0PureStream => write!(f, "S0 (pure stream)"),
            Self::S1PerElement => write!(f, "S1 (per-element)"),
            Self::S2BoundedBuffer => write!(f, "S2 (bounded buffer)"),
            Self::S3Materializing => write!(f, "S3 (materializing)"),
            Self::S4WholeDoc => write!(f, "S4 (whole document)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Demand {
    Unbounded,
    AtMost(usize),
    First,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavStep {
    Field(String),
    Index(usize),
    AnyIndex,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StreamPlan {
    pub prefix: Vec<NavStep>,
    pub suffix: HirExpr,
    pub original_expr: HirExpr,
    pub demand: Demand,
    pub memory_class: MemoryClass,
    pub early_exit: bool,
}

impl StreamPlan {
    #[must_use]
    pub fn explain(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "plan:   class {}", self.memory_class);
        let _ = writeln!(out, "exit:   early exit available: {}", self.early_exit);
        out.push_str("prefix: ");
        if self.prefix.is_empty() {
            out.push_str("<none>\n");
        } else {
            for step in &self.prefix {
                match step {
                    NavStep::Field(name) => {
                        let _ = write!(out, ".{name}");
                    }
                    NavStep::Index(idx) => {
                        let _ = write!(out, "[{idx}]");
                    }
                    NavStep::AnyIndex => {
                        out.push_str("[]");
                    }
                }
            }
            out.push('\n');
        }
        out
    }
}
