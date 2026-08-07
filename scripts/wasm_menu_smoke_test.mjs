#!/usr/bin/env node
// Regression test: opening the pause menu in the wasm build must not crash the Worker.
//
// This is the test that would have caught the real bug (WasmRenderer::get_window_flags/
// show_cursor/set_fullscreen/get_scancode_name were unimplemented!() stubs, hit for the
// first time by a live user pressing Esc -- see project_wasm_esc_menu_crash memory /
// commit d20c68e). Nothing else exercises the pause menu: scripts/run_harness.sh's
// replays never press Escape, and scripted-input testing (scripts/gameplay_smoke_test.sh)
// only drives movement. See scripts/menu_smoke_test.sh for the native counterpart --
// native's real SDL already implements everything the menu needs, so that side was never
// actually at risk; this wasm side is the one that matters.
//
// Drives the scripted-input mechanism (rust/src/seg009.rs's load_scripted_input, ported
// this session to work on wasm too) via web/headless.mjs's runHeadlessScriptedInput.
// scripts/scripted_inputs/open_menu.txt opens the menu and, deliberately, never closes
// it again (see that file's header comment for why a scripted close isn't feasible) --
// wasm is single-threaded, so once the game is stuck polling for input inside the menu,
// this page's JS event loop never runs again either, meaning a JS-side timeout can't
// fire. Racing the page.evaluate() call against a *Node*-side timer works instead, since
// Node's timer runs in a separate process from the (blocked) browser tab -- "the timer
// won the race, no crash was reported yet" is treated as PASS, same inverted-pass-
// condition reasoning as menu_smoke_test.sh's timeout-exit-code check.
//
// Usage: node scripts/wasm_menu_smoke_test.mjs

import { chromium } from 'playwright';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { serve } from './wasm_test_server.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const WEB_DIR = path.join(ROOT, 'web');
const TIMEOUT_MS = 6000;

async function main() {
    if (!fs.existsSync(path.join(WEB_DIR, 'pkg', 'sdlpop.js'))) {
        console.error('web/pkg/sdlpop.js not found -- run `cargo xtask wasm-build` first.');
        process.exit(2);
    }

    const scriptText = fs.readFileSync(
        path.join(ROOT, 'scripts', 'scripted_inputs', 'open_menu.txt'),
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

        console.log('== open_menu (wasm): pressing Escape must not crash the Worker ==');

        // "megahit 3" matches scripts/gameplay_smoke_test.sh's native invocation --
        // without it the run sits at the title screen/intro cutscene, where Escape
        // doesn't reach process_key()'s pause-menu handling at all, and this test would
        // pass even against the broken code (confirmed empirically: it did, silently,
        // before this args fix was added). "headless" is also included -- that flag used
        // to make seg000.rs call std::env::set_var (no wasm32-unknown-unknown backing,
        // panicked outright there) to force dummy SDL video/audio drivers; now uses the
        // shared setenv instead (a correct no-op on wasm, since nothing there ever reads
        // SDL_VIDEODRIVER/SDL_AUDIODRIVER). Including it here doubles as this fix's
        // regression test, and matches scripts/gameplay_smoke_test.sh's native invocation
        // ("headless megahit 3") exactly, rather than a wasm-only subset of the args.
        const run = page.evaluate(
            ([argv, script]) => window.runHeadlessScriptedInput(argv, script, {}),
            [['prince', 'headless', 'megahit', '3'], scriptText],
        );
        const timeout = new Promise((resolve) => setTimeout(() => resolve({ timedOut: true }), TIMEOUT_MS));

        const result = await Promise.race([run, timeout]);
        if (result && result.timedOut) {
            console.log(`PASS: menu opened and stayed open for ${TIMEOUT_MS}ms without crashing`);
            process.exit(0);
        } else {
            console.log(`FAIL: expected the run to hang open (timeout), but it returned: ${JSON.stringify(result)}`);
            process.exit(1);
        }
    } catch (err) {
        console.log(`FAIL: ${err}`);
        process.exit(1);
    } finally {
        // The page may still be synchronously blocked inside the wasm call (that's the
        // expected "hung open" state) -- browser.close() tears down the whole context
        // regardless, no need to wait for the page itself to become responsive again.
        await browser.close();
        server.close();
    }
}

main();
