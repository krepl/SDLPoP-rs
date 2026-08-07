# Plan: Full SDL encapsulation + unified native/web architecture

## Context

Phase 2 (WASM) milestone 1 got `wasm32-unknown-unknown` to link a real binary (commit
`98ae3a0` onward) and proved the wasm-bindgen/canvas JS pipeline works end to end (a test
gradient renders in a real browser via `web/index.html`). Trying to actually get `pop_main()`
rendering surfaced three real architectural walls, none fatal, all requiring deliberate
design rather than a quick shim:

1. **`SDL_Surface`/`SDL_PixelFormat` fields are read directly outside the platform layer.**
   `seg008.rs`, `seg009.rs`, `menu.rs`, `lighting.rs`, `midi.rs` all dereference `.w`/`.h`/
   `.pixels`/`.pitch`/`.format` (and the format's own `.BitsPerPixel`/`.Rmask`/`.palette`/etc.)
   directly — roughly 75 call sites. `Renderer::create_surface` can't hand back an opaque
   handle as-is; a real `WasmRenderer` would need to fake genuine SDL memory layout to satisfy
   these reads.
2. **Blocking C-style game loop vs. the browser's non-blocking event loop.** `pop_main()`
   runs one blocking loop with `SDL_Delay`-style pacing — incompatible with a browser main
   thread.
3. **Synchronous file I/O vs. `fetch`'s async API.** DAT-file loading is still raw blocking
   `fopen`/`fread` (not yet routed through the `FileSystem` trait, despite that trait
   existing since Step C) — no synchronous equivalent exists in a browser for arbitrary
   asset loading.

**Decision:** rather than build a divergent, web-specific hack for each of these, fix the
root cause — game logic is still too tightly coupled to SDL specifics. Fully encapsulate SDL
behind the `Platform` trait boundary (Step C's original goal, not yet finished for surfaces
specifically), and design the *unified* shape once, so native and web share one
implementation with different backends — not two forked implementations. Where the "ideal web
architecture" and "the current native architecture" disagree, prefer whichever shape works
well on **both** targets, even if it means changing native's internals too, as long as
native's *observable behavior* (the differential trace) doesn't change.

**Considered and declined: an existing "SDL for wasm" crate.** No such thing exists for
`wasm32-unknown-unknown` (only via Emscripten's `wasm32-unknown-emscripten`, already passed
on — it fights the wasm-bindgen pipeline already working and needs a separate SDK). The
`Surface`/`PixelFormat`/palette/blit semantics being encapsulated below are inherent to this
specific game's own renderer conventions (indexed 8bpp sprites, `backtable`/`foretable`/
`midtable` double-buffering, color-key transparency), not generic 2D graphics — no crate
provides that "for free" regardless of platform. See "Future consideration" at the end of
this doc for the one piece that *is* genuinely generic (final buffer-to-screen presentation)
and could plausibly use an existing crate later.

**Non-negotiable throughout:** every phase below touches only the platform *boundary*
(`Renderer`/`AudioBackend`/`InputSource`/`FileSystem` trait implementations and call sites),
never gameplay/simulation logic itself. The 30-replay differential-trace harness stays the
authoritative correctness oracle for native the entire time — if a phase is truly "pure
relocation," the harness should pass unchanged after every commit, same discipline as Step
C/D.

---

## Phase A — Surface/PixelFormat encapsulation + pixel-parity test harness — ✅ DONE

**Complete** (commits `707382e` through `6d7cb45`). Five new `Renderer` accessor methods
(`surface_size`/`surface_pitch`/`surface_pixels`/`surface_format_info`/`surface_palette`,
plus a `format: u32` field added to `PixelFormatInfo` mid-phase for
`SDL_ISPIXELFORMAT_INDEXED`) replaced every direct `SDL_Surface`/`SDL_PixelFormat` field
access outside `platform/*.rs` — confirmed by a crate-wide grep sweep with zero hits left
(the only remaining matches are `SDL_Rect`, a plain data struct never in scope for this
phase). Migrated files: `lighting.rs`, `menu.rs`, `seg008.rs`, `seg009.rs` (the bulk — ~60
sites), and `seg006.rs` (2 sites, missed from the original file batch, caught by the final
sweep). `midi.rs` had zero real hits (its one grep match was a MIDI file header's own
`.format` field, unrelated to SDL).

`WasmRenderer` is a real implementation, not a stub, for everything Phase A needed: a
`Vec<u8>`-backed `WasmSurface` store handling `create_surface`, the five accessors,
lock/unlock, `map_rgb`/`map_rgba`, `fill_rect`, `blit`/`blit_scaled` (color-key aware),
`set_color_key`/`set_blend_mode`/`set_alpha_mod`,
`set_palette`/`set_palette_colors`/`set_surface_palette`, and
`convert_surface`/`convert_surface_format`. `surface_palette` allocates a genuine
heap-allocated `SDL_Palette` (not opaque) because `seg009.rs` reads `.ncolors` off it
directly — a real, load-bearing exception to "fully opaque," discovered while building the
test harness. Window/init lifecycle, real audio, and input/controllers are still
`unimplemented!()` — correctly out of Phase A's scope.

**Pixel-parity test harness** (`rust/src/platform/pixel_parity_tests.rs`) runs identical
`Renderer`-trait call sequences against `SdlRenderer` (real SDL2, headless via
`SDL_VIDEODRIVER=dummy`) and `WasmRenderer`, natively, no wasm32 target or browser needed.
`platform::wasm` is now also compiled on native under `cargo test` (not normal native
builds) specifically to make this possible. Two real bugs were caught building this
infrastructure, not hypothetical: (1) `SDL_BlitSurface` between two 8bpp indexed surfaces
with *different* palettes does palette-aware RGB color-matching, not a raw index copy —
found because the test's destination surface's palette wasn't synced with the source's,
mirroring exactly what real SDLPoP always does correctly (keep blitted surfaces' palettes in
sync) and what a naive future migration could get wrong; (2) a hand-built test
`SDL_PixelFormat` needed correctly-computed `Rshift`/`Gshift`/`Bshift`/`Ashift` fields, not
just masks, for real `SDL_MapRGBA` to work. Also needed a `Mutex` to serialize the two tests
touching the shared `SdlRenderer` singleton — `cargo test`'s default parallelism segfaulted
without it.

**Verification throughout:** native build/test/harness reconfirmed green after every merge —
0 warnings, 68/68 tests (66 pre-existing + 2 pixel-parity), 30/30 replays. `wasm32-unknown-
unknown` `cargo check` and full link both still succeed.

**Considered and deferred (not done, not blocking):** the "future consideration" section
below (adopting `pixels`/`softbuffer` + `winit` for presentation) remains exactly that — a
future consideration, not started.

**Why first:** required regardless of the web target (native's own `SDL_Surface` access is
already scattered outside `platform/sdl.rs`, which was always the Step C goal, just not
finished for surfaces). Self-contained, lowest risk, and it's the same "batch-then-verify,
one file at a time" pattern that already worked for Step D — nothing new to invent
procedurally, just a new target for it.

**Scope:** ~75 call sites across `seg008.rs`, `seg009.rs`, `menu.rs`, `lighting.rs`,
`midi.rs` that dereference `SDL_Surface`/`SDL_PixelFormat` fields directly, **plus** — folded
into this phase rather than deferred — a real (not stub) `WasmRenderer` and a pixel-parity
test harness built alongside it. Building the second `Renderer` implementation *during* Phase
A, not after, means every accessor method gets a same-commit correctness check instead of
trusting the encapsulation blindly until Phase B/C.

**Design — trait additions:** extend the `Renderer` trait with accessor methods that replace
direct field reads, e.g.:
- `surface_size(surf) -> (c_int, c_int)` (replaces `.w`/`.h`)
- `surface_pitch(surf) -> c_int`
- `surface_pixels(surf) -> *mut c_void` (or a raw-pointer-yielding closure form for the
  several manual per-row pixel-copy loops in `seg009.rs` — keep it low-level/`unsafe` like
  the rest of this trait; making pixel access *safe* Rust is a separate, later goal)
- `surface_format_info(surf) -> PixelFormatInfo` — a plain Rust struct copy of
  `BitsPerPixel`/`BytesPerPixel`/`Rmask`/`Gmask`/`Bmask`/`Amask` (avoids `.format` field
  access; `map_rgb`/`map_rgba` already take a `*const SDL_PixelFormat`, so decide during
  implementation whether those keep taking the raw pointer or also move to `PixelFormatInfo`)
- `surface_palette(surf) -> *mut SDL_Palette` (stays a raw handle — `set_palette_colors` etc.
  already operate on it that way)

**Native (`SdlRenderer`):** these methods just dereference the real `SDL_Surface`/
`SDL_PixelFormat` — pure relocation of existing dereferences from call sites into the trait
impl, zero behavior change.

**Web (`WasmRenderer`), built for real this phase, not stubbed:** owns a plain Rust `struct
WasmSurface { w, h, pitch, pixels: Vec<u8>, format: PixelFormatInfo }`, stored in a
`Vec`/slotmap keyed by an opaque handle (the `*mut SDL_Surface` game code passes around
becomes just an opaque integer cast to a pointer — never actually dereferenced as a real
`SDL_Surface` anywhere). This is what makes `WasmRenderer` not need to fake real SDL memory
layout at all.

**Pixel-parity test harness (new this phase):**
- A `draw_test_scene<R: Renderer>(r: &mut R) -> Vec<u8>` helper — a fixed, deterministic
  sequence of `Renderer` calls (create a surface, set a palette, fill a rect, blit, read back
  pixels), generic over the trait — run against both `SdlRenderer` and `WasmRenderer` in an
  ordinary native `#[cfg(test)]` unit test, asserting identical output. No `wasm32` target or
  browser needed for this: `WasmRenderer`'s logic is plain `Vec<u8>` manipulation, testable
  natively.
- `SdlRenderer`'s side needs SDL's video subsystem initialized headlessly for `cargo test` —
  same `SDL_VIDEODRIVER=dummy` pattern the harness already uses for audio
  (`SDL_AUDIODRIVER=dummy`), applied to video. `SDL_CreateRGBSurface` doesn't touch a real
  display, so this should be cheap to wire up.
- Grow the test scene's coverage as accessor methods are added — each new method should get
  at least one exercising draw call in the shared scene, not just a signature.

**Execution:** per-file passes, same subagent pattern as the Step D batches (one file per
agent, `git log`-verified, harness+tests green after each). Suggested order: `lighting.rs`
and `midi.rs` first (small, low risk, fast confidence-builders), then `menu.rs`, then the two
large ones (`seg008.rs`, `seg009.rs`) last. Build out `WasmRenderer` and the pixel-parity
harness incrementally alongside the accessor methods each file's pass introduces, rather than
as a separate pass at the end.

**Exit criteria:** zero `SDL_Surface`/`SDL_PixelFormat` field access remains outside
`platform/*.rs`. A real `WasmRenderer` exists and passes pixel-parity tests against
`SdlRenderer` for the accumulated test scene.

---

## Phase B — Game loop compatibility, via a Web Worker (revised design)

**Original design, superseded:** the plan originally called for extracting the per-tick body
of the game loop into a single callable `advance_one_frame(state) -> LoopSignal` unit, driven
by a blocking loop natively and by `requestAnimationFrame` on web. Research before starting
implementation (see below) found this to be a genuinely large, high-risk refactor — bigger
than initially scoped and not worth doing for what it actually buys.

**What the research found:**
- `start_game()`'s restart mechanism (`seg000.rs`, the codebase's one `setjmp`/one `longjmp`
  call site) is invoked from **12 different call sites** across 5 files (`seg000.rs`,
  `seg001.rs`, `seg003.rs`, `seg006.rs`, `replay.rs`) — process_key, level-end handling, demo
  timeout, hall-of-fame, clock expiry, replay end/cycling. Converting that stack-unwind into
  explicit signal-propagation would mean touching every function between each of those 12
  sites and `pop_main` — 20-40+ functions of real control-flow surgery.
- Beyond that, there are **~8 other blocking-wait call sites** that would need the same
  treatment for genuine non-blocking compatibility: a 100ms stall in `redraw_screen_impl`
  (`seg003.rs:355`), a spin in `transition_ltr` (`seg000.rs:2684`), a nested pause loop in
  `do_paused` (`seg000.rs:2251-2257`), `wait_for_sounds_to_finish` (`seg000.rs:2533-2539`),
  both fade routines (`seg009.rs:5120-5124`, `5236-5240`), `do_wait` (`seg009.rs:4965`), and
  the sound-finish wait in `play_level_impl` (`seg003.rs:161-163`).
- This is comparable in size to Phase A (~80 sites across 6 files) but *riskier* — it touches
  actual timing/control-flow logic, not just data-access relocation, which is exactly the
  class of change most likely to introduce subtle, harness-invisible-until-triggered
  regressions.

**Revised design: run the existing (unchanged) blocking loop inside a Web Worker.** A Worker
has its own independent event loop — blocking it does not freeze the browser tab, which was
the actual, sole motivating problem. Consequences:
- `play_level_2`, every nested pause/wait loop, the whole call chain from `pop_main` down —
  **stays completely unchanged.** Zero gameplay logic touched, zero risk to timing-sensitive
  behavior the differential-trace harness cares about.
- All real code changes are confined to the `Platform` trait boundary, already isolated by
  design: `WasmRenderer::present()` posts a finished frame to the main thread (Workers have
  no DOM/canvas access) instead of drawing directly; `WasmInput` receives events via
  message-passing instead of a direct browser API; `WasmAudio` posts PCM buffers. `WasmFiles`
  gets *simpler* than Phase C originally planned: synchronous `XMLHttpRequest` is explicitly
  permitted inside Workers (unlike the main thread, where it's deprecated/discouraged), so
  the async-preload-then-serve-synchronously design may not be needed at all — worth
  revisiting Phase C's plan once this lands.
- `setjmp`/`longjmp` still needs a real fix (this part of the original plan stands). The
  first attempt (commit `4111b02`) wrapped the outer `start_game()`-calling loop in
  `catch_unwind`, with wasm's restart path panicking with a distinguishable marker type the
  wrapper would catch and retry on. **This did not actually work** — see the correction
  below; `catch_unwind` cannot catch anything at all on `wasm32-unknown-unknown` with this
  toolchain, confirmed empirically once real input let the game reach an actual restart call
  site (see item 3's "real fix" note, commit `0fa3cc0`). Contained to `seg000.rs` +
  `lib.rs` + `web/worker.js`, gated `#[cfg(target_arch = "wasm32")]` — native keeps using
  real libc `setjmp`/`longjmp`, completely untouched. **✅ Actually done, commit `0fa3cc0`.**

**Known tradeoff, accepted for a first working version:** without `SharedArrayBuffer` (which
needs COOP/COEP cross-origin-isolation HTTP headers — a real deployment constraint), there's
no efficient blocking-sleep primitive available inside a Worker for frame pacing, so
`Renderer::delay` falls back to a busy-spin (checking wall-clock time in a tight loop) rather
than a real sleep — costs CPU/battery on that one background thread while a level is running,
not correctness. Real efficient sleep via `Atomics.wait` is a deferred future refinement, same
spirit as Phase A's deferred alpha/`ADD`/`MOD` blend-mode compositing.

**Scope for this phase:**
1. `setjmp`/`longjmp` fix (`seg000.rs` + `lib.rs` + `web/worker.js`, wasm32-only). **✅ Done
   for real** (commit `0fa3cc0`, correcting the `catch_unwind`-based commit `4111b02`, which
   turned out never to have worked — see the note above and item 3's real-fix writeup below).
2. `WasmRenderer::present()`/texture pipeline — post the finished frame buffer to the main
   thread. **✅ Done** (commits `b31b0ff`, and frame delivery this pass):
   `create_texture`/`update_texture`/`set_render_target`/`render_clear`/`render_copy`/
   `render_present`/`render_set_logical_size`/`get_renderer_output_size` all have real
   `WasmRenderer` implementations (in-memory texture store + screen buffer, verified with a
   pixel-parity test that reads back the presented frame). `render_present` now also calls a
   new `post_frame_to_js` helper, which reads `postMessage` reflectively off `globalThis` (same
   pattern as `performance_now_ms`, so this stays a no-op on native/under `cargo test` and on
   the headless Node probe, and only actually sends once run inside a real Worker/browser) and
   posts `{type: 'frame', w, h, bpp, pixels}`.
3. `WasmInput` — receive key/mouse state and deliver it to the game live. **✅ Done** (commit
   `292a19a`), via `SharedArrayBuffer` per the discussion below (a deliberate first-pass choice,
   not the intended long-term shape). `web/index.html` creates a 521-byte `SharedArrayBuffer`
   (512 key-state bytes + mouse x/y/buttons) and writes into it directly with ordinary
   (non-atomic — a torn read of an already-atomic single byte isn't a real correctness concern
   here) typed-array writes from `keydown`/`keyup`/`mouse*` listeners, mapping
   `KeyboardEvent.code` to SDL scancodes. It's passed to the Worker as the very first
   `postMessage` (the one message a Worker actually *can* receive, since it arrives before
   `run_game()`'s blocking loop starts — `web/worker.js` waits for it before doing anything
   else). `WasmRenderer::set_shared_input_buffer` (new `lib.rs` wasm-bindgen export) stores a
   `js_sys::Uint8Array` view; `sync_shared_input` copies its contents into the existing
   `key_states()`/`mouse_state_mut()` storage every poll, so `InputSource::key_state`/
   `mouse_state` needed no changes. `synthesize_key_edge_events` diffs current vs
   previous-poll key state and queues real `SDL_KEYDOWN`/`SDL_KEYUP` events (byte-for-byte
   matching `seg009.rs`'s private `SDL_KeyboardEvent`/`SDL_Keysym` layout, including computed
   modifier bits), turning level state into the edge-triggered events `process_events`
   actually consumes — the same way real SDL turns HID reports into an event queue.
   `push_event`/`poll_event` are now both real, sharing one FIFO queue (previously
   `poll_event` was a deliberate "always empty" stub; see the exit-criteria note below on why
   that was correct at the time, not a shortcut). `scripts/serve_wasm.sh` (new) serves `web/`
   with the `Cross-Origin-Opener-Policy`/`Cross-Origin-Embedder-Policy` headers
   `SharedArrayBuffer` requires — plain `python3 -m http.server` can't set custom headers.
   **Verified end-to-end with Playwright MCP**: a real `keydown` event dispatched from the
   main thread advanced the game from its intro text past the real, fully-rendered "Prince of
   Persia" title screen (ornate border, palace art, logo, copyright text — all real sprite/
   palette decoding) into its attract-mode demo animation, confirming genuine gameplay-loop
   progression driven by real input, not just a static frame. Mouse button *events* (not just
   polled state) aren't synthesized yet — this game is overwhelmingly keyboard-driven (mouse
   only matters for menu clicks), so that's a known, minor, explicitly-scoped gap, not an
   oversight.
4. `WasmAudio` — post PCM buffers for the main thread to play. **✅ Done** (commit `c56b774`):
   `WasmRenderer::open_audio_raw` parses the real `SDL_AudioSpec` `init_digi` builds
   (`seg009.rs`), storing its callback/userdata/format. Real SDL pulls that callback from a
   dedicated realtime audio thread, decoupled from game timing -- no such thread exists here
   (wasm32 is single-threaded in this build), so a new `pump_audio()` calls it synchronously
   instead, from every spin of `Renderer::delay`'s busy-wait (also implemented for real this
   pass, closing another `unimplemented!()` -- the already-accepted busy-spin frame-pacing
   tradeoff, see below) and once per `render_present` -- the two points the blocking game loop
   actually yields time with any regularity. Each pulled PCM chunk posts to JS via the same
   reflective-`postMessage` pattern as frames (`post_audio_to_js`). `AudioBackend::pause`/
   `lock`/`unlock` (the methods real call sites actually use -- `.open()` turned out to be
   dead code, the same "trait exists, real callers use something else" shape Phase C found
   for `FileSystem`) are real: `pause` toggles whether `pump_audio` does anything; `lock`/
   `unlock` are correctly no-ops (nothing else runs concurrently to race against on a single
   thread). `web/index.html` plays received chunks via the Web Audio API
   (`AudioContext`/`AudioBufferSourceNode`, scheduled back-to-back via a running
   next-play-time cursor), gated behind a required "Enable audio" button click (browser
   autoplay policy). Verified end-to-end with Playwright MCP: audio init completes with no
   panics, and the game reaches its real title/splash screen with fully legible text. No audio
   chunks have actually posted yet in that verification run, since nothing calls `pause(false)`
   before that screen -- reaching a point with real sound needs input (item 3) to advance past
   "Press any key to continue...".
5. JS harness: a dedicated worker script loading the wasm module and relaying messages;
   `web/index.html` updated to spawn the worker, paint received frames to the canvas, forward
   input events, and play received audio. **✅ Frames + audio out done, verified working in a
   real browser** (`web/worker.js` new, `web/index.html` rewritten, `scripts/build_wasm.sh` updated
   to symlink `data/`/`SDLPoP.ini` into `web/` the way the native harness does for
   `target/debug/`). Verified end-to-end with Playwright MCP (`chrome-for-testing`, installed
   this session): the Worker fetches the preloaded assets, runs `pop_main()` unmodified, and a
   real frame produced by actual gameplay code — the game's own `showmessage()` dialog box
   rendered via its real font/text-drawing code — reaches the page's `<canvas>` via
   `postMessage`, confirmed both via console log (`frame 640x400, bpp=3`, repeating) and a
   screenshot. This is the milestone this phase's exit criteria asked for. Input is
   deliberately still not wired (see item 3) — an explicit "frames first, then input"
   sequencing decision made this session, since a naive `postMessage`-based input design would
   silently never deliver a single keystroke (a Worker's `onmessage` can't fire while
   `pop_main()`'s blocking loop holds the stack; see below).

   **Update, same session:** `web/worker.js` now preloads *every* file under `data/` (via a
   `scripts/build_wasm.sh`-generated `web/data_manifest.txt`, fetched and loaded with bounded
   concurrency) instead of a hand-picked two-file list. This surfaced three more real bugs,
   all fixed via repeated Playwright-driven browser runs (not just the headless Node probe --
   these needed the real texture/blit pipeline exercised with real assets to show up):
   - `decode_png_to_surface` assumed every real asset was 8-bit; 926/927 real sprite/font
     PNGs are actually 1/2/4-bit indexed. Fixed by unpacking sub-8-bit indexed scanlines by
     hand (`unpack_indexed_scanlines`) rather than using the `png` crate's `EXPAND` transform,
     which would've resolved the palette into RGB(A) and broken `set_color_key`'s
     index-based contract.
   - `blit_impl` never did palette-aware conversion (indexed source -> truecolor destination)
     and never implemented `SDL_BLENDMODE_BLEND` compositing at all -- both real gaps, found
     because hardcoded-font text rendering (`method_3_blit_mono`) depends on both. Fixed;
     the same-format/blend-NONE fast path everything else uses is unchanged, so this carries
     no regression risk to the differential harness (which is native-SDL-only anyway and
     never touches `WasmRenderer`).
   - `access()`/`stat()` couldn't recognize a loose-resource-folder path (e.g.
     `"data/IBM_SND1"`) as existing, since the VFS is a flat `path -> bytes` map with no
     literal entry for the directory itself, only for files under it. Added
     `vfs_contains_dir` (prefix check) and wired it into both.

   **Result: real hardcoded-font text now renders legibly in the browser** (previously solid
   rectangles) -- confirmed visually via Playwright screenshot showing "Cannot find a required
   data file: IBM_SND1.DAT or folder: data/IBM_SND1 / Press any key to quit." in the game's
   own crisp bitmap font. That specific message is itself expected and correct: `IBM_SND1` is
   a real asset folder that exists on disk but isn't consumed by anything yet since audio
   (item 4) isn't implemented -- the probe has now reached the actual audio-init code path.
6. Enough of `sdl_init`/`create_window`/`create_renderer`/window-lifecycle stubs to get
   `pop_main()` actually running inside the worker without hitting `unimplemented!()` on the
   startup path. **Done** — the empirical Node-based `run_game()` probe found and fixed every
   wall up through `poll_event`, and item 5's real-browser run confirms the whole chain now
   runs cleanly with no more startup-path panics (commits `e5ba566`, `9a3613a`, `b31b0ff`,
   `a1ca374`, plus this pass).

**The real `setjmp`/`longjmp` fix, correcting commit `4111b02` (this session, commit
`0fa3cc0`):** the `catch_unwind`-based wasm32 restart mechanism was never actually
functional. Confirmed with an isolated minimal test, independent of this codebase: on
`wasm32-unknown-unknown` with the current stable toolchain, every panic unconditionally
aborts via `__rust_abort`, regardless of whether it's wrapped in `catch_unwind` — this is a
real, long-standing limitation of this target (real unwinding needs a nightly toolchain,
`-Z build-std`, and the wasm exception-handling target feature; not something to take on for
this). The bug went unnoticed for as long as it did because it requires actually reaching one
of the ~12 restart call sites to trigger — which nothing did until real keyboard input (item
3) let the game advance past its title screen, "press any key to continue" being one of those
call sites. Once input worked, every restart crashed the browser tab with an uncaught panic.

The real fix crosses the wasm/JS boundary instead of trying to unwind purely within wasm:
`wasm_bindgen::throw_str` throws a real, catchable JS `Error` that correctly unwinds every
intervening wasm stack frame — also confirmed empirically (a synthetic 3-levels-deep nested
throw unwound cleanly back to the JS caller, with the wasm instance's memory/globals fully
intact and callable again afterward). This is the same "JS exceptions" mechanism Emscripten
has long used for `setjmp`/`longjmp`, and needs no toolchain change at all.

The catch: this throw necessarily unwinds *all* the way back to whatever JS call is currently
running — there is no way to "catch and resume mid-wasm-stack" the way `setjmp`/`longjmp` or
a working `catch_unwind` could. This works here specifically because `start_game`'s own outer
call is already the very last thing `pop_main()`/`init_game_main()` do (nothing meaningful
runs after it), so losing every frame back to the JS boundary loses nothing real. A new
`resume_game_after_restart()` export (`lib.rs`) re-enters directly at `start_game_body()`,
skipping straight past `pop_main()`'s one-time setup (asset loading, `SDL_Init`, ...), which
must not re-run on a restart. `web/worker.js`'s `runWithRestartRetries` wraps the first
`run_game()` call and retries via `resume_game_after_restart()` on that specific signal
(matched by exact message string), letting any other exception — a real panic/bug — propagate
normally instead of being silently swallowed. Native's `start_game` is completely untouched
(still real `setjmp`/`longjmp`).

Verified end-to-end with Playwright MCP: the exact rapid key sequence that previously crashed
with an uncaught panic (advancing past the title screen into gameplay) now runs cleanly with
zero console errors, reaching real in-game rendering — the Prince standing in an actual
dungeon room, torches lit, HUD visible.

**`SharedArrayBuffer`/input note, implemented (commit `292a19a`):** getting live keyboard/
mouse input into a *running* (blocking) Worker game loop cannot work via plain `postMessage`
— a Worker's `onmessage` handler literally cannot run while a synchronous call still occupies
the stack, and `pop_main()` never returns until the game exits. The real fix, now built, is a
plain `SharedArrayBuffer` the main thread writes into directly with ordinary (non-atomic —
deliberate; see item 3 above) typed-array writes, which the wasm side reads on every
`poll_event`. This is a *separate* `SharedArrayBuffer` from wasm's own linear memory, not real
wasm32 threads (`WebAssembly.Memory({shared: true})`) — much simpler, no special toolchain/
threading build needed, just a JS object passed by reference through one `postMessage`. The
catch, now a real constraint rather than a hypothetical one: the buffer needs COOP/COEP
cross-origin-isolation headers, which is why `scripts/serve_wasm.sh` exists (plain
`python3 -m http.server` can't set them) — a real hosting target must support setting these
two response headers, or this design doesn't work there. `Renderer::delay`'s busy-spin
frame-pacing tradeoff is unrelated to this buffer (it doesn't use `Atomics.wait`, just wall-
clock polling — see item 4/`WasmAudio`'s done note), so that potential future refinement
remains separately deferred. The user has said they'd like to move off `SharedArrayBuffer`
eventually regardless, in favor of the `advance_one_frame()`/event-driven-restart redesign
(recorded above as future work) — a non-blocking per-tick design would let plain `postMessage`
handle input with no shared memory needed at all. Not scheduled; this first-pass
implementation is deliberately not the intended long-term shape.

**Exit criteria: ✅ met, and exceeded.** The `setjmp`/`longjmp` gap is resolved (not a silent
panic); a real frame produced by actual gameplay code reaches the browser canvas via the
Worker, driven by the genuinely unmodified game loop. Beyond the phase's original stated bar:
audio (item 4) and live input (item 3) are both now real too, confirmed end-to-end with
Playwright MCP — real keyboard input drives the game from its intro text through the fully-
rendered title screen into its attract-mode demo animation, with audio init completing
without panics. All of Phase B's scope is now done except mouse *events* (a known, minor,
explicitly-scoped gap — see item 3) and the deliberately-deferred `advance_one_frame()`
redesign (recorded as future work, not part of this phase's scope).

---

### Future consideration (deferred, not scheduled): `advance_one_frame()` and removing setjmp/longjmp for real

The originally-sketched design — extracting the game loop's per-tick body into a single
callable `advance_one_frame(state) -> LoopSignal`, driven identically by native (a plain loop)
and web (`requestAnimationFrame`/a JS-driven tick) — remains a genuinely nicer end state than
the Worker approach above: native and web would share the literal same driving loop, instead
of diverging into "native: real blocking loop" vs. "web: the same blocking loop, just inside
a Worker instead of the main thread." The Worker design was chosen first because it reaches
the same practical goal (a browser tab that doesn't freeze) for a small fraction of the risk —
see "Original design, superseded" above for exactly why the inline version is large (the
restart mechanism's 12 call sites, ~8 other blocking-wait call sites).

**This future work should also retire both of the current restart-mechanism's real liabilities,
not just add a nicer loop on top:**

- **Native's real `setjmp`/`longjmp` should eventually go away too, not stay as a
  native-only exception.** It's a genuine non-local jump — it doesn't compile to (or map
  onto) anything a normal reader's mental model of function calls/returns handles, it's
  inherently a divergence from idiomatic Rust, and as long as it exists natively, native and
  web have two *structurally different* restart mechanisms (real non-local jump vs.
  panic-based emulation) instead of one shared implementation — directly working against
  this whole plan's "one effective implementation" goal, not just a wasm32-specific wart.
- **The wasm32 restart mechanism (`wasm_bindgen::throw_str` across the wasm/JS boundary,
  commit `0fa3cc0`, replacing the never-actually-functional `catch_unwind` attempt from
  commit `4111b02`) is explicitly acknowledged tech debt, not a real fix — keep it only until
  the real fix below lands.** Using an exception for ordinary control flow (not an actual
  error) is the same anti-pattern `catch_unwind` was reaching for, just via a mechanism that
  actually works on this target: it makes real bugs harder to distinguish from intentional
  restarts (mitigated today by matching on an exact signal string, but still fragile in
  spirit), and the control flow is just as opaque as the `longjmp` it replaced — a
  different-shaped version of the same problem, not a solution to it.
- **The real fix for both, when this is picked up: make "restart the game" an ordinary
  return value that bubbles up through the call stack, the same way `advance_one_frame`'s
  `LoopSignal` already would.** Concretely: none of the 12 current restart call sites
  (Ctrl+R, the win/Hall-of-Fame sequence, the demo timing out, the level clock expiring,
  etc.) should call `start_game()` directly at all. Each is already reacting to a specific
  game event — model that explicitly: the event handler returns (or sets) a signal value
  (e.g. `GameEvent::RestartRequested`), which propagates upward through ordinary function
  returns — through `play_frame` → `play_level_2` → `play_level` → `init_game_main` — the
  same path `LoopSignal` would already need to traverse for the frame-loop unification above.
  Once every restart trigger is expressed as "return this event," the *one* place at the top
  that receives it (`init_game_main`'s driving loop) can just loop and call the setup logic
  again — no jump, no panic, no unwinding, on **either** target. This is real, comparable in
  size to the frame-loop extraction itself (touches the same 12 call sites either way), which
  is exactly why it's bundled with `advance_one_frame()` as one future initiative rather than
  attempted piecemeal.

Worth revisiting once the Worker-based version is fully working end to end (a real reason to
want it would be: wanting to drop the Worker/`postMessage` indirection entirely, wanting
`advance_one_frame` for something else like Phase 3's automated game-beating search wanting
fast single-step control over simulation ticks, or just wanting to finally delete the
`catch_unwind` hack). Not scheduled now — explicitly a "nice future goal," not a commitment.

---

## Phase C — Virtual filesystem for `fopen`-family calls — ✅ DONE (revised design)

**Original design, superseded:** the plan called for migrating every game-facing `fopen`/
`fread`/`fwrite`/`fclose` call site to the `FileSystem` trait (`read_file`/`write_file`/
`file_exists`), on the assumption DAT loading and friends bypassed that trait. A file-I/O
audit before implementing found something better: **the `FileSystem` trait is completely
unused dead code** — zero real call sites anywhere in the crate. Every DAT/asset/INI/
quicksave/HOF/replay file access already goes through plain libc `fopen`/`fread`/`fwrite`/
`fseek`/`ftell`/`fclose`, which already resolve to `wasm_libc.rs`'s shim on wasm32. So no
call-site migration was needed at all — making `wasm_libc.rs`'s `fopen`-family functions
real (backed by a virtual filesystem) makes every existing call site work with zero changes
anywhere else in the crate, a smaller and lower-risk change than the planned migration.

**What shipped (commit `e5ba566`):**
- `wasm_vfs.rs` (new) — the shared `path -> bytes` store. Split into its own
  dependency-free module rather than living in `wasm_libc.rs`, because `wasm_libc.rs` can't
  be widened to compile under native `cargo test` the way `platform::wasm` was (Phase A) —
  it depends on `js_sys` (for `time()`), a wasm32-only crate.
- `wasm_libc.rs` — `fopen`/`fread`/`fwrite`/`fseek`/`ftell`/`fclose`/`rewind`/`feof`/
  `fgetc`/`fputc`/`fputs`/`access`/`remove` are now real implementations against the VFS,
  not stubs. Write-mode files get copied back into the VFS on `fclose`, so a
  quicksave-then-quickload round-trip works within one browser session even with no real
  backing storage yet (doesn't survive a page reload — deferred, e.g. wiring to IndexedDB).
- `lib.rs`'s `preload_file(path, data)` — the JS-facing way to populate the VFS before
  starting the game. No on-demand fetch fallback yet (a synchronous-XHR-inside-a-Worker
  option, per Phase B's revised design, is a reasonable future refinement, not implemented).
- Also discovered (empirically, via the Node-based `run_game()` probe) that
  `Renderer::rw_from_file`'s only real caller is `SDLPoP.cfg` loading (`menu.rs`), not DAT
  files as originally assumed. Implemented `WasmRenderer`'s `rw_from_file`/`rw_from_mem`/
  `rw_from_const_mem`/`rw_tell`/`rw_close`/`rw_write`/`rw_read` against the *same* VFS (one
  filesystem, not two), backed by an opaque `SDL_RWops` handle store (confirmed safe —
  nothing dereferences `SDL_RWops` fields directly anywhere in the crate).

**Also landed opportunistically while probing further with `run_game()`** (these are really
Phase B's "item 6: enough SDL init/lifecycle stubs to get `pop_main()` running, scoped
empirically" — noted here since they happened in the same pass): `set_hint`, `sdl_init`/
`sdl_init_subsystem`/`sdl_quit` (no real subsystems to manage), `create_window`/
`create_renderer` (sentinel non-null pointers — confirmed safe, nothing dereferences them),
`get_renderer_info_flags` (reports no target-texture support, steering the game onto its
simpler rendering fallback), and a real (not stub) `WasmInput` backed by module-level
key/mouse-state statics plus `set_key_state`/`set_mouse_state` entry points for the eventual
message-passing input design.

**Update (commit `9a3613a`): PNG decoding done.** Added the `png` crate (image-rs org,
vetted per the same rigor as `symphonia`'s earlier adoption — 511 stars, actively maintained,
0 `cargo-audit` vulnerabilities, 0 unsafe code in the crate itself, confirmed wasm32
buildable via the pure-Rust `miniz_oxide` backend). `load_image_from_file`/
`load_image_from_memory`/`img_load_rw` decode real PNGs into either an 8bpp indexed surface
with a real palette (matching real `IMG_Load`, and what `SDL_ISPIXELFORMAT_INDEXED`-gated
code in `seg009.rs` branches on) or a 32bpp RGBA surface otherwise. Also picked off
`set_window_icon` and `render_set_logical_size` (both no-ops — no real OS window/canvas
scaling concern at this layer) while continuing the empirical probe.

**Next wall found, not yet started:** `create_texture`/`update_texture`/`render_present` —
the actual frame-presentation pipeline. This is real Phase B territory (the deliberately
deferred `present()` design) — needs the Worker/JS message-passing design, not another cheap
stub.

**Exit criteria:** met for the stated scope (game-facing file access works through a real VFS
with zero call-site changes elsewhere). Native behavior unchanged (still real synchronous
`fopen`/`fread` via glibc). `stat`/`fstat` (mod-folder detection, loose-file sizing) and
`opendir`/`readdir` (mod/replay directory scanning) remain fail-stubs — not exercised by the
base-game startup path tested so far, deferred until mod support is prioritized.

**Deferred, explicitly not forgotten: file I/O still isn't behind a real interface.** The fix
above makes wasm32 file access *work*, but it works by making `wasm_libc.rs`'s raw libc
`fopen`-family shim real — not by routing it through a proper Rust trait boundary. The
`FileSystem` trait (`platform/mod.rs`) still exists, still describes the right shape
(`read_file`/`write_file`/`file_exists`), and is still unused. This matters for the same
reason Phase A's `Renderer` encapsulation mattered: without a real interface, there's no
seam to swap backends (real fetch/IndexedDB for a production web build vs. the flat
preloaded-map VFS today), no seam to unit-test file-loading logic independent of the wasm32
libc shim, and native/web diverge in *how* file access is implemented even though they now
agree on behavior. Why not done now: routing every real call site (concentrated in
`seg009.rs` and `options.rs`, per the Phase C file-I/O audit) through `FileSystem` instead of
raw `fopen`/`fread` is a real, broader call-site migration — comparable in shape to Phase A's
`Renderer` accessor-method migration, not a quick follow-on to the VFS fix that just shipped.
The VFS storage layer (`wasm_vfs.rs`) is at least already a clean, separate module, so this
migration has a real foundation to build on whenever it's prioritized — it does not need to
be redone from scratch, just re-routed through a trait instead of called directly.

### Future consideration (deferred, not scheduled): stop returning raw pointers from `Renderer` accessors on the wasm side

`Renderer::surface_format_ptr` (added this session to fix the `(*surf).format` wild-pointer
bug — see `platform/wasm.rs`) returns `*mut SDL_PixelFormat`, matching the type the C-ported
call sites expect. On native that's a real pointer into real SDL memory, so it's the natural
type. On wasm, `WasmSurface` doesn't have real SDL memory at all — the returned pointer is
just `&mut` on a heap-allocated `Box<SDL_PixelFormat>` cast to a raw pointer, purely so the
same call-site code compiles unchanged against both backends. That's exactly the kind of
pointer the "wild pointer" bug came from in the first place (a pointer standing in for
something that isn't really memory-shaped) — it's *safe* here only because `WasmSurface` now
keeps a real backing `SDL_PixelFormat` alongside it, but the accessor's signature doesn't make
that guarantee visible or enforced.

Not fixable in general: most `Renderer` methods intentionally keep C-shaped pointer signatures
because they're called directly from faithfully-ported C code across hundreds of sites, and
changing those signatures would mean touching the ported code itself, not just the platform
layer — out of scope for "faithful translation." But methods like `surface_format_ptr` that
exist purely inside the platform-trait boundary (not preserved C call sites) are a smaller,
real candidate: they could return a `&SDL_PixelFormat`/`&mut SDL_PixelFormat` (borrow-checked,
can't outlive the surface) or a slice-based accessor instead of a synthetic raw pointer,
without touching native's implementation or any ported gameplay code. Worth auditing which
`Renderer` methods are "real C call site, pointer is unavoidable" vs. "platform-internal,
could be a safe reference" once the JS harness work settles down — not urgent, since the
current pointer-returning version is verified correct (pixel-parity tests, harness, wasm32
build all green).

### Future consideration (deferred, not scheduled): asset manifest / preload strategy

Every asset preloaded into the VFS so far has been added by hand, one file at a time, as
testing discovered each was needed (`SDLPoP.ini`, then `data/icon.png`, ...). A real build
needs an actual manifest — an explicit list of every file to fetch and hand to
`preload_file()` before the game starts — rather than a hand-maintained ad hoc list. Not
designed yet; a few real open questions to think through once this is prioritized, not now:

- **Offline play (PWA) is an explicit future goal**, and it pulls toward "preload
  everything up front into a real local cache" (Service Worker + Cache API/IndexedDB,
  not just the current in-memory-only VFS, which forgets everything on reload) — a full
  manifest fetched and cached once, then playable with no network at all afterward.
- **Online play might benefit from a different, lazier strategy**: eagerly load only what's
  needed right now (the current level's assets), then prefetch the *next* level's assets in
  the background while the player is still on the current one — reducing initial load time,
  if there's an actual latency win to be had. That's a real "if": if the total asset set
  turns out small/cheap enough to fetch in full up front anyway, the lazy/prefetch design
  adds real complexity (two loading code paths, prefetch-timing logic) for no measurable
  benefit. Worth measuring actual asset sizes and load times against a working baseline
  before deciding, not assumed now.
- **These two modes aren't necessarily exclusive** — a real design might end up being
  "offline mode: full manifest, cached once, played from cache" and "online mode: lazy
  current-level load + background next-level prefetch," selected based on whether the PWA
  has already cached everything. Genuinely needs more thought once there's a working
  end-to-end version to measure against; captured here so it isn't lost, not designed now.

---

## Ordering and dependencies

1. **Phase A** first — self-contained, required either way, lowest risk, and delivers a real,
   pixel-parity-tested `WasmRenderer`, not just cleaner call sites.
2. **Phase B** next — benefits from having a working renderer to test frame-by-frame
   stepping against, and is the one most likely to also resolve the `setjmp`/`longjmp` gap.
3. **Phase C** can run in parallel with B (low interdependency with A/B) but must land before
   any real asset can load in a browser.
4. After A+B+C: first real "see the actual game render in a browser" milestone — a first
   playable frame, not full gameplay parity. `WasmAudio`/`WasmInput` real implementations are
   separate follow-up work, not covered by this plan.

## Verification strategy (answers "how do we know new code is faithful?")

- **All native-side changes in every phase are pure relocation with zero gameplay behavior
  change** — verified continuously via the existing 30-replay differential-trace harness +
  unit tests, exactly like Step C/D. This remains the single source of truth for simulation
  correctness throughout; if a phase's harness run ever diverges, that phase introduced a
  real bug, not an acceptable side effect.
- **Web-only code has no trace-harness equivalent** — there is no golden oracle for "does the
  canvas look right." Three-tier approach:
  1. **Pixel-parity unit tests, no browser needed** — built in Phase A itself (see above), not
     deferred. This is the primary, cheap, fast-feedback check going forward.
  2. **Manual visual smoke tests in a real browser** for what pixel-parity checks can't cover
     (actual canvas presentation, real-time pacing, browser-specific quirks). No headless
     browser is available in this environment currently — flag if that becomes worth setting
     up (e.g. `chromium`/`wasm-bindgen-test --headless`) once there's enough surface to
     justify automating it.
  3. **Once Phase B lands**, reassess whether an automated `wasm-bindgen-test`-based
     regression suite for the JS-facing surface (tick-driving, input events) is worth the
     setup cost.

## What is NOT changing

- The C oracle and golden traces stay untouched, as in every prior phase of this project.
- `WasmAudio`/`WasmInput`/real asset-manifest curation are out of scope for this plan — it
  covers only the rendering/loop/file-I/O encapsulation needed to make a unified architecture
  possible.

---

## Future consideration (deferred, not scheduled): an existing crate for presentation

Once `Renderer` is a real, opaque-handle seam (post–Phase A), the *only* genuinely generic
piece left is the final "push a finished RGBA buffer to the screen" step — everything before
that (indexed-palette surfaces, blit/fill semantics, double-buffering) is inherent to this
game's own renderer, not something a library provides. That one generic piece is exactly what
crates like [`pixels`](https://crates.io/crates/pixels) or
[`softbuffer`](https://crates.io/crates/softbuffer) already do well: hand them a buffer, they
get it onto a `winit` window natively or a `<canvas>` on web, with no `sdl2`/hand-rolled
`web_sys` presentation code on our side.

**Why this isn't scheduled now:** adopting either crate for real would mean adopting `winit`
for native windowing too, replacing the `sdl2` crate's windowing/rendering entirely — a much
bigger, separate change than swapping out ~10 lines of `put_image_data` code, and one that
touches native's window/input/controller setup (`seg009.rs`'s SDL init path), not just
presentation. `sdl2` would likely still be needed for audio/controller/haptic support either
way (`pixels`/`softbuffer` don't cover those), so this would be a partial replacement, not a
full one.

**Worth revisiting once:** Phase A-C land and there's a working, pixel-parity-tested renderer
on both targets — at that point, evaluate whether replacing native's `sdl2`-based windowing
with `winit` + `pixels`/`softbuffer` (keeping `sdl2` only for audio/input/controllers, or
migrating those too if a good pure-Rust alternative exists) produces more idiomatic,
maintainable code without sacrificing native fidelity. This is real, valuable follow-up work
if it turns out clean — but it's a windowing-layer replacement, not a rendering-semantics one,
and should be scoped and decided on its own once there's a working baseline to compare against.

## Phase D — Menu/input completeness for wasm (not started)

Prompted by the Esc-menu crash (2026-08-06, commit `d20c68e`, memory
`project_wasm_esc_menu_crash`): nobody had opened the pause menu in the wasm build before a
live user did, and `WasmRenderer` still had real `unimplemented!()` stubs on that code path
(`get_window_flags`, `show_cursor`, `set_fullscreen`, `get_scancode_name` — the first four
fixed; see that memory and commit `68eb7b9`'s regression test). The pause menu — and
everything reachable from it — is a whole area of the game the replay-based harness structurally
cannot cover (replays are deterministic recorded *gameplay* input; opening a menu isn't part
of that stream at all, and even scripted-input testing can only crudely poke at it — see
`open_menu.txt`'s header for why a scripted close isn't even feasible). Expect more gaps than
the four already found. This phase is: find them methodically, decide fix vs. remove for each,
and build real test coverage for the ones worth having, rather than waiting for the next one to
surface via another live crash.

**1. Audit every menu action for wasm reachability.** Walk `menu.rs`'s setting/action tables
(General, Gameplay, Visuals, Mods, Controls, plus the top-level pause menu itself — quicksave/
quickload/restart level/restart game/quit) and, for each, check whether it calls anything
still unimplemented or behaviorally wrong on wasm. Known so far, not yet fixed:
   - `set_fullscreen`/`get_window_flags` are no-ops (correct "not supported yet" behavior, not
     placeholders, but genuinely inert — toggling "fullscreen" in Visuals does nothing visible).
     A real fix needs a JS bridge to the Fullscreen API (`element.requestFullscreen()`).
   - `show_cursor` is a no-op — needs a JS bridge (DOM/CSS `cursor` property on the canvas) to
     ever actually hide the cursor.
   - ~~`std::env::set_var` panics outright on wasm32-unknown-unknown~~ — **fixed, 2026-08-07,
     commit `00a674d`.** Only the "headless" CLI flag (`seg000.rs`) was a real risk; the
     `SDLPOP_SAVE_PATH` call sites turned out to all be `#[cfg(test)]`-only, never compiled
     into the wasm build, so quicksave/quickload were never actually at risk from this
     specific bug. Added real `setenv`/`unsetenv` and migrated every call site. Quicksave/
     quickload still don't have real persistent backing storage on wasm (`platform/wasm.rs`'s
     VFS doesn't survive a page reload — noted in Phase C) — that's separate, still open.
   - Anything else surfaces during the audit — this list is a known-so-far floor, not a ceiling.

**2. Fix what should work, remove what genuinely can't (expect this to be rare).** The user's
own expectation, and a reasonable one: "We can probably make everything work" — most of these
are missing JS bridges (Fullscreen API, cursor CSS, real persistent storage), not fundamental
platform impossibilities. Only remove/hide a menu action from the wasm build if it's genuinely
inapplicable there (nothing identified so far is), not just because it needs new plumbing.

**3. Mouse input.** Explicitly still a TODO, called out separately from the above (not blocking
it): `web/index.html`'s live-input path forwards keyboard events but not mouse clicks yet (see
existing notes elsewhere in this doc and in session history). The pause menu supports full
mouse interaction (`process_additional_menu_input`'s `read_mouse_state`, hover-highlight,
click-to-select) — none of it is currently reachable in the browser build. Wire up real mouse
event forwarding (mousemove/mousedown/mouseup, coordinate-mapped through the canvas the same
way keyboard scancodes are mapped today) as its own scoped piece of work, then re-run the
audit above specifically checking mouse-driven interaction parity with keyboard.

**4. Native-vs-wasm comparison coverage for menu frames specifically.** The pixel-hash harness
(`traces/pixels/`) and the wasm pixel-comparison tooling (`scripts/wasm_pixel_harness.mjs`,
`dump_frame_raw`) both only ever capture frames from the *outer* per-tick loop
(`dump_frame_pixels`/`dump_frame_state`, called from `play_level_2_impl` after `play_frame()`
returns) — while the pause menu is open, execution is inside `do_paused()`'s own blocking
inner loop (`draw_menu`), which never reaches that call site at all (this is also *why* the
Esc-crash regression test can't cleanly close the menu again — see `open_menu.txt`'s header).
So today there is **no** pixel/state capture mechanism for "what does the menu actually look
like" on either build, native or wasm — the new regression test only proves "doesn't crash,"
not "renders correctly" or "looks the same as native." Two viable directions, not yet decided
between:
   - Add a parallel capture call inside `draw_menu`'s loop (or wherever it redraws), gated the
     same way (`POPPIXELS_OUT`/`POPRAWFRAME_TICK`-style), producing its own tick-like counter
     independent of the frozen outer `tick_counter` so menu frames get their own comparable
     sequence.
   - Or: since the menu can't be scripted to close cleanly via keyboard today, extend the
     scripted-input mechanism (or add a dedicated test-only escape hatch) enough to script a
     *specific* sequence of menu navigation (open → down → down → select → back → close) once
     mouse/keyboard-in-menu is fully audited (item 1) and reliable, capturing frames along the
     way. This is more work but gives real behavioral parity coverage, not just "didn't crash."

   Whichever direction, the end goal (matching the user's ask): confirm the menu actually
   *appears* and *renders correctly* on wasm (not just "didn't panic"), and that keyboard and
   (once implemented) mouse input produce the *same effect* as native for every action audited
   in item 1 — via real pixel/state comparison, the same rigor the climb z-order and RGB24 mask
   bugs were found and fixed with, not just eyeballing a screenshot.

**Suggested order:** 1 (audit, cheap, mostly reading; its most concrete finding, the
`std::env::set_var` crash risk, is already fixed) → 4's capture mechanism (needed to verify
anything else in this phase beyond "doesn't crash") → the rest of item 1's fixes, each verified
via 4 → 3 (mouse) → re-audit mouse-driven parity via 4 again.

## Future consideration (deferred, not scheduled): drop the SDL2 build dependency for wasm

Raised 2026-08-07 (README.md review): building for `wasm32-unknown-unknown` currently still
requires the real `SDL2`/`SDL2_image` development headers to be installed on the host, even
though the wasm build never links against real SDL2 and `WasmRenderer` never reads or writes an
`SDL_Surface`/`SDL_PixelFormat`/etc. by its actual field layout (every such type is used only as
an opaque pointer outside `platform/sdl.rs`, which is `#[cfg(not(target_arch = "wasm32"))]` and
never compiled for wasm at all — confirmed by a comment already in `build.rs`).

The dependency is a build-time side effect, not a real architectural need:
`bindgen::Builder::header("src/common.h")` parses that one shared header in a single pass for
*both* targets, to generate bindings for the thousands of other C declarations both builds
genuinely need (game structs, globals, function signatures) — and `common.h` unconditionally
`#include`s `<SDL2/SDL.h>` (via `types.h`). bindgen can't selectively skip a `#include` it can't
resolve, so if the real SDL2 headers aren't on disk, the whole parse fails — wasm ends up
needing headers on the host just to get bindgen through the file, even though the SDL-specific
types it produces from them are then only used as inert opaque pointers on that target.

**Why this matters enough to fix eventually:** the stated goal is for the wasm build to be
usable with minimal ceremony (e.g. cloning onto a bare server/droplet that only ever serves the
browser build, never runs anything native) — forcing a full native GUI library's dev headers
onto that machine for a build that never uses them at runtime is an unnecessary dependency, not
a real requirement.

**Possible approaches, none attempted yet:**
- Guard the `#include <SDL2/SDL.h>` in `types.h` behind a preprocessor condition bindgen can be
  told to set only for the wasm parse (e.g. a `SDLPOP_WASM_BINDGEN` define), paired with a
  minimal hand-written stub declaring just the SDL type *names* bindgen needs to see (no real
  field layout required, since wasm only ever uses them as opaque pointers) so the rest of
  `common.h` still parses cleanly.
- Or: split the bindgen invocation itself so the wasm target parses a trimmed header that
  doesn't reach the SDL include at all, if audit shows nothing wasm-relevant actually needs
  bindings generated from inside the SDL-touching portion of the header chain.

Either way this needs a careful audit first: something in the header chain currently visible to
bindgen might turn out to matter for wasm bindings generation in a way that isn't obvious until
tried (e.g. a struct that embeds an `SDL_*` type by value rather than by pointer). Not scheduled
now; a real but small piece of follow-up work.
