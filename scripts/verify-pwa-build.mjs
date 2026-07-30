import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { stdout } from 'node:process';

const root = resolve(import.meta.dirname, '..');
const dist = join(root, 'dist');
const required = ['index.html', 'manifest.webmanifest', 'service-worker.js'];

for (const file of required) {
  if (!existsSync(join(dist, file))) {
    throw new Error(`Build PWA incompleto: dist/${file} não foi gerado.`);
  }
}

function filesBelow(directory) {
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name);
    return statSync(path).isDirectory() ? filesBelow(path) : [path];
  });
}

if (filesBelow(dist).some((path) => path.endsWith('.map'))) {
  throw new Error('Build de produção contém sourcemap público.');
}

const worker = readFileSync(join(dist, 'service-worker.js'), 'utf8');
for (const forbidden of [
  'indexedDB',
  'localStorage',
  'sessionStorage',
  'sync.register',
]) {
  if (worker.includes(forbidden)) {
    throw new Error(`Service Worker contém capacidade proibida: ${forbidden}.`);
  }
}

const fetchHandler = worker.indexOf("self.addEventListener('fetch'");
const apiGuard = worker.indexOf('isApiRequest(url)', fetchHandler);
const firstRespondWith = worker.indexOf('event.respondWith', fetchHandler);
if (
  fetchHandler < 0 ||
  apiGuard < fetchHandler ||
  firstRespondWith < 0 ||
  apiGuard > firstRespondWith
) {
  throw new Error('Service Worker não prova o bypass de /api antes de qualquer cache.');
}
if (!worker.includes("url.pathname.startsWith('/api/')")) {
  throw new Error('Service Worker não exclui toda a árvore /api/.');
}

const manifest = JSON.parse(readFileSync(join(dist, 'manifest.webmanifest'), 'utf8'));
if (!Array.isArray(manifest.icons) || manifest.icons.length === 0) {
  throw new Error('Manifest PWA não contém ícone local.');
}
if (manifest.start_url !== '/' || manifest.scope !== '/') {
  throw new Error('Manifest PWA possui start_url/scope inesperados.');
}

stdout.write(
  'PWA validada: shell local presente, sem sourcemaps e /api fora do cache.\n',
);
