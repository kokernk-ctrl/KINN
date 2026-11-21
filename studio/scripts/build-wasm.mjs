/**
 * Build the compiler backend for Studio: compile kinn-ffi to
 * WebAssembly and copy it to studio/public/kinn.wasm.
 *
 * Requires the Rust toolchain with the wasm32-unknown-unknown target
 * (rustup target add wasm32-unknown-unknown). Studio degrades honestly
 * to editor checks when kinn.wasm is absent.
 */
import { execSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const studio = resolve(here, '..');
const compiler = resolve(studio, '..', 'compiler');
const target = join(compiler, 'target', 'wasm32-unknown-unknown', 'release', 'kinn_ffi.wasm');
const outDir = join(studio, 'public');
const out = join(outDir, 'kinn.wasm');

console.log('building kinn-ffi for wasm32-unknown-unknown (release)...');
execSync('cargo build --release -p kinn-ffi --target wasm32-unknown-unknown', {
  cwd: compiler,
  stdio: 'inherit',
});

if (!existsSync(target)) {
  console.error(`expected wasm build at ${target} but it is missing`);
  process.exit(1);
}

mkdirSync(outDir, { recursive: true });
copyFileSync(target, out);
console.log(`copied ${target} -> ${out}`);
