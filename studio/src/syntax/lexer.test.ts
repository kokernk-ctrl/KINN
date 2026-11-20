import { describe, expect, it } from 'vitest';
import { lex } from './lexer';

describe('studio lexer', () => {
  it('tokenizes keywords, types and identifiers', () => {
    const { tokens, errors } = lex('func let var if while int float string');
    expect(errors).toEqual([]);
    const kinds = tokens.map((t) => t.kind);
    expect(kinds).toEqual(['func', 'let', 'var', 'if', 'while', 'int-ty', 'float-ty', 'string-ty', 'eof']);
  });

  it('classifies numbers as int or float', () => {
    const { tokens, errors } = lex('42 3.14 0.5');
    expect(errors).toEqual([]);
    expect(tokens.map((t) => t.kind)).toEqual(['int', 'float', 'float', 'eof']);
  });

  it('keeps comments as tokens (needed by the highlighter)', () => {
    const { tokens } = lex('// line\nlet x; /* block */');
    const comments = tokens.filter((t) => t.kind === 'comment');
    expect(comments.map((c) => c.text)).toEqual(['// line', '/* block */']);
  });

  it('tracks line and column positions', () => {
    const { tokens } = lex('let x;\nlet y;');
    const secondLet = tokens.find((t) => t.kind === 'let' && t.line === 2);
    expect(secondLet?.col).toBe(1);
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
    const { errors } = lex('let $x;');
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
