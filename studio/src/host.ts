/**
 * Host bridge for embedded shells (KINN Studio for Android 0.4.0+).
 *
 * Studio runs unchanged in any browser. When the page is hosted by the
 * Android app, the host injects `window.kinnAndroid` with a small,
 * typed surface (all methods — that is what a WebView JS interface can
 * expose). Two things need it:
 *
 *  1. Downloads — a WebView has no download manager: `Blob` URLs and
 *     `<a download>` clicks are silently dropped. The host routes the
 *     bytes through the system document creator (SAF) instead.
 *  2. "Classic" navigation — on Android the classic fallback is the
 *     *native* Studio 0.3 activity, not another web page. The classic
 *     links stay as normal anchors in the browser, but inside the host
 *     they open the native activity.
 *
 * Every call site feature-detects the bridge; when it is absent the
 * exact previous browser behavior runs. Nothing here is simulated:
 * when the host is missing, exports use the real browser download.
 */

/** Surface injected as `window.kinnAndroid` by NextActivity (Kotlin). */
export interface KinnAndroidHost {
  /** Identifies the host implementation ("android"). */
  platform(): string;
  /** Host bridge version, e.g. "0.4.0". */
  hostVersion(): string;
  /**
   * Persist bytes through the host (SAF create-document on Android).
   * The host launches the system picker and writes the file when the
   * user confirms a location.
   */
  saveFile(name: string, base64: string, mime: string): void;
  /** Open the host's classic native Studio interface. */
  openClassicStudio(): void;
}

declare global {
  interface Window {
    kinnAndroid?: KinnAndroidHost;
  }
}

/** The Android host bridge, or null outside an Android host. */
export function androidHost(): KinnAndroidHost | null {
  if (typeof window === 'undefined') return null;
  const host = window.kinnAndroid;
  if (!host || typeof host.platform !== 'function') return null;
  try {
    return host.platform() === 'android' ? host : null;
  } catch {
    return null;
  }
}

/** Encode bytes as base64 (chunked; export sizes are far below limits). */
export function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/**
 * Save a file the way this host can:
 * browser -> Blob URL + `<a download>` (unchanged), Android host ->
 * `saveFile` through the bridge.
 */
export function hostSaveFile(name: string, bytes: Uint8Array, mime: string): void {
  const host = androidHost();
  if (host) {
    host.saveFile(name, bytesToBase64(bytes), mime);
    return;
  }
  const blob = new Blob([bytes.buffer as ArrayBuffer], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}

/**
 * Wire an anchor that points at the web classic entry (./index.html)
 * so that inside an Android host it opens the native classic Studio
 * instead. In a plain browser the anchor keeps its normal navigation.
 */
export function wireClassicLink(a: HTMLAnchorElement): void {
  const host = androidHost();
  if (!host) return;
  a.addEventListener('click', (e) => {
    e.preventDefault();
    host.openClassicStudio();
  });
  a.title = 'Open the classic native Studio on this device';
}
