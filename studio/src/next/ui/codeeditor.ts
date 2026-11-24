/**
 * Studio Next code editor — single-scroll architecture (0.4.1 fix).
 *
 * DOM shape (see next.html / next.css):
 *
 *   .nx-editor-wrap                flex row, overflow hidden
 *   ├─ .nx-gutter                  line numbers, never scrolls itself,
 *   │                              translated by the one vertical scroll
 *   └─ .nx-editor-scroll           THE ONLY scrollable element (x + y)
 *      └─ .nx-editor-sizer         position:relative; sized by the <pre>
 *         ├─ .nx-active-line       absolute, moves with the content
 *         ├─ .nx-highlight (pre)   in-flow: it DEFINES the sizer size
 *         └─ .nx-editor-input      absolute inset:0 — exactly the size of
 *                                  the <pre>, so it never scrolls inside
 *
 * Why this fixes the 0.4.0 symptoms:
 *
 *  - The <pre> overlay and the transparent textarea share one containing
 *    block and one scroller, so the visible (highlighted) text and the
 *    real text are aligned by construction — no scroll copying, no lag,
 *    no drift. A tap lands on the character the user sees (cursor fix).
 *  - The textarea's box is exactly as large as its content, so it never
 *    has its own scrollbar: exactly one horizontal and one vertical
 *    scrollbar, at the editor edges (no bars cutting through the code).
 *  - The gutter is a pure visual mirror driven by the same single
 *    ScrollState (scrollTop), generated from the exact same line split
 *    the overlay rendered, with identical metrics (font, line-height,
 *    padding) — numbers match the real lines at any scroll offset.
 *  - `render()` only touches the overlay (innerHTML) and the gutter
 *    (textContent); the textarea's value/selection are never rebuilt,
 *    so IME composition survives every keystroke (backspace fix).
 *  - Cursor tracking listens to `selectionchange` (the standard event
 *    for caret/selection moves, including IME and touch handles).
 *  - `ensureCaretVisible()` is the web equivalent of a
 *    BringIntoViewRequester: it scrolls the one scroller (never a
 *    phantom inner one) to keep the caret on screen.
 *
 * The source of truth for the text is ALWAYS `textarea.value`
 * (getText()); the overlay and the gutter are derived views. Callers
 * that compile (run/check/build) read getText()/the vfs snapshot that
 * is saved synchronously on every input event — and main.ts additionally
 * flushes the editor into the vfs right before compiling.
 */

import { lex } from '../../syntax/lexer';
import { renderLine, segmentsForLines } from '../../syntax/highlight';
import { editorLineHeight } from '../settings';

export interface CodeEditorCallbacks {
  onEdit(text: string): void;
  onCursor(line: number, col: number, lines: number): void;
}

export interface CodeEditorElements {
  wrap: HTMLElement;
  scroll: HTMLElement;
  sizer: HTMLElement;
  highlight: HTMLElement;
  gutter: HTMLElement;
  activeLine: HTMLElement;
  input: HTMLTextAreaElement;
}

export class CodeEditor {
  private findMatches: number[] = []; // absolute offsets
  private findIdx = -1;
  private findQuery = '';
  private lh = 20; // line height px, kept in sync with CSS vars
  private pad = 10; // vertical padding px, kept in sync with CSS vars

  constructor(
    private els: CodeEditorElements,
    private cb: CodeEditorCallbacks,
  ) {
    const { input, scroll } = els;

    // text changes: mirror into the overlay + gutter, notify host.
    input.addEventListener('input', () => {
      this.render();
      this.cb.onEdit(input.value);
    });

    // THE one scroll: drive the gutter (vertical) from it.
    scroll.addEventListener('scroll', () => this.syncGutter());

    // cursor/selection tracking: selectionchange is the standard event
    // and fires for touch drags, IME composition moves and taps.
    document.addEventListener('selectionchange', () => {
      if (document.activeElement === input) this.updateCursor();
    });
    // fallbacks for engines without textarea selectionchange
    for (const ev of ['keyup', 'click', 'select', 'touchend'] as const) {
      input.addEventListener(ev, () => this.updateCursor());
    }
    input.addEventListener('keydown', (e) => this.onKeyDown(e));
    window.addEventListener('resize', () => this.syncGutter());
  }

  setText(text: string): void {
    this.els.input.value = text;
    this.els.scroll.scrollTop = 0;
    this.els.scroll.scrollLeft = 0;
    this.render();
    this.updateCursor();
  }

  getText(): string {
    return this.els.input.value;
  }

  focus(): void {
    this.els.input.focus();
  }

  /** Apply font size; keeps overlay metrics pixel-aligned. */
  setFontSize(px: number): void {
    this.lh = editorLineHeight(px);
    this.els.wrap.style.setProperty('--nx-ed-font', `${px}px`);
    this.els.wrap.style.setProperty('--nx-ed-lh', `${this.lh}px`);
    this.render();
    this.updateCursor();
  }

  setIndentSize(n: number): void {
    this.indent = n;
  }
  private indent = 2;

  /** Move the caret to the start of a 1-based line, focus, scroll it into view. */
  gotoLine(line: number): void {
    const lines = this.els.input.value.split('\n');
    const clamped = Math.max(1, Math.min(line, lines.length));
    let pos = 0;
    for (let i = 0; i < clamped - 1; i++) pos += lines[i].length + 1;
    this.els.input.focus();
    this.els.input.setSelectionRange(pos, pos);
    this.els.scroll.scrollTop = Math.max(0, (clamped - 4) * this.lh);
    this.syncGutter();
    this.updateCursor();
  }

  // -- rendering ----------------------------------------------------------------

  /**
   * Rebuild the visual mirror (overlay + gutter) from the current
   * textarea value. Never touches the textarea itself: the IME
   * composition, selection and value stay untouched.
   */
  private render(): void {
    const { input, highlight, gutter } = this.els;
    const text = input.value;
    const { tokens } = lex(text);
    const segs = segmentsForLines(text, tokens);
    const lines = text.split('\n');
    highlight.innerHTML = lines.map((line, i) => renderLine(line, segs[i] ?? [])).join('\n') +
      (text.endsWith('\n') || text === '' ? '\n' : '');
    // The gutter is generated from the SAME line split the overlay just
    // rendered — they cannot disagree about the line count.
    gutter.textContent = lines.map((_, i) => String(i + 1)).join('\n') + '\n';
    this.syncGutter();
  }

  /** Vertical sync of the gutter with the single scroll state. */
  private syncGutter(): void {
    const { scroll, gutter } = this.els;
    gutter.style.transform = `translateY(${-scroll.scrollTop}px)`;
  }

  private updateCursor(): void {
    const { input, activeLine } = this.els;
    const pos = input.selectionStart;
    const before = input.value.slice(0, pos);
    const line = before.split('\n').length;
    const col = pos - (before.lastIndexOf('\n') + 1) + 1;
    // current-line backdrop: absolute inside the sizer, offset by the
    // same top padding the text has, one line-height per logical line.
    activeLine.style.transform = `translateY(${(line - 1) * this.lh}px)`;
    this.ensureCaretVisible(line);
    this.cb.onCursor(line, col, input.value.split('\n').length);
  }

  /**
   * Keep the caret line inside the viewport of the single scroller
   * (web equivalent of BringIntoViewRequester). Only acts when the line
   * is fully out of view, so it never fights the browser's own
   * caret-following scroll.
   */
  private ensureCaretVisible(line: number): void {
    const { scroll } = this.els;
    // content-space y of the caret line (top padding + line height * index)
    const top = this.pad + (line - 1) * this.lh;
    const viewTop = scroll.scrollTop;
    const viewH = scroll.clientHeight;
    if (viewH <= 0) return; // not laid out / hidden — nothing to scroll
    if (top < viewTop) {
      scroll.scrollTop = top;
    } else if (top + this.lh > viewTop + viewH) {
      scroll.scrollTop = top + this.lh - viewH;
    }
    this.syncGutter();
  }

  private onKeyDown(e: KeyboardEvent): void {
    // Never intercept keys while the IME is composing (mobile keyboards):
    // backspace, delete, enter and friends must reach the textarea
    // untouched so composition state stays consistent.
    if (e.isComposing) return;
    const { input } = this.els;
    if (e.key === 'Tab') {
      e.preventDefault();
      this.insertAtCaret(' '.repeat(this.indent));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const pos = input.selectionStart;
      const value = input.value;
      const lineStart = value.lastIndexOf('\n', pos - 1) + 1;
      const currentLine = value.slice(lineStart, pos);
      const indentMatch = /^[ \t]*/.exec(currentLine);
      let indent = indentMatch ? indentMatch[0] : '';
      if (currentLine.trimEnd().endsWith('{')) indent += ' '.repeat(this.indent);
      this.insertAtCaret('\n' + indent);
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      this.openFind();
    } else if (e.key === 'Escape' && !this.findBarHidden()) {
      this.closeFind();
    }
  }

  private insertAtCaret(text: string): void {
    const { input } = this.els;
    const start = input.selectionStart;
    const end = input.selectionEnd;
    input.setRangeText(text, start, end, 'end');
    this.render();
    this.cb.onEdit(input.value);
    this.updateCursor();
  }

  // -- find -----------------------------------------------------------------------

  private findBar(): HTMLElement | null {
    return document.getElementById('nx-findbar');
  }
  private findBarHidden(): boolean {
    return this.findBar()?.hidden ?? true;
  }

  openFind(): void {
    const bar = this.findBar();
    if (!bar) return;
    bar.hidden = false;
    const field = bar.querySelector('input') as HTMLInputElement;
    field.focus();
    field.select();
    this.updateFind();
  }

  closeFind(): void {
    const bar = this.findBar();
    if (!bar) return;
    bar.hidden = true;
    this.els.input.focus();
  }

  findNext(backward = false): void {
    if (this.findMatches.length === 0) return;
    this.findIdx = (this.findIdx + (backward ? -1 : 1) + this.findMatches.length) % this.findMatches.length;
    this.jumpToMatch();
  }

  /** Recompute matches and update the counter; called by the find bar. */
  updateFind(): void {
    const bar = this.findBar();
    if (!bar || bar.hidden) return;
    const field = bar.querySelector('input') as HTMLInputElement;
    this.findQuery = field.value;
    this.findMatches = [];
    this.findIdx = -1;
    if (this.findQuery.length > 0) {
      const haystack = this.els.input.value;
      const needle = this.findQuery;
      let idx = haystack.indexOf(needle);
      while (idx !== -1) {
        this.findMatches.push(idx);
        idx = haystack.indexOf(needle, idx + needle.length);
      }
    }
    const counter = bar.querySelector('.nx-find-count') as HTMLElement;
    if (this.findMatches.length === 0) {
      counter.textContent = this.findQuery ? 'no matches' : '';
    } else {
      this.findIdx = 0;
      counter.textContent = `1 / ${this.findMatches.length}`;
      this.jumpToMatch();
    }
  }

  private jumpToMatch(): void {
    const bar = this.findBar();
    if (!bar) return;
    const counter = bar.querySelector('.nx-find-count') as HTMLElement;
    const { input } = this.els;
    const pos = this.findMatches[this.findIdx];
    if (pos === undefined) return;
    input.focus();
    input.setSelectionRange(pos, pos + this.findQuery.length);
    const line = input.value.slice(0, pos).split('\n').length;
    this.els.scroll.scrollTop = Math.max(0, (line - 4) * this.lh);
    this.syncGutter();
    this.updateCursor();
    counter.textContent = `${this.findIdx + 1} / ${this.findMatches.length}`;
  }
}
