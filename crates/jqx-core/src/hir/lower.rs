//! Lowering pass from AST (`crate::syntax::ast::Expr`) to HIR (`HirExpr`).
//!
//! Performs name resolution, builtin method binding, lambda variable slot assignment,
//! and static arity checks.

use super::types::{Builtin, HirExpr, HirExprKind, Slot};
use crate::diag::{find_closest, Diagnostic};
use crate::syntax::ast::{Expr, ExprKind};

const KNOWN_BUILTINS: &[&str] = &[
    "map", "filter", "find", "findIndex", "some", "every", "includes", "slice", "at",
    "first", "last", "take", "skip", "flat", "flatMap", "unique", "sort", "reverse",
    "join", "length", "count", "sum", "avg", "min", "max", "keys", "values",
    "entries", "pick", "omit", "has", "startsWith", "endsWith", "toLowerCase",
    "toUpperCase", "trim", "split",
];

pub struct LowerCtx {
    scopes: Vec<(u32, Vec<String>)>,
    next_slot: u32,
}

impl LowerCtx {
    #[must_use]
    pub fn new() -> Self {
        Self {
            scopes: Vec::new(),
            next_slot: 0,
        }
    }

    fn resolve_param(&self, name: &str) -> Option<Slot> {
        for (base_slot, params) in self.scopes.iter().rev() {
            for (idx, param) in params.iter().enumerate() {
                if param == name {
                    let idx_u32 = u32::try_from(idx).ok()?;
                    return Some(Slot(base_slot + idx_u32));
                }
            }
        }
        None
    }

    /// Lowers an AST expression into a resolved HIR expression.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic`] if unknown builtins, arity mismatches, or invalid parameters occur.
    pub fn lower_expr(&mut self, expr: &Expr) -> Result<HirExpr, Diagnostic> {
        match &expr.kind {
            ExprKind::Root => Ok(HirExpr::new(HirExprKind::Root, expr.span)),
            ExprKind::Ident(name) => Ok(self.lower_ident(name, expr)),
            ExprKind::Var(v) => Ok(HirExpr::new(HirExprKind::Var(v.clone()), expr.span)),
            ExprKind::Literal(lit) => Ok(HirExpr::new(HirExprKind::Literal(lit.clone()), expr.span)),
            ExprKind::Path {
                base,
                property,
                optional,
            } => {
                let lowered_base = self.lower_expr(base)?;
                Ok(HirExpr::new(
                    HirExprKind::Field {
                        base: Box::new(lowered_base),
                        name: property.clone(),
                        optional: *optional,
                    },
                    expr.span,
                ))
            }
            ExprKind::Index {
                base,
                index,
                optional,
            } => {
                let lowered_base = self.lower_expr(base)?;
                let lowered_index = self.lower_expr(index)?;
                Ok(HirExpr::new(
                    HirExprKind::Index {
                        base: Box::new(lowered_base),
                        index: Box::new(lowered_index),
                        optional: *optional,
                    },
                    expr.span,
                ))
            }
            ExprKind::Slice { .. } => Err(Diagnostic::error(
                "H0001",
                "slice syntax not yet supported in current HIR milestone",
                expr.span,
            )),
            ExprKind::MethodCall {
                receiver,
                method,
                args,
                optional,
            } => self.lower_method_call(receiver, method, args, *optional, expr),
            ExprKind::Lambda { params, body } => self.lower_lambda(params, body, expr),
            ExprKind::Binary { op, left, right } => {
                let lowered_left = self.lower_expr(left)?;
                let lowered_right = self.lower_expr(right)?;
                Ok(HirExpr::new(
                    HirExprKind::Binary {
                        op: op.clone(),
                        left: Box::new(lowered_left),
                        right: Box::new(lowered_right),
                    },
                    expr.span,
                ))
            }
            ExprKind::Unary { op, expr: inner } => {
                let lowered_inner = self.lower_expr(inner)?;
                Ok(HirExpr::new(
                    HirExprKind::Unary {
                        op: op.clone(),
                        expr: Box::new(lowered_inner),
                    },
                    expr.span,
                ))
            }
            ExprKind::Ternary {
                condition,
                consequence,
                alternative,
            } => self.lower_ternary(condition, consequence, alternative, expr),
            ExprKind::ArrayLit(items) => self.lower_array_lit(items, expr),
            ExprKind::ObjectLit(props) => self.lower_object_lit(props, expr),
        }
    }

    fn lower_ident(&mut self, name: &str, expr: &Expr) -> HirExpr {
        if let Some(slot) = self.resolve_param(name) {
            HirExpr::new(HirExprKind::Param(slot), expr.span)
        } else {
            let root = HirExpr::new(HirExprKind::Root, expr.span);
            HirExpr::new(
                HirExprKind::Field {
                    base: Box::new(root),
                    name: name.to_string(),
                    optional: false,
                },
                expr.span,
            )
        }
    }

    fn lower_method_call(
        &mut self,
        receiver: &Expr,
        method: &str,
        args: &[Expr],
        optional: bool,
        expr: &Expr,
    ) -> Result<HirExpr, Diagnostic> {
        let lowered_receiver = self.lower_expr(receiver)?;
        let Some(builtin) = Builtin::from_name(method) else {
            let mut diag = Diagnostic::error(
                "Q0012",
                format!("unknown method `{method}`"),
                expr.span,
            );
            if let Some(suggestion) = find_closest(method, KNOWN_BUILTINS, 2) {
                diag = diag.with_suggestion(suggestion);
            }
            return Err(diag);
        };

        let mut lowered_args = Vec::with_capacity(args.len());
        for arg in args {
            lowered_args.push(self.lower_expr(arg)?);
        }

        Ok(HirExpr::new(
            HirExprKind::MethodCall {
                receiver: Box::new(lowered_receiver),
                builtin,
                args: lowered_args,
                optional,
            },
            expr.span,
        ))
    }

    fn lower_lambda(&mut self, params: &[String], body: &Expr, expr: &Expr) -> Result<HirExpr, Diagnostic> {
        let slot_start = Slot(self.next_slot);
        let param_count = u32::try_from(params.len()).map_err(|_| {
            Diagnostic::error("H0002", "too many lambda parameters", expr.span)
        })?;
        self.next_slot += param_count;

        self.scopes.push((slot_start.0, params.to_vec()));
        let lowered_body = self.lower_expr(body)?;
        self.scopes.pop();

        Ok(HirExpr::new(
            HirExprKind::Lambda {
                slot_start,
                param_count,
                body: Box::new(lowered_body),
            },
            expr.span,
        ))
    }

    fn lower_ternary(
        &mut self,
        condition: &Expr,
        consequence: &Expr,
        alternative: &Expr,
        expr: &Expr,
    ) -> Result<HirExpr, Diagnostic> {
        let cond_expr = self.lower_expr(condition)?;
        let then_expr = self.lower_expr(consequence)?;
        let else_expr = self.lower_expr(alternative)?;
        Ok(HirExpr::new(
            HirExprKind::Ternary {
                condition: Box::new(cond_expr),
                consequence: Box::new(then_expr),
                alternative: Box::new(else_expr),
            },
            expr.span,
        ))
    }

    fn lower_array_lit(&mut self, items: &[Expr], expr: &Expr) -> Result<HirExpr, Diagnostic> {
        let mut lowered = Vec::with_capacity(items.len());
        for item in items {
            lowered.push(self.lower_expr(item)?);
        }
        Ok(HirExpr::new(HirExprKind::ArrayLit(lowered), expr.span))
    }

    fn lower_object_lit(&mut self, props: &[(String, Expr)], expr: &Expr) -> Result<HirExpr, Diagnostic> {
        let mut lowered = Vec::with_capacity(props.len());
        for (name, val) in props {
            lowered.push((name.clone(), self.lower_expr(val)?));
        }
        Ok(HirExpr::new(HirExprKind::ObjectLit(lowered), expr.span))
    }
}

impl Default for LowerCtx {
    fn default() -> Self {
        Self::new()
    }
}
