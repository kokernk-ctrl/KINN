/**
 * The code editor: a transparent textarea over a syntax-highlighted
 * <pre>, with a line-number gutter, tab/auto-indent handling and a
 * Ln/Col status bar. All metrics (font, line-height, padding) are shared
 * via CSS so the overlay stays pixel-aligned.
 */

import { lex } from '../syntax/lexer';
import { renderLine, segmentsForLines } from '../syntax/highlight';

export interface EditorHooks {
  onEdit: (text: string) => void;
}

export class Editor {
  private input: HTMLTextAreaElement;
  private highlight: HTMLElement;
  private gutter: HTMLElement;
  private statusPos: HTMLElement;
  private statusCount: HTMLElement;
  private highlightTimer: number | undefined;

  constructor(
    input: HTMLTextAreaElement,
    highlight: HTMLElement,
    gutter: HTMLElement,
    statusPos: HTMLElement,
    statusCount: HTMLElement,
    private hooks: EditorHooks,
  ) {
    this.input = input;
    this.highlight = highlight;
    this.gutter = gutter;
    this.statusPos = statusPos;
    this.statusCount = statusCount;

    input.addEventListener('input', () => {
      this.rehighlight(true);
      this.hooks.onEdit(input.value);
    });
    input.addEventListener('scroll', () => this.syncScroll());
    input.addEventListener('keyup', () => this.updateStatus());
    input.addEventListener('click', () => this.updateStatus());
    input.addEventListener('keydown', (e) => this.onKeyDown(e));
  }

  setText(text: string): void {
    this.input.value = text;
    this.input.scrollTop = 0;
    this.input.scrollLeft = 0;
    this.rehighlight(false);
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
    // bring the line into view
    const lineHeight = 13 * 1.5;
    this.input.scrollTop = Math.max(0, (clamped - 4) * lineHeight);
    this.syncScroll();
    this.updateStatus();
  }

  private rehighlight(debounced: boolean): void {
    const update = () => this.render();
    if (debounced) {
      window.clearTimeout(this.highlightTimer);
      this.highlightTimer = window.setTimeout(update, 30);
    } else {
      window.clearTimeout(this.highlightTimer);
      update();
    }
  }

  private render(): void {
    const text = this.input.value;
    const { tokens } = lex(text);
    const segs = segmentsForLines(text, tokens);
    const lines = text.split('\n');
    const html = lines.map((line, i) => renderLine(line, segs[i] ?? [])).join('\n');
    // trailing newline keeps the last line visible in the <pre>
    this.highlight.innerHTML = html + (text.endsWith('\n') || text === '' ? '\n' : '');
    this.gutter.textContent = lines.map((_, i) => String(i + 1)).join('\n') + '\n';
    this.syncScroll();
    this.statusCount.textContent = `${lines.length} line${lines.length === 1 ? '' : 's'}`;
  }

  private syncScroll(): void {
    this.highlight.scrollTop = this.input.scrollTop;
    this.highlight.scrollLeft = this.input.scrollLeft;
    this.gutter.style.transform = `translateY(${-this.input.scrollTop}px)`;
  }

  private updateStatus(): void {
    const pos = this.input.selectionStart;
    const before = this.input.value.slice(0, pos);
    const line = before.split('\n').length;
    const col = pos - (before.lastIndexOf('\n') + 1) + 1;
    this.statusPos.textContent = `Ln ${line}, Col ${col}`;
  }

  private onKeyDown(e: KeyboardEvent): void {
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
    this.rehighlight(true);
    this.hooks.onEdit(this.input.value);
    this.updateStatus();
  }
}
