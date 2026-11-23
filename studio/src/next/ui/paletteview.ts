/**
 * Command palette view: one palette, two modes — commands and file
 * search. It only lists commands handed to it by the host; a command
 * that cannot run is never registered (spec: no fake actions).
 */

import { filterCommands, filterNames, type PaletteCommand } from '../palette';

type PaletteMode = 'commands' | 'files';

export interface PaletteViewCallbacks {
  onPickFile(name: string): void;
  onClose(): void;
}

export class PaletteView {
  private backdrop: HTMLElement;
  private input: HTMLInputElement;
  private list: HTMLElement;
  private title: HTMLElement;
  private commands: PaletteCommand[] = [];
  private fileNames: string[] = [];
  private mode: PaletteMode = 'commands';
  private sel = 0;
  private items: Array<{ kind: 'cmd'; cmd: PaletteCommand } | { kind: 'file'; name: string }> = [];

  constructor(backdrop: HTMLElement, private cb: PaletteViewCallbacks) {
    this.backdrop = backdrop;
    this.input = backdrop.querySelector('#nx-palette-input') as HTMLInputElement;
    this.list = backdrop.querySelector('#nx-palette-list') as HTMLElement;
    this.title = backdrop.querySelector('#nx-palette-title') as HTMLElement;

    this.input.addEventListener('input', () => {
      this.sel = 0;
      this.render();
    });
    this.input.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        this.sel = Math.min(this.sel + 1, this.items.length - 1);
        this.render();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        this.sel = Math.max(0, this.sel - 1);
        this.render();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        this.pickSelected();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        this.close();
        this.cb.onClose();
      }
    });
    backdrop.addEventListener('click', (e) => {
      if (e.target === backdrop) {
        this.close();
        this.cb.onClose();
      }
    });
  }

  showCommands(commands: PaletteCommand[]): void {
    this.mode = 'commands';
    this.commands = commands;
    this.input.value = '';
    this.title.textContent = 'commands';
    this.sel = 0;
    this.backdrop.hidden = false;
    this.render();
    this.input.focus();
  }

  showFiles(names: string[]): void {
    this.mode = 'files';
    this.fileNames = names;
    this.input.value = '';
    this.title.textContent = 'go to file';
    this.sel = 0;
    this.backdrop.hidden = false;
    this.render();
    this.input.focus();
  }

  close(): void {
    this.backdrop.hidden = true;
  }

  get visible(): boolean {
    return !this.backdrop.hidden;
  }

  private pickSelected(): void {
    const item = this.items[this.sel];
    if (!item) return;
    this.close();
    if (item.kind === 'cmd') item.cmd.run();
    else this.cb.onPickFile(item.name);
  }

  private render(): void {
    const q = this.input.value;
    this.items = [];
    this.list.replaceChildren();

    if (this.mode === 'commands') {
      const filtered = filterCommands(this.commands, q);
      for (const cmd of filtered.slice(0, 60)) {
        this.items.push({ kind: 'cmd', cmd });
      }
    } else {
      const filtered = filterNames(this.fileNames, q);
      for (const name of filtered.slice(0, 60)) {
        this.items.push({ kind: 'file', name });
      }
    }

    if (this.items.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'nx-palette-empty';
      empty.textContent = this.mode === 'commands' ? 'no matching command' : 'no matching file';
      this.list.appendChild(empty);
      return;
    }

    this.items.forEach((item, i) => {
      const row = document.createElement('button');
      row.type = 'button';
      row.className = 'nx-palette-item' + (i === this.sel ? ' selected' : '');
      if (item.kind === 'cmd') {
        const g = document.createElement('span');
        g.className = 'nx-palette-group';
        g.textContent = item.cmd.group;
        const t = document.createElement('span');
        t.className = 'nx-palette-title';
        t.textContent = item.cmd.title;
        row.appendChild(g);
        row.appendChild(t);
        if (item.cmd.hint) {
          const h = document.createElement('span');
          h.className = 'nx-palette-hint';
          h.textContent = item.cmd.hint;
          row.appendChild(h);
        }
        row.addEventListener('click', () => {
          this.close();
          item.cmd.run();
        });
      } else {
        const t = document.createElement('span');
        t.className = 'nx-palette-title';
        t.textContent = item.name;
        row.appendChild(t);
        row.addEventListener('click', () => {
          this.close();
          this.cb.onPickFile(item.name);
        });
      }
      this.list.appendChild(row);
    });

    const selEl = this.list.children[this.sel] as HTMLElement | undefined;
    selEl?.scrollIntoView({ block: 'nearest' });
  }
}
