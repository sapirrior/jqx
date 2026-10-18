//! Token definitions for the jqx query language.
//!
//! Every token retains exact source `Span` information for diagnostic reporting.

use jqx_diag::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Identifiers & Literals
    Ident(String),
    StringLit(String),
    NumberLit(String),
    True,
    False,
    Null,

    // Punctuation & Delimiters
    Dot,          // .
    DotQuestion,  // ?.
    Question,     // ?
    Colon,        // :
    Comma,        // ,
    Arrow,        // =>
    Dollar,       // $
    Pipe,         // |

    OpenParen,    // (
    CloseParen,   // )
    OpenBracket,  // [
    CloseBracket, // ]
    OpenBrace,    // {
    CloseBrace,   // }

    // Operators
    EqEq,         // == or ===
    NotEq,        // != or !==
    Lt,           // <
    LtEq,         // <=
    Gt,           // >
    GtEq,         // >=
    AndAnd,       // &&
    OrOr,         // ||
    Bang,         // !
    Plus,         // +
    Minus,        // -
    Star,         // *
    Slash,        // /
    Percent,      // %

    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    #[must_use]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
