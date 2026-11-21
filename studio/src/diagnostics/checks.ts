/**
 * Editor-side quick checks for Kin files.
 *
 * These are honest, real checks that run fully in the browser:
 *  1. lexical errors (from the editor lexer)
 *  2. delimiter balance ( `(`/`)` and `{`/`}` ) with positions
 *  3. mode directive validation (must be first, value must be B/S/A)
 *  4. duplicate function names
 *
 * They are NOT the full type checker — semantic analysis runs in the
 * kinc compiler (`kinn check file.kin`), whose integration into Studio
 * is planned for 0.2 via WebAssembly.
 */

import { lex } from '../syntax/lexer';

export type Severity = 'error' | 'warning' | 'info' | 'note';

export interface Problem {
  severity: Severity;
  code: string;
  message: string;
  line: number;
  col: number;
  /** File the diagnostic came from (multi-file compiler diagnostics). */
  file?: string;
}

export function quickCheck(text: string): Problem[] {
  const problems: Problem[] = [];
  const { tokens, errors } = lex(text);

  for (const e of errors) {
    problems.push({ severity: 'error', code: e.code, message: e.message, line: e.line, col: e.col });
  }

  // delimiter balance
  const stack: { ch: '(' | '{'; line: number; col: number }[] = [];
  for (const t of tokens) {
    if (t.kind === '(' || t.kind === '{') {
      stack.push({ ch: t.kind, line: t.line, col: t.col });
    } else if (t.kind === ')' || t.kind === '}') {
      const expected = t.kind === ')' ? '(' : '{';
      const top = stack.pop();
      if (!top || top.ch !== expected) {
        problems.push({
          severity: 'error',
          code: 'E2101',
          message: `unmatched '${t.kind}'`,
          line: t.line,
          col: t.col,
        });
      }
    }
  }
  for (const open of stack) {
    problems.push({
      severity: 'error',
      code: 'E2102',
      message: `unclosed '${open.ch}'`,
      line: open.line,
      col: open.col,
    });
  }

  // mode directive: if a `*` token appears it must start a valid directive
  const first = tokens.find((t) => t.kind !== 'comment');
  if (first && first.kind === '*') {
    const i = tokens.indexOf(first);
    const [modeKw, eq, value, semi] = [tokens[i + 1], tokens[i + 2], tokens[i + 3], tokens[i + 4]];
    const ok = modeKw?.kind === 'ident' && modeKw.text === 'mode'
      && eq?.kind === '='
      && value?.kind === 'ident' && ['B', 'S', 'A'].includes(value.text)
      && semi?.kind === ';';
    if (!ok) {
      problems.push({
        severity: 'error',
        code: 'E2110',
        message: "invalid mode directive (expected '*mode = B | S | A;' as the first statement)",
        line: first.line,
        col: first.col,
      });
    }
  }
  // A `*mode =` shape anywhere except the first statement is a directive
  // written in the wrong place. A bare `*` (multiplication) is not.
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].kind !== '*') continue;
    if (first && tokens[i] === first) continue; // handled above
    if (
      tokens[i + 1]?.kind === 'ident' && tokens[i + 1].text === 'mode'
      && tokens[i + 2]?.kind === '='
    ) {
      problems.push({
        severity: 'error',
        code: 'E2110',
        message: 'the *mode directive must be the first statement of the file',
        line: tokens[i].line,
        col: tokens[i].col,
      });
    }
  }

  // duplicate function names
  const seen = new Map<string, number>();
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].kind === 'func' && tokens[i + 1]?.kind === 'ident') {
      const name = tokens[i + 1].text;
      if (seen.has(name)) {
        problems.push({
          severity: 'error',
          code: 'E3002',
          message: `duplicate function '${name}' (first defined at line ${seen.get(name)})`,
          line: tokens[i + 1].line,
          col: tokens[i + 1].col,
        });
      } else {
        seen.set(name, tokens[i + 1].line);
      }
    }
  }

  problems.sort((a, b) => a.line - b.line || a.col - b.col);
  return problems;
}

/** Detect the programming mode declared in the file (default S). */
export function detectMode(text: string): 'B' | 'S' | 'A' {
  const { tokens } = lex(text);
  const first = tokens.find((t) => t.kind !== 'comment');
  if (first?.kind === '*') {
    const i = tokens.indexOf(first);
    const value = tokens[i + 3];
    if (value?.kind === 'ident' && ['B', 'S', 'A'].includes(value.text)) {
      return value.text as 'B' | 'S' | 'A';
    }
  }
  return 'S';
}
