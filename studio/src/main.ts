/**
 * KINN Studio bootstrap: wires the virtual file system (persisted to
 * IndexedDB), explorer, editor, terminal, problems panel, the WebAssembly
 * compiler backend and responsive tabs together.
 */

import { Vfs } from './vfs';
import { Explorer } from './ui/explorer';
import { Editor } from './ui/editor';
import { Terminal } from './ui/terminal';
import { ProblemsPanel } from './ui/problems';
import { Tabs } from './ui/tabs';
import { detectMode, quickCheck } from './diagnostics/checks';
import { backend, type BuildResult, type OutputFile } from './wasm/kinnwasm';
import { defaultStorage, parseProject } from './storage/storage';
import { buildPreviewHtml, downloadZip, htmlPages } from './build/preview';
import { zipFiles } from './build/zip';

import helloSample from '../samples/hello.kin?raw';
import loopsSample from '../samples/loops.kin?raw';
import brokenSample from '../samples/broken.kin?raw';
import siteSample from '../samples/site.kin?raw';

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

// -- editor ---------------------------------------------------------------

const editor = new Editor(
  $('editor-input') as HTMLTextAreaElement,
  $('highlight'),
  $('gutter'),
  $('status-pos'),
  $('status-count'),
  {
    onEdit: (text: string) => {
      const f = vfs.active;
      if (f) vfs.save(f.name, text);
      updateModeBadge();
      setStatusSaved();
    },
  },
);

function loadActiveFile(): void {
  const f = vfs.active;
  if (!f) return;
  editor.setText(f.content);
  $('active-file').textContent = f.name;
  updateModeBadge();
  runChecks();
}

function updateModeBadge(): void {
  const f = vfs.active;
  if (!f) return;
  $('mode-badge').textContent = `MODE ${detectMode(f.content)}`;
}

function setStatusSaved(): void {
  const el = $('status-saved');
  el.textContent = 'saved to this browser';
  el.classList.remove('dirty');
}

// -- problems ---------------------------------------------------------------

const problems = new ProblemsPanel($('problems-list'), $('problems-count'), (line) => {
  editor.gotoLine(line);
});

function runChecks(): void {
  const f = vfs.active;
  if (!f) return;
  if (backend.available) {
    // Real compiler diagnostics (same engine as the CLI).
    const result = backend.check(f.name, vfs.projectFiles());
    problems.render(result.diagnostics, f.name);
  } else {
    problems.render(quickCheck(f.content), f.name);
  }
}

// -- terminal ---------------------------------------------------------------

const terminal = new Terminal(
  $('terminal-output'),
  $('terminal-input') as HTMLInputElement,
  {
    vfs,
    onOpenFile: () => {
      loadActiveFile();
      tabs.select('editor');
    },
    onProblems: (list, name) => problems.render(list, name),
    onImportRequest: () => importPicker.click(),
    onBuildResult: (result, target) => onBuildResult(result, target),
    onPreview: (files) => showPreview(files),
  },
);

// -- build / preview / export -------------------------------------------------

let lastWebBuild: BuildResult | null = null;

function onBuildResult(result: BuildResult, target: 'web' | 'android'): void {
  if (target === 'web' && result.ok) {
    lastWebBuild = result;
    const select = $('preview-page') as HTMLSelectElement;
    select.replaceChildren();
    for (const page of htmlPages(result.files)) {
      const opt = document.createElement('option');
      opt.value = page.path;
      opt.textContent = page.path;
      select.appendChild(opt);
    }
    ($('btn-download-web') as HTMLButtonElement).disabled = false;
    showPreview(result.files);
  }
  if (target === 'android' && result.ok) {
    ($('btn-export-android') as HTMLButtonElement).disabled = false;
  }
}

function showPreview(files: OutputFile[]): void {
  const select = $('preview-page') as HTMLSelectElement;
  const path = select.value || htmlPages(files)[0]?.path;
  if (!path) return;
  ($('preview-frame') as HTMLIFrameElement).srcdoc = buildPreviewHtml(files, path);
}

$('preview-page').addEventListener('change', () => {
  if (lastWebBuild) showPreview(lastWebBuild.files);
});

// The previewed page posts a message when an internal link/button is
// clicked; switch the previewed page instead of fetching a relative URL.
window.addEventListener('message', (e: MessageEvent) => {
  const msg = e.data as { kinnPreviewNav?: boolean; page?: string };
  if (!msg || !msg.kinnPreviewNav || !msg.page || !lastWebBuild) return;
  const select = $('preview-page') as HTMLSelectElement;
  if (select.querySelector(`option[value="${msg.page}"]`)) {
    select.value = msg.page;
    showPreview(lastWebBuild.files);
  }
});

$('btn-download-web').addEventListener('click', () => {
  if (!lastWebBuild) return;
  downloadZip(lastWebBuild.files, lastWebBuild.site.toLowerCase() || 'kinn-site', zipFiles);
  terminal.print(`downloaded ${lastWebBuild.files.length}-file web dist as .zip`, 'ok');
});

$('btn-export-android').addEventListener('click', () => {
  const f = vfs.active;
  if (!f) return;
  const result = backend.buildAndroid(f.name, vfs.projectFiles());
  if (!result.ok) {
    terminal.print('android export failed — build diagnostics are in the PROBLEMS panel', 'err');
    return;
  }
  downloadZip(result.files, `${result.site.toLowerCase() || 'kinn-app'}-android-project`, zipFiles);
  terminal.print(
    `exported Android Gradle project (${result.files.length} files) as .zip — build the APK with Android Studio, gradle assembleDebug, or remote CI (docs/BACKEND_ANDROID.md)`,
    'ok',
  );
});

// -- explorer ---------------------------------------------------------------

new Explorer(vfs, $('tree'), () => loadActiveFile());

// -- import / export ----------------------------------------------------------

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
        loadActiveFile();
        terminal.print(`imported ${file.name}`, 'ok');
      } else {
        terminal.print(`cannot import '${file.name}' (must match name.kin)`, 'err');
      }
    } else {
      const project = parseProject(text);
      vfs.loadProject(project);
      loadActiveFile();
      $('project-name').textContent = vfs.projectName;
      terminal.print(
        `imported project '${project.name}' (${project.files.length} files) from ${file.name}`,
        'ok',
      );
    }
  } catch (e) {
    terminal.print(`import failed: ${(e as Error).message}`, 'err');
  }
});

// -- top bar actions ----------------------------------------------------------

$('btn-check').addEventListener('click', () => {
  terminal.exec(`check ${vfs.activeName}`);
  tabs.select('side');
});

$('btn-tokens').addEventListener('click', () => {
  terminal.exec('tokens');
  tabs.select('side');
});

$('btn-run').addEventListener('click', () => {
  terminal.exec(`run ${vfs.activeName}`);
  tabs.select('side');
});

$('btn-build-web').addEventListener('click', () => {
  terminal.exec('build web');
  tabs.select('side');
});

$('btn-new-file').addEventListener('click', () => {
  const name = window.prompt('new file name (e.g. mymod.kin):', 'untitled.kin');
  if (!name) return;
  if (vfs.create(name)) {
    loadActiveFile();
    terminal.print(`created ${name}`, 'ok');
  } else {
    terminal.print(`cannot create '${name}' (must match name.kin and not exist)`, 'err');
  }
});

// -- keyboard shortcuts ---------------------------------------------------------

document.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
    e.preventDefault();
    $('btn-check').click();
  } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
    e.preventDefault();
    void vfs.markSaved();
    setStatusSaved();
    toast('project saved to this browser');
  }
});

// -- toast --------------------------------------------------------------------------

let toastTimer = 0;
function toast(message: string): void {
  const el = $('toast');
  el.textContent = message;
  el.classList.add('show');
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => el.classList.remove('show'), 2600);
}

// -- responsive tabs -------------------------------------------------------------------

const tabs = new Tabs($('tabbar'), (tab) => {
  for (const pane of Array.from(document.querySelectorAll('.pane'))) {
    pane.classList.toggle('visible', (pane as HTMLElement).dataset.pane === tab);
  }
});

// -- boot --------------------------------------------------------------------------------

async function boot(): Promise<void> {
  // 1. Restore the persisted project (first visit: bundled samples).
  try {
    const stored = await storage.loadProject();
    if (stored) {
      vfs.loadProject(stored);
    }
  } catch {
    // Storage unavailable (private mode?): stay on the samples.
  }

  // 2. Load the compiler backend; degrade honestly when it is missing.
  const ok = await backend.load();
  const status = $('backend-status');
  if (ok) {
    status.textContent = `compiler ${backend.version()} · wasm`;
    status.classList.add('ready');
  } else {
    status.textContent = 'compiler: not built';
    status.classList.add('missing');
    status.title =
      'kinn.wasm was not found. Build it with: cd compiler && cargo build --release -p kinn-ffi --target wasm32-unknown-unknown && cp target/wasm32-unknown-unknown/release/kinn_ffi.wasm studio/public/kinn.wasm — Studio falls back to editor-level checks.';
  }

  $('project-name').textContent = vfs.projectName;
  loadActiveFile();
  setStatusSaved();
}

void boot();

// Note: on desktop (>900px) the CSS grid shows all panes regardless of the
// tab state; on narrow screens the panes are hidden unless `.visible`, which
// Tabs maintains. No resize handling needed: the classes stay valid for both.
