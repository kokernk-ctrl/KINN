/**
 * Vfs 0.2 behavior: file management (create/rename/remove/import) and
 * debounced persistence through a ProjectStorage adapter.
 */

import { describe, expect, it, vi } from 'vitest';
import { Vfs } from './vfs';
import { MemoryStorage } from './storage/storage';

function seed(): Array<[string, string]> {
  return [
    ['hello.kin', 'func main() { print("hi"); }'],
    ['loops.kin', 'func main() { }'],
  ];
}

describe('Vfs file management', () => {
  it('lists, opens and switches the active file', () => {
    const vfs = new Vfs('demo', seed());
    expect(vfs.list().map((f) => f.name)).toEqual(['hello.kin', 'loops.kin']);
    expect(vfs.active?.name).toBe('hello.kin');
    expect(vfs.setActive('loops.kin')).toBe(true);
    expect(vfs.setActive('nope.kin')).toBe(false);
    expect(vfs.active?.name).toBe('loops.kin');
  });

  it('creates and rejects invalid or duplicate names', () => {
    const vfs = new Vfs('demo', seed());
    expect(vfs.create('new.kin')).toBe(true);
    expect(vfs.activeName).toBe('new.kin');
    expect(vfs.create('new.kin')).toBe(false);
    expect(vfs.create('no-ext')).toBe(false);
    expect(vfs.create('../escape.kin')).toBe(false);
    expect(vfs.create('sub/dir.kin')).toBe(false);
  });

  it('renames and keeps the active file', () => {
    const vfs = new Vfs('demo', seed());
    expect(vfs.rename('hello.kin', 'greeting.kin')).toBe(true);
    expect(vfs.activeName).toBe('greeting.kin');
    expect(vfs.open('hello.kin')).toBeUndefined();
    expect(vfs.rename('greeting.kin', 'loops.kin')).toBe(false);
    expect(vfs.rename('missing.kin', 'x.kin')).toBe(false);
  });

  it('removes files and falls back to a remaining file', () => {
    const vfs = new Vfs('demo', seed());
    expect(vfs.remove('hello.kin')).toBe(true);
    expect(vfs.activeName).toBe('loops.kin');
    expect(vfs.remove('hello.kin')).toBe(false);
  });

  it('imports add or replace files', () => {
    const vfs = new Vfs('demo', seed());
    expect(vfs.importFile('math.kin', 'func f() {}')).toBe(true);
    expect(vfs.activeName).toBe('math.kin');
    expect(vfs.importFile('hello.kin', 'replaced')).toBe(true);
    expect(vfs.open('hello.kin')?.content).toBe('replaced');
    expect(vfs.importFile('bad name', 'x')).toBe(false);
  });

  it('exposes the project in the compile shape', () => {
    const vfs = new Vfs('demo', seed());
    const files = vfs.projectFiles();
    expect(files).toEqual([
      { name: 'hello.kin', content: 'func main() { print("hi"); }' },
      { name: 'loops.kin', content: 'func main() { }' },
    ]);
  });
});

describe('Vfs persistence', () => {
  it('saves the project (debounced) and can reload it', async () => {
    const storage = new MemoryStorage();
    const vfs = new Vfs('demo', seed(), storage);
    vfs.save('hello.kin', 'edited');
    vfs.create('extra.kin');

    await vfs.flush();
    const stored = await storage.loadProject();
    expect(stored?.name).toBe('demo');
    expect(stored?.active).toBe('extra.kin');
    expect(stored?.files.map((f) => f.name).sort()).toEqual(['extra.kin', 'hello.kin', 'loops.kin']);
    expect(stored?.files.find((f) => f.name === 'hello.kin')?.content).toBe('edited');
  });

  it('reloads a whole project with loadProject', async () => {
    const storage = new MemoryStorage();
    await storage.saveProject({
      name: 'restored',
      active: 'b.kin',
      files: [
        { name: 'a.kin', content: '// a' },
        { name: 'b.kin', content: '// b' },
      ],
    });
    const vfs = new Vfs('demo', seed(), storage);
    vfs.loadProject((await storage.loadProject())!);
    expect(vfs.projectName).toBe('restored');
    expect(vfs.activeName).toBe('b.kin');
    expect(vfs.list().length).toBe(2);
  });

  it('debounces rapid edits into one save', async () => {
    const save = vi.fn(async () => {});
    const storage = {
      loadProject: async () => null,
      saveProject: save,
    };
    const vfs = new Vfs('demo', seed(), storage);
    vfs.save('hello.kin', '1');
    vfs.save('hello.kin', '2');
    vfs.save('hello.kin', '3');
    await vfs.flush();
    expect(save).toHaveBeenCalledTimes(1);
  });
});
