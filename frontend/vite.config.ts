import { svelte } from '@sveltejs/vite-plugin-svelte';
import { VitePWA } from 'vite-plugin-pwa';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [
    svelte(),
    VitePWA({
      registerType: 'autoUpdate',
      pwaAssets: { image: 'public/favicon.svg' },
      manifest: {
        name: 'Cinemap',
        short_name: 'Cinemap',
        description: 'Les séances de cinéma près de chez vous, sur une carte.',
        lang: 'fr',
        theme_color: '#b4232c',
        background_color: '#faf8f5',
        display: 'standalone',
        start_url: '/',
      },
      workbox: {
        globPatterns: ['**/*.{js,css,html,svg,png,woff2}'],
        // MapLibre (~290 Ko gzip) n'est pas préchargé : sur mobile, on ne le télécharge
        // que si la carte est ouverte. Il est mis en cache au premier usage (ci-dessous).
        globIgnores: ['**/maplibre-gl-*'],
        navigateFallbackDenylist: [/^\/api\//],
        runtimeCaching: [
          {
            urlPattern: ({ url }) => url.pathname.startsWith('/assets/maplibre-gl-'),
            handler: 'CacheFirst',
            options: { cacheName: 'maplibre', expiration: { maxEntries: 4 } },
          },
          {
            // Dernière réponse gardée pour le hors-ligne ; le réseau reste prioritaire.
            urlPattern: ({ url }) => url.pathname.startsWith('/api/'),
            handler: 'NetworkFirst',
            options: {
              cacheName: 'api',
              networkTimeoutSeconds: 5,
              expiration: { maxEntries: 200, maxAgeSeconds: 2 * 24 * 3600 },
            },
          },
          {
            urlPattern: ({ url }) => url.hostname === 'tiles.openfreemap.org',
            handler: 'StaleWhileRevalidate',
            options: {
              cacheName: 'tiles',
              expiration: { maxEntries: 800, maxAgeSeconds: 14 * 24 * 3600 },
            },
          },
          {
            urlPattern: ({ url }) =>
              url.hostname.endsWith('acsta.net') || url.hostname === 'image.tmdb.org',
            handler: 'CacheFirst',
            options: {
              cacheName: 'images',
              expiration: { maxEntries: 300, maxAgeSeconds: 30 * 24 * 3600 },
            },
          },
        ],
      },
    }),
  ],
  build: {
    // MapLibre seul dépasse la limite par défaut ; il est chargé à part, à la demande.
    chunkSizeWarningLimit: 1200,
    rolldownOptions: {
      output: {
        // Le worker de MapLibre (`maplibre-gl-worker.mjs?url`) est un module worker : le
        // navigateur exige un type JavaScript. Le nginx d'Ubuntu ne connaît pas `.mjs`
        // (servi en application/octet-stream, donc refusé, et non compressé) : on le
        // publie en `.js`.
        assetFileNames: (asset) =>
          asset.names.some((name) => name.endsWith('.mjs'))
            ? 'assets/[name]-[hash].js'
            : 'assets/[name]-[hash][extname]',
      },
    },
  },
  server: {
    // En dev, l'API Rust tourne à côté : même origine pour le navigateur, pas de CORS.
    proxy: { '/api': 'http://localhost:3000' },
  },
  preview: {
    // Comme nginx en prod : les fichiers du build ici, les pages HTML (titre, JSON-LD,
    // redirections, sitemap) par l'API, qui lit ../frontend/dist/index.html.
    // `npm run dev`, lui, sert son propre index.html : les pages du serveur n'y passent pas.
    proxy: {
      '/api': 'http://localhost:3000',
      '^/$': 'http://localhost:3000',
      '^/(film|cinema)/': 'http://localhost:3000',
      '^/(programme|sitemap\\.xml)$': 'http://localhost:3000',
    },
  },
  test: {
    include: ['src/**/*.test.ts'],
  },
});
