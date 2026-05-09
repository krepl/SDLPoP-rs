# Plan 6: Port seg006.c to Rust

**Goal:** Replace `src/seg006.c` with a Rust implementation in `rust/src/seg006.rs`.
All 81 public functions are re-exported with `#[no_mangle]` so the rest of the C
codebase calls them transparently. The game must run identically after the swap.

---

## Background

`seg006.c` (2154 lines) is the tile/character system module — the largest file
ported so far. It is already the most-called-into module from the existing Rust
code: seg004.rs and seg005.rs both reach into it via FFI for tile queries, physics
helpers, and sequence playback. Porting it converts those FFI calls to direct Rust
calls.

Responsibilities:

- **Tile lookup:** `get_tile`, `find_room_of_tile`, `get_tilepos`,
  `get_tilepos_nominus` — navigate the room-linked tile map, wrapping across room
  boundaries via `level.roomlinks`.
- **Frame loading:** `load_fram_det_col`, `determine_col`, `load_frame` — look up
  per-frame sprite data from the frame tables using the `get_frame` macro.
- **Sequence playback:** `play_seq` — the animation state machine. Reads bytecodes
  from `seqtbl` via absolute offsets and dispatches SEQ_DX, SEQ_DY, SEQ_FLIP,
  SEQ_JMP, SEQ_ACTION, SEQ_SOUND, SEQ_END_LEVEL, SEQ_GET_ITEM, and others. This is
  the hottest function in this file.
- **Coordinate helpers:** `get_tile_div_mod`, `get_tile_div_mod_m7`, `y_to_row_mod4`,
  `x_to_xh_and_xl`, `back_delta_x`, `distance_to_edge`, `distance_to_edge_weight`,
  `char_dx_forward`, `obj_dx_forward`, `dx_weight`.
- **Character save/restore:** `loadkid`, `savekid`, `loadshad`, `saveshad`, and
  the `_and_opp` variants — copies between `Char`, `Kid`, `Guard`, `Opp`.
- **Physics:** `fall_accel`, `fall_speed`, `check_action`, `check_on_floor`,
  `start_fall`, `fell_out`.
- **Collision / grab:** `check_grab`, `check_grab_run_jump`, `can_grab`,
  `can_grab_front_above`, `in_wall`, `set_char_collision`, `check_spiked`,
  `tile_is_floor`, `wall_type`.
- **Tile-relative queries:** `get_tile_at_char`, `get_tile_infrontof_char`,
  `get_tile_infrontof2_char`, `get_tile_behind_char`, `get_tile_above_char`,
  `get_tile_behind_above_char`, `get_tile_front_above_char`.
- **Kid / guard control dispatch:** `play_kid`, `control_kid`, `play_guard`,
  `do_demo`, `user_control`, `read_user_control`, `flip_control_x`,
  `release_arrows`, `save_ctrl_1`, `rest_ctrl_1`, `clear_saved_ctrl`,
  `control_guard_inactive`.
- **Object/item handling:** `do_pickup`, `check_press`, `check_spike_below`,
  `proc_get_object`, `add_sword_to_objtable`.
- **Character display:** `clip_char`, `stuck_lower`, `set_objtile_at_char`,
  `reset_obj_clip`, `draw_hurt_splash`.
- **Character lifecycle:** `is_dead`, `play_death_music`, `on_guard_killed`,
  `clear_char`, `check_killed_shadow`, `char_opp_dist`.
- **Object save/load:** `save_obj`, `load_obj`.
- **Row increment:** `inc_curr_row`.
- **Health:** `take_hp`.

---

## Feature flags

All flags are `#define`d in `config.h` and are active. Include all guarded blocks
unconditionally (using runtime `(*fixes).fix_*` checks where appropriate):

| Flag | Kind |
|---|---|
| `FIX_CORNER_GRAB` | compile-time opt-in, defined — changes `find_room_of_tile` row-check order |
| `USE_REPLAY` | compile-time opt-in, defined |
| `USE_TELEPORTS` | compile-time opt-in, defined — `play_seq` SEQ_GET_ITEM branch |
| `FIX_SPRITE_XPOS` | compile-time opt-in, defined — simplifies `x_to_xh_and_xl` |
| `FIX_FEATHER_FALL_AFFECTS_GUARDS` | runtime fix, `fixes->fix_feather_fall_affects_guards` |
| `USE_SUPER_HIGH_JUMP` | compile-time opt-in, defined |
| `USE_JUMP_GRAB` | compile-time opt-in, defined — gates `check_grab_run_jump` |
| `FIX_STAND_ON_THIN_AIR` | runtime fix |
| `FIX_DEAD_FLOATING_IN_AIR` | runtime fix |
| `FIX_FALLING_THROUGH_FLOOR_DURING_SWORD_STRIKE` | runtime fix |
| `FIX_HIDDEN_FLOORS_DURING_FLASHING` | runtime fix |
| `FIX_RUNNING_JUMP_THROUGH_TAPESTRY` | runtime fix |
| `FIX_GRAB_FALLING_SPEED` | runtime fix |
| `FIX_CHOMPERS_NOT_STARTING` | compile-time opt-in, defined |
| `FIX_PRESS_THROUGH_CLOSED_GATES` | runtime fix |
| `FIX_INFINITE_DOWN_BUG` | runtime fix |

---

## Exported symbols

All 81 functions are declared in `proto.h` (SEG006.C section) and need
`#[no_mangle] pub unsafe extern "C"`. Note: `sub_70B6` appears in proto.h but
has no definition anywhere in the C source and is never called — omit it.

| Function | Purpose |
|---|---|
| `get_tile` | Set globals and look up tile type for (room, col, row) |
| `find_room_of_tile` | Resolve out-of-bounds tile coords across room links |
| `get_tilepos` | Convert (col, row) to linear tile index, clamping negative |
| `get_tilepos_nominus` | As above but returns -1 for out-of-range instead of clamping |
| `load_fram_det_col` | Load frame and call `determine_col` |
| `determine_col` | Set `obj_tilepos` from current object position |
| `load_frame` | Load sprite frame data via `get_frame` dispatch |
| `dx_weight` | Return `Char.x` delta weighted by direction |
| `char_dx_forward` | Move `Char.x` by delta_x in char's direction; return new x |
| `obj_dx_forward` | Move `obj_x` by delta_x in obj's direction; return new x |
| `play_seq` | Execute seqtbl bytecode for current character until frame byte |
| `get_tile_div_mod_m7` | `get_tile_div_mod(xpos - 7)` |
| `get_tile_div_mod` | Convert xpos to tile column; sets `obj_xl`, returns `obj_xh` |
| `y_to_row_mod4` | Convert ypos to row index mod 4 minus 1 |
| `loadkid` | `Char = Kid` |
| `savekid` | `Kid = Char` |
| `loadshad` | `Char = Guard` |
| `saveshad` | `Guard = Char` |
| `loadkid_and_opp` | `loadkid(); Opp = Guard` |
| `savekid_and_opp` | `savekid(); Guard = Opp` |
| `loadshad_and_opp` | `loadshad(); Opp = Kid` |
| `saveshad_and_opp` | `saveshad(); Kid = Opp` |
| `reset_obj_clip` | Reset clip rect to full screen (0,0,320,192) |
| `x_to_xh_and_xl` | Split xpos into sprite coarse/fine fields |
| `fall_accel` | Update `Char.fall_x` acceleration per frame |
| `fall_speed` | Update `Char.fall_y` velocity |
| `check_action` | Per-frame action state machine entry point |
| `tile_is_floor` | Return 1 if tiletype is a walkable floor |
| `check_spiked` | Check if Char is standing on spikes; apply damage |
| `take_hp` | Deduct HP from current char; return 1 if dead |
| `get_tile_at_char` | `get_tile` at char's current col/row |
| `set_char_collision` | Compute char's collision box globals |
| `check_on_floor` | Verify char is still on a floor; start fall if not |
| `start_fall` | Initiate free-fall state |
| `check_grab` | Check for ledge grab from falling |
| `can_grab_front_above` | Return 1 if a grabbable ledge is directly above and forward |
| `in_wall` | Push char out of wall if overlap detected |
| `get_tile_infrontof_char` | `get_tile` one column ahead of char |
| `get_tile_infrontof2_char` | `get_tile` two columns ahead of char |
| `get_tile_behind_char` | `get_tile` one column behind char |
| `distance_to_edge_weight` | Distance to tile edge, weighted by direction |
| `distance_to_edge` | Pixel distance from xpos to the nearest tile edge |
| `fell_out` | Handle char falling out of the bottom of the room |
| `play_kid` | Per-frame kid update: play seq, collision, control |
| `control_kid` | Dispatch kid input to seg005 handlers |
| `do_demo` | Replay-based demo input |
| `play_guard` | Per-frame guard update |
| `user_control` | Read and apply user gamepad/keyboard input |
| `flip_control_x` | Mirror horizontal control axes |
| `release_arrows` | Release all directional input flags |
| `save_ctrl_1` | Save current control state to `ctrl1_saved` |
| `rest_ctrl_1` | Restore control state from `ctrl1_saved` |
| `clear_saved_ctrl` | Zero saved control state |
| `read_user_control` | Read raw SDL input into control globals |
| `can_grab` | Full grab-eligibility check (tile, position, speed) |
| `wall_type` | Classify tile as wall subtype (0–5) |
| `get_tile_above_char` | `get_tile` one row above char |
| `get_tile_behind_above_char` | `get_tile` behind and above char |
| `get_tile_front_above_char` | `get_tile` in front and above char |
| `back_delta_x` | Negate delta_x if char faces left |
| `do_pickup` | Execute item pickup (potion or sword) |
| `check_press` | Check if char is pressing a pressure plate |
| `check_spike_below` | Start fall if a spike tile is directly below |
| `clip_char` | Clip char sprite to room bounds |
| `stuck_lower` | Push char upward if stuck in floor |
| `set_objtile_at_char` | Write char's tile entry into the object table |
| `proc_get_object` | Process item at char's tile (potion / sword pickup) |
| `is_dead` | Return 1 if current char action is dead |
| `play_death_music` | Trigger death sound/music |
| `on_guard_killed` | Handle guard death: drop sword, update kill count |
| `clear_char` | Zero out `Char` struct |
| `save_obj` | Save current object render state |
| `load_obj` | Restore saved object render state |
| `draw_hurt_splash` | Draw splash effect at hurt location |
| `check_killed_shadow` | Check if shadow (Guard) has been killed |
| `add_sword_to_objtable` | Add guard's sword to the object render table |
| `control_guard_inactive` | AI for inactive/idle guard |
| `char_opp_dist` | Signed x-distance between Char and Opp |
| `inc_curr_row` | `++Char.curr_row` |
| `check_grab_run_jump` | Check grab during run-jump (`USE_JUMP_GRAB`) |

---

## File-private items

### `get_frame_internal` / `get_frame` macro

`get_frame_internal` is not declared in proto.h — it is a file-private helper:

```c
void get_frame_internal(const frame_type frame_table[], int frame,
                        const char* frame_table_name, int count);
#define get_frame(frame_table, frame) \
    get_frame_internal(frame_table, frame, #frame_table, COUNT(frame_table))
```

In Rust, make it `unsafe fn get_frame_internal(...)` (no `#[no_mangle]`). Replace
the `get_frame` macro call sites with direct calls passing the slice and its
`.len()` as count. The `frame_table_name` parameter is only used in a debug assert;
pass a string literal matching the original name.

### `tile_div_tbl` and `tile_mod_tbl`

These 256-element lookup tables are defined in seg006.c and referenced nowhere
outside it. They are not in proto.h or data.h. Translate as file-private Rust
`const` arrays:

```rust
const TILE_DIV_TBL: [i8; 256] = [ /* ... */ ];
const TILE_MOD_TBL: [u8; 256] = [ /* ... */ ];
```

`get_tile_div_mod` currently uses computed arithmetic rather than the tables
(the tables are the original DOS approach, but the SDLPoP implementation replaced
them). Keep the computed path — match the C exactly.

---

## Non-obvious translation patterns

### `SEQTBL_0` indexing in `play_seq()`

The C code defines:
```c
#define SEQTBL_BASE 0x196E
#define SEQTBL_0 (seqtbl - SEQTBL_BASE)
// ...
byte command = *(SEQTBL_0 + Char.curr_seq);  // = seqtbl[curr_seq - SEQTBL_BASE]
```

`Char.curr_seq` holds an **absolute** DOS address offset. In Rust, `seqtbl` is a
zero-based array (`seqtbl.rs`), and `SEQTBL_BASE` is exported as `pub const`.
Every access into `seqtbl` via `SEQTBL_0` must subtract the base:

```rust
use super::seqtbl::{seqtbl, SEQTBL_BASE};
// ...
let idx = (Char.curr_seq - SEQTBL_BASE) as usize;
let command = seqtbl[idx];
```

This is the most error-prone part of the port — an off-by-one or missing base
subtraction will silently play the wrong animation.

### `SDL_SwapLE16` in `play_seq()` SEQ_JMP case

The C code reads a little-endian `u16` from the sequence table:
```c
Char.curr_seq = SDL_SwapLE16(*(const word*)(SEQTBL_0 + Char.curr_seq));
```

This is an unaligned little-endian read. In Rust:
```rust
let idx = (Char.curr_seq - SEQTBL_BASE) as usize;
Char.curr_seq = u16::from_le_bytes([seqtbl[idx], seqtbl[idx + 1]]);
```

On x86 `SDL_SwapLE16` is a no-op, but using `from_le_bytes` is correct on all
targets.

### `x_to_xh_and_xl` — `FIX_SPRITE_XPOS` branch

Since `FIX_SPRITE_XPOS` is defined, include only the fix branch and omit the
complex negative-x arithmetic:

```rust
*xh_addr = (xpos >> 3) as i8;
*xl_addr = (xpos & 7) as i8;
```

The `#ifndef` branch still needs to be present in comments for reference, but
only the fix path compiles.

### `find_room_of_tile` — `FIX_CORNER_GRAB` reorders conditions

The fix moves the `tile_row < 0` check **before** the `tile_col < 0` check.
Since both `#ifdef` and `#ifndef` blocks are present in the C source (and
`FIX_CORNER_GRAB` is defined), include only the fix-path ordering.

---

## Tests

Focus on the pure-computation helpers; the control-dispatch functions depend on
deep game state and are not unit-testable in isolation.

- **`get_tile_div_mod(xpos)`**: verify column and `obj_xl` for a known xpos
  (e.g. `xpos = 58 + 14*3 + 5 = 105` → col 3, xl 5).
- **`y_to_row_mod4(ypos)`**: verify for boundary ypos values.
- **`tile_is_floor(tiletype)`**: spot-check a floor tile and a non-floor tile.
- **`wall_type(tiletype)`**: verify subtypes 0–5 for representative tile values.
- **`distance_to_edge(xpos)`**: pure arithmetic, verify with a hand-calculated case.
- **`inc_curr_row`**: trivial — `Char.curr_row` increments by 1.
