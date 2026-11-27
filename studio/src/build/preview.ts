/**
 * In-Studio preview of a generated web build.
 *
 * The generated dist is a plain multi-page site — static (0.3) or
 * dynamic (0.4 S6: kinnrt.js + kinnrt.css + app.js). To preview it
 * inside Studio (which has no filesystem), each page is rendered
 * self-contained: the stylesheets and the generated app.js are
 * inlined and internal navigation is intercepted, so clicking a link
 * or button switches the previewed page instead of trying to fetch a
 * relative URL. `download .zip` gives the real multi-page dist for
 * hosting.
 */

import type { OutputFile } from '../wasm/kinnwasm';
import { hostSaveFile } from '../host';

/** Message posted by the injected script when the user navigates. */
export interface PreviewNavMessage {
  kinnPreviewNav: true;
  page: string;
}

/** Pages of a build: every `*.html` artifact, in build order. */
export function htmlPages(files: OutputFile[]): OutputFile[] {
  return files.filter((f) => f.path.endsWith('.html'));
}

/**
 * Turn a generated page into a self-contained preview document: the
 * stylesheet is inlined, the generated app.js is inlined, and a small
 * script intercepts internal navigation (posts a PreviewNavMessage to
 * the parent).
 */
export function buildPreviewHtml(files: OutputFile[], pagePath: string): string {
  const page = files.find((f) => f.path === pagePath);
  if (!page) throw new Error(`no generated page named '${pagePath}'`);
  const css = files.find((f) => f.path === 'styles.css')?.content ?? '';
  const animCss = files.find((f) => f.path === 'kinnrt.css')?.content ?? '';
  const rt = files.find((f) => f.path === 'kinnrt.js')?.content ?? '';
  const js = files.find((f) => f.path === 'app.js')?.content ?? '';

  let html = page.content;
  if (css) {
    html = html.replace(
      /<link rel="stylesheet" href="styles.css">/,
      `<style>\n${css}\n</style>`,
    );
  }
  // Dynamic apps only (S6 Phase C): the predefined animation sheet.
  // Static 0.3 sites have no kinnrt.css link and are untouched.
  if (animCss) {
    html = html.replace(
      /<link rel="stylesheet" href="kinnrt\.css">/,
      `<style>\n${animCss}\n</style>`,
    );
  }
  // Dynamic apps only: the runtime loads before the generated module
  // (which reads globalThis.KinnRT at its IIFE call). Static 0.3 sites
  // have no kinnrt.js tag and are untouched.
  if (rt) {
    html = html.replace(
      /<script src="kinnrt\.js" defer><\/script>/,
      `<script>\n${rt}\n</script>`,
    );
  }
  if (js) {
    html = html.replace(
      /<script src="app\.js" defer><\/script>/,
      `<script>\n${js}\n</script>`,
    );
  }
  // Navigation interception: internal links switch the previewed page.
  return html.replace(
    '</body>',
    `<script>
(function () {
  "use strict";
  document.addEventListener("click", function (e) {
    var a = e.target && e.target.closest ? e.target.closest("a[href]") : null;
    if (!a) return;
    var href = a.getAttribute("href") || "";
    if (href.endsWith(".html") && !href.startsWith("http")) {
      e.preventDefault();
      window.parent.postMessage({ kinnPreviewNav: true, page: href }, "*");
    }
  });
})();
</script>
</body>`,
  );
}

/** Download a build result as a real .zip (store-only, no deps). */
export function downloadZip(
  files: OutputFile[],
  name: string,
  zipper: (entries: Array<{ path: string; content: string }>) => Uint8Array,
): void {
  // zipFiles returns a fresh buffer over its whole ArrayBuffer, so the
  // cast to BlobPart is exact (TS strict mode wants the narrower type).
  const bytes = zipper(files.map((f) => ({ path: f.path, content: f.content })));
  // Browser: Blob + <a download>; Android host: SAF save through the
  // bridge (a WebView drops anchor downloads silently).
  hostSaveFile(`${name}.zip`, bytes, 'application/zip');
}
