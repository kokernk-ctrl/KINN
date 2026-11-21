//! The Kin lexer: source text to tokens.
//!
//! Positions are 1-based (`line`, `col` in characters). Comments are
//! skipped and not tokenized. Lexical errors are collected and lexing
//! continues where it safely can.

use crate::diagnostics::Diagnostic;
use crate::token::{kind_name, Token, TokKind};

/// Tokenize `src`. Returns the token stream (ending in `Eof`) and any
/// lexical diagnostics.
pub fn tokenize(src: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        pos: 0,
        line: 1,
        col: 1,
        toks: Vec::new(),
        diags: Vec::new(),
    };
    lx.run();
    (lx.toks, lx.diags)
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    toks: Vec<Token>,
    diags: Vec<Diagnostic>,
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        if let Some(ch) = c {
            self.pos += 1;
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        c
    }

    fn run(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\r' | '\n' => {
                    self.bump();
                }
                '/' if self.peek2() == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                '/' if self.peek2() == Some('*') => {
                    self.bump();
                    self.bump();
                    let mut closed = false;
                    while let Some(c) = self.peek() {
                        if c == '*' && self.peek2() == Some('/') {
                            self.bump();
                            self.bump();
                            closed = true;
                            break;
                        }
                        self.bump();
                    }
                    if !closed {
                        self.diags.push(Diagnostic::error(
                            "E1006",
                            "unterminated block comment (missing `*/`)",
                            self.line,
                            self.col,
                            1,
                        ));
                    }
                }
                '0'..='9' => self.lex_number(),
                '"' => self.lex_string(),
                c if c.is_ascii_alphabetic() || c == '_' => self.lex_ident(),
                _ => self.lex_operator(),
            }
        }
        let (line, col) = (self.line, self.col);
        self.toks.push(Token { kind: TokKind::Eof, text: String::new(), line, col });
    }

    fn lex_number(&mut self) {
        let (line, col) = (self.line, self.col);
        let mut text = String::new();
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            text.push(self.bump().unwrap());
        }
        let mut is_float = false;
        if self.peek() == Some('.') {
            if matches!(self.peek2(), Some(c) if c.is_ascii_digit()) {
                is_float = true;
                text.push(self.bump().unwrap());
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    text.push(self.bump().unwrap());
                }
                if self.peek() == Some('.') && self.peek2() != Some('.') {
                    self.diags.push(Diagnostic::error(
                        "E1004",
                        format!("malformed number literal `{}` (a second `.` is not allowed)", text),
                        line,
                        col,
                        text.chars().count() as u32 + 1,
                    ));
                    return;
                }
            } else if self.peek2() == Some('.') {
                // `0..` — the `.` starts a range, not a float. Emit the
                // integer as-is; the main loop lexes the `..` next.
            } else {
                self.diags.push(Diagnostic::error(
                    "E1004",
                    "malformed number literal: expected a digit after `.`",
                    self.line,
                    self.col,
                    1,
                ));
                return;
            }
        }
        if is_float {
            if let Ok(v) = text.parse::<f64>() {
                self.toks.push(Token { kind: TokKind::Float(v), text, line, col });
            }
        } else {
            match text.parse::<i64>() {
                Ok(v) => self.toks.push(Token { kind: TokKind::Int(v), text, line, col }),
                Err(_) => self.diags.push(Diagnostic::error(
                    "E1005",
                    format!("integer literal `{}` is out of range for int (64-bit signed)", text),
                    line,
                    col,
                    text.chars().count() as u32,
                )),
            }
        }
    }

    fn lex_string(&mut self) {
        let (line, col) = (self.line, self.col);
        let mut raw = String::from("\"");
        self.bump(); // opening quote
        let mut value = String::new();
        loop {
            match self.peek() {
                None => {
                    self.diags.push(Diagnostic::error(
                        "E1002",
                        "unterminated string literal",
                        line,
                        col,
                        raw.chars().count() as u32,
                    ));
                    return;
                }
                Some('\n') => {
                    self.diags.push(Diagnostic::error(
                        "E1002",
                        "unterminated string literal (strings cannot span lines)",
                        line,
                        col,
                        raw.chars().count() as u32,
                    ));
                    return;
                }
                Some('"') => {
                    self.bump();
                    raw.push('"');
                    break;
                }
                Some('\\') => {
                    self.bump();
                    raw.push('\\');
                    match self.peek() {
                        Some('n') => {
                            self.bump();
                            raw.push('n');
                            value.push('\n');
                        }
                        Some('t') => {
                            self.bump();
                            raw.push('t');
                            value.push('\t');
                        }
                        Some('r') => {
                            self.bump();
                            raw.push('r');
                            value.push('\r');
                        }
                        Some('\\') => {
                            self.bump();
                            raw.push('\\');
                            value.push('\\');
                        }
                        Some('"') => {
                            self.bump();
                            raw.push('"');
                            value.push('"');
                        }
                        Some(c) => {
                            self.bump();
                            raw.push(c);
                            self.diags.push(Diagnostic::error(
                                "E1003",
                                format!(
                                    "invalid escape sequence `\\{}` in string literal (allowed: \\n \\t \\r \\\" \\\\)",
                                    c
                                ),
                                line,
                                col,
                                raw.chars().count() as u32,
                            ));
                        }
                        None => {
                            self.diags.push(Diagnostic::error(
                                "E1002",
                                "unterminated string literal",
                                line,
                                col,
                                raw.chars().count() as u32,
                            ));
                            return;
                        }
                    }
                }
                Some(c) => {
                    self.bump();
                    raw.push(c);
                    value.push(c);
                }
            }
        }
        self.toks.push(Token { kind: TokKind::Str(value), text: raw, line, col });
    }

    fn lex_ident(&mut self) {
        let (line, col) = (self.line, self.col);
        let mut text = String::new();
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
            text.push(self.bump().unwrap());
        }
        let kind = keyword(&text).unwrap_or(TokKind::Ident(text.clone()));
        self.toks.push(Token { kind, text, line, col });
    }

    fn lex_operator(&mut self) {
        let (line, col) = (self.line, self.col);
        let c = self.bump().unwrap();
        let mut text = String::new();
        text.push(c);
        let kind = match c {
            '*' => Some(TokKind::Star),
            '+' => self.two_char('=', TokKind::PlusEq, TokKind::Plus, &mut text),
            '-' => {
                if self.peek() == Some('=') {
                    self.bump();
                    text.push('=');
                    Some(TokKind::MinusEq)
                } else if self.peek() == Some('>') {
                    self.bump();
                    text.push('>');
                    Some(TokKind::Arrow)
                } else {
                    Some(TokKind::Minus)
                }
            }
            '/' => Some(TokKind::Slash),
            '%' => Some(TokKind::Percent),
            '=' => self.two_char('=', TokKind::Eq, TokKind::Assign, &mut text),
            '!' => {
                if self.peek() == Some('=') {
                    self.bump();
                    text.push('=');
                    Some(TokKind::NotEq)
                } else {
                    self.diags.push(Diagnostic::error(
                        "E1001",
                        "unexpected character `!` (use `not` for logical negation or `!=` for inequality)",
                        line,
                        col,
                        1,
                    ));
                    return;
                }
            }
            '<' => self.two_char('=', TokKind::Le, TokKind::Lt, &mut text),
            '>' => self.two_char('=', TokKind::Ge, TokKind::Gt, &mut text),
            '(' => Some(TokKind::LParen),
            ')' => Some(TokKind::RParen),
            '{' => Some(TokKind::LBrace),
            '}' => Some(TokKind::RBrace),
            '[' => Some(TokKind::LBracket),
            ']' => Some(TokKind::RBracket),
            ',' => Some(TokKind::Comma),
            ':' => Some(TokKind::Colon),
            ';' => Some(TokKind::Semi),
            '.' => {
                if self.peek() == Some('.') {
                    self.bump();
                    text.push('.');
                    if self.peek() == Some('=') {
                        self.bump();
                        text.push('=');
                        Some(TokKind::DotDotEq)
                    } else {
                        Some(TokKind::DotDot)
                    }
                } else {
                    Some(TokKind::Dot)
                }
            }
            other => {
                self.diags.push(Diagnostic::error(
                    "E1001",
                    format!("unexpected character `{}`", other),
                    line,
                    col,
                    1,
                ));
                return;
            }
        };
        if let Some(k) = kind {
            self.toks.push(Token { kind: k, text, line, col });
        }
    }

    fn two_char(
        &mut self,
        second: char,
        combined: TokKind,
        single: TokKind,
        text: &mut String,
    ) -> Option<TokKind> {
        if self.peek() == Some(second) {
            self.bump();
            text.push(second);
            Some(combined)
        } else {
            Some(single)
        }
    }
}

/// Keyword table. Type names are keywords so they cannot be used as
/// identifiers.
fn keyword(word: &str) -> Option<TokKind> {
    Some(match word {
        "func" => TokKind::Func,
        "let" => TokKind::Let,
        "var" => TokKind::Var,
        "if" => TokKind::If,
        "else" => TokKind::Else,
        "while" => TokKind::While,
        "for" => TokKind::For,
        "in" => TokKind::In,
        "return" => TokKind::Return,
        "break" => TokKind::Break,
        "continue" => TokKind::Continue,
        "true" => TokKind::True,
        "false" => TokKind::False,
        "and" => TokKind::And,
        "or" => TokKind::Or,
        "not" => TokKind::Not,
        "as" => TokKind::As,
        "import" => TokKind::Import,
        "int" => TokKind::IntTy,
        "float" => TokKind::FloatTy,
        "bool" => TokKind::BoolTy,
        "string" => TokKind::StrTy,
        "void" => TokKind::VoidTy,
        _ => return None,
    })
}

/// One-line token description for `kinn tokens` output (public re-export
/// helper).
pub fn token_summary(t: &Token) -> String {
    format!("{:>3}:{:<3} {:<10} {}", t.line, t.col, kind_name(&t.kind), t.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokKind> {
        tokenize(src).0.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn keywords_and_identifiers() {
        let ks = kinds("func let var if else while return break continue and or not");
        assert_eq!(
            ks,
            vec![
                TokKind::Func, TokKind::Let, TokKind::Var, TokKind::If, TokKind::Else,
                TokKind::While, TokKind::Return, TokKind::Break, TokKind::Continue,
                TokKind::And, TokKind::Or, TokKind::Not, TokKind::Eof,
            ]
        );
        let ks = kinds("my_var _x x2");
        assert_eq!(ks[0], TokKind::Ident("my_var".into()));
        assert_eq!(ks[1], TokKind::Ident("_x".into()));
        assert_eq!(ks[2], TokKind::Ident("x2".into()));
    }

    #[test]
    fn type_names_are_keywords() {
        let ks = kinds("int float bool string void");
        assert_eq!(
            ks,
            vec![
                TokKind::IntTy, TokKind::FloatTy, TokKind::BoolTy,
                TokKind::StrTy, TokKind::VoidTy, TokKind::Eof,
            ]
        );
    }

    #[test]
    fn numbers() {
        let ks = kinds("42 3.14 0 0.5");
        assert_eq!(ks[0], TokKind::Int(42));
        assert_eq!(ks[1], TokKind::Float(3.14));
        assert_eq!(ks[2], TokKind::Int(0));
        assert_eq!(ks[3], TokKind::Float(0.5));
    }

    #[test]
    fn strings_with_escapes() {
        let (toks, diags) = tokenize("\"a\\nb\\t\\\"c\\\\d\"");
        assert!(diags.is_empty());
        match &toks[0].kind {
            TokKind::Str(s) => assert_eq!(s, "a\nb\t\"c\\d"),
            k => panic!("expected string, got {:?}", k),
        }
    }

    #[test]
    fn comments_are_skipped() {
        let ks = kinds("// line comment\nlet x; /* block\ncomment */ let y;");
        assert_eq!(
            ks,
            vec![TokKind::Let, TokKind::Ident("x".into()), TokKind::Semi,
                 TokKind::Let, TokKind::Ident("y".into()), TokKind::Semi, TokKind::Eof]
        );
    }

    #[test]
    fn two_char_operators() {
        let ks = kinds("== != <= >= -> += -= = <");
        assert_eq!(
            ks,
            vec![
                TokKind::Eq, TokKind::NotEq, TokKind::Le, TokKind::Ge, TokKind::Arrow,
                TokKind::PlusEq, TokKind::MinusEq, TokKind::Assign, TokKind::Lt, TokKind::Eof,
            ]
        );
    }

    #[test]
    fn positions_track_lines() {
        let (_, diags) = tokenize("");
        assert!(diags.is_empty());
        let (toks, _) = tokenize("let x;\nlet y;");
        assert_eq!(toks[0].line, 1);
        assert_eq!(toks[0].col, 1);
        let second_let = toks.iter().find(|t| t.line == 2).unwrap();
        assert_eq!(second_let.col, 1);
        assert_eq!(second_let.kind, TokKind::Let);
    }

    #[test]
    fn unterminated_string_is_reported() {
        let (_, diags) = tokenize("\"oops");
        assert!(diags.iter().any(|d| d.code == "E1002"));
        let (_, diags) = tokenize("\"oops\n\"");
        assert!(diags.iter().any(|d| d.code == "E1002"));
    }

    #[test]
    fn invalid_escape_is_reported() {
        let (_, diags) = tokenize("\"bad\\q\"");
        assert!(diags.iter().any(|d| d.code == "E1003"));
    }

    #[test]
    fn invalid_characters_are_reported() {
        let (_, diags) = tokenize("let $x;");
        assert!(diags.iter().any(|d| d.code == "E1001" && d.message.contains('$')));
        // `!` alone is invalid, with a helpful message.
        let (_, diags) = tokenize("a ! b");
        assert!(diags.iter().any(|d| d.code == "E1001" && d.message.contains("not")));
    }

    #[test]
    fn malformed_and_overflowing_numbers() {
        let (_, diags) = tokenize("1.");
        assert!(diags.iter().any(|d| d.code == "E1004"));
        let (_, diags) = tokenize("1.2.3");
        assert!(diags.iter().any(|d| d.code == "E1004"));
        let (_, diags) = tokenize("99999999999999999999");
        assert!(diags.iter().any(|d| d.code == "E1005"));
    }

    #[test]
    fn unterminated_block_comment_is_reported() {
        let (_, diags) = tokenize("/* never closed");
        assert!(diags.iter().any(|d| d.code == "E1006"));
    }

    #[test]
    fn star_is_a_token_for_mode_directives() {
        assert_eq!(kinds("*")[0], TokKind::Star);
    }

    #[test]
    fn new_0_2_tokens() {
        let ks = kinds("for in as import");
        assert_eq!(
            ks,
            vec![TokKind::For, TokKind::In, TokKind::As, TokKind::Import, TokKind::Eof]
        );
        let ks = kinds("[ ] . .. ..=");
        assert_eq!(
            ks,
            vec![
                TokKind::LBracket, TokKind::RBracket, TokKind::Dot, TokKind::DotDot,
                TokKind::DotDotEq, TokKind::Eof,
            ]
        );
    }

    #[test]
    fn ranges_are_not_mistaken_for_floats() {
        // `0..10` must lex as INT, DOT_DOT, INT — not a malformed float.
        let ks = kinds("0..10");
        assert_eq!(
            ks,
            vec![TokKind::Int(0), TokKind::DotDot, TokKind::Int(10), TokKind::Eof]
        );
        // `0..=9` inclusive range
        let ks = kinds("0..=9");
        assert_eq!(
            ks,
            vec![TokKind::Int(0), TokKind::DotDotEq, TokKind::Int(9), TokKind::Eof]
        );
        // floats still work, including before a range
        let ks = kinds("1.5..2.5");
        assert_eq!(
            ks,
            vec![
                TokKind::Float(1.5),
                TokKind::DotDot,
                TokKind::Float(2.5),
                TokKind::Eof,
            ]
        );
        // a real malformed number is still an error
        let (_, diags) = tokenize("1.2.3");
        assert!(diags.iter().any(|d| d.code == "E1004"));
    }
}
