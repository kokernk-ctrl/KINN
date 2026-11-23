import { describe, expect, it } from 'vitest';
import { WorkspaceState } from './state';

class FakeStorage {
  private data = new Map<string, string>();
  getItem(k: string): string | null {
    return this.data.get(k) ?? null;
  }
  setItem(k: string, v: string): void {
    this.data.set(k, v);
  }
}

describe('WorkspaceState', () => {
  it('opens tabs and tracks the active one', () => {
    const s = new WorkspaceState();
    s.openFile('a.kin');
    s.openFile('b.kin');
    s.openFile('c.kin');
    expect(s.openTabs).toEqual(['a.kin', 'b.kin', 'c.kin']);
    expect(s.active).toBe('c.kin');
    s.openFile('a.kin'); // re-open is a no-op on order
    expect(s.openTabs).toEqual(['a.kin', 'b.kin', 'c.kin']);
  });

  it('setActive requires an open tab', () => {
    const s = new WorkspaceState();
    s.openFile('a.kin');
    expect(s.setActive('ghost.kin')).toBe(false);
    expect(s.setActive('a.kin')).toBe(true);
    expect(s.active).toBe('a.kin');
  });

  it('closing the active tab moves to a neighbour', () => {
    const s = new WorkspaceState();
    s.openFile('a.kin');
    s.openFile('b.kin');
    s.openFile('c.kin');
    s.setActive('b.kin');
    s.closeTab('b.kin');
    expect(s.openTabs).toEqual(['a.kin', 'c.kin']);
    expect(s.active).toBe('c.kin');
    s.closeTab('a.kin');
    s.closeTab('c.kin');
    expect(s.active).toBe('');
  });

  it('modified tracking survives tab switches and clears on save', () => {
    const s = new WorkspaceState();
    s.openFile('a.kin');
    s.openFile('b.kin');
    s.markModified('a.kin');
    expect(s.isModified('a.kin')).toBe(true);
    expect(s.isModified('b.kin')).toBe(false);
    s.markAllSaved();
    expect(s.isModified('a.kin')).toBe(false);
  });

  it('closing a tab clears its modified flag', () => {
    const s = new WorkspaceState();
    s.openFile('a.kin');
    s.markModified('a.kin');
    s.closeTab('a.kin');
    expect(s.isModified('a.kin')).toBe(false);
  });

  it('layout modes cycle and toggle', () => {
    const s = new WorkspaceState();
    expect(s.snapshot().layout).toBe('split');
    expect(s.cycleLayout()).toBe('editor');
    expect(s.cycleLayout()).toBe('terminal');
    expect(s.cycleLayout()).toBe('split');
    expect(s.toggleExplorer()).toBe(false);
    expect(s.toggleExplorer()).toBe(true);
    expect(s.toggleBottom()).toBe(false);
  });

  it('notifies listeners on changes', () => {
    const s = new WorkspaceState();
    let calls = 0;
    s.onChange(() => calls++);
    s.openFile('a.kin');
    s.markModified('a.kin');
    s.setLayout('terminal');
    expect(calls).toBe(3);
  });

  it('layout persistence round-trips with clamping on load', () => {
    const s = new WorkspaceState();
    const store = new FakeStorage() as unknown as Storage;
    s.setLayout('terminal');
    s.setExplorer(false);
    s.saveLayout(store, { explorerWidth: 300, bottomHeight: 400 });
    const other = new WorkspaceState();
    const p = other.loadLayout(store);
    expect(p.layout).toBe('terminal');
    expect(p.explorerVisible).toBe(false);
    expect(p.explorerWidth).toBe(300);
    expect(p.bottomHeight).toBe(400);
    // out-of-range values are dropped
    const bad = new FakeStorage() as unknown as Storage;
    bad.setItem('kinn-studio-next-layout', JSON.stringify({ layout: 'split', explorerWidth: 9999, bottomHeight: -5 }));
    const q = other.loadLayout(bad);
    expect(q.explorerWidth).toBeUndefined();
    expect(q.bottomHeight).toBeUndefined();
  });
});
