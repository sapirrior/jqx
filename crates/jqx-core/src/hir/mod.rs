//! `jqx-hir`: High-level intermediate representation, name resolution, and builtin binding.
//!
//! Transforms AST expressions into slot-indexed HIR expressions for efficient runtime evaluation.

#![forbid(unsafe_code)]

pub mod lower;
pub mod types;

pub use lower::LowerCtx;
pub use types::{Builtin, HirExpr, HirExprKind, Slot};

/// Convenience function to lower an AST `Expr` into `HirExpr`.
///
/// # Errors
///
/// Returns [`crate::diag::Diagnostic`] if name resolution or validation fails.
pub fn lower(expr: &crate::syntax::ast::Expr) -> Result<HirExpr, crate::diag::Diagnostic> {
    let mut ctx = LowerCtx::new();
    ctx.lower_expr(expr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse_query;

    #[test]
    fn test_lambda_slot_binding_invariant() {
        // High-risk invariant: lambda parameter `u` must resolve to Slot(0) without string lookups
        let ast = parse_query("users.filter(u => u.age > 21)").unwrap();
        let hir = lower(&ast).unwrap();

        match hir.kind {
            HirExprKind::MethodCall {
                builtin,
                args,
                receiver,
                ..
            } => {
                assert_eq!(builtin, Builtin::Filter);
                assert_eq!(args.len(), 1);

                // Receiver should be root field `users`
                match receiver.kind {
                    HirExprKind::Field { name, .. } => assert_eq!(name, "users"),
                    other => panic!("expected Field, got {other:?}"),
                }

                // Lambda body should reference Param(Slot(0))
                match &args[0].kind {
                    HirExprKind::Lambda {
                        slot_start,
                        param_count,
                        body,
                    } => {
                        assert_eq!(*slot_start, Slot(0));
                        assert_eq!(*param_count, 1);
                        match &body.kind {
                            HirExprKind::Binary { left, .. } => match &left.kind {
                                HirExprKind::Field { base, name, .. } => {
                                    assert_eq!(name, "age");
                                    assert_eq!(base.kind, HirExprKind::Param(Slot(0)));
                                }
                                other => panic!("expected Field, got {other:?}"),
                            },
                            other => panic!("expected Binary, got {other:?}"),
                        }
                    }
                    other => panic!("expected Lambda, got {other:?}"),
                }
            }
            other => panic!("expected MethodCall, got {other:?}"),
        }
    }

    #[test]
    fn test_did_you_mean_suggestion_invariant() {
        // High-risk invariant: typo `fliter` produces Q0012 with suggestion `filter`
        let ast = parse_query("users.fliter(u => u.age > 21)").unwrap();
        let err = lower(&ast).unwrap_err();

        assert_eq!(err.code, "Q0012");
        assert_eq!(err.suggestion.as_deref(), Some("filter"));
    }
}
