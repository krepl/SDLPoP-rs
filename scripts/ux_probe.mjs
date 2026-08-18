// Headless UX probe for the *interactive* wasm build (web/index.html), as opposed to
// scripts/wasm_pixel_harness.mjs which drives the deterministic `validate`-mode replay page.
//
// Why this exists: every automated check in this repo (replay traces, pixel hashes, smoke
// tests) runs through validate mode or scripted input. Nothing exercises the live
// human-facing surface -- menus, pause, fullscreen, save/load, error paths. This drives that
// surface headlessly so it can be regression-tested without a human at a keyboard.
//
// Usage:  node ux_probe.mjs <scenario.json> <out-dir>
//
// Scenario is a JSON array of steps:
//   { "key": "Escape", "hold": 150, "wait": 800 }   press/release one key
//   { "chord": ["ControlLeft", "KeyA"], "hold": 250, "wait": 3000 }
//   { "wait": 2000 }
//   { "shot": "menu-open" }                          save <out-dir>/menu-open.png
//   { "eval": "() => ...", "as": "label" }           run JS, record result
//
// Keys are KeyboardEvent.code values, matching web/index.html's SCANCODE map.
//
// Notes on fidelity: key events are dispatched synthetically (dispatchEvent) rather than via
// Playwright's real input, because a real page.keyboard.press() is a ~0ms down/up pair and
// the shared-input transport is level-sampled -- synthesize_key_edge_events
// (rust/src/platform/wasm.rs) diffs buffer levels against a PREV snapshot, so a press that
// begins and ends between two polls produces no event at all. Holding for a realistic
// duration is what a human keyboard actually does.

import { chromium, firefox, webkit } from 'playwright';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [scenarioPath, outDir] = process.argv.slice(2);
if (!scenarioPath || !outDir) {
    console.error('usage: node ux_probe.mjs <scenario.json> <out-dir>');
    process.exit(2);
}

const steps = JSON.parse(readFileSync(scenarioPath, 'utf8'));
mkdirSync(outDir, { recursive: true });

const URL = process.env.UX_PROBE_URL || 'http://localhost:8642/';
const results = [];
const consoleErrors = [];

// A default launch() gets a throwaway profile, so OPFS starts empty every run -- fine for
// most scenarios, useless for testing that saves survive a reload. Set UX_PROBE_PROFILE to a
// directory to reuse one profile across runs, which is what makes cross-session persistence
// (quicksave/HOF/config via OPFS, rust/src/wasm_persist.rs) actually testable.
// UX_PROBE_BROWSER selects the engine. Worth exercising: the wasm build needs
// SharedArrayBuffer (so cross-origin isolation) and, for pausing in fullscreen, either the
// Keyboard Lock API (Chromium only) or the KeyP/Backspace fallbacks -- all of which differ
// per engine, and none of which Chromium-only testing would ever catch.
const BROWSERS = { chromium, firefox, webkit };
const browserName = process.env.UX_PROBE_BROWSER || 'chromium';
const browserType = BROWSERS[browserName];
if (!browserType) {
    console.error(`unknown UX_PROBE_BROWSER: ${browserName} (chromium|firefox|webkit)`);
    process.exit(2);
}

const profileDir = process.env.UX_PROBE_PROFILE;
let browser = null;
let context;
if (profileDir) {
    context = await browserType.launchPersistentContext(profileDir, {}); // headless by default
} else {
    browser = await browserType.launch();
    context = await browser.newContext();
}
const page = context.pages()[0] ?? (await context.newPage());

page.on('console', (m) => {
    if (m.type() === 'error') consoleErrors.push(m.text());
});
page.on('pageerror', (e) => consoleErrors.push(`pageerror: ${e.message}`));

await page.goto(URL);

// A previously-installed service worker will happily serve a stale pkg/sdlpop.js across
// reloads, silently masking a freshly built wasm bundle. Clear it before every run.
await page.evaluate(async () => {
    const regs = await navigator.serviceWorker.getRegistrations();
    for (const r of regs) await r.unregister();
    for (const k of await caches.keys()) await caches.delete(k);
});
await page.goto(URL);

// Belt-and-braces silence: headless Chromium produces no audio output anyway, but route
// game audio through a zero-gain node and count buffers so "did audio play?" stays testable.
await page.addInitScript(() => {
    window.__audioBuffers = 0;
    const origConnect = AudioBufferSourceNode.prototype.connect;
    AudioBufferSourceNode.prototype.connect = function (...rest) {
        const ctx = this.context;
        if (!ctx.__silentSink) {
            const g = ctx.createGain();
            g.gain.value = 0;
            origConnect.call(g, ctx.destination);
            ctx.__silentSink = g;
        }
        window.__audioBuffers++;
        return origConnect.call(this, ctx.__silentSink);
    };
});
await page.goto(URL);

// The start overlay exists to unlock audio and take keyboard focus in one gesture.
await page.locator('#start-overlay').click();

// Wait for the preload to finish and the first real frame to arrive.
await page.waitForFunction(
    () => /^frame /.test(document.getElementById('status')?.textContent ?? ''),
    null,
    { timeout: 120_000 },
);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function fireKey(codes, hold) {
    await page.evaluate(
        async ({ codes, hold }) => {
            const fire = (type, code) =>
                window.dispatchEvent(
                    new KeyboardEvent(type, { code, bubbles: true, cancelable: true }),
                );
            for (const c of codes) {
                fire('keydown', c);
                await new Promise((r) => setTimeout(r, 60));
            }
            await new Promise((r) => setTimeout(r, hold));
            for (const c of [...codes].reverse()) {
                fire('keyup', c);
                await new Promise((r) => setTimeout(r, 60));
            }
        },
        { codes, hold },
    );
}

async function shot(name) {
    const dataUrl = await page.evaluate(() =>
        document.querySelector('canvas').toDataURL('image/png'),
    );
    const b64 = dataUrl.split(',', 2)[1];
    writeFileSync(join(outDir, `${name}.png`), Buffer.from(b64, 'base64'));
    results.push({ shot: name });
}

for (const step of steps) {
    if (step.key) await fireKey([step.key], step.hold ?? 150);
    if (step.chord) await fireKey(step.chord, step.hold ?? 250);
    if (step.eval) {
        // Playwright evaluates a bare string as an *expression*, so "() => ..." would just
        // evaluate to a function object and return undefined. Wrap it in a call.
        const value = await page.evaluate(`(${step.eval})()`);
        results.push({ label: step.as ?? 'eval', value });
    }
    if (step.wait) await sleep(step.wait);
    if (step.shot) await shot(step.shot);
}

results.push({ audioBuffers: await page.evaluate(() => window.__audioBuffers) });
results.push({ consoleErrors });

writeFileSync(join(outDir, 'results.json'), JSON.stringify(results, null, 2));
console.log(JSON.stringify(results, null, 2));

await context.close();
if (browser) await browser.close();
