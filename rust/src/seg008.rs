//! Turning a room's 10×3 grid of tile bytes into pixels.
//!
//! # The three tables
//!
//! Nothing in this module paints directly. Instead each tile contributes
//! *sprite requests* to one of three display lists, which [`draw_tables`] then
//! plays back in order:
//!
//! * **backtable** — scenery behind the characters (floors, walls, gate slices).
//! * **midtable** — the characters themselves, plus the few scenery pieces that
//!   have to be interleaved with them. Only midtable entries can be flipped,
//!   clipped, or "peeled" (see below).
//! * **foretable** — scenery that must occlude the characters: pillar fronts,
//!   spike tips, chomper blades, lattice.
//!
//! A fourth list, **wipetable**, holds flat coloured rectangles, and a fifth,
//! **objtable**, is a staging area: [`draw_people`] and `draw_mobs` push
//! characters into it, then [`draw_objtable_items_at_tile`] pulls them back out
//! *per tile*, in tile order, so a character is drawn at exactly the right depth
//! in the scenery. [`sort_curr_objs`] settles ties within one tile.
//!
//! `ptr_add_table` is the seam that makes this work. Most draw helpers do not
//! name a table; they call through that function pointer, and the caller decides
//! whether the result lands in the back or the mid table. [`draw_floor_overlay`]
//! and [`draw_other_overlay`] flip it to `add_midtable` to re-draw a piece of
//! scenery *in front of* the Kid, then flip it back.
//!
//! # Drawing one tile
//!
//! [`draw_room`] walks rows bottom-to-top and columns left-to-right, and for each
//! cell calls [`draw_tile`], which is just a fixed sequence of nine helpers. The
//! order is load-bearing: it is the painter's algorithm, and each helper knows
//! which layer of the tile it owns.
//!
//! The awkward part is that the original artwork does not line up with the tile
//! grid. A tile's sprite bleeds into its right-hand neighbour and into the tile
//! below, so drawing cell *(row, col)* actually means drawing:
//!
//! * the *right* part of the tile to the **left** ([`draw_tile_right`],
//!   [`draw_tile_anim_right`]) — hence the `tile_left`/`modifier_left` globals
//!   that sit alongside `curr_tile`/`curr_modifier`;
//! * the *top-right* part of the tile **below-left** ([`draw_tile_topright`],
//!   [`draw_tile_floorright`]), which is why `row_below_left_[]` is preloaded by
//!   [`load_rowbelow`];
//! * and only then the tile's own base, animation and foreground.
//!
//! Because of that overhang, the row above the visible room can poke into view,
//! so [`draw_room`] runs one extra pass with `drawn_room = room_A` and
//! `draw_main_y = -1`. **In that pass `draw_main_y` and the gate y-coordinates go
//! negative**, and since the C originals are `word`, they wrap to values near
//! 65535 and are then compared as `int`. Several functions here reproduce that
//! deliberately; they are commented where it matters.
//!
//! # Static tiles vs. animated tiles
//!
//! A full [`draw_room`] happens on a room change. Every other frame only the
//! dirty tiles are repainted, by [`redraw_needed_tiles`] walking the same grid
//! and consulting the `redraw_frames_*` counters that `seg007` set. The various
//! counters select how much of the tile to repaint: `redraw_frames_full` reruns
//! all of [`draw_tile`], `redraw_frames_anim` only the animated layers, and so on.
//!
//! # Modifier rewriting
//!
//! Level data stores a tile's modifier byte in a compact form that the renderer
//! cannot use directly. [`alter_mods_allrm`] runs once per level and rewrites
//! every modifier in place via [`load_alter_mod`]: potion types are shifted into
//! the high bits, gates get an openness, torches surrender their colour to
//! `torch_colors`, and walls gain two bits recording whether their left and right
//! neighbours are also walls — which is what the wall-pattern generator at the
//! bottom of this file keys off. [`get_tile_to_draw`] then does the *per-frame*
//! rewriting: pressed buttons, and the "fake tile" modifiers that let a floor
//! masquerade as a wall or an empty tile.
//!
//! # Peels
//!
//! A peel is a saved rectangle of screen contents. Before a character sprite is
//! blitted, [`add_peel`] snapshots the pixels underneath it; the next frame
//! [`restore_peels`] puts them back, erasing the character without redrawing the
//! scenery behind it.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short, c_char, c_void};
use super::*;
use crate::platform::Renderer;
use crate::state::State;

extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
}

// File-local statics (seg008.c data section) — the "where am I drawing" cursor
// that every helper in this module reads instead of taking parameters.

/// Row of the tile being drawn, 2 (bottom) down to 0 (top).
static mut drawn_row: c_short = 0;

/// Screen y of the floor line of `drawn_row`. `-1`/`2` in the above-room pass.
static mut draw_bottom_y: c_short = 0;

/// Screen y of the main body of `drawn_row`, always `draw_bottom_y - 3`.
static mut draw_main_y: c_short = 0;

/// Column of the tile being drawn, 0..=9.
static mut drawn_col: c_short = 0;

/// Tile type at `drawn_col - 1`, whose artwork overhangs into `drawn_col`.
/// Column 0 takes it from `leftroom_`, i.e. the room to the left.
static mut tile_left: u8 = 0;

/// Modifier byte belonging to [`tile_left`].
static mut modifier_left: u8 = 0;

/// Top of the gate shaft. `word` in C, so negative values (above-room pass)
/// appear here as ~65500 and *must* be compared after widening to `c_int`.
static mut gate_top_y: u16 = 0;

/// How far the gate to the left has slid up, in pixels.
static mut gate_openness: u16 = 0;

/// Screen y of the bottom edge of the gate. Same `word` caveat as
/// [`gate_top_y`].
static mut gate_bottom_y: u16 = 0;

type add_table_fn = unsafe extern "C" fn(c_short, c_int, i8, i8, c_int, c_int, u8) -> c_int;

/// Which display list the generic draw helpers append to.
///
/// Normally [`add_backtable`]. The overlay functions and [`wall_pattern`]
/// temporarily point it at [`add_midtable`] / [`add_foretable`] so the very same
/// tile-drawing code emits into a different layer, then restore it.
static mut ptr_add_table: add_table_fn = add_backtable;

/// Which sprite makes up each part of each of the 31 tile types.
///
/// The `*_id` fields are 1-based image numbers in chtab 6 (environment), with 0
/// meaning "this tile has no such part"; the `*_y` fields are pixel offsets from
/// [`draw_main_y`]. Split this way because a tile is painted in up to five
/// separate passes at different depths — see the module docs.
///
/// Verbatim from `seg008.c:27`; the `tile_table_spot_check` test guards it.
// data:259C
static tile_table: [piece; 31] = [
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  0, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x00 empty
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x01 floor
    piece { base_id: 127, floor_left: 1, base_y:   0, right_id: 133, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x02 spike
    piece { base_id:  92, floor_left: 1, base_y:   0, right_id:  93, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:  94, bottom_id: 43, fore_id:  95, fore_x: 1, fore_y:   0 }, // 0x03 pillar
    piece { base_id:  46, floor_left: 1, base_y:   0, right_id:  47, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:  48, bottom_id: 43, fore_id:  49, fore_x: 3, fore_y:   0 }, // 0x04 door
    piece { base_id:  41, floor_left: 1, base_y:   1, right_id:  35, floor_right: 1, right_y:  3, stripe_id: 145, topright_id:   0, bottom_id: 36, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x05 stuck floor
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 96, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x06 close button
    piece { base_id:  46, floor_left: 1, base_y:   0, right_id:   0, floor_right: 0, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:  49, fore_x: 3, fore_y:   0 }, // 0x07 door top with floor
    piece { base_id:  86, floor_left: 1, base_y:   0, right_id:  87, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:  88, fore_x: 1, fore_y:   0 }, // 0x08 big pillar bottom
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:  89, floor_right: 0, right_y:  3, stripe_id:   0, topright_id:  90, bottom_id:  0, fore_id:  91, fore_x: 1, fore_y:   3 }, // 0x09 big pillar top
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:  12, fore_x: 2, fore_y:  -3 }, // 0x0A potion
    piece { base_id:   0, floor_left: 1, base_y:   0, right_id:   0, floor_right: 0, right_y:  0, stripe_id: 145, topright_id:   0, bottom_id:  0, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x0B loose floor
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:   0, floor_right: 0, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 85, fore_id:  49, fore_x: 3, fore_y:   0 }, // 0x0C door top
    piece { base_id:  75, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:  77, fore_x: 0, fore_y:   0 }, // 0x0D mirror
    piece { base_id:  97, floor_left: 1, base_y:   0, right_id:  98, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id: 100, fore_x: 0, fore_y:   0 }, // 0x0E debris
    piece { base_id: 147, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  1, stripe_id: 145, topright_id:   0, bottom_id:149, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x0F open button
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  37, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:  38, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x10 leveldoor left
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:  39, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:  40, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x11 leveldoor right
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x12 chomper
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x13 torch
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:   1, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   2, bottom_id:  0, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x14 wall
    piece { base_id:  30, floor_left: 1, base_y:   0, right_id:  31, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x15 skeleton
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x16 sword
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  10, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:  11, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x17 balcony left
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:  12, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:  13, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x18 balcony right
    piece { base_id:  92, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:  95, fore_x: 1, fore_y:   0 }, // 0x19 lattice pillar
    piece { base_id:   1, floor_left: 0, base_y:   0, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  2, fore_id:   9, fore_x: 0, fore_y: -53 }, // 0x1A lattice down
    piece { base_id:   3, floor_left: 0, base_y: -10, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  0, fore_id:   9, fore_x: 0, fore_y: -53 }, // 0x1B lattice small
    piece { base_id:   4, floor_left: 0, base_y: -10, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  0, fore_id:   9, fore_x: 0, fore_y: -53 }, // 0x1C lattice left
    piece { base_id:   5, floor_left: 0, base_y: -10, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  0, fore_id:   9, fore_x: 0, fore_y: -53 }, // 0x1D lattice right
    piece { base_id:  97, floor_left: 1, base_y:   0, right_id:  98, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id: 100, fore_x: 0, fore_y:   0 }, // 0x1E debris with torch
];

/// Screen x of each column, in 8-pixel units (`draw_xh`). Four per tile.
// data:24C6
static col_xh: [u16; 10] = [0, 4, 8, 12, 16, 20, 24, 28, 32, 36];

/// Palace-only lintel above a door, seen from the tile above it.
static doortop_fram_top: [u8; 4] = [0, 81, 83, 0];

/// Eight sub-pixel phases of the gate lattice, cycled by openness.
static door_fram_top: [u8; 8] = [60, 61, 62, 63, 64, 65, 66, 67];

/// The blue floor stripe drawn where an empty tile meets the tile to its left,
/// indexed by that tile's modifier (`0` = none, so nothing is drawn).
static blueline_fram1: [u8; 4] = [0, 124, 125, 126];

/// Vertical offset of each [`blueline_fram1`] variant.
static blueline_fram_y: [i8; 4] = [0, -20, -20, 0];

/// The blue floor stripe variant used over a plain floor tile.
static blueline_fram3: [u8; 4] = [44, 44, 45, 45];

/// Palace-only door threshold, drawn from the tile to the door's right.
static doortop_fram_bot: [u8; 4] = [78, 80, 82, 0];

// The spike animation is one image split across three depths: the right-hand
// overhang goes in the back table, the left-hand body in the base pass, and the
// tips in the fore table so they cover the Kid. All three are indexed by
// get_spike_frame, and index 0/9 are blank (retracted).
static spikes_fram_right: [u8; 10] = [0, 134, 135, 136, 137, 138, 137, 135, 134, 0];
static spikes_fram_left: [u8; 10] = [0, 128, 129, 130, 131, 132, 131, 129, 128, 0];
static spikes_fram_fore: [u8; 10] = [0, 139, 140, 141, 142, 143, 142, 140, 139, 0];

// Likewise, a wobbling loose floor is three images indexed by get_loose_frame.
static loose_fram_right: [u8; 12] = [42, 71, 42, 72, 72, 42, 42, 42, 72, 72, 72, 0];
static loose_fram_bottom: [u8; 12] = [43, 73, 43, 74, 74, 43, 43, 43, 74, 74, 74, 0];
static loose_fram_left: [u8; 12] = [41, 69, 41, 70, 70, 41, 41, 41, 70, 70, 70, 0];

/// Wall sprites, indexed by the two neighbour bits [`load_alter_mod`] packed
/// into the modifier: 0 = neither side is wall, 1 = right, 2 = left, 3 = both.
static wall_fram_bottom: [u8; 4] = [7, 9, 5, 3];
/// See [`wall_fram_bottom`]; this is the wall's main face rather than its base.
static wall_fram_main: [u8; 4] = [8, 10, 6, 4];

/// Bubble animation inside a potion, indexed by the modifier's low 3 bits.
static potion_fram_bubb: [u8; 8] = [0, 16, 17, 18, 19, 20, 21, 22];

// A chomper's seven modifier states collapse onto five distinct poses via
// chomper_fram1; the pose then indexes the other three tables.
static chomper_fram1: [u8; 8] = [3, 2, 0, 1, 4, 3, 3, 0];
static chomper_fram_bot: [u8; 6] = [101, 102, 103, 104, 105, 0];
static chomper_fram_top: [u8; 6] = [0, 0, 111, 112, 113, 0];
static chomper_fram_y: [u8; 5] = [0, 0, 0x25, 0x2F, 0x32];
static chomper_fram_for: [u8; 6] = [106, 107, 108, 109, 110, 0];

/// Partial gate slices, indexed by how many of the 8 pixel rows of the topmost
/// slice are still below the gate's top. Index 0 is unreachable: callers only
/// use this for a computed frame in 1..=8.
static door_fram_slice: [u8; 9] = [67, 59, 58, 57, 56, 55, 54, 53, 52];

/// Sliver of floor drawn over the Kid's hand while he climbs, indexed by
/// `Kid.frame - frame_137_climbing_3`.
// data:286A
static floor_left_overlay: [u16; 8] = [32, 151, 151, 150, 150, 151, 32, 32];

/// First tilepos of a row: `tbl_line[row]`.
///
/// `tbl_line` is an `extern const word[]`, which bindgen emits as `[u16; 0]`,
/// so it has to be read through a raw pointer.
unsafe fn tbl_line_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(tbl_line).cast::<u16>().add(idx)
}

/// Room holding the copy-protection potion for placement `idx`. Incomplete
/// extern array, same raw-pointer treatment as [`tbl_line_at`].
unsafe fn copyprot_room_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_room).cast::<u16>().add(idx)
}

/// Tilepos of the copy-protection potion for placement `idx`.
unsafe fn copyprot_tile_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_tile).cast::<u16>().add(idx)
}

/// Draw the shadow: the same sprite twice, one pixel apart, OR'd then XOR'd, to
/// get the flickering translucent look.
///
/// This is the body of the `shadow:` label in `seg008.c:1592`. The C code
/// reaches it either by falling into `case 1` or by a backward `goto` from the
/// Kid case when the Kid is blinking after uniting with the shadow; both call
/// sites here call this function instead.
unsafe fn render_shadow_sprite() {
    if united_with_shadow == 2 {
        play_sound(soundids_sound_41_end_level_music as c_int);
    }
    add_midtable(
        obj_chtab as c_short, obj_id as c_int + 1,
        obj_xh as i8, obj_xl as i8, obj_y as c_int,
        blitters_blitters_2_or as c_int, 1,
    );
    add_midtable(
        obj_chtab as c_short, obj_id as c_int + 1,
        obj_xh as i8, obj_xl as i8 + 1, obj_y as c_int,
        blitters_blitters_3_xor as c_int, 1,
    );
}

/// Record in a wall tile's modifier whether its left and right neighbours are
/// also walls, so [`wall_fram_bottom`] and [`wall_fram_main`] can pick a sprite
/// whose edges line up with them.
///
/// This is the body of `label_wall_continued` in `seg008.c:1231`, which the wall
/// case reaches by an explicit `goto` and the fake-wall floor/empty case reaches
/// by falling through. Both call sites here call this function instead.
///
/// `tiletype` selects the encoding: a real wall gets the two neighbour bits
/// OR'd into its existing modifier (which already carries the "no blue" flag and
/// the fake-tile nibble), whereas a floor or empty tile *impersonating* a wall
/// has no bits to spare and instead gets the flat code 51/52/53, which
/// [`get_tile_to_draw`] decodes back into a wall each frame.
///
/// CGA and Hercules have no artwork for the four variants, so they always get
/// modifier 3.
unsafe fn wall_connection_block(tilepos: usize, curr_tile_modif: *mut u8, tiletype: u8) {
    // C's WALL_CONNECTION_CONDITION macro. A neighbour counts as a wall if it
    // is a real wall that is not currently impersonating a floor or an empty
    // tile, or if it is a floor/empty tile impersonating a wall.
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

    if graphics_mode == grmodes_gmCga as u8 || graphics_mode == grmodes_gmHgaHerc as u8 {
        *curr_tile_modif = 3;
        return;
    }

    // A neighbour inside this room is read through curr_room_*; one in the room
    // to the side has to be read out of the level directly. At the edge of the
    // level (no adjacent room) the C code leaves the flag at its initial 1, so
    // the wall is drawn as if it continued off-screen.
    let neighbour_in_room = |adj_tile_index: usize| {
        wall_connection(
            *curr_room_tiles.add(adj_tile_index) & 0x1F,
            *curr_room_modif.add(adj_tile_index),
        )
    };
    let neighbour_in_level = |adj_tile_index: usize| {
        wall_connection(level.fg[adj_tile_index] & 0x1F, level.bg[adj_tile_index])
    };

    let wall_to_left = if tilepos % 10 != 0 {
        neighbour_in_room(tilepos - 1)
    } else {
        room_L == 0 || neighbour_in_level(30 * (room_L as usize - 1) + tilepos + 9)
    };
    let wall_to_right = if tilepos % 10 != 9 {
        neighbour_in_room(tilepos + 1)
    } else {
        room_R == 0 || neighbour_in_level(30 * (room_R as usize - 1) + tilepos - 9)
    };

    // USE_FAKE_TILES is always on: a floor or empty tile impersonating a wall
    // stores the connection as a flat code instead of as bits.
    if tiletype == tiles_tiles_1_floor as u8 || tiletype == tiles_tiles_0_empty as u8 {
        if wall_to_left && wall_to_right {
            *curr_tile_modif = 53;
        } else if wall_to_left {
            *curr_tile_modif = 52;
        } else if wall_to_right {
            *curr_tile_modif = 51;
        }
        return;
    }

    if wall_to_left && wall_to_right {
        *curr_tile_modif |= 3;
    } else if wall_to_left {
        *curr_tile_modif |= 2;
    } else if wall_to_right {
        *curr_tile_modif |= 1;
    }
}

/// Repaint the whole room from scratch: throw away every display list and
/// rebuild it. Called on a room change, not per frame.
// seg008:0006
#[no_mangle]
pub unsafe extern "C" fn redraw_room() {
    free_peels();
    memset(table_counts.as_mut_ptr() as *mut c_void, 0, core::mem::size_of_val(&table_counts));
    reset_obj_clip();
    draw_room();
    clear_tile_wipes();
}

/// Cache `drawn_room`'s eight neighbours in the `room_*` globals.
///
/// The four diagonals are not stored in the level, so they are derived: prefer
/// going up-then-sideways, and only if there is no room above, sideways-then-up.
/// If both routes are missing the diagonal stays 0.
// seg008:0035
unsafe fn load_room_links_impl(state: &mut State) {
    *state.room_BR() = 0;
    *state.room_BL() = 0;
    *state.room_AR() = 0;
    *state.room_AL() = 0;
    let drawn = *state.drawn_room();
    if drawn == 0 {
        *state.room_B() = 0;
        *state.room_A() = 0;
        *state.room_R() = 0;
        *state.room_L() = 0;
        return;
    }
    get_room_address(drawn as c_int);
    let (left, right, above, below) = links_of(state, drawn);

    let (above_left, above_right) = if above != 0 {
        let (l, r, _, _) = links_of(state, above);
        (l, r)
    } else {
        (
            if left != 0 { links_of(state, left).2 } else { 0 },
            if right != 0 { links_of(state, right).2 } else { 0 },
        )
    };
    let (below_left, below_right) = if below != 0 {
        let (l, r, _, _) = links_of(state, below);
        (l, r)
    } else {
        (
            if left != 0 { links_of(state, left).3 } else { 0 },
            if right != 0 { links_of(state, right).3 } else { 0 },
        )
    };

    *state.room_L() = left;
    *state.room_R() = right;
    *state.room_A() = above;
    *state.room_B() = below;
    *state.room_AL() = above_left;
    *state.room_AR() = above_right;
    *state.room_BL() = below_left;
    *state.room_BR() = below_right;
}

/// The `(left, right, up, down)` neighbours of `room`, 1-based as stored.
unsafe fn links_of(state: &mut State, room: u16) -> (u16, u16, u16, u16) {
    let link = state.level().roomlinks[room as usize - 1];
    (link.left as u16, link.right as u16, link.up as u16, link.down as u16)
}

#[no_mangle]
pub unsafe extern "C" fn load_room_links() {
    load_room_links_impl(&mut State);
}

/// Paint every tile of `drawn_room`, plus the strip of the room above that
/// overhangs into view.
///
/// Rows go bottom-to-top and columns left-to-right so that later tiles paint
/// over the parts of earlier ones they are supposed to hide. The extra pass at
/// the end re-points `drawn_room` at `room_A` and uses the negative
/// `draw_main_y = -1` so only the bottom sliver of those tiles lands on screen.
// seg008:0125
#[no_mangle]
pub unsafe extern "C" fn draw_room() {
    load_leftroom();
    for dr in (0i16..3).rev() {
        drawn_row = dr;
        load_rowbelow();
        draw_bottom_y = 63 * drawn_row + 65;
        draw_main_y = draw_bottom_y - 3;
        for dc in 0i16..10 {
            drawn_col = dc;
            load_curr_and_left_tile();
            draw_tile();
        }
    }
    let saved_room = drawn_room;
    drawn_room = room_A;
    load_room_links();
    load_leftroom();
    drawn_row = 2;
    load_rowbelow();
    for dc in 0i16..10 {
        drawn_col = dc;
        load_curr_and_left_tile();
        draw_main_y = -1;
        draw_bottom_y = 2;
        draw_tile_aboveroom();
    }
    drawn_room = saved_room;
    load_room_links();
}

/// Paint one tile, in painter's order: everything that belongs behind the
/// characters first, then the base and animation, then the foreground.
///
/// Several of these actually draw parts of *neighbouring* tiles whose artwork
/// overhangs into this one — see the module docs.
// seg008:01C7
#[no_mangle]
pub unsafe extern "C" fn draw_tile() {
    draw_tile_floorright();
    draw_tile_anim_topright();
    draw_tile_right();
    draw_tile_anim_right();
    draw_tile_bottom(0);
    draw_loose(0);
    draw_tile_base();
    draw_tile_anim();
    draw_tile_fore();
}

/// [`draw_tile`] for the overhanging strip of the room above: no base and no
/// animation, since only the bottom few pixels of those tiles are on screen.
// seg008:01F2
#[no_mangle]
pub unsafe extern "C" fn draw_tile_aboveroom() {
    draw_tile_floorright();
    draw_tile_anim_topright();
    draw_tile_right();
    draw_tile_bottom(0);
    draw_loose(0);
    draw_tile_fore();
}

/// Read one tile out of the level and translate it into what should actually be
/// drawn there this frame.
///
/// Three kinds of translation happen here:
///
/// * **Pressed buttons.** A plate whose doorlink timer is still counting looks
///   depressed: a closer becomes a stuck floor, an opener a plain floor.
/// * **Fake tiles.** Certain modifier values make a floor or an empty tile
///   impersonate the other, or impersonate a wall, and make a wall impersonate a
///   floor or an empty tile. This is how level designers hide passages.
/// * **A loose floor with an all-zero modifier** is drawn as an ordinary floor
///   rather than as wobble frame 0, so it does not give itself away next to a
///   potion.
///
/// `column == -1` means "the rightmost column of the room to the left", which is
/// preloaded into `leftroom_`; `room == 0` means there is no room there at all,
/// in which case `tile_room0` is the level-edge filler the caller wants.
///
/// Returns the resulting tile type, which is also written to `ptr_tiletype`.
// seg008:02FE
#[no_mangle]
pub unsafe extern "C" fn get_tile_to_draw(
    room: c_int, column: c_int, row: c_int,
    ptr_tiletype: *mut u8, ptr_modifier: *mut u8,
    tile_room0: u8,
) -> c_int {
    let tilepos = (tbl_line_at(row as usize) as usize).wrapping_add(column as usize);
    if column == -1 {
        *ptr_tiletype = leftroom_[row as usize].tiletype;
        *ptr_modifier = leftroom_[row as usize].modifier;
    } else if room != 0 {
        *ptr_tiletype = *curr_room_tiles.add(tilepos) & 0x1F;
        *ptr_modifier = *curr_room_modif.add(tilepos);
    } else {
        *ptr_modifier = 0;
        *ptr_tiletype = tile_room0;
    }

    let tiletype = (*ptr_tiletype) & 0x1F;
    let modifier = *ptr_modifier;

    // USE_FAKE_TILES (always on). Modifier values that turn a floor or an empty
    // tile into a wall, and the modifier the wall should then be drawn with.
    // 5/13 are the hand-authored form (13 additionally sets the "no blue" bit);
    // 50..=53 are written by load_alter_mod and carry the wall-connection
    // pattern in their low two bits.
    let as_fake_wall = |modifier: u8| -> Option<u8> {
        match modifier {
            5 => Some(0),
            13 => Some(0x80),
            50..=53 => Some(modifier - 50),
            _ => None,
        }
    };
    let draw_as = |tiletype: tiles, modifier: u8| {
        *ptr_tiletype = tiletype as u8;
        *ptr_modifier = modifier;
    };

    match tiletype as tiles {
        tiles_tiles_6_closer => {
            if get_doorlink_timer(modifier as c_short) > 1 {
                *ptr_tiletype = tiles_tiles_5_stuck as u8;
            }
        }
        tiles_tiles_15_opener => {
            if get_doorlink_timer(modifier as c_short) > 1 {
                draw_as(tiles_tiles_1_floor, 0);
            }
        }
        tiles_tiles_0_empty => match modifier {
            // 12 is 4 plus the "no blue" option, which the floor stores as 1.
            4 | 12 => draw_as(tiles_tiles_1_floor, (modifier == 12) as u8),
            _ => if let Some(m) = as_fake_wall(modifier) { draw_as(tiles_tiles_20_wall, m); },
        },
        tiles_tiles_1_floor => match modifier {
            // An invisible floor: still solid, but drawn as nothing.
            6 | 14 => draw_as(tiles_tiles_0_empty, (modifier == 14) as u8),
            _ => if let Some(m) = as_fake_wall(modifier) { draw_as(tiles_tiles_20_wall, m); },
        },
        tiles_tiles_20_wall => {
            // load_alter_mod moved the "no blue" flag to bit 7 and parked a
            // fake-tile code in bits 4..6, so modifiers 9..15 loop back onto
            // 2..7 but with "no blue" set.
            match (modifier >> 4) & 7 {
                4 => draw_as(tiles_tiles_1_floor, modifier >> 7),
                6 => draw_as(tiles_tiles_0_empty, ((modifier >> 7) != 0) as u8),
                _ => {}
            }
        }
        tiles_tiles_11_loose => {
            // FIX_LOOSE_LEFT_OF_POTION (always on)
            if (*fixes).fix_loose_left_of_potion != 0 && (*ptr_modifier & 0x7F) == 0 {
                *ptr_tiletype = tiles_tiles_1_floor as u8;
            }
        }
        _ => {}
    }

    *ptr_tiletype as c_int
}

/// Load the tile at `(drawn_row, drawn_col)` and the one to its left into
/// `curr_tile`/`curr_modifier` and [`tile_left`]/[`modifier_left`], and set
/// `draw_xh` to the column's screen x.
///
/// The level-edge filler differs by row because the top row of a room is where
/// you would fall in from above: rows 0 and 1 get a wall, row 2 gets whatever
/// `drawn_tile_top_level_edge` says (a floor by default).
// seg008:03BB
#[no_mangle]
pub unsafe extern "C" fn load_curr_and_left_tile() {
    let tiletype = if drawn_row == 2 {
        (*custom).drawn_tile_top_level_edge
    } else {
        tiles_tiles_20_wall as u8
    };
    get_tile_to_draw(drawn_room as c_int, drawn_col as c_int, drawn_row as c_int, &mut curr_tile, &mut curr_modifier, tiletype);
    get_tile_to_draw(drawn_room as c_int, drawn_col as c_int - 1, drawn_row as c_int, &mut tile_left, &mut modifier_left, tiletype);
    draw_xh = col_xh[drawn_col as usize];
}

/// Preload the rightmost column of `room_L` into `leftroom_`, so that column 0
/// of this room has something to draw its left-hand overhang from.
// seg008:041A
#[no_mangle]
pub unsafe extern "C" fn load_leftroom() {
    get_room_address(room_L as c_int);
    for row in 0usize..3 {
        get_tile_to_draw(room_L as c_int, 9, row as c_int, &mut leftroom_[row].tiletype, &mut leftroom_[row].modifier, (*custom).drawn_tile_left_level_edge);
    }
}

/// Preload `row_below_left_`: for each column, the tile *below and to the left*
/// of `(drawn_row, column)`, whose top-right corner pokes up into this row.
///
/// Entry 0 comes from the room to the left (or below-left), entries 1..=9 from
/// this room. Below the bottom row that means row 0 of `room_B`.
///
/// Leaves `curr_room_*` pointing back at `drawn_room`.
// seg008:0460
#[no_mangle]
pub unsafe extern "C" fn load_rowbelow() {
    let (room, room_left, row_below) = if drawn_row == 2 {
        (room_B, room_BL, 0usize)
    } else {
        (drawn_room, room_L, (drawn_row + 1) as usize)
    };
    get_room_address(room as c_int);
    for column in 1usize..10 {
        get_tile_to_draw(room as c_int, column as c_int - 1, row_below as c_int, &mut row_below_left_[column].tiletype, &mut row_below_left_[column].modifier, tiles_tiles_0_empty as u8);
    }
    get_room_address(room_left as c_int);
    get_tile_to_draw(room_left as c_int, 9, row_below as c_int, &mut row_below_left_[0].tiletype, &mut row_below_left_[0].modifier, tiles_tiles_20_wall as u8);
    get_room_address(drawn_room as c_int);
}

/// When this tile is see-through, draw the top-right corner of the tile
/// below-left through it, then black out the sliver of floor that would
/// otherwise show under the tile to the left.
// seg008:04FA
#[no_mangle]
pub unsafe extern "C" fn draw_tile_floorright() {
    if can_see_bottomleft() == 0 { return; }
    draw_tile_topright();
    if tile_table[tile_left as usize].floor_right == 0 { return; }
    add_backtable(
        chtabs_id_chtab_6_environment as c_short,
        42, // floor right part
        draw_xh as i8, 0,
        tile_table[tiles_tiles_1_floor as usize].right_y as c_int + draw_main_y as c_int,
        blitters_blitters_9_black as c_int, 1,
    );
}

/// Whether this tile is see-through enough that the top-right corner of the
/// tile below-left needs drawing through it.
// seg008:053A
#[no_mangle]
pub unsafe extern "C" fn can_see_bottomleft() -> c_int {
    matches!(
        curr_tile as tiles,
        tiles_tiles_0_empty
            | tiles_tiles_9_bigpillar_top
            | tiles_tiles_12_doortop
            | tiles_tiles_26_lattice_down
    ) as c_int
}

/// Draw the top-right corner of the tile below-left, which sticks up into this
/// tile's space.
///
/// A balcony with a non-default modifier is a teleport, and uses a different
/// sprite four images further along.
// seg008:055A
#[no_mangle]
pub unsafe extern "C" fn draw_tile_topright() {
    let below_left = row_below_left_[drawn_col as usize];
    match below_left.tiletype as tiles {
        tiles_tiles_7_doortop_with_floor | tiles_tiles_12_doortop => {
            // Only the palace tileset has a lintel above its doors.
            if (*custom).tbl_level_type[current_level as usize] == 0 { return; }
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                doortop_fram_top[below_left.modifier as usize] as c_int,
                draw_xh as i8, 0, draw_bottom_y as c_int,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        tiles_tiles_20_wall => {
            add_backtable(
                chtabs_id_chtab_7_environmentwall as c_short,
                2, draw_xh as i8, 0, draw_bottom_y as c_int,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        _ => {
            let mut id = tile_table[below_left.tiletype as usize].topright_id as c_int;
            // USE_TELEPORTS (always on)
            if (below_left.tiletype == tiles_tiles_23_balcony_left as u8 && below_left.modifier != 0)
                || (below_left.tiletype == tiles_tiles_24_balcony_right as u8 && below_left.modifier == 1)
            {
                id += 4;
            }
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                id, draw_xh as i8, 0, draw_bottom_y as c_int,
                blitters_blitters_2_or as c_int, 0,
            );
        }
    }
}

/// Draw the top of a gate that is below-left of a see-through tile: a mask to
/// punch a gate-shaped hole, then the lattice phase for its current openness.
// seg008:05D1
#[no_mangle]
pub unsafe extern "C" fn draw_tile_anim_topright() {
    if matches!(
        curr_tile as tiles,
        tiles_tiles_0_empty | tiles_tiles_9_bigpillar_top | tiles_tiles_12_doortop
    ) && row_below_left_[drawn_col as usize].tiletype == tiles_tiles_4_gate as u8
    {
        add_backtable(
            chtabs_id_chtab_6_environment as c_short,
            68, // gate top mask
            draw_xh as i8, 0, draw_bottom_y as c_int,
            blitters_blitters_40h_mono as c_int, 0,
        );
        // 188 is fully open; the modifier can read higher (0xFF = wedged open).
        let modifier = (row_below_left_[drawn_col as usize].modifier as u16).min(188);
        add_backtable(
            chtabs_id_chtab_6_environment as c_short,
            door_fram_top[((modifier >> 2) % 8) as usize] as c_int,
            draw_xh as i8, 0, draw_bottom_y as c_int,
            blitters_blitters_2_or as c_int, 0,
        );
    }
}

/// Draw the right-hand quarter of the tile to the left, which overhangs into
/// this one, plus the blue floor stripe that runs along the top of it in the
/// dungeon tileset.
///
/// Skipped entirely when this tile is a wall, since a wall covers the overhang.
// seg008:066A
#[no_mangle]
pub unsafe extern "C" fn draw_tile_right() {
    if curr_tile == tiles_tiles_20_wall as u8 { return; }
    match tile_left as tiles {
        tiles_tiles_0_empty => {
            if modifier_left > 3 { return; }
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                blueline_fram1[modifier_left as usize] as c_int,
                draw_xh as i8, 0,
                blueline_fram_y[modifier_left as usize] as c_int + draw_main_y as c_int,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        tiles_tiles_1_floor => {
            ptr_add_table(
                chtabs_id_chtab_6_environment as c_short,
                42, // floor B
                draw_xh as i8, 0,
                tile_table[tile_left as usize].right_y as c_int + draw_main_y as c_int,
                blitters_blitters_10h_transp as c_int, 0,
            );
            // Modifier 1 means "no blue stripe", which is also the palace
            // default, so each tileset has one modifier that draws nothing.
            let num = if modifier_left > 3 { 0 } else { modifier_left };
            let default_for_tileset =
                ((*custom).tbl_level_type[current_level as usize] != 0) as u8;
            if num == default_for_tileset { return; }
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                blueline_fram3[num as usize] as c_int,
                draw_xh as i8, 0,
                draw_main_y as c_int - 20,
                blitters_blitters_0_no_transp as c_int, 0,
            );
        }
        tiles_tiles_7_doortop_with_floor | tiles_tiles_12_doortop => {
            if (*custom).tbl_level_type[current_level as usize] == 0 { return; }
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                doortop_fram_bot[modifier_left as usize] as c_int,
                draw_xh as i8, 0,
                tile_table[tile_left as usize].right_y as c_int + draw_main_y as c_int,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        tiles_tiles_20_wall => {
            // Bit 7 of a wall's modifier is the "no blue" flag.
            if (*custom).tbl_level_type[current_level as usize] != 0 && (modifier_left & 0x80) == 0 {
                add_backtable(
                    chtabs_id_chtab_6_environment as c_short,
                    84, // wall stripe
                    draw_xh as i8 + 3, 0,
                    draw_main_y as c_int - 27,
                    blitters_blitters_0_no_transp as c_int, 0,
                );
            }
            add_backtable(
                chtabs_id_chtab_7_environmentwall as c_short,
                1,
                draw_xh as i8, 0,
                tile_table[tile_left as usize].right_y as c_int + draw_main_y as c_int,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        _ => {
            let mut id = tile_table[tile_left as usize].right_id as c_int;
            // USE_TELEPORTS (always on): a balcony with a non-default modifier
            // is a teleport and uses the sprite four images along.
            if (tile_left == tiles_tiles_23_balcony_left as u8 && modifier_left != 0)
                || (tile_left == tiles_tiles_24_balcony_right as u8 && modifier_left == 1)
            {
                id += 4;
            }
            if id != 0 {
                // A stuck floor only shows its distinctive right edge when
                // there is a real floor next to it to be stuck against.
                let blit = if tile_left == tiles_tiles_5_stuck as u8 {
                    if curr_tile == tiles_tiles_0_empty as u8
                        || curr_tile == tiles_tiles_5_stuck as u8
                        || tile_is_floor(curr_tile as c_int) == 0
                    {
                        id = 42; // floor B
                    }
                    blitters_blitters_10h_transp as c_int
                } else {
                    blitters_blitters_2_or as c_int
                };
                add_backtable(
                    chtabs_id_chtab_6_environment as c_short,
                    id, draw_xh as i8, 0,
                    tile_table[tile_left as usize].right_y as c_int + draw_main_y as c_int,
                    blit, 0,
                );
            }
            if (*custom).tbl_level_type[current_level as usize] != 0 {
                add_backtable(
                    chtabs_id_chtab_6_environment as c_short,
                    tile_table[tile_left as usize].stripe_id as c_int,
                    draw_xh as i8, 0,
                    draw_main_y as c_int - 27,
                    blitters_blitters_2_or as c_int, 0,
                );
            }
            if tile_left == tiles_tiles_19_torch as u8 || tile_left == tiles_tiles_30_torch_with_debris as u8 {
                add_backtable(
                    chtabs_id_chtab_6_environment as c_short,
                    146, // torch base
                    draw_xh as i8, 0,
                    draw_bottom_y as c_int - 28,
                    blitters_blitters_0_no_transp as c_int, 0,
                );
            }
        }
    }
}

/// Animation frame for a spike, from its modifier.
///
/// Bit 7 means "fully out and counting down", which is drawn as the widest
/// frame 5 regardless of how much countdown is left in the low bits.
// seg008:08A0
#[no_mangle]
pub unsafe extern "C" fn get_spike_frame(modifier: u8) -> c_int {
    if modifier & 0x80 != 0 { 5 } else { modifier as c_int }
}

/// The animated part of the overhang from the tile to the left: spike tips,
/// gate, level door, wobbling loose floor, or torch flame.
// seg008:08B5
#[no_mangle]
pub unsafe extern "C" fn draw_tile_anim_right() {
    match tile_left as tiles {
        tiles_tiles_2_spike => {
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                spikes_fram_right[get_spike_frame(modifier_left) as usize] as c_int,
                draw_xh as i8, 0,
                draw_main_y as c_int - 7,
                blitters_blitters_10h_transp as c_int, 0,
            );
        }
        tiles_tiles_4_gate => {
            draw_gate_back();
        }
        tiles_tiles_11_loose => {
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                loose_fram_right[get_loose_frame(modifier_left) as usize] as c_int,
                draw_xh as i8, 0,
                draw_bottom_y as c_int - 1,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        tiles_tiles_16_level_door_left => {
            draw_leveldoor();
        }
        tiles_tiles_19_torch | tiles_tiles_30_torch_with_debris => {
            if modifier_left < 9 {
                // USE_COLORED_TORCHES (always on when USE_TEXT is on). The
                // colour lives in torch_colors, where load_alter_mod moved it
                // out of the modifier so the modifier could hold the flame
                // animation frame.
                let color = if drawn_col == 0 {
                    // The torch is in the rightmost column of the room to the left.
                    torch_colors[room_L as usize][drawn_row as usize * 10 + 9]
                } else {
                    torch_colors[drawn_room as usize][drawn_row as usize * 10 + drawn_col as usize - 1]
                };
                let blit = if color != 0 {
                    blitters_blitters_colored_flame as c_int + (color as c_int & 0x3F)
                } else {
                    blitters_blitters_0_no_transp as c_int
                };
                add_backtable(
                    chtabs_id_chtab_1_flameswordpotion as c_short,
                    modifier_left as c_int + 1, // images 1..=9 are the flames
                    draw_xh as i8 + 1, 0,
                    draw_main_y as c_int - 40,
                    blit, 0,
                );
            }
        }
        _ => {}
    }
}

/// Draw the floor line at the bottom of this tile, and for a wall also its
/// procedurally generated brickwork.
///
/// `also_in_foretable` makes the same sprite go into the fore table as well, so
/// it covers a character standing on it. Only [`redraw_needed_above`] asks for
/// that, for the overhanging strip where a character can be behind the floor.
// seg008:0971
#[no_mangle]
pub unsafe extern "C" fn draw_tile_bottom(also_in_foretable: u16) {
    let mut id: u8 = 0;
    let mut blit = blitters_blitters_0_no_transp as c_int;
    let mut chtab_id: u16 = chtabs_id_chtab_6_environment as u16;
    if curr_tile == tiles_tiles_20_wall as u8 {
        // The palace tileset draws its walls entirely from wall_pattern's
        // coloured rectangles, unless the wall-drawing algorithm is forced on
        // or we are not in VGA.
        if (*custom).tbl_level_type[current_level as usize] == 0
            || (*custom).enable_wda_in_palace != 0
            || graphics_mode != grmodes_gmMcgaVga as u8
        {
            id = wall_fram_bottom[(curr_modifier & 0x7F) as usize];
        }
        chtab_id = chtabs_id_chtab_7_environmentwall as u16;
    } else {
        // C falls through from the doortop case into the default, so a doortop
        // gets the default id but an OR blit rather than an opaque one.
        if curr_tile == tiles_tiles_12_doortop as u8 {
            blit = blitters_blitters_2_or as c_int;
        }
        id = tile_table[curr_tile as usize].bottom_id;
    }
    if ptr_add_table(chtab_id as c_short, id as c_int, draw_xh as i8, 0, draw_bottom_y as c_int, blit, 0) != 0
        && also_in_foretable != 0
    {
        add_foretable(chtab_id as c_short, id as c_int, draw_xh as i8, 0, draw_bottom_y as c_int, blit, 0);
    }
    if chtab_id == chtabs_id_chtab_7_environmentwall as u16
        && graphics_mode != grmodes_gmCga as u8
        && graphics_mode != grmodes_gmHgaHerc as u8
    {
        wall_pattern(0, 0);
    }
}

/// Draw the bottom edge of a wobbling loose floor, into both the back and the
/// fore table so that it reads as being both under and in front of the Kid.
///
/// The unused argument is in the C signature too.
// seg008:0A38
#[no_mangle]
pub unsafe extern "C" fn draw_loose(_arg_0: c_int) {
    if curr_tile == tiles_tiles_11_loose as u8 {
        let id = loose_fram_bottom[get_loose_frame(curr_modifier) as usize] as c_int;
        add_backtable(chtabs_id_chtab_6_environment as c_short, id, draw_xh as i8, 0, draw_bottom_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
        add_foretable(chtabs_id_chtab_6_environment as c_short, id, draw_xh as i8, 0, draw_bottom_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
    }
}

/// Draw the tile's own body — the sprite you would name if asked what the tile
/// looks like.
// seg008:0A8E
#[no_mangle]
pub unsafe extern "C" fn draw_tile_base() {
    // USE_SUPER_HIGH_JUMP (always on): lattice is drawn from draw_tile_fore
    // instead, so the Kid passes behind it while jumping through.
    let is_lattice = (tiles_tiles_26_lattice_down..=tiles_tiles_29_lattice_right)
        .contains(&(curr_tile as tiles));
    let is_lattice_over_door =
        tile_left == tiles_tiles_26_lattice_down as u8 && curr_tile == tiles_tiles_12_doortop as u8;
    if (*fixes).enable_super_high_jump != 0 && (is_lattice || is_lattice_over_door) {
        return;
    }

    let (id, ybottom) = if is_lattice_over_door {
        (6, draw_main_y as c_int + 3) // combined lattice + door sprite
    } else if curr_tile == tiles_tiles_11_loose as u8 {
        (loose_fram_left[get_loose_frame(curr_modifier) as usize] as c_int, draw_main_y as c_int)
    } else if curr_tile == tiles_tiles_15_opener as u8
        && tile_left == tiles_tiles_0_empty as u8
        && (*custom).tbl_level_type[current_level as usize] == 0
    {
        (148, draw_main_y as c_int) // left half of open button with no floor to the left
    } else {
        (tile_table[curr_tile as usize].base_id as c_int, draw_main_y as c_int)
    };
    ptr_add_table(
        chtabs_id_chtab_6_environment as c_short, id, draw_xh as i8, 0,
        tile_table[curr_tile as usize].base_y as c_int + ybottom,
        blitters_blitters_10h_transp as c_int, 0,
    );
}

/// Draw the animated part of this tile's own body: spike, potion, sword, or
/// chomper.
// seg008:0B2B
#[no_mangle]
pub unsafe extern "C" fn draw_tile_anim() {
    match curr_tile as tiles {
        tiles_tiles_2_spike => {
            ptr_add_table(chtabs_id_chtab_6_environment as c_short, spikes_fram_left[get_spike_frame(curr_modifier) as usize] as c_int, draw_xh as i8, 0, draw_main_y as c_int - 2, blitters_blitters_10h_transp as c_int, 0);
        }
        tiles_tiles_10_potion => {
            // The potion's kind is in the modifier's top five bits and picks
            // both the bubble colour and the size of the flask. C reaches this
            // by falling from case 3/4 into case 2, so those share pot_size = 1.
            let (color, pot_size) = match (curr_modifier & 0xF8) >> 3 {
                0 => return,             // empty: nothing to draw
                5 | 6 => (9, 0),         // hurt / open: blue
                3 | 4 => (10, 1),        // slow fall / upside down: green, large
                2 => (12, 1),            // life: red, large
                _ => (12, 0),            // red, small
            };
            let ybottom = draw_main_y as c_int - (pot_size * 4) - 14;
            add_backtable(chtabs_id_chtab_1_flameswordpotion as c_short, 23 /*bubble mask*/, draw_xh as i8 + 3, 1, ybottom, blitters_blitters_40h_mono as c_int, 0);
            add_foretable(chtabs_id_chtab_1_flameswordpotion as c_short, potion_fram_bubb[(curr_modifier & 0x7) as usize] as c_int, draw_xh as i8 + 3, 1, ybottom, color + blitters_blitters_40h_mono as c_int, 0);
        }
        tiles_tiles_22_sword => {
            // Modifier 1 is a sword lying in the open, which is drawn in the
            // mid table and peeled so the Kid can walk over it.
            let is_free_sword = curr_modifier == 1;
            add_midtable(chtabs_id_chtab_1_flameswordpotion as c_short, is_free_sword as c_int + 10, draw_xh as i8, 0, draw_main_y as c_int - 3, blitters_blitters_10h_transp as c_int, is_free_sword as u8);
        }
        tiles_tiles_18_chomper => {
            let chomper_num = chomper_fram1[(curr_modifier & 0x7F).min(6) as usize] as usize;
            add_backtable(chtabs_id_chtab_6_environment as c_short, chomper_fram_bot[chomper_num] as c_int, draw_xh as i8, 0, draw_main_y as c_int, blitters_blitters_10h_transp as c_int, 0);
            if curr_modifier & 0x80 != 0 {
                // Bit 7 is the sticky "there is blood on these blades" flag.
                add_backtable(chtabs_id_chtab_6_environment as c_short, chomper_num as c_int + 114, draw_xh as i8 + 1, 4, draw_main_y as c_int - 6, blitters_blitters_4Ch_mono_12 as c_int, 0);
            }
            add_backtable(chtabs_id_chtab_6_environment as c_short, chomper_fram_top[chomper_num] as c_int, draw_xh as i8, 0, draw_main_y as c_int - chomper_fram_y[chomper_num] as c_int, blitters_blitters_10h_transp as c_int, 0);
        }
        _ => {}
    }
}

/// Draw everything that must occlude the characters: spike tips, chomper
/// blades, the wall's face, lattice, pillar fronts, potion flasks.
///
/// Also draws the front of a gate, but only when the Kid is actually standing
/// in that gate's doorway — otherwise the gate stays behind him.
// seg008:0D15
#[no_mangle]
pub unsafe extern "C" fn draw_tile_fore() {
    if tile_left == tiles_tiles_4_gate as u8
        && Kid.curr_row == drawn_row as i8
        && Kid.curr_col == drawn_col as i8 - 1
        && Kid.room != room_R as u8
    {
        draw_gate_fore();
    }
    match curr_tile as tiles {
        tiles_tiles_2_spike => {
            add_foretable(chtabs_id_chtab_6_environment as c_short, spikes_fram_fore[get_spike_frame(curr_modifier) as usize] as c_int, draw_xh as i8, 0, draw_main_y as c_int - 2, blitters_blitters_10h_transp as c_int, 0);
        }
        tiles_tiles_18_chomper => {
            let chomper_num = chomper_fram1[(curr_modifier & 0x7F).min(6) as usize] as usize;
            add_foretable(chtabs_id_chtab_6_environment as c_short, chomper_fram_for[chomper_num] as c_int, draw_xh as i8, 0, draw_main_y as c_int, blitters_blitters_10h_transp as c_int, 0);
            if curr_modifier & 0x80 != 0 {
                add_foretable(chtabs_id_chtab_6_environment as c_short, chomper_num as c_int + 119, draw_xh as i8 + 1, 4, draw_main_y as c_int - 6, blitters_blitters_4Ch_mono_12 as c_int, 0);
            }
        }
        tiles_tiles_20_wall => {
            if (*custom).tbl_level_type[current_level as usize] == 0
                || (*custom).enable_wda_in_palace != 0
                || graphics_mode != grmodes_gmMcgaVga as u8
            {
                add_foretable(chtabs_id_chtab_7_environmentwall as c_short, wall_fram_main[(curr_modifier & 0x7F) as usize] as c_int, draw_xh as i8, 0, draw_main_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
            }
            if graphics_mode != grmodes_gmCga as u8 && graphics_mode != grmodes_gmHgaHerc as u8 {
                wall_pattern(1, 1);
            }
        }
        // C has explicit lattice cases that then fall through into the default,
        // so the lattice body runs *and* the default body runs.
        _ => {
            // USE_SUPER_HIGH_JUMP (always on): lattice belongs in front of the
            // Kid so he passes behind it while jumping through, so its base
            // sprite is emitted here instead of from draw_tile_base.
            if (*fixes).enable_super_high_jump != 0 {
                if (tiles_tiles_26_lattice_down..=tiles_tiles_29_lattice_right)
                    .contains(&(curr_tile as tiles))
                {
                    add_foretable(chtabs_id_chtab_6_environment as c_short, tile_table[curr_tile as usize].base_id as c_int, draw_xh as i8, 0, tile_table[curr_tile as usize].base_y as c_int + draw_main_y as c_int, blitters_blitters_10h_transp as c_int, 0);
                }
                if tile_left == tiles_tiles_26_lattice_down as u8
                    && curr_tile == tiles_tiles_12_doortop as u8
                {
                    add_foretable(chtabs_id_chtab_6_environment as c_short, 6, draw_xh as i8, 0, tile_table[curr_tile as usize].base_y as c_int + draw_main_y as c_int + 3, blitters_blitters_10h_transp as c_int, 0);
                }
            }

            let mut id = tile_table[curr_tile as usize].fore_id as c_int;
            if id == 0 { return; }
            let xh = (tile_table[curr_tile as usize].fore_x as u16 + draw_xh) as i8;
            let ybottom = tile_table[curr_tile as usize].fore_y as c_int + draw_main_y as c_int;

            if curr_tile == tiles_tiles_10_potion as u8 {
                // Potion types 2..=4 come in the large flask, and both flasks
                // look different in the palace.
                let potion_type = ((curr_modifier & 0xF8) >> 3) as c_int;
                if (2..5).contains(&potion_type) { id = 13; }
                if (*custom).tbl_level_type[current_level as usize] != 0 { id += 2; }
                add_foretable(chtabs_id_chtab_1_flameswordpotion as c_short, id, xh, 6, ybottom, blitters_blitters_10h_transp as c_int, 0);
            } else {
                // A dungeon pillar and the small lattice pieces are opaque;
                // everything else keys out its background colour.
                let opaque = (curr_tile == tiles_tiles_3_pillar as u8
                    && (*custom).tbl_level_type[current_level as usize] == 0)
                    || (tiles_tiles_27_lattice_small..tiles_tiles_30_torch_with_debris)
                        .contains(&(curr_tile as tiles));
                let blit = if opaque {
                    blitters_blitters_0_no_transp as c_int
                } else {
                    blitters_blitters_10h_transp as c_int
                };
                add_foretable(chtabs_id_chtab_6_environment as c_short, id, xh, 0, ybottom, blit, 0);
            }
        }
    }
}

/// Work out where the gate to the left currently starts and ends on screen,
/// from its openness modifier.
///
/// All three outputs are `word` in C. In the above-room pass `draw_main_y` is
/// -1, so [`gate_top_y`] and [`gate_bottom_y`] come out negative and wrap to
/// ~65500 — which C's `int` promotion then compares as a large *positive*
/// number. Callers must widen them the same way; see [`gate_slice_top`].
// seg008:178E
#[no_mangle]
pub unsafe extern "C" fn calc_gate_pos() {
    gate_top_y = (draw_bottom_y as i32 - 62) as u16;
    gate_openness = (modifier_left.min(188) as u16 >> 2) + 1;
    gate_bottom_y = (draw_main_y as i32 - gate_openness as i32) as u16;
}

/// Walk the gate shaft from its bottom edge upwards in 8-pixel slices, calling
/// `emit` for each full slice, and return the y where the walk stopped.
///
/// The three loop conditions come straight from `seg008.c:1134`. The last one
/// compares a `short` against a `word`, which C evaluates after promoting both
/// to `int` — so a wrapped-negative [`gate_top_y`] reads as ~65500 and stops the
/// walk immediately. Computing it in `i16` instead would sign-extend the wrap
/// back to a small negative number and run the loop down to y = 0.
unsafe fn gate_slice_top(mut emit: impl FnMut(c_int)) -> i16 {
    let mut ybottom = gate_bottom_y as i16 - 12;
    if ybottom < 192 {
        while ybottom >= 0 && ybottom > 7 && (ybottom as c_int - 7) > gate_top_y as c_int {
            emit(ybottom as c_int);
            ybottom -= 8;
        }
    }
    ybottom
}

/// Draw a gate seen from the tile to its right: the bottom bar at its current
/// height, then the shaft above it, then a partial slice to meet the lintel.
// seg008:17B7
#[no_mangle]
pub unsafe extern "C" fn draw_gate_back() {
    calc_gate_pos();
    // Same word-vs-int promotion as in gate_slice_top.
    if gate_bottom_y as c_int + 12 < draw_main_y as c_int {
        add_backtable(chtabs_id_chtab_6_environment as c_short, 50 /*gate bottom with B*/, draw_xh as i8, 0, gate_bottom_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
    } else {
        // The gate is nearly shut, so its bottom bar overlaps the floor. This
        // opaque blit erases the top-right of the tile below-left...
        add_backtable(chtabs_id_chtab_6_environment as c_short, tile_table[tiles_tiles_4_gate as usize].right_id as c_int, draw_xh as i8, 0, tile_table[tiles_tiles_4_gate as usize].right_y as c_int + draw_main_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
        // ...and these redraw what it erased. The original misses the gate's
        // own top-right section, which FIX_GATE_DRAWING_BUG (always on) adds.
        if can_see_bottomleft() != 0 { draw_tile_topright(); }
        if (*fixes).fix_gate_drawing_bug != 0 {
            draw_tile_anim_topright();
        }
        draw_tile_bottom(0);
        draw_loose(0);
        draw_tile_base();
        add_backtable(chtabs_id_chtab_6_environment as c_short, 51 /*gate bottom*/, draw_xh as i8, 0, gate_bottom_y as c_int - 2, blitters_blitters_10h_transp as c_int, 0);
    }

    let ybottom = gate_slice_top(|y| {
        add_backtable(chtabs_id_chtab_6_environment as c_short, 52 /*gate slice 8px*/, draw_xh as i8, 0, y, blitters_blitters_0_no_transp as c_int, 0);
    });

    // How much of the topmost 8px slice is still below the lintel, 1..=8.
    // `word` in C, so a negative result wraps large and fails `< 9` — the same
    // outcome an i16 would give, but this is the shape the C source has.
    let gate_frame = (ybottom as c_int - gate_top_y as c_int + 1) as u16;
    if gate_frame > 0 && gate_frame < 9 {
        add_backtable(chtabs_id_chtab_6_environment as c_short, door_fram_slice[gate_frame as usize] as c_int, draw_xh as i8, 0, ybottom as c_int, blitters_blitters_0_no_transp as c_int, 0);
    }
}

/// The same gate, but in the fore table, for when the Kid is standing inside
/// the doorway and the bars must cover him.
// seg008:18BE
#[no_mangle]
pub unsafe extern "C" fn draw_gate_fore() {
    calc_gate_pos();
    add_foretable(chtabs_id_chtab_6_environment as c_short, 51 /*gate bottom*/, draw_xh as i8, 0, gate_bottom_y as c_int - 2, blitters_blitters_10h_transp as c_int, 0);
    gate_slice_top(|y| {
        add_foretable(chtabs_id_chtab_6_environment as c_short, 52 /*gate slice 8px*/, draw_xh as i8, 0, y, blitters_blitters_10h_transp as c_int, 0);
    });
}

/// Animation frame for a wobbling loose floor, from its modifier.
///
/// Bit 7 means "shaking but still attached". The frame tables only have 11
/// entries, so a modifier past the end — which a mod can produce by raising
/// `loose_floor_delay` — is clamped to 1 rather than drawing garbage.
// seg008:0FF6
#[no_mangle]
pub unsafe extern "C" fn get_loose_frame(modifier: u8) -> c_int {
    let mut m = modifier;
    if (m & 0x80) != 0 || (*custom).loose_floor_delay > 11 {
        m &= 0x7F;
        if m > 10 { return 1; }
    }
    m as c_int
}

/// Look up image `id` in chtab `chtab_id`, returning null rather than reading
/// out of bounds.
///
/// The C original bounds-checks the chtab with `>` instead of `>=`, so
/// `chtab_id == COUNT(chtab_addrs)` reads one past the array. That is not
/// reachable — no caller passes it — and reproducing it in Rust would be
/// undefined behaviour, so the check here is the correct `>=`. The C version
/// also prints a diagnostic on each failure, which is omitted.
#[no_mangle]
pub unsafe extern "C" fn get_image(chtab_id: c_short, id: c_int) -> *mut image_type {
    let n = core::mem::size_of_val(&chtab_addrs) / core::mem::size_of::<*mut chtab_type>();
    if chtab_id < 0 || chtab_id as usize >= n {
        return core::ptr::null_mut();
    }
    let chtab = chtab_addrs[chtab_id as usize];
    if chtab.is_null() { return core::ptr::null_mut(); }
    if id < 0 || id >= (*chtab).n_images as c_int {
        return core::ptr::null_mut();
    }
    // `images` is a C flexible array member, so it has to be indexed through a
    // raw pointer rather than as a Rust array.
    core::ptr::addr_of!((*chtab).images).cast::<*mut image_type>().add(id as usize).read()
}

/// Append a sprite to the back table (scenery behind the characters).
///
/// `id` is 1-based, so 0 means "nothing to draw" and is silently dropped; that
/// is what all the zero entries in the frame tables above rely on. The stored
/// `y` is the *top* of the sprite, computed from the caller's `ybottom` and the
/// image height. Returns 1 if an entry was appended.
// seg008:10A8
#[no_mangle]
pub unsafe extern "C" fn add_backtable(chtab_id: c_short, id: c_int, xh: i8, xl: i8, ybottom: c_int, blit: c_int, _peel: u8) -> c_int {
    if id == 0 { return 0; }
    let index = table_counts[0] as usize;
    if index >= 200 {
        show_dialog(b"BackTable Overflow\0".as_ptr() as *const c_char);
        return 0;
    }
    let item = &mut backtable[index];
    item.xh = xh;
    item.xl = xl;
    item.chtab_id = chtab_id as u8;
    item.id = (id - 1) as u8;
    let image = get_image(chtab_id, id - 1);
    if image.is_null() { return 0; }
    item.y = (ybottom - (*image).h as c_int + 1) as c_short;
    item.blit = blit;
    if draw_mode != 0 {
        draw_back_fore(0, index as c_int);
    }
    table_counts[0] += 1;
    1
}

/// Append a sprite to the fore table (scenery drawn over the characters).
/// Otherwise identical to [`add_backtable`].
// seg008:1017
#[no_mangle]
pub unsafe extern "C" fn add_foretable(chtab_id: c_short, id: c_int, xh: i8, xl: i8, ybottom: c_int, blit: c_int, _peel: u8) -> c_int {
    if id == 0 { return 0; }
    let index = table_counts[1] as usize;
    if index >= 200 {
        show_dialog(b"ForeTable Overflow\0".as_ptr() as *const c_char);
        return 0;
    }
    let item = &mut foretable[index];
    item.xh = xh;
    item.xl = xl;
    item.chtab_id = chtab_id as u8;
    item.id = (id - 1) as u8;
    let image = get_image(chtab_id, id - 1);
    if image.is_null() { return 0; }
    item.y = (ybottom - (*image).h as c_int + 1) as c_short;
    item.blit = blit;
    if draw_mode != 0 {
        draw_back_fore(1, index as c_int);
    }
    table_counts[1] += 1;
    1
}

/// Append a sprite to the mid table — characters, and the scenery that has to
/// be interleaved with them.
///
/// Unlike the back and fore tables, a mid table entry also carries a clip
/// rectangle, a peel flag, and a horizontal-flip flag folded into bit 7 of
/// `blit`. Everything in the game's artwork faces left, so a right-facing
/// character is drawn by flipping.
// seg008:113A
#[no_mangle]
pub unsafe extern "C" fn add_midtable(chtab_id: c_short, id: c_int, xh: i8, xl: i8, ybottom: c_int, mut blit: c_int, peel: u8) -> c_int {
    if id == 0 { return 0; }
    let index = table_counts[3] as usize;
    if index >= 50 {
        show_dialog(b"MidTable Overflow\0".as_ptr() as *const c_char);
        return 0;
    }
    let item = &mut midtable[index];
    item.xh = xh;
    item.xl = xl;
    item.chtab_id = chtab_id as u8;
    item.id = (id - 1) as u8;
    let image = get_image(chtab_id, id - 1);
    if image.is_null() { return 0; }
    item.y = (ybottom - (*image).h as c_int + 1) as c_short;
    if obj_direction == directions_dir_0_right as i8 && chtab_flip_clip[chtab_id as usize] != 0 {
        blit += 0x80;
    }
    item.blit = blit;
    item.peel = peel;
    item.clip.left = obj_clip_left;
    item.clip.right = obj_clip_right;
    item.clip.top = obj_clip_top;
    item.clip.bottom = obj_clip_bottom;
    if draw_mode != 0 {
        draw_mid(index as c_int);
    }
    table_counts[3] += 1;
    1
}

/// Snapshot the screen contents under a rectangle so [`restore_peels`] can put
/// them back next frame, erasing whatever was drawn on top without having to
/// redraw the scenery.
// seg008:1208
#[no_mangle]
pub unsafe extern "C" fn add_peel(left: c_int, right: c_int, top: c_int, height: c_int) {
    if peels_count >= 50 {
        show_dialog(b"Peels OverFlow\0".as_ptr() as *const c_char);
        return;
    }
    let rect = rect_type {
        left: left as c_short,
        right: right as c_short,
        top: top as c_short,
        bottom: (top + height) as c_short,
    };
    peels_table[peels_count as usize] = read_peel_from_screen(&rect);
    peels_count += 1;
}

/// Append a flat coloured rectangle to the wipe table. Used to erase a region
/// and to paint the palace tileset's solid-colour brickwork.
// seg008:1254
#[no_mangle]
pub unsafe extern "C" fn add_wipetable(layer: i8, left: c_short, bottom: c_short, height: i8, width: c_short, color: i8) {
    let index = table_counts[2] as usize;
    if index >= 300 {
        show_dialog(b"WipeTable Overflow\0".as_ptr() as *const c_char);
        return;
    }
    let item = &mut wipetable[index];
    item.left = left;
    item.bottom = bottom + 1;
    item.height = height;
    item.width = width;
    item.color = color;
    item.layer = layer;
    if draw_mode != 0 {
        draw_wipe(index as c_int);
    }
    table_counts[2] += 1;
}

/// Rasterize an entire display list, in insertion order. Table 3 is the mid
/// table; 0 and 1 are back and fore.
// seg008:12BB
#[no_mangle]
pub unsafe extern "C" fn draw_table(which_table: c_int) {
    let count = table_counts[which_table as usize];
    for index in 0..count as c_int {
        if which_table == 3 {
            draw_mid(index);
        } else {
            draw_back_fore(which_table, index);
        }
    }
}

// seg008:12FE
/// Rasterize the wipe-table entries belonging to one layer. Wipes are
/// interleaved with the other tables rather than drawn as a block, so that a
/// wipe can erase back-table output without erasing the characters.
#[no_mangle]
pub unsafe extern "C" fn draw_wipes(which: c_int) {
    let count = table_counts[2] as usize;
    for index in 0..count {
        if which == wipetable[index].layer as c_int {
            draw_wipe(index as c_int);
        }
    }
}

/// Rasterize one back- or fore-table entry. These are never flipped or clipped,
/// so image and mask are the same surface.
// seg008:133B
#[no_mangle]
pub unsafe extern "C" fn draw_back_fore(which_table: c_int, index: c_int) {
    let table_entry = if which_table == 0 {
        &backtable[index as usize]
    } else {
        &foretable[index as usize]
    };
    let image = get_image(table_entry.chtab_id as c_short, table_entry.id as c_int);
    draw_image(image, image, table_entry.xh as c_int * 8 + table_entry.xl as c_int, table_entry.y as c_int, table_entry.blit);
}

/// Mirror a surface horizontally, one 1-pixel-wide column blit at a time.
///
/// The caller owns the returned surface and must free it. Note the C original
/// dereferences `output` for the palette *before* checking it for null; that
/// ordering is preserved.
fn hflip(input: *mut SDL_Surface) -> *mut SDL_Surface {
    unsafe {
        let width = (*input).w;
        let height = (*input).h;
        let renderer = crate::platform::sdl::shared_renderer();
        let output = renderer.convert_surface(input, (*input).format, 0);
        renderer.set_surface_palette(output, (*(*input).format).palette);
        if output.is_null() {
            sdlperror(b"hflip: SDL_ConvertSurface\0".as_ptr() as *const c_char);
            quit(1);
        }
        renderer.set_blend_mode(input, 0); // SDL_BLENDMODE_NONE
        renderer.set_color_key(input, false, 0);
        renderer.set_color_key(output, false, 0);
        renderer.set_alpha_mod(input, 255);
        let mut source_x = 0;
        let mut target_x = width - 1;
        while source_x < width {
            let srcrect = SDL_Rect { x: source_x, y: 0, w: 1, h: height };
            let mut dstrect = SDL_Rect { x: target_x, y: 0, w: 1, h: height };
            if renderer.blit(input, &srcrect, output, &mut dstrect) != 0 {
                sdlperror(b"hflip: SDL_BlitSurface\0".as_ptr() as *const c_char);
                quit(1);
            }
            source_x += 1;
            target_x -= 1;
        }
        output
    }
}

/// Rasterize one mid-table entry, applying its clip rectangle, its horizontal
/// flip, and its peel.
///
/// Flipping produces a *new* surface, which becomes the image being drawn while
/// `mask` keeps pointing at the original — matching the C source, where only
/// `image` is reassigned by `hflip`. In practice no midtable blitter reads the
/// mask, but the distinction is preserved rather than collapsed.
// seg008:140C
#[no_mangle]
pub unsafe extern "C" fn draw_mid(index: c_int) {
    let entry = &midtable[index as usize];
    let image_id = entry.id as c_int;
    let chtab_id = entry.chtab_id as c_short;
    let mask = get_image(chtab_id, image_id);
    let mut xpos = entry.xh as c_int * 8 + entry.xl as c_int;
    let ypos = entry.y as c_int;

    // Bit 7 of blit is add_midtable's flip flag, not part of the blitter id.
    // C only strips it when it is set, so blit is left untouched otherwise.
    let mut blit = entry.blit;
    let blit_flip = blit & 0x80 != 0;
    if blit_flip {
        blit &= 0x7F;
    }

    if chtab_flip_clip[chtab_id as usize] != 0 {
        set_clip_rect(&entry.clip);
        if chtab_id != chtabs_id_chtab_0_sword as c_short {
            xpos = calc_screen_x_coord(xpos as c_short) as c_int;
        }
    }

    let image = if blit_flip {
        xpos -= (*mask).w as c_int;
        hflip(mask)
    } else {
        mask
    };

    if entry.peel != 0 {
        add_peel(
            round_xpos_to_byte(xpos, 0),
            round_xpos_to_byte((*image).w as c_int + xpos, 1),
            ypos,
            (*image).h as c_int,
        );
    }
    draw_image(image, mask, xpos, ypos, blit);

    if chtab_flip_clip[chtab_id as usize] != 0 {
        reset_clip_rect();
    }
    if blit_flip {
        crate::platform::sdl::shared_renderer().free_surface(image);
    }
}

/// Blit one sprite to the current target surface with the requested blitter,
/// and record the touched rectangle as dirty if the renderer wants dirty rects.
///
/// Blitters at or above 0x100 are the coloured-flame family and read the mask
/// rather than the image.
// seg008:167B
#[no_mangle]
pub unsafe extern "C" fn draw_image(image: *mut image_type, mask: *mut image_type, xpos: c_int, ypos: c_int, blit: c_int) {
    match blit {
        b if b == blitters_blitters_10h_transp as c_int => {
            draw_image_transp(image, mask, xpos, ypos);
        }
        b if b == blitters_blitters_9_black as c_int => {
            method_6_blit_img_to_scr(mask, xpos, ypos, blitters_blitters_9_black as c_int);
        }
        b if b == blitters_blitters_0_no_transp as c_int
          || b == blitters_blitters_2_or as c_int
          || b == blitters_blitters_3_xor as c_int => {
            method_6_blit_img_to_scr(image, xpos, ypos, blit);
        }
        b => {
            if b >= 0x100 {
                method_6_blit_img_to_scr(mask, xpos, ypos, blit);
            } else {
                method_3_blit_mono(image, xpos, ypos, 0, (blit & 0xBF) as u8);
            }
        }
    }
    if need_drects != 0 {
        let rect = rect_type {
            left: xpos as c_short,
            right: (xpos + (*image).w as c_int) as c_short,
            top: ypos as c_short,
            bottom: (ypos + (*image).h as c_int) as c_short,
        };
        add_drect(&rect as *const rect_type as *mut rect_type);
    }
}

/// Rasterize one wipe-table entry as a filled rectangle.
// seg008:1730
#[no_mangle]
pub unsafe extern "C" fn draw_wipe(index: c_int) {
    let ptr = &wipetable[index as usize];
    let rect = rect_type {
        left: ptr.left,
        right: ptr.left + ptr.width,
        top: ptr.bottom - ptr.height as c_short,
        bottom: ptr.bottom,
    };
    draw_rect(&rect, ptr.color as c_int);
    if need_drects != 0 {
        add_drect(&rect as *const rect_type as *mut rect_type);
    }
}

/// Put every saved peel back on screen, newest first, then drop them all.
// seg008:1C4E
#[no_mangle]
pub unsafe extern "C" fn restore_peels() {
    while peels_count > 0 {
        peels_count -= 1;
        let peel = peels_table[peels_count as usize];
        if need_drects != 0 {
            add_drect(&(*peel).rect as *const rect_type as *mut rect_type);
        }
        restore_peel(peel);
    }
    peels_count = 0;
}

/// Mark a rectangle as needing to be pushed to the screen.
///
/// Rectangles that touch (after growing by one pixel on each side) are merged
/// into the one they touch rather than added separately, which keeps the list
/// short at the cost of over-reporting. Overflow just drops the rectangle.
// seg008:1C8F
#[no_mangle]
pub unsafe extern "C" fn add_drect(source: *mut rect_type) {
    for index in 0..drects_count as usize {
        // target_rect is a scratch output; only whether they intersect matters.
        let mut target_rect = core::mem::zeroed::<rect_type>();
        if intersect_rect(&mut target_rect, shrink2_rect(&mut target_rect, source, -1, -1), &drects[index]) != 0 {
            let current_drect = &mut drects[index];
            union_rect(current_drect, current_drect, source);
            return;
        }
    }
    if drects_count >= 30 {
        show_dialog(b"DRects Overflow\0".as_ptr() as *const c_char);
        return;
    }
    drects[drects_count as usize] = *source;
    drects_count += 1;
}

/// Play back all five display lists onto the offscreen surface, in the order
/// that gives the right depth: peels, background wipes, back table, characters,
/// foreground wipes, fore table.
// seg008:1BEB
#[no_mangle]
pub unsafe extern "C" fn draw_tables() {
    drects_count = 0;
    current_target_surface = offscreen_surface;
    if is_blind_mode != 0 {
        draw_rect(&rect_top, colorids_color_0_black as c_int);
    }
    restore_peels();
    draw_wipes(0);
    draw_table(0); // backtable
    // FIX_BLACK_RECT (always on)
    draw_wipes(1);
    draw_table(3); // midtable
    draw_wipes(1);
    draw_table(1); // foretable
    current_target_surface = onscreen_surface_;
    show_copyprot(1);
}

/// Discard every saved peel without restoring it, for when the whole screen is
/// about to be repainted anyway.
// seg008:2627
#[no_mangle]
pub unsafe extern "C" fn free_peels() {
    while peels_count > 0 {
        peels_count -= 1;
        free_peel(peels_table[peels_count as usize]);
    }
}

/// Black out the bottom `height` pixels of the current tile, four bytes wide.
// seg008:1BCB
#[no_mangle]
pub unsafe extern "C" fn draw_tile_wipe(height: u8) {
    add_wipetable(0, (draw_xh * 8) as c_short, draw_bottom_y, height as i8, (4 * 8) as c_short, 0);
}

/// The per-frame draw: falling rubble, then characters, then the scenery tiles
/// that something has dirtied.
// seg008:1AF8
#[no_mangle]
pub unsafe extern "C" fn draw_moving() {
    draw_mobs();
    draw_people();
    redraw_needed_tiles();
}

/// Walk the room the same way [`draw_room`] does, but repaint only the tiles
/// whose `redraw_frames_*` counters say they are dirty.
///
/// The `draw_objtable_items_at_tile` calls at either end flush the characters
/// that belong to no tile: 30 is the "off the grid" bucket and 255 (`-1` as a
/// byte) is the bucket for characters in a room that is not being drawn.
// seg008:1B06
#[no_mangle]
pub unsafe extern "C" fn redraw_needed_tiles() {
    load_leftroom();
    draw_objtable_items_at_tile(30u8);
    for dr in (0i16..3).rev() {
        drawn_row = dr;
        load_rowbelow();
        draw_bottom_y = 63 * drawn_row + 65;
        draw_main_y = draw_bottom_y - 3;
        for dc in 0i16..10 {
            drawn_col = dc;
            load_curr_and_left_tile();
            redraw_needed((tbl_line_at(drawn_row as usize) as c_int + drawn_col as c_int) as c_short);
        }
    }
    let saved_drawn_room = drawn_room;
    drawn_room = room_A;
    load_room_links();
    load_leftroom();
    drawn_row = 2;
    load_rowbelow();
    for dc in 0i16..10 {
        drawn_col = dc;
        load_curr_and_left_tile();
        draw_main_y = -1;
        draw_bottom_y = 2;
        redraw_needed_above(drawn_col as c_int);
    }
    drawn_room = saved_drawn_room;
    load_room_links();
    draw_objtable_items_at_tile(255u8); // -1 as u8
}

/// Repaint as much of one tile as its dirty counters ask for, decrementing each
/// counter it acts on.
///
/// The counters are ordered from coarse to fine and the first two pairs are
/// mutually exclusive: a full redraw subsumes an animation-only redraw, and an
/// "other overlay" subsumes a floor overlay.
///
/// `tile_object_redraw == 0xFF` means a character overlaps the tile boundary, so
/// the tile to the left has to have its characters re-emitted too.
// seg008:0211
#[no_mangle]
pub unsafe extern "C" fn redraw_needed(tilepos: c_short) {
    if wipe_frames[tilepos as usize] != 0 {
        wipe_frames[tilepos as usize] -= 1;
        draw_tile_wipe(wipe_heights[tilepos as usize] as u8);
    }
    if redraw_frames_full[tilepos as usize] != 0 {
        redraw_frames_full[tilepos as usize] -= 1;
        draw_tile();
    } else if redraw_frames_anim[tilepos as usize] != 0 {
        redraw_frames_anim[tilepos as usize] -= 1;
        draw_tile_anim_topright();
        draw_tile_anim_right();
        draw_tile_anim();
        // FIX_ABOVE_GATE (always on)
        draw_tile_fore();
        draw_tile_bottom(0);
    }
    if redraw_frames2[tilepos as usize] != 0 {
        redraw_frames2[tilepos as usize] -= 1;
        draw_other_overlay();
    } else if redraw_frames_floor_overlay[tilepos as usize] != 0 {
        redraw_frames_floor_overlay[tilepos as usize] -= 1;
        draw_floor_overlay();
    }
    if tile_object_redraw[tilepos as usize] != 0 {
        if tile_object_redraw[tilepos as usize] == 0xFF {
            draw_objtable_items_at_tile((tilepos - 1) as u8);
        }
        draw_objtable_items_at_tile(tilepos as u8);
        tile_object_redraw[tilepos as usize] = 0;
    }
    if redraw_frames_fore[tilepos as usize] != 0 {
        redraw_frames_fore[tilepos as usize] -= 1;
        draw_tile_fore();
    }
}

/// [`redraw_needed`] for the overhanging strip of the room above.
///
/// `draw_tile_bottom(1)` and `draw_loose(1)` also emit into the fore table here,
/// because a character in the top row of this room can be standing behind that
/// strip's floor.
// seg008:02C1
#[no_mangle]
pub unsafe extern "C" fn redraw_needed_above(column: c_int) {
    if redraw_frames_above[column as usize] != 0 {
        redraw_frames_above[column as usize] -= 1;
        // FIX_BIGPILLAR_JUMP_UP (always on): wiping under a big pillar top
        // would erase the pillar.
        if curr_tile != tiles_tiles_9_bigpillar_top as u8 {
            draw_tile_wipe(3);
            draw_tile_floorright();
        }
        draw_tile_anim_topright();
        draw_tile_right();
        draw_tile_bottom(1);
        draw_loose(1);
        draw_tile_fore();
    }
}

/// Move every staged character that sits on `tilepos` out of the object table
/// and into the mid table, in back-to-front order.
///
/// This is the mechanism that puts characters at the right depth: the room walk
/// calls it once per tile, so a character is emitted between the scenery behind
/// it and the scenery in front of it.
// seg008:1F67
#[no_mangle]
pub unsafe extern "C" fn draw_objtable_items_at_tile(tilepos: u8) {
    let obj_count = table_counts[4];
    if obj_count == 0 { return; }
    n_curr_objs = 0;
    for obj_index in (0..obj_count as c_short).rev() {
        if objtable[obj_index as usize].tilepos == tilepos {
            curr_objs[n_curr_objs as usize] = obj_index;
            n_curr_objs += 1;
        }
    }
    if n_curr_objs != 0 {
        sort_curr_objs();
        for obj_index in 0..n_curr_objs as usize {
            draw_objtable_item(curr_objs[obj_index] as c_int);
        }
    }
}

/// Bubble-sort the characters found on one tile into back-to-front order.
///
/// The C loop never shrinks its upper bound, so this is the naive O(n²) form —
/// harmless, since `n_curr_objs` is at most a handful.
// seg008:1FDE
#[no_mangle]
pub unsafe extern "C" fn sort_curr_objs() {
    let last = n_curr_objs - 1;
    loop {
        let mut swapped = false;
        for index in 0..last as usize {
            if compare_curr_objs(index as c_int, index as c_int + 1) != 0 {
                curr_objs.swap(index, index + 1);
                swapped = true;
            }
        }
        if !swapped { break; }
    }
}

/// Whether the two staged characters are in the wrong order, i.e. whether
/// [`sort_curr_objs`] should swap them.
///
/// The shadow always goes first so it appears behind everything. Two pieces of
/// falling rubble sort by increasing y, everything else by decreasing y, so
/// lower characters end up drawn later and therefore in front.
// seg008:203C
#[no_mangle]
pub unsafe extern "C" fn compare_curr_objs(index1: c_int, index2: c_int) -> c_int {
    let obj_index1 = curr_objs[index1 as usize] as usize;
    if objtable[obj_index1].obj_type == 1 { return 1; }
    let obj_index2 = curr_objs[index2 as usize] as usize;
    if objtable[obj_index2].obj_type == 1 { return 0; }
    if objtable[obj_index1].obj_type == 0x80 && objtable[obj_index2].obj_type == 0x80 {
        return (objtable[obj_index1].y < objtable[obj_index2].y) as c_int;
    }
    (objtable[obj_index1].y > objtable[obj_index2].y) as c_int
}

/// Emit one staged object into the mid table, as whatever kind of thing it is.
// seg008:20CA
#[no_mangle]
pub unsafe extern "C" fn draw_objtable_item(index: c_int) {
    match load_obj_from_objtable(index) {
        0 | 4 => {
            // Kid or mirror image
            if obj_id == 0xFF { return; }
            // For a few frames after uniting with the shadow the Kid blinks
            // between his own sprite and the shadow's rendering.
            if united_with_shadow != 0 && (united_with_shadow % 2) == 0 {
                render_shadow_sprite();
                return;
            }
            add_midtable(obj_chtab as c_short, obj_id as c_int + 1, obj_xh as i8, obj_xl as i8, obj_y as c_int, blitters_blitters_10h_transp as c_int, 1);
        }
        2 | 3 | 5 => {
            // Guard, sword, or hurt splash — the Kid case falls through to here
            // in C when he is not blinking.
            add_midtable(obj_chtab as c_short, obj_id as c_int + 1, obj_xh as i8, obj_xl as i8, obj_y as c_int, blitters_blitters_10h_transp as c_int, 1);
        }
        1 => {
            // Shadow
            render_shadow_sprite();
        }
        0x80 => {
            // A falling loose floor, drawn as its three separate pieces.
            obj_direction = directions_dir_FF_left as i8;
            add_midtable(obj_chtab as c_short, loose_fram_left[obj_id as usize] as c_int, obj_xh as i8, obj_xl as i8, obj_y as c_int - 3, blitters_blitters_10h_transp as c_int, 1);
            add_midtable(obj_chtab as c_short, loose_fram_bottom[obj_id as usize] as c_int, obj_xh as i8, obj_xl as i8, obj_y as c_int, 0, 1);
            // C computes obj_x + 4 as an int and lets the sbyte parameter
            // truncate it; doing the add in i8 would overflow instead.
            add_midtable(obj_chtab as c_short, loose_fram_right[obj_id as usize] as c_int, (obj_x as c_int + 4) as i8, obj_xl as i8, obj_y as c_int - 1, blitters_blitters_10h_transp as c_int, 1);
        }
        _ => {}
    }
}

/// Copy one object-table entry into the `obj_*` globals that
/// [`draw_objtable_item`] and [`add_midtable`] read, and return its kind.
// seg008:2228
#[no_mangle]
pub unsafe extern "C" fn load_obj_from_objtable(index: c_int) -> c_int {
    let curr_obj = &objtable[index as usize];
    obj_xh = curr_obj.xh as u8;
    obj_x = curr_obj.xh as c_short;
    obj_xl = curr_obj.xl as u8;
    obj_y = curr_obj.y as u8;
    obj_id = curr_obj.id;
    obj_chtab = curr_obj.chtab_id;
    obj_direction = curr_obj.direction;
    obj_clip_top = curr_obj.clip.top;
    obj_clip_bottom = curr_obj.clip.bottom;
    obj_clip_left = curr_obj.clip.left;
    obj_clip_right = curr_obj.clip.right;
    curr_obj.obj_type as c_int
}

/// Stage the Kid, the guard, their swords and any hurt splashes into the object
/// table, then draw the health bars.
// seg008:228A
#[no_mangle]
pub unsafe extern "C" fn draw_people() {
    check_mirror();
    draw_kid();
    draw_guard();
    reset_obj_clip();
    draw_hp();
}

// seg008:22A2
#[no_mangle]
pub unsafe extern "C" fn draw_kid() {
    if Kid.room != 0 && Kid.room == drawn_room as u8 {
        add_kid_to_objtable();
        if hitp_delta < 0 {
            draw_hurt_splash();
        }
        add_sword_to_objtable();
    }
}

// seg008:22C9
#[no_mangle]
pub unsafe extern "C" fn draw_guard() {
    if Guard.direction != directions_dir_56_none as i8 && Guard.room == drawn_room as u8 {
        add_guard_to_objtable();
        if guardhp_delta < 0 {
            draw_hurt_splash();
        }
        add_sword_to_objtable();
    }
}

/// Resolve the Kid's current frame into a sprite, position and clip rectangle,
/// mark the tiles he touches for redraw, and stage him.
// seg008:22F0
#[no_mangle]
pub unsafe extern "C" fn add_kid_to_objtable() {
    loadkid();
    load_fram_det_col();
    load_frame_to_obj();
    stuck_lower();
    set_char_collision();
    set_objtile_at_char();
    redraw_at_char();
    redraw_at_char2();
    clip_char();
    add_objtable(0);
}

/// As [`add_kid_to_objtable`], for the guard.
///
/// The shadow is a special case: on the mirror level it is clipped to the right
/// of the mirror, so it can only ever be seen as a reflection.
// seg008:2324
#[no_mangle]
pub unsafe extern "C" fn add_guard_to_objtable() {
    loadshad();
    load_fram_det_col();
    load_frame_to_obj();
    stuck_lower();
    set_char_collision();
    set_objtile_at_char();
    redraw_at_char();
    redraw_at_char2();
    clip_char();
    let obj_type = if Char.charid == charids_charid_1_shadow as u8 {
        if current_level == (*custom).mirror_level && Char.room == (*custom).mirror_room {
            obj_clip_left = 137;
            obj_clip_left += ((*custom).mirror_column as c_short - 4) * 32;
        }
        1u8 // shadow
    } else {
        2u8 // guard
    };
    add_objtable(obj_type);
}

/// Append the current `obj_*` globals to the object table as a staged object,
/// and mark the tile it lands on for redraw.
// seg008:2388
#[no_mangle]
pub unsafe extern "C" fn add_objtable(obj_type: u8) {
    let index = table_counts[4] as usize;
    table_counts[4] += 1;
    if index >= 50 {
        show_dialog(b"ObjTable Overflow\0".as_ptr() as *const c_char);
        return;
    }
    let entry = &mut objtable[index];
    entry.obj_type = obj_type;
    x_to_xh_and_xl(obj_x as c_int, &mut entry.xh, &mut entry.xl);
    entry.y = obj_y as c_short;
    entry.clip.top = obj_clip_top;
    entry.clip.bottom = obj_clip_bottom;
    entry.clip.left = obj_clip_left;
    entry.clip.right = obj_clip_right;
    entry.chtab_id = obj_chtab;
    entry.id = obj_id;
    entry.direction = obj_direction;
    mark_obj_tile_redraw(index as c_int);
}

/// Record which tile a staged object sits on, so [`redraw_needed`] knows to
/// re-emit it there. `obj_tilepos >= 30` means "not on the visible grid".
// seg008:2423
#[no_mangle]
pub unsafe extern "C" fn mark_obj_tile_redraw(index: c_int) {
    objtable[index as usize].tilepos = obj_tilepos;
    if obj_tilepos < 30 {
        tile_object_redraw[obj_tilepos as usize] = 1;
    }
}

/// Turn `Char`'s current animation frame into the `obj_*` sprite description.
///
/// The frame's `sword` byte packs which chtab to use in its top two bits (kid,
/// kid-with-sword, guard, guard-with-sword), and its `flags` byte's bit 7 is an
/// even/odd pixel adjustment that only applies for one of the two facings.
// seg008:2448
#[no_mangle]
pub unsafe extern "C" fn load_frame_to_obj() {
    let chtab_base = chtabs_id_chtab_2_kid as u8;
    reset_obj_clip();
    load_frame();
    obj_direction = Char.direction;
    obj_id = cur_frame.image;
    obj_chtab = chtab_base + (cur_frame.sword >> 6);
    obj_x = (char_dx_forward(cur_frame.dx as c_int) << 1) as c_short - 116;
    obj_y = (cur_frame.dy as c_int + Char.y as c_int) as u8;
    // C tests `(sbyte)(flags ^ direction) >= 0`, i.e. bit 7 clear.
    if (cur_frame.flags ^ obj_direction as u8) & 0x80 == 0 {
        obj_x += 1;
    }
}

/// Redraw the lip of the floor the Kid is climbing onto, in the mid table, so
/// it covers his hands as he pulls himself up.
///
/// Only meaningful when the tile to the left is see-through — otherwise there is
/// nothing to see through and the wall to the left already hides him.
// seg008:1E3A
#[no_mangle]
pub unsafe extern "C" fn draw_floor_overlay() {
    // FIX_BIGPILLAR_CLIMB (always on): without this, climbing to a floor with a
    // big pillar top behind it lets the Kid be seen through the floor.
    if tile_left != tiles_tiles_0_empty as u8
        && ((*fixes).fix_bigpillar_climb == 0 || tile_left != tiles_tiles_9_bigpillar_top as u8)
    {
        return;
    }
    if matches!(
        curr_tile as tiles,
        tiles_tiles_1_floor | tiles_tiles_3_pillar | tiles_tiles_5_stuck | tiles_tiles_19_torch
    ) {
        // Frames 137..=144 are the climb. C also prints a diagnostic when the
        // frame is outside that window (reachable via a corrupt saved game);
        // the diagnostic is omitted here, the guard is not.
        if (frameids_frame_137_climbing_3..=frameids_frame_144_climbing_10)
            .contains(&(Kid.frame as frameids))
        {
            let overlay_id = floor_left_overlay[(Kid.frame - frameids_frame_137_climbing_3 as u8) as usize];
            add_midtable(
                chtabs_id_chtab_6_environment as c_short,
                overlay_id as c_int,
                draw_xh as i8, 0,
                (curr_tile == tiles_tiles_5_stuck as u8) as c_int + draw_main_y as c_int,
                blitters_blitters_10h_transp as c_int, 0,
            );
        }
        ptr_add_table = add_midtable;
        draw_tile_bottom(0);
        ptr_add_table = add_backtable;
    } else {
        draw_other_overlay();
    }
}

/// Redraw this tile's scenery in the mid table so it covers a character who is
/// standing behind it.
///
/// Two cases: the tile to the left is see-through, so the character shows
/// through it and the overhang must be re-emitted in front; or the tile *two*
/// to the left is see-through, in which case the scenery goes into both the mid
/// and the back table and the tile is flagged 0xFF so its left neighbour's
/// characters get re-emitted too.
// seg008:1EB5
#[no_mangle]
pub unsafe extern "C" fn draw_other_overlay() {
    // Deliberately local, not the curr_tile/curr_modifier globals: the caller is
    // mid-tile and clobbering them would corrupt the rest of its drawing.
    let mut tiletype: u8 = 0;
    let mut modifier: u8 = 0;
    if tile_left == tiles_tiles_0_empty as u8 {
        ptr_add_table = add_midtable;
        draw_tile2();
    } else if curr_tile != tiles_tiles_0_empty as u8
        && drawn_col > 0
        && get_tile_to_draw(
            drawn_room as c_int, drawn_col as c_int - 2, drawn_row as c_int,
            &mut tiletype, &mut modifier,
            tiles_tiles_0_empty as u8,
        ) == tiles_tiles_0_empty as c_int
    {
        ptr_add_table = add_midtable;
        draw_tile2();
        ptr_add_table = add_backtable;
        draw_tile2();
        tile_object_redraw[(tbl_line_at(drawn_row as usize) as c_int + drawn_col as c_int) as usize] = 0xFF;
    }
    ptr_add_table = add_backtable;
}

/// The subset of [`draw_tile`] the overlays re-emit: everything except the
/// pieces that belong to neighbouring tiles and the foreground.
// seg008:1F48
#[no_mangle]
pub unsafe extern "C" fn draw_tile2() {
    draw_tile_right();
    draw_tile_anim_right();
    draw_tile_base();
    draw_tile_anim();
    draw_tile_bottom(0);
    draw_loose(0);
}

/// Rewrite every tile modifier in the level from its stored form into the form
/// the renderer wants. Run once, when a level is loaded.
// seg008:1937
#[no_mangle]
pub unsafe extern "C" fn alter_mods_allrm() {
    // USE_COLORED_TORCHES (always on)
    memset(torch_colors.as_mut_ptr() as *mut c_void, 0, core::mem::size_of_val(&torch_colors));

    // level.used_rooms reads 25 on some levels; clamp it to the real maximum.
    if level.used_rooms > 24 { level.used_rooms = 24; }
    for room in 1u16..=level.used_rooms as u16 {
        get_room_address(room as c_int);
        room_L = level.roomlinks[room as usize - 1].left as u16;
        room_R = level.roomlinks[room as usize - 1].right as u16;
        for tilepos in 0usize..30 {
            load_alter_mod(tilepos as c_int);
        }
    }
}

/// Rewrite one tile's modifier from its stored form into its runtime form.
///
/// See the module docs; the interesting cases are the wall, which gains
/// neighbour bits from [`wall_connection_block`], and the torch, which moves its
/// colour out to `torch_colors` so the modifier byte is free to hold the flame
/// animation frame.
// seg008:198E
#[no_mangle]
pub unsafe extern "C" fn load_alter_mod(tilepos: c_int) {
    let curr_tile_modif = curr_room_modif.add(tilepos as usize);
    let tiletype = (*curr_room_tiles.add(tilepos as usize)) & 0x1F;
    match tiletype as tiles {
        tiles_tiles_4_gate => {
            // Stored 1 means "starts open"; 188 is fully open in quarter-pixels.
            *curr_tile_modif = if *curr_tile_modif == 1 { 188 } else { 0 };
        }
        tiles_tiles_11_loose => {
            *curr_tile_modif = 0;
        }
        tiles_tiles_10_potion => {
            // The potion kind moves into bits 3..=7, leaving the low three bits
            // for the bubble animation frame.
            *curr_tile_modif <<= 3;
            // USE_COPYPROT (always on): on the copy-protection level one
            // designated potion is forced open, as the answer to the quiz.
            if current_level == 15
                && copyprot_room_at(copyprot_plac as usize) == loaded_room
                && copyprot_tile_at(copyprot_plac as usize) == tilepos as u16
            {
                *curr_tile_modif = 6 << 3; // open potion
            }
        }
        tiles_tiles_20_wall => {
            // The original moved the "no blue" flag to bit 7 and threw the rest
            // away. Keeping the stored value in bits 4..=6 instead leaves room
            // for the fake-tile codes that get_tile_to_draw decodes, and leaves
            // the low two bits free for the wall connection.
            let stored_modif = *curr_tile_modif;
            *curr_tile_modif = if stored_modif == 1 { 0x80 } else { stored_modif << 4 };
            wall_connection_block(tilepos as usize, curr_tile_modif, tiletype);
        }
        tiles_tiles_0_empty | tiles_tiles_1_floor => {
            // USE_FAKE_TILES (always on): modifier 5 (or 13) makes this tile
            // impersonate a wall, so it needs wall connections too.
            if (*curr_tile_modif & 7) == 5 {
                wall_connection_block(tilepos as usize, curr_tile_modif, tiletype);
            }
        }
        tiles_tiles_19_torch | tiles_tiles_30_torch_with_debris => {
            // USE_COLORED_TORCHES (always on)
            torch_colors[loaded_room as usize][tilepos as usize] = *curr_tile_modif;
            *curr_tile_modif = 0;
        }
        _ => {}
    }
}

/// Draw the level exit door: the stairs at its foot, then the door panel
/// stacked in 4-pixel slices up to its current openness, then its top.
///
/// In the start room the stairs are wiped to black instead of drawn, because
/// that is where the Kid enters and the stairs would be in front of him.
// seg008:1D29
#[no_mangle]
pub unsafe extern "C" fn draw_leveldoor() {
    let is_palace = (*custom).tbl_level_type[current_level as usize] != 0;
    let ybottom = draw_main_y as c_int - 13;
    leveldoor_right = (draw_xh << 3) + 48;
    if is_palace { leveldoor_right += 8; }
    add_backtable(chtabs_id_chtab_6_environment as c_short, 99 /*leveldoor stairs bottom*/, draw_xh as i8 + 1, 0, ybottom, blitters_blitters_0_no_transp as c_int, 0);
    if modifier_left != 0 {
        if level.start_room != drawn_room as u8 {
            add_backtable(chtabs_id_chtab_6_environment as c_short, 144 /*level door stairs*/, draw_xh as i8 + 1, 0, ybottom - 4, blitters_blitters_0_no_transp as c_int, 0);
        } else {
            let leveldoor_width: c_short = if is_palace { 48 } else { 39 };
            // Dungeon level doors sit 2px further right.
            let x_low: c_short = if is_palace { 0 } else { 2 };
            add_wipetable(0, (8 * (draw_xh + 1)) as c_short + x_low, (ybottom - 4) as c_short, 45, leveldoor_width, 0);
        }
    }
    leveldoor_ybottom = (ybottom - (modifier_left & 3) as c_int - 48) as u16;
    let y = ybottom - modifier_left as c_int;
    // Runs at least once, so a fully shut door still gets its bottom panel.
    loop {
        add_backtable(chtabs_id_chtab_6_environment as c_short, 33 /*level door bottom*/, draw_xh as i8 + 1, 0, leveldoor_ybottom as c_int, blitters_blitters_0_no_transp as c_int, 0);
        // leveldoor_ybottom is a `word` and wraps negative in the above-room
        // pass, where C's int promotion makes it compare as a large positive.
        if y <= leveldoor_ybottom as c_int { break; }
        leveldoor_ybottom = (leveldoor_ybottom as c_int + 4) as u16;
    }
    add_backtable(chtabs_id_chtab_6_environment as c_short, 34 /*level door top*/, draw_xh as i8 + 1, 0, draw_main_y as c_int - 64, blitters_blitters_0_no_transp as c_int, 0);
}

/// Tick the countdown by one frame and, at the milestones, put the remaining
/// time on the bottom status line.
///
/// `rem_min` counts *down* to 0 and then keeps going negative, which
/// ALLOW_INFINITE_TIME repurposes as time *elapsed*: `!rem_min` (C's `~`) turns
/// -1, -2, -3… into 0, 1, 2… minutes passed. Messages appear every minute for
/// the last five, every five minutes before that, and every five minutes once
/// the clock has run out.
// seg008:24A8
#[no_mangle]
pub unsafe extern "C" fn show_time() {
    // FIX_ONE_HP_STOPS_BLINKING (always on): drive the blink from here rather
    // than from the HP display, which stops updating at 1 HP.
    global_blink_state = !global_blink_state;

    if Kid.alive < 0
        // FREEZE_TIME_DURING_END_MUSIC (always on)
        && !((*fixes).enable_freeze_time_during_end_music != 0 && next_level != current_level)
        // ALLOW_INFINITE_TIME (always on): prevent overflow
        && !(rem_min == i16::MIN && rem_tick == 1)
        && rem_min != 0
        && (current_level < (*custom).victory_stops_time_level
            || (current_level == (*custom).victory_stops_time_level && leveldoor_open == 0))
        && current_level < 15
    {
        // rem_tick is a `word`; C wraps rather than trapping if it is already 0.
        rem_tick = rem_tick.wrapping_sub(1);
        if rem_tick == 0 {
            rem_tick = 719; // 720 = 12*60 ticks = one minute
            rem_min -= 1;
            if rem_min > 0 && (rem_min <= 5 || rem_min % 5 == 0) {
                is_show_time = 1;
            } else if rem_min < 0 {
                is_show_time = if (!rem_min) % 5 == 0 { 1 } else { 0 };
            }
        } else if rem_min == 1 && rem_tick % 12 == 0 {
            is_show_time = 1;
            text_time_remaining = 0;
        }
    }
    if is_show_time != 0 && text_time_remaining == 0 {
        text_time_remaining = 24;
        text_time_total = 24;
        if rem_min > 0 {
            let mut buf = [0i8; 40];
            if rem_min == 1 {
                // Widened because rem_tick is a `word`; C promotes to int here.
                let rem_sec = (rem_tick as c_int + 1) / 12;
                if rem_sec == 1 {
                    let s = b"1 SECOND LEFT\0";
                    for (i, &b) in s.iter().enumerate() { buf[i] = b as i8; }
                    text_time_remaining = 12;
                    text_time_total = 12;
                } else {
                    let s = format!("{} SECONDS LEFT\0", rem_sec);
                    for (i, b) in s.bytes().enumerate().take(39) { buf[i] = b as i8; }
                }
            } else {
                let s = format!("{} MINUTES LEFT\0", rem_min);
                for (i, b) in s.bytes().enumerate().take(39) { buf[i] = b as i8; }
            }
            display_text_bottom(buf.as_ptr());
        } else {
            // ALLOW_INFINITE_TIME (always on)
            if rem_min < 0 {
                let mut buf = [0i8; 40];
                let inv = !rem_min;
                if inv == 0 {
                    text_time_remaining = 0;
                    text_time_total = 0;
                    // display empty string (clears text area)
                    display_text_bottom(buf.as_ptr());
                } else if inv == 1 {
                    let s = b"1 MINUTE PASSED\0";
                    for (i, &b) in s.iter().enumerate() { buf[i] = b as i8; }
                    display_text_bottom(buf.as_ptr());
                } else {
                    let s = format!("{} MINUTES PASSED\0", inv);
                    for (i, b) in s.bytes().enumerate().take(39) { buf[i] = b as i8; }
                    display_text_bottom(buf.as_ptr());
                }
            } else {
                // rem_min == 0
                display_text_bottom(b"TIME HAS EXPIRED!\0".as_ptr() as *const c_char);
            }
        }
        is_show_time = 0;
    }
}

/// Announce the level number on the bottom status line.
///
/// Level 13 is the one the game lies about — it is presented as level 12, since
/// the real 12 is the mirror-corridor sequence and 13 continues it. Note the
/// *visibility* test runs on the real level number and only the *displayed*
/// number is remapped, matching `seg008.c:1837`: a mod that maps 13 to a number
/// at or above `hide_level_number_from_level` still shows it.
// seg008:25A8
#[no_mangle]
pub unsafe extern "C" fn show_level() {
    // FIX_LEVEL_14_RESTARTING (always on)
    text_time_remaining = 0;
    text_time_total = 0;
    // C holds this in a `byte`, then promotes it to int for the comparison
    // against the `word` threshold.
    let real_level = current_level as u8;
    if real_level != 0
        && (real_level as u16) < (*custom).hide_level_number_from_level
        && seamless == 0
    {
        let disp_level = if real_level == 13 {
            (*custom).level_13_level_number
        } else {
            real_level
        };
        text_time_remaining = 24;
        text_time_total = 24;
        let s = format!("LEVEL {}\0", disp_level);
        let mut buf = [0i8; 32];
        for (i, b) in s.bytes().enumerate().take(31) { buf[i] = b as i8; }
        display_text_bottom(buf.as_ptr());
        is_show_time = 1;
    }
    seamless = 0;
}

/// Scale a logical x coordinate from the game's 280-pixel play area into the
/// 320-pixel screen. Used to un-squash flipped character sprites.
// seg008:2602
#[no_mangle]
pub unsafe extern "C" fn calc_screen_x_coord(logical_x: c_short) -> c_short {
    (logical_x as i32 * 320 / 280) as c_short
}

/// Clear the bottom status line and print `text` centred on it.
// seg008:2644
#[no_mangle]
pub unsafe extern "C" fn display_text_bottom(text: *const c_char) {
    draw_rect(&rect_bottom_text, colorids_color_0_black as c_int);
    show_text(&rect_bottom_text, halign_center as c_int, valign_bottom as c_int, text);
    // USE_TEXT is on, so SDL_SetWindowTitle is NOT called here
}

/// Erase the bottom status line; `arg_0` also cancels any message still due to
/// be shown.
// seg008:266D
#[no_mangle]
pub unsafe extern "C" fn erase_bottom_text(arg_0: c_int) {
    draw_rect(&rect_bottom_text, colorids_color_0_black as c_int);
    if arg_0 != 0 {
        text_time_total = 0;
        text_time_remaining = 0;
    }
    // USE_TEXT is on, so SDL_SetWindowTitle is NOT called here
}

// ---------------------------------------------------------------------------
// Dungeon wall drawing algorithm by HTamas.
//
// The dungeon's brickwork is not artwork; it is generated. Each wall tile seeds
// the PRNG from its own (room, row, column) so the same tile always produces
// the same bricks, draws a small number of divider and decal sprites at random
// offsets, and restores the PRNG afterwards so gameplay randomness is
// unaffected. The palace tileset instead paints flat coloured rectangles from
// `palace_wall_colors` with a few mortar decals on top.
// ---------------------------------------------------------------------------

/// Image set 7 is `id_chtab_7_environmentwall`, which holds the wall pieces.
const RSET_WALL: c_short = 7;
const RES_WALL_FACE_MAIN: c_int = 1;
const RES_WALL_FACE_TOP: c_int = 2;
const RES_WALL_CENTRE_BASE: c_int = 3;
const RES_WALL_CENTRE_MAIN: c_int = 4;
const RES_WALL_RIGHT_BASE: c_int = 5;
const RES_WALL_RIGHT_MAIN: c_int = 6;
const RES_WALL_SINGLE_BASE: c_int = 7;
const RES_WALL_SINGLE_MAIN: c_int = 8;
const RES_WALL_LEFT_BASE: c_int = 9;
const RES_WALL_LEFT_MAIN: c_int = 10;
const RES_WALL_DIVIDER1: c_int = 11;
const RES_WALL_DIVIDER2: c_int = 12;
const RES_WALL_RNDBLOCK: c_int = 13;
const RES_WALL_MARK_TL: c_int = 14;
const RES_WALL_MARK_BL: c_int = 15;
const RES_WALL_MARK_TR: c_int = 16;
const RES_WALL_MARK_BR: c_int = 17;
const BLIT_NO_TRANS: c_int = 0;
const BLIT_TRANS: c_int = 16;
// The two low bits load_alter_mod put in a wall's modifier, naming the left
// tile, this tile and the right tile as Solid or Wall.
const WALL_MODIFIER_SWS: u8 = 0;
const WALL_MODIFIER_SWW: u8 = 1;
const WALL_MODIFIER_WWS: u8 = 2;
const WALL_MODIFIER_WWW: u8 = 3;

/// Generate one wall tile's brickwork.
///
/// `which_part` selects the tall face (1) or just the base course (0);
/// `which_table` selects the back (0) or fore (1) table. Both are set by the two
/// call sites, [`draw_tile_bottom`] and [`draw_tile_fore`], which between them
/// paint a wall in two passes at two depths.
///
/// The PRNG is reseeded from the tile's own coordinates so the pattern is stable
/// across frames and rooms, and restored on the way out so this cannot perturb
/// gameplay randomness.
///
/// Which dividers and decals appear depends on the wall's neighbour bits: a
/// brick course only gets a divider where there is a wall for it to run into.
// seg008:268F
#[no_mangle]
pub unsafe extern "C" fn wall_pattern(which_part: c_int, which_table: c_int) {
    let saved_sim = ptr_add_table;
    ptr_add_table = if which_table == 0 { add_backtable } else { add_foretable };
    let saved_prng_state = random_seed;
    random_seed = (drawn_room as u32)
        .wrapping_add(tbl_line_at(drawn_row as usize) as u32)
        .wrapping_add(drawn_col as u32);
    prandom(1); // fetch one and discard, to stir the fresh seed
    let is_dungeon = ((*custom).tbl_level_type[current_level as usize] < 1)
        || (*custom).enable_wda_in_palace != 0;
    if !is_dungeon && graphics_mode == grmodes_gmMcgaVga as u8 {
        // The palace wall algorithm was never traced from the original; this is
        // flat colours from a table plus a few mortar decals.
        if which_part != 0 {
            add_wipetable(which_table as i8, (8 * draw_xh) as c_short, (draw_main_y - 40) as c_short, 20, (4 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + drawn_col as usize] as i8);
            add_wipetable(which_table as i8, (8 * draw_xh) as c_short, (draw_main_y - 19) as c_short, 21, (2 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 11 + drawn_col as usize] as i8);
            add_wipetable(which_table as i8, (8 * (draw_xh + 2)) as c_short, (draw_main_y - 19) as c_short, 21, (2 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 12 + drawn_col as usize] as i8);
            add_wipetable(which_table as i8, (8 * draw_xh) as c_short, draw_main_y as c_short, 19, (1 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 22 + drawn_col as usize] as i8);
            add_wipetable(which_table as i8, (8 * (draw_xh + 1)) as c_short, draw_main_y as c_short, 19, (3 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 23 + drawn_col as usize] as i8);
            ptr_add_table(RSET_WALL, prandom(2) as c_int + 3, draw_xh as i8 + 3, 0, draw_main_y as c_int - 53, blitters_blitters_46h_mono_6 as c_int, 0);
            ptr_add_table(RSET_WALL, prandom(2) as c_int + 6, draw_xh as i8, 0, draw_main_y as c_int - 34, blitters_blitters_46h_mono_6 as c_int, 0);
            ptr_add_table(RSET_WALL, prandom(2) as c_int + 9, draw_xh as i8, 0, draw_main_y as c_int - 13, blitters_blitters_46h_mono_6 as c_int, 0);
            ptr_add_table(RSET_WALL, prandom(2) as c_int + 12, draw_xh as i8, 0, draw_main_y as c_int, blitters_blitters_46h_mono_6 as c_int, 0);
        }
        add_wipetable(which_table as i8, (8 * draw_xh) as c_short, draw_bottom_y as c_short, 3, (4 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 33 + drawn_col as usize] as i8);
        ptr_add_table(RSET_WALL, prandom(2) as c_int + 15, draw_xh as i8, 0, draw_bottom_y as c_int, blitters_blitters_46h_mono_6 as c_int, 0);
    } else {
        // 0 = thick brick divider, 1 = thin; the offset shifts it horizontally.
        // All four are drawn even in the branches that use only some of them,
        // because the draws must consume the same number of PRNG values either
        // way for the pattern to stay stable.
        let middle_divider = prandom(1) as c_int;
        let middle_divider_offset = prandom(4) as c_int;
        let bottom_divider = prandom(1) as c_int;
        let bottom_divider_offset = prandom(4) as c_int;
        let bg_modifier = curr_modifier & 0x7F;
        match bg_modifier {
            WALL_MODIFIER_WWW => {
                if which_part != 0 {
                    if prandom(4) == 0 {
                        ptr_add_table(RSET_WALL, RES_WALL_RNDBLOCK, draw_xh as i8, 0, draw_bottom_y as c_int - 42, BLIT_NO_TRANS, 0);
                    }
                    ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + middle_divider, draw_xh as i8 + 1, middle_divider_offset as i8, draw_bottom_y as c_int - 21, BLIT_TRANS, 0);
                }
                ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + bottom_divider, draw_xh as i8, bottom_divider_offset as i8, draw_bottom_y as c_int, BLIT_TRANS, 0);
                if which_part != 0 && is_dungeon {
                    if prandom(4) == 0 { draw_right_mark(prandom(3) as u16, middle_divider_offset as u16); }
                    if prandom(4) == 0 { draw_left_mark(prandom(4) as u16, (middle_divider_offset - middle_divider) as u16, (bottom_divider_offset - bottom_divider) as u16); }
                }
            }
            WALL_MODIFIER_SWS => {
                if is_dungeon && which_part != 0 {
                    if prandom(6) == 0 { draw_left_mark(prandom(1) as u16, (middle_divider_offset - middle_divider) as u16, (bottom_divider_offset - bottom_divider) as u16); }
                }
            }
            WALL_MODIFIER_SWW => {
                if which_part != 0 {
                    if prandom(4) == 0 {
                        ptr_add_table(RSET_WALL, RES_WALL_RNDBLOCK, draw_xh as i8, 0, draw_bottom_y as c_int - 42, BLIT_NO_TRANS, 0);
                    }
                    ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + middle_divider, draw_xh as i8 + 1, middle_divider_offset as i8, draw_bottom_y as c_int - 21, BLIT_TRANS, 0);
                    if is_dungeon {
                        if prandom(4) == 0 { draw_right_mark(prandom(3) as u16, middle_divider_offset as u16); }
                        if prandom(4) == 0 { draw_left_mark(prandom(3) as u16, (middle_divider_offset - middle_divider) as u16, (bottom_divider_offset - bottom_divider) as u16); }
                    }
                }
            }
            WALL_MODIFIER_WWS => {
                if which_part != 0 {
                    ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + middle_divider, draw_xh as i8 + 1, middle_divider_offset as i8, draw_bottom_y as c_int - 21, BLIT_TRANS, 0);
                }
                ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + bottom_divider, draw_xh as i8, bottom_divider_offset as i8, draw_bottom_y as c_int, BLIT_TRANS, 0);
                if which_part != 0 && is_dungeon {
                    if prandom(4) == 0 { draw_right_mark(prandom(1) as u16 + 2, middle_divider_offset as u16); }
                    if prandom(4) == 0 { draw_left_mark(prandom(4) as u16, (middle_divider_offset - middle_divider) as u16, (bottom_divider_offset - bottom_divider) as u16); }
                }
            }
            _ => {}
        }
    }
    random_seed = saved_prng_state;
    ptr_add_table = saved_sim;
}

/// Draw a chipped-mortar decal on the left of a wall tile.
///
/// The five variants alternate between the top-left and bottom-left images and
/// climb the tile; variants 2 and 3 also step one byte to the right, and
/// variants above 1 pick up a horizontal offset from the divider they sit
/// beside (the bottom divider's for 4, the middle divider's for 2 and 3).
///
/// `middle_offset` and `bottom_offset` are `word` in C and reach here as
/// `offset - divider`, which is -1 when the offset is 0 and the divider is 1 —
/// hence the wrapping arithmetic, which the `sbyte` parameter then truncates
/// back to -1 anyway.
#[no_mangle]
pub unsafe extern "C" fn draw_left_mark(decal_variant: u16, middle_offset: u16, bottom_offset: u16) {
    /// Height up the tile of each variant. The last entry appears unused.
    static LPOS: [u16; 5] = [58, 41, 37, 20, 16];
    let image_id = if decal_variant % 2 != 0 { RES_WALL_MARK_BL } else { RES_WALL_MARK_TL };
    let x_low = if decal_variant > 3 {
        bottom_offset.wrapping_add(6)
    } else if decal_variant > 1 {
        middle_offset.wrapping_add(6)
    } else {
        0
    };
    ptr_add_table(
        RSET_WALL, image_id,
        draw_xh as i8 + (decal_variant == 2 || decal_variant == 3) as i8,
        x_low as i8,
        draw_bottom_y as c_int - LPOS[decal_variant as usize] as c_int,
        BLIT_TRANS, 0,
    );
}

/// Draw a chipped-mortar decal on the right of a wall tile. As
/// [`draw_left_mark`], but only four variants, and the two lower ones ignore the
/// divider offset and sit at a fixed x instead.
#[no_mangle]
pub unsafe extern "C" fn draw_right_mark(decal_variant: u16, middle_offset: u16) {
    /// Height up the tile of each variant. The last entry appears unused.
    static RPOS: [u16; 4] = [52, 42, 31, 21];
    let image_id = if decal_variant % 2 != 0 { RES_WALL_MARK_BR } else { RES_WALL_MARK_TR };
    let x_low = if decal_variant < 2 { 24 } else { middle_offset.wrapping_sub(3) };
    ptr_add_table(
        RSET_WALL, image_id,
        draw_xh as i8 + (decal_variant > 1) as i8,
        x_low as i8,
        draw_bottom_y as c_int - RPOS[decal_variant as usize] as c_int,
        BLIT_TRANS, 0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // can_see_bottomleft returns 1 only for the four tile types that allow
    // seeing the bottom-left corner: empty, bigpillar_top, doortop, lattice_down.
    #[test]
    fn can_see_bottomleft_transparent_tiles() {
        unsafe {
            for &t in &[
                tiles_tiles_0_empty as u8,
                tiles_tiles_9_bigpillar_top as u8,
                tiles_tiles_12_doortop as u8,
                tiles_tiles_26_lattice_down as u8,
            ] {
                curr_tile = t;
                assert_eq!(can_see_bottomleft(), 1, "tile {t} should be transparent");
            }
            // A solid tile must not be transparent.
            curr_tile = tiles_tiles_1_floor as u8;
            assert_eq!(can_see_bottomleft(), 0);
            curr_tile = tiles_tiles_20_wall as u8;
            assert_eq!(can_see_bottomleft(), 0);
        }
    }

    // get_spike_frame: modifier bit 7 set → frame 5; otherwise raw modifier.
    #[test]
    fn get_spike_frame_mapping() {
        unsafe {
            assert_eq!(get_spike_frame(0), 0);
            assert_eq!(get_spike_frame(3), 3);
            assert_eq!(get_spike_frame(0x80), 5); // bit 7 set → 5
            assert_eq!(get_spike_frame(0x83), 5); // any bit-7 value → 5
        }
    }

    // get_loose_frame: high bit clear and value ≤ 10 → raw value;
    // high bit set or value > 10 (with default loose_floor_delay ≤ 11) → masked + clamp to 1.
    #[test]
    fn get_loose_frame_normal_range() {
        setup();
        unsafe {
            for v in 0u8..=10 {
                assert_eq!(get_loose_frame(v), v as c_int, "modifier {v}");
            }
        }
    }

    #[test]
    fn get_loose_frame_high_bit_clamps() {
        setup();
        unsafe {
            // 0x80 | 0 = 0x80 → masked = 0 ≤ 10, so return 0
            assert_eq!(get_loose_frame(0x80), 0);
            // 0x80 | 11 = 0x8B → masked = 11 > 10, return 1
            assert_eq!(get_loose_frame(0x8B), 1);
            // 0x80 | 5 = 0x85 → masked = 5 ≤ 10, return 5
            assert_eq!(get_loose_frame(0x85), 5);
        }
    }

    // calc_screen_x_coord scales logical x coordinates from 280-pixel space to 320-pixel space.
    #[test]
    fn calc_screen_x_coord_scaling() {
        unsafe {
            assert_eq!(calc_screen_x_coord(0), 0);
            assert_eq!(calc_screen_x_coord(280), 320);
            assert_eq!(calc_screen_x_coord(140), 160);
        }
    }

    // tile_table spot-check: verify a few entries match the C initializer exactly.
    #[test]
    fn tile_table_spot_check() {
        // 0x01 floor: base_id=41, floor_left=1, base_y=0, right_id=42, bottom_id=43
        let floor = &tile_table[1];
        assert_eq!(floor.base_id, 41);
        assert_eq!(floor.floor_left, 1);
        assert_eq!(floor.base_y, 0);
        assert_eq!(floor.right_id, 42);
        assert_eq!(floor.bottom_id, 43);

        // 0x00 empty: all zeros
        let empty = &tile_table[0];
        assert_eq!(empty.base_id, 0);
        assert_eq!(empty.fore_id, 0);

        // 0x14 wall (index 20): base_id=0, right_id=1, topright_id=2
        let wall = &tile_table[20];
        assert_eq!(wall.base_id, 0);
        assert_eq!(wall.right_id, 1);
        assert_eq!(wall.topright_id, 2);

        // 0x1A lattice_down (index 26): base_id=1, fore_y=-53
        let lattice = &tile_table[26];
        assert_eq!(lattice.base_id, 1);
        assert_eq!(lattice.fore_y, -53);
    }

    // The gate shaft is walked bottom-up in 8-pixel slices, stopping when the
    // next slice would cross the lintel at gate_top_y.
    #[test]
    fn gate_slices_walk_upwards_in_8px_steps() {
        unsafe {
            gate_top_y = 3;
            gate_bottom_y = 62;
            let mut ys = Vec::new();
            let stop = gate_slice_top(|y| ys.push(y));
            assert_eq!(ys, vec![50, 42, 34, 26, 18]);
            assert_eq!(stop, 10);
        }
    }

    // Invariant: gate_top_y is a `word`, and C compares it after promoting it
    // to int. A gate y that has wrapped negative therefore reads as a large
    // positive and stops the walk immediately. Computing the comparison in i16
    // would sign-extend it back to a small negative and run the walk to y = 0.
    #[test]
    fn gate_slices_treat_a_wrapped_gate_top_as_a_large_positive() {
        unsafe {
            gate_top_y = (-60i32) as u16;
            gate_bottom_y = 100;
            let mut emitted = 0;
            let stop = gate_slice_top(|_| emitted += 1);
            assert_eq!(emitted, 0);
            assert_eq!(stop, 88);
            gate_top_y = 0;
            gate_bottom_y = 0;
        }
    }

    // The six "fake wall" modifiers all turn a floor into a wall, and each maps
    // to the wall modifier that selects the right neighbour-connection sprite.
    #[test]
    fn get_tile_to_draw_decodes_fake_wall_modifiers() {
        unsafe {
            set_options_to_default();
            let mut tiles_buf = [0u8; 30];
            let mut modif_buf = [0u8; 30];
            curr_room_tiles = tiles_buf.as_mut_ptr();
            curr_room_modif = modif_buf.as_mut_ptr();
            let (mut tiletype, mut modifier) = (0u8, 0u8);
            for (stored, expected) in [(5u8, 0u8), (13, 0x80), (50, 0), (51, 1), (52, 2), (53, 3)] {
                for base in [tiles_tiles_1_floor, tiles_tiles_0_empty] {
                    tiles_buf[0] = base as u8;
                    modif_buf[0] = stored;
                    let drawn = get_tile_to_draw(1, 0, 0, &mut tiletype, &mut modifier, 0);
                    assert_eq!(drawn, tiles_tiles_20_wall as c_int, "modifier {stored}");
                    assert_eq!(modifier, expected, "modifier {stored}");
                }
            }
            curr_room_tiles = core::ptr::null_mut();
            curr_room_modif = core::ptr::null_mut();
            set_options_to_default();
        }
    }

    // Regression: draw_other_overlay must look up the tile two columns to the
    // left into LOCAL temporaries, never into the global curr_tile/curr_modifier.
    // Clobbering curr_tile here makes a later draw_tile_fore render a wall in the
    // foreground (extra foretable entries) for a non-wall tile.
    #[test]
    fn draw_other_overlay_does_not_clobber_curr_tile() {
        unsafe {
            set_options_to_default();
            let mut tiles = [tiles_tiles_1_floor as u8; 30];
            let mut modifs = [0u8; 30];
            curr_room_tiles = tiles.as_mut_ptr();
            curr_room_modif = modifs.as_mut_ptr();
            drawn_room = 1;
            drawn_row = 0;
            drawn_col = 2;
            tile_left = tiles_tiles_1_floor as u8; // not empty -> first branch skipped
            curr_tile = tiles_tiles_20_wall as u8; // not empty -> enters else-if
            curr_modifier = 7;
            // Tile two columns left (col 0) is floor (non-empty), so the branch
            // body is not executed; only get_tile_to_draw's out-params are written.
            draw_other_overlay();
            assert_eq!(curr_tile, tiles_tiles_20_wall as u8,
                "draw_other_overlay must not modify global curr_tile");
            assert_eq!(curr_modifier, 7,
                "draw_other_overlay must not modify global curr_modifier");
            curr_room_tiles = core::ptr::null_mut();
            curr_room_modif = core::ptr::null_mut();
            set_options_to_default();
        }
    }
}

/// Point `curr_room_tiles` / `curr_room_modif` at `room`'s 30-tile slice of the
/// level. `room == 0` records the room number but leaves the pointers alone, so
/// the caller keeps whatever room was loaded before.
// seg008:1E0C
unsafe fn get_room_address_impl(state: &mut State, room: c_int) {
    *state.loaded_room() = room as u16;
    if room != 0 {
        curr_room_tiles = state.level().fg.as_mut_ptr().add((room as usize - 1) * 30);
        *state.curr_room_modif() = state.level().bg.as_mut_ptr().add((room as usize - 1) * 30);
    }
}

#[no_mangle]
pub unsafe extern "C" fn get_room_address(room: c_int) {
    get_room_address_impl(&mut State, room);
}
