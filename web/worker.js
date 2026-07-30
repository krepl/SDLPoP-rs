// Runs the actual (unmodified) blocking SDLPoP game loop inside a dedicated Worker, so
// blocking it doesn't freeze the browser tab -- see "Phase B" in
// docs/plans/13-platform-architecture-unification.md.
//
// Frames-out only (explicit sequencing decision: frames first, then input). Every finished
// frame reaches here via WasmRenderer::render_present -> post_frame_to_js
// (rust/src/platform/wasm.rs), which calls the Worker's own `postMessage` reflectively --
// this script never calls postMessage itself for frames, it just runs the game.
//
// Input is NOT wired up yet: run_game() never returns, so this Worker's `onmessage` handler
// (if it had one) could never fire while the game is running -- delivering live input into a
// blocking loop needs SharedArrayBuffer/Atomics, a separate not-yet-started piece.
import init, { preload_file, run_game } from './pkg/sdlpop.js';

// Ad hoc list, not a real manifest -- grows one file at a time as testing discovers what the
// startup path actually needs (see the plan's "asset manifest / preload strategy" future
// consideration). Paths are relative to this script's own location (web/), so build_wasm.sh
// symlinks data/ and SDLPoP.ini into web/ the same way scripts/run_harness.sh does for the
// native binary's working directory.
const ASSETS = ['SDLPoP.ini', 'data/icon.png'];

async function main() {
    await init();
    for (const path of ASSETS) {
        const res = await fetch(path);
        if (!res.ok) {
            throw new Error(`failed to fetch ${path}: ${res.status} ${res.statusText}`);
        }
        preload_file(path, new Uint8Array(await res.arrayBuffer()));
    }
    run_game();
}

main().catch((err) => {
    postMessage({ type: 'error', message: String(err && err.stack || err) });
    throw err;
});
