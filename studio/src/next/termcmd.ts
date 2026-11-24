/**
 * Studio Next terminal command engine.
 *
 * Every command here is real: `check`, `run`, `tokens`, `build web`,
 * `build android` go through the actual Kin compiler and VM compiled to
 * WebAssembly (kinn.wasm — the same Rust code as the `kinn` CLI), and
 * file commands operate on the live project. When the backend is not
 * loaded the engine says so and degrades honestly to editor-level
 * checks instead of pretending.
 *
 * The engine is headless: it emits lines and status through a sink, so
 * it can drive any terminal view (and unit tests) without a DOM.
 */

import type { Vfs } from '../vfs';
import { lex } from '../syntax/lexer';
import { detectMode, quickCheck, type Problem } from '../diagnostics/checks';
import { backend, type BuildResult, type OutputFile } from '../wasm/kinnwasm';
import { serializeProject } from '../storage/storage';
import { hostSaveFile } from '../host';

export type LineClass = 'accent' | 'dim' | 'ok' | 'err' | 'warn' | 'out';

export type TermStatus =
  | { kind: 'idle' }
  | { kind: 'running'; label: string }
  | { kind: 'ok'; label: string }
  | { kind: 'fail'; label: string };

export interface TermSink {
  line(text: string, cls?: LineClass): void;
  /** Multi-line pre-formatted output. */
  block(text: string, cls?: LineClass): void;
  clear(): void;
  status(s: TermStatus): void;
  /** `open`/`new` — host shows the file in the editor. */
  openFile(name: string): void;
  /** Diagnostics produced by check/run/build. */
  problems(list: Problem[], file: string): void;
  /** Host file picker for `import`. */
  importRequest(): void;
  /** A finished web/android build (host stores it for preview/export). */
  buildResult(result: BuildResult, target: 'web' | 'android'): void;
  /** `preview` — host shows the built site. */
  preview(files: OutputFile[]): void;
}

export interface TermEngineDeps {
  vfs: Vfs;
  sink: TermSink;
}

export const TERMINAL_VERSION = '0.4.0-next';
export const BANNER_LINES: Array<[string, LineClass | undefined]> = [
  [`KINN Studio ${TERMINAL_VERSION} — terminal shell`, 'accent'],
  ['offline-first · project persists in this browser · real compiler via kinn.wasm', 'dim'],
  ["type 'help' for commands", 'dim'],
];

export class TermEngine {
  private vfs: Vfs;
  private sink: TermSink;
  private history_: string[] = [];
  private lastBuild: { result: BuildResult; target: 'web' | 'android' } | null = null;

  constructor(deps: TermEngineDeps) {
    this.vfs = deps.vfs;
    this.sink = deps.sink;
  }

  get history(): string[] {
    return [...this.history_];
  }

  print(text: string, cls?: LineClass): void {
    this.sink.line(text, cls);
  }

  banner(): void {
    for (const [text, cls] of BANNER_LINES) this.print(text, cls);
  }

  /** Echo + dispatch one command line. */
  exec(raw: string): void {
    const cmd = raw.trim();
    if (cmd === '') return;
    this.sink.line(cmd, undefined); // echo (view styles echoes distinctly)
    this.history_.push(cmd);

    const [name, ...args] = cmd.split(/\s+/);
    switch (name) {
      case 'help': this.help(); break;
      case 'version': this.version(); break;
      case 'about': this.about(); break;
      case 'clear': this.sink.clear(); break;
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
      case 'files': this.files(args); break;
      case 'history': this.printHistory(); break;
      default:
        this.print(`unknown command '${name}' — type 'help'`, 'err');
        this.sink.status({ kind: 'fail', label: `unknown: ${name}` });
    }
  }

  // -- info -------------------------------------------------------------------

  private help(): void {
    this.sink.block(
`commands:
  help              this list
  version           shell and compiler versions
  about             what KINN is
  clear             clear this terminal session
  history           command history of this session
  ls                list files in the project
  files <query>     filter project files by substring
  cat <file>        print a file with line numbers
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
  preview           show the last web build in the preview panel`, 'out');
  }

  private version(): void {
    const compiler = backend.available
      ? `compiler kinn ${backend.version()} (Rust via WebAssembly, local)`
      : 'compiler backend NOT LOADED (editor checks only; run npm run build:wasm)';
    this.print(`KINN Studio ${TERMINAL_VERSION} (shell) — ${compiler}`, 'out');
  }

  private about(): void {
    this.sink.block(
`KINN is an offline-first programming language and development
environment. Kin 0.4 compiles console programs through KIR to a stack
VM, lowers declarative apps to an AppModel, and builds declarative
websites into real HTML/CSS with the web backend. In-Studio check, run
and build use the same Rust compiler as the CLI, compiled to
WebAssembly. No accounts, no cloud, no servers.`, 'out');
  }

  private printHistory(): void {
    if (this.history_.length === 0) {
      this.print('(no commands yet)', 'dim');
      return;
    }
    this.history_.forEach((h, i) => this.print(`${String(i + 1).padStart(3)}  ${h}`, 'out'));
  }

  // -- files ------------------------------------------------------------------

  private ls(): void {
    for (const f of this.vfs.list()) {
      this.print(`${this.vfs.activeName === f.name ? '>' : ' '} ${f.name}`, 'out');
    }
  }

  private files(args: string[]): void {
    const q = (args[0] ?? '').toLowerCase();
    const matches = this.vfs.list().filter((f) => f.name.toLowerCase().includes(q));
    if (matches.length === 0) {
      this.print(`no file matches '${q}'`, 'warn');
      return;
    }
    for (const f of matches) this.print(`${this.vfs.activeName === f.name ? '>' : ' '} ${f.name}`, 'out');
  }

  private resolveFile(args: string[]): { name: string; content: string } | null {
    const name = args[0] ?? this.vfs.activeName;
    if (!name) {
      this.print('no file open and no name given', 'err');
      return null;
    }
    const f = this.vfs.open(name);
    if (!f) {
      this.print(`no such file: '${name}' (try 'ls')`, 'err');
      return null;
    }
    return { name: f.name, content: f.content };
  }

  private cat(args: string[]): void {
    const f = this.resolveFile(args);
    if (!f) return;
    f.content.split('\n').forEach((l, i) => this.print(`${String(i + 1).padStart(3)} | ${l}`, 'out'));
  }

  private open(args: string[]): void {
    if (!args[0]) {
      this.print('usage: open <file>', 'err');
      return;
    }
    if (this.vfs.setActive(args[0])) {
      this.sink.openFile(args[0]);
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
    if (this.vfs.create(args[0])) {
      this.sink.openFile(args[0]);
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
    if (this.vfs.remove(args[0])) {
      this.print(`deleted ${args[0]}`, 'ok');
      this.sink.openFile(this.vfs.activeName);
    } else {
      this.print(`no such file: '${args[0]}'`, 'err');
    }
  }

  private mv(args: string[]): void {
    if (args.length < 2) {
      this.print('usage: mv <old-name.kin> <new-name.kin>', 'err');
      return;
    }
    if (this.vfs.rename(args[0], args[1])) {
      this.print(`renamed ${args[0]} -> ${args[1]}`, 'ok');
      this.sink.openFile(this.vfs.activeName);
    } else {
      this.print(`cannot rename '${args[0]}' to '${args[1]}' (missing, invalid or exists)`, 'err');
    }
  }

  // -- compiler -----------------------------------------------------------------

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
      this.print('(build it with: npm run build:wasm in studio/, needs the Rust toolchain)', 'dim');
      this.editorCheck(f.name, f.content);
      return;
    }
    this.sink.status({ kind: 'running', label: `check ${f.name}` });
    const result = backend.check(f.name, this.vfs.projectFiles());
    this.sink.problems(result.diagnostics, f.name);
    if (result.ok) {
      const notes = result.diagnostics.filter((d) => d.severity !== 'error').length;
      this.print(`OK: no errors in ${f.name}${notes > 0 ? ` (${notes} note(s)/warning(s) shown)` : ''}`, 'ok');
      for (const d of result.diagnostics) {
        this.print(`${d.severity} ${d.code}: ${d.message}  [${d.file}:${d.line}]`, d.severity === 'error' ? 'err' : 'warn');
      }
      this.sink.status({ kind: 'ok', label: `check ${f.name}` });
    } else {
      for (const d of result.diagnostics) {
        if (d.severity !== 'error') continue;
        this.print(`${d.code}: ${d.message}  [${d.file}:${d.line}:${d.col}]`, 'err');
      }
      const n = result.diagnostics.filter((d) => d.severity === 'error').length;
      this.print(`${n} error(s) — see the PROBLEMS panel`, 'warn');
      this.sink.status({ kind: 'fail', label: `check ${f.name}` });
    }
  }

  private editorCheck(name: string, content: string): void {
    const problems = quickCheck(content);
    this.sink.problems(problems, name);
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
    this.sink.block(
`Kin 0.4 cheat sheet
-------------------
*mode = B;                    // B beginner, S standard, A advanced (first line)

-- website module: declarative sites, built with 'build web' --
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

fin x = 1;                    // immutable, type inferred
var count = 0;                // mutable
count += 1;                   // += and -=

if x > 0 { } else if x == 0 { } else { }
while x < 10 { break; continue; }
for i in 0..10 { }            // exclusive range
for item in my_array { }      // array elements

fin a = [1, 2, 3];            // arrays (reference semantics, deep ==)
a[0] = 5;                     // element assignment
fin f = 2 as float;           // casts: int<->float, ->string
print(len(a) + abs(-3) + min(1, 2) + max(1.5, 2.5));

// types: int float bool string [T] void
// no implicit int/float conversion: write 2.0, not 2 (or cast)
// operators: + - * / %  < > <= >= == !=  and or not
// comments: // line and /* block */`, 'out');
  }

  /** Compile and execute through the wasm backend. */
  private run(args: string[]): void {
    const f = this.resolveFile(args);
    if (!f) return;
    if (!backend.available) {
      this.print('running programs inside Studio needs the compiler backend (kinn.wasm)', 'warn');
      this.print('build it with: cd studio && npm run build:wasm (Rust toolchain required)', 'out');
      this.print('or use the CLI, fully offline: kinn run <file.kin>', 'dim');
      return;
    }
    this.sink.status({ kind: 'running', label: `run ${f.name}` });
    this.print(`compiling ${f.name} (kinn ${backend.version()}, wasm)...`, 'dim');
    const result = backend.run(f.name, this.vfs.projectFiles());
    this.sink.problems(result.diagnostics, f.name);

    if (result.stdout.length > 0) this.sink.block(result.stdout, 'out');
    if (result.ok) {
      this.print('— exited 0 —', 'ok');
      this.sink.status({ kind: 'ok', label: `run ${f.name} · exit 0` });
    } else if (result.exitCode === 1) {
      for (const d of result.diagnostics) {
        if (d.severity === 'error') this.print(`${d.code}: ${d.message}  [${d.file}:${d.line}:${d.col}]`, 'err');
      }
      this.print('— compilation failed —', 'err');
      this.sink.status({ kind: 'fail', label: `run ${f.name} · compile error` });
    } else if (result.runtimeError) {
      this.print(`error[${result.runtimeError.code}]: ${result.runtimeError.message}  [${f.name}:${result.runtimeError.line}]`, 'err');
      this.print('— runtime error —', 'err');
      this.sink.status({ kind: 'fail', label: `run ${f.name} · runtime error` });
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
      this.print('build it with: cd studio && npm run build:wasm', 'out');
      return;
    }
    const f = this.resolveFile([]);
    if (!f) return;
    this.sink.status({ kind: 'running', label: `build ${target}` });
    this.print(`building ${f.name} for ${target} (kinn ${backend.version()}, wasm)...`, 'dim');
    const result = target === 'web'
      ? backend.buildWeb(f.name, this.vfs.projectFiles())
      : backend.buildAndroid(f.name, this.vfs.projectFiles());
    this.sink.problems(result.diagnostics, f.name);

    if (!result.ok) {
      for (const d of result.diagnostics) {
        if (d.severity === 'error') this.print(`${d.code}: ${d.message}  [${d.file}:${d.line}:${d.col}]`, 'err');
      }
      this.print('— build failed —', 'err');
      this.sink.status({ kind: 'fail', label: `build ${target} · failed` });
      return;
    }

    this.lastBuild = { result, target };
    this.sink.buildResult(result, target);
    const bytes = result.files.reduce((n, file) => n + file.content.length, 0);
    this.print(`built ${target} site "${result.site}" (${result.pages} page(s), ${result.files.length} files, ~${bytes} B):`, 'ok');
    for (const file of result.files) {
      this.print(`  ${file.path}  (${file.content.length} B)`, 'out');
    }
    this.sink.status({ kind: 'ok', label: `build ${target} · ${result.files.length} files` });
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
    this.sink.preview(this.lastBuild.result.files);
    this.print('preview opened — links and buttons navigate inside the preview', 'ok');
  }

  // -- persistence ----------------------------------------------------------------

  /** Persist now. Returns a promise for hosts that track completion. */
  save(): void {
    void this.vfs.markSaved().then(() => {
      this.print('project saved to this browser (IndexedDB)', 'ok');
    });
  }

  exportProject(): void {
    const json = serializeProject(this.vfs.toStoredProject());
    // Browser: Blob + <a download>; Android host: SAF save via the bridge.
    hostSaveFile(
      `${this.vfs.projectName}.kinnproj`,
      new TextEncoder().encode(json),
      'application/json',
    );
    this.print(`exported ${this.vfs.list().length} files to ${this.vfs.projectName}.kinnproj`, 'ok');
  }

  importProject(): void {
    this.print('choose a .kinnproj (project) or .kin (single file) — the file picker opens', 'dim');
    this.sink.importRequest();
  }
}
