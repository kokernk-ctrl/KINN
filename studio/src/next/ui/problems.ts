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

  constructor(
    root: HTMLElement,
    private cb: ProblemsCallbacks,
  ) {
    this.list = root.querySelector('#nx-problems-list') as HTMLElement;
    this.badgeTotal = root.querySelector('#nx-problems-count') as HTMLElement;
    this.badgeErrors = root.querySelector('#nx-problems-errors') as HTMLElement;
    this.badgeWarnings = root.querySelector('#nx-problems-warnings') as HTMLElement;
  }

  render(problems: ProblemView[], fileName: string): void {
    const errors = problems.filter((p) => p.severity === 'error').length;
    const warnings = problems.filter((p) => p.severity === 'warning').length;
    this.badgeTotal.textContent = String(problems.length);
    this.badgeErrors.textContent = String(errors);
    this.badgeWarnings.textContent = String(warnings);
    this.list.replaceChildren();

    if (problems.length === 0) {
      const li = document.createElement('div');
      li.className = 'nx-problems-empty';
      li.textContent = 'No problems. Run check / build to refresh.';
      this.list.appendChild(li);
      return;
    }

    for (const p of problems) {
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
