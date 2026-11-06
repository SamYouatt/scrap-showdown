import { defineConfig } from 'vite'

export default defineConfig({
  // allow Amp orb portal hostnames to reach the dev server
  server: { allowedHosts: ['.onamp.dev'] },
  build: { chunkSizeWarningLimit: 4000 },
})
