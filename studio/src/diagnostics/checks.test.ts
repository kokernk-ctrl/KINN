import { describe, expect, it } from 'vitest';
import { detectMode, quickCheck } from './checks';

const HELLO = `*mode = B;

func main() {
    print("Hello, Kin!");
}
`;

describe('editor quick checks', () => {
  it('accepts a clean program', () => {
    expect(quickCheck(HELLO)).toEqual([]);
  });

  it('flags unterminated strings', () => {
    const problems = quickCheck('func main() {\n    let s = "oops;\n}\n');
    expect(problems.some((p) => p.code === 'E1002')).toBe(true);
  });

  it('flags unmatched closing braces', () => {
    const problems = quickCheck('func main() {\n    print(1);\n}\n}\n');
    expect(problems.some((p) => p.code === 'E2101' && p.message.includes('}'))).toBe(true);
  });

  it('flags unclosed opening braces with their position', () => {
    const problems = quickCheck('func main() {\n    print(1);\n');
    const unclosed = problems.find((p) => p.code === 'E2102');
    expect(unclosed).toBeDefined();
    expect(unclosed?.line).toBe(1);
  });

  it('does not count braces inside strings or comments', () => {
    const src = `// } comment {\nfunc main() {\n    print("}{");\n}\n`;
    expect(quickCheck(src)).toEqual([]);
  });

  it('validates the mode directive value', () => {
    const problems = quickCheck('*mode = Z;\nfunc main() { }\n');
    expect(problems.some((p) => p.code === 'E2110')).toBe(true);
  });

  it('rejects a mode directive that is not first', () => {
    const problems = quickCheck('func main() { }\n*mode = B;\n');
    expect(problems.some((p) => p.code === 'E2110')).toBe(true);
  });

  it('does not confuse multiplication with the mode directive', () => {
    // The `*` in `6 * 7` is multiplication, not a directive.
    const src = '*mode = B;\nfunc main() { let answer = 6 * 7; print(answer); }\n';
    expect(quickCheck(src)).toEqual([]);
  });

  it('flags duplicate functions', () => {
    const problems = quickCheck('func f() { }\nfunc f() { }\nfunc main() { }\n');
    expect(problems.some((p) => p.code === 'E3002' && p.message.includes('f'))).toBe(true);
  });

  it('sorts problems by line', () => {
    const problems = quickCheck('func f() {\n  let s = "x\n}\n}\n');
    expect(problems.length).toBeGreaterThan(0);
    const lines = problems.map((p) => p.line);
    expect([...lines].sort((a, b) => a - b)).toEqual(lines);
  });
});

describe('mode detection', () => {
  it('reads the declared mode', () => {
    expect(detectMode('*mode = A;\nfunc main() { }')).toBe('A');
    expect(detectMode('*mode = B;\nfunc main() { }')).toBe('B');
  });

  it('defaults to S without a directive', () => {
    expect(detectMode('func main() { }')).toBe('S');
  });

  it('ignores directives after the first statement', () => {
    expect(detectMode('func main() { }\n*mode = B;')).toBe('S');
  });
});
