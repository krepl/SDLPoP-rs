// Shared static file server for the wasm test drivers (scripts/wasm_pixel_harness.mjs,
// scripts/wasm_menu_smoke_test.mjs). Serves web/ with the same COOP/COEP headers
// xtask/src/wasm.rs's `wasm-serve` sets (see that file for why they're required --
// SharedArrayBuffer's cross-origin-isolation requirement; these headless drivers don't
// use SharedArrayBuffer themselves, but it's the same wasm bundle that expects them, and
// harmless either way for a local test server).
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

const MIME = {
    '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript',
    '.wasm': 'application/wasm', '.json': 'application/json', '.txt': 'text/plain',
    '.ini': 'text/plain', '.dat': 'application/octet-stream', '.DAT': 'application/octet-stream',
};

// Resolves to a listening server bound to an OS-chosen port (server.address().port).
// defaultPage: served for `/` (e.g. 'headless.html').
export function serve(webDir, defaultPage) {
    const server = http.createServer((req, res) => {
        const urlPath = decodeURIComponent(req.url.split('?')[0]);
        const filePath = path.join(webDir, urlPath === '/' ? `/${defaultPage}` : urlPath);
        if (!filePath.startsWith(webDir)) {
            res.writeHead(403);
            res.end();
            return;
        }
        fs.readFile(filePath, (err, data) => {
            if (err) {
                res.writeHead(404);
                res.end();
                return;
            }
            const ext = path.extname(filePath);
            res.writeHead(200, {
                'Content-Type': MIME[ext] || 'application/octet-stream',
                'Cross-Origin-Opener-Policy': 'same-origin',
                'Cross-Origin-Embedder-Policy': 'require-corp',
                'Cache-Control': 'no-store',
            });
            res.end(data);
        });
    });
    return new Promise((resolve) => {
        server.listen(0, '127.0.0.1', () => resolve(server));
    });
}
