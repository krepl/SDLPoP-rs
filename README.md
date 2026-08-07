# SDLPoP-rs

A Rust port of [SDLPoP](https://github.com/NagyD/SDLPoP), the open-source reconstruction of
the DOS game *Prince of Persia*. The port is complete — every gameplay source file has been
translated from C to Rust, block-by-block, with behavior parity against the original C build
enforced by an automated differential test harness (state *and* rendered-pixel comparison, not
just "it compiles"). The game also runs in a browser via WebAssembly.

This file covers what's specific to this fork: the port itself, the test harness, building the
three targets (C, native Rust, wasm), and the dev tooling. Gameplay — controls, replays, mods,
cheats — is unchanged from upstream and documented in
**[README.upstream.md](README.upstream.md)**, preserved exactly as it was at the point this
project forked from SDLPoP (see [About this fork](#about-this-fork) below).

## Table of contents

- [About this fork](#about-this-fork)
- [Project status](#project-status)
- [Testing & correctness](#testing--correctness)
- [Building](#building)
  - [Prerequisites](#prerequisites)
  - [The original C build](#the-original-c-build)
  - [The Rust build](#the-rust-build)
  - [The WASM / browser build](#the-wasm--browser-build)
- [Development tooling (`cargo xtask`)](#development-tooling-cargo-xtask)
- [WASM / browser support](#wasm--browser-support)
- [Repository layout](#repository-layout)
- [Playing the game](#playing-the-game)
- [License & attribution](#license--attribution)

## About this fork

SDLPoP-rs started as a straight fork of [NagyD/SDLPoP](https://github.com/NagyD/SDLPoP) and is
being incrementally rewritten in Rust, one original source file at a time, while keeping
behavior bug-for-bug identical to the C version (see [Port — prime
directives](CLAUDE.md#port--prime-directives) in `CLAUDE.md` for the porting rules). It's now
also being extended to run in the browser via WebAssembly, which the original project never
supported.

`README.upstream.md` is the original project's `README.md`, frozen at the commit right before
Rust-port work began — nothing in it has been edited since (confirmed by checking its git
history: the only README change during the entire porting effort was adding a `cargo xtask`
blurb, which has since moved here). Everything it describes — controls, cheats, replays, mod
support, the original compiling instructions — is still accurate for this project too, since
faithful behavior preservation is the whole point of the port. Only its "COMPILING" section is
incomplete now: it only covers building the original C version, which this repository extends
with two more build targets (see [Building](#building)).

## Project status

Every gameplay source file (`seg000.c` through `seg009.c`, `seqtbl.c`, `options.c`, `replay.c`,
`lighting.c`, `screenshot.c`, `menu.c`, `midi.c`, `opl3.c`, `state_dump.c`) has been ported to
Rust. Two files remain — and are intended to remain — C:

- `data.c`: pure global-variable definitions via a `#define BODY` trick with no clean Rust
  equivalent worth chasing.
- `stb_vorbis.c`: a third-party Vorbis decoder, not worth hand-porting.

The original C sources for every *ported* file are also still present under `src/` and still
build — they're the reference oracle the test harness compares the Rust port against, not dead
code. See [The original C build](#the-original-c-build) (and its note about `src/Makefile`
currently being stale — use the CMake build).

The wasm/browser build is functional (rendering, audio, keyboard input, save/restart) with a
few known gaps — see [WASM / browser support](#wasm--browser-support).

## Testing & correctness

The core promise of this port is that it behaves *exactly* like the original C game — not
"close enough," but verified. This is enforced by a differential test harness that runs the
same set of recorded gameplay replays through both builds and compares the results:

- **State comparison**: every mutable game variable (character position, animation frame, HP,
  timers, room contents, ...) is dumped to a binary trace once per game tick and compared
  field-by-field against a golden trace generated from the C build. Any divergence — even one
  tick, one field — fails the test.
- **Pixel comparison**: an FNV-1a hash of the actual rendered frame is also captured once per
  tick and compared the same way. This exists because state parity alone doesn't prove the
  *rendering* is correct — a real bug was found and fixed this way (a wasm-only rendering issue
  that never touched game state at all, so the state comparison alone would never have caught
  it).
- **30 golden replays** (`doc/replays-testcases/`, traces committed under `traces/` and
  `traces/pixels/`) covering normal level completions, deaths by every hazard type, known
  historical bugs (grab bugs, wall-clip bugs), and edge cases (time-limit expiry, a
  prince-disappears bug). Regenerating goldens from the C oracle and diffing are both one
  command — see the `harness` row in the [xtask table](#development-tooling-cargo-xtask).
- **Live-input tests**, separate from replay playback: scripted keyboard input driven through
  the real SDL event queue (not canned replay data) confirms movement and the pause menu work
  end-to-end, on both native and — for the pause menu specifically, since that's what actually
  caught a real crash — the wasm build too.
- **Unit tests** (`cargo test --lib`) for pure/isolable logic, alongside the differential
  harness rather than instead of it — see `CLAUDE.md`'s testing philosophy notes.

`cargo xtask verify` runs all of the above (plus a full build and the wasm32 type-check) in
about half a minute on a modern multi-core machine — the harness's replay comparisons run in
parallel. This is the one command to run before considering a change done.

## Building

### Prerequisites

- `SDL2` and `SDL2_image` development libraries (all three build targets need these; the wasm
  build implements its own renderer but still links against the same C dependencies indirectly
  through shared code).
- A Rust toolchain (native and wasm builds).
- For the wasm build specifically: the `wasm-bindgen` CLI at the exact version pinned in
  `Cargo.lock`, and Node.js + `npm install` if you also want to run the wasm regression tests.

### The original C build

Kept intentionally buildable — it's the reference oracle the test harness diffs the Rust port
against, not legacy cruft.

```sh
cd src/build && cmake -G Ninja .. && ninja   # -> ../prince
```

`src/Makefile` (plain `make` in `src/`) is currently stale — it's missing several source files
(`seg004.c`–`seg007.c`) and won't link. Use the CMake build above; `src/CMakeLists.txt` has the
complete file list.

### The Rust build

```sh
cargo build                          # -> target/debug/prince
./target/debug/prince
```

Run `./target/debug/prince validate replays/some_replay.p1r` to play a replay headlessly and
print a summary, or see `CLAUDE.md`'s harness section for the full replay/trace workflow.

### The WASM / browser build

```sh
cargo xtask wasm-build               # cargo build --target wasm32-unknown-unknown + wasm-bindgen + asset manifest
cargo xtask wasm-serve [--port N]    # serve web/ with the headers SharedArrayBuffer needs
```

Open the served URL, click the canvas (enables audio + keyboard focus), then play with arrow
keys/space/enter. See [WASM / browser support](#wasm--browser-support) for current limitations.

## Development tooling (`cargo xtask`)

All dev/test workflows are wrapped in a `cargo xtask` binary — run `cargo xtask --help` or
`cargo xtask <subcommand> --help` for details. Full list, roughly in the order you'd reach for
them:

| Command | What it does |
|---|---|
| `cargo xtask verify` | Everything: build, unit tests, wasm32 type-check, full harness, wasm test suite. Run this before considering a change done. |
| `cargo xtask harness [regen\|compare A B\|one REPLAY GOLDEN\|build]` | No subcommand: `smoke-test` + `gameplay-smoke-test` + `menu-smoke-test` (below) + state/pixel comparison across all 30 golden replays. `regen` refreshes goldens from the C oracle; `compare`/`one` are for debugging a single divergence. |
| `cargo xtask smoke-test [duration_seconds]` | Launches the real interactive binary and confirms it runs briefly without crashing — catches startup-only bugs the replay-based harness can't see (it never creates a real window). Already part of `harness`/`verify`. |
| `cargo xtask gameplay-smoke-test` | Scripted keyboard input through the real SDL event queue, asserting the Kid's position actually moves — proves live input works, not just canned replay data. Already part of `harness`/`verify`. |
| `cargo xtask menu-smoke-test` | Opens the pause menu via scripted input and confirms the native build doesn't crash. Already part of `harness`/`verify`. |
| `cargo xtask wasm-build` | Builds the wasm32 target and regenerates `web/pkg/` via `wasm-bindgen`. |
| `cargo xtask wasm-serve [--port N]` | Serves the browser build with the COOP/COEP headers `SharedArrayBuffer` requires; rebuilds first only if stale. |
| `cargo xtask wasm-menu-smoke-test` | The wasm counterpart to `menu-smoke-test` — this is the one that actually matters, since it caught a real crash native's already-complete SDL implementation couldn't. Needs `npm install`. |
| `cargo xtask wasm-verify` | Checks Node/Playwright are installed (clear error if not), rebuilds the wasm bundle, runs the wasm test suite. Part of `verify`. |
| `cargo xtask quicksave-fixture` | Compiles the standalone C oracle and captures a fresh quicksave test fixture. |

## WASM / browser support

The game runs in a browser via a Web Worker running the same, unmodified `pop_main()` game
loop compiled to `wasm32-unknown-unknown` — not a rewrite of the game logic, the identical Rust
code the native build runs. Working: rendering, audio, keyboard input, save/restart, the pause
menu. Known gaps, tracked in `docs/plans/13-platform-architecture-unification.md`'s "Phase D":

- Mouse input isn't wired up in the browser yet (keyboard-only for now).
- Fullscreen toggling and cursor-hiding are inert (no-ops) — need a small JS bridge
  (Fullscreen API, CSS) that hasn't been built yet.
- Quicksave/quickload don't persist across a page reload (no durable browser storage wired up
  yet), and one related code path (`SDLPOP_SAVE_PATH` handling) is a known, not-yet-confirmed
  crash risk on wasm specifically.

None of these are believed to be fundamental — see the Phase D plan for the full list and
suggested order.

## Repository layout

| Path | What's there |
|---|---|
| `src/` | Original C source. Two files (`data.c`, `stb_vorbis.c`) are permanently C; the rest is kept as the test harness's reference oracle. |
| `rust/` | The Rust port. `rust/src/lib.rs` is the crate root. |
| `xtask/` | The `cargo xtask` dev-tooling binary. |
| `web/` | The wasm/browser harness: `index.html`/`worker.js` (interactive), `headless.html`/`headless.mjs` (automated testing). |
| `scripts/` | Shell/Node scripts the harness and xtask commands wrap (replay diffing, smoke tests, wasm test drivers). |
| `traces/` | Committed golden state traces and pixel-hash traces, one pair per test replay. |
| `doc/replays-testcases/` | The recorded `.p1r` replay files the harness plays back. |
| `docs/plans/` | Design docs for larger pieces of work (the wasm/platform architecture plan, porting plans). |
| `docs/blog-assets/` | Screenshots and notes kept for a future write-up about this project. |
| `data/` | Game assets (`.DAT` files, music) — same as upstream. |
| `mods/` | Custom levelsets — same as upstream. |

## Playing the game

Controls, cheats, replay recording/viewing, mod support, and the original per-platform
compiling instructions are all in **[README.upstream.md](README.upstream.md)** — unchanged and
still accurate, since this port's entire goal is to reproduce the original game exactly.

## License & attribution

GPLv3 — see `COPYING` and `src/GPLv3.h`. `src/opl3.c`/`.h` and `src/stb_vorbis.c` are
third-party code under their own licenses (see their file headers). The full list of original
SDLPoP authors and contributors is preserved in
[README.upstream.md](README.upstream.md#authors).
