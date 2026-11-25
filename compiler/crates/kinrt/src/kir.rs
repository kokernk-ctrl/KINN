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
    /// `str(x)`: int/float/bool/string → string, formatted exactly like
    /// `print` (0.4.0 S5; app-scope builtin B.10 — the checker only
    /// resolves it inside `app` programs, so console semantics are
    /// unchanged).
    Str,
    /// `upper(s)` / `lower(s)`: string → string, one-to-one per Unicode
    /// code point; a code point whose case mapping expands to more than
    /// one character is kept unchanged (no expansion, deterministic on
    /// every engine).
    Upper,
    Lower,
    /// `trim(s)`: string → string with leading/trailing Unicode
    /// `White_Space` removed (the Rust `char::is_whitespace` set; in
    /// particular U+FEFF is **not** whitespace, unlike JS `trim`).
    Trim,
    /// `contains(s, sub)`: string × string → bool (substring test).
    Contains,
    /// `split(s, sep)`: string × string → [string]. An empty separator
    /// splits into single code points; a non-empty separator splits on
    /// every occurrence (empty pieces between adjacent separators, and
    /// leading/trailing pieces, are kept).
    Split,
    /// `join(a, sep)`: [string] × string → string.
    Join,
}

impl NativeId {
    pub fn name(&self) -> &'static str {
        match self {
            NativeId::Len => "len",
            NativeId::Abs => "abs",
            NativeId::Min => "min",
            NativeId::Max => "max",
            NativeId::Str => "str",
            NativeId::Upper => "upper",
            NativeId::Lower => "lower",
            NativeId::Trim => "trim",
            NativeId::Contains => "contains",
            NativeId::Split => "split",
            NativeId::Join => "join",
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

/// The static kind of a value, used by the JavaScript code generator
/// (0.4.0 S5) to pick int-vs-float-vs-string arms that JS numbers cannot
/// distinguish at runtime. Mirrors the checker's `ast::Type` without a
/// dependency on the compiler crate: `kinrt` stays self-contained.
///
/// `Any` marks "kind unknown" (a hand-built `FuncDef` without
/// `param_kinds`); the JS generator treats reaching an ambiguous operand
/// as a hard error rather than guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValKind {
    Int,
    Float,
    Bool,
    Str,
    Array(Box<ValKind>),
    /// Void / not-yet-written slots.
    Unit,
    Any,
}

impl ValKind {
    /// Compact code-generator tag: "i" | "f" | "b" | "s" | "u",
    /// arrays as "a" + element tag ("ai", "aaf", …).
    pub fn tag(&self) -> String {
        match self {
            ValKind::Int => "i".to_string(),
            ValKind::Float => "f".to_string(),
            ValKind::Bool => "b".to_string(),
            ValKind::Str => "s".to_string(),
            ValKind::Unit => "u".to_string(),
            ValKind::Any => "?".to_string(),
            ValKind::Array(e) => format!("a{}", e.tag()),
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
/// so runtime errors can point back at Kin source. `param_kinds` (0.4.0
/// S5) carries the declared kind of each parameter — the VM ignores it
/// (it dispatches on runtime values); the JS code generator seeds its
/// type analysis from it so `print(2)` and `print(2.0)` can be told
/// apart in the browser. Additive, same spirit as S1's `consts` table.
#[derive(Debug, Clone)]
pub struct FuncDef {
    pub name: String,
    pub nparams: u16,
    pub nlocals: u16,
    pub code: Vec<Instr>,
    pub lines: Vec<u32>,
    pub param_kinds: Vec<ValKind>,
    /// Declared return kind (S5) — like `param_kinds`, the VM ignores
    /// it; the JS generator's type analysis uses it for `Call` sites.
    pub ret_kind: ValKind,
}

/// A complete KIR program.
///
/// `consts` (0.4.0 S1) holds the compile-time constants folded at
/// lowering: `(qualified name, value)` in deterministic file order (entry
/// file first, then imports in load order; source order within a file).
/// Every read of a `const` compiles to a plain `Instr::Const(value)` —
/// there are no new instructions — so this table is the auditable record
/// of what was folded, and `kinn kir` can show it.
#[derive(Debug, Clone)]
pub struct Module {
    pub funcs: Vec<FuncDef>,
    pub consts: Vec<(String, Value)>,
}

impl Module {
    /// Index of the function called `name`, if present.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.funcs.iter().position(|f| f.name == name)
    }

    /// Value of the folded constant called `name`, if present.
    pub fn find_const(&self, name: &str) -> Option<&Value> {
        self.consts.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}
