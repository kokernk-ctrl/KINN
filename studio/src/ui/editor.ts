/**
 * The code editor (classic Studio) — single-scroll architecture
 * (0.4.1 fix, same as NEXT): a transparent textarea that covers
 * EXACTLY the syntax-highlighted <pre> (both live inside
 * .editor-sizer, which is sized by the in-flow <pre>), so the overlay
 * and the input share one scroller (.editor-scroll) by construction.
 *
 *  - the <pre> and the textarea can never drift apart (no scroll
 *    copying, no event lag) — taps land on the character the user sees;
 *  - the textarea has no scrollbar of its own: one horizontal and one
 *    vertical scrollbar total, at the editor edges;
 *  - the gutter is driven by the same single scrollTop, with identical
 *    metrics, so line numbers match the real lines at any offset;
 *  - `render()` only rebuilds the overlay/gutter — the textarea value,
 *    selection and IME composition are never touched;
 *  - the input layer never scrolls: caret visibility is kept by
 *    scrolling the one scroller (BringIntoView equivalent).
 */

import { lex } from '../syntax/lexer';
import { renderLine, segmentsForLines } from '../syntax/highlight';

export interface EditorHooks {
  onEdit: (text: string) => void;
}

const LINE_HEIGHT = 13 * 1.5; // must match theme.css line-height 1.5 × 13px

export class Editor {
  private input: HTMLTextAreaElement;
  private scroll: HTMLElement;
  private highlight: HTMLElement;
  private gutter: HTMLElement;
  private statusPos: HTMLElement;
  private statusCount: HTMLElement;

  constructor(
    input: HTMLTextAreaElement,
    scroll: HTMLElement,
    highlight: HTMLElement,
    gutter: HTMLElement,
    statusPos: HTMLElement,
    statusCount: HTMLElement,
    private hooks: EditorHooks,
  ) {
    this.input = input;
    this.scroll = scroll;
    this.highlight = highlight;
    this.gutter = gutter;
    this.statusPos = statusPos;
    this.statusCount = statusCount;

    input.addEventListener('input', () => {
      // synchronous mirror: the sizer (and therefore the textarea box)
      // is rebuilt in the same task as the text change — no window in
      // which the textarea could grow its own transient scrollbar.
      this.render();
      this.hooks.onEdit(input.value);
    });
    // THE one scroll drives the gutter.
    scroll.addEventListener('scroll', () => this.syncScroll());
    // selectionchange is the standard event for caret/selection moves
    // (touch handles, IME); keyup/click stay as fallbacks.
    document.addEventListener('selectionchange', () => {
      if (document.activeElement === input) this.updateStatus();
    });
    input.addEventListener('keyup', () => this.updateStatus());
    input.addEventListener('click', () => this.updateStatus());
    input.addEventListener('keydown', (e) => this.onKeyDown(e));
    window.addEventListener('resize', () => this.syncScroll());
  }

  setText(text: string): void {
    this.input.value = text;
    this.scroll.scrollTop = 0;
    this.scroll.scrollLeft = 0;
    this.render();
    this.updateStatus();
  }

  getText(): string {
    return this.input.value;
  }

  /** Move the caret to the start of a 1-based line and focus. */
  gotoLine(line: number): void {
    const lines = this.input.value.split('\n');
    const clamped = Math.max(1, Math.min(line, lines.length));
    let pos = 0;
    for (let i = 0; i < clamped - 1; i++) pos += lines[i].length + 1;
    this.input.focus();
    this.input.setSelectionRange(pos, pos);
    // bring the line into view (single scroller)
    this.scroll.scrollTop = Math.max(0, (clamped - 4) * LINE_HEIGHT);
    this.syncScroll();
    this.updateStatus();
  }

  private render(): void {
    const text = this.input.value;
    const { tokens } = lex(text);
    const segs = segmentsForLines(text, tokens);
    const lines = text.split('\n');
    const html = lines.map((line, i) => renderLine(line, segs[i] ?? [])).join('\n');
    // trailing newline keeps the last line visible in the <pre>
    this.highlight.innerHTML = html + (text.endsWith('\n') || text === '' ? '\n' : '');
    // gutter generated from the SAME line split the overlay rendered
    this.gutter.textContent = lines.map((_, i) => String(i + 1)).join('\n') + '\n';
    this.syncScroll();
    this.statusCount.textContent = `${lines.length} line${lines.length === 1 ? '' : 's'}`;
  }

  /** Vertical sync of the gutter with the single scroll state. */
  private syncScroll(): void {
    this.gutter.style.transform = `translateY(${-this.scroll.scrollTop}px)`;
  }

  private updateStatus(): void {
    const pos = this.input.selectionStart;
    const before = this.input.value.slice(0, pos);
    const line = before.split('\n').length;
    const col = pos - (before.lastIndexOf('\n') + 1) + 1;
    this.statusPos.textContent = `Ln ${line}, Col ${col}`;
    this.ensureCaretVisible(line);
  }

  /** Keep the caret line inside the single scroller's viewport. */
  private ensureCaretVisible(line: number): void {
    const viewH = this.scroll.clientHeight;
    if (viewH <= 0) return; // not laid out / hidden — nothing to scroll
    const top = 10 + (line - 1) * LINE_HEIGHT; // 10px = editor top padding
    const viewTop = this.scroll.scrollTop;
    if (top < viewTop) {
      this.scroll.scrollTop = top;
    } else if (top + LINE_HEIGHT > viewTop + viewH) {
      this.scroll.scrollTop = top + LINE_HEIGHT - viewH;
    }
    this.syncScroll();
  }

  private onKeyDown(e: KeyboardEvent): void {
    // Never intercept keys while the IME is composing (mobile keyboards).
    if (e.isComposing) return;
    if (e.key === 'Tab') {
      e.preventDefault();
      this.insertAtCaret('  ');
    } else if (e.key === 'Enter') {
      // auto-indent: keep leading whitespace, add one level after '{'
      e.preventDefault();
      const pos = this.input.selectionStart;
      const value = this.input.value;
      const lineStart = value.lastIndexOf('\n', pos - 1) + 1;
      const currentLine = value.slice(lineStart, pos);
      const indentMatch = /^[ \t]*/.exec(currentLine);
      let indent = indentMatch ? indentMatch[0] : '';
      if (currentLine.trimEnd().endsWith('{')) indent += '  ';
      this.insertAtCaret('\n' + indent);
    }
  }

  private insertAtCaret(text: string): void {
    const start = this.input.selectionStart;
    const end = this.input.selectionEnd;
    this.input.setRangeText(text, start, end, 'end');
    this.render();
    this.hooks.onEdit(this.input.value);
    this.updateStatus();
  }
}
