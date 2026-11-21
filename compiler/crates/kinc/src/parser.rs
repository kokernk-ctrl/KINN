//! The Kin parser: tokens to AST (recursive descent).
//!
//! Since 0.2.0 the parser recovers from syntax errors: when a statement
//! fails to parse, it records the diagnostic, skips tokens up to the next
//! synchronization point (`;`, `}`, a statement keyword or a top-level
//! `func`/`import`) and continues, so one file can yield many precise
//! diagnostics instead of stopping at the first (P-001).
//!
//! The parse still fails as a whole when any error was recorded: the
//! checker and lowering only run on fully valid syntax.

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::token::{describe, Token, TokKind};

/// Stop collecting after this many syntax errors and skip the rest of the
/// input (E2099). This keeps error cascades from flooding the user.
const MAX_PARSE_ERRORS: usize = 25;

/// Parse a token stream (as produced by [`crate::lexer::tokenize`]).
///
/// Returns the program, or all diagnostics collected (possibly several
/// per file thanks to recovery) when any syntax error was found.
pub fn parse(tokens: Vec<Token>) -> Result<Program, Vec<Diagnostic>> {
    let mut p = Parser { toks: tokens, pos: 0, diags: Vec::new(), had_error: false, gave_up: false };
    let prog = p.program();
    if p.had_error {
        Err(p.diags)
    } else {
        Ok(prog)
    }
}

type PResult<T> = Result<T, ()>;

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
    had_error: bool,
    /// Set when the error cap is hit; all loops stop and skip to EOF.
    gave_up: bool,
}

impl Parser {
    fn peek(&self) -> &TokKind {
        &self.toks[self.pos].kind
    }

    fn peek_token(&self) -> &Token {
        &self.toks[self.pos]
    }

    fn peek2_kind(&self) -> &TokKind {
        static EOF: TokKind = TokKind::Eof;
        match self.toks.get(self.pos + 1) {
            Some(t) => &t.kind,
            None => &EOF,
        }
    }

    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if !matches!(t.kind, TokKind::Eof) {
            self.pos += 1;
        }
        t
    }

    fn at(&self, k: TokKind) -> bool {
        *self.peek() == k
    }

    /// Central diagnostic sink: enforces the error cap.
    fn fail(&mut self, code: &'static str, message: impl Into<String>, line: u32, col: u32, len: u32) {
        if self.gave_up {
            return;
        }
        if self.diags.len() >= MAX_PARSE_ERRORS {
            self.diags.push(Diagnostic::error(
                "E2099",
                format!("too many syntax errors ({}); stopping after the first {}", MAX_PARSE_ERRORS, MAX_PARSE_ERRORS),
                line,
                col,
                1,
            ));
            self.gave_up = true;
            return;
        }
        self.diags.push(Diagnostic::error(code, message, line, col, len));
        self.had_error = true;
    }

    fn error_here(&mut self, message: String) {
        let t = self.peek_token().clone();
        self.fail("E2001", message, t.line, t.col, t.len().max(1));
    }

    fn expect(&mut self, k: TokKind, what: &str) -> PResult<Token> {
        if self.at(k) {
            Ok(self.bump())
        } else {
            let t = self.peek_token().clone();
            self.fail(
                "E2002",
                format!("expected {} but found {}", what, describe(&t.kind)),
                t.line,
                t.col,
                t.len().max(1),
            );
            Err(())
        }
    }

    fn expect_ident(&mut self) -> PResult<Token> {
        if matches!(self.peek(), TokKind::Ident(_)) {
            Ok(self.bump())
        } else {
            let t = self.peek_token().clone();
            self.fail(
                "E2002",
                format!("expected an identifier but found {}", describe(&t.kind)),
                t.line,
                t.col,
                t.len().max(1),
            );
            Err(())
        }
    }

    /// Skip to a statement boundary after a statement-level error:
    /// past `;`, or up to `}`, a statement keyword, a top-level `func` /
    /// `import`, or end of file.
    fn recover_stmt(&mut self) {
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return;
            }
            match self.peek() {
                TokKind::Eof => return,
                TokKind::Semi => {
                    self.bump();
                    return;
                }
                TokKind::RBrace
                | TokKind::Func
                | TokKind::Import
                | TokKind::Let
                | TokKind::Var
                | TokKind::If
                | TokKind::While
                | TokKind::For
                | TokKind::Return
                | TokKind::Break
                | TokKind::Continue
                | TokKind::LBrace => return,
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Skip to the next top-level item (`func`, `import`) after a broken
    /// declaration header.
    fn recover_top(&mut self) {
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return;
            }
            match self.peek() {
                TokKind::Eof | TokKind::Func | TokKind::Import => return,
                TokKind::RBrace | TokKind::Semi => {
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    fn skip_to_eof(&mut self) {
        while !self.at(TokKind::Eof) {
            self.bump();
        }
    }

    // -- program structure ---------------------------------------------------

    fn program(&mut self) -> Program {
        let mut mode = Mode::default();
        let mut mode_declared = false;
        let mut imports = Vec::new();
        let mut funcs = Vec::new();

        if self.gave_up {
            return Program { mode, mode_declared, imports, funcs };
        }

        // Optional mode directive — must be the very first thing.
        if self.at(TokKind::Star) {
            match self.mode_directive() {
                Ok(m) => {
                    mode = m;
                    mode_declared = true;
                }
                Err(()) => {
                    self.recover_top();
                }
            }
        }

        // Imports — before all functions.
        while !self.gave_up && self.at(TokKind::Import) {
            match self.import_decl() {
                Ok(d) => imports.push(d),
                Err(()) => self.recover_top(),
            }
        }

        // Functions.
        while !self.gave_up && !self.at(TokKind::Eof) {
            if self.at(TokKind::Star) {
                self.error_here(
                    "the `*mode` directive must be the first statement of the file".to_string(),
                );
                self.recover_top();
                continue;
            }
            if self.at(TokKind::Import) {
                self.fail(
                    "E2012",
                    "`import` declarations must appear before all functions",
                    self.peek_token().line,
                    self.peek_token().col,
                    6,
                );
                self.recover_top();
                continue;
            }
            match self.func() {
                Ok(f) => funcs.push(f),
                Err(()) => self.recover_top(),
            }
        }

        Program { mode, mode_declared, imports, funcs }
    }

    fn mode_directive(&mut self) -> PResult<Mode> {
        let star = self.bump();
        let name = self.expect_ident()?;
        if !matches!(&name.kind, TokKind::Ident(n) if n == "mode") {
            self.fail(
                "E2010",
                "expected `mode` after `*` in the mode directive",
                star.line,
                star.col,
                1,
            );
            return Err(());
        }
        self.expect(TokKind::Assign, "`=`")?;
        let value = self.expect_ident()?;
        let mode = match &value.kind {
            TokKind::Ident(v) if v == "B" => Mode::B,
            TokKind::Ident(v) if v == "S" => Mode::S,
            TokKind::Ident(v) if v == "A" => Mode::A,
            TokKind::Ident(v) => {
                self.fail(
                    "E2011",
                    format!("invalid mode `{}` (expected `B`, `S` or `A`)", v),
                    value.line,
                    value.col,
                    value.len(),
                );
                return Err(());
            }
            _ => {
                self.fail(
                    "E2011",
                    format!(
                        "invalid mode (expected `B`, `S` or `A`) but found {}",
                        describe(&value.kind)
                    ),
                    value.line,
                    value.col,
                    value.len().max(1),
                );
                return Err(());
            }
        };
        self.expect(TokKind::Semi, "';'")?;
        Ok(mode)
    }

    /// `import "path.kin" as alias;`
    fn import_decl(&mut self) -> PResult<ImportDecl> {
        let kw = self.bump(); // Import
        let path_tok = self.peek_token().clone();
        let path = match path_tok.kind.clone() {
            TokKind::Str(s) => {
                self.bump();
                s
            }
            other => {
                self.fail(
                    "E2015",
                    format!(
                        "expected a string path after `import` (like `import \"lib.kin\" as lib;`) but found {}",
                        describe(&other)
                    ),
                    path_tok.line,
                    path_tok.col,
                    path_tok.len().max(1),
                );
                return Err(());
            }
        };
        self.expect(TokKind::As, "`as`")?;
        let alias = self.expect_ident()?;
        let alias = match alias.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        self.expect(TokKind::Semi, "';'")?;
        Ok(ImportDecl { path, alias, line: kw.line, col: kw.col })
    }

    fn func(&mut self) -> PResult<Func> {
        let func_tok = self.expect(TokKind::Func, "`func`")?;
        let name = self.expect_ident()?;
        let name = match name.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!("expect_ident guarantees an identifier"),
        };
        self.expect(TokKind::LParen, "'('")?;

        let mut params = Vec::new();
        if !self.at(TokKind::RParen) {
            loop {
                let pt = self.expect_ident()?;
                let pname = match pt.kind {
                    TokKind::Ident(n) => n,
                    _ => unreachable!(),
                };
                self.expect(TokKind::Colon, "':'")?;
                let ty = self.parse_type()?;
                params.push(Param { name: pname, ty, line: pt.line, col: pt.col });
                if !self.at(TokKind::Comma) {
                    break;
                }
                self.bump();
            }
        }
        self.expect(TokKind::RParen, "')'")?;

        let ret = if self.at(TokKind::Arrow) {
            self.bump();
            self.parse_type()?
        } else {
            Type::Void
        };

        let body = self.block()?;
        Ok(Func { name, params, ret, body, line: func_tok.line, col: func_tok.col })
    }

    fn parse_type(&mut self) -> PResult<Type> {
        let t = self.peek_token().clone();
        let ty = match t.kind {
            TokKind::IntTy => {
                self.bump();
                Type::Int
            }
            TokKind::FloatTy => {
                self.bump();
                Type::Float
            }
            TokKind::BoolTy => {
                self.bump();
                Type::Bool
            }
            TokKind::StrTy => {
                self.bump();
                Type::Str
            }
            TokKind::VoidTy => {
                self.bump();
                Type::Void
            }
            TokKind::LBracket => {
                self.bump();
                let inner = self.parse_type()?;
                self.expect(TokKind::RBracket, "']'")?;
                Type::Array(Box::new(inner))
            }
            _ => {
                self.fail(
                    "E2003",
                    format!(
                        "expected a type (`int`, `float`, `bool`, `string`, `void` or `[T]`) but found {}",
                        describe(&t.kind)
                    ),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
        };
        Ok(ty)
    }

    // -- statements ----------------------------------------------------------

    fn block(&mut self) -> PResult<Block> {
        self.expect(TokKind::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        loop {
            if self.gave_up {
                self.skip_to_eof();
                return Err(());
            }
            if self.at(TokKind::RBrace) {
                self.bump();
                return Ok(Block { stmts });
            }
            match self.peek() {
                TokKind::Eof => {
                    self.error_here("expected '}' but found end of file".to_string());
                    return Err(());
                }
                TokKind::Func | TokKind::Import => {
                    // A top-level item inside a block: the block was never
                    // closed. Point at it instead of mis-parsing it.
                    self.error_here(
                        "expected `}` before the next top-level declaration".to_string(),
                    );
                    return Err(());
                }
                _ => {}
            }
            match self.stmt() {
                Ok(s) => stmts.push(s),
                Err(()) => self.recover_stmt(),
            }
        }
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        let t = self.peek_token().clone();
        match &t.kind {
            TokKind::Let | TokKind::Var => self.var_decl(),
            TokKind::If => self.if_stmt(),
            TokKind::While => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                Ok(Stmt::While { cond, body, line: t.line, col: t.col })
            }
            TokKind::For => self.for_stmt(),
            TokKind::Return => {
                self.bump();
                let value = if self.at(TokKind::Semi) { None } else { Some(self.expr()?) };
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Return { value, line: t.line, col: t.col })
            }
            TokKind::Break => {
                self.bump();
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Break { line: t.line, col: t.col })
            }
            TokKind::Continue => {
                self.bump();
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Continue { line: t.line, col: t.col })
            }
            TokKind::LBrace => {
                let block = self.block()?;
                Ok(Stmt::Block { block, line: t.line, col: t.col })
            }
            TokKind::Ident(_)
                if matches!(
                    self.peek2_kind(),
                    TokKind::Assign | TokKind::PlusEq | TokKind::MinusEq
                ) =>
            {
                let name = match self.bump().kind {
                    TokKind::Ident(n) => n,
                    _ => unreachable!(),
                };
                let op_tok = self.bump();
                let op = match op_tok.kind {
                    TokKind::Assign => AssignOp::Set,
                    TokKind::PlusEq => AssignOp::Add,
                    TokKind::MinusEq => AssignOp::Sub,
                    _ => unreachable!(),
                };
                let value = self.expr()?;
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Assign { name, op, value, line: t.line, col: t.col })
            }
            _ => {
                // Expression statement, or an assignment whose target is
                // an indexing expression (`a[i] = v;`).
                let e = self.expr()?;
                if matches!(self.peek(), TokKind::Assign | TokKind::PlusEq | TokKind::MinusEq) {
                    let op_tok = self.bump();
                    let value = self.expr()?;
                    self.expect(TokKind::Semi, "';'")?;
                    let line = t.line;
                    let col = t.col;
                    return match (e.kind, op_tok.kind) {
                        (ExprKind::Ident(name), TokKind::Assign) => Ok(Stmt::Assign {
                            name,
                            op: AssignOp::Set,
                            value,
                            line,
                            col,
                        }),
                        (ExprKind::Ident(name), TokKind::PlusEq) => Ok(Stmt::Assign {
                            name,
                            op: AssignOp::Add,
                            value,
                            line,
                            col,
                        }),
                        (ExprKind::Ident(name), TokKind::MinusEq) => Ok(Stmt::Assign {
                            name,
                            op: AssignOp::Sub,
                            value,
                            line,
                            col,
                        }),
                        (ExprKind::Index { base, index }, TokKind::Assign) => {
                            Ok(Stmt::IndexAssign {
                                base: *base,
                                index: *index,
                                value,
                                line,
                                col,
                            })
                        }
                        (ExprKind::Index { .. }, _) => {
                            self.fail(
                                "E2014",
                                "compound assignment (`+=`, `-=`) to an array element is not supported; use `a[i] = a[i] + n;`",
                                line,
                                col,
                                1,
                            );
                            Err(())
                        }
                        _ => {
                            self.fail(
                                "E2013",
                                "the left side of an assignment must be a variable or an array element",
                                line,
                                col,
                                1,
                            );
                            Err(())
                        }
                    };
                }
                self.expect(TokKind::Semi, "';'")?;
                Ok(Stmt::Expr { expr: e, line: t.line, col: t.col })
            }
        }
    }

    fn var_decl(&mut self) -> PResult<Stmt> {
        let kw = self.bump(); // Let or Var
        let mutable = kw.kind == TokKind::Var;
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!(),
        };
        let ty = if self.at(TokKind::Colon) {
            self.bump();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(TokKind::Assign, "`=`")?;
        let init = self.expr()?;
        self.expect(TokKind::Semi, "';'")?;
        Ok(Stmt::VarDecl {
            mutable,
            name,
            ty,
            init,
            line: kw.line,
            col: kw.col,
        })
    }

    fn if_stmt(&mut self) -> PResult<Stmt> {
        let if_tok = self.bump(); // If
        let cond = self.expr()?;
        let then_branch = self.block()?;
        let else_branch = if self.at(TokKind::Else) {
            self.bump();
            if self.at(TokKind::If) {
                Some(Box::new(self.if_stmt()?))
            } else {
                let lbrace = self.peek_token().clone();
                let block = self.block()?;
                Some(Box::new(Stmt::Block {
                    block,
                    line: lbrace.line,
                    col: lbrace.col,
                }))
            }
        } else {
            None
        };
        Ok(Stmt::If { cond, then_branch, else_branch, line: if_tok.line, col: if_tok.col })
    }

    /// `for name in iterable { ... }` where the iterable is a range
    /// (`a..b`, `a..=b`) or an array expression.
    fn for_stmt(&mut self) -> PResult<Stmt> {
        let for_tok = self.bump(); // For
        let name_tok = self.expect_ident()?;
        let name = match name_tok.kind {
            TokKind::Ident(n) => n,
            _ => unreachable!(),
        };
        self.expect(TokKind::In, "`in`")?;
        let iter = self.for_iterable()?;
        let body = self.block()?;
        Ok(Stmt::For { var: name, iter, body, line: for_tok.line, col: for_tok.col })
    }

    fn for_iterable(&mut self) -> PResult<ForIter> {
        let start = self.expr()?;
        if self.at(TokKind::DotDot) {
            self.bump();
            let end = self.expr()?;
            Ok(ForIter::Range { start, end, inclusive: false })
        } else if self.at(TokKind::DotDotEq) {
            self.bump();
            let end = self.expr()?;
            Ok(ForIter::Range { start, end, inclusive: true })
        } else {
            Ok(ForIter::Expr(start))
        }
    }

    // -- expressions ---------------------------------------------------------

    fn expr(&mut self) -> PResult<Expr> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> PResult<Expr> {
        let mut lhs = self.and_expr()?;
        while self.at(TokKind::Or) {
            let op_tok = self.bump();
            let rhs = self.and_expr()?;
            lhs = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::Or,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                line: op_tok.line,
                col: op_tok.col,
            };
        }
        Ok(lhs)
    }

    fn and_expr(&mut self) -> PResult<Expr> {
        let mut lhs = self.cmp_expr()?;
        while self.at(TokKind::And) {
            let op_tok = self.bump();
            let rhs = self.cmp_expr()?;
            lhs = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::And,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                line: op_tok.line,
                col: op_tok.col,
            };
        }
        Ok(lhs)
    }

    fn cmp_expr(&mut self) -> PResult<Expr> {
        let lhs = self.add_expr()?;
        let op = match self.peek() {
            TokKind::Lt => Some(BinOp::Lt),
            TokKind::Gt => Some(BinOp::Gt),
            TokKind::Le => Some(BinOp::Le),
            TokKind::Ge => Some(BinOp::Ge),
            TokKind::Eq => Some(BinOp::Eq),
            TokKind::NotEq => Some(BinOp::Ne),
            _ => None,
        };
        if let Some(op) = op {
            let op_tok = self.bump();
            let rhs = self.add_expr()?;
            if matches!(
                self.peek(),
                TokKind::Lt | TokKind::Gt | TokKind::Le | TokKind::Ge | TokKind::Eq | TokKind::NotEq
            ) {
                let t = self.peek_token().clone();
                self.fail(
                    "E2004",
                    "chained comparisons are not allowed; use parentheses or boolean operators",
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
            return Ok(Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: op_tok.line,
                col: op_tok.col,
            });
        }
        Ok(lhs)
    }

    fn add_expr(&mut self) -> PResult<Expr> {
        let mut lhs = self.mul_expr()?;
        loop {
            let op = match self.peek() {
                TokKind::Plus => BinOp::Add,
                TokKind::Minus => BinOp::Sub,
                _ => break,
            };
            let op_tok = self.bump();
            let rhs = self.mul_expr()?;
            lhs = Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: op_tok.line,
                col: op_tok.col,
            };
        }
        Ok(lhs)
    }

    fn mul_expr(&mut self) -> PResult<Expr> {
        let mut lhs = self.unary_expr()?;
        loop {
            let op = match self.peek() {
                TokKind::Star => BinOp::Mul,
                TokKind::Slash => BinOp::Div,
                TokKind::Percent => BinOp::Mod,
                _ => break,
            };
            let op_tok = self.bump();
            let rhs = self.unary_expr()?;
            lhs = Expr {
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                line: op_tok.line,
                col: op_tok.col,
            };
        }
        Ok(lhs)
    }

    fn unary_expr(&mut self) -> PResult<Expr> {
        let t = self.peek_token().clone();
        match t.kind {
            TokKind::Not => {
                self.bump();
                let inner = self.unary_expr()?;
                Ok(Expr {
                    kind: ExprKind::Unary { op: UnOp::Not, expr: Box::new(inner) },
                    line: t.line,
                    col: t.col,
                })
            }
            TokKind::Minus => {
                self.bump();
                let inner = self.unary_expr()?;
                Ok(Expr {
                    kind: ExprKind::Unary { op: UnOp::Neg, expr: Box::new(inner) },
                    line: t.line,
                    col: t.col,
                })
            }
            _ => self.cast_expr(),
        }
    }

    /// `expr as type` — binds tighter than binary operators, looser than
    /// unary (`-x as int` parses as `-(x as int)`, like Rust).
    fn cast_expr(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        while self.at(TokKind::As) {
            let as_tok = self.bump();
            let ty = self.parse_type()?;
            e = Expr {
                kind: ExprKind::Cast { expr: Box::new(e), ty },
                line: as_tok.line,
                col: as_tok.col,
            };
        }
        Ok(e)
    }

    fn primary(&mut self) -> PResult<Expr> {
        let t = self.peek_token().clone();
        let mut e = match t.kind {
            TokKind::Int(v) => {
                self.bump();
                Expr { kind: ExprKind::IntLit(v), line: t.line, col: t.col }
            }
            TokKind::Float(v) => {
                self.bump();
                Expr { kind: ExprKind::FloatLit(v), line: t.line, col: t.col }
            }
            TokKind::Str(v) => {
                self.bump();
                Expr { kind: ExprKind::StrLit(v), line: t.line, col: t.col }
            }
            TokKind::True => {
                self.bump();
                Expr { kind: ExprKind::BoolLit(true), line: t.line, col: t.col }
            }
            TokKind::False => {
                self.bump();
                Expr { kind: ExprKind::BoolLit(false), line: t.line, col: t.col }
            }
            TokKind::LBracket => {
                self.bump();
                let mut items = Vec::new();
                if !self.at(TokKind::RBracket) {
                    loop {
                        items.push(self.expr()?);
                        if !self.at(TokKind::Comma) {
                            break;
                        }
                        self.bump();
                    }
                }
                self.expect(TokKind::RBracket, "']'")?;
                Expr { kind: ExprKind::ArrayLit(items), line: t.line, col: t.col }
            }
            TokKind::Ident(name) => {
                self.bump();
                if self.at(TokKind::LParen) {
                    self.bump();
                    let mut args = Vec::new();
                    if !self.at(TokKind::RParen) {
                        loop {
                            args.push(self.expr()?);
                            if !self.at(TokKind::Comma) {
                                break;
                            }
                            self.bump();
                        }
                    }
                    self.expect(TokKind::RParen, "')'")?;
                    Expr { kind: ExprKind::Call { name, args }, line: t.line, col: t.col }
                } else if self.at(TokKind::Dot) {
                    // Qualified call: `module.function(...)`.
                    self.bump();
                    let f = self.expect_ident()?;
                    let fname = match f.kind {
                        TokKind::Ident(n) => n,
                        _ => unreachable!("expect_ident guarantees an identifier"),
                    };
                    self.expect(TokKind::LParen, "`(` after `module.function`")?;
                    let mut args = Vec::new();
                    if !self.at(TokKind::RParen) {
                        loop {
                            args.push(self.expr()?);
                            if !self.at(TokKind::Comma) {
                                break;
                            }
                            self.bump();
                        }
                    }
                    self.expect(TokKind::RParen, "')'")?;
                    Expr {
                        kind: ExprKind::ModCall { module: name, name: fname, args },
                        line: t.line,
                        col: t.col,
                    }
                } else {
                    Expr { kind: ExprKind::Ident(name), line: t.line, col: t.col }
                }
            }
            TokKind::LParen => {
                self.bump();
                let e = self.expr()?;
                self.expect(TokKind::RParen, "')'")?;
                e
            }
            _ => {
                self.fail(
                    "E2001",
                    format!("expected an expression but found {}", describe(&t.kind)),
                    t.line,
                    t.col,
                    t.len().max(1),
                );
                return Err(());
            }
        };

        // Postfix indexing: `a[0]`, `m.f(x)[i][j]`, ...
        while self.at(TokKind::LBracket) {
            self.bump();
            let idx = self.expr()?;
            self.expect(TokKind::RBracket, "']'")?;
            e = Expr {
                kind: ExprKind::Index { base: Box::new(e), index: Box::new(idx) },
                line: t.line,
                col: t.col,
            };
        }

        Ok(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Program {
        let (toks, diags) = crate::lexer::tokenize(src);
        assert!(diags.is_empty(), "lexer errors: {:?}", diags);
        parse(toks).expect("parse failed")
    }

    fn parse_err(src: &str) -> Vec<Diagnostic> {
        let (toks, _) = crate::lexer::tokenize(src);
        parse(toks).expect_err("expected a parse error")
    }

    #[test]
    fn parses_mode_directive() {
        let p = parse_ok("*mode = B;\nfunc main() { }\n");
        assert_eq!(p.mode, Mode::B);
        assert!(p.mode_declared);
        assert_eq!(p.funcs.len(), 1);
    }

    #[test]
    fn mode_defaults_to_s() {
        let p = parse_ok("func main() { }");
        assert_eq!(p.mode, Mode::S);
        assert!(!p.mode_declared);
    }

    #[test]
    fn invalid_mode_is_rejected() {
        let diags = parse_err("*mode = Z;\nfunc main() { }");
        assert!(diags.iter().any(|d| d.code == "E2011"));
    }

    #[test]
    fn mode_directive_only_allowed_first() {
        let diags = parse_err("func main() { }\n*mode = B;");
        assert!(diags.iter().any(|d| d.code == "E2001"));
    }

    #[test]
    fn parses_function_signature() {
        let p = parse_ok("func add(a: int, b: int) -> int { return a + b; }");
        let f = &p.funcs[0];
        assert_eq!(f.name, "add");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].name, "a");
        assert_eq!(f.params[0].ty, Type::Int);
        assert_eq!(f.ret, Type::Int);
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let p = parse_ok("func main() { print(1 + 2 * 3); }");
        let stmt = &p.funcs[0].body.stmts[0];
        match stmt {
            Stmt::Expr { expr, .. } => match &expr.kind {
                ExprKind::Call { args, .. } => match &args[0].kind {
                    ExprKind::Binary { op, lhs, rhs } => {
                        assert_eq!(*op, BinOp::Add);
                        assert!(matches!(&lhs.kind, ExprKind::IntLit(1)));
                        assert!(matches!(
                            &rhs.kind,
                            ExprKind::Binary { op: BinOp::Mul, .. }
                        ));
                    }
                    k => panic!("expected binary, got {:?}", k),
                },
                k => panic!("expected call, got {:?}", k),
            },
            s => panic!("expected expr stmt, got {:?}", s),
        }
    }

    #[test]
    fn parses_else_if_chain() {
        let p = parse_ok("func f(n: int) -> int { if n < 0 { return 0; } else if n > 9 { return 9; } else { return n; } }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::If { else_branch, .. } => {
                // else must be an If (the `else if`), which itself has a Block else.
                match else_branch.as_deref() {
                    Some(Stmt::If { else_branch: inner_else, .. }) => {
                        assert!(matches!(inner_else.as_deref(), Some(Stmt::Block { .. })));
                    }
                    other => panic!("expected nested if, got {:?}", other),
                }
            }
            s => panic!("expected if, got {:?}", s),
        }
    }

    #[test]
    fn parses_assignment_and_compound_assignment() {
        let p = parse_ok("func main() { var x = 1; x = 2; x += 3; x -= 1; }");
        let stmts = &p.funcs[0].body.stmts;
        assert!(matches!(&stmts[1], Stmt::Assign { op: AssignOp::Set, name, .. } if name == "x"));
        assert!(matches!(&stmts[2], Stmt::Assign { op: AssignOp::Add, .. }));
        assert!(matches!(&stmts[3], Stmt::Assign { op: AssignOp::Sub, .. }));
    }

    #[test]
    fn parses_unary_operators() {
        let p = parse_ok("func main() { print(not true); print(-5); }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::Expr { expr, .. } => {
                assert!(matches!(&expr.kind, ExprKind::Call { name, .. } if name == "print"));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
        // -5 is unary minus applied to 5
        match &p.funcs[0].body.stmts[1] {
            Stmt::Expr { expr, .. } => match &expr.kind {
                ExprKind::Call { args, .. } => {
                    assert!(matches!(&args[0].kind, ExprKind::Unary { op: UnOp::Neg, .. }));
                }
                k => panic!("expected call, got {:?}", k),
            },
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn missing_semicolon_is_an_error() {
        let diags = parse_err("func main() { let x = 1 print(x); }");
        assert!(diags.iter().any(|d| d.code == "E2002" && d.message.contains(';')));
    }

    #[test]
    fn missing_brace_is_an_error() {
        let diags = parse_err("func main() { let x = 1;");
        assert!(!diags.is_empty());
    }

    #[test]
    fn chained_comparison_is_rejected() {
        let diags = parse_err("func main() { print(1 < 2 < 3); }");
        assert!(diags.iter().any(|d| d.code == "E2004"));
    }

    #[test]
    fn unknown_type_is_rejected() {
        let diags = parse_err("func main() { let x: number = 1; }");
        assert!(diags.iter().any(|d| d.code == "E2003"));
    }

    #[test]
    fn bare_blocks_are_statements() {
        let p = parse_ok("func main() { let x = 1; { let y = 2; print(y); } print(x); }");
        assert_eq!(p.funcs[0].body.stmts.len(), 3);
        assert!(matches!(&p.funcs[0].body.stmts[1], Stmt::Block { .. }));
    }

    // -- 0.2.0 syntax --------------------------------------------------------

    #[test]
    fn parses_array_types() {
        let p = parse_ok("func f(xs: [int]) -> [[int]] { return [[1]]; }");
        assert_eq!(p.funcs[0].params[0].ty, Type::Array(Box::new(Type::Int)));
        assert_eq!(
            p.funcs[0].ret,
            Type::Array(Box::new(Type::Array(Box::new(Type::Int))))
        );
    }

    #[test]
    fn parses_array_literals_and_indexing() {
        let p = parse_ok("func main() { let a = [1, 2, 3]; print(a[0] + a[2]); }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::VarDecl { init, .. } => {
                assert!(matches!(&init.kind, ExprKind::ArrayLit(items) if items.len() == 3));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
        match &p.funcs[0].body.stmts[1] {
            Stmt::Expr { expr, .. } => match &expr.kind {
                ExprKind::Call { args, .. } => {
                    assert!(matches!(&args[0].kind, ExprKind::Binary { op: BinOp::Add, .. }));
                }
                k => panic!("expected call, got {:?}", k),
            },
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_empty_array_literal() {
        let p = parse_ok("func main() { let a: [int] = []; }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::VarDecl { init, .. } => {
                assert!(matches!(&init.kind, ExprKind::ArrayLit(items) if items.is_empty()));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_nested_indexing_and_index_assignment() {
        let p = parse_ok("func main() { let m = [[1]]; print(m[0][0]); m[0][0] = 5; }");
        let stmts = &p.funcs[0].body.stmts;
        match &stmts[2] {
            Stmt::IndexAssign { .. } => {}
            s => panic!("expected IndexAssign, got {:?}", s),
        }
    }

    #[test]
    fn compound_index_assignment_is_rejected() {
        let diags = parse_err("func main() { let a = [1]; a[0] += 1; }");
        assert!(diags.iter().any(|d| d.code == "E2014"));
    }

    #[test]
    fn invalid_assignment_target_is_rejected() {
        let diags = parse_err("func main() { f(1) = 2; }");
        assert!(diags.iter().any(|d| d.code == "E2013"));
    }

    #[test]
    fn parses_for_over_exclusive_and_inclusive_ranges() {
        let p = parse_ok("func main() { for i in 0..5 { print(i); } for i in 0..=5 { print(i); } }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::For { var, iter, .. } => {
                assert_eq!(var, "i");
                assert!(matches!(iter, ForIter::Range { inclusive: false, .. }));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
        match &p.funcs[0].body.stmts[1] {
            Stmt::For { iter, .. } => {
                assert!(matches!(iter, ForIter::Range { inclusive: true, .. }));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_for_over_array() {
        let p = parse_ok("func main() { let xs = [1, 2]; for x in xs { print(x); } }");
        match &p.funcs[0].body.stmts[1] {
            Stmt::For { var, iter, .. } => {
                assert_eq!(var, "x");
                assert!(matches!(iter, ForIter::Expr(_)));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_casts() {
        let p = parse_ok("func main() { let f = 2 as float; let s = 3 as string; }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::VarDecl { init, .. } => {
                assert!(matches!(&init.kind, ExprKind::Cast { ty: Type::Float, .. }));
            }
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn parses_imports() {
        let p = parse_ok("import \"math.kin\" as math;\nfunc main() { }");
        assert_eq!(p.imports.len(), 1);
        assert_eq!(p.imports[0].path, "math.kin");
        assert_eq!(p.imports[0].alias, "math");
    }

    #[test]
    fn import_after_functions_is_rejected() {
        let diags = parse_err("func main() { }\nimport \"math.kin\" as math;");
        assert!(diags.iter().any(|d| d.code == "E2012"));
    }

    #[test]
    fn import_without_alias_is_rejected() {
        let diags = parse_err("import \"math.kin\";\nfunc main() { }");
        assert!(diags.iter().any(|d| d.code == "E2002" && d.message.contains("`as`")));
    }

    #[test]
    fn import_with_non_string_path_is_rejected() {
        let diags = parse_err("import math.kin as math;\nfunc main() { }");
        assert!(diags.iter().any(|d| d.code == "E2015"));
    }

    #[test]
    fn parses_qualified_calls() {
        let p = parse_ok("import \"math.kin\" as math;\nfunc main() { print(math.add(1, 2)); }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::Expr { expr, .. } => match &expr.kind {
                ExprKind::Call { args, .. } => match &args[0].kind {
                    ExprKind::ModCall { module, name, args } => {
                        assert_eq!(module, "math");
                        assert_eq!(name, "add");
                        assert_eq!(args.len(), 2);
                    }
                    k => panic!("expected ModCall, got {:?}", k),
                },
                k => panic!("expected call, got {:?}", k),
            },
            s => panic!("unexpected stmt {:?}", s),
        }
    }

    #[test]
    fn qualified_call_requires_parens() {
        let diags = parse_err("func main() { math.add; }");
        // `math.add;` → after `.` we require an identifier and `(`.
        assert!(diags.iter().any(|d| d.is_error()));
    }

    // -- recovery (P-001) ----------------------------------------------------

    #[test]
    fn multiple_syntax_errors_are_all_reported() {
        let diags = parse_err(
            "func main() {\n    let x = ;\n    let y = ;\n    let z = ;\n}\n",
        );
        let errs = diags.iter().filter(|d| d.is_error()).count();
        assert_eq!(errs, 3, "expected 3 separate diagnostics, got: {:#?}", diags);
        // every one points at its own line
        let lines: Vec<u32> = diags.iter().map(|d| d.line).collect();
        assert!(lines.contains(&2) && lines.contains(&3) && lines.contains(&4));
    }

    #[test]
    fn recovery_does_not_cascade_into_following_statements() {
        // One broken statement; the rest must parse fine and produce
        // exactly one diagnostic.
        let diags = parse_err(
            "func main() {\n    let x = ;\n    let y = 2;\n    print(y);\n}\n",
        );
        assert_eq!(diags.len(), 1, "expected exactly one diagnostic, got: {:#?}", diags);
    }

    #[test]
    fn recovery_reports_errors_in_multiple_functions() {
        let diags = parse_err("func a( { }\nfunc main( { }");
        assert!(diags.iter().filter(|d| d.is_error()).count() >= 2);
    }

    #[test]
    fn unclosed_block_before_next_func_is_reported_cleanly() {
        let diags = parse_err("func a() {\n    let x = 1;\nfunc main() { }");
        assert!(diags.iter().any(|d| d.message.contains("}")));
        // The second function header itself is not the error.
        assert!(!diags.iter().any(|d| d.message.contains("expected `func`")));
    }

    #[test]
    fn error_cap_kicks_in() {
        let mut src = String::from("func main() {\n");
        for _ in 0..40 {
            src.push_str("    let x = ;\n");
        }
        src.push_str("}\n");
        let diags = parse_err(&src);
        assert!(diags.iter().any(|d| d.code == "E2099"));
        assert!(diags.len() <= 26);
    }

    #[test]
    fn semicolon_sync_recovers_within_one_line() {
        // Two errors on different statements; second is still reported.
        let diags = parse_err("func main() { let 1 = 2; let 3 = 4; }");
        assert!(diags.iter().filter(|d| d.is_error()).count() >= 2);
    }
}
