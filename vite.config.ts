import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  // Vitest resolves packages with the node condition by default, which hands `svelte`'s server build to a
  // jsdom test — and that build refuses to `mount`. A component test needs the same build the browser gets.
  resolve: { conditions: ['browser'] },
  test: {
    server: { deps: { inline: [/svelte/, /codemirror/] } },
  },
})
