/**
 * The integrated terminal: a scrollback plus a real command loop.
 * Every command listed in `help` is implemented; anything that is not
 * ready yet says so honestly instead of pretending.
 *
 * Since 0.2.0 `check` and `run` use the real compiler and VM through the
 * WebAssembly backend (compiler/crates/kinn-ffi compiled to kinn.wasm).
 * Since 0.3.0 `build web` and `build android` run the same target
 * backends as the CLI through the wasm bridge; `preview` renders the
 * generated site in the preview panel. When the backend is not loaded,
 * they fall back to editor-level checks and say so.
 */

import type { Vfs } from '../vfs';
import { lex } from '../syntax/lexer';
import { detectMode, quickCheck } from '../diagnostics/checks';
import { backend, type BuildResult, type OutputFile } from '../wasm/kinnwasm';
import { serializeProject } from '../storage/storage';

const BANNER = [
  { text: 'KINN Studio 0.3.0 — terminal shell', cls: 'accent' },
  { text: 'Offline-first. The project persists in this browser.', cls: 'dim' },
  { text: "Type 'help' for commands, 'syntax' for the cheat sheet, 'build web' to compile a site.", cls: 'dim' },
];

export interface TerminalDeps {
  vfs: Vfs;
  onOpenFile: (name: string) => void;
  onProblems: (problems: ReturnType<typeof quickCheck>, fileName: string) => void;
  /** Trigger the host file picker for `import`. */
  onImportRequest?: () => void;
  /** A finished build (web or android) — the host stores it for preview/export. */
  onBuildResult?: (result: BuildResult, target: 'web' | 'android') => void;
  /** `preview` was requested with the given build files. */
  onPreview?: (files: OutputFile[]) => void;
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
      case 'version': this.version(); break;
      case 'about': this.about(); break;
      case 'clear': this.output.replaceChildren(); break;
      case 'ls': this.ls(); break;
      case 'cat': this.cat(args); break;
      case 'open': this.open(args); break;
      case 'new': this.newFile(args); break;
      case 'rm': this.rm(args); break;
      case 'mv': this.mv(args); break;
      case 'tokens': this.tokens(args); break;
      case 'check': this.check(args); break;
      case 'mode': this.mode(); break;
      case 'syntax': this.syntax(); break;
      case 'run': this.run(args); break;
      case 'save': this.save(); break;
      case 'export': this.exportProject(); break;
      case 'import': this.importProject(); break;
      case 'build': this.build(args); break;
      case 'preview': this.preview(); break;
      default:
        this.print(`unknown command '${name}' — type 'help'`, 'err');
    }
    this.scroll();
  }

  private lastBuild: { result: BuildResult; target: 'web' | 'android' } | null = null;

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
  rm <file>         delete a file
  mv <old> <new>    rename a file
  tokens [file]     print the token stream
  check [file]      full compiler check (real diagnostics via kinn.wasm)
  mode              show the active file's programming mode
  syntax            Kin cheat sheet (incl. the website module)
  run [file]        compile and execute (real VM, output below)
  save              persist the project now (also automatic)
  export            download the project as a .kinnproj file
  import            load a .kinnproj or .kin file from disk
  build [target]    generate artifacts: web (default) or android
  preview           show the last web build in the preview panel`);
  }

  private version(): void {
    const compiler = backend.available
      ? `compiler kinn ${backend.version()} (Rust via WebAssembly, local)`
      : 'compiler backend NOT LOADED (editor checks only; run npm run build:wasm)';
    this.print(`KINN Studio 0.3.0 (shell) — ${compiler}`);
  }

  private about(): void {
    this.printBlock(
`KINN is an offline-first programming language and development
environment. Kin 0.3 compiles console programs through KIR to a stack
VM and builds declarative websites into real HTML/CSS with the web
backend. In-Studio check, run and build use the same Rust compiler as
the CLI, compiled to WebAssembly. No accounts, no cloud, no servers.`,
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

  private rm(args: string[]): void {
    if (!args[0]) {
      this.print('usage: rm <file>', 'err');
      return;
    }
    if (this.deps.vfs.remove(args[0])) {
      this.print(`deleted ${args[0]}`, 'ok');
      this.deps.onOpenFile(this.deps.vfs.activeName);
    } else {
      this.print(`no such file: '${args[0]}'`, 'err');
    }
  }

  private mv(args: string[]): void {
    if (args.length < 2) {
      this.print('usage: mv <old-name.kin> <new-name.kin>', 'err');
      return;
    }
    if (this.deps.vfs.rename(args[0], args[1])) {
      this.print(`renamed ${args[0]} -> ${args[1]}`, 'ok');
      this.deps.onOpenFile(this.deps.vfs.activeName);
    } else {
      this.print(`cannot rename '${args[0]}' to '${args[1]}' (missing, invalid or exists)`, 'err');
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

  /** Full compiler check through the wasm backend (fallback: editor checks). */
  check(args: string[]): void {
    const f = this.resolveFile(args);
    if (!f) return;
    if (!backend.available) {
      this.print('compiler backend not loaded — falling back to editor-level checks', 'warn');
      this.print('(build it with: cd compiler && cargo build --release -p kinn-ffi --target wasm32-unknown-unknown)', 'dim');
      this.editorCheck(f.name, f.content);
      return;
    }
    const result = backend.check(f.name, this.deps.vfs.projectFiles());
    this.deps.onProblems(result.diagnostics, f.name);
    if (result.ok) {
      const notes = result.diagnostics.filter((d) => d.severity !== 'error').length;
      this.print(
        `OK: no errors in ${f.name}${notes > 0 ? ` (${notes} note(s)/warning(s) shown)` : ''}`,
        'ok',
      );
      for (const d of result.diagnostics) {
        this.print(`${d.severity} ${d.code}: ${d.message}  [${d.file}:${d.line}]`, d.severity === 'error' ? 'err' : 'warn');
      }
    } else {
      for (const d of result.diagnostics) {
        if (d.severity !== 'error') continue;
        this.print(`${d.code}: ${d.message}  [${d.file}:${d.line}:${d.col}]`, 'err');
      }
      this.print(`${result.diagnostics.filter((d) => d.severity === 'error').length} error(s) — see the PROBLEMS panel`, 'warn');
    }
  }

  private editorCheck(name: string, content: string): void {
    const problems = quickCheck(content);
    this.deps.onProblems(problems, name);
    if (problems.length === 0) {
      this.print(`OK: no editor-level issues found in ${name}`, 'ok');
    } else {
      for (const p of problems) {
        this.print(`${p.severity} ${p.code}: ${p.message}  [${name}:${p.line}]`, p.severity === 'error' ? 'err' : 'warn');
      }
    }
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
`Kin 0.3 cheat sheet
-------------------
*mode = B;                    // B beginner, S standard, A advanced (first line)

-- website module (0.3): declarative sites, built with 'build web' --
website "My Site" {
  page Home {
    title "Welcome"                     // document title + page heading
    heading "Section"                   // section heading
    text "A paragraph"
    button "Get Started" open About     // prominent navigation
    link "More" open About              // inline navigation
    image "assets/pic.png" alt "Pic"    // alt text required
    divider
  }
  page About { text "About page" }
}
build web                     // index.html + pages + styles.css + app.js
preview                       // open the built site in the preview panel

func main() { }               // program entry (void, no params)
func add(a: int, b: int) -> int { return a + b; }
import "math.kin" as math;    // modules: qualified calls math.add(1, 2)

let x = 1;                    // immutable, type inferred
var count = 0;                // mutable
count += 1;                   // += and -=

if x > 0 { } else if x == 0 { } else { }
while x < 10 { break; continue; }
for i in 0..10 { }            // exclusive range
for i in 0..=9 { }            // inclusive range
for item in my_array { }      // array elements

let a = [1, 2, 3];            // arrays (reference semantics, deep ==)
let m = [[1], [2]];           // nested arrays
a[0] = 5;                     // element assignment
let empty: [int] = [];        // empty array needs its type

let f = 2 as float;           // casts: int<->float, ->string
let s = 5 as string;

print(len(a) + abs(-3) + min(1, 2) + max(1.5, 2.5));

// types: int float bool string [T] void
// no implicit int/float conversion: write 2.0, not 2 (or cast)
print("text " + "concat");    // print, len, abs, min, max builtins
// operators: + - * / %  < > <= >= == !=  and or not
// comments: // line and /* block */`,
      'out');
  }

  /** Compile and execute through the wasm backend. */
  private run(args: string[]): void {
    const f = this.resolveFile(args);
    if (!f) return;
    if (!backend.available) {
      this.print('running programs inside Studio needs the compiler backend (kinn.wasm)', 'warn');
      this.print('build it with: cd compiler && cargo build --release -p kinn-ffi --target wasm32-unknown-unknown', 'out');
      this.print('then copy it: cp compiler/target/wasm32-unknown-unknown/release/kinn_ffi.wasm studio/public/kinn.wasm', 'out');
      this.print('or use the CLI, fully offline: kinn run <file.kin>', 'dim');
      return;
    }
    this.print(`compiling ${f.name} (kinn ${backend.version()}, wasm)...`, 'dim');
    const result = backend.run(f.name, this.deps.vfs.projectFiles());
    this.deps.onProblems(result.diagnostics, f.name);

    if (result.stdout.length > 0) this.printBlock(result.stdout, 'out');
    if (result.ok) {
      this.print(`— exited 0 —`, 'ok');
    } else if (result.exitCode === 1) {
      for (const d of result.diagnostics) {
        if (d.severity === 'error') this.print(`${d.code}: ${d.message}  [${d.file}:${d.line}:${d.col}]`, 'err');
      }
      this.print('— compilation failed —', 'err');
    } else if (result.runtimeError) {
      this.print(`error[${result.runtimeError.code}]: ${result.runtimeError.message}  [${f.name}:${result.runtimeError.line}]`, 'err');
      this.print('— runtime error —', 'err');
    }
  }

  /** Generate artifacts through the wasm backend (same backends as the CLI). */
  private build(args: string[]): void {
    const target = args[0] ?? 'web';
    if (target !== 'web' && target !== 'android') {
      this.print(`unknown target '${target}' — supported: web, android`, 'err');
      return;
    }
    if (!backend.available) {
      this.print('building needs the compiler backend (kinn.wasm)', 'warn');
      this.print('build it with: cd compiler && cargo build --release -p kinn-ffi --target wasm32-unknown-unknown', 'out');
      return;
    }
    const f = this.resolveFile([]);
    if (!f) return;
    this.print(`building ${f.name} for ${target} (kinn ${backend.version()}, wasm)...`, 'dim');
    const result = target === 'web'
      ? backend.buildWeb(f.name, this.deps.vfs.projectFiles())
      : backend.buildAndroid(f.name, this.deps.vfs.projectFiles());
    this.deps.onProblems(result.diagnostics, f.name);

    if (!result.ok) {
      for (const d of result.diagnostics) {
        if (d.severity === 'error') this.print(`${d.code}: ${d.message}  [${d.file}:${d.line}:${d.col}]`, 'err');
      }
      this.print('— build failed —', 'err');
      return;
    }

    this.lastBuild = { result, target };
    this.deps.onBuildResult?.(result, target);
    const bytes = result.files.reduce((n, file) => n + file.content.length, 0);
    this.print(`built ${target} site "${result.site}" (${result.pages} page(s), ${result.files.length} files, ~${bytes} B):`, 'ok');
    for (const file of result.files) {
      this.print(`  ${file.path}  (${file.content.length} B)`, 'out');
    }
    if (target === 'web') {
      this.print("type 'preview' to open it in the preview panel", 'dim');
    } else {
      this.print('this is an exported Android Gradle project (not an APK). Build it in', 'dim');
      this.print('Android Studio, with gradle assembleDebug, or via remote CI (docs/BACKEND_ANDROID.md).', 'dim');
    }
  }

  private preview(): void {
    if (!this.lastBuild || this.lastBuild.target !== 'web') {
      this.print("no web build yet — run 'build web' first", 'warn');
      return;
    }
    if (this.deps.onPreview) {
      this.deps.onPreview(this.lastBuild.result.files);
      this.print('preview opened — links and buttons navigate inside the preview', 'ok');
    } else {
      this.print('preview is not available in this host', 'err');
    }
  }

  private save(): void {
    void this.deps.vfs.markSaved().then(() => {
      this.print('project saved to this browser (IndexedDB)', 'ok');
    });
  }

  private exportProject(): void {
    const json = serializeProject(this.deps.vfs.toStoredProject());
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${this.deps.vfs.projectName}.kinnproj`;
    a.click();
    URL.revokeObjectURL(url);
    this.print(`exported ${this.deps.vfs.list().length} files to ${this.deps.vfs.projectName}.kinnproj`, 'ok');
  }

  private importProject(): void {
    if (this.deps.onImportRequest) {
      this.print('choose a .kinnproj (project) or .kin (single file) — see the file picker', 'dim');
      this.deps.onImportRequest();
    } else {
      this.print('import is not available in this host', 'err');
    }
  }
}
