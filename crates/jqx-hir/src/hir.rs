//! High-Level Intermediate Representation (HIR) for jqx.
//!
//! Replaces string-based variable names with index-based `Slot`s and identifies
//! built-in methods at compile-time to eliminate runtime lookups.

use jqx_diag::Span;
use jqx_syntax::ast::{BinaryOp, Literal, UnaryOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    // Array transformations
    Map,
    Filter,
    Find,
    FindIndex,
    Some,
    Every,
    Includes,
    Slice,
    At,
    First,
    Last,
    Take,
    Skip,
    Flat,
    FlatMap,
    Unique,
    Sort,
    Reverse,
    Join,
    Length,
    Count,

    // Aggregates
    Sum,
    Avg,
    Min,
    Max,

    // Object operations
    Keys,
    Values,
    Entries,
    Pick,
    Omit,
    Has,

    // String operations
    StartsWith,
    EndsWith,
    ToLowerCase,
    ToUpperCase,
    Trim,
    Split,
}

impl Builtin {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "map" => Some(Self::Map),
            "filter" => Some(Self::Filter),
            "find" => Some(Self::Find),
            "findIndex" => Some(Self::FindIndex),
            "some" => Some(Self::Some),
            "every" => Some(Self::Every),
            "includes" => Some(Self::Includes),
            "slice" => Some(Self::Slice),
            "at" => Some(Self::At),
            "first" => Some(Self::First),
            "last" => Some(Self::Last),
            "take" => Some(Self::Take),
            "skip" => Some(Self::Skip),
            "flat" => Some(Self::Flat),
            "flatMap" => Some(Self::FlatMap),
            "unique" => Some(Self::Unique),
            "sort" => Some(Self::Sort),
            "reverse" => Some(Self::Reverse),
            "join" => Some(Self::Join),
            "length" => Some(Self::Length),
            "count" => Some(Self::Count),
            "sum" => Some(Self::Sum),
            "avg" => Some(Self::Avg),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            "keys" => Some(Self::Keys),
            "values" => Some(Self::Values),
            "entries" => Some(Self::Entries),
            "pick" => Some(Self::Pick),
            "omit" => Some(Self::Omit),
            "has" => Some(Self::Has),
            "startsWith" => Some(Self::StartsWith),
            "endsWith" => Some(Self::EndsWith),
            "toLowerCase" => Some(Self::ToLowerCase),
            "toUpperCase" => Some(Self::ToUpperCase),
            "trim" => Some(Self::Trim),
            "split" => Some(Self::Split),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum HirExprKind {
    Root,
    Field {
        base: Box<HirExpr>,
        name: String,
        optional: bool,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
        optional: bool,
    },
    Param(Slot),
    Var(String),
    Literal(Literal),
    MethodCall {
        receiver: Box<HirExpr>,
        builtin: Builtin,
        args: Vec<HirExpr>,
        optional: bool,
    },
    Lambda {
        slot_start: Slot,
        param_count: u32,
        body: Box<HirExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<HirExpr>,
        right: Box<HirExpr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<HirExpr>,
    },
    Ternary {
        condition: Box<HirExpr>,
        consequence: Box<HirExpr>,
        alternative: Box<HirExpr>,
    },
    ArrayLit(Vec<HirExpr>),
    ObjectLit(Vec<(String, HirExpr)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub span: Span,
}

impl HirExpr {
    #[must_use]
    pub const fn new(kind: HirExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}
