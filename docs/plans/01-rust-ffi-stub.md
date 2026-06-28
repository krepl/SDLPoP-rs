# Plan 1: Rust FFI Stub

**Goal:** Introduce a Rust build that compiles all SDLPoP C sources into a static
library, links against it, and drives the game from a Rust `main()`. The C code is
not touched. This gives us a working Rust binary, a solved build-system integration
problem, and a test harness for catching regressions in later port phases.

---

## Repository layout after this plan

```
SDLPoP/
  Cargo.toml          ← new: Rust package manifest
  build.rs            ← new: drives cc + bindgen
  rust/
    src/
      main.rs         ← new: Rust entry point (calls pop_main)
      lib.rs          ← new: re-exports bindings; home for #[cfg(test)] modules
  docs/
    plans/
      1-rust-ffi-stub.md   ← this file
  src/                ← existing C sources, completely untouched
  data/               ← existing game assets, untouched
  SDLPoP.ini
```

`src/` stays as-is. The Rust toolchain sits alongside it, not inside it. The
existing `make` and CMake builds continue to work unchanged.

---

## Dependencies

Add to `Cargo.toml` (exact versions pinned at write time; update as needed):

| Crate | Where | Purpose |
|-------|-------|---------|
| `cc` | `[build-dependencies]` | Compile C sources into a static lib |
| `bindgen` | `[build-dependencies]` | Generate `bindings.rs` from `common.h` |
| `pkg-config` | `[build-dependencies]` | Resolve SDL2 include paths and link flags |

Runtime dependencies: none (SDL2 is linked natively via pkg-config).

System requirements (must already be installed):
- `libclang` / `clang` — required by bindgen at build time
- `libsdl2-dev`, `libsdl2-image-dev` — same as the existing C build

---

## Step-by-step checklist

### Step 1 — `Cargo.toml`

- [ ] Create `Cargo.toml` at the project root.
- [ ] Set `name = "sdlpop"`, `edition = "2021"`.
- [ ] Declare the binary with a custom path:
  ```toml
  [[bin]]
  name = "prince"
  path = "rust/src/main.rs"
  ```
- [ ] Declare the library (needed so `#[cfg(test)]` in `lib.rs` is picked up by
  `cargo test`):
  ```toml
  [lib]
  name = "sdlpop"
  path = "rust/src/lib.rs"
  ```
- [ ] Add build dependencies: `cc`, `bindgen`, `pkg-config`.
- [ ] Add `.gitignore` entry for `target/` if not already present.

### Step 2 — `build.rs`

This is the most complex piece. It has three responsibilities:
compile the C library, emit link directives, and generate bindings.

#### 2a — Resolve SDL2 paths

- [ ] Use `pkg_config::Config::new().probe("sdl2")` and `probe("SDL2_image")` to
  get include paths and link flags.
- [ ] Collect all include paths into a `Vec<PathBuf>` for use by both `cc` and
  bindgen.

#### 2b — Compile C sources

- [ ] List every `.c` file in `src/` **except `src/main.c`** (Rust provides `main`).
  Full list:
  `data.c`, `seg000.c`, `seg001.c`, `seg002.c`, `seg003.c`, `seg004.c`,
  `seg005.c`, `seg006.c`, `seg007.c`, `seg008.c`, `seg009.c`,
  `seqtbl.c`, `replay.c`, `options.c`, `lighting.c`, `screenshot.c`,
  `menu.c`, `midi.c`, `opl3.c`, `stb_vorbis.c`
- [ ] Configure a `cc::Build`:
  - `.std("c99")`
  - `.define("_GNU_SOURCE", "1")`
  - `.flag("-O2")`
  - `.flag("-w")` — suppress warnings from the C code (they are expected and not
    our concern at this phase)
  - Add each SDL2 include path with `.include(path)`
  - Add each source file with `.file(path)`
- [ ] Call `.compile("sdlpop")` — this emits `cargo:rustc-link-lib=static=sdlpop`
  automatically.

#### 2c — Emit link directives

- [ ] Iterate `sdl2.libs` and `sdl2_image.libs` from pkg-config, emitting
  `println!("cargo:rustc-link-lib={}", lib)` for each.
- [ ] Emit `println!("cargo:rustc-link-lib=m")` for the math library.
- [ ] Emit `println!("cargo:rustc-link-search=...")` for any non-standard lib
  search paths returned by pkg-config.

#### 2d — Generate bindings

- [ ] Build a `bindgen::Builder`:
  - `.header("src/common.h")`
  - `.clang_arg("-std=c99")`, `.clang_arg("-D_GNU_SOURCE=1")`
  - Add each SDL2 include path as `.clang_arg(format!("-I{}", path.display()))`
  - `.allowlist_file(r".*src/.*")` — restricts generated bindings to symbols
    defined in SDLPoP's own headers, excluding SDL2 headers. This keeps
    `bindings.rs` to a manageable size and avoids conflicts with any future
    `sdl2` Rust crate usage.
  - `.parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))` — re-runs
    bindgen when headers change.
- [ ] Write output to `$OUT_DIR/bindings.rs`.

### Step 3 — `rust/src/lib.rs`

- [ ] Include the generated bindings:
  ```rust
  #![allow(non_upper_case_globals)]
  #![allow(non_camel_case_types)]
  #![allow(non_snake_case)]
  #![allow(dead_code)]

  include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
  ```
- [ ] Add an empty `#[cfg(test)] mod tests {}` block as a placeholder for Phase 2
  regression tests.

### Step 4 — `rust/src/main.rs`

`main.c` does two things before calling `pop_main()`: it sets `g_argc` and `g_argv`
(globals declared in `data.h`). Rust's `main` must replicate this.

- [ ] Bring the library bindings into scope: `use sdlpop::*;`
- [ ] Collect `std::env::args()` into a `Vec<CString>` and a corresponding
  `Vec<*mut c_char>`.
- [ ] Inside `unsafe { ... }`:
  - Set `g_argc` to the argument count.
  - Set `g_argv` to the pointer to the argv array.
  - Call `pop_main()`.
- [ ] Keep the `CString` vec alive for the duration of `pop_main()` — hold it in a
  named `let` binding before the `unsafe` block, not as a temporary, or Rust will
  drop it before C reads it.

### Step 5 — Verify the build

- [ ] Run `cargo build` and confirm it compiles without errors.
- [ ] Run `cargo run` from the project root and confirm the game launches
  identically to the C-built `./prince`.
- [ ] Run `cargo build --release` and confirm the release binary works.
- [ ] Confirm the existing `make` build in `src/` still works (no C files were
  modified).

### Step 6 — First Rust test (smoke test)

- [ ] In `rust/src/lib.rs`, under `#[cfg(test)]`, write a test that:
  1. Calls `set_options_to_default()` to bring global state to a known baseline.
  2. Calls `prandom(15)` with a known `random_seed` value and asserts the result
     matches the expected output (determine the expected value from the first run).
- [ ] Run `cargo test` and confirm it passes.

---

## Bindgen configuration notes

### Why `allowlist_file(r".*src/.*")`?

Without filtering, bindgen walks all transitively included headers, including the
entire SDL2 public API. This produces a very large `bindings.rs` full of SDL types
we don't need and which may conflict later when we introduce the `sdl2` Rust crate
for pure-Rust phases. The allowlist limits output to symbols whose definition lives
in `src/`.

### Packed structs

`types.h` uses `#pragma pack(push,1)` for several structs (`level_type`,
`dat_header_type`, `instrument_type`, etc.). Bindgen correctly emits
`#[repr(C, packed)]` for these. Accessing fields of packed structs in Rust requires
`unsafe` because unaligned reads are undefined behaviour on some architectures.
This is expected and correct — do not work around it in this phase.

### The `#ifdef BODY` trick in `data.h`

`data.h` uses `#ifdef BODY` to switch between emitting definitions (in `data.c`)
and `extern` declarations (everywhere else). When bindgen parses `common.h` without
`BODY` defined, it sees only the `extern` declarations — which is exactly what we
want. It generates bindings to the existing global symbols in the compiled C
library, not duplicate definitions.

---

## Testing strategy

The goal of tests at this phase is to create a regression baseline. A test that
passes against the C library must also pass once the corresponding C module is
replaced by Rust in Plan 2.

### What is testable

Pure or near-pure computation functions that do not call into SDL:

| Module | Testable functions | Notes |
|--------|--------------------|-------|
| `seg004.c` | `get_left_wall_xpos`, `get_right_wall_xpos`, `is_obstacle` | Reads tile globals; requires level state setup |
| `seg006.c` | `x_to_xh_and_xl`, `fall_accel`, `fall_speed` | Mostly arithmetic; easy to test in isolation |
| `seg009.c` | `prandom` | Pure RNG with known output |
| `options.c` | `set_options_to_default` | Sets known global state; useful as test fixture setup |
| `seqtbl.c` | `check_seqtable_matches_original` | Debug assertion; validates internal consistency |

### What is not testable in unit tests

- Any function that calls into SDL (rendering, input, audio).
- Functions that open files (`load_level`, `load_sounds`, etc.) without a real
  `data/` directory present.
- The main loop (`pop_main`, `play_frame`).

### Test pattern

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    #[test]
    fn prandom_known_output() {
        setup();
        unsafe {
            random_seed = 0;
            let result = prandom(15);
            assert_eq!(result, /* fill in after first run */);
        }
    }
}
```

Each Plan 2 PR should add tests for a module before replacing it, so the test
suite grows as a side-effect of the port.

---

## Known pitfalls

1. **`g_argc`/`g_argv` lifetime.** The `Vec<CString>` and `Vec<*mut c_char>` must
   stay alive until `pop_main()` returns. Assign them to named `let` bindings
   before the `unsafe` block; do not construct them inline as temporaries.

2. **`cargo run` working directory.** The game expects `SDLPoP.ini`, `data/`, and
   `mods/` relative to the current directory. Always run `cargo run` from the
   project root.

3. **`-lm` link order.** On some Linux linkers, the math library must come after
   the object files that reference it. The `cc` crate emits the static lib link
   directive before we emit `-lm`, which is the correct order.

4. **bindgen requires `libclang`.** On Debian/Ubuntu: `sudo apt install libclang-dev`.
   Build-time only; does not affect the runtime binary.

5. **Warnings from the C code.** The `.flag("-w")` in `build.rs` suppresses all C
   warnings intentionally — the C code is not ours to fix at this stage, and warning
   noise obscures Rust compiler output.

6. **`cargo test` links SDL2.** Tests that call any C function pull in the full
   linked binary including SDL2. Restrict test functions to the non-SDL list above.
   If a headless CI environment cannot initialise SDL2, tests that touch SDL will
   fail — keep them out of the test suite.

---

## Out of scope for this plan

- Any modification to the C source code.
- Making the generated bindings idiomatic or safe.
- Replacing any C module with Rust (that is Plan 2).
- Windows or macOS build support (Linux only for now).
