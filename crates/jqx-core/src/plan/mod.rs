//! `jqx-plan`: Stream planner, prefix automaton extraction, and memory classification.
//!
//! Categorizes queries into honest memory classes (S0–S4) and isolates early-exit prefixes.

#![forbid(unsafe_code)]

pub mod planner;
pub mod types;

pub use planner::plan_query;
pub use types::{Demand, MemoryClass, NavStep, StreamPlan};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower;
    use crate::syntax::parse_query;

    #[test]
    fn test_memory_classification_invariant() {
        // High-risk invariant: sort is materializing (S3), find has early exit (S1)
        let sort_ast = parse_query("items.sort()").unwrap();
        let sort_hir = lower(&sort_ast).unwrap();
        let sort_plan = plan_query(&sort_hir).unwrap();
        assert_eq!(sort_plan.memory_class, MemoryClass::S3Materializing);
        assert!(!sort_plan.early_exit);

        let find_ast = parse_query("users.find(u => u.id == 42)").unwrap();
        let find_hir = lower(&find_ast).unwrap();
        let find_plan = plan_query(&find_hir).unwrap();
        assert_eq!(find_plan.memory_class, MemoryClass::S1PerElement);
        assert!(find_plan.early_exit);
    }
}
