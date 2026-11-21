/**
 * Tests for the .kinnproj project format and the storage adapters.
 */

import { describe, expect, it } from 'vitest';
import {
  MemoryStorage,
  parseProject,
  serializeProject,
  type StoredProject,
} from './storage';

const PROJECT: StoredProject = {
  name: 'demo',
  active: 'main.kin',
  files: [
    { name: 'main.kin', content: 'func main() { }' },
    { name: 'math.kin', content: 'func add(a: int, b: int) -> int { return a + b; }' },
  ],
};

describe('serializeProject / parseProject round trip', () => {
  it('round trips losslessly', () => {
    const json = serializeProject(PROJECT);
    const back = parseProject(json);
    expect(back).toEqual(PROJECT);
  });

  it('rejects non-project JSON', () => {
    expect(() => parseProject('{"hello": 1}')).toThrow(/format/);
    expect(() => parseProject('not json at all')).toThrow(/JSON/);
    expect(() => parseProject('[]')).toThrow();
  });

  it('rejects newer versions', () => {
    const json = serializeProject(PROJECT).replace('"version": 1', '"version": 99');
    expect(() => parseProject(json)).toThrow(/version/);
  });

  it('rejects bad file names and duplicates', () => {
    expect(() =>
      parseProject({ format: 'kinnproj', version: 1, name: 'x', files: [{ name: 'evil.js', content: '' }] }),
    ).toThrow(/invalid file name/);
    expect(() =>
      parseProject({
        format: 'kinnproj',
        version: 1,
        name: 'x',
        files: [
          { name: 'a.kin', content: '' },
          { name: 'a.kin', content: '' },
        ],
      }),
    ).toThrow(/duplicate/);
  });

  it('falls back to the first file when active is missing', () => {
    const p = parseProject({
      format: 'kinnproj',
      version: 1,
      name: 'x',
      active: 'nope.kin',
      files: [
        { name: 'a.kin', content: '' },
        { name: 'b.kin', content: '' },
      ],
    });
    expect(p.active).toBe('a.kin');
  });
});

describe('MemoryStorage', () => {
  it('saves and loads projects', async () => {
    const storage = new MemoryStorage();
    expect(await storage.loadProject()).toBeNull();
    await storage.saveProject(PROJECT);
    expect(await storage.loadProject()).toEqual(PROJECT);
  });

  it('returns copies, not references', async () => {
    const storage = new MemoryStorage();
    await storage.saveProject(PROJECT);
    const a = await storage.loadProject();
    const b = await storage.loadProject();
    expect(a).not.toBe(b);
    a!.files[0].content = 'mutated';
    expect((await storage.loadProject())!.files[0].content).toBe('func main() { }');
  });
});
