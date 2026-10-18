//! `jqx-eval`: In-memory DOM evaluation engine for `jqx`.
//!
//! Evaluates `HirExpr` against `Value<'static>` with strict type comparisons and fuel/memory budgets.

#![forbid(unsafe_code)]

pub mod budget;
pub mod builtins;
pub mod eval;

pub use budget::Budget;
pub use eval::eval_expr;

/// Evaluates a lowered `HirExpr` against a root `Value<'static>` DOM tree.
///
/// # Errors
///
/// Returns [`jqx_diag::Diagnostic`] if runtime type mismatches, fuel exhaustion, or runtime errors occur.
pub fn evaluate(
    expr: &jqx_hir::HirExpr,
    root: &jqx_json::Value<'static>,
    budget: &mut Budget,
) -> Result<jqx_json::Value<'static>, jqx_diag::Diagnostic> {
    let mut env = vec![jqx_json::Value::Absent; 64];
    eval_expr(expr, &mut env, root, budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jqx_hir::lower;
    use jqx_json::Value;
    use jqx_syntax::parse_query;

    #[test]
    fn test_filter_map_pipeline_invariant() {
        // High-risk invariant: verify filter + map pipeline evaluates accurately over DOM
        let json_input = r#"[{"name": "Alice", "age": 30}, {"name": "Bob", "age": 17}, {"name": "Charlie", "age": 25}]"#;
        let root = Value::from_json_str(json_input).unwrap();

        let ast = parse_query("filter(u => u.age >= 21).map(u => u.name)").unwrap();
        let hir = lower(&ast).unwrap();
        let mut budget = Budget::default();

        let result = evaluate(&hir, &root, &mut budget).unwrap();
        assert_eq!(result.to_string(), r#"["Alice","Charlie"]"#);
    }

    #[test]
    fn test_strict_type_comparison_invariant() {
        // High-risk invariant: comparing string with number must error (R0031), never coerce silently!
        let json_input = r#"{"age": 30}"#;
        let root = Value::from_json_str(json_input).unwrap();

        let ast = parse_query("age > \"21\"").unwrap();
        let hir = lower(&ast).unwrap();
        let mut budget = Budget::default();

        let err = evaluate(&hir, &root, &mut budget).unwrap_err();
        assert_eq!(err.code, "R0031");
        assert!(err.message.contains("cannot compare `number` with `string`"));
    }

    #[test]
    fn test_fuel_exhaustion_invariant() {
        // High-risk invariant: exhausting fuel terminates evaluation safely
        let json_input = r#"[1, 2, 3, 4, 5]"#;
        let root = Value::from_json_str(json_input).unwrap();

        let ast = parse_query("map(x => x + 1)").unwrap();
        let hir = lower(&ast).unwrap();
        let mut budget = Budget::new(2, 1024 * 1024); // Only 2 steps of fuel

        let err = evaluate(&hir, &root, &mut budget).unwrap_err();
        assert_eq!(err.code, "R0001");
        assert!(err.message.contains("fuel budget exhausted"));
    }
}
