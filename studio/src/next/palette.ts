/**
 * Command palette for Studio Next.
 *
 * `fuzzyScore` is a headless, deterministic subsequence matcher with
 * bonuses for consecutive and word-start hits — it ranks commands and
 * file names. The palette itself only ever lists commands that were
 * registered with a working `run()` — a command that cannot run is not
 * shown at all (spec: "Only expose commands that actually work").
 */

export interface PaletteCommand {
  id: string;
  title: string;
  /** Optional right-side hint (e.g. keybinding). */
  hint?: string;
  group: 'file' | 'build' | 'view' | 'terminal' | 'project' | 'app';
  run: () => void;
}

/**
 * Score `query` against `text` (both lowercased ASCII-tolerant).
 * Returns null when the query is not a subsequence; higher is better.
 */
export function fuzzyScore(query: string, text: string): number | null {
  if (query.length === 0) return 0;
  const q = query.toLowerCase();
  const t = text.toLowerCase();

  // direct substring hit — strongest signal
  const sub = t.indexOf(q);
  if (sub !== -1) {
    let score = 100 - Math.min(sub, 40);
    if (sub === 0) score += 20;
    return score + Math.max(0, 20 - (t.length - q.length));
  }

  let score = 0;
  let ti = 0;
  let prev = -2;
  for (let qi = 0; qi < q.length; qi++) {
    const ch = q[qi];
    const idx = t.indexOf(ch, ti);
    if (idx === -1) return null;
    score += idx === prev + 1 ? 6 : 2; // consecutive bonus
    if (idx === 0 || /[\s._\-/]/.test(t[idx - 1] ?? '')) score += 5; // word start
    score -= Math.min(idx - ti, 8); // gap penalty
    prev = idx;
    ti = idx + 1;
  }
  return score;
}

/** Filter + rank commands for a query. Empty query keeps registration order. */
export function filterCommands(commands: PaletteCommand[], query: string): PaletteCommand[] {
  if (query.trim() === '') return [...commands];
  const scored: Array<{ cmd: PaletteCommand; score: number }> = [];
  for (const cmd of commands) {
    const s = fuzzyScore(query.trim(), `${cmd.title} ${cmd.id}`);
    if (s !== null) scored.push({ cmd, score: s });
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.map((x) => x.cmd);
}

/**
 * Filter plain names (file open mode). Returns names sorted by score.
 */
export function filterNames(names: string[], query: string): string[] {
  if (query.trim() === '') return [...names];
  const scored: Array<{ name: string; score: number }> = [];
  for (const name of names) {
    const s = fuzzyScore(query.trim(), name);
    if (s !== null) scored.push({ name, score: s });
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.map((x) => x.name);
}
