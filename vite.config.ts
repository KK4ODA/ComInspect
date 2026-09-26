import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const host = process.env.TAURI_DEV_HOST;

// The frontend lives in ui/; Tauri loads ui/dist in production builds.
export default defineConfig({
  root: 'ui',
  plugins: [svelte({ configFile: '../svelte.config.js' })],
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 5174 } : undefined,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    // WebView2 (Chromium) on Windows, WKWebView (Safari 15+) on macOS 11+,
    // WebKitGTK 4.1 on Linux.
    target: ['es2021', 'chrome105', 'safari15'],
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    chunkSizeWarningLimit: 600,
  },
  test: {
    root: '.',
    include: ['ui/src/**/*.test.ts'],
    environment: 'node',
  },
});
