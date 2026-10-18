//! Abstract Syntax Tree (AST) definitions for the jqx query language.
//!
//! Every AST node maintains source `Span` information for compiler and runtime diagnostics.

use jqx_diag::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Null,
    Bool(bool),
    Number(String),
    String(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Root,
    Ident(String),
    Var(String),
    Literal(Literal),
    Path {
        base: Box<Expr>,
        property: String,
        optional: bool,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
        optional: bool,
    },
    Slice {
        base: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
        optional: bool,
    },
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Ternary {
        condition: Box<Expr>,
        consequence: Box<Expr>,
        alternative: Box<Expr>,
    },
    ArrayLit(Vec<Expr>),
    ObjectLit(Vec<(String, Expr)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    #[must_use]
    pub const fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}
