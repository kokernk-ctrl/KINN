/**
 * Problems panel: clickable list of diagnostics — editor-level checks or
 * full compiler diagnostics (via the wasm backend), including multi-file
 * diagnostics that carry their own file name.
 */

import type { Problem } from '../diagnostics/checks';

export class ProblemsPanel {
  private list: HTMLElement;
  private count: HTMLElement;

  constructor(list: HTMLElement, count: HTMLElement, private onJump: (line: number) => void) {
    this.list = list;
    this.count = count;
  }

  render(problems: Problem[], fileName: string): void {
    this.count.textContent = String(problems.length);
    this.list.replaceChildren();

    if (problems.length === 0) {
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
