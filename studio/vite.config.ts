/// <reference types="vitest" />
import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  // Relative base so the built app also works from file:// and inside
  // future desktop/Android shells without a server.
  base: './',
  build: {
    target: 'es2022',
    // Two independent entries: index.html (classic Studio, untouched
    // behavior) and next.html (experimental terminal-first Studio).
    // Both ship in dist/ so the previous interface stays available.
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL('./index.html', import.meta.url)),
        next: fileURLToPath(new URL('./next.html', import.meta.url)),
      },
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
