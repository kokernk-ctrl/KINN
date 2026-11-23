/**
 * Studio Next code editor: transparent textarea over a highlighted
 * <pre> overlay (same proven technique as the classic Studio), plus
 * current-line highlighting, a real find bar (match count, next/prev),
 * configurable indent, and a shared-metrics gutter. Syntax highlighting
 * uses the editor-side Kin lexer that mirrors the reference Rust lexer.
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

  constructor(
    private els: CodeEditorElements,
    private cb: CodeEditorCallbacks,
  ) {
    const { input } = els;
    input.addEventListener('input', () => {
      this.render();
      this.cb.onEdit(input.value);
    });
    input.addEventListener('scroll', () => this.syncScroll());
    for (const ev of ['keyup', 'click', 'select'] as const) {
      input.addEventListener(ev, () => this.updateCursor());
    }
    input.addEventListener('keydown', (e) => this.onKeyDown(e));
  }

  setText(text: string): void {
    this.els.input.value = text;
    this.els.input.scrollTop = 0;
    this.els.input.scrollLeft = 0;
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

  /** Move the caret to the start of a 1-based line and focus. */
  gotoLine(line: number): void {
    const lines = this.els.input.value.split('\n');
    const clamped = Math.max(1, Math.min(line, lines.length));
    let pos = 0;
    for (let i = 0; i < clamped - 1; i++) pos += lines[i].length + 1;
    this.els.input.focus();
    this.els.input.setSelectionRange(pos, pos);
    this.els.input.scrollTop = Math.max(0, (clamped - 4) * this.lh);
    this.syncScroll();
    this.updateCursor();
  }

  // -- rendering ----------------------------------------------------------------

  private render(): void {
    const { input, highlight, gutter } = this.els;
    const text = input.value;
    const { tokens } = lex(text);
    const segs = segmentsForLines(text, tokens);
    const lines = text.split('\n');
    highlight.innerHTML = lines.map((line, i) => renderLine(line, segs[i] ?? [])).join('\n') +
      (text.endsWith('\n') || text === '' ? '\n' : '');
    gutter.textContent = lines.map((_, i) => String(i + 1)).join('\n') + '\n';
    this.syncScroll();
  }

  private syncScroll(): void {
    const { input, highlight, gutter, activeLine } = this.els;
    highlight.scrollTop = input.scrollTop;
    highlight.scrollLeft = input.scrollLeft;
    gutter.style.transform = `translateY(${-input.scrollTop}px)`;
    // keep the active-line bar as wide as the scrolled content
    activeLine.style.width = `${Math.max(highlight.scrollWidth, highlight.clientWidth)}px`;
  }

  private updateCursor(): void {
    const { input, activeLine } = this.els;
    const pos = input.selectionStart;
    const before = input.value.slice(0, pos);
    const line = before.split('\n').length;
    const col = pos - (before.lastIndexOf('\n') + 1) + 1;
    // current-line backdrop (scrolls with content)
    activeLine.style.top = `${(line - 1) * this.lh}px`;
    this.cb.onCursor(line, col, input.value.split('\n').length);
  }

  private onKeyDown(e: KeyboardEvent): void {
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
    input.scrollTop = Math.max(0, (line - 4) * this.lh);
    this.syncScroll();
    this.updateCursor();
    counter.textContent = `${this.findIdx + 1} / ${this.findMatches.length}`;
  }
}
