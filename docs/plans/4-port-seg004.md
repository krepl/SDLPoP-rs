# Plan 4: Port seg004.c to Rust

**Goal:** Replace `src/seg004.c` with a Rust implementation in `rust/src/seg004.rs`.
All 26 public functions are re-exported with `#[no_mangle]` so the rest of the C
codebase calls them transparently. The game must run identically after the swap;
all existing tests must pass, and new tests cover what's testable without standing
up the full game state machine.

---

## Background

`seg004.c` (621 lines) is the collision-detection module. Its responsibilities:

- **Per-frame collision pipeline:** `check_collisions` → `check_bumped` →
  `bumped` → `bumped_floor` / `bumped_fall`. Walks a 10-column "collision row"
  and decides whether the character has hit a wall, gate, or doortop, and
  what animation sequence to play in response.
- **Tile geometry:** `get_left_wall_xpos` / `get_right_wall_xpos` look up the
  pixel x-coordinate of a wall tile's edges; `get_edge_distance` returns the
  distance from the character to the next significant tile feature; `is_obstacle`
  classifies the current tile.
- **Special-case interactions:** `chomped`, `check_chomped_kid`,
  `check_chomped_guard`, `check_gate_push`, `check_guard_bumped`. Each handles
  one specific game-mechanic interaction (chompers, closing gates, sword-pushed
  guards).

The file has **zero SDL calls** and **zero file I/O**. All function calls reach
into other C modules (`seg005.c`, `seg006.c`, etc.) for tile lookup, sequence
loading, and sound effects — those stay in C and are reached via the existing
FFI bindings.

The functions all operate on shared global state (`Char`, the collision row
arrays, `curr_room`, `tile_col`, `tile_row`, `curr_tile2`, `curr_room_modif`,
the `fixes` pointer). Five additional globals plus two `const` lookup arrays
are file-private to `seg004.c` and not referenced elsewhere — they move to
Rust as private `static`s.

---

## Exported symbols

All 26 functions are declared in `proto.h` and called from other C modules
(`seg002.c`, `seg003.c`, `seg005.c`, `seg006.c`, …). All need C-ABI exports.

| Function | Purpose |
|---|---|
| `check_collisions` | Compute collision flags for the current row and detect bumps |
| `move_coll_to_prev` | Shift current-row collision data into the prev-row slot |
| `get_row_collision_data` | Fill collision arrays for one tile row |
| `get_left_wall_xpos` / `get_right_wall_xpos` | Wall edge x-position at a tile |
| `check_bumped` | Dispatch to look-left/look-right based on bump cols |
| `check_bumped_look_left` / `check_bumped_look_right` | Per-direction bump handling |
| `is_obstacle_at_col` / `is_obstacle` | Tile classification |
| `xpos_in_drawn_room` | Coordinate translation across room boundaries |
| `bumped` | Apply a bump: adjust position, choose response (floor vs. fall) |
| `bumped_fall` | Fall after a bump |
| `bumped_floor` | Bump while standing/walking on a floor |
| `bumped_sound` | Play the wall-bump SFX, set guard-notice flag |
| `clear_coll_rooms` | Reset all collision arrays at level start |
| `can_bump_into_gate` | Closed-gate height check |
| `get_edge_distance` | Distance + classification of nearest edge feature |
| `check_chomped_kid` / `chomped` | Kid chomper death |
| `check_gate_push` | Closing gate pushes Kid sideways |
| `check_guard_bumped` | Guard pushed back by sword strike |
| `check_chomped_guard` / `check_chomped_here` | Guard chomper death |
| `dist_from_wall_forward` / `dist_from_wall_behind` | Signed distance to wall in current facing direction |

### File-private state

These move to Rust as private `static mut` items (or `const` for the lookup arrays).
Nothing outside `seg004.c` references them, verified by grep.

| Symbol | Type | Notes |
|---|---|---|
| `bump_col_left_of_wall` | `sbyte` | Set by `check_collisions`, read by `check_bumped` |
| `bump_col_right_of_wall` | `sbyte` | Same pair as above |
| `right_checked_col` | `sbyte` | Set by `check_collisions`, read by `get_row_collision_data` |
| `left_checked_col` | `sbyte` | Same pair |
| `coll_tile_left_xpos` | `short` | Reused by several functions; not module-private but… see pitfall #5 |
| `wall_dist_from_left[6]` | `const sbyte[]` | Indexed by `wall_type` return value |
| `wall_dist_from_right[6]` | `const sbyte[]` | Same |

> **Pitfall on `coll_tile_left_xpos`:** despite no header declaration, this
> global is mutated by `is_obstacle` and read by other helpers across function
> calls. It's effectively shared state between `seg004.c` functions that don't
> always pass through `get_row_collision_data`. Keep it as a Rust `static mut`
> in `seg004.rs` — but verify by grep before assuming it's truly private.

---

## Repository changes

```
SDLPoP/
  src/
    seg004.c           ← excluded from cc build (file stays on disk)
  rust/
    src/
      lib.rs           ← add `pub mod seg004;`
      seg004.rs        ← new: full Rust port
  build.rs             ← remove "src/seg004.c"; add rerun-if-changed
  docs/plans/
    4-port-seg004.md   ← this file
```

No new C wrapper file is needed — there are no SDL types and no fundamentally
unportable constructs.

---

## Step-by-step checklist

### Step 1 — Update `build.rs`

- [ ] Remove `"src/seg004.c"` from the C source list.
- [ ] Add `rerun-if-changed=rust/src/seg004.rs`.

### Step 2 — Module skeleton in `rust/src/seg004.rs`

```rust
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

use std::os::raw::{c_int, c_short};
use super::*;

// File-private state (mirrors seg004.c globals)
static mut bump_col_left_of_wall:  i8 = 0;
static mut bump_col_right_of_wall: i8 = 0;
static mut right_checked_col:      i8 = 0;
static mut left_checked_col:       i8 = 0;
static mut coll_tile_left_xpos:    i16 = 0;

const wall_dist_from_left:  [i8; 6] = [0, 10,  0, -1, 0, 0];
const wall_dist_from_right: [i8; 6] = [0,  0, 10, 13, 0, 0];
```

### Step 3 — Port the simple helpers

Translate these first; they're the leaves of the call graph and have minimal state
interaction.

- `bumped_sound` (2 lines): set `is_guard_notice = 1`, call `play_sound(8)`.
- `xpos_in_drawn_room`: pure arithmetic; reads `curr_room` and `drawn_room`
  globals plus `room_L/R/BL/BR` constants.
- `can_bump_into_gate`: one-liner — `(curr_room_modif[curr_tilepos] >> 2) + 6 < char_height`.
- `clear_coll_rooms`: memset four collision arrays to `-1`; reset
  `prev_collision_row = -1`. Use `<arr>.fill(0xFF)` or a loop.
- `dist_from_wall_forward` / `dist_from_wall_behind`: the geometry helpers;
  port verbatim.

### Step 4 — Port the wall-edge helpers

- `get_left_wall_xpos` / `get_right_wall_xpos`: each calls `get_tile` (FFI),
  then `wall_type` (FFI), then indexes one of the `wall_dist_*` arrays.
- `is_obstacle`: dispatch on `curr_tile2` value with FFI calls back into the
  game (modifies `curr_room_modif`, `jumped_through_mirror`, `coll_tile_left_xpos`).
- `is_obstacle_at_col`: thin wrapper that reads `Char.curr_row`, normalises into
  `[0, 3)`, calls `get_tile` and `is_obstacle`.

### Step 5 — Port the per-row collision pipeline

- `get_row_collision_data`: reads `Char.room`, `x_bump`, `char_x_left_coll`,
  `char_x_right_coll`. The two `* 0x0F`/`* 0xF0` bool-to-mask multiplications
  must be preserved (they're load-bearing — see pitfall #2).
- `move_coll_to_prev`: dispatch on `collision_row` vs `prev_collision_row`
  difference, copy 10 entries between collision arrays. Reset rows to `-1`.
- `check_collisions`: top-level driver. Calls `move_coll_to_prev`,
  `get_row_collision_data` × 3, then walks 10 columns to set
  `bump_col_left_of_wall` / `bump_col_right_of_wall`.

### Step 6 — Port the bump-handling pipeline

- `check_bumped`: dispatches based on `Char.action` and `Char.frame` ranges,
  calls `check_bumped_look_left` / `check_bumped_look_right`. The
  `FIX_TWO_COLL_BUG` block is always-defined in the C build, so just include it
  unconditionally with the `if (*fixes).fix_two_coll_bug != 0` runtime check.
- `check_bumped_look_left` / `check_bumped_look_right`: include the
  `USE_JUMP_GRAB` block unconditionally (it's always-on in the C build);
  the runtime gate is `(*fixes).enable_jump_grab`.
- `bumped`: position adjustment, dispatch to `bumped_floor` or `bumped_fall`.
- `bumped_floor`: chooses one of three sequence indices based on character
  state; calls `seqtbl_offset_char`, `play_seq`, `bumped_sound`.
- `bumped_fall`: simpler; sets fall_x or starts the bumpfall sequence.

### Step 7 — Port the special-case checkers

These are all standalone, called from elsewhere in the codebase, and use the
collision arrays + `curr_*` globals as input.

- `check_chomped_kid` (10-column scan).
- `chomped` (sets blood mod, optionally repositions char, calls `take_hp(100)`,
  starts the chomp sequence). Includes `FIX_SKELETON_CHOMPER_BLOOD` and
  `FIX_OFFSCREEN_GUARDS_DISAPPEARING` runtime gates.
- `check_gate_push` (closing-gate side-push). Includes the
  `FIX_CAPED_PRINCE_SLIDING_THROUGH_GATE` runtime gate.
- `check_guard_bumped` (sword push-back). Includes the
  `FIX_PUSH_GUARD_INTO_WALL` runtime gate.
- `check_chomped_guard` / `check_chomped_here`.

### Step 8 — Port `get_edge_distance` (the goto chain)

This function uses two `goto` labels (`loc_59DD`, `loc_59E8`) that make the
control flow non-obvious. Translate it as nested `if`/`else` blocks. A faithful
direct translation is preferable to "cleaning up" the logic — the original is
the spec.

```rust
pub unsafe extern "C" fn get_edge_distance() -> c_int {
    determine_col();
    load_frame_to_obj();
    set_char_collision();
    let mut tiletype = get_tile_at_char() as u8;
    let mut distance: c_int;

    let wall_branch = wall_type(tiletype) != 0;
    let mut entered_loc_59E8 = !wall_branch;

    if wall_branch {
        tile_col = Char.curr_col as c_short;
        distance = dist_from_wall_forward(tiletype);
        if distance < 0 {
            entered_loc_59E8 = true;
        }
        // else: fall through to the loc_59DD logic below
    }

    if entered_loc_59E8 {
        // … else-branch from the C code …
    } else {
        // loc_59DD:
        if distance <= TILE_RIGHTX as c_int {
            edge_type = EDGE_TYPE_WALL as u8;
        } else {
            edge_type = EDGE_TYPE_FLOOR as u8;
            distance = 11;
        }
    }

    curr_tile2 = tiletype;
    distance
}
```

> Don't try to be clever here. Mirror the C control flow exactly, even at the
> cost of an `else_branch` flag. Refactor only after the port is bit-equivalent
> and tests pass.

### Step 9 — Wire up `lib.rs`

- [ ] Add `pub mod seg004;` to `rust/src/lib.rs`.

### Step 10 — Verify

- [ ] `cargo build` compiles cleanly (no duplicate-symbol link errors — the
      removed `src/seg004.c` from `build.rs` is what prevents this).
- [ ] `cargo run` launches the game.
- [ ] `cargo test` — all 58 existing tests pass; the new tests in Step 11 pass.

### Step 11 — Tests

The collision functions are deeply stateful (10-column collision arrays,
`Char` struct, multiple per-room tile arrays). Full unit tests of the
collision pipeline would require simulating an entire game tick and are out
of scope. Focus tests on:

#### `wall_dist_lookups_match_c_values`
Trivial constant-table sanity. Asserts the two 6-element arrays.

| Index | `wall_dist_from_left` | `wall_dist_from_right` |
|---|---|---|
| 0 | 0 | 0 |
| 1 | 10 | 0 |
| 2 | 0 | 10 |
| 3 | -1 | 13 |
| 4 | 0 | 0 |
| 5 | 0 | 0 |

#### `xpos_in_drawn_room_translates_across_neighbours`

Setup: write `drawn_room`, `curr_room`, `room_L`, `room_R`, `room_BL`, `room_BR`
to known values. Read back `xpos_in_drawn_room(input)` for each branch.

| `curr_room` | `drawn_room` | `input` | Expected output | Rationale |
|---|---|---|---|---|
| 5 | 5 | 100 | 100 | same room → identity |
| 5 | 9 | 100 | -40 | curr is `room_L` (=5) of drawn → subtract `TILE_SIZEX * 10` (140) |
| 5 | 9 | 100 | 240 | curr is `room_R` (=5) of drawn → add 140 |
| (BL case) | … | … | … | same as room_L behaviour |
| (BR case) | … | … | … | same as room_R behaviour |

(Two of these rows actually need different `room_L`/`room_R` values; the test
parameterises by the matching variable.)

#### `can_bump_into_gate_height_check`

Setup: write `curr_tilepos`, `curr_room_modif[curr_tilepos]`, and `char_height`
to known values. The function is one expression: `(modif >> 2) + 6 < char_height`.

| `modif` | `char_height` | Expected | Rationale |
|---|---|---|---|
| 0 | 7 | true | `(0>>2)+6 = 6 < 7` |
| 0 | 6 | false | `6 < 6` false |
| 4 | 8 | true | `(4>>2)+6 = 7 < 8` |
| 252 | 70 | false | `(252>>2)+6 = 69 < 70` (just on the edge) |

#### `clear_coll_rooms_resets_arrays`

Pre-populate the four collision-room arrays with non-`-1` values, set
`prev_collision_row` to a non-negative value. Call `clear_coll_rooms`. Assert:
- All four arrays are now `[-1; 10]`.
- `prev_collision_row == -1`.

#### `bumped_sound_sets_guard_notice_and_calls_play_sound`

Setup: zero `is_guard_notice`. Call `bumped_sound`. Assert
`is_guard_notice == 1`.
(We don't verify `play_sound` was called — that's a C side-effect; if the
linker resolves the symbol and it doesn't crash, that's enough.)

#### `dist_from_wall_forward_geometry`

This depends on `wall_type` (FFI) and `Char.direction`. Set up `Char.direction`,
`tile_col`, `coll_tile_left_xpos`, and `char_x_left_coll`/`char_x_right_coll`.
Pass a tiletype where `wall_type(t)` returns a known value (e.g. `tiles_20_wall`
maps to wall type 1).

| Setup | Expected |
|---|---|
| `direction = dir_FF_left`, `char_x_left_coll = 100`, tiletype that yields wall_type 1 | per the formula: `100 - (coll_tile_left_xpos + 13 - wall_dist_from_right[1])` |
| `direction = dir_0_right`, `char_x_right_coll = 50`, same tiletype | per the formula: `wall_dist_from_left[1] + coll_tile_left_xpos - 50` |
| `tiletype = tiles_4_gate` with `can_bump_into_gate()` returning false | `-1` |

The exact expected values depend on what `wall_type(tiles_20_wall)` actually
returns — derive them by inspection and assert numerically.

> **Out of scope for tests:** `check_collisions`, `check_bumped*`, `bumped*`,
> `chomped`, `check_gate_push`, `check_guard_bumped`, `get_edge_distance`,
> `is_obstacle*`. These require setting up full game state (Char, all four
> collision arrays, room links, sequence table). Coverage comes from the
> integration smoke test (the game still runs and behaves identically).

---

## Key pitfalls

1. **Globals are accessed everywhere.** Every function in this file reads or
   writes some global `static mut` from the C side. Each Rust function body
   needs to be wrapped in `unsafe { … }` (or marked `unsafe extern "C" fn`,
   which makes the body implicitly an unsafe context in 2024 edition; in 2021
   you still need the inner block). Keep the unsafe blocks small for clarity
   in functions that have a non-trivial pure-arithmetic core.

2. **Bool-to-mask multiplication.** `get_row_collision_data` uses
   `(left_wall_xpos < char_x_right_coll) * 0x0F` to derive a flag mask. In
   Rust: `((left_wall_xpos < char_x_right_coll) as u8) * 0x0F`. The `as u8`
   cast is mandatory; without it the comparison is `bool` and won't multiply.

3. **`memset(arr, -1, …)`.** `clear_coll_rooms` memsets `sbyte[10]` arrays to
   `-1`, which means filling with `0xFF` bytes. In Rust:
   `arr.fill(0xFFu8 as i8)` or `arr.fill(-1)`. Both work; pick one and stick
   with it.

4. **The `goto` chain in `get_edge_distance`.** Translate as nested
   `if/else` with a flag for the cross-jump. Don't refactor the logic during
   the port — the C is the spec, including its quirks.

5. **`coll_tile_left_xpos` mutation across function calls.** Several functions
   mutate this global as a side channel: `is_obstacle` writes it on the mirror
   path; `check_chomped_here`, `dist_from_wall_forward`, and
   `get_row_collision_data` all overwrite it. Keep it as a `static mut` (not
   a function parameter) so call ordering is preserved exactly.

6. **`#ifdef FIX_COLL_FLAGS` is OFF by default.** Unlike the other fix flags
   in this file, `FIX_COLL_FLAGS` is commented out in `config.h`. Match the
   default C build: drop those `#ifdef`'d blocks entirely. (If the user wants
   to re-enable, they can convert to a Cargo feature later — out of scope here.)

7. **`enable_jump_grab` and `fix_*` flags are runtime, not compile-time.**
   The `#ifdef USE_JUMP_GRAB` and `#ifdef FIX_*` blocks in the C source are
   compile-time gates around code that *also* checks the runtime fix flag.
   In Rust, drop the `#ifdef` (it's always on in the C build) and keep the
   `if (*fixes).fix_… != 0` runtime check.

8. **`tile_col` shadowing in `is_obstacle_at_col`.** The C function takes a
   parameter named `tile_col` that shadows the global `tile_col`. In Rust
   you can't shadow a `static`; rename the parameter to e.g. `col` and pass
   it explicitly to `get_tile`.

---

## Out of scope

- Refactoring the collision algorithm into something more idiomatic (newtype
  wrappers around tile coords, removing `coll_tile_left_xpos` as a global,
  replacing `get_edge_distance`'s goto with a cleaner state machine, etc.).
  The port is a faithful translation; refactors come later.
- Porting `seg005.c`, `seg006.c`, or any of the helpers this file calls back
  into.
- Re-enabling `FIX_COLL_FLAGS`. Match the default C build.
- Windows or macOS build support.
