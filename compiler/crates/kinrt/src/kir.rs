//! The KINN Intermediate Representation (KIR): a flat, stack-based
//! instruction set shared by the compiler backend and the runtime.

use crate::value::Value;

/// A native (builtin) function invoked by [`Instr::Native`].
///
/// Native functions are type-checked by the compiler; the VM only ever
/// sees operands with the shapes described here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeId {
    /// `len(x)`: array or string → int (string length counts Unicode
    /// characters, consistent with source positions).
    Len,
    /// `abs(x)`: int → int (overflow-checked) or float → float.
    Abs,
    /// `min(a, b)`: two ints or two floats.
    Min,
    /// `max(a, b)`: two ints or two floats.
    Max,
}

impl NativeId {
    pub fn name(&self) -> &'static str {
        match self {
            NativeId::Len => "len",
            NativeId::Abs => "abs",
            NativeId::Min => "min",
            NativeId::Max => "max",
        }
    }
}

/// The target type of a [`Instr::Cast`]. The VM picks the exact
/// conversion from the operand value and this target; the checker has
/// already rejected impossible combinations.
///
/// `FloatToInt` truncates toward zero and saturates at the i64 bounds
/// (NaN converts to 0), matching Rust's `as` semantics. String casts use
/// the same formatting as `print`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastTarget {
    Int,
    Float,
    Str,
}

impl CastTarget {
    pub fn name(&self) -> &'static str {
        match self {
            CastTarget::Int => "int",
            CastTarget::Float => "float",
            CastTarget::Str => "string",
        }
    }
}

/// One KIR instruction.
///
/// Jumps are absolute positions inside the enclosing function's code
/// vector. Every instruction the current backend can emit has exactly one
/// behaviour; there are no reserved or placeholder instructions.
#[derive(Debug, Clone)]
pub enum Instr {
    /// Push a constant onto the stack.
    Const(Value),
    /// Push the value of local slot `n`.
    Load(u16),
    /// Pop a value into local slot `n`.
    Store(u16),
    /// Pop `b`, pop `a`, push `a + b` (numbers, or string concatenation).
    Add,
    /// Pop `b`, pop `a`, push `a - b`.
    Sub,
    /// Pop `b`, pop `a`, push `a * b`.
    Mul,
    /// Pop `b`, pop `a`, push `a / b` (division by zero is a runtime error).
    Div,
    /// Pop `b`, pop `a`, push `a % b` (integer operands only).
    Mod,
    /// Comparison operators: pop two values, push a `bool`.
    Lt,
    Gt,
    Le,
    Ge,
    /// Equality: pop two values of the same type, push a `bool`.
    Eq,
    Ne,
    /// Unary minus: pop a number, push its negation.
    Neg,
    /// Logical not: pop a `bool`, push its negation.
    Not,
    /// Unconditional jump to `target`.
    Jump(u32),
    /// Pop a `bool`; jump to `target` when it is `false`.
    JumpIfFalse(u32),
    /// Call function index `n`; arguments were pushed in order.
    Call(u16),
    /// Pop the return value (or `Unit`), drop the frame, push the value.
    Return,
    /// Discard the top of the stack.
    Pop,
    /// Pop one value and write it, followed by a newline, to the output.
    Print,
    /// Pop `n` values (the last one is the final element) and push an
    /// array of them, in source order.
    ArrNew(u16),
    /// Pop an index, pop an array, push the element at that index.
    /// Out-of-bounds access is a runtime error (E4005).
    ArrGet,
    /// Pop a value, an index and an array; store the value into the array.
    /// Out-of-bounds access is a runtime error (E4005).
    ArrSet,
    /// Invoke a native builtin. Operands like a function call.
    Native(NativeId),
    /// Pop one value and convert it to the target type.
    Cast(CastTarget),
}

/// A compiled function.
///
/// `lines` maps each instruction index to the source line that produced it,
/// so runtime errors can point back at Kin source.
#[derive(Debug, Clone)]
pub struct FuncDef {
    pub name: String,
    pub nparams: u16,
    pub nlocals: u16,
    pub code: Vec<Instr>,
    pub lines: Vec<u32>,
}

/// A complete KIR program.
#[derive(Debug, Clone)]
pub struct Module {
    pub funcs: Vec<FuncDef>,
}

impl Module {
    /// Index of the function called `name`, if present.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.funcs.iter().position(|f| f.name == name)
    }
}
