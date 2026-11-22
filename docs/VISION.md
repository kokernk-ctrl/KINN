# Vision

KINN exists for one reason: **learning to program should not require the
internet, an account, or permission from anyone.**

A student with a cheap Android phone, a borrowed laptop, or a machine
that never goes online should be able to write real programs, get real
diagnostics, and build real things — entirely locally. That constraint
shapes every decision in this project.

## What KINN is

- **A small, explicit language.** Braces are required, keywords are
  readable (`func`, `fin`, `not`, `and`), semicolons end statements,
  and the compiler never converts types behind your back. What you read
  is what runs.
- **Three programming modes.** `*mode = B` for beginners (fewer
  decisions, more guidance), `S` for standard work, `A` for advanced
  control. One language, growing with the learner — not three
  languages to relearn.
- **A terminal-inspired studio.** Dark graphite, one orange accent,
  monospace everywhere, panels you can read like a console. The IDE
  respects the aesthetic of the terminal without requiring terminal
  skills.
- **A graphics path designed for phones first.** OpenGL ES 3.0 is the
  first backend target because it runs on Android and on the desktop
  GL implementations most learners already have. Vulkan comes later,
  after the first backend works — not before.

## What KINN refuses to be

- **Not a cloud product.** No mandatory accounts, no telemetry, no
  online-only compilation, no "sync" you cannot turn off.
- **Not a kitchen sink.** One graphics backend before a second. One
  backend target per milestone. Zero-dependency Rust where possible.
- **Not vaporware.** Documentation distinguishes implemented,
  experimental, planned and unsupported. No placeholder buttons that
  pretend to work. If a feature is not done, KINN says so, in the UI
  and in the docs.

## Honest constraints (stated up front)

- On Android, local compilation is bounded by device memory, storage
  and toolchain availability. The KIR runtime is pure, portable Rust
  and Android-buildable in principle; claiming that every desktop
  target cross-compiles on-device would be dishonest, so we do not
  claim it. Device verification is part of the 0.5 milestone.
- Native code generation is not implemented. Today every Kin program
  executes on the KIR virtual machine. The native backend
  investigation is milestone 0.3.

## Where this goes

0.1 (done) proves the spine: source → tokens → AST → checked → KIR →
execution, with tests as the arbiter of truth. 0.2 grows the language
(modules, arrays, casts, richer stdlib) and connects Studio to the real
compiler. 0.3 investigates native codegen. 0.5 opens the graphics path
with OpenGL ES 3.0. 1.0 is a preview you can learn with, offline, on
the machines you actually own.

The north star never changes: a language and a studio that treat *your
machine* as first-class and *the network* as optional.
