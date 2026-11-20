/**
 * The integrated terminal: a scrollback plus a real command loop.
 * Every command listed in `help` is implemented; anything that is not
 * ready yet says so honestly instead of pretending.
 */

import type { Vfs } from '../vfs';
import { lex } from '../syntax/lexer';
import { detectMode, quickCheck } from '../diagnostics/checks';

const BANNER = [
  { text: 'KINN Studio 0.1.0 — terminal shell', cls: 'accent' },
  { text: 'Offline-first. Files live in the in-memory "samples" project.', cls: 'dim' },
  { text: "Type 'help' for commands, 'syntax' for the Kin 0.1 cheat sheet.", cls: 'dim' },
];

export interface TerminalDeps {
  vfs: Vfs;
  onOpenFile: (name: string) => void;
  onProblems: (problems: ReturnType<typeof quickCheck>, fileName: string) => void;
}

export class Terminal {
  private output: HTMLElement;
  private history: string[] = [];
  private historyIdx = -1;

  constructor(
    output: HTMLElement,
    input: HTMLInputElement,
    private deps: TerminalDeps,
  ) {
    this.output = output;
    for (const line of BANNER) this.print(line.text, line.cls);
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        const cmd = input.value;
        input.value = '';
        this.exec(cmd);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        if (this.history.length > 0) {
          this.historyIdx = Math.max(0, this.historyIdx - 1);
          input.value = this.history[this.historyIdx] ?? '';
        }
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        this.historyIdx = Math.min(this.history.length, this.historyIdx + 1);
        input.value = this.history[this.historyIdx] ?? '';
      }
    });
  }

  print(text: string, cls?: string): void {
    const div = document.createElement('div');
    if (cls) div.className = cls;
    div.textContent = text;
    this.output.appendChild(div);
    this.output.parentElement?.scrollTo({ top: this.output.parentElement.scrollHeight });
  }

  printBlock(text: string, cls?: string): void {
    for (const line of text.replace(/\n$/, '').split('\n')) this.print(line, cls);
  }

  exec(raw: string): void {
    const cmd = raw.trim();
    const echo = document.createElement('div');
    echo.className = 'cmd';
    echo.textContent = cmd;
    this.output.appendChild(echo);

    if (cmd === '') {
      this.scroll();
      return;
    }
    this.history.push(cmd);
    this.historyIdx = this.history.length;

    const [name, ...args] = cmd.split(/\s+/);
    switch (name) {
      case 'help': this.help(); break;
      case 'version': this.print('KINN Studio 0.1.0 (shell) — compiler kinn 0.1.0 (Rust, local)'); break;
      case 'about': this.about(); break;
      case 'clear': this.output.replaceChildren(); break;
      case 'ls': this.ls(); break;
      case 'cat': this.cat(args); break;
      case 'open': this.open(args); break;
      case 'new': this.newFile(args); break;
      case 'tokens': this.tokens(args); break;
      case 'check': this.check(args); break;
      case 'mode': this.mode(); break;
      case 'syntax': this.syntax(); break;
      case 'run': this.run(); break;
      default:
        this.print(`unknown command '${name}' — type 'help'`, 'err');
    }
    this.scroll();
  }

  private scroll(): void {
    this.output.parentElement?.scrollTo({ top: this.output.parentElement.scrollHeight });
  }

  private help(): void {
    this.printBlock(
`commands:
  help              this list
  version           shell and compiler versions
  about             what KINN is
  clear             clear the scrollback
  ls                list files in the project
  cat <file>        print a file
  open <file>       open a file in the editor
  new <file>        create a new .kin file
  tokens [file]     print the token stream
  check [file]      run editor checks (lexical, balance, duplicates)
  mode              show the active file's programming mode
  syntax            Kin 0.1 syntax cheat sheet
  run               explain how to run programs today`);
  }

  private about(): void {
    this.printBlock(
`KINN is an offline-first programming language and development
environment. Kin 0.1 compiles through KIR to a stack VM; KINN Studio
is its terminal-inspired editor. No accounts, no cloud, no servers.
Compiler: compiler/ (Rust). Studio: studio/ (TypeScript).`,
      'out');
  }

  private ls(): void {
    for (const f of this.deps.vfs.list()) {
      this.print(`${f.dirty ? '*' : ' '} ${f.name}${f.name === this.deps.vfs.activeName ? '  (active)' : ''}`, 'out');
    }
  }

  private resolveFile(args: string[]): { name: string; content: string } | null {
    const name = args[0] ?? this.deps.vfs.activeName;
    const f = this.deps.vfs.open(name);
    if (!f) {
      this.print(`no such file: '${name}' (try 'ls')`, 'err');
      return null;
    }
    return { name: f.name, content: f.content };
  }

  private cat(args: string[]): void {
    const f = this.resolveFile(args);
    if (!f) return;
    const lines = f.content.split('\n');
    lines.forEach((l, i) => this.print(`${String(i + 1).padStart(3)} | ${l}`, 'out'));
  }

  private open(args: string[]): void {
    if (!args[0]) {
      this.print('usage: open <file>', 'err');
      return;
    }
    if (this.deps.vfs.setActive(args[0])) {
      this.deps.onOpenFile(args[0]);
      this.print(`opened ${args[0]}`, 'ok');
    } else {
      this.print(`no such file: '${args[0]}'`, 'err');
    }
  }

  private newFile(args: string[]): void {
    if (!args[0]) {
      this.print('usage: new <name.kin>', 'err');
      return;
    }
    if (this.deps.vfs.create(args[0])) {
      this.deps.onOpenFile(args[0]);
      this.print(`created ${args[0]}`, 'ok');
    } else {
      this.print(`cannot create '${args[0]}' (must match name.kin and not exist)`, 'err');
    }
  }

  private tokens(args: string[]): void {
    const f = this.resolveFile(args);
    if (!f) return;
    const { tokens } = lex(f.content);
    for (const t of tokens) {
      const label = t.kind === 'eof' ? 'EOF' : t.kind.toUpperCase();
      this.print(
        `${String(t.line).padStart(3)}:${String(t.col).padEnd(3)} ${label.padEnd(10)} ${t.kind === 'string' ? '"' + t.text.slice(1, -1) + '"' : t.text}`,
        'out',
      );
    }
  }

  private check(args: string[]): void {
    const f = this.resolveFile(args);
    if (!f) return;
    const problems = quickCheck(f.content);
    this.deps.onProblems(problems, f.name);
    if (problems.length === 0) {
      this.print(`OK: no editor-level issues found in ${f.name}`, 'ok');
      this.print('(full type checking runs via the kinc compiler — planned inside Studio for 0.2)', 'dim');
      return;
    }
    for (const p of problems) {
      this.print(`${p.severity} ${p.code}: ${p.message}  [${f.name}:${p.line}]`, p.severity === 'error' ? 'err' : 'warn');
    }
    this.print(`${problems.length} problem(s) — see the PROBLEMS panel`, 'warn');
  }

  private mode(): void {
    const f = this.resolveFile([]);
    if (!f) return;
    const m = detectMode(f.content);
    const meaning: Record<string, string> = {
      B: 'Beginner — reduced configuration, extra hints and lints',
      S: 'Standard — balanced defaults',
      A: 'Advanced — terse diagnostics, more technical control planned',
    };
    this.print(`mode ${m} — ${meaning[m]}`, 'accent');
    this.print('set with the first line: *mode = B; / S; / A;', 'dim');
  }

  private syntax(): void {
    this.printBlock(
`Kin 0.1 cheat sheet
-------------------
*mode = B;                    // B beginner, S standard, A advanced (first line)

func main() { }               // program entry (void, no params)
func add(a: int, b: int) -> int { return a + b; }

let x = 1;                    // immutable, type inferred
var count = 0;                // mutable
count += 1;                   // += and -=

if x > 0 { } else if x == 0 { } else { }
while x < 10 { break; continue; }

// types: int (64-bit) float (64-bit) bool string
// no implicit int/float conversion: write 2.0, not 2
print("text " + "concat");    // the one builtin
// operators: + - * / %  < > <= >= == !=  and or not
// comments: // line and /* block */`,
      'out');
  }

  private run(): void {
    this.print('running programs inside Studio is planned for 0.2', 'warn');
    this.print('today: build the compiler and run locally, fully offline:', 'out');
    this.print('  cd compiler && cargo build --release', 'out');
    this.print('  ./target/release/kinn run path/to/file.kin', 'out');
    this.print('details: docs/CLI_DESIGN.md', 'dim');
  }
}
