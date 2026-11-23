/**
 * Syntax highlighting: token stream → per-line colored segments,
 * rendered by the editor as an overlay under a transparent textarea.
 */

import type { Token, TokKind } from './lexer';

/** A colored range within one line (0-based start/end in characters). */
export interface Seg {
  start: number;
  end: number;
  cls: string;
}

const CLASS_FOR: Partial<Record<TokKind, string>> = {
  func: 'kw', fin: 'kw', var: 'kw', if: 'kw', else: 'kw', while: 'kw',
  for: 'kw', in: 'kw', as: 'kw', import: 'kw',
  return: 'kw', break: 'kw', continue: 'kw', and: 'kw', or: 'kw', not: 'kw',
  'int-ty': 'ty', 'float-ty': 'ty', 'bool-ty': 'ty', 'string-ty': 'ty', 'void-ty': 'ty',
  int: 'num', float: 'num',
  string: 'str',
  true: 'bool', false: 'bool',
  comment: 'comment',
  ident: '',
};

const OP_KINDS: TokKind[] = ['+', '-', '*', '/', '%', '=', '==', '!=', '<', '>', '<=', '>=', '->', '+=', '-=', '.', '..', '..='];
const PUNCT_KINDS: TokKind[] = ['(', ')', '{', '}', '[', ']', ',', ':', ';'];

/** CSS class for a token kind ('' = default ink). */
export function classFor(kind: TokKind): string {
  if (CLASS_FOR[kind] !== undefined) return CLASS_FOR[kind];
  if (OP_KINDS.includes(kind)) return 'op';
  if (PUNCT_KINDS.includes(kind)) return 'punct';
  return '';
}

/**
 * Compute colored segments per line. Tokens that span lines (block
 * comments) are split across the lines they cover. The `*mode = X;`
 * directive line gets its own class; every token on that line is
 * consumed by the covering segment (otherwise the trailing tokens
 * would be emitted a second time after it).
 */
export function segmentsForLines(text: string, tokens: Token[]): Seg[][] {
  const lines = text.split('\n');
  const lineCount = lines.length;
  const segs: Seg[][] = Array.from({ length: lineCount }, () => []);

  // Directive lines first: one covering segment per `*mode = X;` line.
  const modeLines = new Set<number>();
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t.kind === '*' && tokens[i + 1]?.kind === 'ident' && tokens[i + 1].text === 'mode') {
      modeLines.add(t.line);
      const lineText = lines[t.line - 1] ?? '';
      segs[t.line - 1].push({ start: t.col - 1, end: lineText.length, cls: 'mode' });
    }
  }

  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (modeLines.has(t.line)) continue; // covered by the directive segment

    const cls = classFor(t.kind);
    if (!cls) continue;

    const parts = t.text.split('\n');
    let line = t.line;
    for (let p = 0; p < parts.length; p++) {
      const part = parts[p];
      const start = p === 0 ? t.col - 1 : 0;
      if (part.length > 0 && line - 1 < lineCount) {
        segs[line - 1].push({ start, end: start + part.length, cls });
      }
      line += 1;
    }
  }

  for (const line of segs) line.sort((a, b) => a.start - b.start);
  return segs;
}

/** Escape text for safe use in innerHTML. */
export function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

/** Render one line's HTML from its text and segments. */
export function renderLine(text: string, segs: Seg[]): string {
  let html = '';
  let pos = 0;
  for (const s of segs) {
    if (s.start > pos) html += escapeHtml(text.slice(pos, s.start));
    html += `<span class="tk-${s.cls}">${escapeHtml(text.slice(s.start, s.end))}</span>`;
    pos = s.end;
  }
  html += escapeHtml(text.slice(pos));
  return html;
}
