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
// Compares against the same golden file scripts/menu_mouse_navigation_test.sh uses
// (traces/menu_pixels/menu_mouse_navigation.pixels, generated from the C oracle) -- with a
// fixed `seed=42` (torch-flicker animation, and anything else driven by prandom(), is
// otherwise seeded from wall-clock time, which would make hashes differ between any two runs
// for reasons that have nothing to do with a real rendering bug), wasm genuinely produces
// byte-identical hashes to both native Rust and the C oracle. Confirmed this once the residual
// alpha-blend rounding gap (project_wasm_menu_alpha_blend_bug memory) turned out to matter far
// less than the earlier non-seeded comparison suggested -- most of that comparison's diff was
// actually torch-flicker seed nondeterminism, not the blend formula.
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
            [['prince', 'headless', 'megahit', '3', 'seed=42'], scriptText, { POPMENUPIXELS_OUT: 'menu_pixels.out' }],
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
        const text = Buffer.from(result.r.files.POPMENUPIXELS_OUT || '', 'base64').toString('utf8').trim();
        const golden = fs.readFileSync(
            path.join(ROOT, 'traces', 'menu_pixels', 'menu_mouse_navigation.pixels'),
            'utf8',
        ).trim();
        if (text !== golden) {
            console.log('FAIL: menu-frame pixel hashes diverged from the golden (C oracle) trace');
            console.log('--- got ---');
            console.log(text);
            console.log('--- want ---');
            console.log(golden);
            process.exit(1);
        }
        console.log('PASS: exited cleanly, menu-frame pixel hashes match the golden (C oracle) trace');
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
