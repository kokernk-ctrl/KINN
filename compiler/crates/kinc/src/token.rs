//! Token model shared by the lexer, parser and diagnostics.

/// A lexical token kind.
#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    // Literals and identifiers
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),

    // Keywords
    Func,
    Let,
    Var,
    If,
    Else,
    While,
    Return,
    Break,
    Continue,
    True,
    False,
    And,
    Or,
    Not,

    // Type keywords
    IntTy,
    FloatTy,
    BoolTy,
    StrTy,
    VoidTy,

    // Operators and punctuation
    Star,
    Plus,
    Minus,
    Slash,
    Percent,
    Assign,
    PlusEq,
    MinusEq,
    Eq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    Arrow,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Semi,

    // End of input
    Eof,
}

/// A token with its raw text and 1-based position.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokKind,
    /// Raw source text (used for caret width and debug output).
    pub text: String,
    pub line: u32,
    pub col: u32,
}

impl Token {
    pub fn len(&self) -> u32 {
        self.text.chars().count() as u32
    }
}

/// Stable machine-readable name for a token kind (`kinn tokens` output).
pub fn kind_name(kind: &TokKind) -> &'static str {
    match kind {
        TokKind::Int(_) => "INT",
        TokKind::Float(_) => "FLOAT",
        TokKind::Str(_) => "STR",
        TokKind::Ident(_) => "IDENT",
        TokKind::Func => "FUNC",
        TokKind::Let => "LET",
        TokKind::Var => "VAR",
        TokKind::If => "IF",
        TokKind::Else => "ELSE",
        TokKind::While => "WHILE",
        TokKind::Return => "RETURN",
        TokKind::Break => "BREAK",
        TokKind::Continue => "CONTINUE",
        TokKind::True => "TRUE",
        TokKind::False => "FALSE",
        TokKind::And => "AND",
        TokKind::Or => "OR",
        TokKind::Not => "NOT",
        TokKind::IntTy => "INT_TY",
        TokKind::FloatTy => "FLOAT_TY",
        TokKind::BoolTy => "BOOL_TY",
        TokKind::StrTy => "STRING_TY",
        TokKind::VoidTy => "VOID_TY",
        TokKind::Star => "STAR",
        TokKind::Plus => "PLUS",
        TokKind::Minus => "MINUS",
        TokKind::Slash => "SLASH",
        TokKind::Percent => "PERCENT",
        TokKind::Assign => "ASSIGN",
        TokKind::PlusEq => "PLUS_EQ",
        TokKind::MinusEq => "MINUS_EQ",
        TokKind::Eq => "EQ_EQ",
        TokKind::NotEq => "NOT_EQ",
        TokKind::Lt => "LT",
        TokKind::Gt => "GT",
        TokKind::Le => "LE",
        TokKind::Ge => "GE",
        TokKind::Arrow => "ARROW",
        TokKind::LParen => "LPAREN",
        TokKind::RParen => "RPAREN",
        TokKind::LBrace => "LBRACE",
        TokKind::RBrace => "RBRACE",
        TokKind::Comma => "COMMA",
        TokKind::Colon => "COLON",
        TokKind::Semi => "SEMI",
        TokKind::Eof => "EOF",
    }
}

/// Human-readable description used in "expected X but found Y" messages.
pub fn describe(kind: &TokKind) -> String {
    match kind {
        TokKind::Int(v) => format!("integer `{}`", v),
        TokKind::Float(v) => format!("float `{}`", v),
        TokKind::Str(_) => "string literal".to_string(),
        TokKind::Ident(n) => format!("identifier `{}`", n),
        TokKind::Func => "keyword `func`".to_string(),
        TokKind::Let => "keyword `let`".to_string(),
        TokKind::Var => "keyword `var`".to_string(),
        TokKind::If => "keyword `if`".to_string(),
        TokKind::Else => "keyword `else`".to_string(),
        TokKind::While => "keyword `while`".to_string(),
        TokKind::Return => "keyword `return`".to_string(),
        TokKind::Break => "keyword `break`".to_string(),
        TokKind::Continue => "keyword `continue`".to_string(),
        TokKind::True => "keyword `true`".to_string(),
        TokKind::False => "keyword `false`".to_string(),
        TokKind::And => "keyword `and`".to_string(),
        TokKind::Or => "keyword `or`".to_string(),
        TokKind::Not => "keyword `not`".to_string(),
        TokKind::IntTy => "type `int`".to_string(),
        TokKind::FloatTy => "type `float`".to_string(),
        TokKind::BoolTy => "type `bool`".to_string(),
        TokKind::StrTy => "type `string`".to_string(),
        TokKind::VoidTy => "type `void`".to_string(),
        TokKind::Star => "`*`".to_string(),
        TokKind::Plus => "`+`".to_string(),
        TokKind::Minus => "`-`".to_string(),
        TokKind::Slash => "`/`".to_string(),
        TokKind::Percent => "`%`".to_string(),
        TokKind::Assign => "`=`".to_string(),
        TokKind::PlusEq => "`+=`".to_string(),
        TokKind::MinusEq => "`-=`".to_string(),
        TokKind::Eq => "`==`".to_string(),
        TokKind::NotEq => "`!=`".to_string(),
        TokKind::Lt => "`<`".to_string(),
        TokKind::Gt => "`>`".to_string(),
        TokKind::Le => "`<=`".to_string(),
        TokKind::Ge => "`>=`".to_string(),
        TokKind::Arrow => "`->`".to_string(),
        TokKind::LParen => "`(`".to_string(),
        TokKind::RParen => "`)`".to_string(),
        TokKind::LBrace => "`{`".to_string(),
        TokKind::RBrace => "`}`".to_string(),
        TokKind::Comma => "`,`".to_string(),
        TokKind::Colon => "`:`".to_string(),
        TokKind::Semi => "`;`".to_string(),
        TokKind::Eof => "end of file".to_string(),
    }
}
