//! Builtin method execution library for jqx.
//!
//! Implements pure array, object, string, and aggregate transformations
//! operating over `Value<'static>`.

use super::budget::Budget;
use super::engine::{eval_call_arg, eval_expr};
use crate::diag::{Diagnostic, Span};
use crate::hir::{Builtin, HirExpr, HirExprKind};
use crate::json::{Number, Object, Value};
use std::borrow::Cow;

/// Evaluates a builtin method invocation over a materialized receiver `Value`.
///
/// # Errors
///
/// Returns [`Diagnostic`] if method arguments, receiver types, or budget constraints fail.
pub fn eval_builtin(
    builtin: Builtin,
    receiver: Value<'static>,
    args: &[HirExpr],
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    match builtin {
        Builtin::Length => eval_length(receiver, span),
        Builtin::Map => eval_map(receiver, args, env, root, budget, span),
        Builtin::Filter => eval_filter(receiver, args, env, root, budget, span),
        Builtin::Find => eval_find(receiver, args, env, root, budget, span),
        Builtin::Slice => eval_slice(receiver, args, env, root, budget, span),
        Builtin::Keys => eval_keys(receiver, span),
        Builtin::Values => eval_values(receiver, span),
        Builtin::Entries => eval_entries(receiver, span),
        Builtin::Pick => eval_pick(receiver, args, span),
        Builtin::Omit => eval_omit(receiver, args, span),
        Builtin::Sum => eval_sum(receiver, span),
        _ => Err(Diagnostic::error(
            "R0010",
            format!("builtin `{builtin:?}` not yet implemented in v1 milestone"),
            span,
        )),
    }
}

fn eval_length(receiver: Value<'static>, span: Span) -> Result<Value<'static>, Diagnostic> {
    match receiver {
        Value::Array(arr) => {
            let len = i64::try_from(arr.len()).map_err(|_| {
                Diagnostic::error("R0019", "array length overflowed i64", span)
            })?;
            Ok(Value::Number(Number::Int(len)))
        }
        Value::String(s) => {
            let len = i64::try_from(s.chars().count()).map_err(|_| {
                Diagnostic::error("R0019", "string length overflowed i64", span)
            })?;
            Ok(Value::Number(Number::Int(len)))
        }
        Value::Object(obj) => {
            let len = i64::try_from(obj.len()).map_err(|_| {
                Diagnostic::error("R0019", "object length overflowed i64", span)
            })?;
            Ok(Value::Number(Number::Int(len)))
        }
        other => Err(Diagnostic::error(
            "R0020",
            format!("cannot get length of type `{}`", other.type_name()),
            span,
        )),
    }
}

fn eval_map(
    receiver: Value<'static>,
    args: &[HirExpr],
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    let Value::Array(items) = receiver else {
        return Err(Diagnostic::error(
            "R0021",
            format!("cannot call `.map()` on type `{}`", receiver.type_name()),
            span,
        ));
    };

    let Some(first_arg) = args.first() else {
        return Err(Diagnostic::error("R0022", "`.map()` requires a callback argument", span));
    };

    let mut result = Vec::with_capacity(items.len());
    for item in items {
        budget.consume_fuel(span)?;
        let mapped = eval_call_arg(first_arg, item, env, root, budget)?;
        if !mapped.is_absent() {
            result.push(mapped);
        }
    }
    Ok(Value::Array(result))
}

fn eval_filter(
    receiver: Value<'static>,
    args: &[HirExpr],
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    let Value::Array(items) = receiver else {
        return Err(Diagnostic::error(
            "R0023",
            format!("cannot call `.filter()` on type `{}`", receiver.type_name()),
            span,
        ));
    };

    let Some(first_arg) = args.first() else {
        return Err(Diagnostic::error("R0024", "`.filter()` requires a predicate argument", span));
    };

    let mut result = Vec::new();
    for item in items {
        budget.consume_fuel(span)?;
        let predicate_result = eval_call_arg(first_arg, item.clone(), env, root, budget)?;
        if is_truthy(&predicate_result) {
            result.push(item);
        }
    }
    Ok(Value::Array(result))
}

fn eval_find(
    receiver: Value<'static>,
    args: &[HirExpr],
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    let Value::Array(items) = receiver else {
        return Err(Diagnostic::error(
            "R0025",
            format!("cannot call `.find()` on type `{}`", receiver.type_name()),
            span,
        ));
    };

    let Some(first_arg) = args.first() else {
        return Err(Diagnostic::error("R0026", "`.find()` requires a predicate argument", span));
    };

    for item in items {
        budget.consume_fuel(span)?;
        let predicate_result = eval_call_arg(first_arg, item.clone(), env, root, budget)?;
        if is_truthy(&predicate_result) {
            return Ok(item);
        }
    }
    Ok(Value::Null)
}

fn eval_slice(
    receiver: Value<'static>,
    args: &[HirExpr],
    env: &mut [Value<'static>],
    root: &Value<'static>,
    budget: &mut Budget,
    span: Span,
) -> Result<Value<'static>, Diagnostic> {
    let (start_idx, end_idx) = match args {
        [s, e] => {
            let s_val = eval_expr(s, env, root, budget)?;
            let e_val = eval_expr(e, env, root, budget)?;
            (s_val.as_number().and_then(Number::as_i64).unwrap_or(0),
             e_val.as_number().and_then(Number::as_i64))
        }
        [s] => {
            let s_val = eval_expr(s, env, root, budget)?;
            (s_val.as_number().and_then(Number::as_i64).unwrap_or(0), None)
        }
        _ => (0, None),
    };

    match receiver {
        Value::Array(arr) => {
            let len = i64::try_from(arr.len()).map_err(|_| {
                Diagnostic::error("R0019", "array length overflowed i64", span)
            })?;
            let start = normalize_index(start_idx, len);
            let end = end_idx.map_or(arr.len(), |e| normalize_index(e, len));
            let sliced = if start < end {
                arr[start..end.min(arr.len())].to_vec()
            } else {
                Vec::new()
            };
            Ok(Value::Array(sliced))
        }
        other => Err(Diagnostic::error(
            "R0027",
            format!("cannot call `.slice()` on type `{}`", other.type_name()),
            span,
        )),
    }
}

fn eval_keys(receiver: Value<'static>, span: Span) -> Result<Value<'static>, Diagnostic> {
    match receiver {
        Value::Object(obj) => {
            let keys: Vec<Value<'static>> = obj
                .iter()
                .map(|(k, _)| Value::String(Cow::Owned(k.to_string())))
                .collect();
            Ok(Value::Array(keys))
        }
        other => Err(Diagnostic::error(
            "R0028",
            format!("cannot get keys of non-object type `{}`", other.type_name()),
            span,
        )),
    }
}

fn eval_values(receiver: Value<'static>, span: Span) -> Result<Value<'static>, Diagnostic> {
    match receiver {
        Value::Object(obj) => {
            let values: Vec<Value<'static>> = obj
                .iter()
                .map(|(_, v)| v.clone())
                .collect();
            Ok(Value::Array(values))
        }
        other => Err(Diagnostic::error(
            "R0029",
            format!("cannot get values of non-object type `{}`", other.type_name()),
            span,
        )),
    }
}

fn eval_entries(receiver: Value<'static>, span: Span) -> Result<Value<'static>, Diagnostic> {
    match receiver {
        Value::Object(obj) => {
            let entries: Vec<Value<'static>> = obj
                .iter()
                .map(|(k, v)| {
                    Value::Array(vec![
                        Value::String(Cow::Owned(k.to_string())),
                        v.clone(),
                    ])
                })
                .collect();
            Ok(Value::Array(entries))
        }
        other => Err(Diagnostic::error(
            "R0030",
            format!("cannot get entries of non-object type `{}`", other.type_name()),
            span,
        )),
    }
}

fn eval_pick(receiver: Value<'static>, args: &[HirExpr], span: Span) -> Result<Value<'static>, Diagnostic> {
    match receiver {
        Value::Object(obj) => {
            let mut result = Object::new();
            for arg in args {
                if let HirExprKind::Field { name, .. } = &arg.kind {
                    if let Some(val) = obj.get(name) {
                        result.insert(Cow::Owned(name.clone()), val.clone());
                    }
                }
            }
            Ok(Value::Object(result))
        }
        other => Err(Diagnostic::error(
            "R0032",
            format!("cannot pick from non-object type `{}`", other.type_name()),
            span,
        )),
    }
}

fn eval_omit(receiver: Value<'static>, args: &[HirExpr], span: Span) -> Result<Value<'static>, Diagnostic> {
    match receiver {
        Value::Object(obj) => {
            let mut omit_names = Vec::new();
            for arg in args {
                if let HirExprKind::Field { name, .. } = &arg.kind {
                    omit_names.push(name.as_str());
                }
            }
            let mut result = Object::new();
            for (k, v) in obj.iter() {
                if !omit_names.contains(&k.as_ref()) {
                    result.insert(Cow::Owned(k.to_string()), v.clone());
                }
            }
            Ok(Value::Object(result))
        }
        other => Err(Diagnostic::error(
            "R0033",
            format!("cannot omit from non-object type `{}`", other.type_name()),
            span,
        )),
    }
}

fn eval_sum(receiver: Value<'static>, span: Span) -> Result<Value<'static>, Diagnostic> {
    match receiver {
        Value::Array(arr) => {
            let mut total_i = 0i64;
            let mut total_f = 0.0f64;
            let mut is_float = false;

            for item in arr {
                match item {
                    Value::Number(Number::Int(i)) => {
                        total_i = total_i.saturating_add(i);
                        if let Some(f) = Number::Int(i).as_f64() {
                            total_f += f;
                        }
                    }
                    Value::Number(Number::Float(f)) => {
                        is_float = true;
                        total_f += f;
                    }
                    other => {
                        return Err(Diagnostic::error(
                            "R0034",
                            format!("cannot sum non-number item of type `{}`", other.type_name()),
                            span,
                        ))
                    }
                }
            }

            if is_float {
                Ok(Value::Number(Number::Float(total_f)))
            } else {
                Ok(Value::Number(Number::Int(total_i)))
            }
        }
        other => Err(Diagnostic::error(
            "R0035",
            format!("cannot call `.sum()` on non-array type `{}`", other.type_name()),
            span,
        )),
    }
}

fn normalize_index(idx: i64, len: i64) -> usize {
    let resolved = if idx < 0 {
        (len + idx).max(0)
    } else {
        idx.min(len)
    };
    usize::try_from(resolved).unwrap_or(0)
}

#[must_use]
pub fn is_truthy(v: &Value<'_>) -> bool {
    match v {
        Value::Absent | Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64() != Some(0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
