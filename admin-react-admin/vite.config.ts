import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

function removeCrossorigin(): import('vite').Plugin {
  return {
    name: 'remove-crossorigin',
    enforce: 'post',
    transformIndexHtml: {
      enforce: 'post',
      transform(html: string) {
        return html.replace(/\bcrossorigin\b/g, '');
      },
    },
  };
}

export default defineConfig({
  plugins: [react(), removeCrossorigin()],
  base: '/admin/',
  build: {
    outDir: 'dist',
    sourcemap: false,
  },
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:8888',
    },
  },
});
