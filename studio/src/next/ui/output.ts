/**
 * Output panel: structured results of real builds (target, site, page
 * count, artifact list with sizes) and the actions that work on them —
 * preview the web build, download it as .zip, export the Android Gradle
 * project. A build that has not happened is shown as exactly that.
 */

import type { BuildResult } from '../../wasm/kinnwasm';

export interface OutputCallbacks {
  onPreviewWeb(): void;
}

export class OutputPanel {
  private root: HTMLElement;
  private logEl: HTMLElement;

  constructor(root: HTMLElement, private cb: OutputCallbacks) {
    this.root = root;
    this.logEl = root.querySelector('#nx-output-log') as HTMLElement;
    (root.querySelector('#nx-btn-out-preview') as HTMLButtonElement).addEventListener('click', () => this.cb.onPreviewWeb());
    (root.querySelector('#nx-btn-out-zip') as HTMLButtonElement).addEventListener('click', () => this.downloadWeb());
    (root.querySelector('#nx-btn-out-android') as HTMLButtonElement).addEventListener('click', () => this.exportAndroid());
    this.renderEmpty();
  }

  clearLog(): void {
    this.logEl.replaceChildren();
  }

  log(text: string, cls?: string): void {
    const div = document.createElement('div');
    div.className = cls ? `nx-out-line ${cls}` : 'nx-out-line';
    div.textContent = text;
    this.logEl.appendChild(div);
  }

  renderBuild(result: BuildResult, target: 'web' | 'android'): void {
    const card = this.root.querySelector('#nx-output-build') as HTMLElement;
    card.replaceChildren();

    const head = document.createElement('div');
    head.className = 'nx-out-head';
    head.textContent = `${target} · "${result.site}" · ${result.pages} page(s) · ${result.files.length} files`;
    card.appendChild(head);

    const list = document.createElement('div');
    list.className = 'nx-out-files';
    for (const f of result.files) {
      const row = document.createElement('div');
      row.className = 'nx-out-file';
      const path = document.createElement('span');
      path.textContent = f.path;
      const size = document.createElement('span');
      size.className = 'nx-out-size';
      size.textContent = `${f.content.length} B`;
      row.appendChild(path);
      row.appendChild(size);
      list.appendChild(row);
    }
    card.appendChild(list);
  }

  renderEmpty(): void {
    const card = this.root.querySelector('#nx-output-build') as HTMLElement;
    card.replaceChildren();
    const empty = document.createElement('div');
    empty.className = 'nx-out-empty';
    empty.textContent = 'No build yet — run `build web` or `build android` (real target backends via kinn.wasm).';
    card.appendChild(empty);
  }

  private downloadWeb(): void {
    this.fire('nx-last-web-build');
  }
  private exportAndroid(): void {
    this.fire('nx-last-android-build');
  }

  /** Ask the host to act on the stored last build of a kind. */
  private fire(kind: string): void {
    this.root.dispatchEvent(new CustomEvent('nx-output-action', { detail: kind, bubbles: true }));
  }
}
