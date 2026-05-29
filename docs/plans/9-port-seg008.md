# Plan: Port seg008.c (room renderer) to Rust

## Context

seg001–seg007 are now ported. The three remaining C files are seg008.c (2,068 lines,
room renderer), seg000.c (2,513 lines, main loop), and seg009.c (4,248 lines, SDL
platform layer). This plan covers **seg008 only** — the natural next step because it
is the smallest of the three and is pure rendering logic with no SDL lifecycle
management.

Remaining after this plan: seg000 → seg009.

---

## seg008.c at a glance

- **61 functions** across tile/room drawing, sprite table management, gate animation,
  wall pattern generation, object sorting/drawing, time display, and screen-region
  peeling.
- **8 file-local statics** (not in data.h): `drawn_row`, `draw_bottom_y`, `draw_main_y`,
  `drawn_col` (all `short`/i16) and `tile_left`, `modifier_left` (both `byte`/u8),
  plus `gate_top_y`, `gate_openness`, `gate_bottom_y` (all `word`/u16).
- **1 file-local function pointer**: `ptr_add_table` (initially `add_backtable`).
- **~20 const byte/word arrays** interspersed between functions (frame index tables).
- **2 goto statements** (one in `draw_objtable_item`, one in `load_alter_mod`).
- **SDL calls** not in bindings (need `extern "C"` block): `SDL_ConvertSurface`,
  `SDL_SetSurfacePalette`, `SDL_SetSurfaceBlendMode`, `SDL_SetColorKey`,
  `SDL_SetSurfaceAlphaMod`, `SDL_BlitSurface`, `SDL_FreeSurface`, `SDL_SetWindowTitle`.
- **libc calls** not in bindings: `memset`, `malloc`, `free`.
- `table_counts[5]` aliases: in C, `backtable_count` etc. are macros for
  `table_counts[0..4]` — in Rust use the indexed form directly.

---

## Key patterns and how to handle them

### 1. File-local statics

Declare at the top of `rust/src/seg008.rs`:

```rust
static mut drawn_row: c_short = 0;
static mut draw_bottom_y: c_short = 0;
static mut draw_main_y: c_short = 0;
static mut drawn_col: c_short = 0;
static mut tile_left: u8 = 0;
static mut modifier_left: u8 = 0;
static mut gate_top_y: u16 = 0;
static mut gate_openness: u16 = 0;
static mut gate_bottom_y: u16 = 0;
```

### 2. Function pointer `ptr_add_table`

C type: `typedef int (*add_table_type)(short chtab_id, int id, sbyte xh, sbyte xl, int ybottom, int blit, byte peel);`

```rust
type add_table_type = unsafe extern "C" fn(c_short, c_int, i8, i8, c_int, c_int, u8) -> c_int;
static mut ptr_add_table: add_table_type = add_backtable;
```

Call sites: `ptr_add_table(...)` becomes `ptr_add_table(...)` (function pointer call is
identical syntax in Rust once the type alias is in scope).

Assignment sites (e.g. in `wall_pattern`):
```rust
ptr_add_table = add_backtable;   // or add_foretable
```

### 3. `tile_table` const array

Declare as a `static` of type `[piece; 31]` (repr matches C struct). The 31-entry
initializer can be transcribed directly from seg008.c lines 27–59. `piece` is already
in bindings.rs as a `#[repr(C)]` struct.

### 4. Const frame-index arrays

~20 small arrays (doortop_fram_top, door_fram_top, blueline_fram*, spikes_fram_right,
loose_fram_right, wall_fram_bottom, loose_fram_bottom, loose_fram_left, spikes_fram_left,
potion_fram_bubb, chomper_fram*, spikes_fram_fore, chomper_fram_for, wall_fram_main,
door_fram_slice, floor_left_overlay, col_xh, etc.).

Use a Python one-liner to auto-extract these from seg008.c rather than transcribing by
hand:
```python
import re, sys
for m in re.finditer(r'const (\w+) (\w+)\[\] = \{([^}]+)\}', src):
    ...
```

Declare as `static` in Rust (not `const` — some arrays are indexed with runtime values
and Rust needs them accessible from unsafe).

### 5. `goto shadow` in `draw_objtable_item`

C: case 0/4 conditionally jumps to `shadow:` label inside case 1.

Rust: extract the shadow-rendering body as an inline helper and call it from both paths:

```rust
unsafe fn render_shadow_sprite() {
    if united_with_shadow == 2 {
        play_sound(soundids_sound_41_end_level_music as c_int);
    }
    add_midtable(obj_chtab, obj_id + 1, obj_xh, obj_xl, obj_y,
        blitters_blitters_2_or as c_int, 1);
    add_midtable(obj_chtab, obj_id + 1, obj_xh, obj_xl + 1, obj_y,
        blitters_blitters_3_xor as c_int, 1);
}
```

In the match:
- case `0 | 4`: if shadow condition → `render_shadow_sprite(); return;`, else add_midtable + break
- case `1`: `render_shadow_sprite();`

### 6. `goto label_wall_continued` in `load_alter_mod`

C: `case tiles_20_wall:` does some processing then `goto label_wall_continued;`, which
is also reached by `case tiles_0_empty: | case tiles_1_floor:` (fake-wall path, always
active since `USE_FAKE_TILES` is on).

Rust: extract the `label_wall_continued` block as a private helper
`unsafe fn wall_continued(tilepos: u16, curr_tile_modif: *mut u8)` (or inline closure).

The `WALL_CONNECTION_CONDITION` macro (with `USE_FAKE_TILES` active) expands to a
multi-clause boolean. Inline it as a Rust closure:

```rust
let wall_connection = |adj_tile: u8, adj_tile_modif: u8| -> bool {
    (adj_tile == tiles_tiles_20_wall as u8
        && adj_tile_modif != 4 && (adj_tile_modif >> 4) != 4
        && adj_tile_modif != 6 && (adj_tile_modif >> 4) != 6)
    || (adj_tile == tiles_tiles_0_empty as u8
        && (adj_tile_modif == 5 || adj_tile_modif == 13
            || (adj_tile_modif >= 50 && adj_tile_modif <= 53)))
    || (adj_tile == tiles_tiles_1_floor as u8
        && (adj_tile_modif == 5 || adj_tile_modif == 13
            || (adj_tile_modif >= 50 && adj_tile_modif <= 53)))
};
```

### 7. SDL and libc extern declarations

Add to the top of seg008.rs (same pattern as seg001.rs):

```rust
extern "C" {
    fn SDL_ConvertSurface(src: *mut SDL_Surface, fmt: *mut SDL_PixelFormat,
                          flags: u32) -> *mut SDL_Surface;
    fn SDL_SetSurfacePalette(surface: *mut SDL_Surface,
                             palette: *mut SDL_Palette) -> c_int;
    fn SDL_SetSurfaceBlendMode(surface: *mut SDL_Surface,
                               blendMode: SDL_BlendMode) -> c_int;
    fn SDL_SetColorKey(surface: *mut SDL_Surface, flag: c_int,
                       key: u32) -> c_int;
    fn SDL_SetSurfaceAlphaMod(surface: *mut SDL_Surface, alpha: u8) -> c_int;
    fn SDL_BlitSurface(src: *mut SDL_Surface, srcrect: *const SDL_Rect,
                       dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int;
    fn SDL_FreeSurface(surface: *mut SDL_Surface);
    fn SDL_SetWindowTitle(window: *mut SDL_Window, title: *const c_char);
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
}
```

SDL types (`SDL_Surface`, `SDL_PixelFormat`, `SDL_Palette`, `SDL_BlendMode`, `SDL_Rect`,
`SDL_Window`) are in bindings.rs since SDL headers are on the include path.

### 8. `table_counts` macros

Wherever C uses `backtable_count`, `foretable_count`, etc., use:
- `table_counts[0]` = backtable_count
- `table_counts[1]` = foretable_count
- `table_counts[2]` = wipetable_count
- `table_counts[3]` = midtable_count
- `table_counts[4]` = objtable_count

`table_counts` is `[c_short; 5]` in bindings. The `fill(0)` call: `table_counts.fill(0)`.

### 9. `hflip()` — pixel-by-pixel SDL blit

This function copies one pixel column at a time using SDL_Rect + SDL_BlitSurface.
Port straight — it accesses `input->format` (pointer to `SDL_PixelFormat`), `input->h`
(c_int), `input->w` (c_int). These are fields on `SDL_Surface` which bindgen emits.

---

## Porting order (7 batches of ~9 functions each)

Port in this order to keep dependencies satisfied:

**Batch 1** — Room/tile infrastructure:
`get_room_address`, `load_room_links`, `load_curr_and_left_tile`, `load_leftroom`,
`load_rowbelow`, `get_tile_to_draw`, `draw_tile`, `draw_tile_aboveroom`, `redraw_room`

**Batch 2** — Tile edge drawing (depend on batch 1):
`draw_tile_floorright`, `can_see_bottomleft`, `draw_tile_topright`,
`draw_tile_anim_topright`, `draw_tile_right`, `draw_tile_anim_right`,
`get_spike_frame`, `draw_tile_bottom`, `draw_loose`

**Batch 3** — Tile base/anim/fore layers:
`draw_tile_base`, `draw_tile_anim`, `get_loose_frame`, `draw_tile_fore`,
`draw_tile_wipe`, `calc_gate_pos`, `draw_gate_back`, `draw_gate_fore`

**Batch 4** — Sprite tables + image routing:
`get_image`, `add_backtable`, `add_foretable`, `add_midtable`, `add_peel`,
`add_wipetable`, `draw_table`, `draw_wipes`, `draw_back_fore`

**Batch 5** — Mid/image/wipe drawing + peels:
`hflip`, `draw_mid`, `draw_image`, `draw_wipe`, `restore_peels`, `free_peels`,
`add_drect`, `draw_tables`, `draw_moving`, `redraw_needed_tiles`

**Batch 6** — Object tables + character drawing:
`draw_objtable_items_at_tile`, `sort_curr_objs`, `compare_curr_objs`,
`draw_objtable_item` (goto!), `load_obj_from_objtable`, `draw_people`, `draw_kid`,
`draw_guard`, `add_kid_to_objtable`, `add_guard_to_objtable`, `add_objtable`,
`mark_obj_tile_redraw`, `load_frame_to_obj`

**Batch 7** — Modifier processing + UI + wall pattern (most complex):
`alter_mods_allrm`, `load_alter_mod` (goto!), `redraw_needed`, `redraw_needed_above`,
`draw_leveldoor`, `show_time`, `show_level`, `calc_screen_x_coord`,
`display_text_bottom`, `erase_bottom_text`, `wall_pattern`, `draw_left_mark`,
`draw_right_mark`

---

## Tests to write

After all functions compile, add tests for the pure/near-pure functions:

- `can_see_bottomleft` — tile type → bool mapping (no globals needed, mock `curr_tile`)
- `get_spike_frame` — modifier → frame index
- `get_loose_frame` — modifier → frame index with overflow guard
- `calc_screen_x_coord` — `x * 320 / 280` scaling
- `tile_table` contents — spot-check a few entries match the C initializer

---

## Build system changes (after all functions compile and tests pass)

1. `build.rs` — remove `"src/seg008.c"` from sources array; add
   `println!("cargo:rerun-if-changed=rust/src/seg008.rs");`
2. `src/Makefile` — remove `seg008.o` from `OBJ =` line
3. `src/CMakeLists.txt` — remove `seg008.c` from `SOURCE_FILES` block
4. `rust/src/lib.rs` — add `pub mod seg008;`

---

## Verification

```sh
cargo check          # after each batch
cargo test           # after all functions compile
cargo run            # smoke-test: play into a level, walls/gates/sprites should render
```

The game relies on this file for every frame of in-level rendering, so a successful
`cargo run` that shows correct tile drawing is the definitive test.
