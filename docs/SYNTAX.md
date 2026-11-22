# Syntax — Kin 0.1 quick reference

Everything below is implemented and tested as of 0.1.0. The normative
grammar is in [LANGUAGE_SPEC_0.1.md](LANGUAGE_SPEC_0.1.md).

## Program structure

```kin
*mode = B;                       // optional, must be the first statement
                                 // B beginner, S standard (default), A advanced

func main() {                    // program entry: no parameters, returns void
    print("Hello, Kin!");
}
```

A minimal project is one file with `func main()` — nothing else. No
headers, no manifest, no build script.

## Functions

```kin
func add(a: int, b: int) -> int {
    return a + b;
}

func greet(name: string) -> string {
    return "Hello, " + name + "!";
}
```

- Parameter types are always written; the return type is `void` when
  `-> T` is omitted.
- Functions can call functions declared later in the file (signatures
  are collected first).
- `func main()` is required: no parameters, void return.

## Variables

```kin
fin x = 1;              // immutable (final value), type inferred (int)
fin ratio: float = 0.5; // immutable, annotated
var count = 0;          // mutable
count += 1;             // compound assignment: += and -=
x = 2;                  // ERROR: x is immutable (E3004)
```

- `fin` (a final value; renamed from `let`, ADR-0029) cannot be reassigned; `var` can. A legacy `let x = 1;` reports E2017.
- Type annotations are optional (inferred from the initializer).
- Same-scope redeclaration is an error; shadowing inside a nested
  block is allowed.

## Control flow

```kin
if x < 0 {
    print("negative");
} else if x == 0 {
    print("zero");
} else {
    print("positive");
}

var i = 0;
while i < 3 {
    if i == 1 {
        i += 1;
        continue;
    }
    print(i);
    i += 1;
}
```

- Conditions must be `bool` (E3005).
- `break` and `continue` work inside `while` loops only (E3009).
- Chained comparisons like `1 < 2 < 3` are a syntax error (E2004).

## Expressions and operators

| Category | Operators | Notes |
| --- | --- | --- |
| arithmetic | `+ - * / %` | same types only; `+` also concatenates strings |
| comparison | `< > <= >= == !=` | same-type operands, result `bool` |
| logic | `and or not` | short-circuit; bool operands only |
| unary | `-` | numeric negation |

Precedence (loosest to tightest): `or` < `and` < comparison <
`+ -` < `* / %` < unary < call. Parentheses override everything.

**There is no implicit int/float conversion.** `1 + 0.5` is an error
(E3011); write `1.0 + 0.5`. This is deliberate — see
[TYPE_SYSTEM.md](TYPE_SYSTEM.md).

## Types

| Type | Values | Notes |
| --- | --- | --- |
| `int` | 64-bit signed integers | overflow is a runtime error (E4004) |
| `float` | 64-bit IEEE floats | division by 0.0 is an error, not `inf` |
| `bool` | `true` / `false` | |
| `string` | UTF-8 text, immutable | `+` concatenates, `==` compares |
| `void` | nothing | return type only |

`print` formats values for humans: integral floats keep their decimal
point (`2.0` prints `2.0`), `0.1 + 0.2` prints `0.30000000000000004`.

## Comments

```kin
// line comment

/* block comment
   can span lines, does not nest */
```

## A complete example

```kin
*mode = A;

func fib(n: int) -> int {
    if n < 2 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

func main() {
    var i = 0;
    while i <= 10 {
        print(fib(i));
        i += 1;
    }
}
```

Run it: `kinn run fibonacci.kin`.
