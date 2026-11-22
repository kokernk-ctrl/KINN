import { describe, expect, it } from 'vitest';
import { lex } from './lexer';

describe('studio lexer', () => {
  it('tokenizes keywords, types and identifiers', () => {
    const { tokens, errors } = lex('func fin var if while int float string');
    expect(errors).toEqual([]);
    const kinds = tokens.map((t) => t.kind);
    expect(kinds).toEqual(['func', 'fin', 'var', 'if', 'while', 'int-ty', 'float-ty', 'string-ty', 'eof']);
    // ADR-0029: `let` is no longer a keyword — it lexes as an identifier
    // (the compiler reports the E2017 migration on the declaration
    // pattern; the Studio lexer mirrors the token stream).
    const legacy = lex('let x;');
    expect(legacy.tokens[0].kind).toBe('ident');
    expect(legacy.tokens[0].text).toBe('let');
  });

  it('classifies numbers as int or float', () => {
    const { tokens, errors } = lex('42 3.14 0.5');
    expect(errors).toEqual([]);
    expect(tokens.map((t) => t.kind)).toEqual(['int', 'float', 'float', 'eof']);
  });

  it('keeps comments as tokens (needed by the highlighter)', () => {
    const { tokens } = lex('// line\nfin x; /* block */');
    const comments = tokens.filter((t) => t.kind === 'comment');
    expect(comments.map((c) => c.text)).toEqual(['// line', '/* block */']);
  });

  it('tracks line and column positions', () => {
    const { tokens } = lex('fin x;\nfin y;');
    const secondFin = tokens.find((t) => t.kind === 'fin' && t.line === 2);
    expect(secondFin?.col).toBe(1);
  });

  it('rejects unterminated strings with E1002', () => {
    const { errors } = lex('"oops');
    expect(errors.some((e) => e.code === 'E1002')).toBe(true);
  });

  it('rejects invalid escapes with E1003', () => {
    const { errors } = lex('"bad\\q"');
    expect(errors.some((e) => e.code === 'E1003')).toBe(true);
  });

  it('rejects invalid characters with E1001', () => {
    const { errors } = lex('fin $x;');
    expect(errors.some((e) => e.code === 'E1001' && e.message.includes('$'))).toBe(true);
  });

  it('recognizes the star used by the mode directive', () => {
    const { tokens } = lex('*mode = B;');
    expect(tokens[0].kind).toBe('*');
    expect(tokens[1].kind).toBe('ident');
    expect(tokens[1].text).toBe('mode');
  });

  it('handles two-character operators', () => {
    const { tokens } = lex('== != <= >= -> +=');
    expect(tokens.map((t) => t.kind)).toEqual(['==', '!=', '<=', '>=', '->', '+=', 'eof']);
  });
});

// -- 0.2 tokens ---------------------------------------------------------------

function kindsOf(src: string): string[] {
  return lex(src).tokens.map((t) => t.kind);
}

describe('0.2 tokens: ranges, brackets, new keywords', () => {
  it('lexes exclusive and inclusive ranges without float confusion', () => {
    expect(kindsOf('0..10')).toEqual(['int', '..', 'int', 'eof']);
    expect(kindsOf('0..=9')).toEqual(['int', '..=', 'int', 'eof']);
    expect(kindsOf('1.5..2.5')).toEqual(['float', '..', 'float', 'eof']);
    expect(kindsOf('1..=1.5')).toEqual(['int', '..=', 'float', 'eof']);
  });

  it('still rejects malformed numbers', () => {
    const r = lex('1.2.3');
    expect(r.errors.some((e) => e.code === 'E1004')).toBe(true);
  });

  it('lexes brackets, dots and the new keywords', () => {
    expect(kindsOf('[1, 2]')).toEqual(['[', 'int', ',', 'int', ']', 'eof']);
    expect(kindsOf('for i in a.b { }')).toEqual([
      'for', 'ident', 'in', 'ident', '.', 'ident', '{', '}', 'eof',
    ]);
    expect(kindsOf('as import')).toEqual(['as', 'import', 'eof']);
  });
});
