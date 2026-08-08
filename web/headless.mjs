// scripts/wasm_pixel_harness.mjs's driver page. Loads the wasm module, preloads every
// game asset plus one replay file, runs it in `validate` mode (deterministic recorded
// input, no live keyboard/canvas needed -- see run_game_with_args's doc comment,
// rust/src/lib.rs), and hands the resulting POPTRACE_OUT/POPPIXELS_OUT dumps back to
// Node. One page load runs exactly one replay: the wasm module's diagnostic dumps
// (dump_frame_state/dump_frame_pixels) are "open the output file once" by design, same
// as the native build's process-per-run model, so a second replay needs a fresh page
// (fresh wasm instance), not a second call into this same one.
import init, { preload_file, run_game_with_args, resume_game_after_restart, wasm_setenv, read_vfs_file } from './pkg/sdlpop.js';

// Matches wasm_libc::EXIT_SIGNAL (rust/src/wasm_libc.rs) exactly.
const EXIT_SIGNAL = 'SDLPOP_EXIT';
// Matches seg000::RESTART_SIGNAL exactly -- an ordinary in-game restart (death, a level
// timer expiring, "press any key" at the title screen, ...), not an error. worker.js's
// runWithRestartRetries handles this same signal for the live interactive build; a
// validated replay can trigger it too (e.g. time_limit_expiry_lvl3.p1r's timer running
// out mid-level), so this driver needs the same retry loop, not just the single
// run_game_with_args call a restart-free replay would need.
const RESTART_SIGNAL = 'SDLPOP_RESTART';

function isExitSignal(e) {
    return e instanceof Error && e.message === EXIT_SIGNAL;
}

function isRestartSignal(e) {
    return e instanceof Error && e.message === RESTART_SIGNAL;
}

// Same manifest/preload approach as worker.js -- see that file's comment for why it's
// "fetch everything up front" rather than lazy per-level loading.
async function fetchManifest() {
    const res = await fetch('data_manifest.txt');
    if (!res.ok) {
        throw new Error(`failed to fetch data_manifest.txt: ${res.status} ${res.statusText}`);
    }
    return (await res.text()).split('\n').map((l) => l.trim()).filter(Boolean);
}

async function preloadAsset(path) {
    const res = await fetch(path);
    if (!res.ok) {
        throw new Error(`failed to fetch ${path}: ${res.status} ${res.statusText}`);
    }
    preload_file(path, new Uint8Array(await res.arrayBuffer()));
}

function base64ToBytes(b64) {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return bytes;
}

function bytesToBase64(bytes) {
    let bin = '';
    // String.fromCharCode.apply chokes on very large arrays; chunk it.
    const CHUNK = 0x8000;
    for (let i = 0; i < bytes.length; i += CHUNK) {
        bin += String.fromCharCode.apply(null, bytes.subarray(i, i + CHUNK));
    }
    return btoa(bin);
}

// replayVfsPath: the path preload_file registers the replay under, and the path passed
// as the `validate` argument -- must match (see check_param, rust/src/seg009.rs).
// env: e.g. { POPTRACE_OUT: 'test.trace', POPPIXELS_OUT: 'test.pixels' }.
window.runHeadlessReplay = async function (replayVfsPath, replayBytesBase64, env) {
    await init();

    const paths = await fetchManifest();
    const CONCURRENCY = 24;
    let next = 0;
    async function worker() {
        while (next < paths.length) {
            const path = paths[next++];
            await preloadAsset(path);
        }
    }
    await Promise.all(Array.from({ length: CONCURRENCY }, worker));

    preload_file(replayVfsPath, base64ToBytes(replayBytesBase64));
    for (const [k, v] of Object.entries(env)) {
        wasm_setenv(k, v);
    }

    // Same run-then-retry-on-restart shape as worker.js's runWithRestartRetries: the
    // first call is run_game_with_args (one-time setup: assets, argv, SDL_Init, ...),
    // every retry after an in-game restart is resume_game_after_restart (re-enters
    // directly at the game's restart point, skipping that one-time setup, which must
    // not run twice -- see that function's doc comment, rust/src/lib.rs).
    let first = true;
    for (;;) {
        try {
            if (first) {
                first = false;
                run_game_with_args(['prince', 'validate', replayVfsPath]);
            } else {
                resume_game_after_restart();
            }
            // A validated replay always ends by calling C's exit(), which throws
            // EXIT_SIGNAL -- returning normally means pop_main() exited some other way
            // (a real bug, e.g. the replay file failed to load and validate mode never
            // even started).
            throw new Error('run_game_with_args returned without exit() -- did validate mode start?');
        } catch (e) {
            if (isExitSignal(e)) break;
            if (!isRestartSignal(e)) throw e;
        }
    }

    return {
        trace: bytesToBase64(read_vfs_file(env.POPTRACE_OUT || '')),
        pixels: bytesToBase64(read_vfs_file(env.POPPIXELS_OUT || '')),
        // Only populated when POPRAWFRAME_TICK/POPRAWFRAME_OUT are set (see
        // dump_frame_raw, rust/src/state_dump.rs) -- a one-shot single-frame capture
        // for visual debugging, empty otherwise.
        raw: bytesToBase64(read_vfs_file(env.POPRAWFRAME_OUT || '')),
    };
};

// Live scripted-keyboard-input mode, for exercising code paths a recorded replay can't
// reach (anything that isn't part of normal deterministic gameplay -- the pause menu is
// the motivating case: see project_wasm_esc_menu_crash memory / commit d20c68e). Reuses
// the same POPTRACE_INPUT mechanism the native build's scripts/scripted_inputs/*.txt
// scripts already use (load_scripted_input, rust/src/seg009.rs -- ported this session to
// read via getenv/fopen instead of std::env/std::fs specifically so it would work here
// unchanged). argv is passed straight to run_game_with_args, e.g.
// ['prince', 'headless', 'megahit', '3'] to match a native scripted-input invocation.
//
// Unlike runHeadlessReplay, this deliberately does NOT *require* EXIT_SIGNAL as the success
// signal -- a script that opens the pause menu and never closes it again (the common case;
// see scripts/scripted_inputs/open_menu.txt's header comment for why a scripted close wasn't
// feasible there) makes the game hang forever on purpose, the same way it would waiting for a
// real keyboard. wasm is single-threaded: once run_game_with_args is called, a promise that
// never settles means the JS event loop never gets to run anything else on this page again,
// including a JS-side timeout -- so a hang here is only detectable from OUTSIDE the page.
// scripts/wasm_menu_smoke_test.mjs races this call against a Node-side timer instead, which
// works because Node's timer runs in a different process from the (blocked) browser tab.
//
// A script CAN end with a real exit now (scripts/scripted_inputs/menu_mouse_navigation.txt,
// Phase D item 4 -- mouse-driven menu navigation all the way to a real QUIT GAME + confirm),
// and that path resolves normally with `env`'s output files read back via read_vfs_file,
// mirroring runHeadlessReplay -- this is the ONLY way to retrieve POPMENUPIXELS_OUT/etc.
// contents from a scripted-input run on wasm: read_vfs_file is itself a wasm_bindgen call,
// and nothing (including it) can run again on this page once run_game_with_args is
// permanently blocked inside a script that never exits, so a hanging script's caller must
// still race this promise externally and cannot recover its output afterward.
window.runHeadlessScriptedInput = async function (argv, scriptText, env) {
    await init();

    const paths = await fetchManifest();
    const CONCURRENCY = 24;
    let next = 0;
    async function worker() {
        while (next < paths.length) {
            const path = paths[next++];
            await preloadAsset(path);
        }
    }
    await Promise.all(Array.from({ length: CONCURRENCY }, worker));

    const scriptVfsPath = 'input.txt';
    preload_file(scriptVfsPath, new TextEncoder().encode(scriptText));
    wasm_setenv('POPTRACE_INPUT', scriptVfsPath);
    for (const [k, v] of Object.entries(env || {})) {
        wasm_setenv(k, v);
    }

    let first = true;
    for (;;) {
        try {
            if (first) {
                first = false;
                run_game_with_args(argv);
            } else {
                resume_game_after_restart();
            }
            break; // pop_main() itself returned -- not the usual EXIT_SIGNAL path, but still "exited"
        } catch (e) {
            if (isExitSignal(e)) break;
            if (!isRestartSignal(e)) throw e;
        }
    }

    const files = {};
    for (const [k, v] of Object.entries(env || {})) {
        if (k.endsWith('_OUT')) {
            files[k] = bytesToBase64(read_vfs_file(v));
        }
    }
    return { exited: true, files };
};

window.__headlessReady = true;
