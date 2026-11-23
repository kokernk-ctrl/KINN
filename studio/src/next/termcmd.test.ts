/**
 * Tests for the terminal command engine with a fake sink. Without the
 * wasm backend loaded these tests verify the honest fallbacks: the
 * engine says exactly what is missing instead of pretending.
 */

import { describe, expect, it } from 'vitest';
import { TermEngine, type LineClass, type TermSink, type TermStatus } from './termcmd';
import { Vfs } from '../vfs';
import type { Problem } from '../diagnostics/checks';
import type { BuildResult, OutputFile } from '../wasm/kinnwasm';

interface Recorded {
  lines: Array<{ text: string; cls: LineClass | undefined; echo: boolean }>;
  statuses: TermStatus[];
  cleared: number;
  opened: string[];
  problems: Array<{ list: Problem[]; file: string }>;
  imports: number;
  builds: Array<{ result: BuildResult; target: 'web' | 'android' }>;
  previews: OutputFile[][];
}

function fakeSink(): { sink: TermSink; rec: Recorded } {
  const rec: Recorded = {
    lines: [],
    statuses: [],
    cleared: 0,
    opened: [],
    problems: [],
    imports: 0,
    builds: [],
    previews: [],
  };
  const sink: TermSink = {
    line: (text, cls) => rec.lines.push({ text, cls, echo: cls === undefined }),
    block: (text, cls) => {
      for (const l of text.split('\n')) rec.lines.push({ text: l, cls, echo: false });
    },
    clear: () => {
      rec.cleared++;
    },
    status: (s) => rec.statuses.push(s),
    openFile: (name) => rec.opened.push(name),
    problems: (list, file) => rec.problems.push({ list, file }),
    importRequest: () => {
      rec.imports++;
    },
    buildResult: (result, target) => rec.builds.push({ result, target }),
    preview: (files) => rec.previews.push(files),
  };
  return { sink, rec };
}

function engineWith(files: Array<[string, string]>, rec: Recorded, sink: TermSink): TermEngine {
  const vfs = new Vfs('proj', files);
  vfs.setActive(files[0][0]);
  return new TermEngine({ vfs, sink });
}

describe('TermEngine (no wasm backend loaded)', () => {
  it('echoes commands and records history', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', 'func main() { }']], rec, sink);
    e.exec('ls');
    expect(rec.lines[0].echo).toBe(true);
    expect(rec.lines[0].text).toBe('ls');
    expect(e.history).toEqual(['ls']);
  });

  it('ignores blank input', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('   ');
    expect(rec.lines).toHaveLength(0);
    expect(e.history).toHaveLength(0);
  });

  it('reports unknown commands honestly and marks the status failed', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('frobnicate');
    expect(rec.lines.some((l) => l.cls === 'err' && l.text.includes("unknown command 'frobnicate'"))).toBe(true);
    expect(rec.statuses.at(-1)?.kind).toBe('fail');
  });

  it('ls lists files with the active marker', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', ''], ['b.kin', '']], rec, sink);
    e.exec('ls');
    const out = rec.lines.filter((l) => l.cls === 'out').map((l) => l.text);
    expect(out.some((t) => t.includes('> a.kin'))).toBe(true);
    expect(out.some((t) => t.includes('b.kin') && !t.includes('>'))).toBe(true);
  });

  it('files filters by substring', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['hello.kin', ''], ['loops.kin', '']], rec, sink);
    e.exec('files loop');
    const out = rec.lines.filter((l) => l.cls === 'out').map((l) => l.text);
    expect(out).toHaveLength(1);
    expect(out[0]).toContain('loops.kin');
  });

  it('cat prints numbered lines', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', 'func main() {\n  print(1);\n}']], rec, sink);
    e.exec('cat');
    const out = rec.lines.filter((l) => l.cls === 'out').map((l) => l.text);
    expect(out[0]).toContain('1 | func main() {');
    expect(out[2]).toContain('3 | }');
  });

  it('cat on a missing file reports the error', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('cat ghost.kin');
    expect(rec.lines.some((l) => l.cls === 'err' && l.text.includes('no such file'))).toBe(true);
  });

  it('new creates a file and asks the host to open it', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('new b.kin');
    expect(rec.opened).toEqual(['b.kin']);
    expect(rec.lines.some((l) => l.cls === 'ok' && l.text.includes('created b.kin'))).toBe(true);
  });

  it('new rejects invalid names', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('new nope.txt');
    expect(rec.lines.some((l) => l.cls === 'err')).toBe(true);
    expect(rec.opened).toHaveLength(0);
  });

  it('rm deletes and re-opens the next active file', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', ''], ['b.kin', '']], rec, sink);
    e.exec('rm a.kin');
    expect(rec.lines.some((l) => l.cls === 'ok' && l.text.includes('deleted a.kin'))).toBe(true);
    expect(rec.opened).toEqual(['b.kin']);
  });

  it('mv renames', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', ''], ['b.kin', '']], rec, sink);
    e.exec('mv a.kin c.kin');
    expect(rec.lines.some((l) => l.cls === 'ok' && l.text.includes('renamed a.kin -> c.kin'))).toBe(true);
  });

  it('open switches the active file', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', ''], ['b.kin', '']], rec, sink);
    e.exec('open b.kin');
    expect(rec.opened).toEqual(['b.kin']);
    e.exec('open ghost.kin');
    expect(rec.lines.some((l) => l.cls === 'err' && l.text.includes('no such file'))).toBe(true);
  });

  it('check without backend falls back to editor checks and says so', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', 'func main() { }']], rec, sink);
    e.exec('check');
    expect(rec.lines.some((l) => l.cls === 'warn' && l.text.includes('backend not loaded'))).toBe(true);
    expect(rec.problems).toHaveLength(1);
    expect(rec.lines.some((l) => l.cls === 'ok' && l.text.startsWith('OK'))).toBe(true);
  });

  it('check surfaces editor-level errors for broken sources', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['broken.kin', 'func main() { print(1); ']], rec, sink);
    e.exec('check broken.kin');
    expect(rec.problems[0].list.some((p) => p.code === 'E2102')).toBe(true);
  });

  it('run without backend explains what is needed instead of faking', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', 'func main() { print(1); }']], rec, sink);
    e.exec('run');
    expect(rec.lines.some((l) => l.cls === 'warn' && l.text.includes('kinn.wasm'))).toBe(true);
    expect(rec.statuses).toHaveLength(0); // no fake execution status
  });

  it('build rejects unknown targets', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('build dos');
    expect(rec.lines.some((l) => l.cls === 'err' && l.text.includes('unknown target'))).toBe(true);
  });

  it('build without backend says it needs kinn.wasm', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('build web');
    expect(rec.lines.some((l) => l.cls === 'warn' && l.text.includes('kinn.wasm'))).toBe(true);
  });

  it('preview without a build is honest', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('preview');
    expect(rec.lines.some((l) => l.cls === 'warn' && l.text.includes('no web build yet'))).toBe(true);
    expect(rec.previews).toHaveLength(0);
  });

  it('mode reports the programming mode', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '*mode = B;\nfunc main() { }']], rec, sink);
    e.exec('mode');
    expect(rec.lines.some((l) => l.cls === 'accent' && l.text.includes('mode B'))).toBe(true);
  });

  it('import requests the host picker', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('import');
    expect(rec.imports).toBe(1);
  });

  it('clear asks the sink to clear', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('clear');
    expect(rec.cleared).toBe(1);
  });

  it('history prints the session history', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('ls');
    e.exec('history');
    const hist = rec.lines.filter((l) => l.cls === 'out').map((l) => l.text);
    expect(hist.some((t) => t.includes('1  ls'))).toBe(true);
  });

  it('help lists the real commands', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', '']], rec, sink);
    e.exec('help');
    const text = rec.lines.filter((l) => l.cls === 'out').map((l) => l.text).join('\n');
    for (const cmd of ['check', 'run', 'build', 'preview', 'files', 'history', 'save', 'export', 'import']) {
      expect(text).toContain(cmd);
    }
  });

  it('tokens prints the token stream', () => {
    const { sink, rec } = fakeSink();
    const e = engineWith([['a.kin', 'fin x = 1;']], rec, sink);
    e.exec('tokens');
    const out = rec.lines.filter((l) => l.cls === 'out').map((l) => l.text);
    expect(out.some((t) => t.includes('FIN'))).toBe(true);
    expect(out.some((t) => t.includes('INT'))).toBe(true);
  });
});
