/**
 * KINN Studio NEXT — bootstrap and wiring.
 *
 * The experimental terminal-first workspace. Everything that exists in
 * the underlying system is wired for real:
 *   - editor, tabs, explorer  -> Vfs (persisted to IndexedDB)
 *   - terminal                -> TermEngine -> wasm compiler/VM backends
 *   - problems                -> real compiler diagnostics
 *   - output                  -> real build artifacts (web / android)
 *   - preview                 -> exact bytes produced by `build web`
 *
 * The classic Studio (index.html) is left untouched and stays reachable
 * from both directions.
 */

import { Vfs } from '../vfs';
import { detectMode } from '../diagnostics/checks';
import { backend, type BuildResult } from '../wasm/kinnwasm';
import { defaultStorage, parseProject } from '../storage/storage';
import { downloadZip } from '../build/preview';
import { zipFiles } from '../build/zip';

import { WorkspaceState } from './state';
import { loadSettings, saveSettings, type NextSettings } from './settings';
import { Layout } from './layout';
import type { PaletteCommand } from './palette';
import type { TermStatus } from './termcmd';
import { androidHost, wireClassicLink } from '../host';
import { Topbar } from './ui/topbar';
import { NextExplorer } from './ui/explorer';
import { EditorTabs } from './ui/editortabs';
import { CodeEditor } from './ui/codeeditor';
import { TerminalView } from './ui/terminal';
import { ProblemsPanel } from './ui/problems';
import { OutputPanel } from './ui/output';
import { StatusBar } from './ui/statusbar';
import { SettingsModal } from './ui/settings';
import { PaletteView } from './ui/paletteview';
import { PreviewOverlay } from './ui/preview';

import helloSample from '../../samples/hello.kin?raw';
import loopsSample from '../../samples/loops.kin?raw';
import brokenSample from '../../samples/broken.kin?raw';
import siteSample from '../../samples/site.kin?raw';

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el;
}

const SAMPLES: Array<[string, string]> = [
  ['hello.kin', helloSample],
  ['loops.kin', loopsSample],
  ['broken.kin', brokenSample],
  ['site.kin', siteSample],
];

const storage = defaultStorage();
const vfs = new Vfs('samples', SAMPLES, storage);
const state = new WorkspaceState();
let settings: NextSettings = loadSettings(localStorage);
const isMobile = (): boolean => window.matchMedia('(max-width: 880px)').matches;

const root = $('nx-root');

// -- panels -----------------------------------------------------------------

const topbar = new Topbar($('nx-root'), {
  onRun: () => runActive(),
  onBuild: () => buildWeb(),
  onPalette: () => palette.showCommands(commands()),
  onSettings: () => settingsModal.open(),
});

const explorer = new NextExplorer(vfs, state, $('nx-explorer'), {
  onOpen: (name) => openFileInEditor(name),
  onNewFile: () => newFile(),
  onRename: (oldName, newName) => {
    if (vfs.rename(oldName, newName)) {
      toast(`renamed ${oldName} -> ${newName}`);
    } else {
      toast(`cannot rename to '${newName}' (must match name.kin and not exist)`);
    }
  },
  onDelete: (name) => {
    if (vfs.remove(name)) {
      toast(`deleted ${name}`);
      openFileInEditor(vfs.activeName);
    }
  },
});

const editorTabs = new EditorTabs(state, $('nx-tabs'), {
  onActivate: (name) => openFileInEditor(name),
  onClose: (name) => state.closeTab(name),
});
void editorTabs; // the tabs stay alive through their state subscription

const editor = new CodeEditor(
  {
    wrap: $('nx-editor-wrap'),
    scroll: $('nx-editor-scroll'),
    sizer: $('nx-editor-sizer'),
    highlight: $('nx-highlight'),
    gutter: $('nx-gutter'),
    activeLine: $('nx-active-line'),
    input: $('nx-editor-input') as HTMLTextAreaElement,
  },
  {
    onEdit: (text) => onEdited(text),
    onCursor: (line, col, lines) => statusbar.setCursor(line, col, lines),
  },
);

const statusbar = new StatusBar($('nx-statusbar'), {
  onToggleExplorer: () => void state.toggleExplorer(),
  onToggleBottom: () => toggleBottomPanel(),
});

const problems = new ProblemsPanel($('nx-bottom'), { // badges live in the bottom tab row
  onJump: (file, line) => jumpTo(file, line),
});

const output = new OutputPanel($('nx-output'), {
  onPreviewWeb: () => previewWeb(),
});
$('nx-output').addEventListener('nx-output-action', (e) => {
  const kind = (e as CustomEvent<string>).detail;
  if (kind === 'nx-last-web-build' && lastWeb) {
    downloadZip(lastWeb.files, lastWeb.site.toLowerCase() || 'kinn-site', zipFiles);
    output.log('downloaded web dist as .zip', 'ok');
  }
  if (kind === 'nx-last-android-build' && lastAndroid) {
    downloadZip(lastAndroid.files, `${lastAndroid.site.toLowerCase() || 'kinn-app'}-android-project`, zipFiles);
    output.log('exported Android Gradle project as .zip (build the APK with Android Studio, gradle, or CI)', 'ok');
  }
});

const preview = new PreviewOverlay($('nx-preview'));

const settingsModal = new SettingsModal(
  $('nx-settings'),
  settings,
  (next) => {
    settings = next;
    saveSettings(localStorage, settings);
    applySettings();
  },
  () => settingsModal.close(),
);

const palette = new PaletteView($('nx-palette'), {
  onPickFile: (name) => openFileInEditor(name),
  onClose: () => editor.focus(),
});

// -- terminal ----------------------------------------------------------------

let lastWeb: BuildResult | null = null;
let lastAndroid: BuildResult | null = null;

function setMobileView(view: 'files' | 'editor' | 'term' | 'out'): void {
  Layout.setMobileView(root, view);
  for (const b of Array.from(document.querySelectorAll<HTMLButtonElement>('#nx-mobilenav button'))) {
    b.classList.toggle('active', b.dataset.mview === view);
  }
}

function terminalFocus(kind: 'term' | 'out'): void {
  showBottomTab(kind === 'out' ? 'output' : 'terminal');
  if (state.snapshot().layout === 'editor') state.setLayout('split');
  if (isMobile()) setMobileView(kind === 'out' ? 'out' : 'term');
}

const terminal = new TerminalView(vfs, {
  onOpenFile: (name) => openFileInEditor(name),
  onProblems: (list, file) => problems.render(list, file),
  onImportRequest: () => importPicker.click(),
  onBuildResult: (result, target) => onBuildResult(result, target),
  onPreview: (files) => {
    if (lastWeb) preview.open(files, lastWeb.site);
  },
  onStatus: (s) => onTermStatus(s),
}, $('nx-bottom')); // root spans the whole bottom panel (tabs + sessions + body)
terminal.setFontSize(settings.terminalFont);

function onTermStatus(s: TermStatus): void {
  if (s.kind === 'running') {
    topbar.setBuildState('busy', s.label);
    statusbar.setRuntime(s.label, '');
  } else if (s.kind === 'ok') {
    topbar.setBuildState('ok', s.label);
    statusbar.setRuntime(s.label, 'ok');
    output.log(`ok — ${s.label}`, 'ok');
  } else if (s.kind === 'fail') {
    topbar.setBuildState('fail', s.label);
    statusbar.setRuntime(s.label, 'err');
    output.log(`fail — ${s.label}`, 'err');
  }
}

// -- layout ---------------------------------------------------------------------

const layout = new Layout(state, root, localStorage, () => {});
void layout; // keeps applying + persisting through its own subscriptions

$('nx-btn-term-clear').addEventListener('click', () => terminal.clearActive());

for (const btn of Array.from(document.querySelectorAll<HTMLButtonElement>('.nx-layout-btn'))) {
  btn.addEventListener('click', () => state.setLayout(btn.dataset.mode as 'editor' | 'split' | 'terminal'));
}

// bottom panel tabs (terminal | problems | output)
type BottomTab = 'terminal' | 'problems' | 'output';
function showBottomTab(tab: BottomTab): void {
  for (const el of Array.from(document.querySelectorAll<HTMLElement>('.nx-btab'))) {
    el.classList.toggle('active', el.dataset.btab === tab);
  }
  ($('nx-terminal') as HTMLElement).hidden = tab !== 'terminal';
  ($('nx-problems') as HTMLElement).hidden = tab !== 'problems';
  ($('nx-output') as HTMLElement).hidden = tab !== 'output';
}
for (const el of Array.from(document.querySelectorAll<HTMLElement>('.nx-btab'))) {
  el.addEventListener('click', () => showBottomTab(el.dataset.btab as BottomTab));
}

function toggleBottomPanel(): void {
  const cur = state.snapshot().layout;
  state.setLayout(cur === 'split' ? 'editor' : 'split');
}

// mobile navigation
for (const btn of Array.from(document.querySelectorAll<HTMLButtonElement>('#nx-mobilenav button'))) {
  btn.addEventListener('click', () => {
    const view = btn.dataset.mview!;
    Layout.setMobileView(root, view);
    for (const b of Array.from(document.querySelectorAll('#nx-mobilenav button'))) {
      b.classList.toggle('active', b === btn);
    }
    if (view === 'term' || view === 'editor') {
      if (view === 'term') showBottomTab('terminal');
      if (view === 'editor') editor.focus();
    }
    if (view === 'out') showBottomTab('output');
  });
}
setMobileView('editor');
Layout.trackVisualViewport(root);

// -- editor flow -----------------------------------------------------------------

let checkTimer = 0;
let saveTimer = 0;

function onEdited(text: string): void {
  const f = vfs.active;
  if (!f) return;
  vfs.save(f.name, text);
  state.markModified(f.name);
  topbar.setFile(f.name);
  $('nx-mode-badge').textContent = `MODE ${detectMode(text)}`;
  statusbar.setStorage('auto-save pending…');
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => statusbar.setStorage('saved to this browser'), 900);

  if (settings.autoCheck && backend.available) {
    window.clearTimeout(checkTimer);
    checkTimer = window.setTimeout(() => {
      const cur = vfs.active;
      if (!cur) return;
      // Re-read the editor at fire time, not at schedule time: the
      // check compiles what is on screen when it runs.
      syncActiveText();
      const result = backend.check(cur.name, vfs.projectFiles());
      problems.render(result.diagnostics, cur.name);
    }, 900);
  }
}

function openFileInEditor(name: string): void {
  const f = vfs.open(name);
  if (!f) return;
  vfs.setActive(name);
  state.openFile(name);
  editor.setText(f.content);
  topbar.setFile(name);
  topbar.setProject(vfs.projectName);
  $('nx-mode-badge').textContent = `MODE ${detectMode(f.content)}`;
  statusbar.setFile(name);
  if (isMobile()) setMobileView('editor');
}

function jumpTo(file: string, line: number): void {
  openFileInEditor(file);
  editor.gotoLine(line);
  if (isMobile()) setMobileView('editor');
}

function newFile(): void {
  const name = window.prompt('new file name (e.g. mymod.kin):', 'untitled.kin');
  if (!name) return;
  if (vfs.create(name)) {
    openFileInEditor(name);
    toast(`created ${name}`);
  } else {
    toast(`cannot create '${name}' (must match name.kin and not exist)`);
  }
}

function applySettings(): void {
  editor.setFontSize(settings.editorFont);
  editor.setIndentSize(settings.indentSize);
  terminal.setFontSize(settings.terminalFont);
}

// -- actions (real) --------------------------------------------------------------

/**
 * Requirement: the compiler always sees exactly what the user sees.
 * Called right before run/check/build/export — reads the CURRENT
 * editor value (the single source of truth) and mirrors it into the
 * vfs, so nothing stale, cached or debounced can reach the compiler.
 */
function syncActiveText(): void {
  const name = vfs.activeName;
  if (!name) return;
  vfs.save(name, editor.getText());
}

function runActive(): void {
  const name = vfs.activeName;
  if (!name) {
    toast('no file open');
    return;
  }
  syncActiveText();
  terminalFocus('term');
  terminal.exec(`run ${name}`);
}

function checkActive(): void {
  const name = vfs.activeName;
  if (!name) return;
  syncActiveText();
  terminalFocus('term');
  terminal.exec(`check ${name}`);
}

function buildWeb(): void {
  const name = vfs.activeName;
  if (!name) return;
  syncActiveText();
  terminalFocus('term');
  terminal.exec(`build web`);
}

function buildAndroidZip(): void {
  const name = vfs.activeName;
  if (!name) return;
  syncActiveText();
  if (!backend.available) {
    toast('compiler backend not built — build web works with kinn.wasm only');
    return;
  }
  const result = backend.buildAndroid(name, vfs.projectFiles());
  if (!result.ok) {
    problems.render(result.diagnostics, name);
    terminalFocus('out');
    toast('android export failed — diagnostics in PROBLEMS');
    return;
  }
  lastAndroid = result;
  output.renderBuild(result, 'android');
  downloadZip(result.files, `${result.site.toLowerCase() || 'kinn-app'}-android-project`, zipFiles);
  output.log(`exported Android Gradle project (${result.files.length} files) as .zip`, 'ok');
  toast('Android Gradle project exported — build the APK with Android Studio / gradle / CI');
}

function previewWeb(): void {
  if (!lastWeb) {
    toast("no web build yet — run 'build web' first");
    return;
  }
  preview.open(lastWeb.files, lastWeb.site);
}

function saveAll(): void {
  void vfs.markSaved().then(() => {
    state.markAllSaved();
    statusbar.setStorage('saved to this browser');
    toast('project saved to this browser');
  });
}

function onBuildResult(result: BuildResult, target: 'web' | 'android'): void {
  if (target === 'web') {
    lastWeb = result.ok ? result : null;
    if (result.ok) {
      output.renderBuild(result, 'web');
      output.log(`web build ok — ${result.files.length} files, site "${result.site}"`, 'ok');
    }
  } else {
    lastAndroid = result.ok ? result : null;
    if (result.ok) output.renderBuild(result, 'android');
  }
}

// -- command palette -----------------------------------------------------------

function commands(): PaletteCommand[] {
  const list: PaletteCommand[] = [
    { id: 'file.open', title: 'Go to file…', group: 'file', hint: '@', run: () => palette.showFiles(vfs.list().map((f) => f.name)) },
    { id: 'file.new', title: 'New file…', group: 'file', run: () => newFile() },
    { id: 'file.save', title: 'Save project (Ctrl+S)', group: 'file', run: () => saveAll() },
    { id: 'file.rename', title: 'Rename active file…', group: 'file', run: () => {
      const cur = vfs.activeName;
      if (!cur) return;
      const next = window.prompt(`rename ${cur} to:`, cur);
      if (next && next !== cur) {
        if (vfs.rename(cur, next)) openFileInEditor(next);
        else toast('rename failed (invalid or exists)');
      }
    } },
    { id: 'file.delete', title: 'Delete active file…', group: 'file', run: () => {
      const cur = vfs.activeName;
      if (!cur) return;
      if (window.confirm(`delete ${cur}?`)) {
        vfs.remove(cur);
        openFileInEditor(vfs.activeName);
      }
    } },
    { id: 'project.import', title: 'Import project / file…', group: 'project', run: () => importPicker.click() },
    { id: 'project.export', title: 'Export project (.kinnproj)', group: 'project', run: () => terminal.exec('export') },
    { id: 'build.run', title: 'Run active file (real VM)', group: 'build', run: () => runActive() },
    { id: 'build.check', title: 'Check active file (real compiler)', group: 'build', hint: 'Ctrl+Enter', run: () => checkActive() },
    { id: 'build.web', title: 'Build web (real backend)', group: 'build', run: () => buildWeb() },
    { id: 'build.android', title: 'Build android (Gradle project export)', group: 'build', run: () => buildAndroidZip() },
    { id: 'build.preview', title: 'Preview web build', group: 'build', run: () => previewWeb() },
    { id: 'build.download', title: 'Download web build (.zip)', group: 'build', run: () => {
      if (lastWeb) {
        downloadZip(lastWeb.files, lastWeb.site.toLowerCase() || 'kinn-site', zipFiles);
        toast('web dist downloaded as .zip');
      } else {
        toast("no web build yet — run 'build web' first");
      }
    } },
    { id: 'view.editor', title: 'Focus editor', group: 'view', hint: 'Ctrl+1', run: () => state.setLayout('editor') },
    { id: 'view.split', title: 'Split editor and terminal', group: 'view', hint: 'Ctrl+2', run: () => state.setLayout('split') },
    { id: 'view.terminal', title: 'Focus terminal', group: 'view', hint: 'Ctrl+3', run: () => state.setLayout('terminal') },
    { id: 'view.explorer', title: 'Toggle explorer', group: 'view', hint: 'Ctrl+B', run: () => void state.toggleExplorer() },
    { id: 'view.panel', title: 'Toggle terminal panel', group: 'view', hint: 'Ctrl+`', run: () => toggleBottomPanel() },
    { id: 'view.settings', title: 'Open settings', group: 'view', run: () => settingsModal.open() },
    { id: 'term.new', title: 'New terminal session', group: 'terminal', run: () => {
      if (terminal.newSession()) terminalFocus('term');
      else toast('session limit reached (4)');
    } },
    { id: 'term.clear', title: 'Clear terminal', group: 'terminal', run: () => terminal.clearActive() },
    { id: 'term.help', title: 'Terminal: list commands', group: 'terminal', run: () => {
      terminalFocus('term');
      terminal.exec('help');
    } },
    { id: 'term.syntax', title: 'Terminal: Kin cheat sheet', group: 'terminal', run: () => {
      terminalFocus('term');
      terminal.exec('syntax');
    } },
  ];
  // Inside the Android host: a real extra command — the classic fallback
  // there is the native Studio activity, opened through the bridge.
  const host = androidHost();
  if (host) {
    list.push({
      id: 'app.classic',
      title: `Open classic Studio (native app · bridge ${host.hostVersion()})`,
      group: 'app',
      run: () => host.openClassicStudio(),
    });
  }
  return list;
}

// -- import / export ------------------------------------------------------------

const importPicker = document.createElement('input');
importPicker.type = 'file';
importPicker.accept = '.kinnproj,.kin,application/json';
importPicker.style.display = 'none';
document.body.appendChild(importPicker);

importPicker.addEventListener('change', async () => {
  const file = importPicker.files?.[0];
  importPicker.value = '';
  if (!file) return;
  try {
    const text = await file.text();
    if (file.name.endsWith('.kin')) {
      if (vfs.importFile(file.name, text)) {
        openFileInEditor(file.name);
        toast(`imported ${file.name}`);
      } else {
        toast(`cannot import '${file.name}' (must match name.kin)`);
      }
    } else {
      const project = parseProject(text);
      vfs.loadProject(project);
      openFileInEditor(vfs.activeName);
      topbar.setProject(vfs.projectName);
      toast(`imported project '${project.name}' (${project.files.length} files)`);
    }
  } catch (e) {
    toast(`import failed: ${(e as Error).message}`);
  }
});

// -- find bar ---------------------------------------------------------------------

const findInput = document.querySelector('#nx-findbar input') as HTMLInputElement;
findInput.addEventListener('input', () => editor.updateFind());
findInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    editor.findNext(e.shiftKey);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    editor.closeFind();
  }
});
$('nx-find-prev').addEventListener('click', () => editor.findNext(true));
$('nx-find-next').addEventListener('click', () => editor.findNext(false));
$('nx-find-close').addEventListener('click', () => editor.closeFind());

// -- keyboard -----------------------------------------------------------------------

document.addEventListener('keydown', (e) => {
  const mod = e.ctrlKey || e.metaKey;
  const anyOverlay = settingsModal.isOpen || palette.visible || preview.isOpen;
  if (e.key === 'Escape') {
    if (preview.isOpen) preview.close();
    else if (settingsModal.isOpen) settingsModal.close();
    else if (palette.visible) { palette.close(); editor.focus(); }
    return;
  }
  if (mod && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    palette.showCommands(commands());
    return;
  }
  if (mod && e.key.toLowerCase() === 's') {
    e.preventDefault();
    saveAll();
    return;
  }
  if (mod && e.key === 'Enter') {
    e.preventDefault();
    checkActive();
    return;
  }
  if (mod && e.key === '`') {
    e.preventDefault();
    toggleBottomPanel();
    return;
  }
  if (mod && e.key.toLowerCase() === 'b') {
    e.preventDefault();
    void state.toggleExplorer();
    return;
  }
  if (mod && e.key === '1') {
    e.preventDefault();
    state.setLayout('editor');
    return;
  }
  if (mod && e.key === '2') {
    e.preventDefault();
    state.setLayout('split');
    return;
  }
  if (mod && e.key === '3') {
    e.preventDefault();
    state.setLayout('terminal');
    return;
  }
  if (anyOverlay) return;
  if (mod && e.key.toLowerCase() === 'f') {
    // handled inside the editor when focused; global fallback opens find
    const tag = (document.activeElement?.tagName ?? '').toLowerCase();
    if (tag !== 'input' && tag !== 'textarea') {
      e.preventDefault();
      editor.openFind();
    }
  }
});

// -- network indicator ------------------------------------------------------------

function updateNet(): void {
  statusbar.setNet(navigator.onLine);
}
window.addEventListener('online', updateNet);
window.addEventListener('offline', updateNet);
updateNet();

// -- android host bridge (inert in the browser) -------------------------------------

// The top-bar "classic" anchor keeps its href (web classic) for plain
// browsers; inside the Android host it opens the *native* classic app.
{
  const classicLink = document.querySelector<HTMLAnchorElement>('.nx-classic-link');
  if (classicLink) wireClassicLink(classicLink);
}

// -- toast ---------------------------------------------------------------------------

let toastTimer = 0;
function toast(message: string): void {
  const el = $('nx-toast');
  el.textContent = message;
  el.classList.add('show');
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => el.classList.remove('show'), 2600);
}

// -- boot ------------------------------------------------------------------------

async function boot(): Promise<void> {
  // 1. restore the persisted project (first visit: bundled samples)
  try {
    const stored = await storage.loadProject();
    if (stored) vfs.loadProject(stored);
  } catch {
    // storage unavailable (private mode): stay on samples
  }

  topbar.setProject(vfs.projectName);
  explorer.render();

  // 2. load the compiler backend; degrade honestly when missing
  const ok = await backend.load();
  if (ok) {
    const v = backend.version();
    topbar.setBackend(true, v);
    statusbar.setCompiler(`kinn ${v} · wasm`);
    explorer.setFacts(`kinn ${v} (wasm)`, 'IndexedDB · auto-save');
  } else {
    topbar.setBackend(false, null);
    statusbar.setCompiler('compiler not built', 'warn');
    statusbar.setRuntime('editor checks only', 'warn');
    explorer.setFacts('not built (wasm)', 'IndexedDB · auto-save');
    statusbar.setStorage('auto-save on');
  }

  applySettings();
  openFileInEditor(vfs.activeName || 'hello.kin');
  showBottomTab('terminal');
  state.setLayout('split');
}

void boot();
