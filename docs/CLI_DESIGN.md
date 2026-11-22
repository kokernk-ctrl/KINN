# CLI Design

`kinn` is the compiler command line. It is hand-rolled with zero
argument-parsing dependencies (ADR-0001) and follows a simple contract:
**stdout is program output, stderr is diagnostics, exit codes carry the
outcome.**

## Commands

```
kinn run <file.kin>      compile and execute the program
kinn check <file.kin>    compile and report diagnostics (no execution)
kinn tokens <file.kin>   print the token stream           (debug)
kinn ast <file.kin>      print the parsed AST             (debug)
kinn kir <file.kin>      print the lowered KIR listing    (debug)
kinn version             print the version
kinn help                print help
```

Examples:

```bash
$ kinn run examples/hello.kin
Hello, Kin!
world
42

$ kinn check examples/errors/immutable.kin ; echo "exit: $?"
error[E3004]: cannot reassign immutable variable `x` (declared with `fin`)
  --> examples/errors/immutable.kin:10:5
   |
10 |     x = 2;
   |     ^
note[N3002]: `x` declared here
  --> examples/errors/immutable.kin:9:5
   |
 9 |     fin x = 1;
   |     ^
exit: 1

$ kinn kir examples/fibonacci.kin | head -6
KIR module (2 functions)

func main (0 params, 2 locals)
  0000  Const(0)           ; line 10
```

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | success (diagnostics, if any, were warnings/notes only) |
| 1 | compile diagnostics: at least one error (E1xxx/E2xxx/E3xxx) |
| 2 | usage error: unknown command, missing file, unreadable file |
| 3 | runtime error (E4xxx), reported with the Kin source line |

## Output conventions

- Program output (`run`) goes to **stdout**, unprefixed — safe to pipe.
- Diagnostics go to **stderr**, sorted by position, with caret
  snippets. In `*mode = A`, notes are suppressed for terse output.
- Debug commands print one line per token / AST node / instruction;
  formats are stable enough for tooling but not frozen until 1.0.
- `kinn check` prints `OK: no errors found (N diagnostics shown)` to
  stdout when the program compiles.

## Design rules

- No config files, no environment variables, no hidden state: a file
  path is the whole interface (offline-first, beginner-friendly).
- `*mode` in the source is authoritative; the CLI never overrides it.
  Flags that exist later (optimization, targets) will be additive and
  default to the mode-appropriate behavior.
- The CLI never pretends: unsupported operations exit 2 with a clear
  message rather than guessing.
- Exit codes are tested (`kinn/tests/cli.rs`), not folklore.

## Planned

- 0.2: `kinn new <name>` project scaffolding; `kinn fmt`;
  multi-file/module compilation as the language grows.
- 0.3: backend selection flags once a native backend exists.
- Not planned: anything that requires the network.
