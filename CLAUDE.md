# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

SDLPoP is an open-source C port of the DOS game Prince of Persia, based on a disassembly of the original executable. The code structure deliberately mirrors the original segmented DOS memory model — function comments like `// seg000:024F` map back to disassembly offsets. This origin is important context: many seemingly odd patterns (global state in a single header, segment-named files) are inherited from the disassembly, not design choices.

## Building

Dependencies: `SDL2` and `SDL2_image` development libraries.

**Linux (make):**
```sh
cd src
make
# Binary is output to ../prince (project root)
```

**Linux (CMake with Ninja — preferred for speed):**
```sh
cd src/build          # CMakeLists.txt is in src/, not the project root
cmake -G Ninja ..
ninja
# Binary is output to the project root (../prince)
```

**Run:**
```sh
./prince                    # normal start
./prince megahit 3          # start at level 3 with cheats
./prince debug              # enable debug cheats
./prince mod "Mod Name"     # play a mod from mods/
```

**Replays and differential harness:**

Replay files live in `replays/`. To play one headlessly (auto-exits, no title screen):
```sh
./prince validate replays/foo.p1r
```
Important: the replay file must be passed as a plain positional argument after `validate` (space-separated). Do NOT pass a level number alongside `validate` — it breaks replay loading. Do NOT use `seed=` with validate — the replay file must be `argv[1]` for seed to be honoured, which conflicts with `validate`.

The differential harness lives in `scripts/run_harness.sh`:
```sh
scripts/run_harness.sh --regen   # regenerate golden trace from current binary
scripts/run_harness.sh           # run binary, compare output against golden trace
scripts/run_harness.sh --compare A.trace B.trace  # diff two arbitrary traces
```

The golden trace (`traces/golden.trace`) was generated from the all-C build and is committed as the correctness reference. `compare_traces.py` supports `--all`, `--tick N`, `--ignore FIELD`.

**Harness gotchas:** The game `chdir(exe_dir)` when loading a replay (see `replay.c:277`), so relative paths passed via `POPTRACE_OUT` break. The harness uses absolute paths to work around this. The Rust binary (`target/debug/prince`) resolves data assets relative to its own path, so `target/debug/data` and `target/debug/replays` must be symlinked to the project-root copies — the harness does this automatically.

## Architecture

All source is in `src/`. The codebase is pure C (C99), structured around the original DOS segments:

| File | Responsibility |
|------|---------------|
| `seg000.c` | Main loop (`pop_main`), game initialization, input, sound loading, sprite loading, HP display |
| `seg001.c` | Cutscene playback, sequence rendering for kid and opponent |
| `seg002.c` | Guard/shadow AI: initialization, HP, fallout checks, guard logic |
| `seg003.c` | Level initialization (`init_game`), the per-frame level loop (`play_level_2`), room redraw |
| `seg004.c` | Collision detection: wall/floor/ceiling checks, bump logic |
| `seg005.c` | Character movement: sequence table execution, falling, landing, control input, sword combat |
| `seg006.c` | Tile system: tile lookup, frame data, character position/direction helpers |
| `seg007.c` | Animated tiles ("trobs" = triggered objects): gates, spikes, loose floors, chompers |
| `seg008.c` | Room rendering: `draw_room`, `draw_tile`, wall drawing algorithm |
| `seg009.c` | Platform layer: SDL init/teardown, file I/O, path resolution, DAT file loading |
| `seqtbl.c` | Animation sequence bytecode table — defines every character animation as a byte stream |
| `options.c` | INI parser, `SDLPoP.ini` / `mod.ini` option loading, fixes/enhancements toggling |
| `replay.c` | Replay recording and playback (`.P1R` files) |
| `lighting.c` | Torch lighting and color palette effects |
| `screenshot.c` | Screenshot and level-map screenshot capture |
| `menu.c` | In-game pause menu |
| `midi.c` / `opl3.c` | MIDI playback via OPL3 emulation |

### Global state pattern

All game state variables are declared in `data.h` and defined in `data.c`. The trick: `data.h` uses `#ifdef BODY` — when included from `data.c` (which `#define BODY` first) it emits definitions with initializers; everywhere else it emits `extern` declarations. This means every `.c` file includes `common.h` → `data.h` and gets extern access to all globals.

### Header inclusion order

`common.h` is the single master include: it pulls in system headers, then `config.h`, `types.h`, `proto.h`, and `data.h` in that order. Every `.c` file starts with `#include "common.h"`.

### Compile-time feature flags

`config.h` controls features via `#define` / `#undef`: `USE_FADE`, `USE_FLASH`, `USE_COPYPROT`, `USE_QUICKSAVE`, `USE_REPLAY`, etc. These gates wrap optional game features.

### Fixes and enhancements system

Runtime bug fixes are controlled by the `fixes` pointer (set in `options.c`). When `use_fixes_and_enhancements` is true, `fixes` points to `fixes_saved` (user config); when false, it points to `fixes_disabled_state` (all off). Individual fixes are fields in this struct and are checked inline throughout the gameplay code.

## Configuration

- `SDLPoP.ini` — main config file in the project root (gameplay options, display, mods)
- `SDLPoP.cfg` — written by the in-game menu; overrides `.ini` until `.ini` is modified again
- `mods/<ModName>/mod.ini` — per-mod config that overrides gameplay options from `SDLPoP.ini`

## Data files

Game assets live in `data/`. `.DAT` files are the original DOS archive format. Music goes in `data/music/` as `.ogg` files (filenames listed in `data/music/names.txt`). Mods go in `mods/<ModName>/` and only need to include files that differ from the base game.

---

## Port — prime directives

All porting work is on `master`. These rules apply:

- **Faithful translation only.** Port each C function block-by-block, statement-by-statement. No refactoring, no idiomatic rewrites, no helper extraction.
- **Use `unsafe` freely.** Every function body should be `unsafe`. Don't fight it.
- **No behavior changes.** Reproduce weird C behavior exactly. Quirks may be load-bearing.
- **Fix harness divergence before moving on.** Run `cargo check` after each batch; run the harness before marking a subsystem done.
- **Model and effort selection:**
  - **Porting** (`pop-porter` agents): `model: "opus"`. Sonnet gets stuck on the trap categories in this codebase (signed/unsigned, `word` vs `c_short`, logical `!`). Opus is slower and more expensive but the retry cost with Sonnet is worse.
  - **Divergence debugging**: Opus + `/effort max` for the main session. Switch when a harness divergence doesn't yield to `--gen-test` + `--dump-tick` within one pass.
  - **Orchestration** (harness runs, commits, plan reading): Sonnet + `/effort high` is sufficient.
  - **Trap review** (`pop-reviewer`): Haiku is fine — it's a checklist pass, not reasoning.
- **Subagent verification (mandatory).** After any agent returns, immediately run `git log --oneline <worktree-branch>` to confirm new commits exist before treating the work as done. Agent prompts must state the target branch explicitly: "You are working on branch `master`." Do not report a subsystem as ported until `git log` confirms a commit and the harness passes on the merged result.

---

## Rust port

The game is being incrementally re-implemented in Rust. The Rust crate lives in `rust/` and is also the root crate (Cargo.toml at the project root links to it). Each ported C file becomes a Rust module exporting `#[no_mangle] pub unsafe extern "C"` functions with identical signatures, so the C linker sees no difference.

### Porting status (branch: master)

**Port is complete.** All gameplay C files are ported to Rust. Only `data.c` (global definitions via `#define BODY` trick — no clean Rust equivalent) and `stb_vorbis.c` (third-party Vorbis decoder — not worth hand-porting) remain as C and are intentionally permanent.

| File | Status |
|------|--------|
| seg000.c | ✅ ported |
| seg001.c | ✅ ported |
| seg002.c | ✅ ported |
| seg003.c | ✅ ported |
| seg004.c | ✅ ported |
| seg005.c | ✅ ported |
| seg006.c | ✅ ported |
| seg007.c | ✅ ported |
| seg008.c | ✅ ported |
| seg009.c | ✅ ported |
| seqtbl.c | ✅ ported |
| options.c | ✅ ported |
| replay.c | ✅ ported |
| sdl_rw_wrappers.c | ✅ ported |
| lighting.c | ✅ ported |
| screenshot.c | ✅ ported |
| menu.c | ✅ ported |
| midi.c | ✅ ported |
| opl3.c | ✅ ported |
| state_dump.c | ✅ ported |
| data.c | 🔒 permanent C (global data via `#define BODY`) |
| stb_vorbis.c | 🔒 permanent C (third-party library) |

When a file is ported: add it as a `pub mod` in `rust/src/lib.rs`, remove it from `build.rs` (the `sources` array), and run the harness to confirm parity.

**Do NOT remove from `src/CMakeLists.txt` or `src/Makefile`.** Those control the pure-C oracle binary used for `--regen`. They must stay complete so the oracle can always be rebuilt.

### Module boilerplate

Every ported module starts with:

```rust
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;  // brings in all bindings + helper fns from lib.rs
```

Every exported function:

```rust
#[no_mangle]
pub unsafe extern "C" fn function_name(arg: c_int) -> c_int { ... }
```

### Compile-time feature flags

All `#ifdef` feature flags are **active** in the Rust port. Do not add conditional compilation — just include the code that would be compiled when the flag is on:

`FIX_CORNER_GRAB`, `USE_REPLAY`, `USE_TELEPORTS`, `FIX_SPRITE_XPOS`, `USE_SUPER_HIGH_JUMP`, `USE_JUMP_GRAB`, `FIX_FEATHER_FALL_AFFECTS_GUARDS`, `FIX_GRAB_FALLING_SPEED` — all on.

### C → Rust type mapping

| C type | Rust type | Notes |
|--------|-----------|-------|
| `byte` | `u8` | defined as `Uint8` |
| `sbyte` | `i8` | defined as `Sint8` |
| `word` | `u16` | defined as `Uint16` — **not** `i16` |
| `short` / `c_short` | `i16` | |
| `int` / `c_int` | `i32` | |
| `bool` (C99) | `bool` | rare |

**Always grep bindings.rs for globals before writing code** — surprises like `have_sword: word` (u16 not i16) and `fall_frame: byte` (u8 not i16) only cost time at compile. Quick lookup:

```sh
grep 'pub static mut VARNAME' target/debug/build/sdlpop-*/out/bindings.rs
```

Known non-obvious global types (do not re-derive):

| Global | Type | |
|--------|------|-|
| `curr_room` | `c_short` (i16) | |
| `current_level` | `word` (u16) | compare with `as u16`, not `as i16` |
| `have_sword` | `word` (u16) | `-1` in C → `u16::MAX` in Rust |
| `flash_color` | `word` (u16) | assign color constants `as u16` |
| `flash_time` | `word` (u16) | |
| `fall_frame` | `byte` (u8) | NOT i16 |
| `hitp_delta` | `c_short` (i16) | |
| `hitp_max` | `word` (u16) | |
| `guardhp_curr` | `word` (u16) | |
| `obj_chtab` | `byte` (u8) | |
| `obj_x` | `c_short` (i16) | |
| `curr_tilepos` | `byte` (u8) | |

Functions that take `c_short` where you might expect `c_int` (bindgen reflects the C prototype exactly):

| Function | `c_short` params |
|----------|-----------------|
| `get_image` | first arg (`chtab_id`) |
| `set_wipe` / `set_redraw_full` | first arg (`tilepos`) |
| `start_anim_spike` | both args |
| `calc_screen_x_coord` | arg and return |
| `draw_guard_hp` | both args |
| `seqtbl_offset_char` | arg |

### Bindgen enum naming

bindgen prefixes each enum constant with the enum's type name. The pattern is `{type}_{original_name}`:

| C constant | Rust name |
|-----------|-----------|
| `tiles_0_empty` | `tiles_tiles_0_empty` |
| `tiles_20_wall` | `tiles_tiles_20_wall` |
| `seq_7_fall` | `seqids_seq_7_fall` |
| `actions_4_in_freefall` | `actions_actions_4_in_freefall` |
| `frame_9_run` | `frameids_frame_9_run` |
| `charid_0_kid` | `charids_charid_0_kid` |
| `dir_0_right` | `directions_dir_0_right` |
| `dir_FF_left` | `directions_dir_FF_left` |
| `dir_56_none` | `directions_dir_56_none` |
| `sound_23_footstep` | `soundids_sound_23_footstep` |
| `sword_0_sheathed` | `sword_status_sword_0_sheathed` |
| `sword_2_drawn` | `sword_status_sword_2_drawn` |
| `id_chtab_0_sword` | `chtabs_id_chtab_0_sword` |
| `color_4_red` | `colorids_color_4_red` |
| `color_14_brightyellow` | `colorids_color_14_brightyellow` |
| `FRAME_WEIGHT_X` | `frame_flags_FRAME_WEIGHT_X` |
| `FRAME_THIN` | `frame_flags_FRAME_THIN` |
| `FRAME_NEEDS_FLOOR` | `frame_flags_FRAME_NEEDS_FLOOR` |
| `WITH_CTRL` | `key_modifiers_WITH_CTRL` |

Cast to the target field type at use: `tiles_tiles_20_wall as u8`, `seqids_seq_7_fall as c_short`, `directions_dir_FF_left as i8`, `frameids_frame_9_run as u8`.

### File-scope variables (not in `data.h`)

Some C files define variables at file scope that are **not** in `data.h` — they are private to that translation unit. These do NOT appear in `bindings.rs`. They become `static mut` in the Rust module.

Confirm a variable is file-local by checking it's absent from bindings.rs:
```sh
grep 'pub static mut VARNAME' target/debug/build/sdlpop-*/out/bindings.rs
```

Known file-local variables by file:

**seg007.c** → `curmob_index: u16`, `curr_tile_temp: u16`

**seg008.c** → `drawn_row: c_short`, `draw_bottom_y: c_short`, `draw_main_y: c_short`, `drawn_col: c_short`, `tile_left: u8`, `modifier_left: u8`, `gate_top_y: u16`, `gate_openness: u16`, `gate_bottom_y: u16`

**seg000.c** → `first_start: u16` (= 1), `setjmp_buf: jmp_buf`

In Rust these become `static mut` at module scope:
```rust
static mut drawn_row: c_short = 0;
static mut ptr_add_table: add_table_fn = add_backtable;
```

### Function pointer globals

`seg008.c` has one function pointer global:
```c
// data:27E0
add_table_type ptr_add_table = add_backtable;  // add_table_type = int(*)(short,int,sbyte,sbyte,int,int,byte)
```

bindgen emits `add_table_type` as `Option<unsafe extern "C" fn(...)>`, but for the static we need a plain fn pointer (non-optional). Define a type alias:
```rust
type add_table_fn = unsafe extern "C" fn(c_short, c_int, i8, i8, c_int, c_int, u8) -> c_int;
static mut ptr_add_table: add_table_fn = add_backtable;
```

Calling it: `ptr_add_table(...)` directly (not through `Option::unwrap`).

### SDL functions not in bindings.rs

bindgen only processes `src/common.h`. SDL functions that `seg008.c` and `seg009.c` call directly must be declared in a module-level `extern "C"` block. Confirmed needed for seg008.rs:

```rust
extern "C" {
    fn SDL_ConvertSurface(src: *mut SDL_Surface, fmt: *mut SDL_PixelFormat, flags: u32) -> *mut SDL_Surface;
    fn SDL_SetSurfacePalette(surface: *mut SDL_Surface, palette: *mut SDL_Palette) -> c_int;
    fn SDL_SetSurfaceBlendMode(surface: *mut SDL_Surface, blendMode: c_int) -> c_int;
    fn SDL_SetColorKey(surface: *mut SDL_Surface, flag: c_int, key: u32) -> c_int;
    fn SDL_SetSurfaceAlphaMod(surface: *mut SDL_Surface, alpha: u8) -> c_int;
    fn SDL_UpperBlit(src: *mut SDL_Surface, srcrect: *const SDL_Rect, dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int;
    fn SDL_FreeSurface(surface: *mut SDL_Surface);
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
}
```

For seg009.rs, add POSIX functions too: `opendir`, `readdir`, `closedir`, `scandir`, `alphasort`.

### `setjmp`/`longjmp` (seg000.c)

`seg000.c` uses `setjmp`/`longjmp` for its main restart loop (`start_game` → `process`/`main_loop`). The Rust equivalent declares them in an `extern "C"` block:

```rust
// seg000.c has file-local:  jmp_buf setjmp_buf;
// Prefer: use libc::{setjmp, longjmp, jmp_buf}; if libc is in Cargo.toml (check with: grep '^libc' Cargo.toml)
// Fallback: [u8; 200] covers jmp_buf on x86-64 Linux (verify: grep '_JBLEN' /usr/include/x86_64-linux-gnu/bits/setjmp.h)
static mut setjmp_buf: [u8; 200] = [0u8; 200];

extern "C" {
    fn setjmp(env: *mut u8) -> c_int;
    fn longjmp(env: *mut u8, val: c_int) -> !;
}

// Usage in port:
unsafe fn start_game() {
    if first_start != 0 {
        first_start = 0;
        setjmp(setjmp_buf.as_mut_ptr());  // sets the restart point
    } else {
        longjmp(setjmp_buf.as_mut_ptr(), -1);  // jumps back to restart point
    }
    // ... rest of function
}
```

Alternatively, use `libc::setjmp`/`libc::longjmp` if `libc` is a dependency (check `Cargo.toml`).

### `goto` within a match arm (seg008.c)

`seg008.c:1221` has `goto label_wall_continued` — a forward jump that skips past some wall-drawing logic. Pattern: restructure with a labeled block that `break`s out:

```rust
// C:
// if (condition) { /* wall setup */ }
// label_wall_continued:
// /* common tail */

// Rust:
'wall_block: {
    if condition {
        /* wall setup */
        break 'wall_block;
    }
    /* common tail */
}
```

For the `goto shadow` at line 1584 (backward jump to call `draw_shadow_overlay`): use a flag variable or restructure as a function call.

### Recurring patterns

**Incomplete extern arrays** — bindgen emits `[T; 0]` for `extern const T[]`. Access via raw pointer. `lib.rs` provides `x_bump_at(idx)` and `y_land_at(idx)`; `seg006.rs` provides `dir_front_at`, `dir_behind_at`, `tbl_line_at`, `y_clip_at`. For a new one:

```rust
unsafe fn foo_at(idx: usize) -> i8 {
    *core::ptr::addr_of!(foo).cast::<i8>().add(idx)
}
```

**Packed struct field → pointer** — `custom_options_type` is 1-byte packed. Taking a reference to a field is UB. Use `addr_of!`:

```rust
// WRONG:  (*custom).demo_moves.as_ptr()
// RIGHT:
core::ptr::addr_of!((*custom).demo_moves) as *const auto_move_type
```

**`goto` → `loop/continue`** — C `goto again` at the top of a function body becomes a `loop { ...; continue; }`. See `find_room_of_tile` in seg006.rs.

**`SDL_SwapLE16`** — not needed; use `u16::from_le_bytes([lo, hi])`.

**SDL_SCANCODE values** — `SDL_SCANCODE_L = 15u32` (from SDL2 headers; bindgen does not emit these).

**C `!` vs Rust `!`** — C's `!` is logical NOT (`!0 == 1`, `!nonzero == 0`). Rust's `!` on integers is bitwise NOT (`!0u8 == 255`). When porting `!(expr)` where `expr` is an integer, use `(expr) == 0` in Rust, not `!expr`. Applying `!` to a masked byte (e.g. `!(x & 0x80)`) is the classic trap: both `0x00` and `0x80` produce nonzero results, making the condition always true.

### Porting workflow

1. **Pre-scan types** — grep bindings.rs for every global the C file touches. Map `word` vs `c_short` vs `byte` before writing a line of Rust.
2. **Check function signatures** — grep bindings.rs for every C function *called* by the file; note any `c_short` parameters.
3. **Script large tables** — do not hand-transcribe arrays with >20 entries. Use a short Python script to emit Rust from C source.
4. **Port in batches of ~10 functions**, then `cargo check`. Fix errors before continuing.
5. **After each batch, audit for the two silent traps:**
   - Integer `!`: `grep -n '!\w' file.rs` — every hit must be on a `bool`
   - `u16` bare arithmetic: scan `+`/`-` on `word`/`u16` values — add `wrapping_add`/`wrapping_sub`
6. **Run the harness** (`scripts/run_harness.sh`) before marking the subsystem done.
7. **Remove the C file** from `src/Makefile` and `src/CMakeLists.txt`. Run `cargo test` and the harness again.
8. **Write tests aggressively (TDD).** For any function where you can set up `State` to make the output deterministic, write the test *before* porting the function. This includes non-pure functions — set up the relevant `State` fields, call the function, assert on resulting state. Derive expected values from the C source or by running the C binary with equivalent inputs.

   Each test gets its own `State` on the stack, so `&mut State` tests are naturally isolated from each other. **However**, C globals accessed via FFI are shared across tests and can leak — if your test touches C globals, reset them at the end (or call `set_options_to_default()` as a setup step):

   ```rust
   #[test]
   fn get_tile_returns_wall_for_tile_20() {
       let mut state = State::default();
       state.level.fg[room_row_col(1, 0, 2)] = tiles_tiles_20_wall as u8;
       let result = unsafe { get_tile(&mut state, 1, 0, 2) };
       assert_eq!(result, tiles_tiles_20_wall as u8);
       // No C globals touched — no cleanup needed.
   }

   #[test]
   fn char_takes_damage_reduces_hitp_curr() {
       unsafe { set_options_to_default(); } // reset shared C globals before test
       let mut state = State::default();
       state.kid.hitp_curr = 3;
       unsafe { take_hp(&mut state, 1); }
       assert_eq!(state.kid.hitp_curr, 2);
       unsafe { set_options_to_default(); } // reset after in case of leakage
   }
   ```

   The harness is the primary oracle; unit tests are fast-feedback supplements. Skip functions whose side effects depend entirely on not-yet-ported FFI calls — test those at the subsystem level via the harness.

9. **Bug fixes get a regression test** describing the invariant, not the bug.
10. **Use `rg` not `grep`, `fd` not `find`** in all shell commands.

### Debugging harness divergences

When `scripts/run_harness.sh` reports a divergence at tick N:

**Step 1 — always use the harness, never hand-roll a comparison.**
`run_harness.sh` deletes `tmp/test.trace` before each run and fails if the trace isn't written. A hand-composed `compare_traces.py golden.trace /some/stale.trace` call against an old file is the fastest way to chase a ghost bug. Use the harness.

**Step 2 — dump the divergent tick.**
```sh
python3 scripts/compare_traces.py --dump-tick N traces/golden.trace
```
This prints every field value (with `char_type` structs decoded into subfields) at tick N. The diverging field and its expected value are right there — no binary decoder, no Python one-liners needed.

**Step 3 — generate a mock test seeded from trace state.**
```sh
python3 scripts/compare_traces.py --gen-test N func_name traces/golden.trace
```
This emits a Rust `#[test]` stub with every scalar global and every `char_type` subfield pre-set to the values at tick N-1 (the input state). Paste it into the relevant `seg*.rs` file.

The stub has two `// TODO:` placeholders: one for `level.fg`/`level.bg` tiles (not in the trace — read them from `--dump-tick` of the C trace if needed and set manually), and one for the assertion (also read from `--dump-tick N` to see the expected post-call state).

**Step 4 — reproduce, fix, verify.**
```sh
cargo test -- test_function_name   # must fail first, confirming the bug is reproduced
# fix the bug
cargo test -- test_function_name   # must pass
scripts/run_harness.sh             # harness must be green
```

**Level tiles** are the one thing the trace doesn't capture. If the function reads tiles, use `--dump-tick` on both golden and test traces at the divergent tick to compare `curr_tilepos`, `tile_col`, `tile_row`, then look up the tile values in the level data (`level.fg` / `level.bg`) by hand or by adding a temporary print to the C binary.

---

## Remaining files — per-file porting guide

*Port is complete. This section is retained as historical reference.*

### seg008.c — Room renderer (2068 lines, 27 ifdefs)

**Porting order: port this first** — no setjmp, no audio, no POSIX, pure rendering logic.

**Start of module:**
```rust
use std::os::raw::{c_int, c_short, c_void};
use super::*;

extern "C" {
    fn SDL_ConvertSurface(src: *mut SDL_Surface, fmt: *mut SDL_PixelFormat, flags: u32) -> *mut SDL_Surface;
    fn SDL_SetSurfacePalette(surface: *mut SDL_Surface, palette: *mut SDL_Palette) -> c_int;
    fn SDL_SetSurfaceBlendMode(surface: *mut SDL_Surface, blendMode: c_int) -> c_int;
    fn SDL_SetColorKey(surface: *mut SDL_Surface, flag: c_int, key: u32) -> c_int;
    fn SDL_SetSurfaceAlphaMod(surface: *mut SDL_Surface, alpha: u8) -> c_int;
    fn SDL_UpperBlit(src: *mut SDL_Surface, srcrect: *const SDL_Rect, dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int;
    fn SDL_FreeSurface(surface: *mut SDL_Surface);
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
}

// File-local statics
static mut drawn_row: c_short = 0;
static mut draw_bottom_y: c_short = 0;
static mut draw_main_y: c_short = 0;
static mut drawn_col: c_short = 0;
static mut tile_left: u8 = 0;
static mut modifier_left: u8 = 0;
static mut gate_top_y: u16 = 0;
static mut gate_openness: u16 = 0;
static mut gate_bottom_y: u16 = 0;

type add_table_fn = unsafe extern "C" fn(c_short, c_int, i8, i8, c_int, c_int, u8) -> c_int;
static mut ptr_add_table: add_table_fn = add_backtable;
```

**Key patterns to watch:**
- `tile_table[31]` — 31-entry `piece` struct array. Mechanical, but use a Python script or copy from old branch (`worktree-agent-a99a6259c842dc9b8:rust/src/seg008.rs`) rather than hand-transcribing.
- `col_xh[10]`, `doortop_fram_top[4]`, `door_fram_top[8]`, `blueline_fram1[4]`, `spikes_fram_right[10]` etc — multiple small static tables.
- `goto label_wall_continued` (line ~1221) — forward jump; use labeled block + `break`.
- `goto shadow` (line ~1584) — backward jump; use a bool flag or restructure as function call.
- `ptr_add_table(...)` — call the function pointer directly (no `Option` unwrap).
- `LPOS` and `RPOS` are `static const word` inside a function body — declare as `static` inside the Rust function.
- `add_table_type` in bindings.rs is `Option<fn>`, but the static needs a plain `fn` type alias.

**A working port of seg008 already exists** on branch `worktree-agent-a99a6259c842dc9b8`. Use it as reference: `git show worktree-agent-a99a6259c842dc9b8:rust/src/seg008.rs`

---

### seg000.c — Main loop (2513 lines, 49 ifdefs)

**Port second.** Main challenge is the `setjmp`/`longjmp` restart loop.

**Key file-local statics:**
```rust
static mut first_start: u16 = 1;
static mut setjmp_buf: [u8; 200] = [0u8; 200];  // platform-sized; 200 bytes covers x86-64 Linux
```

**`setjmp`/`longjmp` declarations:**
```rust
extern "C" {
    fn setjmp(env: *mut u8) -> c_int;
    fn longjmp(env: *mut u8, val: c_int) -> !;
}
```

**`process()` macro (line ~258):** The `#define process(x) ok = ok && process_func(&(x), sizeof(x))` macro in `quick_process` expands to calling `process_func` with a pointer and size for each game field. In Rust, expand each call explicitly:
```rust
unsafe fn quick_process(process_func: Option<unsafe extern "C" fn(*mut c_void, usize) -> bool>) -> c_int {
    let mut ok = true;
    macro_rules! process {
        ($x:expr) => { ok = ok && process_func.unwrap()(&mut $x as *mut _ as *mut c_void, std::mem::size_of_val(&$x)); }
    }
    process!(level);
    process!(checkpoint);
    // ... etc
    ok as c_int
}
```

**SDL timer callback** — `SDL_AddTimer(interval, callback, userdata)` takes a callback `fn(Uint32, *mut c_void) -> Uint32`. Declare in the `extern "C"` block.

**`goto error` pattern** (lines ~2153-2191) — appears in quicksave read/write functions. Use `'block: { ... break 'block; ... return Err; }` or a nested function/closure.

**ifdefs to always enable:** `USE_REPLAY`, `USE_QUICKSAVE`, `USE_COPYPROT`, `USE_FADE`, `USE_FLASH`, `USE_MENU`, `USE_LIGHTING`, `USE_SCREENSHOT`, `USE_SUPER_HIGH_JUMP`, `USE_TELEPORTS`.

---

### seg009.c — Platform layer (4248 lines, 66 ifdefs)

**Port last — most complex.** Full SDL2 init/teardown, audio, POSIX filesystem, decompression.

**File-local functions** (static in C — they don't appear in bindings.rs):
```rust
unsafe fn open_dat_from_root_or_data_dir(filename: *const c_char) -> *mut FILE { ... }
unsafe fn load_font_character_offsets(data: *mut rawfont_type) { ... }
```

**Audio callback:**
```rust
// Declared as SDL callback: void audio_callback(void* userdata, Uint8* stream, int len)
pub unsafe extern "C" fn audio_callback(userdata: *mut c_void, stream: *mut u8, len: c_int) { ... }
```

**`hc_font_data`** — large embedded byte array (~150 lines of hex in C). Use a Python script to extract: `python3 -c "import re,sys; ..."`  and emit as a Rust `static` array.

**POSIX dir listing** — `opendir`/`readdir`/`closedir`/`scandir`/`alphasort`:
```rust
extern "C" {
    fn opendir(name: *const c_char) -> *mut libc::DIR;
    fn readdir(dirp: *mut libc::DIR) -> *mut libc::dirent;
    fn closedir(dirp: *mut libc::DIR) -> c_int;
    // or use libc crate if available
}
```

**Decompression routines** — pure pointer arithmetic; port mechanically. No external dependencies.

**ifdefs to always enable:** `USE_MIDI`, `USE_REPLAY`, `USE_QUICKSAVE`, `USE_SCREENSHOT`, `USE_LIGHTING`, `USE_MENU`, `USE_MOD_SUPPORT`, `USE_COPYPROT`, all `SDL_*` platform blocks.

**Harness note:** seg009 provides the main SDL init/teardown. Divergences here will manifest as crashes or missed trace output, not field divergences. After porting, run with a replay and verify the trace is written correctly before comparing fields.
