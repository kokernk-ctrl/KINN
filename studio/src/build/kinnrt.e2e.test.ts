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
