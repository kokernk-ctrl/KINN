/**
 * Preview overlay: renders the real multi-page output of `build web`
 * in a sandboxed iframe, with page switching and the .zip download of
 * the generated dist. Nothing here is simulated — the pages shown are
 * the exact bytes the compiler produced.
 */

import type { OutputFile } from '../../wasm/kinnwasm';
import { buildPreviewHtml, downloadZip, htmlPages } from '../../build/preview';
import { zipFiles } from '../../build/zip';

export class PreviewOverlay {
  private backdrop: HTMLElement;
  private select: HTMLSelectElement;
  private frame: HTMLIFrameElement;
  private title: HTMLElement;
  private files: OutputFile[] = [];
  private site = 'kinn-site';

  constructor(backdrop: HTMLElement) {
    this.backdrop = backdrop;
    this.select = backdrop.querySelector('#nx-preview-page') as HTMLSelectElement;
    this.frame = backdrop.querySelector('#nx-preview-frame') as HTMLIFrameElement;
    this.title = backdrop.querySelector('#nx-preview-title') as HTMLElement;
    (backdrop.querySelector('#nx-preview-close') as HTMLButtonElement).addEventListener('click', () => this.close());
    (backdrop.querySelector('#nx-preview-download') as HTMLButtonElement).addEventListener('click', () => {
      if (this.files.length === 0) return;
      downloadZip(this.files, this.site.toLowerCase() || 'kinn-site', zipFiles);
    });
    this.select.addEventListener('change', () => this.show());
    // Built pages post a message when an internal link/button is clicked;
    // switch pages inside the overlay instead of fetching relative URLs.
    window.addEventListener('message', (e: MessageEvent) => {
      const msg = e.data as { kinnPreviewNav?: boolean; page?: string };
      if (!msg || !msg.kinnPreviewNav || !msg.page || this.files.length === 0) return;
      if (this.select.querySelector(`option[value="${msg.page}"]`)) {
        this.select.value = msg.page;
        this.show();
      }
    });
  }

  open(files: OutputFile[], site: string): void {
    this.files = files;
    this.site = site || 'kinn-site';
    this.title.textContent = `preview — ${this.site}`;
    this.select.replaceChildren();
    for (const page of htmlPages(files)) {
      const opt = document.createElement('option');
      opt.value = page.path;
      opt.textContent = page.path;
      this.select.appendChild(opt);
    }
    this.backdrop.hidden = false;
    this.show();
  }

  close(): void {
    this.backdrop.hidden = true;
  }

  get isOpen(): boolean {
    return !this.backdrop.hidden;
  }

  private show(): void {
    if (this.files.length === 0) return;
    const path = this.select.value || htmlPages(this.files)[0]?.path;
    if (!path) return;
    this.frame.srcdoc = buildPreviewHtml(this.files, path);
  }
}
