/**
 * The Studio compiler backend: the real Kin compiler and VM, compiled to
 * WebAssembly from the same Rust code as the `kinn` CLI
 * (compiler/crates/kinn-ffi).
 *
 * `kinn.wasm` is produced by `npm run build:wasm` (or the CI pipeline)
 * and served from `public/`. When it is missing — for example a plain
 * `vite dev` without the Rust toolchain — the backend reports itself as
 * unavailable and Studio falls back to editor-level checks, honestly
 * saying so instead of pretending.
 */

export interface CompilerDiagnostic {
  severity: 'error' | 'warning' | 'note';
  code: string;
  message: string;
  file: string;
  line: number;
  col: number;
  len: number;
}

export interface CheckResult {
  ok: boolean;
  mode: 'B' | 'S' | 'A';
  diagnostics: CompilerDiagnostic[];
  text: string;
}

export interface RunResult extends CheckResult {
  exitCode: number;
  stdout: string;
  runtimeError?: { code: string; message: string; line: number };
}

/** One generated artifact from a target backend (path + UTF-8 content). */
export interface OutputFile {
  path: string;
  content: string;
}

/** Result of `kinn_build_web` / `kinn_build_android` (same JSON as FFI). */
export interface BuildResult {
  ok: boolean;
  site: string;
  pages: number;
  files: OutputFile[];
  diagnostics: CompilerDiagnostic[];
  text: string;
}

/** One project file: its project-relative path and its content. */
export interface ProjectFile {
  name: string;
  content: string;
}

interface WasmExports {
  memory: WebAssembly.Memory;
  kinn_alloc(len: number): number;
  kinn_free(ptr: number, len: number): void;
  kinn_set_module(pathPtr: number, pathLen: number, srcPtr: number, srcLen: number): void;
  kinn_clear_modules(): void;
  kinn_check(srcPtr: number, srcLen: number, entryPtr: number, entryLen: number): number;
  kinn_run(srcPtr: number, srcLen: number, entryPtr: number, entryLen: number): number;
  kinn_build_web(srcPtr: number, srcLen: number, entryPtr: number, entryLen: number): number;
  kinn_build_android(srcPtr: number, srcLen: number, entryPtr: number, entryLen: number): number;
  kinn_version(): number;
}

const LEN_PREFIX = 4;

export class KinnBackend {
  private exports: WasmExports | null = null;
  private loading: Promise<boolean> | null = null;

  /** Try to load `kinn.wasm` exactly once. Resolves false when absent. */
  load(url = 'kinn.wasm'): Promise<boolean> {
    if (this.exports) return Promise.resolve(true);
    if (this.loading) return this.loading;
    this.loading = (async () => {
      try {
        const res = await fetch(url);
        if (!res.ok) return false;
        const bytes = await res.arrayBuffer();
        return this.loadBytes(bytes);
      } catch {
        return false;
      }
    })();
    return this.loading;
  }

  /** Instantiate from raw wasm bytes (fetch path is built on this). */
  loadBytes(data: ArrayBuffer | Uint8Array): boolean {
    if (this.exports) return true;
    try {
      const view = data instanceof Uint8Array ? data : new Uint8Array(data);
      const buf = view.buffer.slice(
        view.byteOffset,
        view.byteOffset + view.byteLength,
      ) as ArrayBuffer;
      const mod = new WebAssembly.Module(buf);
      const instance = new WebAssembly.Instance(mod, {});
      this.exports = instance.exports as unknown as WasmExports;
      return true;
    } catch {
      return false;
    }
  }

  get available(): boolean {
    return this.exports !== null;
  }

  /** Backend version as "0.2.0" style, or null when not loaded. */
  version(): string | null {
    if (!this.exports) return null;
    // Encoding (see kinn-ffi): 0x00_MM_mm_pp — major, minor, patch bytes.
    const v = this.exports.kinn_version();
    const major = (v >>> 24) & 0xff;
    const minor = (v >>> 16) & 0xff;
    const patch = (v >>> 8) & 0xff;
    return `${major}.${minor}.${patch}`;
  }

  private writeString(text: string): { ptr: number; len: number } {
    const bytes = new TextEncoder().encode(text);
    const ptr = this.exports!.kinn_alloc(bytes.length);
    new Uint8Array(this.exports!.memory.buffer, ptr, bytes.length).set(bytes);
    return { ptr, len: bytes.length };
  }

  private readResult(ptr: number): string {
    const view = new DataView(this.exports!.memory.buffer);
    const len = view.getUint32(ptr, true);
    const bytes = new Uint8Array(this.exports!.memory.buffer, ptr + LEN_PREFIX, len);
    const text = new TextDecoder().decode(bytes);
    this.exports!.kinn_free(ptr, LEN_PREFIX + len);
    return text;
  }

  /** Register every project file except the entry as an importable module. */
  private registerModules(entry: string, files: ProjectFile[]): void {
    this.exports!.kinn_clear_modules();
    for (const f of files) {
      if (f.name === entry) continue;
      const path = this.writeString(f.name);
      const src = this.writeString(f.content);
      this.exports!.kinn_set_module(path.ptr, path.len, src.ptr, src.len);
      // Strings were copied by the FFI layer; free the input buffers.
      this.exports!.kinn_free(path.ptr, path.len);
      this.exports!.kinn_free(src.ptr, src.len);
    }
  }

  private call(
    fn: 'kinn_check' | 'kinn_run' | 'kinn_build_web' | 'kinn_build_android',
    entry: string,
    src: string,
    files: ProjectFile[],
  ): string {
    if (!this.exports) throw new Error('backend not loaded');
    this.registerModules(entry, files);
    const srcBuf = this.writeString(src);
    const entryBuf = this.writeString(entry);
    const resultPtr = this.exports[fn](srcBuf.ptr, srcBuf.len, entryBuf.ptr, entryBuf.len);
    this.exports.kinn_free(srcBuf.ptr, srcBuf.len);
    this.exports.kinn_free(entryBuf.ptr, entryBuf.len);
    return this.readResult(resultPtr);
  }

  /** Full type check of the entry with the project files as modules. */
  check(entry: string, files: ProjectFile[]): CheckResult {
    const src = files.find((f) => f.name === entry)?.content ?? '';
    return JSON.parse(this.call('kinn_check', entry, src, files));
  }

  /** Compile and execute the entry; program output is captured. */
  run(entry: string, files: ProjectFile[]): RunResult {
    const src = files.find((f) => f.name === entry)?.content ?? '';
    return JSON.parse(this.call('kinn_run', entry, src, files));
  }

  /** Generate the web distribution (same backend as `kinn build --target web`). */
  buildWeb(entry: string, files: ProjectFile[]): BuildResult {
    const src = files.find((f) => f.name === entry)?.content ?? '';
    return JSON.parse(this.call('kinn_build_web', entry, src, files));
  }

  /** Generate the Android Gradle project (project export — not an APK). */
  buildAndroid(entry: string, files: ProjectFile[]): BuildResult {
    const src = files.find((f) => f.name === entry)?.content ?? '';
    return JSON.parse(this.call('kinn_build_android', entry, src, files));
  }
}

/** The singleton backend used by the terminal, buttons and panels. */
export const backend = new KinnBackend();
