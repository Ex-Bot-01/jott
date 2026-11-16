//! Vite config for `npm run ui` — the browser harness. **Not used by `npm run build`, the tests, or CI**: a shipped
//! bundle must talk to Tauri, and an accidental alias here would ship a window that talks to fixtures.
//!
//! It swaps exactly the six modules the front end imports from Tauri (`core`, `event`, `window`, and the dialog,
//! notification and opener plugins) for the fakes in `src/dev/`. Every component, every stylesheet rule and every
//! pure module under `src/lib` is the real thing.

import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

const dev = (name: string) => fileURLToPath(new URL(`./src/dev/${name}`, import.meta.url))

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    alias: [
      { find: '@tauri-apps/api/core', replacement: dev('harnessCore.ts') },
      { find: '@tauri-apps/api/event', replacement: dev('harnessEvent.ts') },
      { find: '@tauri-apps/api/window', replacement: dev('harnessWindow.ts') },
      { find: '@tauri-apps/plugin-dialog', replacement: dev('harnessDialog.ts') },
      { find: '@tauri-apps/plugin-notification', replacement: dev('harnessNotification.ts') },
      { find: '@tauri-apps/plugin-opener', replacement: dev('harnessOpener.ts') },
    ],
  },
  server: { port: 5199, strictPort: true, host: '127.0.0.1' },
})
