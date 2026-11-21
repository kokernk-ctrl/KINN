//! Compile-time constant evaluation for `const` declarations (0.4.0 S1).
//!
//! A `const NAME: Type = expr;` initializer must be a **compile-time
//! constant expression**: literals, array literals, unary and binary
//! operators, and references to consts declared *earlier in the same
//! file*. Function calls, casts, indexing and qualified (`module.NAME`)
//! reads are rejected. Arithmetic uses the same rules as the KIR VM:
//! integer overflow and division/modulo by zero are errors (ADR-0008),
//! and float division by zero is an error too — a constant expression can
//! never smuggle in behavior the VM would reject at runtime.
//!
//! The evaluator is a pure function over the AST, shared by the checker
//! (validation, E3039) and the lowering pass (folding into
//! `Instr::Const(value)`).

use crate::ast::{BinOp, Expr, ExprKind, Type, UnOp};
use kinrt::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Why an expression is not an acceptable constant initializer.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstError {
    /// The expression uses a construct that is not a compile-time
    /// constant (call, cast, variable, indexing, ...).
    NotConstant(String),
    /// The expression is a constant but its value is erroneous
    /// (integer overflow, division/modulo by zero).
    Arithmetic(String),
}

/// Evaluate `e` as a constant expression. `consts` maps the names of
/// earlier consts of the same file to their folded values and declared
/// types. `declared` is the annotation of the const being evaluated; it
/// lets an empty array literal (`[]`) take the declared element type,
/// exactly like `let a: [int] = [];` does.
pub fn eval_const_expr(
    e: &Expr,
    consts: &HashMap<String, (Value, Type)>,
    declared: Option<&Type>,
) -> Result<(Value, Type), ConstError> {
    match &e.kind {
        ExprKind::IntLit(v) => Ok((Value::Int(*v), Type::Int)),
        ExprKind::FloatLit(v) => Ok((Value::Float(*v), Type::Float)),
        ExprKind::BoolLit(v) => Ok((Value::Bool(*v), Type::Bool)),
        ExprKind::StrLit(v) => Ok((Value::Str(v.clone()), Type::Str)),
        ExprKind::ArrayLit(items) => {
            if items.is_empty() {
                return match declared {
                    Some(t @ Type::Array(_)) => {
                        Ok((Value::Array(Rc::new(RefCell::new(Vec::new()))), t.clone()))
                    }
                    _ => Err(ConstError::NotConstant(
                        "cannot infer the element type of an empty array literal".to_string(),
                    )),
                };
            }
            let mut elems = Vec::with_capacity(items.len());
            let mut elem_ty: Option<Type> = None;
            for item in items {
                let (v, t) = eval_const_expr(item, consts, None)?;
                match &elem_ty {
                    None => elem_ty = Some(t),
                    Some(expected) => {
                        if t != *expected {
                            return Err(ConstError::NotConstant(format!(
                                "array literal elements must all have the same type (expected {}, found {})",
                                expected.as_str(),
                                t.as_str()
                            )));
                        }
                    }
                }
                elems.push(v);
            }
            let ty = elem_ty.expect("non-empty array has an element type");
            Ok((
                Value::Array(Rc::new(RefCell::new(elems))),
                Type::Array(Box::new(ty)),
            ))
        }
        ExprKind::Ident(name) => match consts.get(name) {
            Some((v, t)) => Ok((v.clone(), t.clone())),
            None => Err(ConstError::NotConstant(format!(
                "`{}` is not a constant (const initializers may reference only consts declared earlier in the same file)",
                name
            ))),
        },
        ExprKind::Unary { op, expr } => {
            let (v, _t) = eval_const_expr(expr, consts, None)?;
            match (op, v) {
                (UnOp::Not, Value::Bool(b)) => Ok((Value::Bool(!b), Type::Bool)),
                (UnOp::Neg, Value::Int(i)) => match i.checked_neg() {
                    Some(r) => Ok((Value::Int(r), Type::Int)),
                    None => Err(ConstError::Arithmetic(
                        "constant expression overflows the int range".to_string(),
                    )),
                },
                (UnOp::Neg, Value::Float(f)) => Ok((Value::Float(-f), Type::Float)),
                (op, v) => Err(ConstError::NotConstant(format!(
                    "unary `{}` requires {}, found {}",
                    op,
                    if matches!(op, UnOp::Not) { "bool" } else { "a number" },
                    v.type_name()
                ))),
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let (lv, lt) = eval_const_expr(lhs, consts, None)?;
            let (rv, rt) = eval_const_expr(rhs, consts, None)?;
            use BinOp::*;
            match op {
                Add => match (lv, rv) {
                    (Value::Int(a), Value::Int(b)) => int_op(a, checked_add, b, Type::Int),
                    (Value::Float(a), Value::Float(b)) => Ok((Value::Float(a + b), Type::Float)),
                    (Value::Str(a), Value::Str(b)) => {
                        Ok((Value::Str(format!("{}{}", a, b)), Type::Str))
                    }
                    _ => Err(type_mismatch(*op, &lt, &rt)),
                },
                Sub => match (lv, rv) {
                    (Value::Int(a), Value::Int(b)) => int_op(a, checked_sub, b, Type::Int),
                    (Value::Float(a), Value::Float(b)) => Ok((Value::Float(a - b), Type::Float)),
                    _ => Err(type_mismatch(*op, &lt, &rt)),
                },
                Mul => match (lv, rv) {
                    (Value::Int(a), Value::Int(b)) => int_op(a, checked_mul, b, Type::Int),
                    (Value::Float(a), Value::Float(b)) => Ok((Value::Float(a * b), Type::Float)),
                    _ => Err(type_mismatch(*op, &lt, &rt)),
                },
                Div => match (lv, rv) {
                    (Value::Int(a), Value::Int(b)) => {
                        if b == 0 {
                            Err(ConstError::Arithmetic(
                                "constant expression divides by zero".to_string(),
                            ))
                        } else {
                            int_op(a, checked_div, b, Type::Int)
                        }
                    }
                    (Value::Float(a), Value::Float(b)) => {
                        if b == 0.0 {
                            Err(ConstError::Arithmetic(
                                "constant expression divides by zero".to_string(),
                            ))
                        } else {
                            Ok((Value::Float(a / b), Type::Float))
                        }
                    }
                    _ => Err(type_mismatch(*op, &lt, &rt)),
                },
                Mod => match (lv, rv) {
                    (Value::Int(a), Value::Int(b)) => {
                        if b == 0 {
                            Err(ConstError::Arithmetic(
                                "constant expression divides by zero".to_string(),
                            ))
                        } else {
                            int_op(a, checked_rem, b, Type::Int)
                        }
                    }
                    _ => Err(type_mismatch(*op, &lt, &rt)),
                },
                Lt | Gt | Le | Ge | Eq | Ne => {
                    let b = match (lv, rv) {
                        (Value::Int(a), Value::Int(b)) => cmp_values(*op, a.partial_cmp(&b)),
                        (Value::Float(a), Value::Float(b)) => cmp_values(*op, a.partial_cmp(&b)),
                        (Value::Str(a), Value::Str(b)) => cmp_values(*op, a.partial_cmp(&b)),
                        (Value::Bool(a), Value::Bool(b)) => match op {
                            Eq => a == b,
                            Ne => a != b,
                            _ => {
                                return Err(type_mismatch(*op, &lt, &rt));
                            }
                        },
                        _ => return Err(type_mismatch(*op, &lt, &rt)),
                    };
                    Ok((Value::Bool(b), Type::Bool))
                }
                And | Or => match (lv, rv) {
                    (Value::Bool(a), Value::Bool(b)) => {
                        let b2 = if matches!(op, And) { a && b } else { a || b };
                        Ok((Value::Bool(b2), Type::Bool))
                    }
                    _ => Err(type_mismatch(*op, &lt, &rt)),
                },
            }
        }
        ExprKind::Cast { .. } => Err(ConstError::NotConstant(
            "casts (`as`) are not allowed in a constant expression".to_string(),
        )),
        ExprKind::Call { name, .. } => Err(ConstError::NotConstant(format!(
            "function calls are not allowed in a constant expression (`{}`)",
            name
        ))),
        ExprKind::ModCall { module, name, .. } => Err(ConstError::NotConstant(format!(
            "calls into modules (`{}.{}(...)`)",
            module, name
        ))),
        ExprKind::ModConst { module, name } => Err(ConstError::NotConstant(format!(
            "qualified reads (`{}.{}`) are not allowed in a constant expression; reference consts of the same file directly",
            module, name
        ))),
        ExprKind::Index { .. } => Err(ConstError::NotConstant(
            "indexing is not allowed in a constant expression".to_string(),
        )),
    }
}

fn int_op(
    a: i64,
    f: fn(i64, i64) -> Option<i64>,
    b: i64,
    ty: Type,
) -> Result<(Value, Type), ConstError> {
    match f(a, b) {
        Some(r) => Ok((Value::Int(r), ty)),
        None => Err(ConstError::Arithmetic(
            "constant expression overflows the int range".to_string(),
        )),
    }
}

fn checked_add(a: i64, b: i64) -> Option<i64> {
    a.checked_add(b)
}
fn checked_sub(a: i64, b: i64) -> Option<i64> {
    a.checked_sub(b)
}
fn checked_mul(a: i64, b: i64) -> Option<i64> {
    a.checked_mul(b)
}
fn checked_div(a: i64, b: i64) -> Option<i64> {
    a.checked_div(b)
}
fn checked_rem(a: i64, b: i64) -> Option<i64> {
    a.checked_rem(b)
}

/// Apply a comparison operator to an ordering. `None` (unorderable pair,
/// e.g. NaN) becomes a type mismatch.
fn cmp_values(op: BinOp, ord: Option<std::cmp::Ordering>) -> bool {
    use std::cmp::Ordering::*;
    match ord {
        Some(Less) => matches!(op, BinOp::Lt | BinOp::Le | BinOp::Ne),
        Some(Greater) => matches!(op, BinOp::Gt | BinOp::Ge | BinOp::Ne),
        Some(Equal) => matches!(op, BinOp::Le | BinOp::Ge | BinOp::Eq),
        None => matches!(op, BinOp::Ne),
    }
}

fn type_mismatch(op: BinOp, lt: &Type, rt: &Type) -> ConstError {
    ConstError::NotConstant(format!(
        "operator `{}` cannot be applied to {} and {} in a constant expression",
        op,
        lt.as_str(),
        rt.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, ExprKind};

    fn lit_int(v: i64) -> Expr {
        Expr { kind: ExprKind::IntLit(v), line: 1, col: 1 }
    }
    fn lit_float(v: f64) -> Expr {
        Expr { kind: ExprKind::FloatLit(v), line: 1, col: 1 }
    }
    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr { kind: ExprKind::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }, line: 1, col: 1 }
    }

    #[test]
    fn evaluates_literals_and_checked_arithmetic() {
        let e = bin(BinOp::Add, lit_int(2), lit_int(3));
        let (v, t) = eval_const_expr(&e, &HashMap::new(), None).unwrap();
        assert_eq!(v, Value::Int(5));
        assert_eq!(t, Type::Int);

        let e = bin(BinOp::Add, lit_int(i64::MAX), lit_int(1));
        assert!(matches!(
            eval_const_expr(&e, &HashMap::new(), None),
            Err(ConstError::Arithmetic(_))
        ));

        let e = bin(BinOp::Div, lit_float(1.0), lit_float(0.0));
        assert!(matches!(
            eval_const_expr(&e, &HashMap::new(), None),
            Err(ConstError::Arithmetic(_))
        ));
    }

    #[test]
    fn references_earlier_consts_only() {
        let mut consts = HashMap::new();
        consts.insert("MAX".to_string(), (Value::Int(10), Type::Int));
        let e = bin(BinOp::Mul, Expr { kind: ExprKind::Ident("MAX".into()), line: 1, col: 1 }, lit_int(2));
        let (v, _) = eval_const_expr(&e, &consts, None).unwrap();
        assert_eq!(v, Value::Int(20));

        let e = Expr { kind: ExprKind::Ident("NOPE".into()), line: 1, col: 1 };
        assert!(matches!(
            eval_const_expr(&e, &consts, None),
            Err(ConstError::NotConstant(_))
        ));
    }

    #[test]
    fn rejects_calls_casts_and_qualified_reads() {
        let e = Expr {
            kind: ExprKind::Call { name: "f".into(), args: vec![] },
            line: 1,
            col: 1,
        };
        assert!(matches!(eval_const_expr(&e, &HashMap::new(), None), Err(ConstError::NotConstant(_))));
        let e = Expr {
            kind: ExprKind::Cast { expr: Box::new(lit_int(1)), ty: Type::Float },
            line: 1,
            col: 1,
        };
        assert!(matches!(eval_const_expr(&e, &HashMap::new(), None), Err(ConstError::NotConstant(_))));
        let e = Expr {
            kind: ExprKind::ModConst { module: "m".into(), name: "X".into() },
            line: 1,
            col: 1,
        };
        assert!(matches!(eval_const_expr(&e, &HashMap::new(), None), Err(ConstError::NotConstant(_))));
    }

    #[test]
    fn empty_array_takes_the_declared_type() {
        let e = Expr { kind: ExprKind::ArrayLit(vec![]), line: 1, col: 1 };
        let (v, t) =
            eval_const_expr(&e, &HashMap::new(), Some(&Type::Array(Box::new(Type::Int)))).unwrap();
        assert_eq!(t, Type::Array(Box::new(Type::Int)));
        match v {
            Value::Array(a) => assert_eq!(a.borrow().len(), 0),
            other => panic!("expected array, got {:?}", other),
        }
    }
}
