//! Pratt precedence parser for the jqx query language.
//!
//! Parses token streams into an `Expr` AST following the JavaScript-subset grammar.
//! Recursion depth is strictly bounded by `MAX_QUERY_DEPTH = 128` to prevent stack overflow.

use crate::ast::{BinaryOp, Expr, ExprKind, Literal, UnaryOp};
use crate::token::{Token, TokenKind};
use jqx_diag::{Diagnostic, Span};

const MAX_QUERY_DEPTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Lowest = 0,
    Ternary,    // ? :
    Or,         // ||
    And,        // &&
    Comparison, // == != < <= > >=
    Sum,        // + -
    Product,    // * / %
    Unary,      // ! -
    Postfix,    // . ?. [] ()
}

impl Precedence {
    fn from_token(kind: &TokenKind) -> Self {
        match kind {
            TokenKind::Question => Self::Ternary,
            TokenKind::OrOr => Self::Or,
            TokenKind::AndAnd => Self::And,
            TokenKind::EqEq
            | TokenKind::NotEq
            | TokenKind::Lt
            | TokenKind::LtEq
            | TokenKind::Gt
            | TokenKind::GtEq => Self::Comparison,
            TokenKind::Plus | TokenKind::Minus => Self::Sum,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Self::Product,
            TokenKind::Dot
            | TokenKind::DotQuestion
            | TokenKind::OpenBracket
            | TokenKind::OpenParen => Self::Postfix,
            _ => Self::Lowest,
        }
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    depth: usize,
}

impl Parser {
    #[must_use]
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            cursor: 0,
            depth: 0,
        }
    }

    fn current_token(&self) -> &Token {
        if self.cursor < self.tokens.len() {
            &self.tokens[self.cursor]
        } else {
            &self.tokens[self.tokens.len() - 1]
        }
    }

    fn advance(&mut self) -> &Token {
        let prev_cursor = self.cursor;
        if self.cursor < self.tokens.len() {
            self.cursor += 1;
        }
        &self.tokens[prev_cursor]
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<Token, Diagnostic> {
        let tok = self.current_token();
        if &tok.kind == expected {
            Ok(self.advance().clone())
        } else {
            Err(Diagnostic::error(
                "Q0004",
                format!("expected `{expected:?}`, found `{:?}`", tok.kind),
                tok.span,
            ))
        }
    }

    /// Parses the tokens into a top-level `Expr`.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic`] if the query contains syntax errors or exceeds max depth.
    pub fn parse_query(&mut self) -> Result<Expr, Diagnostic> {
        let expr = self.parse_expr(Precedence::Lowest)?;
        if self.current_token().kind != TokenKind::Eof {
            let tok = self.current_token();
            return Err(Diagnostic::error(
                "Q0005",
                format!("unexpected token `{:?}` after query expression", tok.kind),
                tok.span,
            ));
        }
        Ok(expr)
    }

    fn parse_expr(&mut self, precedence: Precedence) -> Result<Expr, Diagnostic> {
        if self.depth >= MAX_QUERY_DEPTH {
            return Err(Diagnostic::error(
                "Q0006",
                "query recursion depth limit exceeded (max 128)",
                self.current_token().span,
            ));
        }

        self.depth += 1;
        let mut left = self.parse_prefix()?;

        while precedence < Precedence::from_token(&self.current_token().kind) {
            left = self.parse_infix(left)?;
        }

        self.depth -= 1;
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expr, Diagnostic> {
        let tok = self.advance().clone();
        match tok.kind {
            TokenKind::Dollar => Ok(Expr::new(ExprKind::Root, tok.span)),
            TokenKind::Ident(name) => self.parse_ident_or_lambda(name, tok.span),
            TokenKind::NumberLit(n) => Ok(Expr::new(ExprKind::Literal(Literal::Number(n)), tok.span)),
            TokenKind::StringLit(s) => Ok(Expr::new(ExprKind::Literal(Literal::String(s)), tok.span)),
            TokenKind::True => Ok(Expr::new(ExprKind::Literal(Literal::Bool(true)), tok.span)),
            TokenKind::False => Ok(Expr::new(ExprKind::Literal(Literal::Bool(false)), tok.span)),
            TokenKind::Null => Ok(Expr::new(ExprKind::Literal(Literal::Null), tok.span)),
            TokenKind::Bang => self.parse_unary(UnaryOp::Not, tok.span),
            TokenKind::Minus => self.parse_unary(UnaryOp::Neg, tok.span),
            TokenKind::OpenParen => self.parse_paren_expr(tok.span),
            TokenKind::OpenBracket => self.parse_array_lit(tok.span),
            TokenKind::OpenBrace => self.parse_object_lit(tok.span),
            other => Err(Diagnostic::error(
                "Q0008",
                format!("unexpected token `{other:?}` in prefix position"),
                tok.span,
            )),
        }
    }

    fn parse_ident_or_lambda(&mut self, name: String, span: Span) -> Result<Expr, Diagnostic> {
        if self.current_token().kind == TokenKind::Arrow {
            self.advance();
            let body = self.parse_expr(Precedence::Lowest)?;
            let total_span = span.merge(body.span);
            Ok(Expr::new(
                ExprKind::Lambda {
                    params: vec![name],
                    body: Box::new(body),
                },
                total_span,
            ))
        } else {
            Ok(Expr::new(ExprKind::Ident(name), span))
        }
    }

    fn parse_unary(&mut self, op: UnaryOp, span: Span) -> Result<Expr, Diagnostic> {
        let operand = self.parse_expr(Precedence::Unary)?;
        let total_span = span.merge(operand.span);
        Ok(Expr::new(
            ExprKind::Unary {
                op,
                expr: Box::new(operand),
            },
            total_span,
        ))
    }

    fn parse_paren_expr(&mut self, span: Span) -> Result<Expr, Diagnostic> {
        if let TokenKind::Ident(_) = &self.current_token().kind {
            let save_cursor = self.cursor;
            self.advance();
            if self.current_token().kind == TokenKind::Comma
                || self.current_token().kind == TokenKind::CloseParen
            {
                self.cursor = save_cursor;
                return self.parse_lambda_with_parens(span);
            }
            self.cursor = save_cursor;
        }

        let inner = self.parse_expr(Precedence::Lowest)?;
        let closing = self.expect(&TokenKind::CloseParen)?;
        Ok(Expr::new(inner.kind, span.merge(closing.span)))
    }

    fn parse_array_lit(&mut self, span: Span) -> Result<Expr, Diagnostic> {
        let mut items = Vec::new();
        if self.current_token().kind != TokenKind::CloseBracket {
            loop {
                items.push(self.parse_expr(Precedence::Lowest)?);
                if self.current_token().kind == TokenKind::Comma {
                    self.advance();
                    if self.current_token().kind == TokenKind::CloseBracket {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        let closing = self.expect(&TokenKind::CloseBracket)?;
        Ok(Expr::new(ExprKind::ArrayLit(items), span.merge(closing.span)))
    }

    fn parse_object_lit(&mut self, span: Span) -> Result<Expr, Diagnostic> {
        let mut props = Vec::new();
        if self.current_token().kind != TokenKind::CloseBrace {
            loop {
                let prop_name = match &self.current_token().kind {
                    TokenKind::Ident(name) | TokenKind::StringLit(name) => name.clone(),
                    other => {
                        return Err(Diagnostic::error(
                            "Q0007",
                            format!("expected property name in object literal, found `{other:?}`"),
                            self.current_token().span,
                        ))
                    }
                };
                self.advance();
                self.expect(&TokenKind::Colon)?;
                let val = self.parse_expr(Precedence::Lowest)?;
                props.push((prop_name, val));

                if self.current_token().kind == TokenKind::Comma {
                    self.advance();
                    if self.current_token().kind == TokenKind::CloseBrace {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        let closing = self.expect(&TokenKind::CloseBrace)?;
        Ok(Expr::new(ExprKind::ObjectLit(props), span.merge(closing.span)))
    }

    fn parse_lambda_with_parens(&mut self, start_span: Span) -> Result<Expr, Diagnostic> {
        let mut params = Vec::new();
        while self.current_token().kind != TokenKind::CloseParen {
            let tok = self.advance().clone();
            if let TokenKind::Ident(param) = tok.kind {
                params.push(param);
            } else {
                return Err(Diagnostic::error(
                    "Q0009",
                    "expected parameter identifier in lambda argument list",
                    tok.span,
                ));
            }

            if self.current_token().kind == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(&TokenKind::CloseParen)?;
        self.expect(&TokenKind::Arrow)?;
        let body = self.parse_expr(Precedence::Lowest)?;
        let span = start_span.merge(body.span);
        Ok(Expr::new(
            ExprKind::Lambda {
                params,
                body: Box::new(body),
            },
            span,
        ))
    }

    fn parse_infix(&mut self, left: Expr) -> Result<Expr, Diagnostic> {
        let tok = self.advance().clone();
        match tok.kind {
            TokenKind::Dot | TokenKind::DotQuestion => self.parse_dot_or_call(left, &tok),
            TokenKind::OpenParen => self.parse_call_args_on_receiver(left, tok.span),
            TokenKind::OpenBracket => self.parse_index_expr(left),
            TokenKind::Question => self.parse_ternary_expr(left),
            _ => self.parse_binary_expr(left, &tok),
        }
    }

    fn parse_call_args_on_receiver(&mut self, left: Expr, paren_span: Span) -> Result<Expr, Diagnostic> {
        let mut args = Vec::new();
        if self.current_token().kind != TokenKind::CloseParen {
            loop {
                args.push(self.parse_expr(Precedence::Lowest)?);
                if self.current_token().kind == TokenKind::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let closing = self.expect(&TokenKind::CloseParen)?;
        let span = left.span.merge(closing.span);

        // If left was an Ident, transform it into a method call on Root: `$.filter(args...)`
        if let ExprKind::Ident(method) = left.kind {
            let root = Expr::new(ExprKind::Root, paren_span);
            Ok(Expr::new(
                ExprKind::MethodCall {
                    receiver: Box::new(root),
                    method,
                    args,
                    optional: false,
                },
                span,
            ))
        } else {
            Err(Diagnostic::error(
                "Q0015",
                "call expression only valid on identifiers or method properties",
                left.span,
            ))
        }
    }

    fn parse_dot_or_call(&mut self, left: Expr, tok: &Token) -> Result<Expr, Diagnostic> {
        let optional = tok.kind == TokenKind::DotQuestion;
        let member_tok = self.advance().clone();
        let member_name = match member_tok.kind {
            TokenKind::Ident(name) => name,
            other => {
                return Err(Diagnostic::error(
                    "Q0010",
                    format!("expected member identifier after dot, found `{other:?}`"),
                    member_tok.span,
                ))
            }
        };

        if self.current_token().kind == TokenKind::OpenParen {
            self.advance();
            let mut args = Vec::new();
            if self.current_token().kind != TokenKind::CloseParen {
                loop {
                    args.push(self.parse_expr(Precedence::Lowest)?);
                    if self.current_token().kind == TokenKind::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
            }
            let closing = self.expect(&TokenKind::CloseParen)?;
            let span = left.span.merge(closing.span);
            Ok(Expr::new(
                ExprKind::MethodCall {
                    receiver: Box::new(left),
                    method: member_name,
                    args,
                    optional,
                },
                span,
            ))
        } else {
            let span = left.span.merge(member_tok.span);
            Ok(Expr::new(
                ExprKind::Path {
                    base: Box::new(left),
                    property: member_name,
                    optional,
                },
                span,
            ))
        }
    }

    fn parse_index_expr(&mut self, left: Expr) -> Result<Expr, Diagnostic> {
        let idx_expr = self.parse_expr(Precedence::Lowest)?;
        let closing = self.expect(&TokenKind::CloseBracket)?;
        let span = left.span.merge(closing.span);
        Ok(Expr::new(
            ExprKind::Index {
                base: Box::new(left),
                index: Box::new(idx_expr),
                optional: false,
            },
            span,
        ))
    }

    fn parse_ternary_expr(&mut self, left: Expr) -> Result<Expr, Diagnostic> {
        let consequence = self.parse_expr(Precedence::Lowest)?;
        self.expect(&TokenKind::Colon)?;
        let alternative = self.parse_expr(Precedence::Ternary)?;
        let span = left.span.merge(alternative.span);
        Ok(Expr::new(
            ExprKind::Ternary {
                condition: Box::new(left),
                consequence: Box::new(consequence),
                alternative: Box::new(alternative),
            },
            span,
        ))
    }

    fn parse_binary_expr(&mut self, left: Expr, tok: &Token) -> Result<Expr, Diagnostic> {
        let op = match tok.kind {
            TokenKind::EqEq => BinaryOp::Eq,
            TokenKind::NotEq => BinaryOp::NotEq,
            TokenKind::Lt => BinaryOp::Lt,
            TokenKind::LtEq => BinaryOp::LtEq,
            TokenKind::Gt => BinaryOp::Gt,
            TokenKind::GtEq => BinaryOp::GtEq,
            TokenKind::AndAnd => BinaryOp::And,
            TokenKind::OrOr => BinaryOp::Or,
            TokenKind::Plus => BinaryOp::Add,
            TokenKind::Minus => BinaryOp::Sub,
            TokenKind::Star => BinaryOp::Mul,
            TokenKind::Slash => BinaryOp::Div,
            TokenKind::Percent => BinaryOp::Mod,
            _ => {
                return Err(Diagnostic::error(
                    "Q0011",
                    format!("unexpected token `{:?}` in infix position", tok.kind),
                    tok.span,
                ))
            }
        };
        let prec = Precedence::from_token(&tok.kind);
        let right = self.parse_expr(prec)?;
        let span = left.span.merge(right.span);
        Ok(Expr::new(
            ExprKind::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            span,
        ))
    }
}
