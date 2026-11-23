/**
 * Editor tab strip: one tab per open file with a modified dot and a
 * close button. Rendered from WorkspaceState, so state and view can
 * never disagree.
 */

import type { WorkspaceState } from '../state';

export interface EditorTabCallbacks {
  onActivate(name: string): void;
  onClose(name: string): void;
}

export class EditorTabs {
  private strip: HTMLElement;

  constructor(state: WorkspaceState, strip: HTMLElement, private cb: EditorTabCallbacks) {
    this.strip = strip;
    state.onChange(() => this.render(state));
    this.render(state);
  }

  private render(state: WorkspaceState): void {
    this.strip.replaceChildren();
    for (const name of state.openTabs) {
      const tab = document.createElement('div');
      tab.className = 'nx-tab' + (name === state.active ? ' active' : '');
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'nx-tab-btn';
      btn.textContent = name;
      btn.title = name;
      btn.addEventListener('click', () => this.cb.onActivate(name));
      tab.appendChild(btn);

      if (state.isModified(name)) {
        const dot = document.createElement('span');
        dot.className = 'nx-tab-dot';
        dot.title = 'modified since last explicit save (Ctrl+S)';
        tab.appendChild(dot);
      }

      const close = document.createElement('button');
      close.type = 'button';
      close.className = 'nx-tab-close';
      close.textContent = '✕';
      close.setAttribute('aria-label', `close ${name}`);
      close.title = 'close tab';
      close.addEventListener('click', (e) => {
        e.stopPropagation();
        this.cb.onClose(name);
      });
      tab.appendChild(close);

      this.strip.appendChild(tab);
    }
    if (state.openTabs.length === 0) {
      const empty = document.createElement('span');
      empty.className = 'nx-tabs-empty';
      empty.textContent = 'no file open — pick one in the explorer';
      this.strip.appendChild(empty);
    }
  }
}
