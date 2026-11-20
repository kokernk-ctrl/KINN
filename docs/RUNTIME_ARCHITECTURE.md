# Runtime Architecture

The runtime is the `kinrt` crate: the KIR module format and the virtual
machine that executes it. It knows nothing about Kin source code — only
about KIR — which keeps it small, portable and independently testable.

## Design

- **Stack machine with call frames.** One shared value stack; each
  frame owns a contiguous locals region (`base .. base + nlocals`),
  with parameters in the first slots. The expression stack lives above
  the locals.

  ```
  stack: [ ... caller data ... ][ frame locals (params first) ][ expr stack ]
                                ^ base
  ```

- **No register allocation, no GC.** Values are a simple enum
  (`Int(i64) | Float(f64) | Bool | Str(String) | Unit`). Strings are
  immutable; nothing in 0.1 allocates beyond string concatenation.
- **Checked execution.** Integer arithmetic is checked (E4004 on
  overflow), division/modulo by zero is E4001, call depth is capped at
  1024 frames (E4002). Type-mismatched operands are impossible for
  checked programs and surface as E4999 internal errors.
- **Debug information.** Every instruction carries the source line that
  produced it (`FuncDef::lines`), so runtime errors report Kin lines.

## Instruction set (KIR)

| Instruction | Effect |
| --- | --- |
| `Const(v)` | push a constant |
| `Load(n)` / `Store(n)` | local slot read/write |
| `Add Sub Mul Div Mod` | arithmetic (`Add` also concatenates strings) |
| `Lt Gt Le Ge Eq Ne` | comparisons |
| `Neg Not` | unary |
| `Jump(t)` / `JumpIfFalse(t)` | control flow (absolute targets) |
| `Call(f)` | call function index `f` (args pushed in order) |
| `Return` | pop result (or `Unit`), truncate to frame base, push result |
| `Pop` | discard |
| `Print` | pop one value, write it with a newline |

There are no reserved or unused instructions — everything in the set is
emitted by the current backend. `kinn kir file.kin` prints the full
disassembly of any program.

## Frame protocol

- `Call`: pop `nargs`, extend the stack by the callee's `nlocals`,
  copy arguments into the first slots, push a frame.
- `Return`: pop the result value, pop the frame, truncate the stack to
  the frame's base, push the result. Void functions return `Unit`;
  callers `Pop` it immediately (the lowerer knows the callee's return
  type).
- Every function body is lowered with an explicit trailing
  `Const(Unit); Return` as a safety net; for accepted programs it is
  unreachable in non-void functions.

## Execution limits (deliberate)

| Limit | Value | Error |
| --- | --- | --- |
| Call depth | 1024 frames | E4002 |
| Integer overflow | checked | E4004 |
| Division / modulo by zero | runtime error (int and float) | E4001 |
| Float division by zero | error, not IEEE `inf` (ADR-0009) | E4001 |

Unbounded loops are allowed — like any language — but Kin 0.1 programs
cannot access files, the network or the clock, so a runaway loop is the
worst a program can do. Loop guards / step limits are a 0.2 discussion
for the Studio embedding.

## Output formatting

`Value::format` prints values the way a beginner expects: `42`, `2.0`
(integral floats keep the decimal point), `0.30000000000000004`
(honest shortest round-trip), `true`/`false`, strings bare.

## Portability and Android

`kinrt` is pure, dependency-free Rust: it compiles anywhere stable Rust
does, including `aarch64-linux-android` with the NDK toolchain. That
makes the *interpretation* path viable on-device. We do not yet claim
verified Android builds or on-device native compilation — device
verification belongs to the 0.5 milestone (see
[GRAPHICS_ARCHITECTURE.md](GRAPHICS_ARCHITECTURE.md) and
[ROADMAP.md](ROADMAP.md)).

## Future

- 0.2: local slot reuse after scope exit (P-003), stdlib builtins as a
  proper native function table.
- 0.3: native backend investigation; the KIR format is the contract
  that survives any backend swap.
