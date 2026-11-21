import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const backend = process.env.MPF_BACKEND ?? 'http://127.0.0.1:8787'
// Relative asset URLs: index.html sets <base href> at runtime, so the same
// build works at the root and under any reverse-proxy prefix.
const base = process.env.MPF_BASE ?? './'

export default defineConfig({
  base,
  plugins: [react()],
  server: {
    proxy: {
      '/api': backend,
      '/media': backend,
    },
  },
})
