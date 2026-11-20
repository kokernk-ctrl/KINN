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

use crate::kir::{Instr, Module};
use crate::value::Value;
use std::fmt;

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
        }
    }
}

enum NumPair {
    Ints(i64, i64),
    Floats(f64, f64),
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
        FuncDef { name: name.to_string(), nparams, nlocals, code, lines }
    }

    /// Build a `main`-only module from `code` plus the epilogue.
    fn module_nl(code: Vec<Instr>, nlocals: u16) -> Module {
        let mut full = code;
        full.extend(end());
        let lines = vec![1u32; full.len()];
        Module { funcs: vec![func("main", 0, nlocals, full, lines)] }
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
        let m = Module { funcs: vec![main, double] };
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
        let m = Module { funcs: vec![main, shout] };
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
        let m = Module { funcs: vec![f, main] };
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
        let m = Module { funcs: vec![func("main", 0, 0, code, lines)] };
        let err = run_module(&m).unwrap_err();
        assert_eq!(err.line, 7);
    }
}
