/**
 * Terminal view: multiple real sessions, each with its own scrollback,
 * input, command history and the shared TermEngine driving actual
 * compiler/VM commands. Sessions are switchable tabs with live status
 * dots; the active session is the only one that renders.
 */

import { TermEngine, type LineClass, type TermSink, type TermStatus } from '../termcmd';
import type { Vfs } from '../../vfs';
import type { Problem } from '../../diagnostics/checks';
import type { BuildResult, OutputFile } from '../../wasm/kinnwasm';

export interface TerminalHost {
  onOpenFile(name: string): void;
  onProblems(list: Problem[], file: string): void;
  onImportRequest(): void;
  onBuildResult(result: BuildResult, target: 'web' | 'android'): void;
  onPreview(files: OutputFile[]): void;
  onStatus(s: TermStatus): void;
}

interface Session {
  id: number;
  scroll: HTMLElement;
  output: HTMLElement;
  input: HTMLInputElement;
  engine: TermEngine;
  historyIdx: number;
  status: TermStatus;
  statusEl: HTMLElement;
}

const MAX_SESSIONS = 4;

export class TerminalView {
  private sessions: Session[] = [];
  private activeId = 0;
  private nextId = 1;
  private sessionsBar: HTMLElement;
  private body: HTMLElement;

  constructor(
    private vfs: Vfs,
    private host: TerminalHost,
    root: HTMLElement,
  ) {
    this.sessionsBar = root.querySelector('#nx-term-sessions') as HTMLElement;
    this.body = root.querySelector('#nx-term-body') as HTMLElement;
    (root.querySelector('#nx-btn-term-new') as HTMLButtonElement).addEventListener('click', () => this.newSession());
    this.newSession();
  }

  get active(): Session | undefined {
    return this.sessions.find((s) => s.id === this.activeId);
  }

  newSession(): Session | null {
    if (this.sessions.length >= MAX_SESSIONS) return null;

    const id = this.nextId++;
    const scroll = document.createElement('div');
    scroll.className = 'nx-term-scroll';
    scroll.dataset.sid = String(id);
    const output = document.createElement('div');
    output.className = 'nx-term-output';
    scroll.appendChild(output);
    const row = document.createElement('div');
    row.className = 'nx-term-input-row';
    const prompt = document.createElement('span');
    prompt.className = 'nx-term-prompt';
    prompt.textContent = 'kinn>';
    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'nx-term-input';
    input.autocomplete = 'off';
    input.spellcheck = false;
    input.setAttribute('aria-label', `Terminal session ${id} command`);
    row.appendChild(prompt);
    row.appendChild(input);
    scroll.appendChild(row);
    this.body.appendChild(scroll);

    const statusEl = document.createElement('span');
    statusEl.className = 'nx-term-session-status';
    statusEl.dataset.state = 'idle';

    const session: Session = {
      id,
      scroll,
      output,
      input,
      engine: null as unknown as TermEngine,
      historyIdx: 0,
      status: { kind: 'idle' },
      statusEl,
    };

    const sink: TermSink = {
      line: (text, cls) => this.appendLine(session, text, cls, cls === undefined),
      block: (text, cls) => {
        for (const l of text.replace(/\n$/, '').split('\n')) this.appendLine(session, l, cls, false);
      },
      clear: () => output.replaceChildren(),
      status: (s) => {
        session.status = s;
        this.renderStatus(session);
        this.host.onStatus(s);
      },
      openFile: (name) => this.host.onOpenFile(name),
      problems: (list, file) => this.host.onProblems(list, file),
      importRequest: () => this.host.onImportRequest(),
      buildResult: (result, target) => this.host.onBuildResult(result, target),
      preview: (files) => this.host.onPreview(files),
    };

    session.engine = new TermEngine({ vfs: this.vfs, sink });
    session.historyIdx = session.engine.history.length;

    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        const cmd = input.value;
        input.value = '';
        session.engine.exec(cmd);
        session.historyIdx = session.engine.history.length;
        this.scrollToEnd(session);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const h = session.engine.history;
        if (h.length > 0) {
          session.historyIdx = Math.max(0, session.historyIdx - 1);
          input.value = h[session.historyIdx] ?? '';
        }
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        const h = session.engine.history;
        session.historyIdx = Math.min(h.length, session.historyIdx + 1);
        input.value = h[session.historyIdx] ?? '';
      }
    });

    this.sessions.push(session);
    this.renderSessionsBar();
    this.setActive(id);
    session.engine.banner();
    return session;
  }

  closeSession(id: number): void {
    const idx = this.sessions.findIndex((s) => s.id === id);
    if (idx === -1 || this.sessions.length === 1) return; // never zero sessions
    const gone = this.sessions[idx];
    gone.scroll.remove();
    this.sessions.splice(idx, 1);
    if (this.activeId === id) {
      this.setActive(this.sessions[Math.max(0, idx - 1)].id);
    }
    this.renderSessionsBar();
  }

  setActive(id: number): void {
    this.activeId = id;
    for (const s of this.sessions) {
      s.scroll.hidden = s.id !== id;
    }
    this.renderSessionsBar();
    this.focusInput();
  }

  focusInput(): void {
    this.active?.input.focus();
  }

  /** Run a command in the active session (top bar buttons, palette). */
  exec(cmd: string): void {
    const s = this.active;
    if (!s) return;
    s.engine.exec(cmd);
    s.historyIdx = s.engine.history.length;
    this.scrollToEnd(s);
  }

  print(text: string, cls?: LineClass): void {
    const s = this.active;
    if (!s) return;
    s.engine.print(text, cls);
    this.scrollToEnd(s);
  }

  clearActive(): void {
    const s = this.active;
    if (s) s.output.replaceChildren();
  }

  setFontSize(px: number): void {
    this.body.style.setProperty('--nx-term-font', `${px}px`);
  }

  private appendLine(session: Session, text: string, cls: LineClass | undefined, isEcho: boolean): void {
    const div = document.createElement('div');
    div.className = cls ?? (isEcho ? 'cmd' : 'out');
    div.textContent = text;
    session.output.appendChild(div);
  }

  private scrollToEnd(session: Session): void {
    session.scroll.scrollTop = session.scroll.scrollHeight;
  }

  private renderSessionsBar(): void {
    this.sessionsBar.replaceChildren();
    for (const s of this.sessions) {
      const tab = document.createElement('button');
      tab.type = 'button';
      tab.className = 'nx-term-session' + (s.id === this.activeId ? ' active' : '');
      tab.appendChild(s.statusEl);
      const label = document.createElement('span');
      label.textContent = `term ${s.id}`;
      tab.appendChild(label);
      if (this.sessions.length > 1) {
        const close = document.createElement('span');
        close.className = 'nx-term-session-close';
        close.textContent = '✕';
        close.title = `close session ${s.id}`;
        close.addEventListener('click', (e) => {
          e.stopPropagation();
          this.closeSession(s.id);
        });
        tab.appendChild(close);
      }
      tab.addEventListener('click', () => this.setActive(s.id));
      this.sessionsBar.appendChild(tab);
    }
  }

  private renderStatus(s: Session): void {
    const st = s.status;
    const el = s.statusEl;
    el.dataset.state = st.kind;
    el.title = st.kind === 'running' ? `running: ${st.label}`
      : st.kind === 'ok' ? `ok: ${st.label}`
      : st.kind === 'fail' ? `failed: ${st.label}`
      : 'idle';
  }
}
