//! `jqx-syntax`: Lexer, AST definitions, and query parser for `jqx`.
//!
//! Hand-written Pratt/precedence parser for the JavaScript-subset query grammar.

#![forbid(unsafe_code)]

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod token;

pub use ast::{BinaryOp, Expr, ExprKind, Literal, UnaryOp};
pub use lexer::Lexer;
pub use parser::Parser;
pub use token::{Token, TokenKind};

/// Helper function to parse a query string into an `Expr` AST.
///
/// # Errors
///
/// Returns [`jqx_diag::Diagnostic`] if lexing or parsing fails.
pub fn parse_query(input: &str) -> Result<Expr, jqx_diag::Diagnostic> {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    parser.parse_query()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_filter_map_pipeline() {
        // High-risk invariant: verify JS-subset method chaining with lambdas parses into valid AST
        let query = "users.filter(u => u.age > 21).map(u => u.name)";
        let expr = parse_query(query).expect("query should parse");

        match expr.kind {
            ExprKind::MethodCall {
                method,
                args,
                receiver,
                ..
            } => {
                assert_eq!(method, "map");
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0].kind, ExprKind::Lambda { .. }));

                // Receiver should be users.filter(...)
                match receiver.kind {
                    ExprKind::MethodCall {
                        method: inner_method,
                        args: inner_args,
                        ..
                    } => {
                        assert_eq!(inner_method, "filter");
                        assert_eq!(inner_args.len(), 1);
                    }
                    other => panic!("expected inner MethodCall, got {other:?}"),
                }
            }
            other => panic!("expected outer MethodCall, got {other:?}"),
        }
    }

    #[test]
    fn test_recursion_depth_limit_invariant() {
        // High-risk invariant: ensure nesting query beyond MAX_QUERY_DEPTH errors out gracefully
        let mut deep_query = String::new();
        for _ in 0..150 {
            deep_query.push('(');
        }
        deep_query.push_str("1");
        for _ in 0..150 {
            deep_query.push(')');
        }

        let err = parse_query(&deep_query).unwrap_err();
        assert_eq!(err.code, "Q0006");
        assert!(err.message.contains("depth limit exceeded"));
    }
}
