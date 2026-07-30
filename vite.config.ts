import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import type { Plugin } from 'vite';
import { defineConfig } from 'vitest/config';

const developmentApiTarget = process.env.VITE_API_TARGET ?? 'http://127.0.0.1:8742';

function shellServiceWorker(): Plugin {
  return {
    name: 'offline-dental-shell-service-worker',
    apply: 'build',
    generateBundle(_options, bundle) {
      const generatedAssets = Object.keys(bundle)
        .filter((fileName) => !fileName.endsWith('.map'))
        .map((fileName) => `/${fileName}`)
        .sort();
      const shellAssets = [
        '/',
        '/index.html',
        '/manifest.webmanifest',
        '/icons/app-icon.svg',
        ...generatedAssets,
      ].filter((value, index, values) => values.indexOf(value) === index);
      const contentHash = createHash('sha256').update(shellAssets.join('\n'));
      for (const publicAsset of ['manifest.webmanifest', 'icons/app-icon.svg']) {
        contentHash.update(publicAsset);
        contentHash.update(
          readFileSync(new URL(`./public/${publicAsset}`, import.meta.url)),
        );
      }
      for (const [fileName, output] of Object.entries(bundle).sort(([left], [right]) =>
        left.localeCompare(right),
      )) {
        contentHash.update(fileName);
        contentHash.update(output.type === 'chunk' ? output.code : output.source);
      }
      const version = contentHash.digest('hex').slice(0, 12);

      const source = `const CACHE_NAME = 'offline-dental-shell-${version}';
const CACHE_PREFIX = 'offline-dental-shell-';
const SHELL_ASSETS = ${JSON.stringify(shellAssets)};

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(SHELL_ASSETS)).then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys()
      .then((names) => Promise.all(
        names
          .filter((name) => name.startsWith(CACHE_PREFIX) && name !== CACHE_NAME)
          .map((name) => caches.delete(name)),
      ))
      .then(() => self.clients.claim()),
  );
});

function isApiRequest(url) {
  return url.pathname === '/api' || url.pathname.startsWith('/api/');
}

function isShellAsset(url) {
  return url.pathname.startsWith('/assets/') ||
    url.pathname.startsWith('/icons/') ||
    url.pathname === '/manifest.webmanifest';
}

async function networkFirstNavigation(request) {
  const cache = await caches.open(CACHE_NAME);
  try {
    const response = await fetch(request);
    if (response.ok) await cache.put('/index.html', response.clone());
    return response;
  } catch {
    return (await cache.match('/index.html')) || (await cache.match('/')) || Response.error();
  }
}

async function cachedShellAsset(request) {
  const cached = await caches.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) {
    const cache = await caches.open(CACHE_NAME);
    await cache.put(request, response.clone());
  }
  return response;
}

self.addEventListener('fetch', (event) => {
  const request = event.request;
  const url = new URL(request.url);

  if (request.method !== 'GET' || url.origin !== self.location.origin || isApiRequest(url)) {
    return;
  }

  if (request.mode === 'navigate') {
    event.respondWith(networkFirstNavigation(request));
    return;
  }

  if (isShellAsset(url)) {
    event.respondWith(cachedShellAsset(request));
  }
});
`;

      this.emitFile({
        type: 'asset',
        fileName: 'service-worker.js',
        source,
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), tailwindcss(), shellServiceWorker()],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    proxy: {
      '/api': {
        target: developmentApiTarget,
        changeOrigin: true,
        secure: false,
        configure(proxy) {
          proxy.on('proxyReq', (request) => {
            if (request.getHeader('origin')) {
              request.setHeader('origin', new URL(developmentApiTarget).origin);
            }
          });
        },
      },
    },
  },
  build: {
    target: 'es2022',
    sourcemap: false,
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    css: true,
    restoreMocks: true,
  },
});
