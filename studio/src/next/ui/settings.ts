/**
 * Settings modal. Every setting is real and applied immediately:
 * editor font size, terminal font size, indent width, and the
 * auto-check toggle (runs the real wasm compiler on edit, debounced).
 * Also carries the honest boundary notes and the way back to the
 * classic Studio.
 */

import { DEFAULT_SETTINGS, EDITOR_FONT_SIZES, INDENT_SIZES, TERMINAL_FONT_SIZES, type NextSettings } from '../settings';

type SettingKey = 'editorFont' | 'terminalFont' | 'indentSize' | 'autoCheck';

function optionRow(
  label: string,
  values: readonly (number | boolean)[],
  format: (v: number | boolean) => string,
  current: number | boolean,
  onPick: (v: number | boolean) => void,
): HTMLElement {
  const row = document.createElement('div');
  row.className = 'nx-set-row';
  const lab = document.createElement('span');
  lab.className = 'nx-set-label';
  lab.textContent = label;
  const seg = document.createElement('div');
  seg.className = 'nx-set-seg';
  seg.setAttribute('role', 'radiogroup');
  for (const v of values) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'nx-set-opt' + (v === current ? ' active' : '');
    btn.textContent = format(v);
    btn.setAttribute('aria-pressed', String(v === current));
    btn.addEventListener('click', () => onPick(v));
    seg.appendChild(btn);
  }
  row.appendChild(lab);
  row.appendChild(seg);
  return row;
}

export class SettingsModal {
  private backdrop: HTMLElement;
  private body: HTMLElement;
  private settings: NextSettings;

  constructor(
    backdrop: HTMLElement,
    settings: NextSettings,
    private onApply: (s: NextSettings) => void,
    private onClose: () => void,
  ) {
    this.backdrop = backdrop;
    this.settings = { ...settings };
    this.body = backdrop.querySelector('#nx-settings-body') as HTMLElement;
    (backdrop.querySelector('#nx-settings-close') as HTMLButtonElement).addEventListener('click', () => this.onClose());
    backdrop.addEventListener('click', (e) => {
      if (e.target === backdrop) this.onClose();
    });
    this.render();
  }

  update(s: NextSettings): void {
    this.settings = { ...s };
    this.render();
  }

  open(): void {
    this.backdrop.hidden = false;
  }

  close(): void {
    this.backdrop.hidden = true;
  }

  get isOpen(): boolean {
    return !this.backdrop.hidden;
  }

  private render(): void {
    this.body.replaceChildren();
    const pick = (key: SettingKey, v: number | boolean): void => {
      const next = { ...this.settings, [key]: v } as NextSettings;
      this.settings = next;
      this.onApply(next);
      this.render();
    };

    this.body.appendChild(optionRow(
      'editor font', EDITOR_FONT_SIZES, (v) => `${v}px`, this.settings.editorFont,
      (v) => pick('editorFont', v),
    ));
    this.body.appendChild(optionRow(
      'terminal font', TERMINAL_FONT_SIZES, (v) => `${v}px`, this.settings.terminalFont,
      (v) => pick('terminalFont', v),
    ));
    this.body.appendChild(optionRow(
      'indent width', INDENT_SIZES, (v) => `${v} sp`, this.settings.indentSize,
      (v) => pick('indentSize', v),
    ));
    this.body.appendChild(optionRow(
      'auto-check on edit', [true, false], (v) => (v ? 'on' : 'off'), this.settings.autoCheck,
      (v) => pick('autoCheck', v),
    ));

    const note = document.createElement('div');
    note.className = 'nx-set-note';
    note.innerHTML =
      'auto-check runs the <b>real compiler</b> (kinn.wasm) on every edit, debounced. ' +
      'Everything in Studio Next runs locally and offline; there are no accounts and no servers.';
    this.body.appendChild(note);

    const about = document.createElement('div');
    about.className = 'nx-set-about';
    const classic = document.createElement('a');
    classic.href = './index.html';
    classic.textContent = '← return to classic Studio';
    about.appendChild(classic);
    const ver = document.createElement('span');
    ver.textContent = ` · defaults restored on data reset · v${DEFAULT_SETTINGS.editorFont}px editor`;
    about.appendChild(ver);
    this.body.appendChild(about);
  }
}
