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
  completely untouched. **✅ Done, commit `4111b02`.**

**Known tradeoff, accepted for a first working version:** without `SharedArrayBuffer` (which
needs COOP/COEP cross-origin-isolation HTTP headers — a real deployment constraint), there's
no efficient blocking-sleep primitive available inside a Worker for frame pacing, so
`Renderer::delay` falls back to a busy-spin (checking wall-clock time in a tight loop) rather
than a real sleep — costs CPU/battery on that one background thread while a level is running,
not correctness. Real efficient sleep via `Atomics.wait` is a deferred future refinement, same
spirit as Phase A's deferred alpha/`ADD`/`MOD` blend-mode compositing.

**Scope for this phase:**
1. `setjmp`/`longjmp` fix (`seg000.rs` + `wasm_libc.rs`, wasm32-only). **✅ Done.**
2. `WasmRenderer::present()`/texture pipeline — post the finished frame buffer to the main
   thread. **✅ Texture/render-target pipeline done** (commit `b31b0ff`):
   `create_texture`/`update_texture`/`set_render_target`/`render_clear`/`render_copy`/
   `render_present`/`render_set_logical_size`/`get_renderer_output_size` all have real
   `WasmRenderer` implementations (in-memory texture store + screen buffer, verified with a
   pixel-parity test that reads back the presented frame). **Still not started**: actually
   posting the presented frame across the Worker/main-thread boundary — that's real JS harness
   work (item 5), not implementable/testable from the Rust side alone.
3. `WasmInput` — receive key/mouse state via a message queue populated by the main thread.
   **Partially done**: real `key_state`/`mouse_state` storage and `set_key_state`/
   `set_mouse_state` entry points exist (commit `e5ba566`), and `WasmRenderer::num_joysticks`
   now correctly reports zero (commit pending) so `set_joy_mode` takes the keyboard-only
   branch. Nothing calls the JS-facing setters yet — no actual JS event listener wiring, since
   there's no Worker harness to wire them into. **New wall found (not yet started):**
   `Renderer::poll_event` is still `unimplemented!()` — `process_events()`
   (`seg009.rs:4610`) expects a real `SDL_Event` queue (edge-triggered KEYDOWN/KEYUP, mouse
   button/motion, QUIT, window events), but `WasmInput`'s current design only exposes
   level-triggered key/mouse *state*, not a queue of discrete events. Closing this needs an
   actual synthetic-event-queue design (JS pushes discrete key/mouse events, Rust buffers them,
   `poll_event` drains one per call) — deliberately not stubbed with a quick "always return no
   event," since `process_events` does real gameplay-affecting bookkeeping (fast-forward
   toggle, screenshot hotkeys, fullscreen toggle, last-key tracking) that a silent no-op would
   quietly break. Treated as part of item 5's JS harness work, not a cheap empirical fix.
4. `WasmAudio` — post PCM buffers for the main thread to play. **Not started**, still all
   `unimplemented!()`.
5. JS harness: a dedicated worker script loading the wasm module and relaying messages;
   `web/index.html` updated to spawn the worker, paint received frames to the canvas, forward
   input events, and play received audio. **Not started.** This is now the natural next step —
   items 2's texture pipeline and 3's input-queue design both need a real message-passing
   counterpart to test against, and probing further via the headless Node harness alone yields
   diminishing returns past this point.
6. Enough of `sdl_init`/`create_window`/`create_renderer`/window-lifecycle stubs to get
   `pop_main()` actually running inside the worker without hitting `unimplemented!()` on the
   startup path — scope this empirically (run it, see what it hits next) rather than trying
   to predict the full startup call graph up front. **Substantial progress, ongoing**: via a
   Node-based `run_game()` probe (no browser needed for this), fixed `rw_from_file`,
   `set_hint`, `sdl_init`/`sdl_init_subsystem`/`sdl_quit`, `create_window`/`create_renderer`,
   `get_renderer_info_flags`, real `WasmInput`, real PNG decoding, `set_window_icon`,
   `render_set_logical_size`, the texture/render-target pipeline, and `num_joysticks`
   (commits `e5ba566`, `9a3613a`, `b31b0ff`). Current wall: `Renderer::poll_event` (see item 3)
   — the probe has now run the startup path all the way to the first real per-frame event-pump
   call, past every SDL init/lifecycle stub.

**Exit criteria:** the `setjmp`/`longjmp` gap is resolved (or has a concretely updated plan,
not a silent panic); a real frame produced by actual gameplay code (not the Phase-2-milestone
test gradient) reaches the browser canvas via the Worker, driven by the genuinely unmodified
game loop.

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
- **The `catch_unwind`/`panic_any(RestartGameSignal)` mechanism used for wasm32 today
  (commit `4111b02`) is explicitly acknowledged tech debt, not a real fix — keep it only
  until the real fix below lands.** Using a panic for ordinary control flow (not an actual
  error) is a Rust anti-pattern: panics are supposed to signal "something went wrong," and
  overloading them for "the player pressed Ctrl+R" makes real bugs harder to distinguish
  from intentional restarts, and makes the control flow just as opaque as the `longjmp` it
  replaced — a different-shaped version of the same problem, not a solution to it.
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
