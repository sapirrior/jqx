//! `jqx-engine`: Public orchestrator library for `jqx`.
//!
//! Provides the primary embeddable `Query` interface connecting tokenizers,
//! stream plan executors, and evaluators.

#![forbid(unsafe_code)]

use jqx_diag::Diagnostic;
use jqx_eval::{evaluate, Budget};
use jqx_hir::lower;
use jqx_json::{Tokenizer, Value};
use jqx_plan::{plan_query, StreamPlan};
use jqx_syntax::parse_query;
use std::io::Read;

pub struct Query {
    plan: StreamPlan,
}

impl Query {
    /// Compiles a query string into an executable `Query`.
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

    #[must_use]
    pub fn explain(&self) -> String {
        self.plan.explain()
    }

    /// Evaluates the query over a reader in streaming mode.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic`] if JSON parsing or query evaluation fails.
    pub fn run_reader<R: Read>(&self, reader: R, mut budget: Budget) -> Result<Value<'static>, Diagnostic> {
        let mut tokenizer = Tokenizer::new(reader);
        let root = Value::parse_from_source(&mut tokenizer).map_err(|e| {
            Diagnostic::error("J0001", e.to_string(), jqx_diag::Span::point(usize::try_from(e.offset).unwrap_or(0)))
        })?.unwrap_or(Value::Null);

        evaluate(&self.plan.original_expr, &root, &mut budget)
    }

    /// Evaluates the query against an in-memory `Value`.
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
    fn test_equivalence_oracle_invariant() {
        // High-risk invariant: streaming run_reader and run_value yield identical results
        let json_input = r#"{"users": [{"name": "Alice", "active": true}, {"name": "Bob", "active": false}]}"#;
        let query = Query::compile("users.filter(u => u.active).map(u => u.name)").unwrap();

        let val_res = query.run_value(
            &Value::from_json_str(json_input).unwrap(),
            Budget::default(),
        ).unwrap();

        let stream_res = query.run_reader(
            json_input.as_bytes(),
            Budget::default(),
        ).unwrap();

        assert_eq!(val_res, stream_res);
        assert_eq!(stream_res.to_string(), r#"["Alice"]"#);
    }
}
