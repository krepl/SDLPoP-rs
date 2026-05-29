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
cd src
mkdir build && cd build
cmake -G Ninja ..
ninja
```

**Run:**
```sh
./prince                    # normal start
./prince megahit 3          # start at level 3 with cheats
./prince debug              # enable debug cheats
./prince mod "Mod Name"     # play a mod from mods/
```

There is no automated test suite.

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

## Rust port

The game is being incrementally re-implemented in Rust. The Rust crate lives in `rust/` and is also the root crate (Cargo.toml at the project root links to it). Each ported C file becomes a Rust module exporting `#[no_mangle] pub unsafe extern "C"` functions with identical signatures, so the C linker sees no difference.

### Porting status

| C file | Rust module | Notes |
|--------|-------------|-------|
| `options.c` | `rust/src/options.rs` | INI parser, option loading |
| `seqtbl.c` | `rust/src/seqtbl.rs` | Animation bytecode table |
| `seg001.c` | `rust/src/seg001.rs` | Cutscene playback, animation sequencing |
| `seg002.c` | `rust/src/seg002.rs` | Guard/shadow AI |
| `seg003.c` | `rust/src/seg003.rs` | Level loop, room redraw |
| `seg004.c` | `rust/src/seg004.rs` | Collision detection |
| `seg005.c` | `rust/src/seg005.rs` | Character movement |
| `seg006.c` | `rust/src/seg006.rs` | Tile system, frame data |
| `seg007.c` | `rust/src/seg007.rs` | Animated tiles (trobs, doors, spikes) |
| `seg008.c` | `rust/src/seg008.rs` | Room renderer (tiles, sprites, walls, UI) |

When a file is ported, remove it from `src/Makefile` (the `OBJ =` line) and `src/CMakeLists.txt` (`SOURCE_FILES` block).

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

Follow this order to minimize wasted compile cycles:

1. **Pre-scan types** — before writing any code, grep bindings.rs for every global the C file touches. Build a mental map of `word` vs `c_short` vs `byte`.
2. **Check function signatures** — grep bindings.rs for every C function *called* by the file being ported; note any `c_short` parameters.
3. **Script large tables** — do not hand-transcribe arrays with >20 entries. A short Python script reading the C source and emitting Rust syntax is faster and error-free.
4. **Port in batches of ~10 functions**, then run `cargo check` (faster than `cargo build`). Fix errors before continuing.
5. **Run `cargo test`** when all functions compile.
6. **Remove the C file** from `src/Makefile` and `src/CMakeLists.txt`, then do a final `cargo test` to confirm nothing broke.
7. **Add tests** for pure or near-pure functions (table lookups, math helpers, state machines). See existing test modules in seg004.rs–seg006.rs for style.
8. **Bug fix → regression test** — whenever a runtime bug is fixed, add a test that would have caught it before the fix. The test name should describe the invariant, not the bug. This applies even when the fix is a one-liner.
