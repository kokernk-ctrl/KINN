/**
 * Studio Next settings: persisted, honest, and all of them actually
 * applied. Stored under a dedicated localStorage key so the classic
 * Studio is never affected. Headless (no DOM) so it is unit-testable.
 */

export interface NextSettings {
  /** Editor font size in px (line height is derived). */
  editorFont: number;
  /** Terminal font size in px. */
  terminalFont: number;
  /** Run the real compiler check (wasm) on edit, debounced. */
  autoCheck: boolean;
  /** Spaces inserted by Tab / auto-indent in the editor. */
  indentSize: number;
}

export const EDITOR_FONT_SIZES = [12, 13, 14, 16] as const;
export const TERMINAL_FONT_SIZES = [11, 12, 13, 15] as const;
export const INDENT_SIZES = [2, 4] as const;

export const DEFAULT_SETTINGS: NextSettings = {
  editorFont: 13,
  terminalFont: 12,
  autoCheck: true,
  indentSize: 2,
};

const KEY = 'kinn-studio-next-settings';

function clamp(value: number, allowed: readonly number[], fallback: number): number {
  return allowed.includes(value) ? value : fallback;
}

/** Validate an untrusted parsed value against the defaults. */
export function normalizeSettings(raw: unknown): NextSettings {
  const o = (typeof raw === 'object' && raw !== null ? raw : {}) as Record<string, unknown>;
  return {
    editorFont: clamp(Number(o.editorFont), EDITOR_FONT_SIZES, DEFAULT_SETTINGS.editorFont),
    terminalFont: clamp(Number(o.terminalFont), TERMINAL_FONT_SIZES, DEFAULT_SETTINGS.terminalFont),
    autoCheck: typeof o.autoCheck === 'boolean' ? o.autoCheck : DEFAULT_SETTINGS.autoCheck,
    indentSize: clamp(Number(o.indentSize), INDENT_SIZES, DEFAULT_SETTINGS.indentSize),
  };
}

/** Load settings from localStorage (falls back to defaults). */
export function loadSettings(store: Storage | null): NextSettings {
  if (!store) return { ...DEFAULT_SETTINGS };
  try {
    const raw = store.getItem(KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    return normalizeSettings(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

/** Persist settings; failures are ignored (settings stay in memory). */
export function saveSettings(store: Storage | null, s: NextSettings): void {
  if (!store) return;
  try {
    store.setItem(KEY, JSON.stringify(s));
  } catch {
    // private mode / quota — keep working with in-memory settings
  }
}

/** Editor line height in px for a given font size (kept in one place). */
export function editorLineHeight(fontPx: number): number {
  return Math.round(fontPx * 1.55);
}
