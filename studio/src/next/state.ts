/**
 * Workspace state for Studio Next: which files are open in editor tabs,
 * which are active, and which were modified since the last explicit
 * save. Headless and observable, so both the UI and tests can react.
 *
 * Note on honesty: the Vfs persists every edit to IndexedDB
 * automatically (debounced). The "modified" dot therefore means
 * "changed since the last explicit save (Ctrl+S / `save`)", not
 * "at risk of being lost" — the status bar states persistence
 * separately.
 */

export type LayoutMode = 'editor' | 'split' | 'terminal';

export interface WorkspaceSnapshot {
  tabs: string[];
  active: string;
  modified: string[];
  layout: LayoutMode;
  explorerVisible: boolean;
  bottomVisible: boolean;
}

type Listener = () => void;

export interface PersistedLayout {
  layout: LayoutMode;
  explorerVisible: boolean;
  bottomVisible: boolean;
  explorerWidth: number;
  bottomHeight: number;
}

const LAYOUT_KEY = 'kinn-studio-next-layout';

export class WorkspaceState {
  private tabs: string[] = [];
  private activeName = '';
  private modifiedSet = new Set<string>();
  private layoutMode: LayoutMode = 'split';
  private explorerOn = true;
  private bottomOn = true;
  private listeners: Listener[] = [];

  constructor() {
    // keep the active tab first-ish: order of opening is preserved
  }

  // -- observation ------------------------------------------------------------

  onChange(fn: Listener): void {
    this.listeners.push(fn);
  }

  private changed(): void {
    for (const fn of this.listeners) fn();
  }

  snapshot(): WorkspaceSnapshot {
    return {
      tabs: [...this.tabs],
      active: this.activeName,
      modified: [...this.modifiedSet],
      layout: this.layoutMode,
      explorerVisible: this.explorerOn,
      bottomVisible: this.bottomOn,
    };
  }

  // -- editor tabs ------------------------------------------------------------

  /** Open a file in a tab (opens a new tab when not yet open). */
  openFile(name: string, makeActive = true): boolean {
    if (!this.tabs.includes(name)) this.tabs.push(name);
    if (makeActive) this.activeName = name;
    this.changed();
    return true;
  }

  setActive(name: string): boolean {
    if (!this.tabs.includes(name)) return false;
    this.activeName = name;
    this.changed();
    return true;
  }

  /** Close a tab; the active tab moves to a neighbour when needed. */
  closeTab(name: string): void {
    const idx = this.tabs.indexOf(name);
    if (idx === -1) return;
    this.tabs.splice(idx, 1);
    this.modifiedSet.delete(name);
    if (this.activeName === name) {
      const next = this.tabs[Math.min(idx, this.tabs.length - 1)] ?? '';
      this.activeName = next;
    }
    this.changed();
  }

  get active(): string {
    return this.activeName;
  }

  get openTabs(): string[] {
    return [...this.tabs];
  }

  // -- modification tracking ----------------------------------------------------

  markModified(name: string): void {
    if (!this.modifiedSet.has(name)) {
      this.modifiedSet.add(name);
      this.changed();
    }
  }

  markAllSaved(): void {
    if (this.modifiedSet.size === 0) return;
    this.modifiedSet.clear();
    this.changed();
  }

  isModified(name: string): boolean {
    return this.modifiedSet.has(name);
  }

  // -- layout -------------------------------------------------------------------

  setLayout(mode: LayoutMode): void {
    if (this.layoutMode === mode) return;
    this.layoutMode = mode;
    this.changed();
  }

  cycleLayout(): LayoutMode {
    const order: LayoutMode[] = ['split', 'editor', 'terminal'];
    const next = order[(order.indexOf(this.layoutMode) + 1) % order.length];
    this.setLayout(next);
    return next;
  }

  toggleExplorer(): boolean {
    this.explorerOn = !this.explorerOn;
    this.changed();
    return this.explorerOn;
  }

  toggleBottom(): boolean {
    this.bottomOn = !this.bottomOn;
    this.changed();
    return this.bottomOn;
  }

  setExplorer(visible: boolean): void {
    this.explorerOn = visible;
    this.changed();
  }

  setBottom(visible: boolean): void {
    this.bottomOn = visible;
    this.changed();
  }

  // -- layout persistence ---------------------------------------------------------

  saveLayout(store: Storage | null, sizes: { explorerWidth: number; bottomHeight: number }): void {
    if (!store) return;
    const data: PersistedLayout = {
      layout: this.layoutMode,
      explorerVisible: this.explorerOn,
      bottomVisible: this.bottomOn,
      explorerWidth: sizes.explorerWidth,
      bottomHeight: sizes.bottomHeight,
    };
    try {
      store.setItem(LAYOUT_KEY, JSON.stringify(data));
    } catch {
      // ignore — layout persistence is a convenience, not data
    }
  }

  loadLayout(store: Storage | null): Partial<PersistedLayout> {
    if (!store) return {};
    try {
      const raw = store.getItem(LAYOUT_KEY);
      if (!raw) return {};
      const o = JSON.parse(raw) as Partial<PersistedLayout>;
      const out: Partial<PersistedLayout> = {};
      if (o.layout === 'editor' || o.layout === 'split' || o.layout === 'terminal') out.layout = o.layout;
      if (typeof o.explorerVisible === 'boolean') out.explorerVisible = o.explorerVisible;
      if (typeof o.bottomVisible === 'boolean') out.bottomVisible = o.bottomVisible;
      if (typeof o.explorerWidth === 'number' && o.explorerWidth >= 170 && o.explorerWidth <= 480) {
        out.explorerWidth = o.explorerWidth;
      }
      if (typeof o.bottomHeight === 'number' && o.bottomHeight >= 120 && o.bottomHeight <= 900) {
        out.bottomHeight = o.bottomHeight;
      }
      return out;
    } catch {
      return {};
    }
  }
}
