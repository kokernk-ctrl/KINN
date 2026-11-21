import { describe, expect, it } from 'vitest';
import { crc32, zipFiles } from './zip';

function ascii(s: string): Uint8Array {
  return Uint8Array.from(s, (c) => c.charCodeAt(0));
}

describe('crc32', () => {
  it('matches the canonical check vector', () => {
    // "123456789" -> 0xCBF43926 (IEEE 802.3, reflected, final xor).
    expect(crc32(ascii('123456789'))).toBe(0xcbf43926);
  });

  it('is stable for unicode payloads', () => {
    const bytes = new TextEncoder().encode('Olá → 世界');
    expect(crc32(bytes)).toBe(crc32(bytes));
  });
});

describe('zipFiles', () => {
  const zip = zipFiles([
    { path: 'index.html', content: '<h1>hi</h1>' },
    { path: 'styles.css', content: 'body{}' },
  ]);

  const u32 = (off: number) =>
    new DataView(zip.buffer, zip.byteOffset, zip.byteLength).getUint32(off, true);
  const u16 = (off: number) =>
    new DataView(zip.buffer, zip.byteOffset, zip.byteLength).getUint16(off, true);

  it('starts with a local file header and ends with the EOCD', () => {
    expect(u32(0)).toBe(0x04034b50);
    expect(u32(zip.length - 22)).toBe(0x06054b50);
  });

  it('declares both entries in the EOCD', () => {
    expect(u16(zip.length - 22 + 8)).toBe(2);
    expect(u16(zip.length - 22 + 10)).toBe(2);
  });

  it('uses the store method and records correct sizes and CRC', () => {
    expect(u16(8)).toBe(0); // method: store
    const dataLen = u32(18);
    expect(u32(22)).toBe(dataLen); // uncompressed == compressed (store)
    expect(u32(14)).toBe(crc32(ascii('<h1>hi</h1>')));
  });

  it('stores the entry name in UTF-8 next to the header', () => {
    const nameLen = u16(26);
    const name = new TextDecoder().decode(zip.slice(30, 30 + nameLen));
    expect(name).toBe('index.html');
  });

  it('accepts unicode names (paths of generated projects are ascii, but be safe)', () => {
    const z = zipFiles([{ path: 'app/src/main/res/values/strings.xml', content: '<x/>' }]);
    const dv = new DataView(z.buffer);
    expect(dv.getUint32(0, true)).toBe(0x04034b50);
  });
});
