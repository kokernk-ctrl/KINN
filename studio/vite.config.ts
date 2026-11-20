/// <reference types="vitest" />
import { defineConfig } from 'vitest/config';

export default defineConfig({
  // Relative base so the built app also works from file:// and inside
  // future desktop/Android shells without a server.
  base: './',
  build: {
    target: 'es2022',
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
