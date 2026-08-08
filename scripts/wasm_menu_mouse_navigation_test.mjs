#!/usr/bin/env node
// Regression test: mouse-driven pause-menu navigation must actually work in the wasm build
// (Phase D items 3/4, docs/plans/13-platform-architecture-unification.md), not just avoid
// crashing (that's wasm_menu_smoke_test.mjs's job). Runs
// scripts/scripted_inputs/menu_mouse_navigation.txt, which opens the menu, hovers and clicks
// SETTINGS, backs out, hovers and clicks QUIT GAME, hovers and clicks OK -- ending in a real
// process exit, unlike open_menu.txt's deliberate hang (see that script's header). Asserts
// the run actually exits (not a timeout) and that POPMENUPIXELS_OUT captured a sane number of
// frames -- the same mechanism that caught a real alpha-blend rounding bug in WasmRenderer
// during this feature's own development (project_wasm_menu_alpha_blend_bug memory).
//
// Deliberately does NOT assert the captured hashes match scripts/menu_mouse_navigation_test.sh's
// native run byte-for-byte: a small, already-investigated rounding residual remains between
// the two backends' alpha-blend math (see that memory) alongside ordinary pixel-level
// nondeterminism (the torch's animated flicker color isn't scripted/seeded the same way
// between runs) -- exact hash parity isn't the right test here. Real backend-agreement
// coverage for the blend math itself lives in rust/src/platform/pixel_parity_tests.rs's
// blended_blit_* tests, which run headless and don't have this timing/rendering-path noise.
//
// Usage: node scripts/wasm_menu_mouse_navigation_test.mjs

import { chromium } from 'playwright';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { serve } from './wasm_test_server.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const WEB_DIR = path.join(ROOT, 'web');
const TIMEOUT_MS = 15000;

async function main() {
    if (!fs.existsSync(path.join(WEB_DIR, 'pkg', 'sdlpop.js'))) {
        console.error('web/pkg/sdlpop.js not found -- run `cargo xtask wasm-build` first.');
        process.exit(2);
    }

    const scriptText = fs.readFileSync(
        path.join(ROOT, 'scripts', 'scripted_inputs', 'menu_mouse_navigation.txt'),
        'utf8',
    );

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

        await page.goto(`http://127.0.0.1:${port}/headless.html`);
        await page.waitForFunction('window.__headlessReady === true');

        console.log('== menu_mouse_navigation (wasm): mouse-driven menu nav must exit cleanly ==');

        const run = page.evaluate(
            ([argv, script, env]) => window.runHeadlessScriptedInput(argv, script, env),
            [['prince', 'headless', 'megahit', '3'], scriptText, { POPMENUPIXELS_OUT: 'menu_pixels.out' }],
        ).then((r) => ({ ok: true, r })).catch((e) => ({ ok: false, e: String(e) }));
        const timeout = new Promise((resolve) => setTimeout(() => resolve({ timedOut: true }), TIMEOUT_MS));

        const result = await Promise.race([run, timeout]);
        if (result.timedOut) {
            console.log(`FAIL: expected a clean exit within ${TIMEOUT_MS}ms, timed out instead`);
            process.exit(1);
        }
        if (!result.ok) {
            console.log(`FAIL: ${result.e}`);
            process.exit(1);
        }
        const text = Buffer.from(result.r.files.POPMENUPIXELS_OUT || '', 'base64').toString('utf8');
        const lines = text.trim().split('\n').filter(Boolean);
        if (lines.length < 3) {
            console.log(`FAIL: expected at least 3 captured menu frames, got ${lines.length}`);
            console.log(text);
            process.exit(1);
        }
        console.log(`PASS: exited cleanly, captured ${lines.length} menu frames`);
        process.exit(0);
    } catch (err) {
        console.log(`FAIL: ${err}`);
        process.exit(1);
    } finally {
        await browser.close();
        server.close();
    }
}

main();
