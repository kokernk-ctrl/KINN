import { describe, expect, it } from 'vitest';
import { filterCommands, filterNames, fuzzyScore, type PaletteCommand } from './palette';

const CMDS: PaletteCommand[] = [
  { id: 'build.web', title: 'Build web (real backend)', group: 'build', run: () => {} },
  { id: 'build.run', title: 'Run active file (real VM)', group: 'build', run: () => {} },
  { id: 'file.new', title: 'New file…', group: 'file', run: () => {} },
  { id: 'view.split', title: 'Split editor and terminal', group: 'view', run: () => {} },
];

describe('fuzzyScore', () => {
  it('empty query scores 0', () => {
    expect(fuzzyScore('', 'anything')).toBe(0);
  });

  it('substring beats scattered subsequence', () => {
    const sub = fuzzyScore('build', 'Build web (real backend)');
    const scat = fuzzyScore('bw', 'Build web (real backend)');
    expect(sub).not.toBeNull();
    expect(scat).not.toBeNull();
    expect(sub!).toBeGreaterThan(scat!);
  });

  it('start-of-string bonus applies', () => {
    const atStart = fuzzyScore('new', 'New file…');
    const mid = fuzzyScore('fil', 'New file…');
    expect(atStart!).toBeGreaterThan(mid!);
  });

  it('returns null when not a subsequence', () => {
    expect(fuzzyScore('zzz', 'Build web')).toBeNull();
  });

  it('is case insensitive', () => {
    expect(fuzzyScore('RUN', 'Run active file (real VM)')).not.toBeNull();
    expect(fuzzyScore('run', 'RUNTIME')).not.toBeNull();
  });
});

describe('filterCommands', () => {
  it('empty query keeps registration order', () => {
    const out = filterCommands(CMDS, '');
    expect(out.map((c) => c.id)).toEqual(CMDS.map((c) => c.id));
  });

  it('ranks exact title matches first', () => {
    const out = filterCommands(CMDS, 'build web');
    expect(out[0].id).toBe('build.web');
  });

  it('drops commands that cannot match', () => {
    const out = filterCommands(CMDS, 'zzz');
    expect(out).toHaveLength(0);
  });

  it('matches on id too', () => {
    const out = filterCommands(CMDS, 'file.new');
    expect(out[0].id).toBe('file.new');
  });
});

describe('filterNames', () => {
  const names = ['hello.kin', 'loops.kin', 'site.kin', 'broken.kin'];
  it('empty query keeps order', () => {
    expect(filterNames(names, '')).toEqual(names);
  });
  it('prefix matches first', () => {
    expect(filterNames(names, 'hel')[0]).toBe('hello.kin');
  });
  it('filters out non-matches', () => {
    expect(filterNames(names, 'site')).toEqual(['site.kin']);
  });
});
