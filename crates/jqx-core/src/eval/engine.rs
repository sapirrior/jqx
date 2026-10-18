//! In-memory tree evaluator for jqx queries.
//!
//! Evaluates `HirExpr` nodes against `Value<'static>` DOM trees,
//! enforcing strict type checks and fuel tracking.

use super::budget::Budget;
use super::builtins::{eval_builtin, is_truthy};
use crate::diag::{Diagnostic, Span};
use crate::hir::{HirExpr, HirExprKind};
use crate::json::{Number, Object, Value};
use crate::syntax::ast::{BinaryOp, Literal, UnaryOp};
use std::borrow::Cow;

/// Evaluates a lowered `HirExpr` AST against an in-memory DOM `Value`.
///
/// # Errors
///
/// Returns [`Diagnostic`] if fuel is exhausted, property access is invalid,
/// or type mismatches occur.
pub fn eval_expr(
    expr: &HirExpr,
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
) -> Result<Value<'static>, Diagnostic> {
    budget.consume_fuel(expr.span)?;

    match &expr.kind {
        HirExprKind::Root => Ok(root.clone()),
        HirExprKind::Param(slot) => Ok(eval_param_slot(slot.0, env)),
        HirExprKind::Var(name) => Err(Diagnostic::error(
            "R0005",
            format!("variable `${name}` not bound; pass `--var {name}=value`"),
            expr.span,
        )),
        HirExprKind::Literal(lit) => Ok(eval_literal(lit)),
        HirExprKind::Field {
            base,
            name,
            optional,
        } => eval_field_access(base, name, *optional, env, root, budget, expr.span),
        HirExprKind::Index {
            base,
            index,
            optional,
        } => eval_index_access(base, index, *optional, env, root, budget, expr.span),
        HirExprKind::MethodCall {
            receiver,
            builtin,
            args,
            optional,
        } => {
            let mut ctx = EvalContext { env, root, budget };
            ctx.eval_method_call(receiver, *builtin, args, *optional, expr.span)
        }
        HirExprKind::Binary { op, left, right } => {
            let left_val = eval_expr(left, env, root, budget)?;
            let right_val = eval_expr(right, env, root, budget)?;
            eval_binary_op(op, left_val, right_val, expr.span)
        }
        HirExprKind::Unary { op, expr: inner } => {
            let inner_val = eval_expr(inner, env, root, budget)?;
            eval_unary_op(op, inner_val, expr.span)
        }
        HirExprKind::Ternary {
            condition,
            consequence,
            alternative,
        } => {
            let cond_val = eval_expr(condition, env, root, budget)?;
            if is_truthy(&cond_val) {
                eval_expr(consequence, env, root, budget)
            } else {
                eval_expr(alternative, env, root, budget)
            }
        }
        HirExprKind::ArrayLit(items) => eval_array_literal(items, env, root, budget),
        HirExprKind::ObjectLit(props) => eval_object_literal(props, env, root, budget),
        HirExprKind::Lambda { .. } => Err(Diagnostic::error(
            "R0007",
            "lambdas cannot be evaluated directly as standalone values",
            expr.span,
        )),
    }
}

fn eval_param_slot(slot: u32, env: &[Value<'static>]) -> Value<'static> {
    let idx = usize::try_from(slot).unwrap_or(usize::MAX);
    env.get(idx).cloned().unwrap_or(Value::Absent)
}

fn eval_field_access(
    base: &HirExpr,
    name: &str,
    optional: bool,
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    let base_val = eval_expr(base, env, root, budget)?;
    match base_val {
        Value::Object(obj) => Ok(obj.get(name).cloned().unwrap_or(Value::Absent)),
        Value::Absent | Value::Null if optional => Ok(Value::Absent),
        other => {
            if optional {
                Ok(Value::Absent)
            } else {
                Err(Diagnostic::error(
                    "R0006",
                    format!(
                        "cannot access property `{name}` on type `{}`",
                        other.type_name()
                    ),
                    span,
                ))
            }
        }
    }
}

fn eval_index_access(
    base: &HirExpr,
    index: &HirExpr,
    optional: bool,
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    let base_val = eval_expr(base, env, root, budget)?;
    let idx_val = eval_expr(index, env, root, budget)?;
    eval_index(base_val, idx_val, optional, span)
}

struct EvalContext<'a, 'b> {
    env: &'a mut [Value<'static>],
    root: &'b Value<'static>,
    budget: &'a mut Budget,
}

impl EvalContext<'_, '_> {
    fn eval_method_call(
        &mut self,
        receiver: &HirExpr,
        builtin: crate::hir::Builtin,
        args: &[HirExpr],
        optional: bool,
        span: Span,
    ) -> Result<Value<'static>, Diagnostic> {
        let receiver_val = eval_expr(receiver, self.env, self.root, self.budget)?;
        if (receiver_val.is_absent() || receiver_val.is_null()) && optional {
            return Ok(Value::Absent);
        }
        eval_builtin(builtin, receiver_val, args, self.env, self.root, self.budget, span)
    }
}

fn eval_array_literal(
    items: &[HirExpr],
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
) -> Result<Value<'static>, Diagnostic> {
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        result.push(eval_expr(item, env, root, budget)?);
    }
    Ok(Value::Array(result))
}

fn eval_object_literal(
    props: &[(String, HirExpr)],
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
) -> Result<Value<'static>, Diagnostic> {
    let mut obj = Object::new();
    for (k, v) in props {
        let val = eval_expr(v, env, root, budget)?;
        if !val.is_absent() {
            obj.insert(Cow::Owned(k.clone()), val);
        }
    }
    Ok(Value::Object(obj))
}

/// Evaluates a lambda or expression argument within a callback context.
///
/// # Errors
///
/// Returns [`Diagnostic`] if parameter slots are out of range or evaluation fails.
pub fn eval_call_arg(
    arg: &HirExpr,
    param_val: Value<'static>,
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
) -> Result<Value<'static>, Diagnostic> {
    match &arg.kind {
        HirExprKind::Lambda {
            slot_start,
            body,
            ..
        } => {
            let slot = usize::try_from(slot_start.0).map_err(|_| {
                Diagnostic::error("R0008", "slot index out of environment bounds", arg.span)
            })?;
            if slot >= env.len() {
                return Err(Diagnostic::error(
                    "R0008",
                    "slot index out of environment bounds",
                    arg.span,
                ));
            }
            let prev = std::mem::replace(&mut env[slot], param_val);
            let res = eval_expr(body, env, root, budget);
            env[slot] = prev;
            res
        }
        _ => eval_expr(arg, env, root, budget),
    }
}

fn eval_literal(lit: &Literal) -> Value<'static> {
    match lit {
        Literal::Null => Value::Null,
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Number(n) => {
            if let Ok(i) = n.parse::<i64>() {
                Value::Number(Number::Int(i))
            } else if let Ok(f) = n.parse::<f64>() {
                Value::Number(Number::Float(f))
            } else {
                Value::Number(Number::Raw(Cow::Owned(n.clone())))
            }
        }
        Literal::String(s) => Value::String(Cow::Owned(s.clone())),
    }
}

fn eval_index(
    base: Value<'static>,
    index: Value<'static>,
    optional: bool,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    match (base, index) {
        (Value::Array(arr), Value::Number(n)) => {
            let Some(i) = n.as_i64() else {
                return Err(Diagnostic::error(
                    "R0009",
                    "array index must be an integer",
                    span,
                ));
            };
            let len = i64::try_from(arr.len()).map_err(|_| {
                Diagnostic::error("R0019", "array length overflowed i64", span)
            })?;
            let norm_idx = if i < 0 { len + i } else { i };
            if norm_idx >= 0 {
                if let Ok(u_idx) = usize::try_from(norm_idx) {
                    if u_idx < arr.len() {
                        return Ok(arr[u_idx].clone());
                    }
                }
            }
            Ok(Value::Absent)
        }
        (Value::Object(obj), Value::String(k)) => Ok(obj.get(&k).cloned().unwrap_or(Value::Absent)),
        (Value::Absent | Value::Null, _) if optional => Ok(Value::Absent),
        (other, _) => {
            if optional {
                Ok(Value::Absent)
            } else {
                Err(Diagnostic::error(
                    "R0013",
                    format!("cannot index into type `{}`", other.type_name()),
                    span,
                ))
            }
        }
    }
}

fn eval_binary_op(
    op: &BinaryOp,
    left: Value<'static>,
    right: Value<'static>,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    match op {
        BinaryOp::Eq => Ok(Value::Bool(left == right)),
        BinaryOp::NotEq => Ok(Value::Bool(left != right)),
        BinaryOp::And => {
            let truthy = is_truthy(&left);
            if truthy {
                Ok(right)
            } else {
                Ok(left)
            }
        }
        BinaryOp::Or => {
            let truthy = is_truthy(&left);
            if truthy {
                Ok(left)
            } else {
                Ok(right)
            }
        }
        BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => {
            eval_comparison(op, &left, &right, span)
        }
        BinaryOp::Add => eval_add(left, right, span),
        BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
            eval_arithmetic(op, left, right, span)
        }
    }
}

fn eval_comparison(
    op: &BinaryOp,
    left: &Value<'static>,
    right: &Value<'static>,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => {
            let ord = compare_numbers(a, b);
            let result = match op {
                BinaryOp::Lt => ord.is_lt(),
                BinaryOp::LtEq => ord.is_le(),
                BinaryOp::Gt => ord.is_gt(),
                BinaryOp::GtEq => ord.is_ge(),
                _ => false,
            };
            Ok(Value::Bool(result))
        }
        (Value::String(a), Value::String(b)) => {
            let ord = a.cmp(b);
            let result = match op {
                BinaryOp::Lt => ord.is_lt(),
                BinaryOp::LtEq => ord.is_le(),
                BinaryOp::Gt => ord.is_gt(),
                BinaryOp::GtEq => ord.is_ge(),
                _ => false,
            };
            Ok(Value::Bool(result))
        }
        // Strict type safety: no silent coercion!
        (a, b) => Err(Diagnostic::error(
            "R0031",
            format!(
                "cannot compare `{}` with `{}`",
                a.type_name(),
                b.type_name()
            ),
            span,
        )),
    }
}

fn compare_numbers(a: &Number<'_>, b: &Number<'_>) -> std::cmp::Ordering {
    match (a, b) {
        (Number::Int(i1), Number::Int(i2)) => i1.cmp(i2),
        (Number::Float(f1), Number::Float(f2)) => {
            f1.partial_cmp(f2).unwrap_or(std::cmp::Ordering::Equal)
        }
        _ => a.raw_str().cmp(&b.raw_str()),
    }
}

fn eval_add(
    left: Value<'static>,
    right: Value<'static>,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    match (left, right) {
        (Value::Number(Number::Int(a)), Value::Number(Number::Int(b))) => {
            Ok(Value::Number(Number::Int(a.saturating_add(b))))
        }
        (Value::String(mut a), Value::String(b)) => {
            a.to_mut().push_str(b.as_ref());
            Ok(Value::String(a))
        }
        (Value::Array(mut a), Value::Array(b)) => {
            a.extend(b);
            Ok(Value::Array(a))
        }
        (a, b) => Err(Diagnostic::error(
            "R0014",
            format!(
                "cannot add/concatenate type `{}` with `{}`",
                a.type_name(),
                b.type_name()
            ),
            span,
        )),
    }
}

fn eval_arithmetic(
    op: &BinaryOp,
    left: Value<'static>,
    right: Value<'static>,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    match (left, right) {
        (Value::Number(Number::Int(a)), Value::Number(Number::Int(b))) => {
            let res = match op {
                BinaryOp::Sub => a.saturating_sub(b),
                BinaryOp::Mul => a.saturating_mul(b),
                BinaryOp::Div => {
                    if b == 0 {
                        return Err(Diagnostic::error("R0015", "division by zero", span));
                    }
                    a / b
                }
                BinaryOp::Mod => {
                    if b == 0 {
                        return Err(Diagnostic::error("R0016", "modulo by zero", span));
                    }
                    a % b
                }
                _ => unreachable!(),
            };
            Ok(Value::Number(Number::Int(res)))
        }
        (a, b) => Err(Diagnostic::error(
            "R0017",
            format!(
                "cannot perform arithmetic on types `{}` and `{}`",
                a.type_name(),
                b.type_name()
            ),
            span,
        )),
    }
}

fn eval_unary_op(
    op: &UnaryOp,
    val: Value<'static>,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    match op {
        UnaryOp::Not => Ok(Value::Bool(!is_truthy(&val))),
        UnaryOp::Neg => match val {
            Value::Number(Number::Int(i)) => Ok(Value::Number(Number::Int(-i))),
            Value::Number(Number::Float(f)) => Ok(Value::Number(Number::Float(-f))),
            other => Err(Diagnostic::error(
                "R0018",
                format!("cannot negate type `{}`", other.type_name()),
                span,
            )),
        },
    }
}
