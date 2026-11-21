/**
 * Virtual file system for Studio projects.
 *
 * Since 0.2.0 the Vfs persists through a ProjectStorage adapter
 * (IndexedDB in browsers): every change schedules a debounced save, so
 * work survives reloads without any server. The in-memory model stays
 * synchronous; persistence is fire-and-forget with an explicit
 * `flush()` for tests and `save now`.
 */

import type { ProjectFile } from './wasm/kinnwasm';
import type { ProjectStorage, StoredProject } from './storage/storage';

export interface KinFile {
  name: string;
  content: string;
  dirty: boolean;
}

type ChangeListener = () => void;

export class Vfs {
  private files = new Map<string, KinFile>();
  private listeners: ChangeListener[] = [];
  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private pending = false;
  activeName: string;
  projectName: string;

  constructor(projectName: string, seed: Array<[string, string]>, private storage?: ProjectStorage) {
    this.projectName = projectName;
    for (const [name, content] of seed) {
      this.files.set(name, { name, content, dirty: false });
    }
    this.activeName = seed.length > 0 ? seed[0][0] : '';
  }

  /** Replace the whole project (loading from storage or importing). */
  loadProject(project: StoredProject): void {
    this.files.clear();
    for (const f of project.files) {
      this.files.set(f.name, { name: f.name, content: f.content, dirty: false });
    }
    this.activeName = project.active && this.files.has(project.active)
      ? project.active
      : (project.files[0]?.name ?? '');
    this.projectName = project.name;
    this.changed();
    this.scheduleSave();
  }

  get active(): KinFile | undefined {
    return this.files.get(this.activeName);
  }

  list(): KinFile[] {
    return [...this.files.values()].sort((a, b) => a.name.localeCompare(b.name));
  }

  /** All files in the export/compile shape. */
  projectFiles(): ProjectFile[] {
    return this.list().map((f) => ({ name: f.name, content: f.content }));
  }

  open(name: string): KinFile | undefined {
    return this.files.get(name);
  }

  setActive(name: string): boolean {
    if (!this.files.has(name)) return false;
    this.activeName = name;
    this.changed();
    this.scheduleSave();
    return true;
  }

  save(name: string, content: string): void {
    const f = this.files.get(name);
    if (!f) return;
    if (f.content !== content) {
      f.content = content;
      f.dirty = false; // content is persisted (debounced), not "dirty"
    }
    this.changed();
    this.scheduleSave();
  }

  /** Mark everything saved (Ctrl+S: flush to storage immediately). */
  async markSaved(): Promise<void> {
    for (const f of this.files.values()) f.dirty = false;
    this.changed();
    await this.flush();
  }

  create(name: string): boolean {
    if (!this.isValidName(name)) return false;
    if (this.files.has(name)) return false;
    this.files.set(name, { name, content: this.template(name), dirty: false });
    this.activeName = name;
    this.changed();
    this.scheduleSave();
    return true;
  }

  /** Add or replace a file from an import. */
  importFile(name: string, content: string): boolean {
    if (!this.isValidName(name)) return false;
    const existed = this.files.has(name);
    this.files.set(name, { name, content, dirty: false });
    if (!existed) this.activeName = name;
    this.changed();
    this.scheduleSave();
    return true;
  }

  rename(oldName: string, newName: string): boolean {
    if (!this.files.has(oldName)) return false;
    if (!this.isValidName(newName) || this.files.has(newName)) return false;
    const f = this.files.get(oldName)!;
    this.files.delete(oldName);
    this.files.set(newName, { ...f, name: newName });
    if (this.activeName === oldName) this.activeName = newName;
    this.changed();
    this.scheduleSave();
    return true;
  }

  remove(name: string): boolean {
    if (!this.files.delete(name)) return false;
    if (this.files.size === 0) {
      this.activeName = '';
    } else if (this.activeName === name) {
      this.activeName = this.list()[0].name;
    }
    this.changed();
    this.scheduleSave();
    return true;
  }

  isValidName(name: string): boolean {
    return /^[\w.-]+\.kin$/.test(name);
  }

  onChange(fn: ChangeListener): void {
    this.listeners.push(fn);
  }

  toStoredProject(): StoredProject {
    return {
      name: this.projectName,
      active: this.activeName,
      files: this.list().map((f) => ({ name: f.name, content: f.content })),
    };
  }

  /** Wait for any debounced save to finish (tests, `save` command). */
  async flush(): Promise<void> {
    if (this.saveTimer !== null) {
      clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }
    if (this.pending) {
      this.pending = false;
      await this.persistNow();
    }
  }

  private template(name: string): string {
    return `// ${name}\n\n*mode = B;\n\nfunc main() {\n    print("new file");\n}\n`;
  }

  private scheduleSave(): void {
    if (!this.storage) return;
    this.pending = true;
    if (this.saveTimer !== null) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null;
      void this.persistNow();
    }, 400);
  }

  private async persistNow(): Promise<void> {
    if (!this.storage) return;
    this.pending = false;
    try {
      await this.storage.saveProject(this.toStoredProject());
    } catch {
      // Storage can fail (private mode, quota); the in-memory project
      // keeps working. The UI shows the storage state separately.
    }
  }

  private changed(): void {
    for (const fn of this.listeners) fn();
  }
}
