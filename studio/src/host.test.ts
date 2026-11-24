import { afterEach, describe, expect, it, vi } from 'vitest';
import { androidHost, bytesToBase64, hostSaveFile, wireClassicLink } from './host';
import type { KinnAndroidHost } from './host';

/**
 * The host bridge must be inert in every plain-browser (and test-node)
 * context, and must route downloads / classic navigation through the
 * Android host when one is injected. No call site may assume the
 * bridge exists.
 */

function injectHost(overrides: Partial<KinnAndroidHost> = {}): KinnAndroidHost {
  const host: KinnAndroidHost = {
    platform: vi.fn(() => 'android'),
    hostVersion: vi.fn(() => '0.4.0-test'),
    saveFile: vi.fn(),
    openClassicStudio: vi.fn(),
    ...overrides,
  };
  (globalThis as { window: Partial<Window> }).window = { kinnAndroid: host };
  return host;
}

afterEach(() => {
  delete (globalThis as { window?: Partial<Window> }).window;
});

describe('androidHost', () => {
  it('returns null outside a window (node, tests, SSR)', () => {
    expect(androidHost()).toBeNull();
  });

  it('returns null when no bridge is injected (plain browser page)', () => {
    (globalThis as { window: Partial<Window> }).window = {};
    expect(androidHost()).toBeNull();
  });

  it('returns null for a foreign bridge (platform guard)', () => {
    (globalThis as { window: Partial<Window> }).window = {
      kinnAndroid: { platform: () => 'something-else', hostVersion: () => 'x', saveFile: () => {}, openClassicStudio: () => {} },
    };
    expect(androidHost()).toBeNull();
  });

  it('returns null when platform is not a function (defensive)', () => {
    (globalThis as { window: Partial<Window> }).window = {
      kinnAndroid: { platform: 'android' } as unknown as KinnAndroidHost,
    };
    expect(androidHost()).toBeNull();
  });

  it('returns the injected Android host', () => {
    const host = injectHost();
    expect(androidHost()).toBe(host);
  });
});

describe('bytesToBase64', () => {
  it('matches the reference encoding (ASCII + multibyte + empty)', () => {
    const enc = new TextEncoder();
    expect(bytesToBase64(enc.encode(''))).toBe('');
    expect(bytesToBase64(enc.encode('kinn'))).toBe('a2lubg==');
    expect(bytesToBase64(enc.encode('compiler/VM ✓ real'))).toBe(
      Buffer.from(enc.encode('compiler/VM ✓ real')).toString('base64'),
    );
  });

  it('encodes more than one chunk without seams', () => {
    const bytes = new Uint8Array(0x8000 + 1234);
    for (let i = 0; i < bytes.length; i++) bytes[i] = (i * 7) & 0xff;
    expect(bytesToBase64(bytes)).toBe(Buffer.from(bytes).toString('base64'));
  });
});

describe('hostSaveFile', () => {
  it('routes through the host bridge when present', () => {
    const host = injectHost();
    const bytes = new TextEncoder().encode('{"format":"kinnproj"}');
    hostSaveFile('proj.kinnproj', bytes, 'application/json');
    expect(host.saveFile).toHaveBeenCalledWith(
      'proj.kinnproj',
      Buffer.from(bytes).toString('base64'),
      'application/json',
    );
  });
});

describe('wireClassicLink', () => {
  it('opens the native classic Studio on click when hosted', () => {
    const host = injectHost();
    let handler: ((e: { preventDefault(): void }) => void) | undefined;
    const anchor = {
      title: '',
      addEventListener: (_: string, cb: (e: { preventDefault(): void }) => void) => {
        handler = cb;
      },
    } as unknown as HTMLAnchorElement;
    wireClassicLink(anchor);
    expect(handler).toBeTypeOf('function');
    const preventDefault = vi.fn();
    handler!({ preventDefault });
    expect(preventDefault).toHaveBeenCalled();
    expect(host.openClassicStudio).toHaveBeenCalledTimes(1);
  });
});
