import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// VITE_BASE lets the GitHub Pages build serve from a subpath.
// Dev-mode proxy targets a locally running `dira simulate` (or replay).
export default defineConfig({
  base: process.env.VITE_BASE ?? '/',
  plugins: [react()],
  server: {
    proxy: {
      '/ws': { target: 'ws://127.0.0.1:8080', ws: true },
      '/api': { target: 'http://127.0.0.1:8080' },
    },
  },
})
