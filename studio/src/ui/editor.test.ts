// @vitest-environment jsdom
/**
 * Unit tests for the classic Studio editor (single-scroll
 * architecture, 0.4.1 fix — same technique as NEXT).
 *
 * Logic invariants (geometric alignment is covered by the E2E run):
 *  - overlay is a mirror of the textarea value with identical offsets;
 *  - gutter generated from the same line split as the overlay;
 *  - IME composing key events are never intercepted;
 *  - getText() is the live value (single source of truth for the vfs
 *    and the compiler flush).
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { Editor } from './editor';

function mount(): { editor: Editor; input: HTMLTextAreaElement; highlight: HTMLElement; gutter: HTMLElement; scroll: HTMLElement } {
  const wrap = document.createElement('div');
  const gutter = document.createElement('div');
  const scroll = document.createElement('div');
  const sizer = document.createElement('div');
  const highlight = document.createElement('pre');
  const input = document.createElement('textarea');
  const statusPos = document.createElement('span');
  const statusCount = document.createElement('span');

  wrap.appendChild(gutter);
  wrap.appendChild(scroll);
  scroll.appendChild(sizer);
  sizer.appendChild(highlight);
  sizer.appendChild(input);
  document.body.appendChild(wrap);

  const editor = new Editor(
    input as HTMLTextAreaElement,
    scroll,
    highlight,
    gutter,
    statusPos,
    statusCount,
    { onEdit: () => {} },
  );
  return { editor, input: input as HTMLTextAreaElement, highlight, gutter, scroll };
}

function type(input: HTMLTextAreaElement, value: string, caret?: number): void {
  input.value = value;
  if (caret !== undefined) input.setSelectionRange(caret, caret);
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

describe('Editor (classic) — single source of truth', () => {
  let h: ReturnType<typeof mount>;
  beforeEach(() => {
    document.body.innerHTML = '';
    h = mount();
  });

  it('overlay mirrors the exact text (identity offsets)', () => {
    const code = 'func main() {\n  fin\n';
    h.editor.setText(code);
    expect(h.editor.getText()).toBe(code);
    expect(h.highlight.textContent).toBe(code + '\n');
  });

  it('gutter matches the overlay line split', () => {
    h.editor.setText('a\nbb\nccc');
    const numbers = (h.gutter.textContent ?? '').split('\n').filter(Boolean);
    expect(numbers).toEqual(['1', '2', '3']);
  });

  it('typing keeps the mirror up to date (debounced render flushed)', async () => {
    h.editor.setText('x');
    type(h.input, 'xyz');
    await new Promise((r) => setTimeout(r, 60)); // debounce 30ms
    expect(h.highlight.textContent).toContain('xyz');
    expect(h.editor.getText()).toBe('xyz');
  });

  it('keydown is a no-op while composing (IME passthrough)', () => {
    h.editor.setText('');
    const ev = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
    Object.defineProperty(ev, 'isComposing', { value: true });
    h.input.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
    expect(h.editor.getText()).toBe('');
  });

  it('gotoLine clamps and positions the caret', () => {
    h.editor.setText('one\ntwo\nthree');
    h.editor.gotoLine(2);
    expect(h.input.selectionStart).toBe(4);
    h.editor.gotoLine(99);
    expect(h.input.selectionStart).toBe(8); // start of the last line
  });

  it('gutter follows the single scroller only (no textarea scroll)', () => {
    Object.defineProperty(h.input, 'scrollTop', { value: 99, writable: true });
    h.input.dispatchEvent(new Event('scroll')); // must be ignored: no textarea scroll exists
    expect(h.gutter.style.transform).toBe('');
    Object.defineProperty(h.scroll, 'scrollTop', { value: 30, writable: true });
    h.scroll.dispatchEvent(new Event('scroll'));
    expect(h.gutter.style.transform).toBe('translateY(-30px)');
  });
});
