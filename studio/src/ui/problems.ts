/**
 * Problems panel: clickable list of diagnostics — editor-level checks or
 * full compiler diagnostics (via the wasm backend), including multi-file
 * diagnostics that carry their own file name.
 */

import type { Problem } from '../diagnostics/checks';

export class ProblemsPanel {
  private list: HTMLElement;
  private count: HTMLElement;
  /** Runtime errors from the previewed app (C.7) — appended, not checked. */
  private runtime: Problem[] = [];

  constructor(list: HTMLElement, count: HTMLElement, private onJump: (line: number) => void) {
    this.list = list;
    this.count = count;
  }

  /** Add a preview runtime error (deduplicated by code+line). */
  appendRuntime(p: Problem): void {
    if (this.runtime.some((r) => r.code === p.code && r.line === p.line && r.message === p.message)) return;
    this.runtime.push(p);
    this.renderChecked(this.lastProblems, this.lastFile);
  }

  /** Drop preview runtime errors (a fresh preview run starts clean). */
  clearRuntime(): void {
    this.runtime = [];
    this.renderChecked(this.lastProblems, this.lastFile);
  }

  private lastProblems: Problem[] = [];
  private lastFile = '';

  render(problems: Problem[], fileName: string): void {
    this.lastProblems = problems;
    this.lastFile = fileName;
    this.renderChecked(problems, fileName);
  }

  private renderChecked(problems: Problem[], fileName: string): void {
    const all = problems.concat(this.runtime);
    this.count.textContent = String(all.length);
    this.list.replaceChildren();

    if (all.length === 0) {
      const li = document.createElement('li');
      li.className = 'problems-empty';
      li.textContent = 'No issues found.';
      this.list.appendChild(li);
      return;
    }

    for (const p of problems) {
      const li = document.createElement('li');
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = `problem ${p.severity}`;
      const sev = document.createElement('span');
      sev.className = 'sev';
      sev.textContent = p.severity === 'error' ? 'error' : p.severity === 'warning' ? 'warn' : 'note';
      const msg = document.createElement('span');
      msg.textContent = `${p.code}: ${p.message}`;
      const loc = document.createElement('span');
      loc.className = 'loc';
      loc.textContent = `${p.file ?? fileName}:${p.line}`;
      btn.appendChild(sev);
      btn.appendChild(msg);
      btn.appendChild(loc);
      btn.addEventListener('click', () => this.onJump(p.line));
      li.appendChild(btn);
      this.list.appendChild(li);
    }
  }
}
