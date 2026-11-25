/**
 * Problems panel: compiler diagnostics (wasm backend) or editor-level
 * checks, with file/line references. Clicking a problem opens its file
 * (multi-file aware) and jumps to the line.
 */

export interface ProblemView {
  severity: string;
  code: string;
  message: string;
  file?: string;
  line: number;
  col?: number;
}

export interface ProblemsCallbacks {
  onJump(file: string, line: number): void;
}

export class ProblemsPanel {
  private list: HTMLElement;
  private badgeTotal: HTMLElement;
  private badgeErrors: HTMLElement;
  private badgeWarnings: HTMLElement;
  /** Runtime errors from the previewed app (C.7) — appended, not checked. */
  private runtime: ProblemView[] = [];
  private lastProblems: ProblemView[] = [];
  private lastFile = '';

  constructor(
    root: HTMLElement,
    private cb: ProblemsCallbacks,
  ) {
    this.list = root.querySelector('#nx-problems-list') as HTMLElement;
    this.badgeTotal = root.querySelector('#nx-problems-count') as HTMLElement;
    this.badgeErrors = root.querySelector('#nx-problems-errors') as HTMLElement;
    this.badgeWarnings = root.querySelector('#nx-problems-warnings') as HTMLElement;
  }

  /** Add a preview runtime error (deduplicated by code+line+message). */
  appendRuntime(p: ProblemView): void {
    if (this.runtime.some((r) => r.code === p.code && r.line === p.line && r.message === p.message)) return;
    this.runtime.push(p);
    this.renderAll(this.lastProblems, this.lastFile);
  }

  /** Drop preview runtime errors (a fresh preview run starts clean). */
  clearRuntime(): void {
    this.runtime = [];
    this.renderAll(this.lastProblems, this.lastFile);
  }

  render(problems: ProblemView[], fileName: string): void {
    this.lastProblems = problems;
    this.lastFile = fileName;
    this.renderAll(problems, fileName);
  }

  private renderAll(problems: ProblemView[], fileName: string): void {
    const all = problems.concat(this.runtime);
    const errors = all.filter((p) => p.severity === 'error').length;
    const warnings = all.filter((p) => p.severity === 'warning').length;
    this.badgeTotal.textContent = String(all.length);
    this.badgeErrors.textContent = String(errors);
    this.badgeWarnings.textContent = String(warnings);
    this.list.replaceChildren();

    if (all.length === 0) {
      const li = document.createElement('div');
      li.className = 'nx-problems-empty';
      li.textContent = 'No problems. Run check / build to refresh.';
      this.list.appendChild(li);
      return;
    }

    for (const p of all) {
      const row = document.createElement('button');
      row.type = 'button';
      row.className = `nx-problem sev-${p.severity}`;
      const sev = document.createElement('span');
      sev.className = 'nx-problem-sev';
      sev.textContent = p.severity === 'error' ? 'err' : p.severity === 'warning' ? 'warn' : 'note';
      const msg = document.createElement('span');
      msg.className = 'nx-problem-msg';
      msg.textContent = `${p.code} ${p.message}`;
      const loc = document.createElement('span');
      loc.className = 'nx-problem-loc';
      loc.textContent = `${p.file ?? fileName}:${p.line}${p.col ? ':' + p.col : ''}`;
      row.appendChild(sev);
      row.appendChild(msg);
      row.appendChild(loc);
      row.addEventListener('click', () => this.cb.onJump(p.file ?? fileName, p.line));
      this.list.appendChild(row);
    }
  }
}
