/**
 * Kin token model for the editor side. Mirrors the token set of the
 * reference Rust lexer in `compiler/crates/kinc/src/token.rs`; comments
 * are kept as tokens because the highlighter needs them.
 */

export type TokKind =
  | 'int'
  | 'float'
  | 'string'
  | 'ident'
  | 'comment'
  | 'func' | 'fin' | 'var' | 'if' | 'else' | 'while' | 'for' | 'in' | 'return' | 'break' | 'continue'
  | 'true' | 'false' | 'and' | 'or' | 'not' | 'as' | 'import'
  | 'int-ty' | 'float-ty' | 'bool-ty' | 'string-ty' | 'void-ty'
  | '*' | '+' | '-' | '/' | '%' | '=' | '+=' | '-='
  | '==' | '!=' | '<' | '>' | '<=' | '>=' | '->'
  | '(' | ')' | '{' | '}' | '[' | ']' | ',' | ':' | ';' | '.' | '..' | '..='
  | 'invalid'
  | 'eof';

export interface Token {
  kind: TokKind;
  /** Raw source text of the token. */
  text: string;
  /** 1-based line. */
  line: number;
  /** 1-based column in characters. */
  col: number;
}

export interface LexError {
  code: string;
  message: string;
  line: number;
  col: number;
}

const KEYWORDS: Record<string, TokKind> = {
  func: 'func', fin: 'fin', var: 'var', if: 'if', else: 'else',
  while: 'while', for: 'for', in: 'in', return: 'return', break: 'break', continue: 'continue',
  true: 'true', false: 'false', and: 'and', or: 'or', not: 'not',
  as: 'as', import: 'import',
  int: 'int-ty', float: 'float-ty', bool: 'bool-ty',
  string: 'string-ty', void: 'void-ty',
};

export interface LexResult {
  tokens: Token[];
  errors: LexError[];
}

/** Tokenize Kin source. Errors are collected; lexing continues. */
export function lex(src: string): LexResult {
  const chars = [...src];
  const tokens: Token[] = [];
  const errors: LexError[] = [];
  let pos = 0;
  let line = 1;
  let col = 1;

  const peek = (ahead = 0): string | undefined => chars[pos + ahead];
  const bump = (): string | undefined => {
    const c = chars[pos];
    if (c === undefined) return undefined;
    pos += 1;
    if (c === '\n') {
      line += 1;
      col = 1;
    } else {
      col += 1;
    }
    return c;
  };

  while (pos < chars.length) {
    const c = chars[pos];
    const startLine = line;
    const startCol = col;

    // whitespace
    if (c === ' ' || c === '\t' || c === '\r' || c === '\n') {
      bump();
      continue;
    }

    // line comment
    if (c === '/' && peek(1) === '/') {
      let text = '';
      while (pos < chars.length && chars[pos] !== '\n') text += bump();
      tokens.push({ kind: 'comment', text, line: startLine, col: startCol });
      continue;
    }

    // block comment (may span lines)
    if (c === '/' && peek(1) === '*') {
      let text = '';
      bump(); bump();
      text += '/*';
      let closed = false;
      while (pos < chars.length) {
        if (chars[pos] === '*' && peek(1) === '/') {
          bump(); bump();
          text += '*/';
          closed = true;
          break;
        }
        text += bump();
      }
      tokens.push({ kind: 'comment', text, line: startLine, col: startCol });
      if (!closed) {
        errors.push({ code: 'E1006', message: 'unterminated block comment (missing */)', line: startLine, col: startCol });
      }
      continue;
    }

    // string literal
    if (c === '"') {
      let raw = '"';
      bump();
      let closed = false;
      while (pos < chars.length) {
        const sc = chars[pos];
        if (sc === '\n') break;
        if (sc === '"') {
          bump();
          raw += '"';
          closed = true;
          break;
        }
        if (sc === '\\') {
          const next = peek(1);
          if (next === undefined) {
            raw += bump();
            break;
          }
          if (next === 'n' || next === 't' || next === 'r' || next === '"' || next === '\\') {
            raw += bump()! + bump()!;
          } else {
            raw += bump()! + bump()!;
            errors.push({
              code: 'E1003',
              message: `invalid escape sequence \\${next} in string literal (allowed: \\n \\t \\r \\" \\\\)`,
              line: startLine,
              col: startCol,
            });
          }
          continue;
        }
        raw += bump();
      }
      if (!closed) {
        errors.push({ code: 'E1002', message: 'unterminated string literal', line: startLine, col: startCol });
      }
      tokens.push({ kind: 'string', text: raw, line: startLine, col: startCol });
      continue;
    }

    // number
    if (/[0-9]/.test(c)) {
      let text = '';
      while (pos < chars.length && /[0-9]/.test(chars[pos])) text += bump();
      if (chars[pos] === '.' && /[0-9]/.test(peek(1) ?? '')) {
        text += bump();
        while (pos < chars.length && /[0-9]/.test(chars[pos])) text += bump();
        // A second '.' is only legal when it starts a range: `1.5..2`.
        if (chars[pos] === '.' && peek(1) !== '.') {
          errors.push({ code: 'E1004', message: `malformed number literal ${text}. (a second . is not allowed)`, line: startLine, col: startCol });
          bump();
          continue;
        }
        tokens.push({ kind: 'float', text, line: startLine, col: startCol });
      } else if (chars[pos] === '.' && peek(1) === '.') {
        // `0..10` — the dots start a range, not a float.
        tokens.push({ kind: 'int', text, line: startLine, col: startCol });
      } else if (chars[pos] === '.') {
        errors.push({ code: 'E1004', message: 'malformed number literal: expected a digit after .', line, col });
        bump();
        continue;
      } else {
        tokens.push({ kind: 'int', text, line: startLine, col: startCol });
      }
      continue;
    }

    // identifier / keyword
    if (/[A-Za-z_]/.test(c)) {
      let text = '';
      while (pos < chars.length && /[A-Za-z0-9_]/.test(chars[pos])) text += bump();
      tokens.push({ kind: KEYWORDS[text] ?? 'ident', text, line: startLine, col: startCol });
      continue;
    }

    // operators and punctuation
    // '..=' first — it is three characters and must win over '..' and '='.
    if (c === '.' && peek(1) === '.' && peek(2) === '=') {
      bump(); bump(); bump();
      tokens.push({ kind: '..=', text: '..=', line: startLine, col: startCol });
      continue;
    }
    const twoChar: Record<string, TokKind> = {
      '==': '==', '!=': '!=', '<=': '<=', '>=': '>=', '->': '->',
      '+=': '+=', '-=': '-=', '..': '..',
    };
    const pair = c + (peek(1) ?? '');
    if (twoChar[pair]) {
      bump(); bump();
      tokens.push({ kind: twoChar[pair], text: pair, line: startLine, col: startCol });
      continue;
    }
    const singles: Record<string, TokKind> = {
      '*': '*', '+': '+', '-': '-', '/': '/', '%': '%', '=': '=',
      '(': '(', ')': ')', '{': '{', '}': '}', '[': '[', ']': ']',
      ',': ',', ':': ':', ';': ';', '.': '.',
      '<': '<', '>': '>',
    };
    if (singles[c]) {
      bump();
      tokens.push({ kind: singles[c], text: c, line: startLine, col: startCol });
      continue;
    }

    // anything else is invalid
    const text = bump() ?? '';
    const hint = c === '!'
      ? ' (use not for logical negation, != for inequality)'
      : '';
    errors.push({ code: 'E1001', message: `unexpected character ${c}${hint}`, line: startLine, col: startCol });
    tokens.push({ kind: 'invalid', text, line: startLine, col: startCol });
  }

  tokens.push({ kind: 'eof', text: '', line, col });
  return { tokens, errors };
}
