// @vitest-environment jsdom
/**
 * End-to-end: the S6 todo app built by the REAL wasm backend, previewed
 * exactly like Studio previews it (buildPreviewHtml inlines everything),
 * and driven through real DOM events. This exercises the *shipped*
 * minified kinnrt.js (the bytes the dist embeds), the generated app.js,
 * the manifest contract and the C.7 envelopes together.
 */
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { JSDOM } from 'jsdom';
import { backend } from '../wasm/kinnwasm';
import { buildPreviewHtml } from './preview';
import { isRuntimeErrorEnvelope } from './preview-events';

const here = dirname(fileURLToPath(import.meta.url));
const hasWasm = (() => {
  try {
    readFileSync(join(here, '../../public/kinn.wasm'));
    return true;
  } catch {
    return false;
  }
})();

const TODO_SRC = readFileSync(join(here, '../../../examples/web/todo_app.kin'), 'utf8');

describe.skipIf(!hasWasm)('todo app end to end (real wasm backend)', () => {
  it('builds, mounts, reacts and reports through the real pipeline', async () => {
    await backend.loadBytes(
      readFileSync(join(here, '../../public/kinn.wasm')),
    );
    const result = await backend.buildWeb('todo_app.kin', [{ name: 'todo_app.kin', content: TODO_SRC }]);
    expect(result.ok).toBe(true);

    // The dynamic dist: semantic HTML skeleton + runtime + app.js + the
    // predefined animation sheet (S6 Phase C).
    const paths = result.files.map((f) => f.path).sort();
    expect(paths).toEqual(['about.html', 'app.js', 'index.html', 'kinnrt.css', 'kinnrt.js', 'styles.css']);

    // Studio's preview: self-contained page, scripts inlined in order.
    const preview = buildPreviewHtml(result.files, 'index.html');
    expect(preview).toContain('data-kpage="Home"');
    const rtAt = preview.indexOf('KinnRT');
    const mountAt = preview.indexOf('K.mount(');
    expect(rtAt).toBeGreaterThan(-1);
    expect(mountAt).toBeGreaterThan(rtAt); // runtime before app.js

    const dom = new JSDOM(preview, {
      runScripts: 'outside-only',
      url: 'https://kinn.local/index.html',
    });
    const w = dom.window as any;
    const envelopes: unknown[] = [];
    Object.defineProperty(w, 'parent', { value: { postMessage: (m: unknown) => envelopes.push(m) }, configurable: true });
    // The preview interceptor is a real script — evaluate the document.
    const scripts = Array.from(w.document.querySelectorAll('script'));
    for (const sc of scripts) {
      try {
        w.eval(sc.textContent || '');
      } catch (e) {
        // The navigation interceptor re-declaration is idempotent; any
        // real failure fails the assertions below.
      }
    }
    const wait = () => new Promise((r) => setTimeout(r, 15));
    await wait();

    const text = () =>
      Array.from(w.document.querySelectorAll<HTMLElement>('main p'))
        .filter((p) => !hidden(p))
        .map((p) => p.textContent);
    const items = () =>
      Array.from(w.document.querySelectorAll('.kinn-each .kinn-item')).map((e) => e.textContent);
    function hidden(el: HTMLElement): boolean {
      let n: Node | null = el;
      while (n && (n as HTMLElement).getAttribute) {
        if ((n as HTMLElement).getAttribute('data-off') === '1') return true;
        n = n.parentNode;
      }
      return false;
    }

    // Initial state: empty list, empty-state message visible.
    expect(items()).toEqual([]);
    expect(text()).toContain('Open: 0 of 0');
    expect(text()).toContain('Nothing here yet — add your first task above.');

    // Type and add two tasks.
    const input = w.document.querySelector('input[data-kb]');
    const add = Array.from(w.document.querySelectorAll('button.kinn-button')).find(
      (b: HTMLElement) => b.textContent === 'Add',
    );
    const addTask = async (label: string) => {
      input.value = label;
      input.dispatchEvent(new w.Event('input', { bubbles: true }));
      add.dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
      await wait();
    };
    await addTask('write the report');
    await addTask('ship the release');

    expect(items()).toEqual(['write the report', 'ship the release']);
    expect(text()).toContain('Open: 2 of 2');
    expect(text()).not.toContain('Nothing here yet');

    // S6 Phase C: the added items carry the predefined fade enter
    // classes (the visuals come from the inlined kinnrt.css).
    const lastItem = w.document.querySelectorAll('.kinn-each .kinn-item p');
    expect((lastItem[lastItem.length - 1] as HTMLElement).className).toContain('kinn-enter-fade');
    expect((lastItem[lastItem.length - 1] as HTMLElement).className).toContain(
      'kinn-enter-active-fade',
    );

    // The state snapshot (MPA transfer) landed in sessionStorage.
    const snap = JSON.parse(w.sessionStorage.getItem('kinn:Todo') || '{}');
    expect(snap.items).toEqual(['write the report', 'ship the release']);

    // No runtime errors surfaced through the envelope channel.
    expect(envelopes.filter(isRuntimeErrorEnvelope)).toEqual([]);
  });
});

describe.skipIf(!hasWasm)('runtime error app (real wasm backend)', () => {
  it('reports E4001 with the source line through the envelope', async () => {
    const src = readFileSync(join(here, '../../../examples/errors/app_div_zero.kin'), 'utf8');
    await backend.loadBytes(readFileSync(join(here, '../../public/kinn.wasm')));
    const result = await backend.buildWeb('app_div_zero.kin', [{ name: 'app_div_zero.kin', content: src }]);
    expect(result.ok).toBe(true);
    const preview = buildPreviewHtml(result.files, 'index.html');
    const dom = new JSDOM(preview, { runScripts: 'outside-only', url: 'https://kinn.local/index.html' });
    const w = dom.window as any;
    const envelopes: unknown[] = [];
    Object.defineProperty(w, 'parent', { value: { postMessage: (m: unknown) => envelopes.push(m) }, configurable: true });
    for (const sc of Array.from(w.document.querySelectorAll('script'))) {
      try {
        w.eval(sc.textContent || '');
      } catch {
        /* idempotent interceptor */
      }
    }
    await new Promise((r) => setTimeout(r, 15));
    const btn = w.document.querySelector('button[data-kh]');
    btn.dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 15));
    const errs = envelopes.filter(isRuntimeErrorEnvelope) as Array<{
      kinnRuntimeError: { code: string; message: string; kinLine: number };
    }>;
    expect(errs.length).toBeGreaterThanOrEqual(1);
    expect(errs[0].kinnRuntimeError.code).toBe('E4001');
    expect(errs[0].kinnRuntimeError.message).toBe('division by zero');
    expect(errs[0].kinnRuntimeError.kinLine).toBeGreaterThan(0);
  });
});

// ---------------------------------------------------------------------------
// S7: the SPA app end to end (real wasm backend, real router).
// ---------------------------------------------------------------------------
const SPA_SRC = readFileSync(join(here, '../../../examples/web/spa_app.kin'), 'utf8');

describe.skipIf(!hasWasm)('spa app end to end (real wasm backend)', () => {
  const wait = () => new Promise((r) => setTimeout(r, 15));
  const click = (w: any, el: Element) =>
    el.dispatchEvent(new w.MouseEvent('click', { bubbles: true, cancelable: true }));

  function boot(url: string, seed?: Record<string, string>) {
    const dom = new JSDOM(previewHtml(), { runScripts: 'outside-only', url });
    const w = dom.window as any;
    if (seed) for (const [k, v] of Object.entries(seed)) w.sessionStorage.setItem(k, v);
    for (const sc of Array.from(w.document.querySelectorAll('script'))) {
      try {
        w.eval(sc.textContent || '');
      } catch {
        /* preview interceptor is idempotent */
      }
    }
    return dom;
  }

  let previewCache: string | null = null;
  function previewHtml(): string {
    return previewCache || '';
  }

  it('routes: mount, navigation, param(), 404, back/forward, deep link, hash mode', async () => {
    await backend.loadBytes(readFileSync(join(here, '../../public/kinn.wasm')));
    const result = await backend.buildWeb('spa_app.kin', [
      { name: 'spa_app.kin', content: SPA_SRC },
    ]);
    expect(result.ok).toBe(true);

    // The SPA dist: ONE document.
    const paths = result.files.map((f) => f.path).sort();
    expect(paths).toEqual(['app.js', 'index.html', 'kinnrt.css', 'kinnrt.js', 'styles.css']);

    previewCache = buildPreviewHtml(result.files, 'index.html');
    const page = () => (w: any) => w.document.querySelector('[data-kpage]').getAttribute('data-kpage');

    // -- mount at the document root (history mode). --
    const dom = boot('https://kinn.local/index.html');
    const w = dom.window as any;
    await wait();
    expect(page()(w)).toBe('Home');
    expect(w.document.querySelector('[data-kpage]').textContent).toContain('Open: 0');

    // -- link navigation through the router (no reload). --
    const settingsLink = Array.from(w.document.querySelectorAll('a')).find(
      (a: HTMLElement) => a.textContent === 'Settings',
    )!;
    click(w, settingsLink);
    await wait();
    expect(w.location.pathname).toBe('/settings');
    expect(page()(w)).toBe('Settings');
    expect(w.document.querySelector('[data-kpage]').textContent).toContain('Visits so far: 0');
    expect(settingsLink.getAttribute('aria-current')).toBe('page');
    expect(w.document.title).toBe('Settings · Guide');

    // -- back/forward (popstate). --
    w.history.back();
    await new Promise((r) => setTimeout(r, 30));
    expect(page()(w)).toBe('Home');
    w.history.forward();
    await new Promise((r) => setTimeout(r, 30));
    expect(page()(w)).toBe('Settings');
    expect(w.location.pathname).toBe('/settings');

    // -- app state survives the swaps; a handler writes before the nav. --
    w.history.back();
    await new Promise((r) => setTimeout(r, 30));
    const countBtn = w.document.querySelector('button');
    click(w, countBtn);
    await wait();
    expect(w.document.querySelector('[data-kpage]').textContent).toContain('Open: 1');
    const itemLink = Array.from(w.document.querySelectorAll('a')).find(
      (a: HTMLElement) => a.textContent === 'Item alpha',
    )!;
    click(w, itemLink);
    await wait();
    expect(page()(w)).toBe('ItemDetail');
    expect(w.location.pathname).toBe('/item/');
    // The link carries no param value: honest empty string (S9's
    // navigate() brings real values).
    expect(w.document.querySelector('[data-kpage]').textContent).toContain('Item: ');

    // -- deep link: URL → correct page on load, param() reads it. --
    const dom2 = boot('https://kinn.local/item/42');
    const w2 = dom2.window as any;
    await wait();
    expect(page()(w2)).toBe('ItemDetail');
    expect(w2.document.querySelector('[data-kpage]').textContent).toContain('Item: 42');

    // -- 404: unknown routes render the notFound page. --
    const dom3 = boot('https://kinn.local/nope/nothing');
    const w3 = dom3.window as any;
    await wait();
    expect(page()(w3)).toBe('Missing');
    expect(w3.document.querySelector('[data-kpage]').textContent).toContain('does not exist');

    // -- sessionStorage: the route is saved, and restores a reload. --
    const saved = w.sessionStorage.getItem('kinn:r:Guide');
    expect(saved).toBe('item/');
    const dom4 = boot('https://kinn.local/index.html', { 'kinn:r:Guide': 'settings' });
    const w4 = dom4.window as any;
    await wait();
    expect(page()(w4)).toBe('Settings');
    expect(w4.location.pathname).toBe('/settings');

    // -- hash fallback (file://-style origins route through the hash). --
    const dom5 = boot('about:blank');
    const w5 = dom5.window as any;
    await wait();
    expect(page()(w5)).toBe('Home');
    const settingsLink5 = Array.from(w5.document.querySelectorAll('a')).find(
      (a: HTMLElement) => a.textContent === 'Settings',
    )!;
    click(w5, settingsLink5);
    await wait();
    expect(w5.location.hash).toBe('#/settings');
    expect(page()(w5)).toBe('Settings');

    const dom6 = boot('about:blank#/item/7');
    const w6 = dom6.window as any;
    await wait();
    expect(page()(w6)).toBe('ItemDetail');
    expect(w6.document.querySelector('[data-kpage]').textContent).toContain('Item: 7');
  });

  it('declared animations generate CSS the preview inlines (S8)', async () => {
    await backend.loadBytes(readFileSync(join(here, '../../public/kinn.wasm')));
    const src = readFileSync(join(here, '../../../examples/web/anim_app.kin'), 'utf8');
    const result = await backend.buildWeb('anim_app.kin', [
      { name: 'anim_app.kin', content: src },
    ]);
    expect(result.ok).toBe(true);
    const css = result.files.find((f) => f.path === 'kinnrt.css')?.content ?? '';
    // The declared wobble keyframe with its 3D transform chain.
    expect(css).toContain('@keyframes kinn-wobble-in');
    expect(css).toContain('transform: perspective(800px) translateZ(6px) rotateX(8deg) rotateY(6deg);');
    expect(css).toContain('.kinn-enter-wobble { animation: kinn-wobble-in 500ms ease-in-out both; }');
    // The declared pop emits its own (identically-timed) rules after
    // the predefined sheet — cascade order makes the declaration win.
    const popRules = css.match(
      /\.kinn-enter-pop \{ animation: kinn-pop-in 300ms ease-out both; \}/g,
    ) ?? [];
    expect(popRules.length).toBeGreaterThanOrEqual(2);
    // The preview inlines the stylesheet (buildPreviewHtml).
    previewCache = buildPreviewHtml(result.files, 'index.html');
    expect(previewCache).toContain('@keyframes kinn-wobble-in');
  });
});
