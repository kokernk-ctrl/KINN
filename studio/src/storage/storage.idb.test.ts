import { afterEach, describe, expect, it } from 'vitest';
import { IdbStorage } from './storage';
import type { StoredProject } from './storage';

/**
 * Regression tests for the real IndexedDB path, against a spec-fake
 * IndexedDB that mirrors the exact semantics that matter here:
 *
 *  - an object store created with { keyPath } uses IN-LINE keys, and
 *    put(value, key) with a key argument on such a store throws
 *    DataError (spec: "Indexed Database API 3.0", IDBObjectStore.put).
 *    Before 0.4.0 saveProject did exactly that — and persistNow
 *    swallowed the error, so projects never persisted in any browser.
 *  - open(name, version) fires onupgradeneeded only when the version
 *    increases; results and transaction completions arrive
 *    asynchronously.
 *
 * These tests exercise the whole IdbStorage round trip; with the old
 * put({key, project}, key) code, saveProject rejects and every test
 * below fails.
 */

interface FakeRequest<T> {
  result?: T;
  error?: Error;
  onsuccess: (() => void) | null;
  onerror: (() => void) | null;
  onupgradeneeded: (() => void) | null;
}

function later(fn: () => void): void {
  queueMicrotask(fn);
}

class FakeStore {
  readonly data = new Map<string, unknown>();
  constructor(readonly keyPath: string | null) {}

  put(value: unknown, key?: string): FakeRequest<IDBValidKey> {
    // Spec semantics (the part the regression is about): a keyPath
    // store derives the key from the value; passing the key argument
    // is a DataError.
    if (this.keyPath !== null) {
      if (key !== undefined) {
        throw new Error(
          "Failed to execute 'put' on 'IDBObjectStore': The object store uses in-line keys and the key parameter was provided.",
        );
      }
      const record = value as Record<string, unknown>;
      const derived = record[this.keyPath];
      if (derived === undefined) {
        throw new Error("Failed to execute 'put' on 'IDBObjectStore': Evaluating the object store's key path yielded no key.");
      }
      this.data.set(String(derived), structuredClone(value));
    } else {
      if (key === undefined) {
        throw new Error("Failed to execute 'put' on 'IDBObjectStore': The object store uses out-of-line keys and has no key generator and no key parameter was provided.");
      }
      this.data.set(key, structuredClone(value));
    }
    const req: FakeRequest<IDBValidKey> = { onsuccess: null, onerror: null, onupgradeneeded: null };
    later(() => req.onsuccess?.());
    return req;
  }

  get(key: string): FakeRequest<unknown> {
    const req: FakeRequest<unknown> = {
      result: this.data.has(key) ? structuredClone(this.data.get(key)) : undefined,
      onsuccess: null,
      onerror: null,
      onupgradeneeded: null,
    };
    later(() => req.onsuccess?.());
    return req;
  }
}

function installFakeIndexedDB(): { storeNames(): string[]; dump(): Map<string, unknown> } {
  const stores = new Map<string, FakeStore>();
  let version = 0;

  const fakeDb = {
    get objectStoreNames() {
      return { contains: (name: string) => stores.has(name) };
    },
    createObjectStore(name: string, opts?: { keyPath?: string }) {
      const store = new FakeStore(opts?.keyPath ?? null);
      stores.set(name, store);
      return store;
    },
    transaction(name: string, _mode: string) {
      const store = stores.get(name);
      if (!store) throw new Error(`NotFoundError: no such store "${name}"`);
      const tx = {
        objectStore: () => store,
        oncomplete: null as (() => void) | null,
        onerror: null as (() => void) | null,
      };
      later(() => tx.oncomplete?.());
      return tx;
    },
  };

  (globalThis as { indexedDB?: unknown }).indexedDB = {
    open(name: string, requestedVersion: number) {
      const req: FakeRequest<unknown> = {
        onsuccess: null,
        onerror: null,
        onupgradeneeded: null,
        result: fakeDb,
      };
      if (requestedVersion > version) {
        version = requestedVersion;
        later(() => req.onupgradeneeded?.());
      }
      later(() => req.onsuccess?.());
      void name;
      return req;
    },
  };

  return {
    storeNames: () => [...stores.keys()],
    dump: () => {
      const out = new Map<string, unknown>();
      for (const [name, store] of stores) for (const [k, v] of store.data) out.set(`${name}/${k}`, v);
      return out;
    },
  };
}

const PROJECT: StoredProject = {
  name: 'samples',
  active: 'hello.kin',
  files: [
    { name: 'hello.kin', content: '*mode = B;\nfunc main() {}\n' },
    { name: 'mytest.kin', content: '*mode = B;\nfunc main() { print(42); }\n' },
  ],
};

afterEach(() => {
  delete (globalThis as { indexedDB?: unknown }).indexedDB;
});

describe('IdbStorage (spec-fake IndexedDB)', () => {
  it('saves and reloads a project round-trip', async () => {
    installFakeIndexedDB();
    const storage = new IdbStorage();
    await storage.saveProject(PROJECT);
    const loaded = await storage.loadProject();
    expect(loaded).not.toBeNull();
    expect(loaded!.name).toBe('samples');
    expect(loaded!.files.map((f) => f.name)).toEqual(['hello.kin', 'mytest.kin']);
    expect(loaded!.files[1].content).toContain('print(42)');
  });

  it('actually writes the row (the regression: in-line-key put threw silently)', async () => {
    const fake = installFakeIndexedDB();
    const storage = new IdbStorage();
    await storage.saveProject(PROJECT);
    // The row must exist in the store under the in-line key "current".
    expect(fake.storeNames()).toContain('projects');
    const row = fake.dump().get('projects/current') as { key: string; project: string };
    expect(row).toBeDefined();
    expect(row.key).toBe('current');
    // The row holds the versioned .kinnproj JSON (what loadProject
    // parses through parseProject) — not the raw StoredProject object.
    const parsed = JSON.parse(row.project) as { format: string; version: number; name: string };
    expect(parsed.format).toBe('kinnproj');
    expect(parsed.version).toBe(1);
    expect(parsed.name).toBe('samples');
  });

  it('returns null when nothing was stored yet (first visit)', async () => {
    installFakeIndexedDB();
    const storage = new IdbStorage();
    expect(await storage.loadProject()).toBeNull();
  });

  it('creates the object store exactly once across reopens', async () => {
    const fake = installFakeIndexedDB();
    const storage = new IdbStorage();
    await storage.saveProject(PROJECT);
    await storage.loadProject();
    await storage.saveProject(PROJECT);
    expect(fake.storeNames()).toEqual(['projects']);
  });
});
