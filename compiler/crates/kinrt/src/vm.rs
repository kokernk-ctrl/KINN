//! The KIR virtual machine: a stack machine with one frame per call.
//!
//! Frames keep their locals in a region of the shared value stack:
//!
//! ```text
//! stack:  [ ... caller data ... ][ frame locals (params first) ][ expr stack ]
//!                                  ^ base
//! ```
//!
//! Every instruction the backend emits keeps the value stack balanced, so
//! `Return` can simply truncate back to the frame's base after popping the
//! return value.

use crate::kir::{CastTarget, Instr, Module, NativeId, ValKind};
use crate::value::Value;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// Maximum call depth before the VM reports a stack overflow.
pub const MAX_CALL_DEPTH: usize = 1024;

/// A runtime failure.
///
/// Compile-time problems are diagnosed by `kinc`; these errors can only
/// occur while executing otherwise well-typed programs.
#[derive(Debug, Clone)]
pub struct KinError {
    pub code: &'static str,
    pub message: String,
    pub line: u32,
}

impl KinError {
    fn new(code: &'static str, message: String, line: u32) -> Self {
        KinError { code, message, line }
    }
}

impl fmt::Display for KinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {} (line {})", self.code, self.message, self.line)
    }
}

struct Frame {
    func: usize,
    ip: usize,
    /// Stack index where this frame's locals begin.
    base: usize,
}

/// Execute `module`, starting at `func main()`.
///
/// Program output is written to `out`; a [`KinError`] is returned on
/// runtime failure.
pub fn run(module: &Module, out: &mut dyn std::io::Write) -> Result<(), KinError> {
    let main_idx = module
        .find("main")
        .ok_or_else(|| KinError::new("E4000", "module has no `main` function".to_string(), 0))?;

    let mut stack: Vec<Value> = Vec::with_capacity(64);
    let mut frames: Vec<Frame> = Vec::with_capacity(16);
    stack.resize(module.funcs[main_idx].nlocals as usize, Value::Unit);
    frames.push(Frame { func: main_idx, ip: 0, base: 0 });

    loop {
        let (fi, ip) = {
            let fr = frames.last().expect("frame stack is never empty");
            (fr.func, fr.ip)
        };
        let fdef = &module.funcs[fi];
        if ip >= fdef.code.len() {
            return Err(KinError::new(
                "E4999",
                format!("internal error: fell off the end of `{}`", fdef.name),
                fdef.lines.last().copied().unwrap_or(0),
            ));
        }
        let instr = fdef.code[ip].clone();
        let line = fdef.lines[ip];
        frames.last_mut().unwrap().ip += 1;

        match instr {
            Instr::Const(v) => stack.push(v),

            Instr::Load(n) => {
                let base = frames.last().unwrap().base;
                let v = stack[base + n as usize].clone();
                stack.push(v);
            }
            Instr::Store(n) => {
                let v = stack.pop().expect("vm bug: store on empty stack");
                let base = frames.last().unwrap().base;
                stack[base + n as usize] = v;
            }

            Instr::Add => {
                let (a, b) = pop2(&mut stack);
                match (a, b) {
                    (Value::Int(x), Value::Int(y)) => {
                        stack.push(int_op(line, "integer overflow in `+`", x.checked_add(y))?)
                    }
                    (Value::Float(x), Value::Float(y)) => stack.push(Value::Float(x + y)),
                    (Value::Str(x), Value::Str(y)) => {
                        let mut s = x;
                        s.push_str(&y);
                        stack.push(Value::Str(s));
                    }
                    // 0.4.0 S5 (B.10): array concatenation — a fresh array,
                    // per the checker's app-scope rule. The checker only
                    // types `array + array` inside `app` programs, so
                    // console programs can never reach this arm and the
                    // 0.2 console semantics are unchanged.
                    (Value::Array(x), Value::Array(y)) => {
                        let mut v = x.borrow().clone();
                        v.extend(y.borrow().iter().cloned());
                        stack.push(Value::Array(Rc::new(RefCell::new(v))));
                    }
                    (a, b) => return Err(internal("Add", &a, &b, line)),
                }
            }
            Instr::Sub => {
                let (a, b) = pop2(&mut stack);
                match (a, b) {
                    (Value::Int(x), Value::Int(y)) => {
                        stack.push(int_op(line, "integer overflow in `-`", x.checked_sub(y))?)
                    }
                    (Value::Float(x), Value::Float(y)) => stack.push(Value::Float(x - y)),
                    (a, b) => return Err(internal("Sub", &a, &b, line)),
                }
            }
            Instr::Mul => {
                let (a, b) = pop2(&mut stack);
                match (a, b) {
                    (Value::Int(x), Value::Int(y)) => {
                        stack.push(int_op(line, "integer overflow in `*`", x.checked_mul(y))?)
                    }
                    (Value::Float(x), Value::Float(y)) => stack.push(Value::Float(x * y)),
                    (a, b) => return Err(internal("Mul", &a, &b, line)),
                }
            }
            Instr::Div => {
                let (a, b) = pop2(&mut stack);
                match (a, b) {
                    (Value::Int(x), Value::Int(y)) => {
                        if y == 0 {
                            return Err(KinError::new("E4001", "division by zero".to_string(), line));
                        }
                        stack.push(int_op(line, "integer overflow in `/`", x.checked_div(y))?)
                    }
                    (Value::Float(x), Value::Float(y)) => {
                        // Deliberate deviation from IEEE-754 infinity: Kin
                        // 0.1 treats float division by zero as an error so
                        // beginners see a message instead of `inf`.
                        if y == 0.0 {
                            return Err(KinError::new("E4001", "division by zero".to_string(), line));
                        }
                        stack.push(Value::Float(x / y))
                    }
                    (a, b) => return Err(internal("Div", &a, &b, line)),
                }
            }
            Instr::Mod => {
                let (a, b) = pop2(&mut stack);
                match (a, b) {
                    (Value::Int(x), Value::Int(y)) => {
                        if y == 0 {
                            return Err(KinError::new("E4001", "modulo by zero".to_string(), line));
                        }
                        stack.push(int_op(line, "integer overflow in `%`", x.checked_rem(y))?)
                    }
                    (a, b) => return Err(internal("Mod", &a, &b, line)),
                }
            }

            Instr::Lt | Instr::Gt | Instr::Le | Instr::Ge => {
                let (a, b) = pop2(&mut stack);
                let r = match as_num_pair("comparison", a, b, line)? {
                    NumPair::Ints(x, y) => cmp(&instr, x, y),
                    NumPair::Floats(x, y) => cmp(&instr, x, y),
                };
                stack.push(Value::Bool(r));
            }
            Instr::Eq | Instr::Ne => {
                let (a, b) = pop2(&mut stack);
                if std::mem::discriminant(&a) != std::mem::discriminant(&b) {
                    return Err(internal("equality", &a, &b, line));
                }
                let eq = a == b;
                stack.push(Value::Bool(if matches!(instr, Instr::Eq) { eq } else { !eq }));
            }

            Instr::Neg => {
                let v = stack.pop().expect("vm bug: neg on empty stack");
                match v {
                    Value::Int(x) => stack.push(int_op(
                        line,
                        "integer overflow in unary `-`",
                        x.checked_neg(),
                    )?),
                    Value::Float(x) => stack.push(Value::Float(-x)),
                    v => return Err(internal("Neg", &v, &Value::Unit, line)),
                }
            }
            Instr::Not => {
                let v = stack.pop().expect("vm bug: not on empty stack");
                match v {
                    Value::Bool(b) => stack.push(Value::Bool(!b)),
                    v => return Err(internal("Not", &v, &Value::Unit, line)),
                }
            }

            Instr::Jump(t) => {
                frames.last_mut().unwrap().ip = t as usize;
            }
            Instr::JumpIfFalse(t) => {
                let v = stack.pop().expect("vm bug: jump on empty stack");
                match v {
                    Value::Bool(b) => {
                        if !b {
                            frames.last_mut().unwrap().ip = t as usize;
                        }
                    }
                    v => return Err(internal("JumpIfFalse", &v, &Value::Unit, line)),
                }
            }

            Instr::Call(f) => {
                if frames.len() >= MAX_CALL_DEPTH {
                    return Err(KinError::new(
                        "E4002",
                        format!("call stack overflow (depth limit is {})", MAX_CALL_DEPTH),
                        line,
                    ));
                }
                let callee = &module.funcs[f as usize];
                let nargs = callee.nparams as usize;
                let args: Vec<Value> = if nargs > 0 {
                    let at = stack.len() - nargs;
                    stack.split_off(at)
                } else {
                    Vec::new()
                };
                let base = stack.len();
                stack.resize(base + callee.nlocals as usize, Value::Unit);
                for (i, a) in args.into_iter().enumerate() {
                    stack[base + i] = a;
                }
                frames.push(Frame { func: f as usize, ip: 0, base });
            }
            Instr::Return => {
                let v = stack.pop().unwrap_or(Value::Unit);
                let fr = frames.pop().unwrap();
                stack.truncate(fr.base);
                stack.push(v);
                if frames.is_empty() {
                    return Ok(());
                }
            }

            Instr::Pop => {
                stack.pop().expect("vm bug: pop on empty stack");
            }
            Instr::Print => {
                let v = stack.pop().expect("vm bug: print on empty stack");
                writeln!(out, "{}", v.format()).map_err(|e| {
                    KinError::new("E4003", format!("output error: {}", e), line)
                })?;
            }

            Instr::ArrNew(n) => {
                let count = n as usize;
                if stack.len() < count {
                    return Err(KinError::new(
                        "E4999",
                        "internal error: ArrNew underflow (compiler bug)".to_string(),
                        line,
                    ));
                }
                let at = stack.len() - count;
                let elems: Vec<Value> = stack.split_off(at);
                stack.push(Value::Array(Rc::new(RefCell::new(elems))));
            }
            Instr::ArrGet => {
                let idx = stack.pop().expect("vm bug: ArrGet underflow");
                let arr = stack.pop().expect("vm bug: ArrGet underflow");
                let i = match idx {
                    Value::Int(i) => i,
                    v => {
                        return Err(internal("ArrGet", &v, &Value::Unit, line));
                    }
                };
                match &arr {
                    Value::Array(a) => {
                        let borrowed = a.borrow();
                        if i < 0 || i as usize >= borrowed.len() {
                            return Err(KinError::new(
                                "E4005",
                                format!(
                                    "array index {} out of bounds (len {})",
                                    i,
                                    borrowed.len()
                                ),
                                line,
                            ));
                        }
                        stack.push(borrowed[i as usize].clone());
                    }
                    v => return Err(internal("ArrGet", v, &Value::Unit, line)),
                }
            }
            Instr::ArrSet => {
                let value = stack.pop().expect("vm bug: ArrSet underflow");
                let idx = stack.pop().expect("vm bug: ArrSet underflow");
                let arr = stack.pop().expect("vm bug: ArrSet underflow");
                let i = match idx {
                    Value::Int(i) => i,
                    v => return Err(internal("ArrSet", &v, &Value::Unit, line)),
                };
                match &arr {
                    Value::Array(a) => {
                        let len = a.borrow().len();
                        if i < 0 || i as usize >= len {
                            return Err(KinError::new(
                                "E4005",
                                format!("array index {} out of bounds (len {})", i, len),
                                line,
                            ));
                        }
                        a.borrow_mut()[i as usize] = value;
                    }
                    v => return Err(internal("ArrSet", v, &Value::Unit, line)),
                }
            }

            Instr::Native(id) => {
                let result = exec_native(id, &mut stack, line)?;
                stack.push(result);
            }

            Instr::Cast(target) => {
                let v = stack.pop().expect("vm bug: cast on empty stack");
                let out_val = match (&v, target) {
                    (Value::Int(i), CastTarget::Float) => Value::Float(*i as f64),
                    (Value::Float(f), CastTarget::Int) => {
                        // Truncate toward zero; saturate at the i64 bounds,
                        // NaN converts to 0 (documented Rust `as` semantics).
                        Value::Int(*f as i64)
                    }
                    (Value::Int(i), CastTarget::Str) => Value::Str(i.to_string()),
                    (Value::Float(_), CastTarget::Str) => Value::Str(v.format()),
                    (Value::Bool(_), CastTarget::Str) => Value::Str(v.format()),
                    _ => {
                        return Err(internal(
                            &format!("cast to {}", target.name()),
                            &v,
                            &Value::Unit,
                            line,
                        ));
                    }
                };
                stack.push(out_val);
            }
        }
    }
}

enum NumPair {
    Ints(i64, i64),
    Floats(f64, f64),
}

/// Execute a native builtin. Pops its operands from `stack` and returns
/// the result (the caller pushes it).
fn exec_native(id: NativeId, stack: &mut Vec<Value>, line: u32) -> Result<Value, KinError> {
    match id {
        NativeId::Len => {
            let v = stack.pop().expect("vm bug: len underflow");
            match v {
                Value::Array(a) => Ok(Value::Int(a.borrow().len() as i64)),
                Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
                v => Err(internal("len", &v, &Value::Unit, line)),
            }
        }
        NativeId::Abs => {
            let v = stack.pop().expect("vm bug: abs underflow");
            match v {
                Value::Int(i) => int_op(line, "integer overflow in `abs`", i.checked_abs()),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                v => Err(internal("abs", &v, &Value::Unit, line)),
            }
        }
        NativeId::Min => {
            let (a, b) = pop2(stack);
            match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x.min(y))),
                (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x.min(y))),
                (a, b) => Err(internal("min", &a, &b, line)),
            }
        }
        NativeId::Max => {
            let (a, b) = pop2(stack);
            match (a, b) {
                (Value::Int(x), Value::Int(y)) => Ok(Value::Int(x.max(y))),
                (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x.max(y))),
                (a, b) => Err(internal("max", &a, &b, line)),
            }
        }
        // 0.4.0 S5 (B.10 string helpers). The checker resolves these
        // names only in `app` scope, so a console program can never
        // emit the instructions; the arms exist for the app code
        // generator and the differential corpus.
        NativeId::Str => {
            let v = stack.pop().expect("vm bug: str underflow");
            match v {
                Value::Int(_) | Value::Float(_) | Value::Bool(_) | Value::Str(_) => {
                    Ok(Value::Str(v.format()))
                }
                v => Err(internal("str", &v, &Value::Unit, line)),
            }
        }
        NativeId::Upper => {
            let v = stack.pop().expect("vm bug: upper underflow");
            match v {
                Value::Str(s) => Ok(Value::Str(case_map(&s, true))),
                v => Err(internal("upper", &v, &Value::Unit, line)),
            }
        }
        NativeId::Lower => {
            let v = stack.pop().expect("vm bug: lower underflow");
            match v {
                Value::Str(s) => Ok(Value::Str(case_map(&s, false))),
                v => Err(internal("lower", &v, &Value::Unit, line)),
            }
        }
        NativeId::Trim => {
            let v = stack.pop().expect("vm bug: trim underflow");
            match v {
                Value::Str(s) => Ok(Value::Str(s.trim().to_string())),
                v => Err(internal("trim", &v, &Value::Unit, line)),
            }
        }
        NativeId::Contains => {
            let (a, b) = pop2(stack);
            match (a, b) {
                (Value::Str(s), Value::Str(sub)) => Ok(Value::Bool(s.contains(&sub))),
                (a, b) => Err(internal("contains", &a, &b, line)),
            }
        }
        NativeId::Split => {
            let (a, b) = pop2(stack);
            match (a, b) {
                (Value::Str(s), Value::Str(sep)) => {
                    let pieces: Vec<Value> = if sep.is_empty() {
                        s.chars().map(|c| Value::Str(c.to_string())).collect()
                    } else {
                        s.split(sep.as_str()).map(|p| Value::Str(p.to_string())).collect()
                    };
                    Ok(Value::Array(Rc::new(RefCell::new(pieces))))
                }
                (a, b) => Err(internal("split", &a, &b, line)),
            }
        }
        NativeId::Join => {
            let (a, b) = pop2(stack);
            match (a, b) {
                (Value::Array(arr), Value::Str(sep)) => {
                    let parts: Vec<String> = arr
                        .borrow()
                        .iter()
                        .map(|v| match v {
                            Value::Str(s) => s.clone(),
                            v => {
                                // The checker guarantees [string]; this
                                // arm exists for hand-built KIR honesty.
                                v.format()
                            }
                        })
                        .collect();
                    Ok(Value::Str(parts.join(&sep)))
                }
                (a, b) => Err(internal("join", &a, &b, line)),
            }
        }
        // 0.4.0 S7 (app scope only): the route table lives in the WEB
        // runtime (kinnrt.js reads the current URL); the plain VM has no
        // router to consult. Console programs can never reach this arm
        // (the checker resolves `param` only inside `app` programs, and
        // app code is translated by the web backend, not executed here),
        // so refusing honestly beats inventing a host.
        NativeId::Param => {
            let v = stack.pop().expect("vm bug: param underflow");
            match v {
                Value::Str(_) => Err(KinError::new(
                    "E4000",
                    "param() is a web-router builtin — this host has no route context".to_string(),
                    line,
                )),
                v => Err(internal("param", &v, &Value::Unit, line)),
            }
        }
    }
}

/// One-to-one Unicode case mapping: each code point whose case mapping
/// expands to several characters keeps its original form (no expansion —
/// deterministic and portable across engines; the JS runtime implements
/// the identical rule).
fn case_map(s: &str, to_upper: bool) -> String {
    s.chars()
        .map(|c| {
            let mapped: String = if to_upper {
                c.to_uppercase().collect()
            } else {
                c.to_lowercase().collect()
            };
            if mapped.chars().count() == 1 { mapped } else { c.to_string() }
        })
        .collect()
}

fn as_num_pair(op: &str, a: Value, b: Value, line: u32) -> Result<NumPair, KinError> {
    Ok(match (a, b) {
        (Value::Int(x), Value::Int(y)) => NumPair::Ints(x, y),
        (Value::Float(x), Value::Float(y)) => NumPair::Floats(x, y),
        (a, b) => return Err(internal(op, &a, &b, line)),
    })
}

fn cmp<T: PartialOrd>(instr: &Instr, x: T, y: T) -> bool {
    match instr {
        Instr::Lt => x < y,
        Instr::Gt => x > y,
        Instr::Le => x <= y,
        _ => x >= y,
    }
}

fn int_op(line: u32, what: &str, v: Option<i64>) -> Result<Value, KinError> {
    v.map(Value::Int).ok_or_else(|| KinError::new("E4004", format!("{}", what), line))
}

fn pop2(stack: &mut Vec<Value>) -> (Value, Value) {
    let b = stack.pop().expect("vm bug: stack underflow");
    let a = stack.pop().expect("vm bug: stack underflow");
    (a, b)
}

fn internal(op: &str, a: &Value, b: &Value, line: u32) -> KinError {
    KinError::new(
        "E4999",
        format!(
            "internal error: invalid operand types for `{}` ({} and {}) — \
             this is a compiler bug, please report it",
            op,
            a.type_name(),
            b.type_name()
        ),
        line,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kir::{FuncDef, Instr};

    /// `Const(Unit); Return` — the epilogue every function ends with.
    fn end() -> Vec<Instr> {
        vec![Instr::Const(Value::Unit), Instr::Return]
    }

    fn func(name: &str, nparams: u16, nlocals: u16, code: Vec<Instr>, lines: Vec<u32>) -> FuncDef {
        FuncDef {
            name: name.to_string(),
            nparams,
            nlocals,
            code,
            lines,
            param_kinds: Vec::new(),
            ret_kind: ValKind::Unit,
        }
    }

    /// Build a `main`-only module from `code` plus the epilogue.
    fn module_nl(code: Vec<Instr>, nlocals: u16) -> Module {
        let mut full = code;
        full.extend(end());
        let lines = vec![1u32; full.len()];
        Module { consts: Vec::new(), funcs: vec![func("main", 0, nlocals, full, lines)] }
    }

    fn module(code: Vec<Instr>) -> Module {
        module_nl(code, 0)
    }

    fn run_module(m: &Module) -> Result<String, KinError> {
        let mut buf: Vec<u8> = Vec::new();
        run(m, &mut buf)?;
        Ok(String::from_utf8(buf).unwrap())
    }

    #[test]
    fn prints_a_constant() {
        let m = module(vec![Instr::Const(Value::Int(7)), Instr::Print]);
        assert_eq!(run_module(&m).unwrap(), "7\n");
    }

    #[test]
    fn arithmetic_with_locals() {
        let m = module_nl(
            vec![
                Instr::Const(Value::Int(2)),
                Instr::Store(0),
                Instr::Const(Value::Int(3)),
                Instr::Store(1),
                Instr::Load(0),
                Instr::Load(1),
                Instr::Add,
                Instr::Print,
            ],
            2,
        );
        assert_eq!(run_module(&m).unwrap(), "5\n");
    }

    #[test]
    fn conditional_jump_counts_to_three() {
        // slot 0: i, slot 1: limit — `i = 0; while i < 3 { print(i); i = i + 1; }`
        let mut code = vec![
            Instr::Const(Value::Int(0)),  // 0
            Instr::Store(0),              // 1
            Instr::Const(Value::Int(3)),  // 2
            Instr::Store(1),              // 3
            Instr::Load(0),               // 4  L0:
            Instr::Load(1),               // 5
            Instr::Lt,                    // 6
            Instr::JumpIfFalse(0),        // 7  patched below
            Instr::Load(0),               // 8
            Instr::Print,                 // 9
            Instr::Load(0),               // 10
            Instr::Const(Value::Int(1)),  // 11
            Instr::Add,                   // 12
            Instr::Store(0),              // 13
            Instr::Jump(4),               // 14 back to L0
        ];
        let epilogue = code.len() as u32; // 15
        code[7] = Instr::JumpIfFalse(epilogue);
        let m = module_nl(code, 2);
        assert_eq!(run_module(&m).unwrap(), "0\n1\n2\n");
    }

    #[test]
    fn calls_and_frames() {
        let double_code = vec![
            Instr::Load(0),
            Instr::Const(Value::Int(2)),
            Instr::Mul,
            Instr::Return,
        ];
        let double = func("double", 1, 1, double_code.clone(), vec![1; double_code.len()]);
        let mut main_code = vec![
            Instr::Const(Value::Int(21)),
            Instr::Call(1),
            Instr::Print,
        ];
        main_code.extend(end());
        let main = func("main", 0, 0, main_code.clone(), vec![1; main_code.len()]);
        let m = Module { consts: Vec::new(), funcs: vec![main, double] };
        assert_eq!(run_module(&m).unwrap(), "42\n");
    }

    #[test]
    fn void_call_leaves_unit_which_is_popped() {
        let shout_code = vec![
            Instr::Const(Value::Str("hey".to_string())),
            Instr::Print,
            Instr::Const(Value::Unit),
            Instr::Return,
        ];
        let shout = func("shout", 0, 0, shout_code.clone(), vec![1; shout_code.len()]);
        let mut main_code = vec![Instr::Call(1), Instr::Pop, Instr::Const(Value::Int(1)), Instr::Print];
        main_code.extend(end());
        let main = func("main", 0, 0, main_code.clone(), vec![1; main_code.len()]);
        let m = Module { consts: Vec::new(), funcs: vec![main, shout] };
        assert_eq!(run_module(&m).unwrap(), "hey\n1\n");
    }

    #[test]
    fn division_by_zero_is_a_runtime_error() {
        let m = module(vec![
            Instr::Const(Value::Int(1)),
            Instr::Const(Value::Int(0)),
            Instr::Div,
            Instr::Print,
        ]);
        let err = run_module(&m).unwrap_err();
        assert_eq!(err.code, "E4001");
    }

    #[test]
    fn integer_overflow_is_detected() {
        let m = module(vec![
            Instr::Const(Value::Int(i64::MAX)),
            Instr::Const(Value::Int(1)),
            Instr::Add,
            Instr::Print,
        ]);
        let err = run_module(&m).unwrap_err();
        assert_eq!(err.code, "E4004");
    }

    #[test]
    fn deep_recursion_reports_stack_overflow() {
        // f(n): if n > 0 { f(n - 1); }
        let f_code = vec![
            Instr::Load(0),               // 0
            Instr::Const(Value::Int(0)),  // 1
            Instr::Gt,                    // 2
            Instr::JumpIfFalse(9),        // 3
            Instr::Load(0),               // 4
            Instr::Const(Value::Int(1)),  // 5
            Instr::Sub,                   // 6
            Instr::Call(0),               // 7
            Instr::Pop,                   // 8
            Instr::Const(Value::Unit),    // 9
            Instr::Return,                // 10
        ];
        let f = func("f", 1, 1, f_code.clone(), vec![1; f_code.len()]);
        let mut main_code = vec![
            Instr::Const(Value::Int(200_000)),
            Instr::Call(0),
            Instr::Pop,
        ];
        main_code.extend(end());
        let main = func("main", 0, 0, main_code.clone(), vec![1; main_code.len()]);
        let m = Module { consts: Vec::new(), funcs: vec![f, main] };
        let err = run_module(&m).unwrap_err();
        assert_eq!(err.code, "E4002");
    }

    #[test]
    fn string_concatenation_and_equality() {
        let m = module(vec![
            Instr::Const(Value::Str("a".to_string())),
            Instr::Const(Value::Str("b".to_string())),
            Instr::Add,
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), "ab\n");
    }

    #[test]
    fn float_formatting_through_print() {
        let m = module(vec![
            Instr::Const(Value::Float(2.0)),
            Instr::Print,
            Instr::Const(Value::Float(0.5)),
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), "2.0\n0.5\n");
    }

    #[test]
    fn runtime_errors_carry_the_source_line() {
        let code = vec![
            Instr::Const(Value::Int(1)),
            Instr::Const(Value::Int(0)),
            Instr::Div,
        ];
        let lines = vec![3u32, 3, 7];
        let m = Module { consts: Vec::new(), funcs: vec![func("main", 0, 0, code, lines)] };
        let err = run_module(&m).unwrap_err();
        assert_eq!(err.line, 7);
    }

    // -- 0.2.0: arrays, natives, casts ------------------------------------

    #[test]
    fn array_create_read_write() {
        // a = [10, 20]; print(a[1]); a[0] = 5; print(a[0]);
        let m = module_nl(
            vec![
                Instr::Const(Value::Int(10)),
                Instr::Const(Value::Int(20)),
                Instr::ArrNew(2),
                Instr::Store(0),
                Instr::Load(0),
                Instr::Const(Value::Int(1)),
                Instr::ArrGet,
                Instr::Print,
                Instr::Load(0),
                Instr::Const(Value::Int(0)),
                Instr::Const(Value::Int(5)),
                Instr::ArrSet,
                Instr::Load(0),
                Instr::Const(Value::Int(0)),
                Instr::ArrGet,
                Instr::Print,
            ],
            1,
        );
        assert_eq!(run_module(&m).unwrap(), "20\n5\n");
    }

    #[test]
    fn arrays_have_reference_semantics_in_the_vm() {
        // a = [1]; b = a; b[0] = 9; print(a[0]);
        let m = module_nl(
            vec![
                Instr::Const(Value::Int(1)),
                Instr::ArrNew(1),
                Instr::Store(0),
                Instr::Load(0),
                Instr::Store(1),
                Instr::Load(1),
                Instr::Const(Value::Int(0)),
                Instr::Const(Value::Int(9)),
                Instr::ArrSet,
                Instr::Load(0),
                Instr::Const(Value::Int(0)),
                Instr::ArrGet,
                Instr::Print,
            ],
            2,
        );
        assert_eq!(run_module(&m).unwrap(), "9\n");
    }

    #[test]
    fn out_of_bounds_read_and_write_are_errors() {
        let m = module(vec![
            Instr::Const(Value::Int(1)),
            Instr::ArrNew(1),
            Instr::Const(Value::Int(5)),
            Instr::ArrGet,
        ]);
        let err = run_module(&m).unwrap_err();
        assert_eq!(err.code, "E4005");
        assert!(err.message.contains("len 1"));

        let m = module(vec![
            Instr::Const(Value::Int(1)),
            Instr::ArrNew(1),
            Instr::Const(Value::Int(-1)),
            Instr::Const(Value::Int(7)),
            Instr::ArrSet,
        ]);
        assert_eq!(run_module(&m).unwrap_err().code, "E4005");
    }

    #[test]
    fn array_equality_is_deep() {
        // [1, 2] == [1, 2]
        let m = module(vec![
            Instr::Const(Value::Int(1)),
            Instr::Const(Value::Int(2)),
            Instr::ArrNew(2),
            Instr::Const(Value::Int(1)),
            Instr::Const(Value::Int(2)),
            Instr::ArrNew(2),
            Instr::Eq,
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), "true\n");
    }

    #[test]
    fn native_len_abs_min_max() {
        // len([1,2,3])
        let m = module(vec![
            Instr::Const(Value::Int(1)),
            Instr::Const(Value::Int(2)),
            Instr::Const(Value::Int(3)),
            Instr::ArrNew(3),
            Instr::Native(NativeId::Len),
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), "3\n");

        // len("héllo") counts characters, not bytes
        let m = module(vec![
            Instr::Const(Value::Str("héllo".to_string())),
            Instr::Native(NativeId::Len),
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), "5\n");

        // abs(-7)
        let m = module(vec![
            Instr::Const(Value::Int(-7)),
            Instr::Native(NativeId::Abs),
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), "7\n");

        // min(3, 2), max(2.5, 1.5)
        let m = module(vec![
            Instr::Const(Value::Int(3)),
            Instr::Const(Value::Int(2)),
            Instr::Native(NativeId::Min),
            Instr::Print,
            Instr::Const(Value::Float(2.5)),
            Instr::Const(Value::Float(1.5)),
            Instr::Native(NativeId::Max),
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), "2\n2.5\n");
    }

    #[test]
    fn abs_of_min_int_overflows() {
        let m = module(vec![
            Instr::Const(Value::Int(i64::MIN)),
            Instr::Native(NativeId::Abs),
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap_err().code, "E4004");
    }

    #[test]
    fn casts_numeric_and_to_string() {
        let m = module(vec![
            Instr::Const(Value::Int(2)),
            Instr::Cast(CastTarget::Float),
            Instr::Print, // 2.0
            Instr::Const(Value::Float(2.9)),
            Instr::Cast(CastTarget::Int),
            Instr::Print, // 2 (truncate toward zero)
            Instr::Const(Value::Float(-2.9)),
            Instr::Cast(CastTarget::Int),
            Instr::Print, // -2
            Instr::Const(Value::Int(42)),
            Instr::Cast(CastTarget::Str),
            Instr::Print,
            Instr::Const(Value::Float(2.0)),
            Instr::Cast(CastTarget::Str),
            Instr::Print, // 2.0 keeps its decimal point
            Instr::Const(Value::Bool(true)),
            Instr::Cast(CastTarget::Str),
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), "2.0\n2\n-2\n42\n2.0\ntrue\n");
    }

    #[test]
    fn float_to_int_saturates() {
        let m = module(vec![
            Instr::Const(Value::Float(1e300)),
            Instr::Cast(CastTarget::Int),
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), format!("{}\n", i64::MAX));
    }

    // -- 0.4.0 S5: array concatenation and the B.10 string helpers ------

    #[test]
    fn array_concat_makes_a_fresh_array() {
        let a = Value::Array(Rc::new(RefCell::new(vec![Value::Int(1)])));
        let b = Value::Array(Rc::new(RefCell::new(vec![Value::Int(2)])));
        let m = module(vec![
            Instr::Const(a.clone()),
            Instr::Const(b.clone()),
            Instr::Add,
            Instr::Print,
        ]);
        assert_eq!(run_module(&m).unwrap(), "[1, 2]\n");
        // The operands are untouched (fresh result, not a mutation).
        assert_eq!(a.format(), "[1]");
        assert_eq!(b.format(), "[2]");
    }

    #[test]
    fn str_native_formats_like_print() {
        let m = module(vec![
            Instr::Const(Value::Float(2.0)),
            Instr::Native(NativeId::Str),
            Instr::Print, // 2.0
            Instr::Const(Value::Int(-7)),
            Instr::Native(NativeId::Str),
            Instr::Print, // -7
            Instr::Const(Value::Bool(false)),
            Instr::Native(NativeId::Str),
            Instr::Print, // false
        ]);
        assert_eq!(run_module(&m).unwrap(), "2.0\n-7\nfalse\n");
    }

    #[test]
    fn upper_lower_are_one_to_one() {
        let m = module(vec![
            Instr::Const(Value::Str("Kin Ä ß 𐐨".to_string())),
            Instr::Native(NativeId::Upper),
            Instr::Print, // KIN Ä ß 𐐀 (ß expands, stays; 𐐨 maps 1:1)
            Instr::Const(Value::Str("İNFO".to_string())),
            Instr::Native(NativeId::Lower),
            Instr::Print, // İnfo (İ expands, stays; NFO lower)
        ]);
        assert_eq!(run_module(&m).unwrap(), "KIN Ä ß 𐐀\nİnfo\n");
    }

    #[test]
    fn trim_uses_unicode_whitespace_not_bom() {
        let m = module(vec![
            Instr::Const(Value::Str(" \u{a0}x\u{feff}\t".to_string())),
            Instr::Native(NativeId::Trim),
            Instr::Print, // "x\u{feff}" — U+FEFF is not White_Space
        ]);
        assert_eq!(run_module(&m).unwrap(), "x\u{feff}\n");
    }

    #[test]
    fn split_empty_separator_yields_code_points() {
        let m = module(vec![
            Instr::Const(Value::Str("hï".to_string())),
            Instr::Const(Value::Str(String::new())),
            Instr::Native(NativeId::Split),
            Instr::Print, // [h, ï]
        ]);
        assert_eq!(run_module(&m).unwrap(), "[h, ï]\n");
    }

    #[test]
    fn split_keeps_empty_pieces_and_join_round_trips() {
        let m = module(vec![
            Instr::Const(Value::Str("a,,b".to_string())),
            Instr::Const(Value::Str(",".to_string())),
            Instr::Native(NativeId::Split),
            Instr::Print, // [a, , b]
        ]);
        assert_eq!(run_module(&m).unwrap(), "[a, , b]\n");
    }

    #[test]
    fn contains_and_join_work() {
        let m = module_nl(
            vec![
                Instr::Const(Value::Str("Kin".to_string())),
                Instr::Const(Value::Str("in".to_string())),
                Instr::Native(NativeId::Contains),
                Instr::Print, // true
                Instr::Const(Value::Str("a".to_string())),
                Instr::Const(Value::Str("b".to_string())),
                Instr::Const(Value::Str("c".to_string())),
                Instr::ArrNew(3),
                Instr::Store(0),
                Instr::Load(0),
                Instr::Const(Value::Str("-".to_string())),
                Instr::Native(NativeId::Join),
                Instr::Print, // a-b-c
            ],
            1,
        );
        assert_eq!(run_module(&m).unwrap(), "true\na-b-c\n");
    }
}
