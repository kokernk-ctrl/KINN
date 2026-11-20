//! The Kin parser: tokens to AST (recursive descent).
//!
//! The 0.1 parser stops at the first syntax error. Multi-error recovery
//! is planned for 0.2 (see PROBLEM_REGISTRY P-001).

use crate::ast::*;
use crate::diagnostics::Diagnostic;
use crate::token::{describe, Token, TokKind};

/// Parse a token stream (as produced by [`crate::lexer::tokenize`]).
///
/// Returns the program, or all diagnostics collected up to the first
/// error.
pub fn parse(tokens: Vec<Token>) -> Result<Program, Vec<Diagnostic>> {
    let mut p = Parser { toks: tokens, pos: 0, diags: Vec::new() };
    match p.program() {
        Ok(prog) => Ok(prog),
        Err(()) => Err(p.diags),
    }
}

type PResult<T> = Result<T, ()>;

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
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

    fn error_here(&mut self, message: String) {
        let t = self.peek_token().clone();
        self.diags.push(Diagnostic::error(
            "E2001",
            message,
            t.line,
            t.col,
            t.len().max(1),
        ));
    }

    fn expect(&mut self, k: TokKind, what: &str) -> PResult<Token> {
        if self.at(k) {
            Ok(self.bump())
        } else {
            let t = self.peek_token().clone();
            self.diags.push(Diagnostic::error(
                "E2002",
                format!("expected {} but found {}", what, describe(&t.kind)),
                t.line,
                t.col,
                t.len().max(1),
            ));
            Err(())
        }
    }

    fn expect_ident(&mut self) -> PResult<Token> {
        if matches!(self.peek(), TokKind::Ident(_)) {
            Ok(self.bump())
        } else {
            let t = self.peek_token().clone();
            self.diags.push(Diagnostic::error(
                "E2002",
                format!("expected an identifier but found {}", describe(&t.kind)),
                t.line,
                t.col,
                t.len().max(1),
            ));
            Err(())
        }
    }

    // -- program structure ---------------------------------------------------

    fn program(&mut self) -> PResult<Program> {
        let mut mode = Mode::default();
        let mut mode_declared = false;

        if self.at(TokKind::Star) {
            let star = self.bump();
            let name = self.expect_ident()?;
            if !matches!(&name.kind, TokKind::Ident(n) if n == "mode") {
                self.diags.push(Diagnostic::error(
                    "E2010",
                    "expected `mode` after `*` in the mode directive",
                    star.line,
                    star.col,
                    1,
                ));
                return Err(());
            }
            self.expect(TokKind::Assign, "`=`")?;
            let value = self.expect_ident()?;
            mode = match &value.kind {
                TokKind::Ident(v) if v == "B" => Mode::B,
                TokKind::Ident(v) if v == "S" => Mode::S,
                TokKind::Ident(v) if v == "A" => Mode::A,
                TokKind::Ident(v) => {
                    self.diags.push(Diagnostic::error(
                        "E2011",
                        format!("invalid mode `{}` (expected `B`, `S` or `A`)", v),
                        value.line,
                        value.col,
                        value.len(),
                    ));
                    return Err(());
                }
                _ => {
                    self.diags.push(Diagnostic::error(
                        "E2011",
                        format!(
                            "invalid mode (expected `B`, `S` or `A`) but found {}",
                            describe(&value.kind)
                        ),
                        value.line,
                        value.col,
                        value.len().max(1),
                    ));
                    return Err(());
                }
            };
            mode_declared = true;
            self.expect(TokKind::Semi, "';'")?;
        }

        let mut funcs = Vec::new();
        while !self.at(TokKind::Eof) {
            if self.at(TokKind::Star) {
                self.error_here(
                    "the `*mode` directive must be the first statement of the file".to_string(),
                );
                return Err(());
            }
            funcs.push(self.func()?);
        }

        Ok(Program { mode, mode_declared, funcs })
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
            TokKind::IntTy => Type::Int,
            TokKind::FloatTy => Type::Float,
            TokKind::BoolTy => Type::Bool,
            TokKind::StrTy => Type::Str,
            TokKind::VoidTy => Type::Void,
            _ => {
                self.diags.push(Diagnostic::error(
                    "E2003",
                    format!(
                        "expected a type (`int`, `float`, `bool`, `string` or `void`) but found {}",
                        describe(&t.kind)
                    ),
                    t.line,
                    t.col,
                    t.len().max(1),
                ));
                return Err(());
            }
        };
        self.bump();
        Ok(ty)
    }

    // -- statements ----------------------------------------------------------

    fn block(&mut self) -> PResult<Block> {
        self.expect(TokKind::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        while !self.at(TokKind::RBrace) {
            if self.at(TokKind::Eof) {
                self.error_here("expected '}' but found end of file".to_string());
                return Err(());
            }
            stmts.push(self.stmt()?);
        }
        self.expect(TokKind::RBrace, "'}'")?;
        Ok(Block { stmts })
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
                let e = self.expr()?;
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
                self.diags.push(Diagnostic::error(
                    "E2004",
                    "chained comparisons are not allowed; use parentheses or boolean operators",
                    t.line,
                    t.col,
                    t.len().max(1),
                ));
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
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> PResult<Expr> {
        let t = self.peek_token().clone();
        match t.kind {
            TokKind::Int(v) => {
                self.bump();
                Ok(Expr { kind: ExprKind::IntLit(v), line: t.line, col: t.col })
            }
            TokKind::Float(v) => {
                self.bump();
                Ok(Expr { kind: ExprKind::FloatLit(v), line: t.line, col: t.col })
            }
            TokKind::Str(v) => {
                self.bump();
                Ok(Expr { kind: ExprKind::StrLit(v), line: t.line, col: t.col })
            }
            TokKind::True => {
                self.bump();
                Ok(Expr { kind: ExprKind::BoolLit(true), line: t.line, col: t.col })
            }
            TokKind::False => {
                self.bump();
                Ok(Expr { kind: ExprKind::BoolLit(false), line: t.line, col: t.col })
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
                    Ok(Expr { kind: ExprKind::Call { name, args }, line: t.line, col: t.col })
                } else {
                    Ok(Expr { kind: ExprKind::Ident(name), line: t.line, col: t.col })
                }
            }
            TokKind::LParen => {
                self.bump();
                let e = self.expr()?;
                self.expect(TokKind::RParen, "')'")?;
                Ok(e)
            }
            _ => {
                self.diags.push(Diagnostic::error(
                    "E2001",
                    format!("expected an expression but found {}", describe(&t.kind)),
                    t.line,
                    t.col,
                    t.len().max(1),
                ));
                Err(())
            }
        }
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
}
