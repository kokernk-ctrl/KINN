/**
 * Tests for the token -> segment -> HTML rendering, including the
 * regression for the `*mode` directive line (its trailing tokens used
 * to be emitted a second time after the covering segment).
 */

import { describe, expect, it } from 'vitest';
import { lex } from './lexer';
import { renderLine, segmentsForLines } from './highlight';

describe('segmentsForLines', () => {
  it('covers the whole mode directive line with one segment', () => {
    const src = '*mode = B;\nfin x = 1;';
    const { tokens } = lex(src);
    const segs = segmentsForLines(src, tokens);
    expect(segs[0]).toHaveLength(1);
    expect(segs[0][0]).toEqual({ start: 0, end: 10, cls: 'mode' });
    // second line keeps its normal tokens
    expect(segs[1].some((s) => s.cls === 'kw')).toBe(true);
  });

  it('renders the mode line without duplicating its trailing tokens', () => {
    const src = '*mode = B;\nfunc main() { }';
    const { tokens } = lex(src);
    const segs = segmentsForLines(src, tokens);
    const html = renderLine('*mode = B;', segs[0]);
    // exactly one span covering the line — no duplicated `= B;` suffix
    expect(html).toBe('<span class="tk-mode">*mode = B;</span>');
    expect(html.match(/tk-op/g)).toBeNull();
  });

  it('handles an indented mode directive', () => {
    const src = '// c\n  *mode = A;\nfin y = 2;';
    const { tokens } = lex(src);
    const segs = segmentsForLines(src, tokens);
    const html = renderLine('  *mode = A;', segs[1]);
    expect(html).toBe('  <span class="tk-mode">*mode = A;</span>');
  });

  it('keeps block comments split across lines', () => {
    const src = '/* a\nb */ fin z = 3;';
    const { tokens } = lex(src);
    const segs = segmentsForLines(src, tokens);
    expect(segs[0][0].cls).toBe('comment');
    expect(segs[1].some((s) => s.cls === 'comment')).toBe(true);
    expect(segs[1].some((s) => s.cls === 'kw')).toBe(true);
  });
});
