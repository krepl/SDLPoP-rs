#!/usr/bin/env node
// Runs one replay through the wasm build headlessly (via Playwright + Chromium) in
// `validate` mode, the same way scripts/run_harness.sh runs the native binary, and
// writes out the resulting POPTRACE_OUT/POPPIXELS_OUT dumps -- see web/headless.mjs for
// what actually executes inside the page, and run_game_with_args's doc comment
// (rust/src/lib.rs) for why this is possible at all (the wasm build previously only
// took live keyboard input, with no replay-file loading path).
//
// Usage:
//   node scripts/wasm_pixel_harness.mjs <replay.p1r> <out.trace> <out.pixels>
//
// Requires `cargo xtask wasm-build` to have been run against current sources (this
// script does not rebuild automatically -- callers control that, same as
// scripts/run_harness.sh not rebuilding the Rust binary itself).

import { chromium } from 'playwright';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { serve } from './wasm_test_server.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const WEB_DIR = path.join(ROOT, 'web');

async function main() {
    const [replayArg, traceOutArg, pixelsOutArg] = process.argv.slice(2);
    if (!replayArg || !traceOutArg || !pixelsOutArg) {
        console.error('Usage: node scripts/wasm_pixel_harness.mjs <replay.p1r> <out.trace> <out.pixels>');
        process.exit(2);
    }
    const replayPath = path.resolve(ROOT, replayArg);
    const traceOutPath = path.resolve(ROOT, traceOutArg);
    const pixelsOutPath = path.resolve(ROOT, pixelsOutArg);
    if (!fs.existsSync(path.join(WEB_DIR, 'pkg', 'sdlpop.js'))) {
        console.error('web/pkg/sdlpop.js not found -- run `cargo xtask wasm-build` first.');
        process.exit(2);
    }

    const server = await serve(WEB_DIR, 'headless.html');
    const port = server.address().port;
    const browser = await chromium.launch();
    try {
        const page = await browser.newPage();
        page.on('console', (msg) => {
            if (process.env.WASM_HARNESS_VERBOSE) {
                console.error(`[page] ${msg.text()}`);
            }
        });
        page.on('pageerror', (err) => {
            console.error(`[pageerror] ${err}`);
        });

        await page.goto(`http://127.0.0.1:${port}/headless.html`);
        await page.waitForFunction('window.__headlessReady === true');

        const replayBytes = fs.readFileSync(replayPath);
        const replayB64 = replayBytes.toString('base64');
        const replayVfsPath = 'replay.p1r';

        const result = await page.evaluate(
            ([vfsPath, b64, env]) => window.runHeadlessReplay(vfsPath, b64, env),
            [replayVfsPath, replayB64, { POPTRACE_OUT: 'test.trace', POPPIXELS_OUT: 'test.pixels' }],
        );

        fs.mkdirSync(path.dirname(traceOutPath), { recursive: true });
        fs.mkdirSync(path.dirname(pixelsOutPath), { recursive: true });
        fs.writeFileSync(traceOutPath, Buffer.from(result.trace, 'base64'));
        fs.writeFileSync(pixelsOutPath, Buffer.from(result.pixels, 'base64'));
        console.log(`Wrote ${traceOutPath} (${fs.statSync(traceOutPath).size} bytes)`);
        console.log(`Wrote ${pixelsOutPath} (${fs.statSync(pixelsOutPath).size} bytes)`);
    } finally {
        await browser.close();
        server.close();
    }
}

main().catch((err) => {
    console.error(err);
    process.exit(1);
});
