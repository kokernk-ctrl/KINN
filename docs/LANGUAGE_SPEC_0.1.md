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
