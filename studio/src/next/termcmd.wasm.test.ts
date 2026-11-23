/**
 * Terminal engine tests against the REAL compiler (kinn.wasm): `check`,
 * `run` and `build web` through the engine produce the same results as
 * the CLI, drive the problems panel, and publish execution statuses.
 */

import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { backend } from '../wasm/kinnwasm';
import { TermEngine, type TermSink, type TermStatus } from './termcmd';
import { Vfs } from '../vfs';

const wasmPath = join(dirname(fileURLToPath(import.meta.url)), '../../public/kinn.wasm');
const hasWasm = existsSync(wasmPath);

interface Rec {
  lines: Array<{ text: string; cls?: string }>;
  statuses: TermStatus[];
  problems: Array<{ list: unknown[]; file: string }>;
  builds: Array<{ target: string; ok: boolean }>;
  previews: number;
}

function makeSink(rec: Rec): TermSink {
  return {
    line: (text, cls) => rec.lines.push({ text, cls }),
    block: (text, cls) => {
      for (const l of text.split('\n')) rec.lines.push({ text: l, cls });
    },
    clear: () => {},
    status: (s) => rec.statuses.push(s),
    openFile: () => {},
    problems: (list, file) => rec.problems.push({ list, file }),
    importRequest: () => {},
    buildResult: (result, target) => rec.builds.push({ target, ok: result.ok }),
    preview: () => {
      rec.previews++;
    },
  };
}

describe.skipIf(!hasWasm)('TermEngine against the real kinn.wasm', () => {
  beforeAll(() => {
    // the engine drives the module-level singleton backend; feed it the
    // real wasm bytes (Node has no fetch(file://))
    expect(backend.loadBytes(readFileSync(wasmPath))).toBe(true);
  });

  function engineWith(files: Array<[string, string]>, rec: Rec): TermEngine {
    const vfs = new Vfs('proj', files);
    vfs.setActive(files[0][0]);
    return new TermEngine({ vfs, sink: makeSink(rec) });
  }

  it('run executes the real VM and reports exit 0', () => {
    const rec: Rec = { lines: [], statuses: [], problems: [], builds: [], previews: 0 };
    const e = engineWith([['a.kin', 'func main() { print("hi"); print(2 + 3); }']], rec);
    e.exec('run');
    const texts = rec.lines.map((l) => l.text);
    expect(texts).toContain('hi');
    expect(texts).toContain('5');
    expect(texts.some((t) => t.includes('exited 0'))).toBe(true);
    expect(rec.statuses.at(-1)?.kind).toBe('ok');
  });

  it('run reports compile errors and drives the problems panel', () => {
    const rec: Rec = { lines: [], statuses: [], problems: [], builds: [], previews: 0 };
    const e = engineWith([['bad.kin', 'func main() { fin x = 1; x = 2; }']], rec);
    e.exec('run bad.kin');
    expect(rec.lines.some((l) => l.cls === 'err' && l.text.startsWith('E3004'))).toBe(true);
    expect(rec.lines.some((l) => l.text.includes('compilation failed'))).toBe(true);
    expect(rec.statuses.at(-1)?.kind).toBe('fail');
    expect(rec.problems).toHaveLength(1);
  });

  it('check reports OK with the runtime error surfaced when it fires', () => {
    const rec: Rec = { lines: [], statuses: [], problems: [], builds: [], previews: 0 };
    const e = engineWith([['a.kin', 'func main() { fin a = [1]; print(a[9]); }']], rec);
    e.exec('run');
    expect(rec.lines.some((l) => l.text.includes('E4005'))).toBe(true);
    expect(rec.lines.some((l) => l.text.includes('runtime error'))).toBe(true);
    expect(rec.statuses.at(-1)?.kind).toBe('fail');
    expect(rec.statuses.at(-1) && rec.statuses.at(-1)!.kind === 'fail' && rec.statuses.at(-1)!.label.includes('runtime')).toBe(true);
  });

  it('build web produces the real dist files and registers them with the host', () => {
    const rec: Rec = { lines: [], statuses: [], problems: [], builds: [], previews: 0 };
    const site = '*mode = B;\nwebsite "Terminal Site" {\n  page Home { title "Hi" text "hello" }\n}';
    const e = engineWith([['site.kin', site]], rec);
    e.exec('build web');
    expect(rec.builds).toEqual([{ target: 'web', ok: true }]);
    const texts = rec.lines.map((l) => l.text);
    expect(texts.some((t) => t.includes('built web site "Terminal Site"'))).toBe(true);
    expect(texts.some((t) => t.includes('index.html'))).toBe(true);
    expect(rec.statuses.at(-1)?.kind).toBe('ok');
  });

  it('build android honestly calls itself a Gradle project, not an APK', () => {
    const rec: Rec = { lines: [], statuses: [], problems: [], builds: [], previews: 0 };
    const e = engineWith([['site.kin', 'website "S" { page Home { text "hi" } }']], rec);
    e.exec('build android');
    expect(rec.builds).toEqual([{ target: 'android', ok: true }]);
    const texts = rec.lines.map((l) => l.text);
    expect(texts.some((t) => t.includes('Gradle project (not an APK)'))).toBe(true);
  });

  it('preview after a web build notifies the host', () => {
    const rec: Rec = { lines: [], statuses: [], problems: [], builds: [], previews: 0 };
    const e = engineWith(
      [['site.kin', 'website "S" { page Home { text "hi" } }']],
      rec,
    );
    e.exec('build web');
    e.exec('preview');
    expect(rec.previews).toBe(1);
  });

  it('version reports the shell and the real compiler version', () => {
    const rec: Rec = { lines: [], statuses: [], problems: [], builds: [], previews: 0 };
    const e = engineWith([['a.kin', '']], rec);
    e.exec('version');
    const text = rec.lines.map((l) => l.text).join(' ');
    expect(text).toContain('0.4.0-next');
    expect(text).toContain('compiler kinn');
    // the compiler version comes from the wasm build itself
    expect(text).toMatch(/compiler kinn \d+\.\d+\.\d+/);
  });
});
