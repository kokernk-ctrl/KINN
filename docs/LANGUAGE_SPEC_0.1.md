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
func fin var if else while return break continue
true false and or not
int float bool string void
```

`let` was renamed to `fin` in 0.4.0 (ADR-0029): `fin` declares a
**final (immutable) value**; the word `let` is no longer a keyword and
lexes as a plain identifier. A statement-position `let x = ...;`
produces the E2017 migration diagnostic. (Historical note: `let` was
the 0.1–0.3 spelling; see §C1a and ADR-0029.)

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
var_decl     := ('fin'|'var') IDENT (':' type)? '=' expr ';'
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

- `fin` binds immutably (`fin` = final value; 0.1–0.3 spelled this
  `let`, renamed by ADR-0029); `var` mutably. Reassigning a `fin` is
  E3004. A legacy `let x = ...;` is E2017 with the migration hint.
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

---

# Kin 0.3 additions (normative)

The declarative `website` module (ADR-0019) and the diagnostic codes
it introduces. Full element semantics and generated-output contract:
docs/BACKEND_WEB.md (web) and docs/BACKEND_ANDROID.md (android).

## B1. Website declarations

```
website_decl := "website" STRING "{" page* "}"        ; E2016 when the name is not a string
page        := "page" IDENT "{" element* "}"
element     := "title" STRING | "heading" STRING | "text" STRING
             | "button" STRING "open" IDENT | "link" STRING "open" IDENT
             | "image" STRING "alt" STRING | "divider"
```

- `website`, `page`, `title`, `heading`, `text`, `button`, `link`,
  `image`, `divider`, `open`, `alt` are **contextual keywords**: they
  are ordinary identifiers everywhere except in these positions. No
  0.2 program changes meaning.
- Top-level items after the imports are `website` declarations and
  `func` definitions in any order. A top-level `website` identifier is
  always parsed as a website declaration attempt.
- Elements are newline-separated declarative items; no semicolons.
- Valid programs carry **at most one** website (E3030) and only in the
  entry file (E3037). A website program does not need `func main()`;
  `kinn run` on a site-only program reports guidance (exit 1), never a
  fake execution.
- The mode directive is unchanged and keeps its semicolon. The website
  module is available in all three modes; modes do not fork the
  language.
- Lowering: the checker-validated website lowers to a
  target-independent `SiteModel` (ADR-0018). The first `title` element
  becomes the page title; additional titles demote to section
  headings (W3036). The grammar cannot express anything outside the
  element table, so every backend covers the model totally.

## B2. New diagnostic codes (complete for 0.3 additions)

| Code | Stage | Meaning |
| --- | --- | --- |
| E2016 | parser | website name is not a string literal |
| E2030 | parser | unknown page element |
| E3030 | checker | more than one website declaration |
| E3031 | checker | duplicate page name (case-insensitive) |
| E3032 | checker | navigation target is not a declared page |
| E3033 | checker | website name empty |
| E3034 | checker | image alt text empty (accessibility) |
| E3035 | checker | button/link label empty |
| E3036 | checker | image source empty |
| E3037 | checker | website declared in a non-entry file |
| E3038 | checker/FFI | build target used on a program without a website |
| W3035 | checker | page never linked from another page |
| W3036 | checker | duplicate `title` element (first wins) |
| W3037 | checker | empty `heading`/`text` content |

---

# Kin 0.4 additions (normative, in progress)

The first 0.4.0 language addition is cross-module value exports
(`const`; docs/DYNAMIC_WEB_ARCHITECTURE.md B.1, step S1, ADR-0028).
Later 0.4 sections will cover the `app` module grammar (S2) and its
semantics (S3) as they land.

## C1. Cross-module value exports (`const`)

```
const_decl := 'const' IDENT ':' type '=' const_expr ';'
```

- `const NAME: Type = expr;` declares a **compile-time constant** at
  top level. The type annotation is part of the grammar and is always
  present.
- `const` is a **contextual keyword** (ADR-0019 recipe): at top level a
  `const` identifier always starts a const-declaration attempt (the
  only other top-level items are `import`, `func` and `website`);
  everywhere else it remains an ordinary identifier, so `fin const = 1;`
  and `func f(const: int)` keep compiling.
- Constant expressions allow exactly: literals (int, float, bool,
  string), array literals of constant expressions, unary/binary
  operators, and references to consts declared **earlier in the same
  file**. Function calls, casts (`as`), indexing, and qualified reads
  are rejected (E3039). Empty array literals take the declared type
  (`const A: [int] = [];`).
- Arithmetic inside a constant expression follows VM rules (ADR-0008):
  integer overflow and division/modulo by zero are compile-time errors
  (E3039), and float division by zero is an error too — a constant can
  never encode a value the VM would reject at runtime.
- Exports: like functions, consts are exported implicitly. The
  declaring file reads its own consts by bare name; importers read them
  qualified as `module.NAME` (the KIR-level name is `module::NAME`,
  mirroring functions). Unknown modules E3020; unknown members E3021
  (with hints when the name is a function or a const).
- Name rules: a const must not collide with a builtin (E3016), a
  function of the same file, or another const of the same file (E3002).
  A const cannot have type void (E3015). Locals may shadow a const
  (locals win); assigning to a const is E3004 (immutable binding —
  ADR-0014's binding/contents distinction applies: element writes into
  a const **array** mutate the one shared array object, matching the
  language's array reference semantics).
- Lowering: constants **fold at lowering** — every read compiles to a
  plain `Const(value)` instruction; `Module` records the folded values
  under qualified names (no new KIR instructions), and `kinn kir`
  prints them.

## C1a. The binding-keyword rename: `let` → `fin` (0.4.0, ADR-0029)

**Owner design decision.** The local-binding keyword trio is now:

- **`fin`** — an immutable local (a **final value**; `let`'s former
  role). `fin x = 1;` — reassignment is E3004.
- **`var`** — a mutable local (unchanged).
- **`const`** — a top-level compile-time constant (C1, unchanged).

Semantics are unchanged: only the keyword changes. The rename is
**intentionally breaking for `let`-sources**: the word `let` is no
longer a keyword and lexes as a plain identifier, and a
statement-position `let x = ...;` (the legacy declaration shape:
`let` immediately followed by an identifier) produces **E2017**,
a positioned migration diagnostic — "`let` was renamed to `fin`;
use `fin`" — after which the parser continues parsing the
declaration as `fin`, so every legacy use reports exactly once and
the rest of the program parses normally. Handler bodies (B.6) are
covered by the same rule (they use the ordinary statement parser).
Outside the declaration pattern, `let` is a legal identifier
(`fin let = 1;` compiles), mirroring the ADR-0019 contextual-word
gates. This supersedes the A2 backwards-compatibility clause **for
this keyword only** (ADR-0029).

Token/AST surface: the token kind prints as `FIN` in `kinn tokens`;
`kinn ast` prints `fin x = 1;`. Codegen, KIR and the VM are
untouched — output-based goldens (0.3 web dist, DEB stdout) are
byte-identical across the rename.

## C2. New diagnostic codes (complete for 0.4 additions so far)

| Code | Stage | Meaning |
| --- | --- | --- |
| E3039 | checker | invalid constant expression (non-constant construct, or constant arithmetic overflow/division by zero) |
| E2017 | parser | `let` was renamed to `fin` (ADR-0029): a statement-position `let` followed by an identifier — the legacy local-declaration shape — is the migration diagnostic; the declaration then parses as `fin`. The word `let` stays a legal identifier elsewhere |
| E2031 | parser | unknown event name in `on` (supported: click, input, change, submit, focus, blur, key, keydown, keyup, pointerdown, pointerup) |
| E2032 | parser | malformed `each`/`when`/component instantiation syntax (missing `as`, positional prop args, missing `(`) |
| E2033 | parser | a view argument must be a string or an expression (the next token cannot start one) |
| E2034 | parser | unknown theme token (whitelist: accent, bg, surface, text, muted, font, radius, max_width) |
| E2035 | parser | unknown field kind (whitelist: text, number, date, email, password, textarea) |
| E2036 | parser | unknown field attribute (whitelist: label, placeholder, required, min_length, max_length, min_value, max_value, pattern) |
| E2037 | parser | malformed animation block (bad step/option word, unquoted animation name after enter/exit/hover/press/scroll/transition) |
| E2038 | parser | an element that cannot take handlers is followed by an event block |
| E2039 | parser | malformed input/field/`on` structure (`input` without `bind`, `field` without `kind`, `on key` without a filter, key filter on a non-keyboard event) |
| E3051 | checker | `app` declaration outside the entry file |
| E3052 | checker | both `website` and `app` declared, or more than one `app` |
| E3042 | checker | duplicate state/computed/prop name in one scope (app scope: state fields + computed; component scope: props + state fields; cross-kind collisions count) |
| E3055 | checker | malformed route path (must be relative and non-empty, no empty segments; each segment a literal of `A-Za-z0-9._-` or a unique `:name` parameter; `.`/`..` segments rejected) |
| E3056 | checker | duplicate route — the same match shape as an earlier route (literals equal, parameters at the same positions; parameter names may differ) |
| E3057 | checker | SPA without `not_found`, or `not_found` in MPA (including the default mode) |
| E3062 | checker | the app name must not be empty (trim check; mirrors E3033) |
| E3063 | checker | unknown app page target — any page-target position (`route "x" -> Missing`, `not_found Missing`, `open <Page>` on buttons/links in page and component views, nested views included); app-scoped sibling of E3032, never reuses it |
| E3064 | checker | duplicate page name inside an app — case-insensitive (mirrors E3031) |
| E3065 | checker | duplicate component name inside an app — case-sensitive (identifier semantics, matching function names) |
| E3066 | checker | unknown name in the app scope — a binding expression, handler statement, call or `input bind`/`field` target that resolves to no local, `each` loop variable, component prop/state, shared state, computed value, const, function or registered app builtin; app-scoped sibling of E3001, never reuses it (assigned in slice 3) |
| E3058 | checker | `param("…")` reads a route parameter no valid route declares (literal arguments only — non-literals are runtime data); anchored on the quoted argument, the hint lists the declared parameters (implemented in slice 3) |
| E3043 | checker | invalid assignment target inside an app: assigning to a prop, computed value or event builtin in a handler (`fin` locals and `const`s keep the frozen E3004), and `field`/`input bind` targets that are not state fields (live since slice 3) |
| W3041 | warning | a handler local (`fin`/`var`) shadows an app state field or component state field of the same name — legal, but called out so beginners see the shadow (live since slice 3) |

E2031–E2039 are frozen by the S2 parser test suite; E3051/E3052 are the
presence rules and E3042/E3055–E3057/E3062 the declaration-validation
rules — both frozen by the S2/S3 checker test suites (the remaining
semantic layer E3040–E3079 / W3040–W3049 arrives with the later S3
slices). E3039 sits outside those reserved ranges and is fully owned
by `const` (S1). E2017 likewise sits outside the app-grammar span
(E2017 was free: 0.1/0.2 took E2001–E2015 and E2099, 0.3 took E2016
and E2030, the app grammar E2031–E2039) and is fully owned by the
`let`→`fin` rename (ADR-0029), frozen by its parser/lexer/CLI test
suite.

S3 slice 1 rulings (B.11 authorizes implementation-time wording;
codes and meanings are frozen, tests pin the wording):

- **E3062 mirrors E3033 exactly**: trim check, the diagnostic anchored
  on the `app` keyword with the quoted name's width, a hint included.
  An unnamed app is not registered — its internals are not further
  checked — and `E3008` pairs with it exactly as it does for websites
  with an empty name.
- **E3055 anchors on the path string** (quote to quote). Route paths
  are relative: `"items/:id"`, not `"/items/:id"` (B.9). Literal
  segments are `A-Za-z0-9._-` — URL- and filename-safe, because MPA
  generates one file per route; `.` and `..` are rejected since they
  would escape the generated output directory. `:name` parameters are
  identifiers and unique within a path.
- **E3056 compares match shapes, not spellings**: `item/:id` and
  `item/:name` match the same addresses, so they are duplicates — no
  router could tell them apart. The diagnostic anchors on the later
  route and names the first (path and line).
- **E3057**: the default navigation mode is `mpa` (B.9/D3), so a
  `not_found` section without `navigation spa` is an error (anchored
  on the target); a SPA app without `not_found` errors on the app
  declaration with a hint.
- **E3042 scopes**: the app scope is state fields + computed values
  (`state` blocks merge); each component's scope is its props + its
  state fields. Cross-kind collisions count — a binding expression
  could not tell them apart. The diagnostic names the kind and line
  of the first declaration. Same names in different components, or a
  component name sharing an app-state name, are not duplicates
  (shadowing rules arrive with scope resolution).

S3 slice 2 rulings (P-029 code assignment, architect ruling
2026-11-22; codes and meanings frozen, tests pin the wording):

- **E3063 resolves every page-target position**: `route "path" ->
  Page`, `not_found Page`, and `open <Page>` on buttons and links
  anywhere in the app's views — pages and components, `when`/
  `otherwise`, `each` and `form` bodies included. It is the
  app-scoped sibling of E3032 (website-scoped, frozen by ADR-0011) —
  never reused. The diagnostic anchors on the target identifier with
  its width; the message words the position (`route "x" targets page
  …`, `` `not_found` targets page … ``, `button "Go" opens page …`,
  non-literal labels fall back to the element kind name). The hint
  lists the declared pages (`declared pages: A, B`; an app with no
  pages says so); a target that names a **component** says "'X' is a
  component; routes target pages" ahead of the list. A `not_found`
  rejected by E3057 (MPA) is not further resolved — the established
  "invalid declarations are not further checked" pattern (E3062,
  E3055). `input bind`/`field bind` targets are **state keys**, not
  page targets — their resolution is scope resolution (slice 3).
- **E3064/E3065 and the two-namespace rule**: pages and components
  are SEPARATE namespaces — a page and a component may share a name,
  because use-site position disambiguates (`<Card>(...)` instantiates
  the component; `open Card` / `route -> Card` targets the page).
  Within each namespace the name is unique: page names compare
  case-insensitively (mirrors E3031 — page slugs are lowercased by
  the backends), component names compare case-sensitively (identifiers,
  matching function names). Both diagnostics anchor on the section
  keyword of the later declaration (`page` → width 4, `component` →
  width 9) and name the first definition's line.
- **E3066 is assigned (slice 3); E3067–E3069 stay reserved** for the
  remaining S3 slices (handler subset, prop instantiation, computed
  cycles, stores, fields, animations — see B.11 and ROADMAP).

S3 slice 3 rulings (scope resolution and binding typing, audit
approved 2026-11-23; codes and meanings frozen, tests pin the
wording):

- **E3066 siblings E3001, never reuses it**: inside the app semantic
  pass, an unknown name — a read in a binding expression or handler
  statement, a call target, an assignment target naming nothing, or
  an `input bind`/`field` target naming nothing — reports E3066 with
  a hint listing what the current scope declares (app state,
  computed values; props and component state first inside
  components). Ordinary function bodies keep E3001 (frozen,
  ADR-0011).
- **Resolution order, innermost first**: handler local → event
  builtins → `each` loop variable (in views) → component props/state
  (in components) → shared state → computed values → top-level consts
  → functions and natives → registered app builtins. **User
  definitions shadow app builtins** (the builtin table is a fallback
  consulted after user functions and the global natives; no
  redefinition guard exists), and a component prop or state field
  legally shadows app state (E3042 rejects duplicates only within
  one scope — the slice-1 ruling).
- **E3043 vs E3004 (the B.4 erratum)**: assigning to a prop, computed
  value or event builtin in a handler is E3043; `fin` locals and
  `const`s keep the frozen E3004. `field`/`input bind` targets must
  be state keys (shared state in pages, component state in
  components) — a computed value, prop or const in that position is
  E3043 (two-way binding writes); an unknown name is E3066.
- **E3058**: `param("…")` with a **literal** argument must name a
  `:param` segment of some valid route (malformed paths contribute
  nothing — the established E3055 pattern); non-literal arguments are
  runtime data and pass.
- **W3041**: a handler local shadowing an app state field or component
  state field warns (B.4); shadowing computed values, props or consts
  stays silent (locals shadow consts, the ordinary rule).
- **Binding typing reuses the ordinary codes**: element text
  (`text`/`heading`/`title`, labels, `alt`, form legends, field
  `label`/`placeholder`/`pattern`) must be `string`; `when`
  conditions `bool`; `each` iterables arrays; field
  `min_length`/`max_length`/`min_value`/`max_value` must be `int` —
  mismatches are the frozen E3003/E3011 diagnostics (E3011 keeps its
  int↔float message), and `when` never borrows E3005. State-field and
  prop initializers type against their declared types, empty array
  literals against a declared array type included.
- **App builtins (the A2 ruling), names and types only**:
  `param(string) -> string`; `store_save`/`store_load`/`store_clear`
  `(string) -> void` (commands); `json_of(string) -> string`;
  `save_json(string, string) -> bool` (B.7); `str(int|float|bool|
  string) -> string` (mirrors the `as string` cast surface; arrays
  are rejected); `upper`/`lower`/`trim(string) -> string`;
  `contains(string, string) -> bool`; `split(string, string) ->
  [string]`; `join([string], string) -> string` (B.10); and the
  handler event variables `event_value: string`, `event_key:
  string`, `event_x`/`event_y: int`, `form_valid: bool`,
  `field_errors: [string]` (B.6), registered for every handler.
  **Known limit: param/store names recognized; context-restricted
  invocation enforced at S6.** The B.10 helpers' VM/JS natives arrive
  with S5 — app programs cannot build or run before S4+ anyway, so
  nothing silently executes.
- **Array `+`/`+=` inside app expressions only (the B.7 example
  ruling)**: `items + [draft]` and `xs += [x]` (same element type)
  type-check inside the app semantic pass — B.7's normative example
  appends with `+`. Execution lands with S5; console programs keep
  the 0.2 rule (array `+` stays E3003 outside apps).
- **Computed values publish their type in source order**; references
  to later computed values resolve by name but poison silently until
  the E3044 (cycles) slice — names always resolve, cycles never
  cascade as unknowns.
- **Not in this slice (P-030, narrowed)**: component names and prop
  signatures at instantiation sites (E3040/E3041 — the argument
  expressions themselves do resolve), the handler subset (E3045/
  E3046, W3040 — `return;` is accepted like in any void body until
  then), computed cycles (E3044), stores/fields/animations
  (E3047–E3061), W3042–W3044, N3040; E3067–E3069 stay reserved.

## C3. The `app` declaration — dynamic web module, grammar (0.4.0 S2)

`app` is the dynamic sibling of `website` (ADR-0024). **S2 delivered
the grammar; S3 slice 1 adds declaration validation** — E3062 (empty
app name), E3055–E3057 (route rules) and E3042 (duplicate
state/computed/prop names per scope), all registered in §C2. **Slice 2
resolves app page targets** — E3063 (unknown page target at every
page-target position: routes, `not_found`, `open` on buttons/links in
page and component views) and the page/component name rules E3064
(case-insensitive) / E3065 (case-sensitive); pages and components are
separate namespaces. **Slice 3 resolves names**: binding expressions,
handler bodies, computed values and state/prop initializers
type-check against the app scope (locals, `each` loop variables,
component props/state, shared state, computed values, consts,
functions and the registered param/store/B.10 builtins — the §C2
slice-3 rulings), unknown names report E3066, `field`/`input bind`
targets must be state keys, `param("…")` must name a declared route
parameter (E3058), props/computed values/event builtins are
read-only in handlers (E3043), a handler local shadowing state warns
W3041, and binding typing reuses the ordinary E3003/E3011 mismatch
codes. An app program parses, `kinn ast` shows its tree, the presence
rules (E3051/E3052, no `main` required) and the slice 1–3 rules
hold. **Still unchecked, honestly**: component names and prop
signatures at instantiation sites (E3040/E3041), handler subset
enforcement (E3045/E3046, W3040), computed cycles (E3044), and
store/field/animation semantics (E3047–E3061, W3042–W3044, slice 4+
— E3067–E3069 stay reserved; **param/store names recognized;
context-restricted invocation enforced at S6**). Every other command
says honestly that dynamic builds are not implemented yet. Lowering
arrives with S4.

```
app_decl      := "app" STRING "{" app_section* "}"            ; E2016 when the name is not a string
app_section   := navigation | not_found | theme | state | computed
               | animation | store | route | transition | page | component
navigation    := "navigation" ("mpa" | "spa")                 ; default: mpa
not_found     := "not_found" IDENT
theme         := "theme" "{" theme_token* "}"
theme_token   := TOKEN STRING          ; TOKEN ∈ {accent,bg,surface,text,muted,font,radius,max_width} — E2034
state         := "state" "{" state_field* "}"                 ; app-level: a block
state_field   := IDENT ":" type "=" expr ["," | ";"]          ; type mandatory, initializer mandatory
computed      := "computed" IDENT "=" expr
animation     := "animation" "{" (keyframes | transition_decl)* "}"
keyframes     := "keyframes" STRING "{" kf_item* "}"
kf_item       := "from" anim_props | "to" anim_props
               | "step" expr anim_props
               | "duration" expr | "delay" expr | "easing" expr
anim_props    := "{" (IDENT expr)* "}"
transition_decl := "transition" STRING "{" ("duration" expr | "delay" expr | "easing" expr)* "}"
store         := "store" STRING "{" IDENT ["," IDENT]* "}"
route         := "route" STRING "->" IDENT
transition    := "transition" STRING              ; app-level page/route transition
page          := "page" IDENT "{" view_element* "}"
component     := "component" IDENT "{" comp_section* "}"
comp_section  := prop | comp_state | view
prop          := "prop" IDENT ":" type ["=" expr]
comp_state    := "state" IDENT ":" type "=" expr  ; component-level: one per line, like prop
view          := "view" "{" view_element* "}"
```

View elements (B.5–B.8):

```
view_element  := "title" vexpr anim*
              | "heading" vexpr anim* | "text" vexpr anim* | "divider" anim*
              | "button" vexpr ["open" IDENT] anim* handlers    ; E2038: no block after text/heading/title/divider
              | "link" vexpr ["open" IDENT] anim* handlers
              | "image" vexpr "alt" vexpr anim* handlers
              | "input" "bind" IDENT anim* handlers             ; E2039 without bind
              | "when" vexpr "{" view_element* "}" ["otherwise" "{" view_element* "}"]
              | "each" vexpr "as" IDENT ["key" vexpr] "{" view_element* "}"   ; E2032 without as
              | IDENT ["." IDENT] "(" [prop_arg {"," prop_arg}] ")" anim* handlers  ; component instantiation
              | "form" vexpr "{" (view_element | on_event)* "}"
              | "field" IDENT "kind" field_kind ["{" (field_attr | on_event)* "}"] anim*
vexpr         := expr                    ; E2033 when the next token cannot start an expression
anim          := ("enter" | "exit" | "hover" | "press" | "scroll") STRING   ; E2037 unquoted
handlers      := ["{" on_event* "}"]     ; only interactive elements; E2038 otherwise
on_event      := "on" event_word [STRING] "{" stmt* "}"   ; E2031 unknown word; E2039 bad key filter
event_word    := click | input | change | submit | focus | blur | key | keydown | keyup | pointerdown | pointerup
field_kind    := text | number | date | email | password | textarea      ; E2035
field_attr    := ("label" | "placeholder" | "min_length" | "max_length" | "min_value" | "max_value" | "pattern") expr
               | "required"                                   ; E2036 unknown attribute
prop_arg      := IDENT ":" expr                              ; E2032 positional args
```

Normative rules:

- **Contextual keywords only** (ADR-0019 recipe). Every word above —
  `app`, `component`, `state`, `computed`, `view`, `prop`, `each`,
  `when`, `otherwise`, `on`, `route`, `store`, `animation`,
  `keyframes`, `from`, `to`, `step`, `duration`, `delay`, `easing`,
  `enter`, `exit`, `hover`, `press`, `scroll`, `transition`,
  `navigation`, `mpa`, `spa`, `not_found`, `form`, `field`, `kind`,
  `bind`, `theme`, the field attributes, the event words, the theme
  tokens — remains a legal identifier outside these grammar positions.
  The test `app_words_stay_identifiers_outside_the_declaration` is the
  regression gate (mirrors the 0.3 website gate); it freezes all 79
  contextual words — the B.0 list, the event words, the theme tokens,
  the field attributes, the field `kind` values and the shared
  view-element words.
- **Handler bodies are official Kin statements** (B.6): parsed by the
  ordinary statement parser — semicolons required, conditionals are
  `if` (views use `when`; the two never blur, B.5).
- **`each` and casts**: `as` is the cast operator, so a cast in an
  `each` iterable needs parentheses (`each (vals as [string]) as v`).
  A bare `as` followed by an identifier is always the loop-variable
  marker, never a cast. Outside `each`, `x as some_ident` is a
  positioned E2002 on the `as` with a hint naming both readings
  (loop marker vs cast operator; casts need a type keyword).
- **Component state vs app state**: component `state` is one
  declaration per line (like `prop`); app `state` is a `{ ... }` block.
- **Qualified component instantiation** uses the established dot form
  `alias.Name(props)` — the same convention as calls and const reads
  (the `alias::Name` spelling in the architecture doc is informal).
- **Types are the standard Kin types**: `int`, `float`, `bool`,
  `string`, `[T]` (the capitalized `Str`/`Bool` in the architecture
  doc examples is informal notation).
- **Position rules**: one `website` or one `app` per program, entry
  file only (E3051/E3052); app programs do not need `main` (B.2).
- **Grammar error recovery**: sections, views, flat blocks (state,
  theme, store, field attributes) and keyframe blocks each recover to
  their own closing `}`, so one broken item never truncates its parent
  (regression-tested).
