import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(process.env.npm_package_version ?? '0.0.0'),
  },
  // The repository intentionally keeps large upstream research checkouts and
  // Rust build output beside the app. Restrict dependency discovery and file
  // watching to the actual frontend so dev startup does not crawl WaveTerm or
  // reload whenever Cargo writes generated Tauri assets.
  optimizeDeps: {
    entries: ['index.html'],
  },
  server: {
    watch: {
      ignored: ['**/references/**', '**/target/**'],
    },
  },
})
