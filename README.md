# SDLPoP-rs

A Rust port of [SDLPoP](https://github.com/NagyD/SDLPoP) (open-source *Prince of Persia*),
verified bug-for-bug identical to the original C build by an automated test harness. Also runs
in the browser via WebAssembly.

Gameplay, controls, mods, and replays are unchanged from upstream. See
**[README.upstream.md](README.upstream.md)** for all of that. This file is just the Rust/wasm
side: how to build it and where to look for more.

## Quick start

These three builds are independent; pick whichever one you want.

### Native (Rust)

```sh
cargo run   # builds and launches the game
```

### Browser (wasm)

```sh
cargo xtask wasm-serve   # builds the wasm bundle if needed, then serves it
```

Then open the URL it prints (`http://localhost:8642/` by default).

### Original C build

Kept as the test harness's reference oracle, not for everyday use.

```sh
cd c/build && cmake -G Ninja .. && ninja   # builds ../prince
```

### Dependencies

* `SDL2` and `SDL2_image` development libraries: needed by the native Rust build and the
  original C build. The wasm build does *not* need them — it never links against real SDL2, and
  a minimal stand-in header (`c/sdl_stub.h`) covers what the build needs to generate Rust
  bindings, so a machine that only ever builds/serves the browser version needs nothing here.
  See [README.upstream.md's COMPILING section](README.upstream.md#compiling) for per-OS install
  instructions.
* `wasm-bindgen-cli`, pinned version: needed to build the wasm bundle. `cargo xtask wasm-build`
  prints the install command if the installed version doesn't match.
* Node.js and `npm install`: only needed to run the wasm regression tests, not to build or
  serve the wasm bundle.

## Testing

```sh
cargo xtask verify   # build, unit tests, and the full differential harness
```

The harness replays real recorded gameplay through both the Rust and C builds and diffs every
tick: game state and rendered pixels, not just "it compiles." Expected output is committed as
"golden" traces and pixel hashes under `traces/`, generated from the C build; run this before
considering any change done. `cargo xtask --help` lists everything else (individual harness
pieces, smoke tests, wasm tooling).

## Repository layout

* `c/` original C source, kept as the harness's reference oracle
* `rust/` the Rust port
* `web/` the wasm/browser build
* `traces/`, `doc/replays-testcases/` golden test data
* `docs/plans/` design docs for larger pieces of work
* `xtask/` the `cargo xtask` dev tooling

## Learn more

* **`CLAUDE.md`**: porting rules, architecture, wasm platform design, harness internals
* **`docs/plans/`**: design docs
* **`README.upstream.md`**: gameplay, mods, replays, the original project's compiling docs

## License

GPLv3, see `COPYING`. `c/opl3.c`/`.h` and `c/stb_vorbis.c` are third-party code under their
own licenses. Full author/contributor list in
[README.upstream.md](README.upstream.md#authors).

Rust/wasm port: [krepl](https://github.com/krepl), AI-assisted with Claude Code.
