# Plan 5: Port seg005.c to Rust

**Goal:** Replace `src/seg005.c` with a Rust implementation in `rust/src/seg005.rs`.
All 37 public functions are re-exported with `#[no_mangle]` so the rest of the C
codebase calls them transparently. The game must run identically after the swap.

---

## Background

`seg005.c` (1172 lines) is the character movement and control module. Its responsibilities:

- **Sequence dispatch:** `seqtbl_offset_char` / `seqtbl_offset_opp` index into
  `seqtbl_offsets[]` to set the current animation sequence for the kid or opponent.
- **Falling and landing pipeline:** `do_fall` → `land` → `spiked`. Decides whether
  the character lands safely, takes damage, or dies based on fall distance, tile type,
  and fixes flags. Contains the most complex `goto` chains in the file.
- **Input handlers (standing):** `control` dispatches to `control_crouched`,
  `control_standing`, `control_turning`, `control_running`, `control_startrun`,
  `control_jumpup`, `control_hanging` based on current frame and action.
- **Movement helpers:** `up_pressed`, `down_pressed`, `forward_pressed`, `back_pressed`,
  `safe_step`, `crouch`, `standing_jump`, `check_jump_up`, `jump_up`, `run_jump`,
  `grab_up_with_floor_behind`, `grab_up_no_floor_behind`, `jump_up_or_grab`,
  `can_climb_up`, `hang_fall`, `go_up_leveldoor`, `check_get_item`, `get_item`.
- **Sword combat:** `draw_sword`, `control_with_sword`, `swordfight`, `sword_strike`,
  `parry`, `forward_with_sword`, `back_with_sword`.
- **Teleport** (`USE_TELEPORTS`): `teleport` — finds a paired balcony tile and
  repositions the kid.

Zero SDL calls, zero file I/O. All tile/sequence/sound calls reach into other C
modules via FFI bindings.

---

## Feature flags

All flags used in this file are `#define`d in `config.h` — include every guarded
block unconditionally (using runtime `(*fixes).fix_*` checks as needed):

| Flag | Kind |
|---|---|
| `USE_SUPER_HIGH_JUMP` | compile-time opt-in, defined |
| `USE_TELEPORTS` | compile-time opt-in, defined |
| `USE_COPYPROT` | compile-time opt-in, defined |
| `USE_REPLAY` | compile-time opt-in, defined |
| `ALLOW_CROUCH_AFTER_CLIMBING` | compile-time opt-in, defined |
| `FIX_GLIDE_THROUGH_WALL` | runtime fix, `(*fixes).fix_glide_through_wall` |
| `FIX_JUMP_THROUGH_WALL_ABOVE_GATE` | runtime fix |
| `FIX_DROP_THROUGH_TAPESTRY` | runtime fix |
| `FIX_LAND_AGAINST_GATE_OR_TAPESTRY` | runtime fix |
| `FIX_SAFE_LANDING_ON_SPIKES` | runtime fix |
| `FIX_MOVE_AFTER_DRINK` | runtime fix |
| `FIX_MOVE_AFTER_SHEATHE` | runtime fix |
| `FIX_TURN_RUN_NEAR_WALL` | runtime fix |
| `FIX_JUMP_DISTANCE_AT_EDGE` | runtime fix |
| `FIX_EDGE_DISTANCE_CHECK_WHEN_CLIMBING` | runtime fix |
| `FIX_UNINTENDED_SWORD_STRIKE` | runtime fix |
| `FIX_OFFSCREEN_GUARDS_DISAPPEARING` | runtime fix |

---

## Exported symbols

All 37 functions are declared in `proto.h` and need `#[no_mangle] pub unsafe extern "C"`.

| Function | Purpose |
|---|---|
| `seqtbl_offset_char` | Set kid's current sequence from `seqtbl_offsets` |
| `seqtbl_offset_opp` | Set opponent's current sequence |
| `do_fall` | Per-frame fall update: screaming, grab check, land or continue falling |
| `land` | Resolve landing: soft/medium/hard/spike/death, choose animation |
| `spiked` | Impale on spikes: mark spike consumed, lose all HP |
| `control` | Top-level input dispatch based on frame/action/sword state |
| `control_crouched` | Input while crouching |
| `control_standing` | Input while standing (sword draw, jump, step, etc.) |
| `control_turning` | Input during turn animation |
| `control_running` | Input while running |
| `control_startrun` | Input during start-run frames |
| `control_jumpup` | Input during jump-up startup frames |
| `control_hanging` | Input while hanging from a ledge |
| `up_pressed` | Jump up, enter level door, or teleport |
| `down_pressed` | Crouch or climb down |
| `go_up_leveldoor` | Snap x-position and start level-door-exit sequence |
| `crouch` | Set crouch sequence and release controls |
| `back_pressed` | Turn around (or turn and draw sword) |
| `forward_pressed` | Start running or safe-step near a wall |
| `safe_step` | Step to tile edge without falling off |
| `check_get_item` | Check for potion/sword and trigger pickup |
| `get_item` | Execute pickup: potion or sword |
| `standing_jump` | Initiate standing jump sequence |
| `check_jump_up` | Check for grab above and dispatch to grab/jump variants |
| `jump_up_or_grab` | Choose between straight jump and grab based on distance |
| `grab_up_no_floor_behind` | Jump and grab with no floor behind |
| `jump_up` | Jump up (straight or super-high if feather active) |
| `can_climb_up` | Climb up from hang (or abort at closed gate) |
| `hang_fall` | Release ledge: fall or step back |
| `grab_up_with_floor_behind` | Jump and grab with floor behind |
| `run_jump` | Align to edge and launch run-jump |
| `back_with_sword` | Step back during sword combat |
| `forward_with_sword` | Step forward during sword combat |
| `draw_sword` | Initiate draw-sword sequence |
| `control_with_sword` | Sword combat input dispatch |
| `swordfight` | Strike / parry / sheathe decisions |
| `sword_strike` | Execute a sword strike |
| `parry` | Execute a parry |
| `teleport` | Teleport to paired balcony tile (`USE_TELEPORTS`) |

---

## File-private state

Three variables are defined at file scope in seg005.c without `static`, but are not
referenced anywhere outside the file (not in `proto.h` or `data.h`). Treat them as
Rust `static mut`:

```rust
static mut source_modifier: c_int = 0;
static mut source_room:     c_int = 0;
static mut source_tilepos:  c_int = 0;
```

---

## Non-obvious translation patterns

### `x_bump[]` — incomplete array

`x_bump` is `extern const byte x_bump[]` (no size), so bindgen emits `[u8; 0]`.
Direct indexing panics at runtime. Move the `x_bump_at(idx)` helper from
`seg004.rs` to `lib.rs` as `pub(crate) unsafe fn x_bump_at(idx: usize) -> u8`
(bindings are already in scope there, and `use super::*` in each submodule picks
it up). Remove the file-private copy from `seg004.rs` at the same time.

There are three call sites:
- `spiked`: `x_bump[spike_col + FIRST_ONSCREEN_COLUMN]` and `x_bump[tile_col + ...]`
- `go_up_leveldoor`: `x_bump[tile_col + FIRST_ONSCREEN_COLUMN]`
- `teleport`: `x_bump[Char.curr_col + 5]` (twice)

### `seqtbl_offsets[]` — safe to index directly

`seqtbl_offsets` is now defined in Rust (`seqtbl.rs`) as a properly sized
`[u16; SEQTBL_OFFSETS_LEN]`. Index it directly — no raw-pointer workaround needed.

### `goto` chains in `land()`

`land()` has four labels (`loc_5EE6`, `loc_5EFD`, `loc_5F6C`, `loc_5F75`) and
several cross-jumps. Translate using boolean flags:

- `on_spikes: bool` — true when the character landed on a spike tile; replaces both
  `goto loc_5EE6` and the separate `else { goto loc_5EE6 }` branch, merging spike
  detection into a single flag evaluated once before the main if/else.
- `soft_land: bool` — true when fall_y < 22 **or** charid == shadow; replaces
  `goto loc_5EFD` (the shadow fall-2-rows shortcut to the soft-land case).
- `dead: bool` — true when fall_y >= 33, charid == guard at fall_y < 33, or
  take_hp returns true; replaces all `goto loc_5F6C`.
- `skip_take_hp: bool` — true when take_hp(1) returned true (last HP taken in the
  medium-land path); replaces `goto loc_5F75` to skip the redundant `take_hp(100)`.

The logic at the end then reads:
```
if dead {
    if !skip_take_hp { take_hp(100); }
    seq_id = seq_22_crushed;
} else { /* seq_id already set */ }
seqtbl_offset_char(seq_id);
```

### `goto exit` in `teleport()`

`exit` is used as a C label. In Rust use a labeled block:

```rust
let found = 'search: {
    for dest_room in 1..=24 { ... if condition { break 'search true; } }
    false
};
```

### `JUMP_STRAIGHT_CONDITION` macro in `grab_up_with_floor_behind()`

Expand inline — no macro needed:

```rust
let jump_straight = if (*fixes).fix_edge_distance_check_when_climbing != 0 {
    distance < 4 && edge_type != EDGE_TYPE_WALL as c_int
} else {
    distance < 4 && edge_distance < 4 && edge_type != EDGE_TYPE_WALL as c_int
};
```

### `delta_x_reference` macro in `do_fall()`

`#define delta_x_reference 10` — replace with literal `10`.

---

## Tests

The input-handler functions depend on deep game state (frame, action, control
inputs, tile lookup results) and are not easily unit-tested in isolation. Focus
tests on the pure-computation helpers:

- **`seqtbl_offset_char` / `seqtbl_offset_opp`**: set `Char.curr_seq` /
  `Opp.curr_seq` to the correct `seqtbl_offsets` entry for a known `seq_index`.
- **`go_up_leveldoor`**: sets `Char.x` and `Char.direction` — verify against known
  `tile_col` and expected `x_bump` value.
