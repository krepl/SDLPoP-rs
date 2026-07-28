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
- `setjmp`/`longjmp` still needs a real fix (this part of the original plan stands), but it
  turns out much smaller in isolation than assumed: it's exactly one `longjmp` call site, and
  the fix does **not** require touching any of the 12 restart call sites. Wrap just the outer
  `start_game()`-calling loop (`init_game_main`, `seg000.rs`) in `catch_unwind`, and make
  wasm's `longjmp` shim (`wasm_libc.rs`) panic with a distinguishable marker type that
  wrapper catches and turns into a retry. Contained to `seg000.rs` + `wasm_libc.rs`, gated
  `#[cfg(target_arch = "wasm32")]` — native keeps using real libc `setjmp`/`longjmp`,
  completely untouched.

**Known tradeoff, accepted for a first working version:** without `SharedArrayBuffer` (which
needs COOP/COEP cross-origin-isolation HTTP headers — a real deployment constraint), there's
no efficient blocking-sleep primitive available inside a Worker for frame pacing, so
`Renderer::delay` falls back to a busy-spin (checking wall-clock time in a tight loop) rather
than a real sleep — costs CPU/battery on that one background thread while a level is running,
not correctness. Real efficient sleep via `Atomics.wait` is a deferred future refinement, same
spirit as Phase A's deferred alpha/`ADD`/`MOD` blend-mode compositing.

**Scope for this phase:**
1. `setjmp`/`longjmp` fix (`seg000.rs` + `wasm_libc.rs`, wasm32-only).
2. `WasmRenderer::present()` — post the finished frame buffer to the main thread.
3. `WasmInput` — receive key/mouse state via a message queue populated by the main thread.
4. `WasmAudio` — post PCM buffers for the main thread to play.
5. JS harness: a dedicated worker script loading the wasm module and relaying messages;
   `web/index.html` updated to spawn the worker, paint received frames to the canvas, forward
   input events, and play received audio.
6. Enough of `sdl_init`/`create_window`/`create_renderer`/window-lifecycle stubs to get
   `pop_main()` actually running inside the worker without hitting `unimplemented!()` on the
   startup path — scope this empirically (run it, see what it hits next) rather than trying
   to predict the full startup call graph up front.

**Exit criteria:** the `setjmp`/`longjmp` gap is resolved (or has a concretely updated plan,
not a silent panic); a real frame produced by actual gameplay code (not the Phase-2-milestone
test gradient) reaches the browser canvas via the Worker, driven by the genuinely unmodified
game loop.

---

## Phase C — Unified file I/O via `FileSystem`

**Why this doesn't need native to change much:** the `FileSystem` trait already exists
(Step C) but DAT-file/config loading still calls raw `fopen`/`fread` directly, bypassing it
entirely. The fix is call-site migration, not a new native-side preload mechanism — native's
`FileSystem` impl can keep doing real synchronous `fopen`/`fread` under the hood, unchanged
behavior. Only the **web** implementation differs in *how* it's populated: an async preload
phase (JS `fetch`, then an exported `preload_file(path, bytes)` call) populates an in-memory
store before the game starts, and `WasmFiles::read_file` serves synchronously from that store
from the game's perspective — no async-aware rewrite of gameplay code needed.

**Scope:** find and convert every game-facing raw `fopen`/`fread`/`fwrite`/`fclose` call
(DAT files, `SDLPoP.ini`, replays, quicksaves, hall-of-fame, mod files — likely concentrated
in `seg009.rs` and `options.rs`) to `FileSystem::read_file`/`write_file`/`file_exists`. Keep
`wasm_libc.rs`'s own `fopen`-family stub as the low-level fallback for anything that
legitimately isn't a real asset (there shouldn't be much left once this phase is done).

**Web asset list:** define an explicit manifest of files the game needs before it can start
(the base `data/*.DAT` set at minimum) rather than trying to fetch-on-demand mid-tick, since
`read_file` must return synchronously once called. Fetching on demand is a possible future
refinement, not this phase's scope.

**Exit criteria:** no game-facing file access happens outside the `FileSystem` trait. Native
behavior unchanged (still real synchronous file I/O). Web can load real assets via
preload-then-serve-from-memory.

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
