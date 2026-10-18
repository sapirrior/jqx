//! Query planner decomposing HIR into navigational prefix and computational suffix.
//!
//! Separates streaming path navigation from in-memory subtree evaluations.

use super::types::{Demand, MemoryClass, NavStep, StreamPlan};
use crate::diag::Diagnostic;
use crate::hir::{Builtin, HirExpr, HirExprKind};

/// Compiles a lowered `HirExpr` into an optimized `StreamPlan`.
///
/// # Errors
///
/// Returns [`Diagnostic`] if the expression tree cannot be planned.
pub fn plan_query(expr: &HirExpr) -> Result<StreamPlan, Diagnostic> {
    let mut prefix = Vec::new();
    let suffix = extract_prefix(expr, &mut prefix);

    let (memory_class, demand, early_exit) = classify(&suffix);

    Ok(StreamPlan {
        prefix,
        suffix,
        original_expr: expr.clone(),
        demand,
        memory_class,
        early_exit,
    })
}

fn extract_prefix(expr: &HirExpr, steps: &mut Vec<NavStep>) -> HirExpr {
    match &expr.kind {
        HirExprKind::Field {
            base,
            name,
            optional: false,
        } => {
            if matches!(base.kind, HirExprKind::Root) {
                steps.push(NavStep::Field(name.clone()));
                *base.clone()
            } else {
                let inner_base = extract_prefix(base, steps);
                if matches!(inner_base.kind, HirExprKind::Root) {
                    steps.push(NavStep::Field(name.clone()));
                    inner_base
                } else {
                    expr.clone()
                }
            }
        }
        _ => expr.clone(),
    }
}

fn classify(suffix: &HirExpr) -> (MemoryClass, Demand, bool) {
    match &suffix.kind {
        HirExprKind::MethodCall { builtin, .. } => match builtin {
            Builtin::Find | Builtin::First => (MemoryClass::S1PerElement, Demand::First, true),
            Builtin::Take => (MemoryClass::S1PerElement, Demand::AtMost(10), true),
            Builtin::Filter | Builtin::Map => (MemoryClass::S1PerElement, Demand::Unbounded, false),
            Builtin::Sum | Builtin::Avg | Builtin::Min | Builtin::Max | Builtin::Length => {
                (MemoryClass::S2BoundedBuffer, Demand::Unbounded, false)
            }
            Builtin::Sort | Builtin::Reverse | Builtin::Unique => {
                (MemoryClass::S3Materializing, Demand::Unbounded, false)
            }
            _ => (MemoryClass::S4WholeDoc, Demand::Unbounded, false),
        },
        _ => (MemoryClass::S0PureStream, Demand::Unbounded, false),
    }
}
