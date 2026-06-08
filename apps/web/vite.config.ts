import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';

// Resolve the workspace packages to their TypeScript source so the whole repo
// builds with no per-package compile step in Phase 0.
const r = (p: string) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  resolve: {
    alias: {
      '@cherry/core': r('../../packages/core/src/index.ts'),
      '@cherry/modes': r('../../packages/modes/src/index.ts'),
    },
  },
  server: { port: 5173, host: true },
  build: { target: 'es2022', outDir: 'dist' },
});
