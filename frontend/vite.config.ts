import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

const backend = process.env.MPF_BACKEND ?? 'http://127.0.0.1:8787'
const base = process.env.MPF_BASE ?? '/'

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
