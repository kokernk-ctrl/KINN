# Compiler Architecture

The compiler is a Rust workspace under `compiler/` with three crates
and **zero external dependencies** — `cargo build` never touches the
network (ADR-0001).

```
compiler/
├── Cargo.toml             workspace: kinrt, kinc, kinn
└── crates/
    ├── kinrt/             runtime: KIR + stack VM + disassembler
    ├── kinc/              compiler library: the five stages below
    └── kinn/              CLI binary
```

## Pipeline

```
            kinc                                        kinrt
source ─► lexer ─► tokens ─► parser ─► AST ─► checker ─► lower ─► KIR module ─► run()
                 (E1xxx)             (E2xxx)  (E3xxx)          (stack VM)
                     │                   │        │                              │
                     └───────── diagnostics (codes, positions, snippets) ───────┘
                                                              runtime errors (E4xxx)
```

One `Vec<Diagnostic>` flows through the whole pipeline: every stage
enriches it, the CLI renders it with source snippets and carets.
`driver::compile_full` returns `(Option<Compiled>, Vec<Diagnostic>)`
so warnings and notes survive even on success.

### Stage details

| Stage | Module | Notes |
| --- | --- | --- |
| Lexing | `kinc::lexer` | char-scanner, 1-based line/col, comments skipped, errors collected and lexing continues where safe |
| Parsing | `kinc::parser` | recursive descent; **stops at the first syntax error** (P-001); mode directive must be the first statement |
| AST | `kinc::ast` | owned, position-annotated nodes; `ast::dump` prints the tree for `kinn ast` |
| Checking | `kinc::checker` | two passes: signatures, then bodies; block scopes; poison `Type::Error` prevents cascades; B-mode lint and unreachable-code warning |
| Lowering | `kinc::lower` | AST → KIR; assumes checker invariants (no re-resolution); `and`/`or` lowered as short-circuit jumps; `break`/`continue` patched per loop |
| Backend | `kinrt::vm` | interpretation of KIR (see [RUNTIME_ARCHITECTURE.md](RUNTIME_ARCHITECTURE.md)) |

### Key invariants

- The lowerer never re-checks: if `kinc::checker` accepted the program,
  the VM can execute it; any violation is an `E4999` internal error
  (treated as a compiler bug, with a message asking for a report).
- The value stack is balanced per statement: `Return` can truncate to
  the frame base and push one result.
- Every KIR instruction records the source line that produced it, so
  runtime errors point back at Kin code, not bytecode.

## Error code registry

| Range | Producer | Examples |
| --- | --- | --- |
| E1xxx | lexer | E1001 invalid character, E1002 unterminated string, E1003 bad escape, E1004 malformed number, E1005 int out of range, E1006 unterminated block comment |
| E2xxx | parser | E2001 expected expression, E2002 expected X found Y, E2003 unknown type, E2004 chained comparison, E2010/E2011 mode directive errors |
| E3xxx | checker | E3001 undefined name, E3002 duplicate, E3003 type mismatch, E3004 immutable reassign, E3005 non-bool condition, E3006 return misuse, E3007 arity, E3008 main rules, E3009 break/continue outside loop, E3010 void consumed, E3011 no implicit int/float, E3012 missing return, E3014/E3015 void param/var, E3016 print redefined |
| E4xxx | runtime | E4000 no main, E4001 division by zero, E4002 call stack overflow, E4003 output error, E4004 integer overflow, E4999 internal |
| W3xxx / N3xxx | checker | W3001 unreachable code, N3001 B-mode `var` lint, N3002 declared-here note |
| E21xx | Studio editor checks only | E2101 unmatched delimiter, E2102 unclosed delimiter, E2110 mode directive guidance (not part of the compiler) |

## Testing strategy

- Unit tests inside each module (lexer positions, parser shapes, every
  checker rule).
- Pipeline integration tests (`kinc/tests/pipeline.rs`): every
  `examples/*.kin` with an `// expect:` block is compiled and executed,
  and its full stdout is compared; error examples assert their declared
  code.
- CLI integration tests (`kinn/tests/cli.rs`) run the real binary and
  assert exit codes 0/1/2/3.
- Totals at 0.1.0: 53 kinc unit + 10 pipeline + 12 CLI + 15 kinrt =
  **90 tests**, plus 22 Studio tests.

## Native codegen (planned, 0.3)

The pipeline deliberately fronts a clean IR so the backend can be
replaced or extended. Candidates for the native backend investigation:
Cranelift, LLVM, or C transpilation with the platform toolchain. No
decision yet — ADR-0002 records the trade-offs and the 0.3 milestone
owns the decision.
