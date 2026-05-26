# Plan: Port seg007.c (Animated Tiles) to Rust

## Context

seg004–seg006 are now ported, forming the collision, movement, and tile-query
backbone of the game. seg007.c is the natural next step: it is the animated-tile
("trob") system that sits directly on top of seg006's tile queries and seg005's
character manipulation. All of its C-side dependencies are already in Rust.

seg007.c: 1256 lines, ~75 exported functions, 6 small static tables, no `goto`
statements. Moderate complexity — the hard parts are bit-packed doorlink fields,
the loose-floor physics state machine, and a multi-room coordinate system for
falling tiles ("mobs"). Everything else is straightforward dispatch.

---

## Pre-porting prep (do before writing any Rust)

**1. Type-scan all globals touched by seg007.c:**
```sh
grep 'pub static mut' target/debug/build/sdlpop-*/out/bindings.rs \
  | grep -E 'trobs|trobs_count|curmob|mobs_count|wipe_heights|redraw_height\
|drawn_room|leveldoor_open|last_loose_sound|doorlink|curr_modifier|curr_tile\b'
```
Key non-obvious types to confirm before writing (from analysis):
- `wipe_heights`: `sbyte` ([i8; 30]) — signed, compared against `redraw_height`
- `trob.type` field: `sbyte` — negative means "marked for deletion"
- `curmob.speed`: `sbyte` — negative values are sentinels (-1 deleted, -2 stopped)
- `curr_modifier`: `byte` (u8) — high bit 0x80 = shaking state flag
- `drawn_room`: `word` (u16)
- `leveldoor_open`: `word` (u16)

**2. Check function signatures for `c_short` params:**
```sh
grep -A3 'pub fn \(set_redraw_anim\|get_curr_tile\|start_anim_\)' \
  target/debug/build/sdlpop-*/out/bindings.rs
```
Already known: `set_wipe`, `set_redraw_full`, `start_anim_spike` take `c_short`.
Confirm the others in this file before writing calls to them.

---

## Static tables to define in Rust

All are small and can be hand-transcribed (no scripting needed):

| Name | Size | Type | Source line |
|------|------|------|-------------|
| `GATE_CLOSE_SPEEDS` | 9 | `u8` | seg007.c:339 |
| `DOOR_DELTA` | 3 | `u8` | seg007.c:341 |
| `LEVELDOOR_CLOSE_SPEEDS` | 5 | `u8` | seg007.c:418 |
| `Y_LOOSE_LAND` | 5 | `u16` | seg007.c:814 |
| `LOOSE_SOUND` | 12 | `u8` | seg007.c:868 |
| `Y_SOMETHING` | 5 | `i16` | seg007.c:1011 |

---

## Porting order (batches of ~10, `cargo check` after each)

**Batch 1 — Trob infrastructure** (~10 functions)
`process_trobs`, `animate_tile`, `is_trob_in_drawn_room`, `find_trob`,
`add_trob`, `get_trob_pos_in_drawn_room`, `get_trob_right_pos_in_drawn_room`,
`get_trob_right_above_pos_in_drawn_room`

**Batch 2 — Redraw/wipe utilities** (~14 functions)
`set_redraw_anim`, `set_redraw2`, `set_redraw_floor_overlay`, `set_redraw_fore`,
`set_redraw_full`, `set_wipe`, `clear_tile_wipes`, `redraw_at_trob`,
`redraw_21h`, `redraw_11h`, `redraw_20h`, `draw_trob`, `redraw_tile_height`,
`set_redraw_anim_curr`, `set_redraw_anim_right`

**Batch 3 — Simple tile animators** (~6 functions)
`animate_torch`, `animate_potion`, `animate_sword`, `animate_empty`,
`animate_button`, `get_torch_frame`, `bubble_next_frame`,
`start_anim_torch`, `start_anim_potion`, `start_anim_sword`

**Batch 4 — Spike + chomper** (~5 functions)
`animate_spike`, `animate_chomper`, `start_anim_spike`, `start_anim_chomper`,
`is_spike_harmful`, `next_chomper_timing`, `start_chompers`

**Batch 5 — Gate/door system** (~8 functions)
`animate_door`, `animate_leveldoor`, `start_level_door`,
`get_doorlink_timer`, `set_doorlink_timer`, `get_doorlink_tile`,
`get_doorlink_next`, `get_doorlink_room`, `gate_stop`,
`trigger_1`, `do_trigger_list`, `trigger_gate`,
`play_door_sound_if_visible`, `trigger_button`

**Batch 6 — Loose floor system** (~8 functions)
`animate_loose`, `loose_shake`, `remove_loose`, `make_loose_fall`,
`loose_make_shake`, `loose_fall`, `loose_land`, `do_knock`

**Batch 7 — Mob (falling tile) system** (~8 functions)
`add_mob`, `do_mobs`, `move_mob`, `move_loose`, `mob_down_a_row`,
`draw_mobs`, `draw_mob`, `add_mob_to_objtable`, `redraw_at_cur_mob`

**Batch 8 — Misc** (~4 functions)
`get_curr_tile`, `died_on_button`, `check_loose_fall_on_kid`,
`fell_on_your_head`

---

## Known tricky points

**Doorlink bit fields** — `get_doorlink_room` combines two byte arrays with bit
shifts. Port exactly, don't simplify. Use `u8` arithmetic with explicit masking.

**Loose floor state encoding** — `curr_modifier & 0x80` = shaking, lower 7 bits
= shake/fall frame. `animate_loose` has a dual-mode state machine; port it
exactly, resist refactoring.

**`wipe_heights` is `[i8; 30]`** — comparisons with `redraw_height` (also `i16`)
need explicit casts. Getting this wrong causes invisible rendering artifacts.

**Mob coordinate system** — `move_loose` uses `y_something[]` (i16 table) to
detect row boundaries and transitions to adjacent rooms. The negative sentinel
value in `y_something[0]` (-1) must be preserved.

**`trob.type` sign** — `process_trobs` marks entries for deletion by setting
`type` negative. Comparisons like `if trob.type < 0` must stay signed.

---

## Build file changes

After all functions compile and tests pass, remove `seg007.c` from:
- `src/Makefile` — `OBJ =` line
- `src/CMakeLists.txt` — `SOURCE_FILES` block

---

## Tests to add

Focus on the pure/near-pure functions:

- `next_chomper_timing` — verify the full 10-step cycle (15→12→9→...→repeat)
- `get_torch_frame` / `bubble_next_frame` — verify frame cycling bounds
- `is_spike_harmful` — verify return values 0/1/2 based on modifier state
- `get_doorlink_timer` / `get_doorlink_tile` / `get_doorlink_next` — bit-field
  extraction correctness (set up a mock doorlink byte, assert extracted fields)
- `get_tilepos` wrappers — `get_trob_pos_in_drawn_room` returns 30 for off-screen

---

## Verification

```sh
cargo check          # after each batch
cargo test           # after all batches compile
cargo test seg007    # run only the new tests
```

Then do a full game build (CMake/Ninja) to confirm the C→Rust boundary is intact.
