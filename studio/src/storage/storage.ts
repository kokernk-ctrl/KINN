/**
 * Project persistence for KINN Studio.
 *
 * The `.kinnproj` format is plain JSON (versioned) and is used both by
 * the IndexedDB persistence and by export/import files:
 *
 * ```json
 * { "format": "kinnproj", "version": 1, "name": "samples",
 *   "active": "hello.kin",
 *   "files": [{ "name": "hello.kin", "content": "..." }] }
 * ```
 *
 * Studio keeps zero runtime dependencies: IndexedDB is used directly.
 */

import type { ProjectFile } from '../wasm/kinnwasm';

export interface StoredProject {
  name: string;
  active: string;
  files: ProjectFile[];
}

export interface ProjectStorage {
  loadProject(): Promise<StoredProject | null>;
  saveProject(project: StoredProject): Promise<void>;
}

const DB_NAME = 'kinn-studio';
const STORE = 'projects';
const KEY = 'current';

/** In-memory storage (tests, and hosts without IndexedDB). */
export class MemoryStorage implements ProjectStorage {
  private data: StoredProject | null = null;

  async loadProject(): Promise<StoredProject | null> {
    return this.data ? structuredClone(this.data) : null;
  }

  async saveProject(project: StoredProject): Promise<void> {
    this.data = structuredClone(project);
  }
}

/** IndexedDB-backed storage (browsers, Android WebView). */
export class IdbStorage implements ProjectStorage {
  async loadProject(): Promise<StoredProject | null> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, 'readonly');
      const req = tx.objectStore(STORE).get(KEY);
      req.onsuccess = () => {
        const row = req.result as { project?: unknown } | undefined;
        const parsed = row?.project ? parseProject(row.project) : null;
        resolve(parsed);
      };
      req.onerror = () => reject(req.error);
    });
  }

  async saveProject(project: StoredProject): Promise<void> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, 'readwrite');
      tx.objectStore(STORE).put({ key: KEY, project }, KEY);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  }

  private open(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
      const req = indexedDB.open(DB_NAME, 1);
      req.onupgradeneeded = () => {
        if (!req.result.objectStoreNames.contains(STORE)) {
          req.result.createObjectStore(STORE, { keyPath: 'key' });
        }
      };
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  }
}

/** Pick the best available storage for this host. */
export function defaultStorage(): ProjectStorage {
  if (typeof indexedDB !== 'undefined') return new IdbStorage();
  return new MemoryStorage();
}

// ---------------------------------------------------------------------------
// .kinnproj format
// ---------------------------------------------------------------------------

const FORMAT = 'kinnproj';
const FORMAT_VERSION = 1;

/** Serialize a project to the .kinnproj JSON text. */
export function serializeProject(p: StoredProject): string {
  return JSON.stringify(
    {
      format: FORMAT,
      version: FORMAT_VERSION,
      name: p.name,
      active: p.active,
      files: p.files.map((f) => ({ name: f.name, content: f.content })),
    },
    null,
    2,
  );
}

/**
 * Parse and validate .kinnproj JSON. Throws Error with a readable
 * message when the data is not a valid project.
 */
export function parseProject(data: unknown): StoredProject {
  if (typeof data === 'string') {
    try {
      data = JSON.parse(data);
    } catch (e) {
      throw new Error(`not valid JSON: ${(e as Error).message}`);
    }
  }
  if (typeof data !== 'object' || data === null) {
    throw new Error('project must be a JSON object');
  }
  const obj = data as Record<string, unknown>;
  if (obj.format !== FORMAT) {
    throw new Error(`missing "format": "${FORMAT}" — is this a .kinnproj file?`);
  }
  if (typeof obj.version !== 'number' || obj.version > FORMAT_VERSION) {
    throw new Error(`unsupported .kinnproj version ${String(obj.version)} (this Studio reads version <= ${FORMAT_VERSION})`);
  }
  if (typeof obj.name !== 'string' || obj.name.length === 0) {
    throw new Error('project "name" must be a non-empty string');
  }
  if (!Array.isArray(obj.files) || obj.files.length === 0) {
    throw new Error('project "files" must be a non-empty array');
  }
  const files: ProjectFile[] = [];
  const seen = new Set<string>();
  for (const raw of obj.files) {
    if (typeof raw !== 'object' || raw === null) {
      throw new Error('each file must be an object with "name" and "content"');
    }
    const f = raw as Record<string, unknown>;
    if (typeof f.name !== 'string' || !/^[\w.-]+\.kin$/.test(f.name)) {
      throw new Error(`invalid file name: ${String(f.name)} (expected name.kin)`);
    }
    if (typeof f.content !== 'string') {
      throw new Error(`file ${f.name} is missing string "content"`);
    }
    if (seen.has(f.name)) throw new Error(`duplicate file name: ${f.name}`);
    seen.add(f.name);
    files.push({ name: f.name, content: f.content });
  }
  const active = typeof obj.active === 'string' && seen.has(obj.active)
    ? obj.active
    : files[0].name;
  return { name: obj.name, active, files };
}
