/**
 * Project explorer: a real collapsible tree (project root node + files),
 * file management (create/rename/delete) and a live project info
 * section. The VFS is a flat name.kin project — folders are not faked;
 * the tree collapses on nodes that exist.
 */

import type { Vfs } from '../../vfs';
import type { WorkspaceState } from '../state';

export interface ExplorerCallbacks {
  onOpen(name: string): void;
  onNewFile(): void;
  onRename(oldName: string, newName: string): void;
  onDelete(name: string): void;
}

export class NextExplorer {
  private tree: HTMLElement;
  private info: HTMLElement;
  private rootOpen = true;
  private infoOpen = true;

  constructor(
    private vfs: Vfs,
    private state: WorkspaceState,
    root: HTMLElement,
    private cb: ExplorerCallbacks,
  ) {
    this.tree = root.querySelector('#nx-tree') as HTMLElement;
    this.info = root.querySelector('#nx-info') as HTMLElement;
    (root.querySelector('#nx-btn-newfile') as HTMLButtonElement).addEventListener('click', () => this.cb.onNewFile());
    (root.querySelector('#nx-explorer-files-head') as HTMLElement).addEventListener('click', () => {
      this.rootOpen = !this.rootOpen;
      this.render();
    });
    (root.querySelector('#nx-explorer-info-head') as HTMLElement).addEventListener('click', () => {
      this.infoOpen = !this.infoOpen;
      this.render();
    });
    vfs.onChange(() => this.render());
    state.onChange(() => this.render());
    this.render();
  }

  render(): void {
    this.renderTree();
    this.renderInfo();
  }

  private renderTree(): void {
    this.tree.replaceChildren();

    const rootBtn = document.createElement('button');
    rootBtn.type = 'button';
    rootBtn.className = 'nx-tree-row nx-tree-root';
    rootBtn.innerHTML =
      `<span class="nx-chevron${this.rootOpen ? ' open' : ''}" aria-hidden="true"></span>` +
      `<span class="nx-tree-icon" aria-hidden="true"></span>`;
    const rootLabel = document.createElement('span');
    rootLabel.className = 'nx-tree-label';
    rootLabel.textContent = this.vfs.projectName;
    rootBtn.appendChild(rootLabel);
    rootBtn.addEventListener('click', () => {
      this.rootOpen = !this.rootOpen;
      this.render();
    });
    this.tree.appendChild(rootBtn);

    if (!this.rootOpen) return;

    for (const file of this.vfs.list()) {
      const row = document.createElement('div');
      row.className = 'nx-tree-row nx-tree-file' + (file.name === this.state.active ? ' active' : '');
      row.setAttribute('role', 'treeitem');

      const openBtn = document.createElement('button');
      openBtn.type = 'button';
      openBtn.className = 'nx-tree-open';
      openBtn.appendChild(Object.assign(document.createElement('span'), { className: 'nx-tree-icon', textContent: '' }));
      const label = document.createElement('span');
      label.className = 'nx-tree-label';
      label.textContent = file.name;
      openBtn.appendChild(label);
      openBtn.addEventListener('click', () => this.cb.onOpen(file.name));
      row.appendChild(openBtn);

      const actions = document.createElement('span');
      actions.className = 'nx-tree-actions';
      const ren = document.createElement('button');
      ren.type = 'button';
      ren.className = 'nx-tree-action';
      ren.textContent = '✎';
      ren.title = `rename ${file.name}`;
      ren.setAttribute('aria-label', `rename ${file.name}`);
      ren.addEventListener('click', (e) => {
        e.stopPropagation();
        const next = window.prompt(`rename ${file.name} to:`, file.name);
        if (next && next !== file.name) this.cb.onRename(file.name, next);
      });
      const del = document.createElement('button');
      del.type = 'button';
      del.className = 'nx-tree-action';
      del.textContent = '✕';
      del.title = `delete ${file.name}`;
      del.setAttribute('aria-label', `delete ${file.name}`);
      del.addEventListener('click', (e) => {
        e.stopPropagation();
        if (window.confirm(`delete ${file.name}?`)) this.cb.onDelete(file.name);
      });
      actions.appendChild(ren);
      actions.appendChild(del);
      row.appendChild(actions);

      if (this.state.isModified(file.name)) {
        const dot = document.createElement('span');
        dot.className = 'nx-modified-dot';
        dot.title = 'modified since last explicit save (Ctrl+S)';
        row.appendChild(dot);
      }

      this.tree.appendChild(row);
    }
  }

  private renderInfo(): void {
    this.info.replaceChildren();
    if (!this.infoOpen) return;
    const files = this.vfs.list();
    const rows: Array<[string, string]> = [
      ['project', this.vfs.projectName],
      ['files', String(files.length)],
      ['active', this.state.active || '—'],
      ['compiler', this.compilerFact],
      ['storage', this.storageFact],
    ];
    for (const [k, v] of rows) {
      const div = document.createElement('div');
      div.className = 'nx-info-row';
      const key = document.createElement('span');
      key.className = 'nx-info-key';
      key.textContent = k;
      const val = document.createElement('span');
      val.className = 'nx-info-val';
      val.textContent = v;
      div.appendChild(key);
      div.appendChild(val);
      this.info.appendChild(div);
    }
  }

  /** Compiler/storage lines updated by the host when known. */
  setFacts(compiler: string, storage: string): void {
    this.compilerFact = compiler;
    this.storageFact = storage;
    this.renderInfo();
  }

  private compilerFact = '…';
  private storageFact = '…';
}
