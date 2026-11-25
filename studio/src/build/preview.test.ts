import { describe, expect, it } from 'vitest';
import { buildPreviewHtml, htmlPages } from './preview';
import type { OutputFile } from '../wasm/kinnwasm';

const INDEX: OutputFile = {
  path: 'index.html',
  content: `<!doctype html>
<html lang="en">
<head>
<title>Welcome · Site</title>
<link rel="stylesheet" href="styles.css">
</head>
<body>
<nav><a href="index.html" aria-current="page">Home</a><a href="about.html">About</a></nav>
<main><h1 class="kinn-title">Welcome</h1></main>
<script src="app.js" defer></script>
</body>
</html>`,
};

const ABOUT: OutputFile = {
  path: 'about.html',
  content: `<!doctype html>
<html lang="en"><head><link rel="stylesheet" href="styles.css"></head>
<body><main><a href="index.html">Home</a></main>
<script src="app.js" defer></script>
</body></html>`,
};

const CSS: OutputFile = { path: 'styles.css', content: 'body { color: #111; }' };
const JS: OutputFile = { path: 'app.js', content: 'document.documentElement.classList.add("kinn-js");' };

const FILES = [INDEX, ABOUT, CSS, JS];

describe('htmlPages', () => {
  it('lists only html artifacts in build order', () => {
    expect(htmlPages(FILES).map((f) => f.path)).toEqual(['index.html', 'about.html']);
  });
});

describe('buildPreviewHtml', () => {
  it('inlines the stylesheet and the generated script', () => {
    const html = buildPreviewHtml(FILES, 'index.html');
    expect(html).toContain('<style>');
    expect(html).toContain('body { color: #111; }');
    expect(html).not.toContain('<link rel="stylesheet"');
    expect(html).not.toContain('<script src="app.js"');
    expect(html).toContain('classList.add("kinn-js")');
  });

  it('keeps the generated content intact', () => {
    const html = buildPreviewHtml(FILES, 'index.html');
    expect(html).toContain('<h1 class="kinn-title">Welcome</h1>');
    expect(html).toContain('aria-current="page"');
  });

  it('injects the navigation interceptor that messages the parent', () => {
    const html = buildPreviewHtml(FILES, 'about.html');
    expect(html).toContain('kinnPreviewNav');
    expect(html).toContain('window.parent.postMessage');
    // Internal hrefs stay as-is; the interceptor handles the click.
    expect(html).toContain('href="index.html"');
  });

  // Regression: the injected interceptor must target anchors with a
  // VALID selector. A corrupted selector (e.g. a broken literal) would throw a
  // DOMException on every click and break in-preview navigation while
  // the build itself kept passing — this pins the exact selector.
  it('uses a valid anchor selector in the navigation interceptor', () => {
    const html = buildPreviewHtml(FILES, 'about.html');
    const m = html.match(/closest\("([^"]+)"\)/);
    expect(m).not.toBeNull();
    expect(m![1]).toBe('a[href]');
    // And the selector is genuinely valid in a DOM (throws otherwise).
    if (typeof document !== 'undefined') {
      expect(() => document.querySelector(m![1])).not.toThrow();
    }
  });

  it('throws on an unknown page (honest error, no fake output)', () => {
    expect(() => buildPreviewHtml(FILES, 'missing.html')).toThrow(/missing\.html/);
  });
});
