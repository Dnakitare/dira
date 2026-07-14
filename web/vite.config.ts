import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Dev-mode proxy to a locally running `dira simulate` (or replay) instance.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/ws': { target: 'ws://127.0.0.1:8080', ws: true },
      '/api': { target: 'http://127.0.0.1:8080' },
    },
  },
})
