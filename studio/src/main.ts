/**
 * KINN Studio bootstrap: wires the virtual file system, explorer,
 * editor, terminal, problems panel and responsive tabs together.
 */

import { Vfs } from './vfs';
import { Explorer } from './ui/explorer';
import { Editor } from './ui/editor';
import { Terminal } from './ui/terminal';
import { ProblemsPanel } from './ui/problems';
import { Tabs } from './ui/tabs';
import { detectMode, quickCheck } from './diagnostics/checks';

import helloSample from '../samples/hello.kin?raw';
import loopsSample from '../samples/loops.kin?raw';
import brokenSample from '../samples/broken.kin?raw';

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing element #${id}`);
  return el;
}

const vfs = new Vfs('samples', [
  ['hello.kin', helloSample],
  ['loops.kin', loopsSample],
  ['broken.kin', brokenSample],
]);

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
  const f = vfs.active;
  const el = $('status-saved');
  el.textContent = f?.dirty ? 'modified (in-memory)' : 'in-memory project';
  el.classList.toggle('dirty', Boolean(f?.dirty));
}

// -- problems ---------------------------------------------------------------

const problems = new ProblemsPanel($('problems-list'), $('problems-count'), (line) => {
  editor.gotoLine(line);
});

function runChecks(): void {
  const f = vfs.active;
  if (!f) return;
  problems.render(quickCheck(f.content), f.name);
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
  },
);

// -- explorer ---------------------------------------------------------------

new Explorer(vfs, $('tree'), () => loadActiveFile());

// -- top bar actions ----------------------------------------------------------

$('btn-check').addEventListener('click', () => {
  terminal.print('check ' + vfs.activeName, 'cmd');
  const f = vfs.active;
  if (!f) return;
  const list = quickCheck(f.content);
  problems.render(list, f.name);
  if (list.length === 0) {
    terminal.print(`OK: no editor-level issues found in ${f.name}`, 'ok');
  } else {
    for (const p of list) {
      terminal.print(`${p.severity} ${p.code}: ${p.message}  [${f.name}:${p.line}]`, p.severity === 'error' ? 'err' : 'warn');
    }
  }
  tabs.select('side');
});

$('btn-tokens').addEventListener('click', () => {
  terminal.exec('tokens');
  tabs.select('side');
});

$('btn-run').addEventListener('click', () => {
  terminal.exec('run');
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
    vfs.markSaved();
    setStatusSaved();
    toast('saved (in-memory project — file system persistence arrives in 0.2)');
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

$('project-name').textContent = vfs.projectName;
loadActiveFile();
setStatusSaved();

// Note: on desktop (>900px) the CSS grid shows all panes regardless of the
// tab state; on narrow screens the panes are hidden unless `.visible`, which
// Tabs maintains. No resize handling needed: the classes stay valid for both.

