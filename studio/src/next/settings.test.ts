import { describe, expect, it } from 'vitest';
import {
  DEFAULT_SETTINGS,
  editorLineHeight,
  loadSettings,
  normalizeSettings,
  saveSettings,
} from './settings';

/** Minimal Storage stub (headless tests). */
class FakeStorage {
  private data = new Map<string, string>();
  getItem(k: string): string | null {
    return this.data.get(k) ?? null;
  }
  setItem(k: string, v: string): void {
    this.data.set(k, v);
  }
}

describe('settings', () => {
  it('defaults are sane', () => {
    expect(DEFAULT_SETTINGS.editorFont).toBe(13);
    expect(DEFAULT_SETTINGS.terminalFont).toBe(12);
    expect(DEFAULT_SETTINGS.autoCheck).toBe(true);
    expect(DEFAULT_SETTINGS.indentSize).toBe(2);
  });

  it('normalize clamps unknown values to defaults', () => {
    const s = normalizeSettings({ editorFont: 999, terminalFont: 'bogus', autoCheck: 'yes', indentSize: 7 });
    expect(s.editorFont).toBe(DEFAULT_SETTINGS.editorFont);
    expect(s.terminalFont).toBe(DEFAULT_SETTINGS.terminalFont);
    expect(s.autoCheck).toBe(DEFAULT_SETTINGS.autoCheck);
    expect(s.indentSize).toBe(DEFAULT_SETTINGS.indentSize);
  });

  it('normalize keeps valid values', () => {
    const s = normalizeSettings({ editorFont: 16, terminalFont: 11, autoCheck: false, indentSize: 4 });
    expect(s).toEqual({ editorFont: 16, terminalFont: 11, autoCheck: false, indentSize: 4 });
  });

  it('save + load round-trips', () => {
    const store = new FakeStorage() as unknown as Storage;
    saveSettings(store, { editorFont: 14, terminalFont: 15, autoCheck: false, indentSize: 2 });
    const loaded = loadSettings(store);
    expect(loaded).toEqual({ editorFont: 14, terminalFont: 15, autoCheck: false, indentSize: 2 });
  });

  it('load with no store falls back to defaults', () => {
    expect(loadSettings(null)).toEqual(DEFAULT_SETTINGS);
  });

  it('corrupt data falls back to defaults', () => {
    const store = new FakeStorage() as unknown as Storage;
    store.setItem('kinn-studio-next-settings', '{not json');
    expect(loadSettings(store)).toEqual(DEFAULT_SETTINGS);
  });

  it('line height is derived from font size', () => {
    expect(editorLineHeight(13)).toBe(20);
    expect(editorLineHeight(16)).toBe(25);
  });
});
