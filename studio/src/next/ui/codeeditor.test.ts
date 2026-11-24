// @vitest-environment jsdom
/**
 * Unit tests for the Studio NEXT code editor (single-scroll
 * architecture, 0.4.1 fix).
 *
 * jsdom does not do real layout, so these cover the LOGIC invariants:
 *  - the overlay is a pure mirror of the textarea value (same offsets,
 *    same line split — the equivalent of VisualTransformation with
 *    OffsetMapping.identity);
 *  - the gutter is generated from the SAME line split as the overlay
 *    (line count cannot disagree);
 *  - render() never touches the textarea value/selection (IME-safe);
 *  - keydown handling ignores composing events (backspace/IME passthrough);
 *  - the editor state (getText) is always the live textarea value —
 *    the single source of truth for compilation.
 *
 * Geometric alignment (overlay == textarea box, gutter == line
 * positions) is verified in the browser E2E run on the built bundle.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { CodeEditor, type CodeEditorElements } from './codeeditor';

interface Harness {
  editor: CodeEditor;
  els: CodeEditorElements;
  edits: string[];
  cursors: Array<[number, number, number]>;
}

function mount(): Harness {
  const wrap = document.createElement('div');
  const gutter = document.createElement('div');
  const scroll = document.createElement('div');
  const sizer = document.createElement('div');
  const highlight = document.createElement('pre');
  const activeLine = document.createElement('div');
  const input = document.createElement('textarea');
  input.setAttribute('wrap', 'off');

  wrap.appendChild(gutter);
  wrap.appendChild(scroll);
  scroll.appendChild(sizer);
  sizer.appendChild(activeLine);
  sizer.appendChild(highlight);
  sizer.appendChild(input);
  document.body.appendChild(wrap);

  const edits: string[] = [];
  const cursors: Array<[number, number, number]> = [];
  const els: CodeEditorElements = {
    wrap, scroll, sizer, highlight, gutter, activeLine,
    input: input as HTMLTextAreaElement,
  };
  const editor = new CodeEditor(els, {
    onEdit: (text) => edits.push(text),
    onCursor: (line, col, lines) => cursors.push([line, col, lines]),
  });
  return { editor, els, edits, cursors };
}

/** Simulate an IME/browser text edit on the textarea. */
function type(input: HTMLTextAreaElement, value: string, caret?: number): void {
  input.value = value;
  if (caret !== undefined) input.setSelectionRange(caret, caret);
  input.dispatchEvent(new Event('input', { bubbles: true }));
}

describe('CodeEditor (NEXT) — single source of truth', () => {
  let h: Harness;
  beforeEach(() => {
    document.body.innerHTML = '';
    h = mount();
  });

  it('overlay mirrors the exact text (same offsets — identity mapping)', () => {
    const code = 'func main() {\n  var x = 10\n  // comment\n}\n';
    h.editor.setText(code);
    // textContent keeps the trailing phantom line; strip it exactly the
    // way render() adds it
    const expected = code + (code.endsWith('\n') ? '\n' : '');
    expect(h.els.highlight.textContent).toBe(expected);
    expect(h.editor.getText()).toBe(code);
  });

  it('gutter is generated from the same line split as the overlay', () => {
    const code = 'a\nbb\nccc\ndd';
    h.editor.setText(code);
    const overlayLines = (h.els.highlight.textContent ?? '').split('\n');
    const gutterLines = (h.els.gutter.textContent ?? '').split('\n');
    // gutter has numbers + trailing empty from the final '\n'
    const numbers = gutterLines.filter((l) => l.length > 0);
    expect(numbers).toEqual(['1', '2', '3', '4']);
    // overlay lines (may have a trailing empty entry): real lines match
    expect(overlayLines.slice(0, 4)).toEqual(['a', 'bb', 'ccc', 'dd']);
  });

  it('typing updates the mirror and notifies with the live value', () => {
    h.editor.setText('let');
    type(h.els.input, 'let x = 1');
    expect(h.editor.getText()).toBe('let x = 1');
    expect(h.edits.at(-1)).toBe('let x = 1');
    expect(h.els.highlight.textContent).toContain('let x = 1');
  });

  it('render never rebuilds the textarea value or selection (IME-safe)', () => {
    h.editor.setText('hello world');
    h.els.input.setSelectionRange(5, 5);
    const before = h.els.input.value;
    type(h.els.input, 'hello world!', 12);
    expect(h.els.input.value).not.toBe(before);
    // selection preserved by the edit itself — render must not touch it
    expect(h.els.input.selectionStart).toBe(12);
    expect(h.els.input.selectionEnd).toBe(12);
  });

  it('backspace-sized edits keep overlay == value at all times', () => {
    const code = 'func main() {\n  var x = 10\n}\n';
    h.editor.setText(code);
    // delete from the middle, start and end of a line
    const edits: Array<[string, number, number]> = [
      ['fn main() {\n  var x = 10\n}\n', 1, 4],   // middle of line 1
      ['func main() {\n  var x = 0\n}\n', 20, 20], // end of a line
      ['func main() {\n var x = 10\n}\n', 18, 19], // start of line 2
    ];
    for (const [next, s, e] of edits) {
      type(h.els.input, next, s);
      expect(h.editor.getText()).toBe(next);
      expect(h.els.highlight.textContent).toBe(next + '\n');
      expect(h.els.input.selectionStart).toBeLessThanOrEqual(e + 1);
    }
  });

  it('keydown is a no-op while the IME is composing', () => {
    h.editor.setText('');
    const ev = new KeyboardEvent('keydown', {
      key: 'Enter', bubbles: true, cancelable: true,
    });
    Object.defineProperty(ev, 'isComposing', { value: true });
    h.els.input.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(false);
    expect(h.editor.getText()).toBe('');
  });

  it('Enter (not composing) auto-indents and grows the line count', () => {
    h.editor.setText('func main() {');
    h.els.input.setSelectionRange(14, 14);
    const ev = new KeyboardEvent('keydown', {
      key: 'Enter', bubbles: true, cancelable: true,
    });
    h.els.input.dispatchEvent(ev);
    expect(h.editor.getText()).toBe('func main() {\n  ');
    expect((h.els.gutter.textContent ?? '').split('\n').filter(Boolean)).toEqual(['1', '2']);
  });

  it('Tab inserts the configured indent without replacing the text', () => {
    h.editor.setText('a');
    h.els.input.setSelectionRange(1, 1);
    h.editor.setIndentSize(4);
    const ev = new KeyboardEvent('keydown', {
      key: 'Tab', bubbles: true, cancelable: true,
    });
    h.els.input.dispatchEvent(ev);
    expect(h.editor.getText()).toBe('a    ');
  });

  it('gotoLine clamps, places the caret and reports the cursor', () => {
    const code = 'one\ntwo\nthree\nfour\nfive';
    h.editor.setText(code);
    h.editor.gotoLine(99);
    // clamped to the last line — caret at its START (offset 19)
    expect(h.els.input.selectionStart).toBe(19);
    h.editor.gotoLine(2);
    expect(h.els.input.selectionStart).toBe(4);
    expect(h.cursors.at(-1)).toEqual([2, 1, 5]);
  });

  it('selectionchange (standard event) updates the cursor report', () => {
    h.editor.setText('abc\ndef');
    h.els.input.focus();
    h.els.input.setSelectionRange(5, 5); // 'd' — line 2, col 2
    document.dispatchEvent(new Event('selectionchange'));
    expect(h.cursors.at(-1)).toEqual([2, 2, 2]);
  });

  it('active-line highlight is positioned per line, offset by the top padding', () => {
    h.editor.setText('a\nb\nc');
    h.els.input.setSelectionRange(4, 4); // line 3
    document.dispatchEvent(new Event('selectionchange'));
    expect(h.els.activeLine.style.transform).toBe('translateY(40px)'); // (3-1)*20
  });

  it('gutter follows the single scroll state (transform)', () => {
    h.editor.setText('x\ny');
    h.els.scroll.dispatchEvent(new Event('scroll'));
    Object.defineProperty(h.els.scroll, 'scrollTop', { value: 42, writable: true });
    h.els.scroll.dispatchEvent(new Event('scroll'));
    expect(h.els.gutter.style.transform).toBe('translateY(-42px)');
  });

  it('setText resets the single scroller (both axes)', () => {
    h.editor.setText('a\nb');
    Object.defineProperty(h.els.scroll, 'scrollTop', { value: 30, writable: true });
    h.editor.setText('c');
    expect(h.els.scroll.scrollTop).toBe(0);
    expect(h.els.scroll.scrollLeft).toBe(0);
  });

  it('setFontSize keeps overlay metrics variables in sync', () => {
    h.editor.setFontSize(18);
    expect(h.els.wrap.style.getPropertyValue('--nx-ed-font')).toBe('18px');
    // editorLineHeight = round(px * 1.55)
    expect(h.els.wrap.style.getPropertyValue('--nx-ed-lh')).toBe('28px');
    expect(h.editor.getText).toBeInstanceOf(Function);
  });

  it('compilation path invariant: getText() always returns the live value', () => {
    h.editor.setText('fn v1');
    const snapshot1 = h.editor.getText();
    type(h.els.input, 'fn v2 (edited)');
    const snapshot2 = h.editor.getText();
    expect(snapshot1).toBe('fn v1');
    expect(snapshot2).toBe('fn v2 (edited)');
    expect(snapshot2).toBe(h.els.input.value);
  });
});
