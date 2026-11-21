# Kin Language Specification — 0.1

Normative reference for Kin 0.1. Everything here is implemented and
tested in this repository; where behavior is limited, the limitation is
normative (and tracked in [PROBLEM_REGISTRY.md](PROBLEM_REGISTRY.md)).
The next version will supersede this file, not amend it silently.

## 1. Source files

- Encoding: UTF-8. File names are not significant to the compiler.
- Lines and columns are 1-based; a tab counts as one column (P-011).
- A program is a single file (multi-file modules: 0.2).

## 2. Lexical grammar

```
digit        := '0'..'9'
letter       := 'A'..'Z' | 'a'..'z' | '_'
ident        := letter (letter | digit)*        ; ASCII only
int_lit      := digit+                          ; must fit i64 (E1005)
float_lit    := digit+ '.' digit+               ; no exponent form in 0.1
string_lit   := '"' char* '"'
               ; escapes: \n \t \r \" \\  (E1003 otherwise)
               ; may not contain a raw newline (E1002)
comment_line := '//' until end of line
comment_blk  := '/*' ... '*/'                   ; non-nesting (E1006 if unterminated)
```

Keywords (cannot be identifiers):

```
func let var if else while return break continue
true false and or not
int float bool string void
```

Operators and punctuation:

```
*  +  -  /  %  =  +=  -=  ==  !=  <  >  <=  >=  ->
(  )  {  }  ,  :  ;
```

A standalone `!` is an error (E1001) with a hint to use `not`/`!=`.

## 3. Grammar

```
program      := mode_decl? func_decl*
mode_decl    := '*' 'mode' '=' ('B'|'S'|'A') ';'      ; must be first
func_decl    := 'func' IDENT '(' params? ')' ('->' type)? block
params       := IDENT ':' type (',' IDENT ':' type)*
type         := 'int' | 'float' | 'bool' | 'string' | 'void'
block        := '{' stmt* '}'

stmt         := var_decl | assign | if_stmt | while_stmt | return_stmt
              | 'break' ';' | 'continue' ';'
              | expr ';' | block
var_decl     := ('let'|'var') IDENT (':' type)? '=' expr ';'
assign       := IDENT ('='|'+='|'-=') expr ';'
if_stmt      := 'if' expr block ('else' (if_stmt | block))?
while_stmt   := 'while' expr block
return_stmt  := 'return' expr? ';'

expr         := or_expr
or_expr      := and_expr ('or' and_expr)*
and_expr     := cmp_expr ('and' cmp_expr)*
cmp_expr     := add_expr (cmp_op add_expr)?        ; at most one (E2004)
add_expr     := mul_expr (('+'|'-') mul_expr)*
mul_expr     := unary (('*'|'/'|'%') unary)*
unary        := ('not'|'-') unary | primary
primary      := int_lit | float_lit | string_lit | 'true' | 'false'
              | IDENT | IDENT '(' args? ')' | '(' expr ')'
args         := expr (',' expr)*
```

Notes:

- The mode directive is only valid as the first statement; a `*` token
  elsewhere is a syntax error (E2001 from the parser).
- Chained comparisons (`1 < 2 < 3`) are rejected (E2004).
- `else if` chains desugar to an `if` in the else branch.

## 4. Semantics

### 4.1 Declarations

- `let` binds immutably; `var` mutably. Reassigning a `let` is E3004.
- Types are inferred from the initializer when the annotation is
  absent; when present they must match exactly (E3003).
- Redeclaration in the same scope is E3002. Inner blocks may shadow.
- `void` cannot be the type of a parameter (E3014) or variable (E3015).

### 4.2 Functions

- Signatures are collected before bodies are checked; forward calls
  are allowed. Duplicate names are E3002.
- `main` must exist, take no parameters, and return void (E3008).
  Execution starts at `main`; the process exit code is 0 on normal
  completion.
- Arity mismatches are E3007; argument type mismatches E3003/E3011.
- `print` is a builtin: exactly one non-void argument, returns void,
  cannot be redefined (E3016). It prints the value followed by `\n`.

### 4.3 Types and operators

| Operands | Result |
| --- | --- |
| `int ⊕ int` (`+ - * / %`) | `int` (division truncates toward zero; `%` takes the dividend's sign) |
| `float ⊕ float` (`+ - * /`) | `float` |
| `string + string` | concatenation |
| same numeric type, `< > <= >=` | `bool` |
| same type, `== !=` | `bool` (strings compare by value) |
| `bool` `and`/`or` | `bool`, short-circuit |
| `not bool` | `bool` |
| `-` on numeric | negation |

Any other operand combination is a type error (E3003; E3011
specifically when `int` meets `float`).

### 4.4 Control flow

- `if`/`while` conditions must be `bool` (E3005).
- `break`/`continue` only inside loops (E3009).
- A non-void function whose body can reach its end is E3012
  (conservative: `while true` divergence not recognized — P-002).
- A `return` with a value in a void function, without one in a non-void
  function, or with the wrong type, is E3006/E3003.

### 4.5 Runtime

- Integer overflow: E4004. Division/modulo by zero: E4001 (also for
  floats — ADR-0008). Call depth over 1024 frames: E4002.
- Float printing: shortest round-trip; integral floats keep `.0`.
- Programs cannot access files, the network, or the clock.

## 5. Modes

`*mode` selects diagnostic behavior (see
[PROGRAMMING_MODES.md](PROGRAMMING_MODES.md)): B adds the N3001 lint
and shows everything; S is the default; A suppresses notes. Syntax and
semantics are identical across modes.

## 6. Diagnostic codes (complete registry)

**Lexical (lexer):** E1001 unexpected character, E1002 unterminated
string, E1003 invalid escape, E1004 malformed number, E1005 integer out
of range, E1006 unterminated block comment.

**Syntax (parser):** E2001 unexpected token / expected expression,
E2002 expected X but found Y, E2003 expected a type, E2004 chained
comparison, E2010 expected `mode` after `*`, E2011 invalid mode value.

**Semantic (checker):** E3001 undefined name, E3002 duplicate
definition, E3003 type mismatch, E3004 immutable reassignment, E3005
non-bool condition, E3006 return misuse, E3007 wrong argument count,
E3008 main rules, E3009 break/continue outside loop, E3010 void value
consumed, E3011 no implicit int/float conversion, E3012 missing return,
E3014 void parameter, E3015 void variable, E3016 print redefined.
Warnings: W3001 unreachable code. Notes: N3001 (B-mode var lint),
N3002 declared-here.

**Runtime (kinrt):** E4000 no main, E4001 division/modulo by zero,
E4002 call stack overflow, E4003 output error, E4004 integer overflow,
E4999 internal error (compiler bug).

**Studio editor checks (not the compiler):** E2101 unmatched delimiter,
E2102 unclosed delimiter, E2110 mode-directive guidance; plus mirrors
of E1001–E1006 and E3002.

## 7. Limits

| Limit | Value |
| --- | --- |
| `int` | i64; literals must fit (E1005) |
| call depth | 1024 frames |
| string literal | must fit on one line |
| identifiers | ASCII letters, digits, `_` |

---

# Kin 0.2 additions

Normative additions shipped in KINN 0.2.0 on top of the 0.1
specification above. The 0.1 rules remain valid except where amended
below. Everything here is implemented and tested.

## A1. Multi-file programs (modules)

```
import_decl := 'import' string_lit 'as' ident ';'
```

- Import declarations appear after the optional mode directive and
  before all functions (E2012 otherwise). The path must end in `.kin`,
  be relative, and stay inside the entry file's directory (E3017
  otherwise; no `..`, no absolute paths, no `\`).
- Paths resolve against the importing file's directory. Circular
  imports are errors reporting the full chain (E3018); chains deeper
  than 32 files are rejected; a file imported twice is parsed once.
- Modules export all top-level functions. Calls are qualified:
  `math.add(1, 2)` where `math` is the import alias (E3020 unknown
  module, E3021 unknown function). An imported file's `main` is inert;
  only the entry's `main` runs. Duplicate aliases in one file are
  errors (E3019).
- Cross-file globals and selective/re-named exports do not exist in
  0.2 (ADR-0015).

## A2. Arrays

```
array_type  := '[' type ']'
array_lit   := '[' expr (',' expr)* ']' | '[' ']'   ; empty needs an
                                                      ; annotation (E3022)
index_expr  := postfix '[' expr ']'
index_assign:= postfix '[' expr ']' '=' expr ';'
```

- Arrays have **reference semantics** (ADR-0014): assignment and
  parameter passing share the array. `==`/`!=` compare contents deeply.
- Elements must have one type (E3028). Indexing requires an `int`
  index (E3023) and an array base (E3024). `a[i] = v;` assigns an
  element; compound assignment to elements is rejected (E2014), as is
  assignment to any other expression (E2013).
- Runtime: bounds are checked — index out of `[0, len)` is E4005 with
  the length in the message. `len(array)` is the element count;
  `len(string)` counts Unicode characters (not bytes).

## A3. `for` loops and ranges

```
for_stmt := 'for' ident 'in' for_iter block
for_iter := expr '..' expr        ; exclusive
          | expr '..=' expr       ; inclusive
          | expr                  ; must be an array (E3026)
```

- Range bounds must be `int` (E3027). Ranges exist only in `for`
  headers; elsewhere `..` is a syntax error.
- The iterable expression is evaluated once. The loop variable is a
  fresh immutable binding each iteration (assignment is E3004); it is
  not visible outside the loop (E3001).
- `break` and `continue` work as in `while`; `continue` proceeds to
  the increment.

## A4. Casts

```
cast_expr := cast_expr 'as' type      ; binds tighter than binary ops,
                                      ; looser than unary (like Rust)
```

Supported casts (checked at compile time, E3025 otherwise — including
same-type casts, which are "unnecessary"):

| From | To | Semantics |
| --- | --- | --- |
| int | float | exact widening |
| float | int | truncate toward zero; saturate at i64 bounds; NaN → 0 |
| int | string | decimal digits |
| float | string | same formatting as `print` (`2.0` keeps the point) |
| bool | string | `true` / `false` |

String→number casts do not exist in 0.2 (P-013).

## A5. Native functions

`print` (as in 0.1) plus a native table, all reserved against
redefinition (E3016):

| Name | Signature | Notes |
| --- | --- | --- |
| `len` | `(string) -> int`, `([T]) -> int` | string length counts characters |
| `abs` | `(int) -> int`, `(float) -> float` | `abs(i64::MIN)` overflows (E4004) |
| `min` / `max` | `(T, T) -> T`, T in {int, float} | both operands same type |

## A6. Parser recovery

The parser reports multiple syntax errors per file: after a failing
statement it skips to `;`, `}`, a statement keyword, or a top-level
`func`/`import`, then continues. After 25 errors it stops with E2099.
Compilation still fails on any error — recovery improves diagnostics,
never accepts broken input.

## A7. New diagnostic codes (complete for 0.2 additions)

| Code | Stage | Meaning |
| --- | --- | --- |
| E2012 | parser | `import` after functions |
| E2013 | parser | invalid assignment target |
| E2014 | parser | compound assignment to an array element |
| E2015 | parser | malformed import (path not a string) |
| E2099 | parser | too many syntax errors |
| E3017 | project | import path invalid/unloadable |
| E3018 | project | circular or too-deep imports |
| E3019 | project | duplicate import alias |
| E3020 | checker | unknown module in qualified call |
| E3021 | checker | unknown function in module |
| E3022 | checker | empty array literal without annotation |
| E3023 | checker | array index not int |
| E3024 | checker | indexing a non-array |
| E3025 | checker | unsupported/unnecessary cast |
| E3026 | checker | `for` iterable not a range or array |
| E3027 | checker | range bound not int |
| E3028 | checker | mixed element types in array literal |
| E4005 | runtime | array index out of bounds |
| E5000 | ffi | compiler panic caught at the FFI boundary |
| E5002 | ffi | JNI bridge error |
| E5003 | host | malformed FFI result |
