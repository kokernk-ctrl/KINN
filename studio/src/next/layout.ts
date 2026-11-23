/**
 * Workspace layout controller.
 *
 * Desktop: three-area grid (explorer | editor / bottom) with draggable
 * dividers that write CSS variables and persist sizes; layout focus
 * modes (editor / split / terminal) and panel toggles come from
 * WorkspaceState and are applied as data attributes on the root.
 *
 * Mobile (<880px): the areas become full-screen views switched by the
 * bottom navigation; the keyboard handling keeps the terminal input
 * visible by following the visual viewport.
 */

import type { WorkspaceState } from './state';

const CLAMP = {
  explorerWidth: { min: 170, max: 480 },
  bottomHeight: { min: 120, max: 900 },
} as const;

export class Layout {
  private root: HTMLElement;
  private explorerWidth = 232;
  private bottomHeight = 300;
  private store: Storage | null;

  constructor(
    state: WorkspaceState,
    root: HTMLElement,
    store: Storage | null,
    private onLayoutChanged: () => void,
  ) {
    this.root = root;
    this.store = store;

    const persisted = state.loadLayout(store);
    if (persisted.explorerWidth !== undefined) this.explorerWidth = persisted.explorerWidth;
    if (persisted.bottomHeight !== undefined) this.bottomHeight = persisted.bottomHeight;
    if (persisted.layout !== undefined) state.setLayout(persisted.layout);
    if (persisted.explorerVisible !== undefined) state.setExplorer(persisted.explorerVisible);
    if (persisted.bottomVisible !== undefined) state.setBottom(persisted.bottomVisible);

    state.onChange(() => this.apply(state));

    this.bindHandle(state, 'nx-handle-v', 'x');
    this.bindHandle(state, 'nx-handle-h', 'y');

    window.addEventListener('resize', () => this.apply(state));
    this.apply(state);
  }

  private bindHandle(state: WorkspaceState, id: string, axis: 'x' | 'y'): void {
    const handle = document.getElementById(id);
    if (!handle) return;
    handle.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      handle.setPointerCapture(e.pointerId);
      handle.classList.add('dragging');
      document.body.classList.add('nx-resizing');
      const move = (ev: PointerEvent): void => {
        const bench = document.getElementById('nx-workbench');
        if (!bench) return;
        const rect = bench.getBoundingClientRect();
        if (axis === 'x') {
          this.explorerWidth = Math.round(
            Math.min(CLAMP.explorerWidth.max, Math.max(CLAMP.explorerWidth.min, ev.clientX - rect.left)),
          );
        } else {
          this.bottomHeight = Math.round(
            Math.min(CLAMP.bottomHeight.max, Math.max(CLAMP.bottomHeight.min, rect.bottom - ev.clientY)),
          );
        }
        this.apply(state);
      };
      const up = (): void => {
        handle.classList.remove('dragging');
        document.body.classList.remove('nx-resizing');
        handle.removeEventListener('pointermove', move);
        handle.removeEventListener('pointerup', up);
        this.persist(state);
      };
      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', up);
    });
  }

  private apply(state: WorkspaceState): void {
    const snap = state.snapshot();
    this.root.dataset.layout = snap.layout;
    this.root.dataset.explorer = String(snap.explorerVisible);
    this.root.dataset.bottom = String(snap.bottomVisible);
    this.root.style.setProperty('--nx-explorer-w', `${this.explorerWidth}px`);
    this.root.style.setProperty('--nx-bottom-h', `${this.bottomHeight}px`);
    // highlight the active layout button
    for (const btn of Array.from(this.root.querySelectorAll<HTMLButtonElement>('.nx-layout-btn'))) {
      btn.classList.toggle('active', btn.dataset.mode === snap.layout);
    }
  }

  private persist(state: WorkspaceState): void {
    state.saveLayout(this.store, { explorerWidth: this.explorerWidth, bottomHeight: this.bottomHeight });
    this.onLayoutChanged();
  }

  // -- mobile ------------------------------------------------------------------

  /**
   * Mobile view switching: data-mobile on the root chooses which area
   * fills the screen (files | editor | term | out).
   */
  static setMobileView(root: HTMLElement, view: string): void {
    root.dataset.mobile = view;
  }

  /**
   * Track the visual viewport so the terminal input stays usable when
   * the soft keyboard opens (Android). Sets --nx-vvh; CSS uses it as
   * the app height instead of 100vh.
   */
  static trackVisualViewport(root: HTMLElement): void {
    const vv = window.visualViewport;
    if (!vv) return;
    const update = (): void => {
      root.style.setProperty('--nx-vvh', `${vv.height}px`);
    };
    vv.addEventListener('resize', update);
    update();
  }
}
