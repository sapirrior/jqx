//! `jqx-core`: Foundational query execution engine for `jqx`.
//!
//! Exposes the complete JSON query compiler, streaming tokenizer,
//! planner, and evaluation engine with zero external runtime dependencies.

#![forbid(unsafe_code)]

pub mod diag;
pub mod eval;
pub mod hir;
pub mod json;
pub mod plan;
pub mod syntax;

pub use diag::{Diagnostic, Severity, Span};
pub use eval::{evaluate, Budget};
pub use hir::{lower, Builtin, HirExpr, HirExprKind, Slot};
pub use json::{Event, EventSource, JsonError, Number, Object, Scalar, Tokenizer, Value};
pub use plan::{plan_query, Demand, MemoryClass, NavStep, StreamPlan};
pub use syntax::{parse_query, BinaryOp, Expr, ExprKind, Literal, UnaryOp};

use std::io::Read;

/// High-level executable JSON Query representation.
pub struct Query {
    plan: StreamPlan,
}

impl Query {
    /// Compiles a JavaScript-subset query string into an executable query.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic`] if lexing, parsing, name resolution, or planning fails.
    pub fn compile(query_str: &str) -> Result<Self, Diagnostic> {
        let ast = parse_query(query_str)?;
        let hir = lower(&ast)?;
        let plan = plan_query(&hir)?;
        Ok(Self { plan })
    }

    /// Generates a human-readable explanation of the execution plan and memory tier.
    #[must_use]
    pub fn explain(&self) -> String {
        self.plan.explain()
    }

    /// Evaluates the query over a streaming byte reader.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic`] if JSON parsing or query evaluation fails.
    pub fn run_reader<R: Read>(&self, reader: R, mut budget: Budget) -> Result<Value<'static>, Diagnostic> {
        let mut tokenizer = Tokenizer::new(reader);
        let root = Value::parse_from_source(&mut tokenizer).map_err(|e| {
            Diagnostic::error(
                "J0001",
                e.to_string(),
                Span::point(usize::try_from(e.offset).unwrap_or(0)),
            )
        })?.unwrap_or(Value::Null);

        evaluate(&self.plan.original_expr, &root, &mut budget)
    }

    /// Evaluates the query directly against an in-memory `Value`.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic`] if evaluation fails.
    pub fn run_value(&self, root: &Value<'static>, mut budget: Budget) -> Result<Value<'static>, Diagnostic> {
        evaluate(&self.plan.original_expr, root, &mut budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_end_to_end_pipeline_invariant() {
        // High-risk invariant: full compile -> stream reader execution pipeline
        let json_input = r#"{"users": [{"id": 1, "name": "Alice", "role": "admin"}, {"id": 2, "name": "Bob", "role": "user"}]}"#;
        let query = Query::compile("users.filter(u => u.role == \"admin\").map(u => u.name)").unwrap();
        let result = query.run_reader(json_input.as_bytes(), Budget::default()).unwrap();
        assert_eq!(result.to_string(), r#"["Alice"]"#);
    }
}
