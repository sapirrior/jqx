//! Lexer for the jqx query language.
//!
//! Transforms input query strings into a stream of `Token`s,
//! attributing exact byte spans for precision error reporting.

use crate::token::{Token, TokenKind};
use jqx_diag::{Diagnostic, Span};

pub struct Lexer<'a> {
    input: &'a str,
    chars: Vec<(usize, char)>,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.char_indices().collect(),
            cursor: 0,
        }
    }

    fn current_offset(&self) -> usize {
        if self.cursor < self.chars.len() {
            self.chars[self.cursor].0
        } else {
            self.input.len()
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.cursor).map(|(_, c)| *c)
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.cursor + 1).map(|(_, c)| *c)
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek();
        if ch.is_some() {
            self.cursor += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn lex_identifier_or_keyword(&mut self) -> Token {
        let start = self.current_offset();
        let mut text = String::new();

        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                text.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let end = self.current_offset();
        let span = Span::new(start, end);

        let kind = match text.as_str() {
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            _ => TokenKind::Ident(text),
        };

        Token::new(kind, span)
    }

    fn lex_number(&mut self) -> Token {
        let start = self.current_offset();
        let mut text = String::new();

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
                text.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let end = self.current_offset();
        Token::new(TokenKind::NumberLit(text), Span::new(start, end))
    }

    fn lex_string(&mut self, quote: char) -> Result<Token, Diagnostic> {
        let start = self.current_offset();
        self.advance(); // consume opening quote
        let mut text = String::new();

        while let Some(ch) = self.peek() {
            if ch == quote {
                self.advance(); // consume closing quote
                let end = self.current_offset();
                return Ok(Token::new(TokenKind::StringLit(text), Span::new(start, end)));
            }
            if ch == '\\' {
                self.advance();
                let Some(esc) = self.advance() else {
                    return Err(Diagnostic::error(
                        "Q0001",
                        "unexpected end of query inside string escape",
                        Span::new(start, self.input.len()),
                    ));
                };
                match esc {
                    'n' => text.push('\n'),
                    'r' => text.push('\r'),
                    't' => text.push('\t'),
                    '"' => text.push('"'),
                    '\'' => text.push('\''),
                    '\\' => text.push('\\'),
                    other => text.push(other),
                }
            } else {
                text.push(ch);
                self.advance();
            }
        }

        Err(Diagnostic::error(
            "Q0002",
            "unterminated string literal",
            Span::new(start, self.input.len()),
        ))
    }

    fn lex_two_char_operator(&mut self, start: usize, ch: char) -> Option<Token> {
        match (ch, self.peek_next()) {
            ('?', Some('.')) => {
                self.advance();
                self.advance();
                Some(Token::new(TokenKind::DotQuestion, Span::new(start, start + 2)))
            }
            ('=', Some('>')) => {
                self.advance();
                self.advance();
                Some(Token::new(TokenKind::Arrow, Span::new(start, start + 2)))
            }
            ('&', Some('&')) => {
                self.advance();
                self.advance();
                Some(Token::new(TokenKind::AndAnd, Span::new(start, start + 2)))
            }
            ('|', Some('|')) => {
                self.advance();
                self.advance();
                Some(Token::new(TokenKind::OrOr, Span::new(start, start + 2)))
            }
            ('<', Some('=')) => {
                self.advance();
                self.advance();
                Some(Token::new(TokenKind::LtEq, Span::new(start, start + 2)))
            }
            ('>', Some('=')) => {
                self.advance();
                self.advance();
                Some(Token::new(TokenKind::GtEq, Span::new(start, start + 2)))
            }
            ('=', Some('=')) => {
                self.advance();
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Some(Token::new(TokenKind::EqEq, Span::new(start, start + 3)))
                } else {
                    Some(Token::new(TokenKind::EqEq, Span::new(start, start + 2)))
                }
            }
            ('!', Some('=')) => {
                self.advance();
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Some(Token::new(TokenKind::NotEq, Span::new(start, start + 3)))
                } else {
                    Some(Token::new(TokenKind::NotEq, Span::new(start, start + 2)))
                }
            }
            _ => None,
        }
    }

    fn lex_single_char_punctuation(&mut self, start: usize, ch: char) -> Option<Token> {
        let kind = match ch {
            '.' => TokenKind::Dot,
            '?' => TokenKind::Question,
            ':' => TokenKind::Colon,
            ',' => TokenKind::Comma,
            '$' => TokenKind::Dollar,
            '|' => TokenKind::Pipe,
            '(' => TokenKind::OpenParen,
            ')' => TokenKind::CloseParen,
            '[' => TokenKind::OpenBracket,
            ']' => TokenKind::CloseBracket,
            '{' => TokenKind::OpenBrace,
            '}' => TokenKind::CloseBrace,
            '<' => TokenKind::Lt,
            '>' => TokenKind::Gt,
            '!' => TokenKind::Bang,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            _ => return None,
        };
        self.advance();
        Some(Token::new(kind, Span::new(start, start + 1)))
    }

    /// Tokenizes the complete query string into tokens.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic`] if lexical errors (e.g., unterminated strings) occur.
    pub fn tokenize(&mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace();
            let start = self.current_offset();
            let Some(ch) = self.peek() else {
                tokens.push(Token::new(TokenKind::Eof, Span::point(self.input.len())));
                break;
            };

            if let Some(tok) = self.lex_two_char_operator(start, ch) {
                tokens.push(tok);
            } else if let Some(tok) = self.lex_single_char_punctuation(start, ch) {
                tokens.push(tok);
            } else {
                match ch {
                    '"' | '\'' => tokens.push(self.lex_string(ch)?),
                    '0'..='9' => tokens.push(self.lex_number()),
                    'a'..='z' | 'A'..='Z' | '_' => tokens.push(self.lex_identifier_or_keyword()),
                    unexpected => {
                        self.advance();
                        return Err(Diagnostic::error(
                            "Q0003",
                            format!("unexpected character `{unexpected}` in query"),
                            Span::new(start, start + 1),
                        ));
                    }
                }
            }
        }

        Ok(tokens)
    }
}
