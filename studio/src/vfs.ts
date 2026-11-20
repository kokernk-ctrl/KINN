/**
 * In-memory virtual file system for Studio projects.
 *
 * 0.1 keeps files in memory only (persistence is planned for 0.2 via the
 * File System Access API in the desktop shell). Files are seeded from
 * bundled samples.
 */

export interface KinFile {
  name: string;
  content: string;
  dirty: boolean;
}

type ChangeListener = () => void;

export class Vfs {
  private files = new Map<string, KinFile>();
  private listeners: ChangeListener[] = [];
  activeName: string;

  constructor(projectName: string, seed: Array<[string, string]>) {
    this.projectName = projectName;
    for (const [name, content] of seed) {
      this.files.set(name, { name, content, dirty: false });
    }
    this.activeName = seed.length > 0 ? seed[0][0] : '';
  }

  projectName: string;

  get active(): KinFile | undefined {
    return this.files.get(this.activeName);
  }

  list(): KinFile[] {
    return [...this.files.values()].sort((a, b) => a.name.localeCompare(b.name));
  }

  open(name: string): KinFile | undefined {
    return this.files.get(name);
  }

  setActive(name: string): boolean {
    if (!this.files.has(name)) return false;
    this.activeName = name;
    this.changed();
    return true;
  }

  save(name: string, content: string): void {
    const f = this.files.get(name);
    if (!f) return;
    if (f.content !== content) {
      f.content = content;
      f.dirty = true;
    }
    this.changed();
  }

  /** Mark everything saved (used by Ctrl+S in the in-memory project). */
  markSaved(): void {
    for (const f of this.files.values()) f.dirty = false;
    this.changed();
  }

  create(name: string): boolean {
    if (!/^[\w.-]+\.kin$/.test(name)) return false;
    if (this.files.has(name)) return false;
    this.files.set(name, { name, content: '// ' + name + '\n\n*mode = B;\n\nfunc main() {\n    print("new file");\n}\n', dirty: true });
    this.activeName = name;
    this.changed();
    return true;
  }

  onChange(fn: ChangeListener): void {
    this.listeners.push(fn);
  }

  private changed(): void {
    for (const fn of this.listeners) fn();
  }
}
