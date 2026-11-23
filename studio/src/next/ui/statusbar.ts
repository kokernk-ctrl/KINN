/**
 * Bottom status bar: project/storage status, compiler status, current
 * file, cursor position, runtime status of the last command and the
 * offline/online state. Everything shown is measured, not decorative.
 */

export interface StatusBarCallbacks {
  onToggleExplorer(): void;
  onToggleBottom(): void;
}

export class StatusBar {
  private storage: HTMLElement;
  private compiler: HTMLElement;
  private file: HTMLElement;
  private cursor: HTMLElement;
  private runtime: HTMLElement;
  private net: HTMLElement;
  private netDot: HTMLElement;

  constructor(root: HTMLElement, cb: StatusBarCallbacks) {
    this.storage = root.querySelector('#nx-sb-storage') as HTMLElement;
    this.compiler = root.querySelector('#nx-sb-compiler') as HTMLElement;
    this.file = root.querySelector('#nx-sb-file') as HTMLElement;
    this.cursor = root.querySelector('#nx-sb-cursor') as HTMLElement;
    this.runtime = root.querySelector('#nx-sb-runtime') as HTMLElement;
    this.net = root.querySelector('#nx-sb-net') as HTMLElement;
    this.netDot = root.querySelector('#nx-sb-net-dot') as HTMLElement;
    (root.querySelector('#nx-sb-toggle-explorer') as HTMLButtonElement).addEventListener('click', () => cb.onToggleExplorer());
    (root.querySelector('#nx-sb-toggle-bottom') as HTMLButtonElement).addEventListener('click', () => cb.onToggleBottom());
  }

  setStorage(text: string, cls?: string): void {
    this.storage.textContent = text;
    this.storage.className = cls ?? '';
  }

  setCompiler(text: string, cls?: string): void {
    this.compiler.textContent = text;
    this.compiler.className = cls ?? '';
  }

  setFile(name: string): void {
    this.file.textContent = name || 'no file';
  }

  setCursor(line: number, col: number, lines: number): void {
    this.cursor.textContent = `Ln ${line}, Col ${col} · ${lines} lines`;
  }

  setRuntime(text: string, cls?: string): void {
    this.runtime.textContent = text;
    this.runtime.className = cls ?? '';
  }

  setNet(online: boolean): void {
    this.net.textContent = online ? 'ONLINE' : 'OFFLINE';
    this.netDot.dataset.state = online ? 'on' : 'off';
    this.net.title = online
      ? 'Network reachable. KINN Studio is offline-first: compilation, runs and builds are local (wasm), no accounts, no servers.'
      : 'No network. Everything keeps working: compiler, VM and builds run locally in WebAssembly.';
  }
}
