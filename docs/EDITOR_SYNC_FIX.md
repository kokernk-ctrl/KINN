# Editor alignment & sync fix (0.4.1)

*2026-11-24 — fix for the six editing symptoms reported on Android
phones: wrong tap-to-caret position, backspace deleting the wrong
character (visible text diverging from compiled text), highlight
drifting (lower lines shifted left), horizontal scrollbars cutting
through the code, background/gutter ending mid-file, and layout
breakage with the soft keyboard open.*

## What the editor actually is (analysis, step 1)

The IDE was assumed to be Compose / Compose Multiplatform. It is not.
There is no Compose anywhere in the project. Two real editors exist:

| Editor | Stack | Where it runs |
| --- | --- | --- |
| **Studio NEXT** | TypeScript + HTML (`textarea` + highlighted `<pre>` overlay) | Default interface of the Android APK 0.4.0 (`NextActivity`, WebView via `WebViewAssetLoader`) and desktop browsers |
| **Classic native** | Android Views (`EditText` + separate `TextView` gutter inside a `HorizontalScrollView`) | Fallback app (`MainActivity`, reachable from NEXT through the host bridge) |

The classic **web** editor (`studio/index.html`) shares the NEXT
technique and is reachable from the "classic" link, so it was fixed
with the same architecture. All fixes below keep each editor inside
its own consistent stack — no Compose/Views mixing, no editor
replacement.

## Root causes (one per symptom)

### NEXT + classic web (textarea-over-overlay editors)

The old DOM put the overlay `<pre>` and the textarea in **different
scroll worlds**:

```
old:  .nx-editor-scroll (overflow:auto)          ← scroller #1
        ├─ .nx-highlight (absolute, inset:0, overflow:hidden)  ← scroll copied via JS
        └─ textarea (relative, height:100%, overflow:auto)     ← scroller #2
```

1. **Tap lands elsewhere / backspace deletes the wrong char** — the
   overlay stretched the parent's scrollable overflow (`min-width:
   max-content` on an absolutely positioned element), so the browser
   had *two* things it could scroll. Whenever it scrolled the parent
   (caret bring-into-view, touch panning), the textarea's
   `scrollTop/Left` never changed, so `syncScroll()` never ran and the
   overlay stayed behind: the text the user *saw* (overlay) diverged
   from the text being *edited* (textarea). Taps and deletions then
   act on offsets that do not match what is on screen.
2. **Highlight misaligned, lower lines shifted left** — same
   divergence: parent scrolled horizontally, overlay anchored at
   content origin → appears shifted left; vertical drift accumulates
   toward the bottom.
3. **Horizontal scrollbars in the middle of the code** — literally the
   textarea's own scrollbar (it was `overflow:auto`) plus the outer
   scroller's; one of them renders mid-editor.
4. **Background ends mid-file / line numbers mismatch** — the overlay
   was `overflow:hidden` with `min-height:max-content` (clipped at
   stale scroll offsets), and the gutter was translated using the
   **textarea's** scrollTop, which stays 0 when the parent scrolls.
5. **Keyboard breaks layout** — the resize itself is handled
   (`adjustResize` + `interactive-widget=resizes-content` +
   `--nx-vvh`), but with two scrollers the caret bring-into-view moved
   the wrong layer, producing jumps and drift.

### Classic native (Android Views)

1. **`rehighlight()` replaced the whole field content**
   (`editor.text = spanned`, a fresh `SpannableStringBuilder`) 250 ms
   after every edit — exactly what requirement J forbids. The IME
   composition state is destroyed mid-word: backspace then deletes
   characters the keyboard *thinks* exist, the visible text stops
   matching the real content, and the compiler (fed from disk via the
   watcher) reports errors on code that looks correct.
2. **Gutter was a separate `TextView`** whose numbers came from
   counting `\n`, while the `EditText` **soft-wrapped** long lines
   (`textMultiLine` without `setHorizontallyScrolling`) — visual lines
   ≠ logical lines, so numbers drift apart going down. No shared
   scroll, metrics "estimated" by two independent TextViews.
3. **`HorizontalScrollView` wrapper** put one horizontal scrollbar
   across the editor and dragged the gutter off-screen horizontally.
4. **Compilation read the disk copy** (`store.readFile`) instead of
   the field — a cached copy, one watcher side-effect away from being
   stale.

## The fix — one architecture, one source of truth

### Web editors (NEXT + classic)

```
new:  .nx-editor-wrap (flex, overflow:hidden)
      ├─ .nx-gutter        (never scrolls; translated by the ONE vertical scroll)
      └─ .nx-editor-scroll (THE only scroller, x+y)
         └─ .nx-editor-sizer (relative; min-width:max-content, min-height:100%)
            ├─ .nx-active-line (absolute, moves with content)
            ├─ .nx-highlight (pre, IN-FLOW — it sizes the sizer)
            └─ textarea (absolute inset:0 — exactly the <pre> box)
```

- The `<pre>` is in-flow and defines the sizer size; the textarea is
  `absolute; inset:0` over it. Both share one containing block and one
  scroller — **alignment by construction**; there is no scroll copying
  and nothing can drift (requirement B: a pure visual transformation
  with identical offsets — `escapeHtml` keeps every character).
- The textarea box always equals its content box, so it **never
  scrolls internally** (`overflow:hidden`): exactly one horizontal and
  one vertical scrollbar, at the editor edges (requirement D), with
  `wrap="off"` / `white-space:pre` (no soft wrap).
- The gutter is driven by the single `scrollTop` (vertical only) and
  generated from the **same line split the overlay rendered**; same
  font/size/line-height/padding (requirement C, E).
- `render()` touches only the overlay and the gutter — never the
  textarea value/selection → **IME composition survives every
  keystroke** (requirement A, G; `keydown` handlers return immediately
  when `e.isComposing`).
- Cursor tracking uses the standard `selectionchange` event; the
  caret line is kept on screen by scrolling the single scroller
  (`ensureCaretVisible`, the web BringIntoView — requirement F).
- **Compilation always reads the current editor value**: run / check /
  build / export call `syncActiveText()` (mirrors `editor.getText()`
  into the vfs) *right before* invoking the compiler (requirement H);
  the debounced 900 ms auto-check re-reads at fire time.

### Classic native (Android Views)

- New `CodeEditorView` (extends `AppCompatEditText`) owns the whole
  editing surface:
  - **line numbers drawn in `onDraw`** from the real text layout
    (`Layout.getLineBaseline()`, `Layout.getLineTop()`, offset by
    `scrollY`/`extendedPaddingTop`) — numbers match the rendered lines
    by construction, follow vertical scroll exactly, stay fixed
    horizontally (requirement K, preferred option);
  - **`setHorizontallyScrolling(true)`** — no soft wrap, the field
    scrolls itself horizontally: no `HorizontalScrollView`, no
    scrollbar across the code (requirement D softWrap=false);
  - input configured as
    `textMultiLine|textNoSuggestions|textVisiblePassword` — the
    strongest Gboard hint to disable autocorrect/suggestions
    (requirement M; `imeOptions=flagNoExtractUi`, `autocorrect`
    off on the web side);
  - the gutter column is reserved via dynamic left padding computed
    from the digit count (outside `onDraw`, so `requestLayout` stays
    legal).
- `KinSyntax.applyHighlight(editable, colors)` now **applies spans to
  the live Editable** (`setSpan` with our own `KinSpan` marker class,
  removal only ever touches `KinSpan`s) — no `setText`, no watcher
  removal, no selection restore needed: caret, selection and IME
  composition are never interrupted (requirement J). Debounced 250 ms
  as before, but it is now a pure visual update.
- `MainActivity` compiles **straight from the field**: `entrySource()`
  returns `editor.text.toString()` when the entry is the active file
  (requirement H); the disk write stays as persistence.
- Keyboard: `windowSoftInputMode="adjustResize"` was already set for
  both activities (requirement L). With `targetSdk 34` and the WebView
  filling the window, `adjustResize` is the single resize strategy —
  deliberately **not** combined with `setDecorFitsSystemWindows(false)`
  + manual insets, which would be a second compensation and is exactly
  the "double resize" the requirements forbid. On the web side the
  page tracks `visualViewport` (`--nx-vvh`) and the manifest/viewport
  pair (`interactive-widget=resizes-content`) keeps the ICB consistent
  with the resized window — one strategy per layer, no compounding.

## Changed files

**Android (classic native)**
- `android/app/src/main/java/dev/kinn/studio/CodeEditorView.kt` — new:
  owns gutter drawing + highlight scheduling.
- `android/app/src/main/java/dev/kinn/studio/KinSyntax.kt` —
  `applyHighlight(Editable)` replaces `highlight(String)` (span-based,
  in-place).
- `android/app/src/main/java/dev/kinn/studio/MainActivity.kt` —
  watcher only persists; no more `setText` rehighlight; `entrySource`
  reads the field; `gotoLine` uses `bringPointIntoView`.
- `android/app/src/main/res/layout/activity_main.xml` —
  `HorizontalScrollView` + gutter `TextView` replaced by the single
  `CodeEditorView`.

**Studio NEXT (default APK interface)**
- `studio/src/next/ui/codeeditor.ts` — rewritten for the single-scroll
  sizer architecture; `selectionchange`; IME-safe keydown;
  `ensureCaretVisible`.
- `studio/src/next/main.ts` — `syncActiveText()` before every
  run/check/build/export; auto-check re-reads at fire time.
- `studio/next.html`, `studio/src/next/next.css` — new DOM/CSS
  (sizer; in-flow `<pre>`; textarea `absolute inset:0`;
  `autocorrect="off"`).

**Studio classic web**
- `studio/src/ui/editor.ts`, `studio/index.html`,
  `studio/src/theme.css` — same single-scroll architecture; render is
  synchronous (no 30 ms window in which the textarea could grow a
  transient scrollbar); `studio/src/main.ts` — `syncActiveText()`
  before check/run/build.

**Tests**
- `studio/src/next/ui/codeeditor.test.ts` — 15 unit tests (mirror
  offsets, gutter split, IME guard, cursor reports, scroll state).
- `studio/src/ui/editor.test.ts` — 6 unit tests (classic web).
- `scripts/e2e/` (dev-side, not shipped) — browser battery on the
  built `dist/` at 390×844.

## Tests executed

Unit (vitest, jsdom): **163 passed, 0 failed** — 21 new editor tests +
the existing 142.

E2E on the built bundle, real Chromium at mobile viewport 390×844
(`scripts/e2e/run_next_e2e.sh`):

| Test | Verifies | Result |
| --- | --- | --- |
| T1 | overlay and textarea: same origin/box, identical computed font metrics | PASS |
| T2 | tap on 3 positions → caret at the exact visible character offset (3×) | PASS |
| T3 | insert block, insert mid, backspace×3, word replace, empty — overlay == real text after every step | PASS |
| T4 | long line: textarea never scrolls inside, only the one scroller extends, still aligned after h-scroll | PASS |
| T5 | 80 lines, scroll 600px: gutter transform matches, numbers == lines, 4 sampled lines aligned ≤1px | PASS |
| T7 | type invalid code (`var = 1` line 2) → **Run immediately** → diagnostics reference line 2 of the just-typed text (kinn.wasm compiler) | PASS |
| T8 | reload → edited content restored from IndexedDB | PASS |
| T9 | viewport shrunk to 420px (keyboard equivalent): still aligned, app height follows visual viewport, caret visible, gutter visible | PASS |

Classic web (1280×800): T1 geometry+styles PASS, T3 mirror PASS, T4
single-hscroll PASS, T5 gutter alignment (transform/numbers/3 sampled
lines) PASS.

Android compilation: no Android SDK is available in this environment;
the Kotlin sources were syntax-checked with kotlinc (only unresolved
Android references, as expected) and the full `assembleDebug` +
`assembleRelease` build runs in CI (`android.yml`) on push.

## Limitations / not verifiable here

- **Real soft-keyboard IME behaviour** (Gboard composition, autocorrect
  suppression) cannot be exercised in a headless browser — no Android
  device or emulator in this environment. The composition-safety is
  covered by construction (no code path replaces the text field during
  editing) and by the `isComposing` unit tests, but on-device
  validation is recommended (chrome://inspect into the debug APK).
- The `selectionchange` event is not supported for textareas in older
  WebViews (< Chromium 91); `click`/`keyup`/`select` fallbacks remain
  wired, so only the Ln/Col indicator lags there — alignment is not
  affected.
- The native classic editor's drawing is verified by design (same
  `Layout` object as the rendered text) and compiled in CI; no
  instrumented UI test exists for it.
