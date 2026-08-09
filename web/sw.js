// PWA offline support. Precaches the app shell (index.html/worker.js/pkg/*) plus every file
// listed in data_manifest.txt (~4.3MB/~1000 files -- see web/worker.js's preload comment for
// why that size makes whole-game precache the right call, same reasoning applies here) so the
// game boots and plays fully offline after the first successful load.
//
// Versioning: cache-name-bump, not per-file content hashing. Bump CACHE_NAME on any change to
// the app shell or to data/ contents; `activate` deletes every other sdlpop-cache-* entry, so
// old assets never linger. This is the standard, simplest approach for a small fixed asset set
// -- see memory project_wasm_opfs_persistence.md-adjacent notes for why per-file hashing (the
// alternative) wasn't worth it here.
const CACHE_NAME = 'sdlpop-cache-v1';

const APP_SHELL = [
    './',
    'index.html',
    'worker.js',
    'manifest.webmanifest',
    'icons/icon-192.png',
    'icons/icon-512.png',
    'pkg/sdlpop.js',
    'pkg/sdlpop_bg.wasm',
    'SDLPoP.ini',
    'data_manifest.txt',
];

async function fetchDataManifest() {
    const res = await fetch('data_manifest.txt');
    if (!res.ok) return [];
    return (await res.text()).split('\n').map((l) => l.trim()).filter(Boolean);
}

async function precacheAll(cache, paths) {
    // Bounded-concurrency cache.add pool -- same shape as worker.js's asset preload pool, for
    // the same reason (politeness to the server, not a hard technical limit).
    const CONCURRENCY = 24;
    let next = 0;
    async function worker() {
        while (next < paths.length) {
            const path = paths[next++];
            try {
                await cache.add(path);
            } catch (e) {
                console.warn(`sw: failed to precache ${path}:`, e);
            }
        }
    }
    await Promise.all(Array.from({ length: CONCURRENCY }, worker));
}

self.addEventListener('install', (event) => {
    event.waitUntil((async () => {
        const cache = await caches.open(CACHE_NAME);
        await precacheAll(cache, APP_SHELL);
        const dataPaths = await fetchDataManifest();
        await precacheAll(cache, dataPaths);
        await self.skipWaiting();
    })());
});

self.addEventListener('activate', (event) => {
    event.waitUntil((async () => {
        const names = await caches.keys();
        await Promise.all(
            names.filter((n) => n !== CACHE_NAME).map((n) => caches.delete(n))
        );
        await self.clients.claim();
    })());
});

self.addEventListener('fetch', (event) => {
    if (event.request.method !== 'GET') return;
    event.respondWith((async () => {
        const cached = await caches.match(event.request);
        if (cached) return cached;
        try {
            const res = await fetch(event.request);
            // Cache successful same-origin responses opportunistically, so anything not in
            // APP_SHELL/data_manifest.txt at precache time (e.g. a future asset) still ends up
            // available offline after one online visit.
            if (res.ok && new URL(event.request.url).origin === self.location.origin) {
                const cache = await caches.open(CACHE_NAME);
                cache.put(event.request, res.clone());
            }
            return res;
        } catch (e) {
            // Offline and not in cache -- nothing more we can do for this request.
            throw e;
        }
    })());
});
