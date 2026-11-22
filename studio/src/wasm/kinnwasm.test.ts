/**
 * Tests for the WebAssembly compiler backend. These run against the
 * REAL compiler: studio/public/kinn.wasm, built with `npm run
 * build:wasm` (kinn-ffi compiled for wasm32-unknown-unknown).
 *
 * When the wasm file is not present (a checkout without the Rust
 * toolchain), these tests are skipped loudly — CI builds the wasm
 * first, so the real assertions always run there.
 */

import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { KinnBackend, type ProjectFile } from './kinnwasm';

const wasmPath = join(dirname(fileURLToPath(import.meta.url)), '../../public/kinn.wasm');
const hasWasm = existsSync(wasmPath);

describe.skipIf(!hasWasm)('KinnBackend against the real kinn.wasm', () => {
  let backend: KinnBackend;

  beforeAll(() => {
    backend = new KinnBackend();
    // Node has no fetch(file://): instantiate from raw bytes.
    const bytes = readFileSync(wasmPath);
    expect(backend.loadBytes(bytes)).toBe(true);
  });

  const HELLO: ProjectFile[] = [
    { name: 'main.kin', content: '*mode = S;\nfunc main() { print("hi"); }' },
  ];

  it('reports its version', () => {
    expect(backend.version()).toBe('0.3.0');
  });

  it('checks a valid program', () => {
    const result = backend.check('main.kin', HELLO);
    expect(result.ok).toBe(true);
    expect(result.mode).toBe('S');
    expect(result.diagnostics).toEqual([]);
  });

  it('returns positioned diagnostics for type errors', () => {
    const result = backend.check('main.kin', [
      { name: 'main.kin', content: 'func main() { fin x = 1; x = 2; }' },
    ]);
    expect(result.ok).toBe(false);
    const e = result.diagnostics.find((d) => d.code === 'E3004');
    expect(e).toBeDefined();
    expect(e!.line).toBe(1);
    expect(e!.col).toBeGreaterThan(1);
  });

  it('runs a program and captures stdout', () => {
    const result = backend.run('main.kin', [
      {
        name: 'main.kin',
        content: 'func main() {\n    fin a = [1, 2, 3];\n    for x in a { print(x * 2); }\n    print(len(a) + abs(-4));\n}',
      },
    ]);
    expect(result.ok).toBe(true);
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toBe('2\n4\n6\n7\n');
  });

  it('reports runtime errors with the failing line', () => {
    const result = backend.run('main.kin', [
      { name: 'main.kin', content: 'func main() {\n    fin a = [1];\n    print(a[9]);\n}' },
    ]);
    expect(result.ok).toBe(false);
    expect(result.exitCode).toBe(3);
    expect(result.runtimeError?.code).toBe('E4005');
    expect(result.runtimeError?.line).toBe(3);
  });

  it('compiles multi-file projects (import resolves in the project)', () => {
    const files: ProjectFile[] = [
      {
        name: 'main.kin',
        content: 'import "math.kin" as math;\nfunc main() { print(math.add(2, 3)); }',
      },
      {
        name: 'math.kin',
        content: 'func add(a: int, b: int) -> int { return a + b; }',
      },
    ];
    const result = backend.run('main.kin', files);
    expect(result.ok).toBe(true);
    expect(result.stdout).toBe('5\n');
  });

  it('flags imports of files that are not in the project', () => {
    const result = backend.check('main.kin', [
      { name: 'main.kin', content: 'import "absent.kin" as a;\nfunc main() { }' },
    ]);
    expect(result.ok).toBe(false);
    expect(result.diagnostics.some((d) => d.code === 'E3017')).toBe(true);
  });

  it('survives many sequential calls (memory management)', () => {
    for (let i = 0; i < 200; i++) {
      const result = backend.run('main.kin', [
        { name: 'main.kin', content: `func main() { print(${i} + 1); }` },
      ]);
      expect(result.stdout).toBe(`${i + 1}\n`);
    }
  });

  it('handles unicode sources and output', () => {
    const result = backend.run('main.kin', [
      { name: 'main.kin', content: 'func main() { print("héllo → 世界"); print(len("héllo")); }' },
    ]);
    expect(result.stdout).toBe('héllo → 世界\n5\n');
  });

  it('builds a website for the web target (0.3 backends)', () => {
    const site = `*mode = B;
website "Studio Site" {
  page Home {
    title "Hi"
    button "Go" open Next
  }
  page Next { link "Back" open Home }
}`;
    const result = backend.buildWeb('site.kin', [{ name: 'site.kin', content: site }]);
    expect(result.ok).toBe(true);
    expect(result.site).toBe('Studio Site');
    expect(result.pages).toBe(2);
    const paths = result.files.map((f) => f.path).sort();
    expect(paths).toEqual(['app.js', 'index.html', 'next.html', 'styles.css']);
    expect(result.files.find((f) => f.path === 'index.html')!.content).toContain('<title>Hi · Studio Site</title>');
  });

  it('exports an android project (not an apk) through wasm', () => {
    const result = backend.buildAndroid('site.kin', [
      {
        name: 'site.kin',
        content: 'website "S" { page Home { text "hi" } }',
      },
    ]);
    expect(result.ok).toBe(true);
    expect(result.files.some((f) => f.path.endsWith('MainActivity.kt'))).toBe(true);
    expect(result.files.some((f) => f.path.endsWith('AndroidManifest.xml'))).toBe(true);
  });

  it('reports E3038 when the project has no website', () => {
    const result = backend.buildWeb('main.kin', [{ name: 'main.kin', content: 'func main() { print(1); }' }]);
    expect(result.ok).toBe(false);
    expect(result.diagnostics.some((d) => d.code === 'E3038')).toBe(true);
  });
});

describe('KinnBackend without wasm', () => {
  it('reports unavailable when the file is missing', async () => {
    const b = new KinnBackend();
    const ok = await b.load('definitely-not-there.wasm');
    expect(ok).toBe(false);
    expect(b.available).toBe(false);
    expect(b.version()).toBeNull();
  });
});
