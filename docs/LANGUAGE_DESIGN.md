# Language Design

This document records the principles behind Kin's design and the
reasoning for the 0.1 surface. The normative grammar and semantics live
in [LANGUAGE_SPEC_0.1.md](LANGUAGE_SPEC_0.1.md); this is the *why*.

## Principles

1. **Explicit over clever.** Braces are mandatory. Statements end with
   `;`. Blocks introduce scopes. There is no significant whitespace and
   no automatic semicolon insertion: what you type is what the parser
   sees, and errors point at real positions.
2. **Readable keywords over symbol soup.** Boolean logic reads
   `and`, `or`, `not`. Mutable declarations say `var`; immutable ones
   say `let`. `func` introduces functions. A beginner can pronounce
   every construct in the language.
3. **No hidden conversions.** `1 + 0.5` is a type error (E3011), not a
   silent promotion. Mixing `int` and `float` forces the writer to
   state intent (`1.0 + 0.5`). Surprises are moved from runtime to
   compile time wherever possible.
4. **One obvious way.** There is one loop (`while`), one conditional
   (`if`/`else`), one declaration form per mutability, one builtin
   (`print`). Features earn their place by being needed, not by being
   familiar from other languages.
5. **Errors teach.** Diagnostics carry stable codes, source snippets
   with carets, and (in Beginner mode) hints and lints. An error that
   only says "syntax error" is a bug in KINN, not in the student.
6. **Small core, verified.** The 0.1 grammar fits on one page. Every
   construct is covered by tests; constructs that are not implemented
   are not pretended to exist.

## The 0.1 surface and why it stops there

- **Types:** `int` (i64), `float` (f64), `bool`, `string`, `void`.
  Fixed widths keep behavior identical everywhere. Strings are
  immutable values; concatenation is `+`.
- **No `for` loop yet.** `while` covers all iteration in 0.1 and keeps
  the grammar minimal; ranges and `for` arrive with arrays in 0.2.
- **No arrays, structs or user types yet.** The type checker's story
  (scoping, inference on declarations, no conversions) is proven on
  scalars first.
- **Chained comparisons are a syntax error** (`1 < 2 < 3`, E2004)
  rather than quietly meaning something surprising.
- **Integer division truncates toward zero; `%` takes the sign of the
  dividend** (same as Rust). Both are documented in the spec.
- **Checked arithmetic.** Integer overflow is a runtime error (E4004),
  not wraparound. Float division by zero is an error, not `inf`
  (ADR-0009) — beginners should meet a message, not a symbol.
- **`print` is the only builtin**, deliberately: input/output beyond
  stdout is a 0.2 stdlib question (and interacts with the offline
  Android story), not something to rush in 0.1.

## What "readable keywords" does not mean

It does not mean banning symbols that carry real meaning: `+ - * / %`,
comparisons, `->` for returns, `=` for assignment. It means preferring
words where words read better (`not x` over `!x`) and reserving
punctuation for arithmetic and structure.

## Programming modes

`*mode = B | S | A;` is a single, explicit, first-line directive — not
a build flag, not an IDE setting, not implicit. The program declares
the level of help it wants from the language. In 0.1 the modes change
diagnostic behavior (see [PROGRAMMING_MODES.md](PROGRAMMING_MODES.md));
their deeper differentiation is designed, not invented later: B removes
configuration, A exposes technical control. Nothing will ever make a
B program invalid in S or A — modes change guidance, not syntax.
