/**
 * Top bar: identity, project/file breadcrumb, live build status pill,
 * run/build actions, layout switch, palette and settings access, and a
 * link back to the classic Studio (kept intact side by side).
 */

export type BuildState = 'idle' | 'busy' | 'ok' | 'fail';

export interface TopbarCallbacks {
  onRun(): void;
  onBuild(): void;
  onPalette(): void;
  onSettings(): void;
}

export class Topbar {
  private project: HTMLElement;
  private file: HTMLElement;
  private pill: HTMLElement;
  private pillText: HTMLElement;
  private backendBadge: HTMLElement;

  constructor(
    root: HTMLElement,
    private cb: TopbarCallbacks,
  ) {
    this.project = root.querySelector('#nx-project') as HTMLElement;
    this.file = root.querySelector('#nx-file') as HTMLElement;
    this.pill = root.querySelector('#nx-buildstatus') as HTMLElement;
    this.pillText = root.querySelector('#nx-buildstatus-text') as HTMLElement;
    this.backendBadge = root.querySelector('#nx-backend-badge') as HTMLElement;

    (root.querySelector('#nx-btn-run') as HTMLButtonElement).addEventListener('click', () => this.cb.onRun());
    (root.querySelector('#nx-btn-build') as HTMLButtonElement).addEventListener('click', () => this.cb.onBuild());
    (root.querySelector('#nx-btn-palette') as HTMLButtonElement).addEventListener('click', () => this.cb.onPalette());
    (root.querySelector('#nx-btn-settings') as HTMLButtonElement).addEventListener('click', () => this.cb.onSettings());
  }

  setProject(name: string): void {
    this.project.textContent = name;
  }

  setFile(name: string): void {
    this.file.textContent = name;
  }

  setBackend(ready: boolean, version: string | null): void {
    this.backendBadge.textContent = ready ? `kinn ${version} · wasm` : 'compiler not built';
    this.backendBadge.classList.toggle('missing', !ready);
    this.backendBadge.title = ready
      ? 'Real Kin compiler and VM, compiled to WebAssembly from the same Rust code as the CLI'
      : 'kinn.wasm was not found — check/run/build degrade honestly to editor-level checks. Build with: cd studio && npm run build:wasm';
  }

  setBuildState(state: BuildState, label: string): void {
    this.pill.dataset.state = state;
    this.pillText.textContent = label;
  }
}
