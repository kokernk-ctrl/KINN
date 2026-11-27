// @vitest-environment jsdom
/**
 * kinnrt.js reactivity core under jsdom (0.4.0 S6).
 *
 * Covers the C.5 update pipeline (binding updates, microtask batching,
 * static dependency map), keyed list reconciliation (append / remove /
 * reorder / duplicate keys), delegated event dispatch with the handler
 * ABI + event builtins, and the C.7 error envelope. The runtime under
 * test is the readable `kinnrt.src.js`; the checked-in *minified*
 * artifact `kinnrt.js` (the exact bytes the dist embeds) is byte-identity
 * gated by the compiler-side tests and exercised end-to-end by
 * kinnrt.e2e.test.ts through the real wasm backend.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { JSDOM } from 'jsdom';

const here = dirname(fileURLToPath(import.meta.url));
const RUNTIME_SRC = readFileSync(
  join(here, '../../../compiler/crates/kinc/src/backend/kinnrt.src.js'),
  'utf8',
)
  .replace('function flushNow() {', 'function flushNow() { console.log("FLUSH recs=", recs.length, "dirty=", JSON.stringify(dirty));')
  .replace('function reconcile(r, v) {', 'function reconcile(r, v) { console.log("RECONCILE v=", JSON.stringify(v), "li=", r.li, "fo=", r.fo);');

interface Harness {
  dom: JSDOM;
  K: any;
  main: HTMLElement;
  click(el: Element, key?: string): void;
  input(el: Element, value: string): void;
  texts(selector?: string): string[];
  flush(): Promise<void>;
  envelopes: unknown[];
}

function mount(manifest: unknown, fns: Record<string, (...args: any[]) => unknown>): Harness {
  const dom = new JSDOM('<!doctype html><html><body><main id="main" data-kpage="Home"></main></body></html>', {
    runScripts: 'outside-only',
    url: 'https://kinn.local/index.html',
  });
  const w = dom.window as unknown as Window & typeof globalThis;
  const envelopes: unknown[] = [];
  (w as any).__parent = { postMessage: (m: unknown) => envelopes.push(m) };
  // Pretend to be embedded: window.parent !== window.
  Object.defineProperty(w, 'parent', { value: (w as any).__parent, configurable: true });
  w.eval(RUNTIME_SRC);
  const K = (w as any).KinnRT;
  for (const [name, fn] of Object.entries(fns)) {
    K.reg(0, name, 'test.kin', fn as any);
  }
  K.mount(manifest);
  return {
    dom,
    K,
    main: w.document.querySelector('main') as HTMLElement,
    click(el: Element, key?: string) {
      el.dispatchEvent(
        key
          ? new w.KeyboardEvent('keydown', { bubbles: true, key })
          : new w.MouseEvent('click', { bubbles: true }),
      );
    },
    input(el: Element, value: string) {
      (el as HTMLInputElement).value = value;
      el.dispatchEvent(new w.Event('input', { bubbles: true }));
    },
    texts(selector = 'main p') {
      return Array.from(w.document.querySelectorAll<HTMLElement>(selector))
        .filter((p) => !hidden(p))
        .map((p) => p.textContent);
    },
    flush() {
      return new Promise<void>((res) => setTimeout(res, 10));
    },
    envelopes,
  };
}

function hidden(el: HTMLElement): boolean {
  let n: Node | null = el;
  while (n && (n as HTMLElement).getAttribute) {
    if ((n as HTMLElement).getAttribute('data-off') === '1') return true;
    n = n.parentNode;
  }
  return false;
}

// -- a small manifest builder mirroring webapp.rs emission ------------

function manifest(over: Record<string, unknown>): Record<string, unknown> {
  return {
    animations: { keyframes: [], transitions: [] },
    bindings: [],
    components: [],
    computed: [],
    file: 'test.kin',
    handlers: [],
    name: 'Test',
    navigation: 'mpa',
    pages: [{ name: 'Home', slug: 'home', title: 'Home', view: [] }],
    routes: [],
    state: [],
    stores: [],
    transition: false,
    version: '0.4.0',
    ...over,
  };
}

function bind(fn: string, params: string[], deps: string[], tag: string) {
  return { fn, p: params, d: deps, t: tag };
}

// ===========================================================================
// Binding updates (C.5)
// ===========================================================================

describe('kinnrt binding updates', () => {
  it('renders a dynamic text binding and patches it on state change', async () => {
    const h = mount(
      manifest({
        state: [{ name: 'n', tag: 'i' }],
        bindings: [bind('app:bind:0', ['n'], ['n'], 'i')],
        pages: [
          {
            name: 'Home',
            slug: 'home',
            title: 'Home',
            view: [{ k: 'text', x: { b: 0 }, a: {} }],
          },
        ],
        handlers: [
          { fn: 'app:h', event: 'click', key: false, owner: 'Home', p: ['n'], bi: [] },
        ],
      }),
      {
        'app:bind:0': (n: number) => n * 2,
        'app:h': (n: number) => [n + 1],
        'app:init': () => [21],
      },
    );
    await h.flush();
    expect(h.texts()).toEqual(['42']);
    // Dispatch the click handler (delegated at the document root).
    const btn = document.createElement('button');
    h.main.appendChild(btn);
    (btn as HTMLElement).setAttribute('data-kh', '0');
    h.click(btn);
    await h.flush();
    expect(h.texts()).toEqual(['44']);
  });

  it('static literal slots never become runtime work (C.6)', async () => {
    const h = mount(
      manifest({
        pages: [{ name: 'Home', slug: 'home', title: 'Home', view: [{ k: 'text', x: 'plain', a: {} }] }],
      }),
      { 'app:init': () => [] },
    );
    await h.flush();
    expect(h.texts()).toEqual(['plain']);
    expect(h.main.querySelector('p')!.textContent).toBe('plain');
  });

  it('when branches toggle visibility and hidden bindings stay inert', async () => {
    const h = mount(
      manifest({
        state: [{ name: 'on', tag: 'b' }],
        bindings: [bind('app:bind:0', ['on'], ['on'], 'b')],
        pages: [
          {
            name: 'Home',
            slug: 'home',
            title: 'Home',
            view: [
              {
                k: 'when',
                c: { b: 0 },
                t: [{ k: 'text', x: 'yes', a: {} }],
                o: [{ k: 'text', x: 'no', a: {} }],
              },
            ],
          },
        ],
        handlers: [
          { fn: 'app:h', event: 'click', key: false, owner: 'Home', p: ['on'], bi: [] },
        ],
      }),
      {
        'app:bind:0': (on: boolean) => on,
        'app:h': (on: boolean) => [!on],
        'app:init': () => [true],
      },
    );
    await h.flush();
    expect(h.texts()).toEqual(['yes']);
    const btn = document.createElement('button');
    (btn as HTMLElement).setAttribute('data-kh', '0');
    h.main.appendChild(btn);
    h.click(btn);
    await h.flush();
    expect(h.texts()).toEqual(['no']);
  });

  it('computed values recompute in declaration order from the dep map', async () => {
    const calls: string[] = [];
    const h = mount(
      manifest({
        state: [{ name: 'n', tag: 'i' }],
        computed: [
          { name: 'double', fn: 'app:c1', p: ['n'], d: ['n'] },
          { name: 'quad', fn: 'app:c2', p: ['double'], d: ['n'] },
        ],
        bindings: [bind('app:b', ['quad'], ['n'], 'i')],
        pages: [{ name: 'Home', slug: 'home', title: 'Home', view: [{ k: 'text', x: { b: 0 }, a: {} }] }],
        handlers: [{ fn: 'app:h', event: 'click', key: false, owner: 'Home', p: ['n'], bi: [] }],
      }),
      {
        'app:init': () => [2],
        'app:c1': (n: number) => {
          calls.push('c1');
          return n * 2;
        },
        'app:c2': (d: number) => {
          calls.push('c2');
          return d * 2;
        },
        'app:b': (q: number) => q,
        'app:h': (n: number) => [n + 1],
      },
    );
    await h.flush();
    expect(h.texts()).toEqual(['8']);
    expect(calls).toEqual(['c1', 'c2']);
    const btn = document.createElement('button');
    (btn as HTMLElement).setAttribute('data-kh', '0');
    h.main.appendChild(btn);
    calls.length = 0;
    h.click(btn);
    await h.flush();
    expect(h.texts()).toEqual(['12']);
    expect(calls).toEqual(['c1', 'c2']);
  });
});

// ===========================================================================
// Microtask batching (C.5)
// ===========================================================================

describe('kinnrt flush batching', () => {
  it('coalesces multiple handler dispatches into one DOM write per binding', async () => {
    let evals = 0;
    const h = mount(
      manifest({
        state: [{ name: 'n', tag: 'i' }],
        bindings: [bind('app:b', ['n'], ['n'], 'i')],
        pages: [{ name: 'Home', slug: 'home', title: 'Home', view: [{ k: 'text', x: { b: 0 }, a: {} }] }],
        handlers: [{ fn: 'app:h', event: 'click', key: false, owner: 'Home', p: ['n'], bi: [] }],
      }),
      {
        'app:init': () => [0],
        'app:b': (n: number) => {
          evals++;
          return n;
        },
        'app:h': (n: number) => [n + 1],
      },
    );
    await h.flush();
    expect(h.texts()).toEqual(['0']);
    const before = evals;
    const btn = document.createElement('button');
    (btn as HTMLElement).setAttribute('data-kh', '0');
    h.main.appendChild(btn);
    // Three dispatches in the SAME tick: one microtask flush covers all.
    h.click(btn);
    h.click(btn);
    h.click(btn);
    await h.flush();
    expect(h.texts()).toEqual(['3']);
    // Exactly one binding re-evaluation for the batched flush (the
    // intermediate states never touch the DOM).
    expect(evals - before).toBe(1);
  });
});

// ===========================================================================
// Keyed list reconciliation (C.5/C.2 Lists)
// ===========================================================================

function listHarness() {
  return mount(
    manifest({
      state: [{ name: 'items', tag: 'as' }],
      bindings: [bind('app:iter', ['items'], ['items'], 'as'), bind('app:key', ['item'], [], 's')],
      pages: [
        {
          name: 'Home',
          slug: 'home',
          title: 'Home',
          view: [
            {
              k: 'each',
              i: { b: 0 },
              v: 'item',
              y: { b: 1 },
              b: [{ k: 'text', x: { b: 2 }, a: {} }],
            },
          ],
        },
      ],
      // binding 2: the per-item text
      handlers: [{ fn: 'app:h', event: 'click', key: false, owner: 'Home', p: ['items'], bi: [] }],
    }),
    {
      'app:init': () => [[]],
      'app:iter': (a: string[][]) => a[0],
      'app:key': (a: string[]) => a[0],
      'app:b': (a: string[]) => a[0],
      'app:h': (a: string[][]) => [a[0]],
    },
  );
}

// fix the bindings: binding 2 references `item` (the loop var)
function listManifest() {
  const m = manifest({
    state: [{ name: 'items', tag: 'as' }],
    bindings: [
      bind('app:iter', ['items'], ['items'], 'as'),
      bind('app:key', ['item'], [], 's'),
      bind('app:b', ['item'], [], 's'),
    ],
    pages: [
      {
        name: 'Home',
        slug: 'home',
        title: 'Home',
        view: [
          {
            k: 'each',
            i: { b: 0 },
            v: 'item',
            y: { b: 1 },
            b: [{ k: 'text', x: { b: 2 }, a: {} }],
          },
        ],
      },
    ],
    handlers: [{ fn: 'app:h', event: 'click', key: false, owner: 'Home', p: ['items'], bi: [] }],
  });
  return m;
}

function rowTexts(h: Harness): string[] {
  return Array.from(h.dom.window.document.querySelectorAll('.kinn-each .kinn-item')).map(
    (el) => el.textContent,
  );
}

function setItems(h: Harness, items: string[], _version: number) {
  // Write state through a real handler dispatch: click a data-kh element
  // whose handler returns the new items.
  const btn = h.dom.window.document.createElement('button');
  (btn as HTMLElement).setAttribute('data-kh', '0');
  h.main.appendChild(btn);
  (h.K as any).fns['app:h'] = () => [items];
  h.click(btn);
  return _version;
}

describe('kinnrt keyed reconciliation', () => {
  it('appends, removes and reorders children by key', async () => {
    const h = mount(
      listManifest(),
      {
        'app:init': () => [[]],
        'app:iter': (items: string[]) => items,
        'app:key': (item: string) => item,
        'app:b': (item: string) => item,
        'app:h': (items: string[]) => [items],
      },
    );
    await h.flush();
    expect(rowTexts(h)).toEqual([]);

    setItems(h, ['a', 'b', 'c'], 1);
    await h.flush();
    expect(rowTexts(h)).toEqual(['a', 'b', 'c']);

    // Append.
    setItems(h, ['a', 'b', 'c', 'd'], 2);
    await h.flush();
    expect(rowTexts(h)).toEqual(['a', 'b', 'c', 'd']);

    // Remove the middle child (by key).
    setItems(h, ['a', 'c', 'd'], 3);
    await h.flush();
    expect(rowTexts(h)).toEqual(['a', 'c', 'd']);

    // Reorder: keys move, nodes are re-used (no re-mount of content).
    const before = h.dom.window.document.querySelectorAll('.kinn-item');
    const nodeA = before[0];
    setItems(h, ['d', 'a', 'c'], 4);
    await h.flush();
    const after = h.dom.window.document.querySelectorAll('.kinn-item');
    expect(rowTexts(h)).toEqual(['d', 'a', 'c']);
    // Node identity is preserved for the surviving keys.
    expect(after[1]).toBe(nodeA);
  });

  it('duplicate keys keep the newest value in one slot', async () => {
    const h = mount(
      listManifest(),
      {
        'app:init': () => [[]],
        'app:iter': (items: string[]) => items,
        'app:key': (item: string) => item,
        'app:b': (item: string) => item,
        'app:h': (items: string[]) => [items],
      },
    );
    await h.flush();
    setItems(h, ['a', 'a'], 1);
    await h.flush();
    expect(rowTexts(h)).toEqual(['a']);
    // Only one child exists for the duplicated key.
    expect(h.dom.window.document.querySelectorAll('.kinn-item').length).toBe(1);
  });

  it('in-place mutation of the state array reconciles children', async () => {
    const h = mount(
      listManifest(),
      {
        'app:init': () => [[]],
        'app:iter': (items: string[]) => items,
        'app:key': (item: string) => item,
        'app:b': (item: string) => item,
        'app:h': (items: string[]) => [items],
      },
    );
    await h.flush();
    setItems(h, ['x', 'y'], 1);
    await h.flush();
    expect(rowTexts(h)).toEqual(['x', 'y']);
    // The handler mutates the SAME array reference (index assign) and
    // returns it — the flush fingerprint detects the content change.
    (h.K as any).fns['app:h'] = (items: string[]) => {
      items[0] = 'z';
      return [items];
    };
    const btn = h.dom.window.document.querySelector('[data-kh]') as HTMLElement;
    h.click(btn);
    await h.flush();
    expect(rowTexts(h)).toEqual(['z', 'y']);
  });
});

// ===========================================================================
// Event dispatch + builtins (C.2 Events)
// ===========================================================================

describe('kinnrt event dispatch', () => {
  it('delivers event_value and writes state back through the handler ABI', async () => {
    const h = mount(
      manifest({
        state: [{ name: 'draft', tag: 's' }],
        bindings: [bind('app:draft', ['draft'], ['draft'], 's')],
        pages: [
          {
            name: 'Home',
            slug: 'home',
            title: 'Home',
            view: [{ k: 'input', n: 'draft', x: { b: 0 }, h: [0], a: {} }],
          },
        ],
        handlers: [
          { fn: 'app:oninput', event: 'input', key: false, owner: 'Home', p: ['draft'], bi: ['event_value'] },
        ],
      }),
      {
        'app:init': () => [''],
        'app:draft': (draft: string) => draft,
        'app:oninput': (draft: string, value: string) => [value],
      },
    );
    await h.flush();
    const input = h.main.querySelector('input')!;
    expect((input as HTMLInputElement).value).toBe('');
    h.input(input, 'hello');
    await h.flush();
    expect((input as HTMLInputElement).value).toBe('hello');
  });

  it('key filters route keydown handlers to the matching key', async () => {
    const seen: string[] = [];
    const h = mount(
      manifest({
        state: [{ name: 'n', tag: 'i' }],
        pages: [{ name: 'Home', slug: 'home', title: 'Home', view: [{ k: 'hr', a: {} }] }],
        handlers: [
          { fn: 'app:esc', event: 'keydown', key: 'Escape', owner: 'Home', p: ['n'], bi: [] },
        ],
      }),
      {
        'app:init': () => [0],
        'app:esc': (n: number) => {
          seen.push('esc');
          return [n];
        },
      },
    );
    const el = h.dom.window.document.createElement('div');
    (el as HTMLElement).setAttribute('data-kh', '0');
    h.main.appendChild(el);
    h.click(el, 'Enter');
    await h.flush();
    expect(seen).toEqual([]);
    h.click(el, 'Escape');
    await h.flush();
    expect(seen).toEqual(['esc']);
  });
});

// ===========================================================================
// Error reporting envelope (C.7)
// ===========================================================================

describe('kinnrt error envelope', () => {
  it('a Kin runtime error in a handler posts the envelope and survives', async () => {
    const h = mount(
      manifest({
        state: [{ name: 'n', tag: 'i' }],
        pages: [{ name: 'Home', slug: 'home', title: 'Home', view: [{ k: 'hr', a: {} }] }],
        handlers: [{ fn: 'app:boom', event: 'click', key: false, owner: 'Home', p: ['n'], bi: [] }],
      }),
      {
        'app:init': () => [0],
        'app:boom': () => {
          throw { kin: true, code: 'E4001', message: 'division by zero', line: 12, kinFile: '' };
        },
      },
    );
    const el = h.dom.window.document.createElement('div');
    (el as HTMLElement).setAttribute('data-kh', '0');
    h.main.appendChild(el);
    h.click(el);
    await h.flush();
    expect(h.envelopes.length).toBeGreaterThanOrEqual(1);
    const env = h.envelopes[0] as { kinnRuntimeError: Record<string, unknown> };
    expect(env.kinnRuntimeError.code).toBe('E4001');
    expect(env.kinnRuntimeError.message).toBe('division by zero');
    expect(env.kinnRuntimeError.kinLine).toBe(12);
    // The document still works afterwards.
    expect(h.main.querySelectorAll('hr').length).toBe(1);
  });
});

// ===========================================================================
// MPA state transfer (Hydrator)
// ===========================================================================

describe('kinnrt MPA hydration', () => {
  it('state snapshots persist through sessionStorage', async () => {
    const dom = new JSDOM('<!doctype html><html><body><main data-kpage="Home"></main></body></html>', {
      runScripts: 'outside-only',
      url: 'https://kinn.local/index.html',
    });
    const w = dom.window as any;
    w.eval(RUNTIME_SRC);
    const K = w.KinnRT;
    K.reg(0, 'app:init', 't.kin', () => [7]);
    K.reg(1, 'app:bind:0', 't.kin', (n: number) => n);
    K.mount(
      manifest({
        state: [{ name: 'n', tag: 'i' }],
        bindings: [bind('app:bind:0', ['n'], ['n'], 'i')],
        pages: [{ name: 'Home', slug: 'home', title: 'Home', view: [{ k: 'text', x: { b: 0 }, a: {} }] }],
      }),
    );
    await new Promise((r) => setTimeout(r, 10));
    expect(w.document.querySelector('p').textContent).toBe('7');
    const snap = w.sessionStorage.getItem('kinn:Test');
    expect(snap).toBe('{"n":7}');
  });
});

// ===========================================================================
// Anim (S6 Phase C): enter/exit/hover/press/scroll class coordination
// against the kinnrt.css contract (the visuals themselves are CSS and
// are asserted by the compiler-side dist test; jsdom drives the JS
// coordination only).
// ===========================================================================

describe('kinnrt animation coordination', () => {
  it('applies enter, hover and press classes at mount', async () => {
    const h = mount(
      manifest({
        pages: [
          {
            name: 'Home',
            slug: 'home',
            title: 'Home',
            view: [
              {
                k: 'text',
                x: 'hi',
                a: { enter: 'fade', hover: 'pop', press: 'scale' },
              },
            ],
          },
        ],
      }),
      {},
    );
    await h.flush();
    const p = h.main.querySelector('p') as HTMLElement;
    expect(p.className).toContain('kinn-enter-fade');
    expect(p.className).toContain('kinn-enter-active-fade');
    expect(p.className).toContain('kinn-hover-pop');
    expect(p.className).toContain('kinn-hover-active-pop');
    expect(p.className).toContain('kinn-press-scale');
    expect(p.className).toContain('kinn-press-active-scale');
  });

  it('enter classes land on newly mounted each children', async () => {
    const m = listManifest();
    (m.pages as any)[0].view[0].b[0].a = { enter: 'fade' };
    const h = mount(m, {
      'app:init': () => [[]],
      'app:iter': (items: string[]) => items,
      'app:key': (item: string) => item,
      'app:b': (item: string) => item,
      'app:h': (items: string[]) => [items],
    });
    await h.flush();
    setItems(h, ['a'], 1);
    await h.flush();
    const item = h.dom.window.document.querySelector('.kinn-item p') as HTMLElement;
    expect(item.className).toContain('kinn-enter-fade');
    expect(item.className).toContain('kinn-enter-active-fade');
  });

  it('exit animations hold the node until animationend, then drop it', async () => {
    const m = listManifest();
    (m.pages as any)[0].view[0].b[0].a = { exit: 'fade' };
    const h = mount(m, {
      'app:init': () => [[]],
      'app:iter': (items: string[]) => items,
      'app:key': (item: string) => item,
      'app:b': (item: string) => item,
      'app:h': (items: string[]) => [items],
    });
    await h.flush();
    setItems(h, ['a', 'b'], 1);
    await h.flush();
    expect(rowTexts(h)).toEqual(['a', 'b']);

    // Remove one: the exiting row stays in the DOM with the exit
    // classes while its animation runs.
    setItems(h, ['a'], 2);
    await h.flush();
    const rows = h.dom.window.document.querySelectorAll('.kinn-item');
    expect(rows.length).toBe(2); // still present: the exit wait
    const exiting = rows[1] as HTMLElement;
    expect(exiting.className).toContain('kinn-exit-fade');
    expect(exiting.className).toContain('kinn-exit-active-fade');
    // (a) is the surviving row; (b) is the exiting one.

    // animationend finishes the wait without waiting for the timeout.
    exiting.dispatchEvent(new (h.dom.window as any).Event('animationend'));
    await h.flush();
    expect(h.dom.window.document.querySelectorAll('.kinn-item').length).toBe(1);
    expect(rowTexts(h)).toEqual(['a']);
  });

  it('the app page transition animates main at boot', async () => {
    const h = mount(
      manifest({
        transition: 'fade',
        pages: [
          {
            name: 'Home',
            slug: 'home',
            title: 'Home',
            view: [{ k: 'text', x: 'hi', a: {} }],
          },
        ],
      }),
      {},
    );
    await h.flush();
    expect(h.main.className).toContain('kinn-enter-fade');
    expect(h.main.className).toContain('kinn-enter-active-fade');
  });

  it('scroll references degrade silently without IntersectionObserver', async () => {
    // jsdom has no IntersectionObserver: the class is never applied and
    // the element renders normally (the CSS keeps it hidden ONLY when
    // the class is present, which requires the observer).
    const h = mount(
      manifest({
        pages: [
          {
            name: 'Home',
            slug: 'home',
            title: 'Home',
            view: [{ k: 'text', x: 'hi', a: { scroll: 'scale' } }],
          },
        ],
      }),
      {},
    );
    await h.flush();
    const p = h.main.querySelector('p') as HTMLElement;
    expect(p.className).not.toContain('kinn-scroll-scale');
    expect(p.textContent).toBe('hi');
  });

  it('prefers-reduced-motion skips every animation class', async () => {
    const dom = new JSDOM('<!doctype html><html><body><main id="main" data-kpage="Home"></main></body></html>', {
      runScripts: 'outside-only',
      url: 'https://kinn.local/index.html',
    });
    const w = dom.window as any;
    // jsdom has no matchMedia: stub it as a reduced-motion user.
    w.matchMedia = (q: string) => ({ matches: q.indexOf('reduce') !== -1 });
    w.eval(RUNTIME_SRC);
    const K = w.KinnRT;
    K.reg(0, 'app:init', 't.kin', () => []);
    K.mount(
      manifest({
        transition: 'fade',
        pages: [
          {
            name: 'Home',
            slug: 'home',
            title: 'Home',
            view: [{ k: 'text', x: 'hi', a: { enter: 'fade', hover: 'pop' } }],
          },
        ],
      }),
    );
    await new Promise((r) => setTimeout(r, 10));
    const main = w.document.querySelector('main') as HTMLElement;
    const p = main.querySelector('p') as HTMLElement;
    expect(main.className).not.toContain('kinn-enter');
    expect(p.className).not.toContain('kinn-enter');
    expect(p.className).not.toContain('kinn-hover');
    expect(p.textContent).toBe('hi');
  });
});
