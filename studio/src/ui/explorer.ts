/**
 * Project explorer: file tree over the virtual file system.
 */

import type { Vfs } from '../vfs';

export class Explorer {
  private tree: HTMLElement;

  constructor(private vfs: Vfs, tree: HTMLElement, private onOpen: (name: string) => void) {
    this.tree = tree;
    vfs.onChange(() => this.render());
    this.render();
  }

  render(): void {
    this.tree.replaceChildren();
    for (const file of this.vfs.list()) {
      const item = document.createElement('button');
      item.type = 'button';
      item.className = 'tree-item' + (file.name === this.vfs.activeName ? ' active' : '') + (file.dirty ? ' dirty' : '');
      item.setAttribute('role', 'treeitem');
      const label = document.createElement('span');
      label.textContent = file.name;
      item.appendChild(label);
      const dot = document.createElement('span');
      dot.className = 'dot';
      dot.title = 'modified (in-memory project)';
      item.appendChild(dot);
      item.addEventListener('click', () => {
        this.vfs.setActive(file.name);
        this.onOpen(file.name);
      });
      this.tree.appendChild(item);
    }
  }
}
