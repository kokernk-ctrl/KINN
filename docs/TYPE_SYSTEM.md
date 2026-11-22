# Type System

The 0.1 type system is intentionally small and strict. Its job is to
prove a few things well — scoping, inference on declarations, and the
no-hidden-conversions rule — before the language grows.

## Types

| Type | Representation | Notes |
| --- | --- | --- |
| `int` | `i64` | literals are checked against the i64 range (E1005); arithmetic overflow is a runtime error (E4004) |
| `float` | `f64` | shortest round-trip printing; integral floats keep `.0` |
| `bool` | `true` / `false` | only type accepted by `if`/`while` conditions and `and`/`or`/`not` |
| `string` | UTF-8, immutable | `+` concatenates; `==`/`!=` compare by value |
| `void` | absence of a value | only a return type; parameters and variables cannot be void (E3014/E3015) |

Internally the checker also uses a poison type `Error` to suppress
cascading errors: after one error in an expression, dependents fail
quietly instead of piling on.

## Inference

Declaration-site inference only, from the initializer:

```kin
fin x = 1;              // int
fin y = 1.0;            // float
fin z: float = 2.0;     // annotated, must match (E3003 otherwise)
var flags = true;       // bool
```

There is no inference for function parameters or return types —
signatures are always explicit, so a function can be understood without
reading its body.

## The no-hidden-conversions rule

Binary arithmetic and comparisons require **identical** operand types:

```kin
1 + 2       // int
1.5 + 0.5   // float
"a" + "b"   // string
1 + 0.5     // ERROR E3011: no implicit conversion between int and float
1.0 + 0.5   // float — write the `.0`
```

The same rule applies to assignment, arguments and returns: a type
mismatch is `E3003`, and the int/float pair specifically is `E3011`
with a hint to use matching literals. Explicit casts (`x as float`) are
planned for 0.2 and will be the only way to convert.

## Scoping

- Blocks `{ ... }` introduce scopes; inner declarations may shadow
  outer ones with any type.
- Redeclaring in the *same* scope is `E3002`.
- `fin` bindings are immutable (`fin` = final value; the 0.1–0.3 keyword was `let`, renamed by ADR-0029); reassignment is `E3004` with an N3002
  note pointing at the declaration.
- Function signatures are collected before bodies are checked, so
  forward calls work; duplicate functions are `E3002`.

## Functions and void safety

- `main()` must exist, take no parameters and return void (E3008).
- Void values cannot be consumed: passing `print(...)` as an argument,
  initializing a variable with it, or using it in an operator are all
  errors (E3010/E3015). Calling a void function as a statement is fine.
- `print` is a builtin taking exactly one non-void argument (E3007) and
  cannot be redefined (E3016).
- Return paths: a non-void function whose body can reach its end is
  `E3012` (conservative analysis — see P-002: `while true` divergence
  is not yet recognized).

## Planned (not implemented)

- 0.2: explicit casts, arrays, `string` indexing/slicing decisions,
  string interpolation, `for` over ranges.
- Later: structs and enums, generics investigation, pattern matching —
  each gated by a design ADR in [DECISIONS.md](DECISIONS.md) before
  implementation.
