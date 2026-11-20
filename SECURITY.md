# Security Policy

## Reporting a vulnerability

Open a private security advisory on this repository (Security tab →
"Report a vulnerability"), or contact the repository owner directly.
Please do not open public issues for vulnerabilities.

## Threat model

KINN is offline-first: the compiler and Studio run entirely on your
machine. Nothing in this repository phones home, requires an account,
or transmits your code anywhere.

## Token and secret hygiene

- Never commit tokens, passwords or keys. This repository contains no
  secrets, and CI (when added) will enforce a secret scan.
- The bootstrap GitHub token used to create this repository was
  provided out-of-band by the owner and was never written to any file,
  commit, log or document. It is recommended practice to rotate any
  token shared over a chat channel after use.
- If you believe a secret has leaked, rotate it immediately at the
  provider — assume it is compromised, then clean history.

## Supply chain

- The Rust workspace (`compiler/`) has **zero external dependencies**.
  `cargo build` never touches the network. There is nothing to
  supply-chain-attack in the compiler itself.
- KINN Studio (`studio/`) has zero runtime dependencies; dev-only
  dependencies are `vite`, `typescript` and `vitest`, pinned through
  `package-lock.json`.
- Generated build artifacts (`compiler/target`, `studio/dist`,
  `studio/node_modules`) are gitignored and must never be committed.

## Compiler safety notes

- Kin 0.1 has no unsafe constructs, no FFI and no file/network access
  in the language; programs can loop forever but cannot escape the VM.
- The VM enforces a call-depth limit (E4002) and checked integer
  arithmetic (E4004). Float division by zero is an error, not `inf`
  (a documented deviation from IEEE-754, ADR-0009).
- Graphics work (0.5+) will introduce native code; a security review
  is part of that milestone's exit criteria.

## Scope

This policy covers the KINN repository (compiler, runtime, CLI, Studio,
documentation). Third-party forks are out of scope.
