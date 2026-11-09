import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  resolve: {
    alias: { $lib: fileURLToPath(new URL('./src/lib', import.meta.url)) }
  },
  clearScreen: false,
  server: {
    host: '0.0.0.0',
    port: 5173,
    strictPort: true,
    // En WSL sobre /mnt/c (NTFS) no llegan eventos de inotify: `ARIADNA_POLL=1 npm run dev` activa el sondeo.
    watch: process.env.ARIADNA_POLL === '1' ? { usePolling: true, interval: 300 } : undefined
  },
  build: {
    // Compatibilidad: WebView2 Evergreen y WebKit de macOS 13 (Safari 16).
    target: ['es2022', 'chrome105', 'safari16'],
    // CSP `font-src 'self'`: ninguna fuente ni recurso se incrusta como data:.
    assetsInlineLimit: 0,
    sourcemap: false
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.ts'],
    css: false
  }
});
