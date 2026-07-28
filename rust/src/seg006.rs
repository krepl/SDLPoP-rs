//! Where a character is, what he is standing on, and what his body looks like.
//!
//! This is the bookkeeping layer the rest of the game asks questions of. It
//! answers three kinds of question.
//!
//! * **"What tile is at …?"** A level is 24 rooms of 10×3 tiles, and rooms are
//!   stitched together by `level.roomlinks`. [`get_tile_impl`] takes a room and
//!   a (column, row) that may point *outside* that room, and
//!   [`find_room_of_tile_impl`] walks the links until the coordinate lands
//!   inside a real room — so callers can freely ask for "one tile to my left"
//!   at a room boundary and get the neighbouring room's tile back. The answer
//!   is left in the globals `curr_room` / `tile_col` / `tile_row` /
//!   `curr_tilepos` / `curr_tile2`, and a lot of code in other modules reads
//!   those *after* a `get_tile` call rather than using its return value. That
//!   makes the call order load-bearing in several places here.
//!
//!   A family of thin wrappers asks the question relative to the current
//!   character: [`get_tile_at_char_impl`], [`get_tile_infrontof_char_impl`],
//!   [`get_tile_behind_char_impl`], [`get_tile_above_char_impl`] and friends.
//!   "In front" and "behind" come from the `dir_front` / `dir_behind` tables
//!   indexed by `direction + 1`, so they work for both facings without a
//!   branch.
//!
//! * **"Where is the character in pixels?"** Character x-positions live in a
//!   14-pixels-per-tile coordinate system whose origin is `SCREENSPACE_X`
//!   (58). [`get_tile_div_mod_impl`] splits such an x into a tile column
//!   (returned) and the offset within that tile (left in `obj_xl`), and
//!   [`distance_to_edge_impl`] turns that offset into "how many pixels until
//!   the character walks off this tile", measured in his facing direction.
//!   Because the DOS original did this with two 256-byte lookup tables,
//!   out-of-range x-positions read whatever bytes sat next to those tables in
//!   memory; `BOGUS_BEFORE` / `BOGUS_AFTER` reproduce those reads exactly, and
//!   they are load-bearing — the game really does index out of range.
//!
//! * **"What does he look like this frame?"** [`load_frame_impl`] looks up
//!   `Char.frame` in one of three frame tables (Kid/mouse, guard/skeleton,
//!   cutscene characters) to get the sprite image, its dx/dy offset, and a
//!   flags byte. The flags carry the character's *weight offset* in the low
//!   five bits (`FRAME_WEIGHT_X`, used by [`dx_weight_impl`] to find where his
//!   centre of gravity is, which is what decides which column he counts as
//!   standing in), plus `FRAME_THIN` (narrow this frame for collision) and
//!   `FRAME_NEEDS_FLOOR` (this pose falls if there is no floor under it).
//!   [`set_char_collision_impl`] turns the frame's sprite into a screen-space
//!   bounding box, and [`clip_char_impl`] shrinks that box where a wall, a
//!   door top or the level door should cut the sprite off.
//!
//! On top of that sit the per-frame drivers: [`play_seq_impl`] interprets the
//! animation bytecode in `seqtbl.c` (see the `SEQ_*` opcodes below),
//! [`play_kid_impl`] and [`play_guard_impl`] run one frame of the Kid and of
//! his opponent, and [`check_action_impl`] decides each frame whether the
//! character should start falling, grab a ledge, or keep doing what he is
//! doing. The control-latching helpers ([`read_user_control_impl`],
//! [`save_ctrl_1_impl`], [`rest_ctrl_1_impl`]) implement the convention that a
//! direction is `CONTROL_HELD` on the frame it is first pressed and
//! `CONTROL_IGNORE` once a move has consumed it.
//!
//! # Step D migration
//!
//! Each function's logic lives in a private `*_impl(state: &mut State, ...)`
//! that reaches shared game state through the `State` facade (see
//! [`crate::state`]) instead of bare globals. The original
//! `#[no_mangle] pub unsafe extern "C" fn` names are kept as thin wrappers that
//! construct a `State` handle and forward. `State` borrows the same
//! `static mut` globals, so there is only ever one copy of the data and
//! unmigrated C-side callers see exactly what they always saw.

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;
use crate::platform::Renderer;
use crate::state::State;

// seqtbl is defined in seqtbl.c with no header; declare it directly.
extern "C" {
    #[link_name = "seqtbl"]
    static seqtbl_data: [u8; 0];
}

/// Address the DOS original's sequence table started at.
///
/// `Char.curr_seq` is a *DOS* address, not an index: the byte it points at is
/// `seqtbl[curr_seq - SEQTBL_BASE]`. Keeping the bias means the two-byte jump
/// targets baked into the bytecode can be stored in `curr_seq` verbatim.
const SEQTBL_BASE: u16 = 0x196E;

/// Reads one byte of animation bytecode at table index `idx`.
///
/// `seqtbl` is an incomplete extern array (`[u8; 0]` to bindgen), so it has to
/// be indexed through a raw pointer.
unsafe fn seqtbl_byte(idx: usize) -> u8 {
    *core::ptr::addr_of!(seqtbl_data).cast::<u8>().add(idx)
}

/// Reads the bytecode byte the current character's program counter points at,
/// without advancing it.
unsafe fn seq_peek(state: &mut State) -> u8 {
    seqtbl_byte(state.Char().curr_seq.wrapping_sub(SEQTBL_BASE) as usize)
}

/// Reads the bytecode byte at the program counter and advances past it.
///
/// Every C site that reads an operand does `*(SEQTBL_0 + Char.curr_seq)` and
/// then `Char.curr_seq++`; nothing between the two reads `curr_seq`, so doing
/// the increment first is equivalent.
unsafe fn seq_next(state: &mut State) -> u8 {
    let byte = seq_peek(state);
    state.Char().curr_seq = state.Char().curr_seq.wrapping_add(1);
    byte
}

/// Takes the two-byte little-endian jump target at the program counter and
/// jumps to it. Shared by [`SEQ_JMP`] and the taken branch of
/// [`SEQ_JMP_IF_FEATHER`], which fall through to each other in the C source.
unsafe fn seq_jump(state: &mut State) {
    let idx = state.Char().curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
    state.Char().curr_seq = u16::from_le_bytes([seqtbl_byte(idx), seqtbl_byte(idx + 1)]);
}

// ── SEQ opcode constants ──────────────────────────────────────────────────────

/// Move forward by the next byte (signed), in the character's facing direction.
const SEQ_DX:             u8 = 0xFB;
/// Move down the screen by the next byte (signed).
const SEQ_DY:             u8 = 0xFA;
/// Turn around (bitwise-complement the facing direction).
const SEQ_FLIP:           u8 = 0xFE;
/// Jump to the next two-byte target, but only while feather fall is active;
/// otherwise skip the target and carry on.
const SEQ_JMP_IF_FEATHER: u8 = 0xF7;
/// Jump to the next two-byte target.
const SEQ_JMP:            u8 = 0xFF;
/// Move up one tile row (and wake any chompers in the new row).
const SEQ_UP:             u8 = 0xFD;
/// Move down one tile row (and wake any chompers in the new row).
const SEQ_DOWN:           u8 = 0xFC;
/// Set `Char.action` from the next byte — what the character counts as doing
/// (standing, in mid-air, hanging, …).
const SEQ_ACTION:         u8 = 0xF9;
/// Set the fall velocity from the next two bytes (`fall_x`, then `fall_y`).
const SEQ_SET_FALL:       u8 = 0xF8;
/// Knock loose floors in the room above.
const SEQ_KNOCK_UP:       u8 = 0xF5;
/// Knock loose floors in this room.
const SEQ_KNOCK_DOWN:     u8 = 0xF4;
/// Play the sound named by the next byte (one of the `SND_*` values).
const SEQ_SOUND:          u8 = 0xF2;
/// Finish the level.
const SEQ_END_LEVEL:      u8 = 0xF1;
/// Act on the pending pickup (1) or teleporter (2), per the next byte.
const SEQ_GET_ITEM:       u8 = 0xF3;
/// No-op; marks the death frames in the source bytecode.
const SEQ_DIE:            u8 = 0xF6;

/// No sound, but guards still notice the Kid.
const SND_SILENT:   u8 = 0;
/// Footstep (guards notice).
const SND_FOOTSTEP: u8 = 1;
/// Bumping into a wall (guards notice).
const SND_BUMP:     u8 = 2;
/// Drinking a potion.
const SND_DRINK:    u8 = 3;
/// End-of-level music.
const SND_LEVEL:    u8 = 4;

// ── Compile-time constants (all feature flags active) ─────────────────────────

/// X coordinate of the left edge of the play field, and the origin of the
/// 14-units-per-tile character coordinate system.
const SCREENSPACE_X: i32 = 58;
/// Width of a tile in character-x units.
const TILE_SIZEX:    i32 = 14;
/// Height of a tile in pixels.
const TILE_SIZEY:    i32 = 63;
/// Offset of the middle of a tile in character-x units.
const TILE_MIDX:     i32 = 7;
/// Offset of the rightmost unit of a tile (`TILE_SIZEX - 1`).
const TILE_RIGHTX:   i32 = 13;
/// Index into `x_bump` of the leftmost column that is actually drawn.
const FIRST_ONSCREEN_COLUMN: i32 = 5;
/// Terminal falling speed, in pixels per frame.
const FALLING_SPEED_MAX:           i8 = 33;
/// Gravity, in pixels per frame per frame.
const FALLING_SPEED_ACCEL:         i8 = 3;
/// Terminal falling speed while floating on a feather potion.
const FALLING_SPEED_MAX_FEATHER:   i8 = 4;
/// Gravity while floating on a feather potion.
const FALLING_SPEED_ACCEL_FEATHER: i8 = 1;

// ── Raw-pointer helpers for incomplete-array globals ──────────────────────────

/// Column offset of the tile *in front of* a character, indexed by
/// `direction + 1` (0 = facing left, 2 = facing right).
///
/// `dir_front` is an incomplete extern array (`[i8; 0]` to bindgen), so it has
/// to be indexed through a raw pointer.
unsafe fn dir_front_at(idx: usize) -> i8 {
    *core::ptr::addr_of!(dir_front).cast::<i8>().add(idx)
}
/// Column offset of the tile *behind* a character, indexed by `direction + 1`.
///
/// Incomplete extern array — see [`dir_front_at`].
unsafe fn dir_behind_at(idx: usize) -> i8 {
    *core::ptr::addr_of!(dir_behind).cast::<i8>().add(idx)
}
/// Tile index at which room row `idx` starts: `{0, 10, 20}`.
///
/// Incomplete extern array — see [`dir_front_at`]. Note the element type is
/// `word`, not `byte`.
unsafe fn tbl_line_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(tbl_line).cast::<u16>().add(idx)
}
/// Y coordinate of the *top* of room row `idx`: `{-60, 3, 66, 129, 192}` — the
/// line a sprite is clipped against, as opposed to `y_land` which is the line
/// a character's feet rest on.
///
/// Incomplete extern array — see [`dir_front_at`].
unsafe fn y_clip_at(idx: usize) -> i16 {
    *core::ptr::addr_of!(y_clip).cast::<i16>().add(idx)
}

// ── Const constructors for table types ───────────────────────────────────────

/// Builds one entry of a frame table: sprite `image`, `sword` frame, the
/// sprite's `dx`/`dy` offset, and the `flags` byte (weight offset in the low
/// five bits plus `FRAME_THIN` / `FRAME_NEEDS_FLOOR`).
const fn ft(image: u8, sword: u8, dx: i8, dy: i8, flags: u8) -> frame_type {
    frame_type { image, sword, dx, dy, flags }
}
/// Builds one entry of [`SWORD_TBL`]: sprite `id` and its offset from the
/// wielder's position.
const fn st(id: u8, x: i8, y: i8) -> sword_table_type {
    sword_table_type { id, x, y }
}

// ── DOS overflow-simulation tables for get_tile_div_mod ──────────────────────

/// Tile column for each byte-sized x-position, as the DOS original's
/// `tile_div_tbl`. Kept because out-of-range indexing into it is load-bearing
/// — see [`get_tile_div_mod_impl`].
#[rustfmt::skip]
static TILE_DIV_TBL: [i8; 256] = [
    -5,-5,
    -4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,
    -3,-3,-3,-3,-3,-3,-3,-3,-3,-3,-3,-3,-3,-3,
    -2,-2,-2,-2,-2,-2,-2,-2,-2,-2,-2,-2,-2,-2,
    -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
     1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
     2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
     3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
     4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
     5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
     6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
     7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
     8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
     9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
    10,10,10,10,10,10,10,10,10,10,10,10,10,10,
    11,11,11,11,11,11,11,11,11,11,11,11,11,11,
    12,12,12,12,12,12,12,12,12,12,12,12,12,12,
    13,13,13,13,13,13,13,13,13,13,13,13,13,13,
    14,14,
];

/// Offset within the tile for each byte-sized x-position, as the DOS
/// original's `tile_mod_tbl`. Laid out immediately after [`TILE_DIV_TBL`] in
/// DOS memory, which is why negative indices into it read [`TILE_DIV_TBL`].
#[rustfmt::skip]
static TILE_MOD_TBL: [u8; 256] = [
    12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1,
];

/// The 34 bytes that sat immediately before `tile_div_tbl` in DOS memory.
/// A negative x-position indexed off the front of the table and read these;
/// reproducing them keeps off-screen positions behaving as they did.
static BOGUS_BEFORE: [u8; 34] = [
    0x02,0x00,0x41,0x00,0x80,0x00,0xBF,0x00,0xFE,0x00,0xFF,0x01,0x01,0xFF,
    0xC4,0xFF,0x03,0x00,0x42,0x00,0x81,0x00,0xC0,0x00,0xF8,0xFF,0x37,0x00,
    0x76,0x00,0xB5,0x00,0xF4,0x00,
];

/// The 34 bytes that sat immediately after `tile_mod_tbl` in DOS memory, read
/// by x-positions past the end of the table. See [`BOGUS_BEFORE`].
static BOGUS_AFTER: [u8; 34] = [
    0xF4,0x02,0x10,0x1E,0x2C,0x3A,0x48,0x56,0x64,0x72,0x80,0x8E,0x9C,0xAA,
    0xB8,0xC6,0xD4,0xE2,0xF0,0xFE,0x00,0x0A,0x00,0xFF,0x00,0x00,0x00,0x00,
    0x0A,0x0D,0x00,0x00,0x00,0x00,
];

// ── Frame tables ──────────────────────────────────────────────────────────────

/// Sprite and geometry for every frame of the Kid's (and the mouse's, and most
/// of the shadow's) animation, indexed directly by `Char.frame`.
#[rustfmt::skip]
static FRAME_TABLE_KID: [frame_type; 241] = [
ft(255,0x00| 0,  0,  0,0x00| 0), // 0
ft(  0,0x00| 0,  1,  0,0xC0| 4),
ft(  1,0x00| 0,  1,  0,0x40| 4),
ft(  2,0x00| 0,  3,  0,0x40| 7),
ft(  3,0x00| 0,  4,  0,0x40| 8),
ft(  4,0x00| 0,  0,  0,0xE0| 6),
ft(  5,0x00| 0,  0,  0,0x40| 9),
ft(  6,0x00| 0,  0,  0,0x40|10),
ft(  7,0x00| 0,  0,  0,0xC0| 5),
ft(  8,0x00| 0,  0,  0,0x40| 4),
ft(  9,0x00| 0,  0,  0,0x40| 7), // 10
ft( 10,0x00| 0,  0,  0,0x40|11),
ft( 11,0x00| 0,  0,  0,0x40| 3),
ft( 12,0x00| 0,  0,  0,0xC0| 3),
ft( 13,0x00| 0,  0,  0,0x40| 7),
ft( 14,0x00| 9,  0,  0,0x40| 3),
ft( 15,0x00| 0,  0,  0,0xC0| 3),
ft( 16,0x00| 0,  0,  0,0x40| 4),
ft( 17,0x00| 0,  0,  0,0x40| 6),
ft( 18,0x00| 0,  0,  0,0x40| 8),
ft( 19,0x00| 0,  0,  0,0x80| 9), // 20
ft( 20,0x00| 0,  0,  0,0x00|11),
ft( 21,0x00| 0,  0,  0,0x80|11),
ft( 22,0x00| 0,  0,  0,0x00|17),
ft( 23,0x00| 0,  0,  0,0x00| 7),
ft( 24,0x00| 0,  0,  0,0x00| 5),
ft( 25,0x00| 0,  0,  0,0xC0| 1),
ft( 26,0x00| 0,  0,  0,0xC0| 6),
ft( 27,0x00| 0,  0,  0,0x40| 3),
ft( 28,0x00| 0,  0,  0,0x40| 8),
ft( 29,0x00| 0,  0,  0,0x40| 2), // 30
ft( 30,0x00| 0,  0,  0,0x40| 2),
ft( 31,0x00| 0,  0,  0,0xC0| 2),
ft( 32,0x00| 0,  0,  0,0xC0| 2),
ft( 33,0x00| 0,  0,  0,0x40| 3),
ft( 34,0x00| 0,  0,  0,0x40| 8),
ft( 35,0x00| 0,  0,  0,0xC0|14),
ft( 36,0x00| 0,  0,  0,0xC0| 1),
ft( 37,0x00| 0,  0,  0,0x40| 5),
ft( 38,0x00| 0,  0,  0,0x80|14),
ft( 39,0x00| 0,  0,  0,0x00|11), // 40
ft( 40,0x00| 0,  0,  0,0x80|11),
ft( 41,0x00| 0,  0,  0,0x80|10),
ft( 42,0x00| 0,  0,  0,0x00| 1),
ft( 43,0x00| 0,  0,  0,0xC0| 4),
ft( 44,0x00| 0,  0,  0,0xC0| 3),
ft( 45,0x00| 0,  0,  0,0xC0| 3),
ft( 46,0x00| 0,  0,  0,0xA0| 5),
ft( 47,0x00| 0,  0,  0,0xA0| 4),
ft( 48,0x00| 0,  0,  0,0x60| 6),
ft( 49,0x00| 0,  4,  0,0x60| 7), // 50
ft( 50,0x00| 0,  3,  0,0x60| 6),
ft( 51,0x00| 0,  1,  0,0x40| 4),
ft( 64,0x00| 0,  0,  0,0xC0| 2),
ft( 65,0x00| 0,  0,  0,0x40| 1),
ft( 66,0x00| 0,  0,  0,0x40| 2),
ft( 67,0x00| 0,  0,  0,0x00| 0),
ft( 68,0x00| 0,  0,  0,0x00| 0),
ft( 69,0x00| 0,  0,  0,0x80| 0),
ft( 70,0x00| 0,  0,  0,0x00| 0),
ft( 71,0x00| 0,  0,  0,0x80| 0), // 60
ft( 72,0x00| 0,  0,  0,0x00| 0),
ft( 73,0x00| 0,  0,  0,0x80| 0),
ft( 74,0x00| 0,  0,  0,0x00| 0),
ft( 75,0x00| 0,  0,  0,0x00| 0),
ft( 76,0x00| 0,  0,  0,0x80| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 80,0x00| 0, -2,  0,0x40| 1),
ft( 81,0x00| 0, -2,  0,0x40| 1),
ft( 82,0x00| 0, -1,  0,0xC0| 2),
ft( 83,0x00| 0, -2,  0,0x40| 2), // 70
ft( 84,0x00| 0, -2,  0,0x40| 1),
ft( 85,0x00| 0, -2,  0,0x40| 1),
ft( 86,0x00| 0, -2,  0,0x40| 1),
ft( 87,0x00| 0, -1,  0,0x00| 7),
ft( 88,0x00| 0, -1,  0,0x00| 5),
ft( 89,0x00| 0,  2,  0,0x00| 7),
ft( 90,0x00| 0,  2,  0,0x00| 7),
ft( 91,0x00| 0,  2, -3,0x00| 0),
ft( 92,0x00| 0,  2,-10,0x00| 0),
ft( 93,0x00| 0,  2,-11,0x80| 0), // 80
ft( 94,0x00| 0,  3, -2,0x40| 3),
ft( 95,0x00| 0,  3,  0,0xC0| 3),
ft( 96,0x00| 0,  3,  0,0xC0| 3),
ft( 97,0x00| 0,  3,  0,0x60| 3),
ft( 98,0x00| 0,  4,  0,0xE0| 3),
ft( 28,0x00| 0,  0,  0,0x00| 0),
ft( 99,0x00| 0,  7,-14,0x80| 0),
ft(100,0x00| 0,  7,-12,0x80| 0),
ft(101,0x00| 0,  4,-12,0x00| 0),
ft(102,0x00| 0,  3,-10,0x80| 0), // 90
ft(103,0x00| 0,  2,-10,0x80| 0),
ft(104,0x00| 0,  1,-10,0x80| 0),
ft(105,0x00| 0,  0,-11,0x00| 0),
ft(106,0x00| 0, -1,-12,0x00| 0),
ft(107,0x00| 0, -1,-14,0x00| 0),
ft(108,0x00| 0, -1,-14,0x00| 0),
ft(109,0x00| 0, -1,-15,0x80| 0),
ft(110,0x00| 0, -1,-15,0x80| 0),
ft(111,0x00| 0,  0,-15,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0), // 100
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(112,0x00| 0,  0,  0,0xC0| 6),
ft(113,0x00| 0,  0,  0,0x40| 6),
ft(114,0x00| 0,  0,  0,0xC0| 5),
ft(115,0x00| 0,  0,  0,0x40| 5),
ft(116,0x00| 0,  0,  0,0xC0| 2),
ft(117,0x00| 0,  0,  0,0xC0| 4),
ft(118,0x00| 0,  0,  0,0xC0| 5),
ft(119,0x00| 0,  0,  0,0x40| 6),
ft(120,0x00| 0,  0,  0,0x40| 7), // 110
ft(121,0x00| 0,  0,  0,0x40| 7),
ft(122,0x00| 0,  0,  0,0x40| 9),
ft(123,0x00| 0,  0,  0,0xC0| 8),
ft(124,0x00| 0,  0,  0,0xC0| 9),
ft(125,0x00| 0,  0,  0,0x40| 9),
ft(126,0x00| 0,  0,  0,0x40| 5),
ft(127,0x00| 0,  2,  0,0x40| 5),
ft(128,0x00| 0,  2,  0,0xC0| 5),
ft(129,0x00| 0,  0,  0,0xC0| 3),
ft(255,0x00| 0,  0,  0,0x00| 0), // 120
ft(133,0x00| 0,  0,  0,0x40| 3),
ft(134,0x00| 0,  0,  0,0xC0| 4),
ft(135,0x00| 0,  0,  0,0xC0| 5),
ft(136,0x00| 0,  0,  0,0x40| 8),
ft(137,0x00| 0,  0,  0,0x60|12),
ft(138,0x00| 0,  0,  0,0xE0|15),
ft(139,0x00| 0,  0,  0,0x60| 3),
ft(140,0x00| 0,  0,  0,0xC0| 3),
ft(141,0x00| 0,  0,  0,0x40| 3),
ft(142,0x00| 0,  0,  0,0x40| 3), // 130
ft(143,0x00| 0,  0,  0,0x40| 4),
ft(144,0x00| 0,  0,  0,0x40| 4),
ft(172,0x00| 0,  0,  1,0xC0| 1),
ft(173,0x00| 0,  0,  1,0xC0| 7),
ft(145,0x00| 0,  0,-12,0x00| 1),
ft(146,0x00| 0,  0,-21,0x00| 0),
ft(147,0x00| 0,  1,-26,0x80| 0),
ft(148,0x00| 0,  4,-32,0x80| 0),
ft(149,0x00| 0,  6,-36,0x80| 1),
ft(150,0x00| 0,  7,-41,0x80| 2), // 140
ft(151,0x00| 0,  2, 17,0x40| 2),
ft(152,0x00| 0,  4,  9,0xC0| 4),
ft(153,0x00| 0,  4,  5,0xC0| 9),
ft(154,0x00| 0,  4,  4,0xC0| 8),
ft(155,0x00| 0,  5,  0,0x60| 9),
ft(156,0x00| 0,  5,  0,0xE0| 9),
ft(157,0x00| 0,  5,  0,0xE0| 8),
ft(158,0x00| 0,  5,  0,0x60| 9),
ft(159,0x00| 0,  5,  0,0x60| 9),
ft(184,0x00|16,  0,  2,0x80| 0), // 150
ft(174,0x00|26,  0,  2,0x80| 0),
ft(175,0x00|18,  3,  2,0x00| 0),
ft(176,0x00|22,  7,  2,0xC0| 4),
ft(177,0x00|21, 10,  2,0x00| 0),
ft(178,0x00|23,  7,  2,0x80| 0),
ft(179,0x00|25,  4,  2,0x80| 0),
ft(180,0x00|24,  0,  2,0xC0|14),
ft(181,0x00|15,  0,  2,0xC0|13),
ft(182,0x00|20,  3,  2,0x00| 0),
ft(183,0x00|31,  3,  2,0x00| 0), // 160
ft(184,0x00|16,  0,  2,0x80| 0),
ft(185,0x00|17,  0,  2,0x80| 0),
ft(186,0x00|32,  0,  2,0x00| 0),
ft(187,0x00|33,  0,  2,0x80| 0),
ft(188,0x00|34,  2,  2,0xC0| 3),
ft( 14,0x00| 0,  0,  0,0x40| 3),
ft(189,0x00|19,  7,  2,0x80| 0),
ft(190,0x00|14,  1,  2,0x80| 0),
ft(191,0x00|27,  0,  2,0x80| 0),
ft(181,0x00|15,  0,  2,0xC0|13), // 170
ft(181,0x00|15,  0,  2,0xC0|13),
ft(112,0x00|43,  0,  0,0xC0| 6), // 172
ft(113,0x00|44,  0,  0,0x40| 6),
ft(114,0x00|45,  0,  0,0xC0| 5),
ft(115,0x00|46,  0,  0,0x40| 5),
ft(114,0x00| 0,  0,  0,0xC0| 5),
ft( 78,0x00| 0,  0,  3,0x80|10),
ft( 77,0x00| 0,  4,  3,0x80| 7),
ft(211,0x00| 0,  0,  1,0x40| 4),
ft(212,0x00| 0,  0,  1,0x40| 4),
ft(213,0x00| 0,  0,  1,0x40| 4), // 181
ft(214,0x00| 0,  0,  1,0x40| 7),
ft(215,0x00| 0,  0,  7,0x40|11),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 79,0x00| 0,  4,  7,0x40| 9),
ft(130,0x00| 0,  0,  0,0x40| 4),
ft(131,0x00| 0,  0,  0,0x40| 4),
ft(132,0x00| 0,  0,  2,0x40| 4),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(192,0x00| 0,  0,  0,0x00| 0), // 191
ft(193,0x00| 0,  0,  1,0x00| 0),
ft(194,0x00| 0,  0,  0,0x80| 0),
ft(195,0x00| 0,  0,  0,0x00| 0),
ft(196,0x00| 0, -1,  0,0x00| 0),
ft(197,0x00| 0, -1,  0,0x00| 0),
ft(198,0x00| 0, -1,  0,0x00| 0),
ft(199,0x00| 0, -4,  0,0x00| 0),
ft(200,0x00| 0, -4,  0,0x80| 0),
ft(201,0x00| 0, -4,  0,0x00| 0),
ft(202,0x00| 0, -4,  0,0x00| 0), // 201
ft(203,0x00| 0, -4,  0,0x00| 0),
ft(204,0x00| 0, -4,  0,0x00| 0),
ft(205,0x00| 0, -5,  0,0x00| 0),
ft(206,0x00| 0, -5,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(207,0x00| 0,  0,  1,0x40| 6),
ft(208,0x00| 0,  0,  1,0xC0| 6),
ft(209,0x00| 0,  0,  1,0xC0| 8),
ft(210,0x00| 0,  0,  1,0x40|10),
ft(255,0x00| 0,  0,  0,0x00| 0), // 211
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 52,0x00| 0,  0,  0,0x80| 0),
ft( 53,0x00| 0,  0,  0,0x00| 0),
ft( 54,0x00| 0,  0,  0,0x00| 0),
ft( 55,0x00| 0,  0,  0,0x00| 0),
ft( 56,0x00| 0,  0,  0,0x80| 0), // 221
ft( 57,0x00| 0,  0,  0,0x00| 0),
ft( 58,0x00| 0,  0,  0,0x00| 0),
ft( 59,0x00| 0,  0,  0,0x00| 0),
ft( 60,0x00| 0,  0,  0,0x80| 0),
ft( 61,0x00| 0,  0,  0,0x00| 0),
ft( 62,0x00| 0,  0,  0,0x80| 0),
ft( 63,0x00| 0,  0,  0,0x00| 0),
ft(160,0x00|35,  1,  1,0xC0| 3),
ft(161,0x00|36,  0,  1,0x40| 9),
ft(162,0x00|37,  0,  1,0xC0| 3), // 231
ft(163,0x00|38,  0,  1,0x40| 9),
ft(164,0x00|39,  0,  1,0xC0| 3),
ft(165,0x00|40,  1,  1,0x40| 9),
ft(166,0x00|41,  1,  1,0x40| 3),
ft(167,0x00|42,  1,  1,0xC0| 9),
ft(168,0x00| 0,  4,  1,0xC0| 6),
ft(169,0x00| 0,  3,  1,0xC0|10),
ft(170,0x00| 0,  1,  1,0x40| 3),
ft(171,0x00| 0,  1,  1,0xC0| 8), // 240
];

/// Frames for guards and the skeleton, indexed by `Char.frame - 149` — the
/// guard sprites start where the Kid's sword frames leave off.
#[rustfmt::skip]
static FRAME_TBL_GUARD: [frame_type; 41] = [
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 12,0xC0|13,  2,  1,0x00| 0),
ft(  2,0xC0| 1,  3,  1,0x00| 0),
ft(  3,0xC0| 2,  4,  1,0x00| 0),
ft(  4,0xC0| 3,  7,  1,0x40| 4),
ft(  5,0xC0| 4, 10,  1,0x00| 0),
ft(  6,0xC0| 5,  7,  1,0x80| 0),
ft(  7,0xC0| 6,  4,  1,0x80| 0),
ft(  8,0xC0| 7,  0,  1,0x80| 0),
ft(  9,0xC0| 8,  0,  1,0xC0|13),
ft( 10,0xC0|11,  7,  1,0x80| 0),
ft( 11,0xC0|12,  3,  1,0x00| 0),
ft( 12,0xC0|13,  2,  1,0x00| 0),
ft( 13,0xC0| 0,  2,  1,0x00| 0),
ft( 14,0xC0|28,  0,  1,0x00| 0),
ft( 15,0xC0|29,  0,  1,0x80| 0),
ft( 16,0xC0|30,  2,  1,0xC0| 3),
ft( 17,0xC0| 9, -1,  1,0x40| 8),
ft( 18,0xC0|10,  7,  1,0x80| 0),
ft( 19,0xC0|14,  3,  1,0x80| 0),
ft(  9,0xC0| 8,  0,  1,0x80| 0),
ft( 20,0xC0| 8,  0,  1,0xC0|13),
ft( 21,0xC0| 8,  0,  1,0xC0|13),
ft( 22,0xC0|47,  0,  0,0xC0| 6),
ft( 23,0xC0|48,  0,  0,0x40| 6),
ft( 24,0xC0|49,  0,  0,0xC0| 5),
ft( 24,0xC0|49,  0,  0,0xC0| 5),
ft( 24,0xC0|49,  0,  0,0xC0| 5),
ft( 26,0xC0| 0,  0,  3,0x80|10),
ft( 27,0xC0| 0,  4,  4,0x80| 7),
ft( 28,0xC0| 0, -2,  1,0x40| 4),
ft( 29,0xC0| 0, -2,  1,0x40| 4),
ft( 30,0xC0| 0, -2,  1,0x40| 4),
ft( 31,0xC0| 0, -2,  2,0x40| 7),
ft( 32,0xC0| 0, -2,  2,0x40|10),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 33,0xC0| 0,  3,  4,0xC0| 9),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
];

/// Frames for the cutscene-only characters: the princess and Jaffar.
#[rustfmt::skip]
static FRAME_TBL_CUTS: [frame_type; 86] = [
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 15,0x40| 0,  0,  0,0x00| 0),
ft(  1,0x40| 0,  0,  0,0x80| 0),
ft(  2,0x40| 0,  0,  0,0x80| 0),
ft(  3,0x40| 0,  0,  0,0x80| 0),
ft(  4,0x40| 0, -1,  0,0x00| 0),
ft(  5,0x40| 0,  2,  0,0x80| 0),
ft(  6,0x40| 0,  2,  0,0x00| 0),
ft(  7,0x40| 0,  0,  0,0x80| 0),
ft(  8,0x40| 0,  1,  0,0x80| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(  0,0x40| 0,  0,  0,0x80| 0),
ft(  9,0x40| 0,  0,  0,0x80| 0),
ft( 10,0x40| 0,  0,  0,0x00| 0),
ft( 11,0x40| 0,  0,  0,0x80| 0),
ft( 12,0x40| 0,  0,  0,0x80| 0),
ft( 13,0x40| 0,  0,  0,0x80| 0),
ft( 14,0x40| 0,  0,  0,0x00| 0),
ft( 16,0x40| 0,  0,  0,0x00| 0),
ft(  0,0x80| 0,  0,  0,0x00| 0),
ft(  2,0x80| 0,  0,  0,0x00| 0),
ft(  3,0x80| 0,  0,  0,0x00| 0),
ft(  4,0x80| 0,  0,  0,0x80| 0),
ft(  5,0x80| 0,  0,  0,0x00| 0),
ft(  6,0x80| 0,  0,  0,0x80| 0),
ft(  7,0x80| 0,  0,  0,0x80| 0),
ft(  8,0x80| 0,  0,  0,0x00| 0),
ft(  9,0x80| 0,  0,  0,0x00| 0),
ft( 10,0x80| 0,  0,  0,0x00| 0),
ft( 11,0x80| 0,  0,  0,0x00| 0),
ft( 12,0x80| 0,  0,  0,0x00| 0),
ft( 13,0x80| 0,  0,  0,0x00| 0),
ft( 14,0x80| 0,  0,  0,0x00| 0),
ft( 15,0x80| 0,  0,  0,0x00| 0),
ft( 16,0x80| 0,  0,  0,0x00| 0),
ft( 17,0x80| 0,  0,  0,0x00| 0),
ft( 18,0x80| 0,  0,  0,0x00| 0),
ft( 19,0x80| 0,  0,  0,0x00| 0),
ft( 20,0x80| 0,  0,  0,0x80| 0),
ft( 21,0x80| 0,  0,  0,0x80| 0),
ft( 22,0x80| 0,  1,  0,0x00| 0),
ft( 23,0x80| 0, -1,  0,0x00| 0),
ft( 24,0x80| 0,  2,  0,0x00| 0),
ft( 25,0x80| 0,  1,  0,0x80| 0),
ft( 26,0x80| 0,  0,  0,0x80| 0),
ft( 27,0x80| 0,  0,  0,0x80| 0),
ft( 28,0x80| 0,  0,  0,0x80| 0),
ft( 29,0x80| 0, -1,  0,0x00| 0),
ft(  0,0x80| 0,  0,  0,0x80| 0),
ft(  1,0x80| 0,  0,  0,0x80| 0),
ft(  2,0x80| 0,  0,  0,0x80| 0),
ft(  3,0x80| 0,  0,  0,0x00| 0),
ft(  4,0x80| 0,  0,  0,0x00| 0),
ft(  5,0x80| 0,  0,  0,0x80| 0),
ft(  6,0x80| 0,  0,  0,0x80| 0),
ft(  7,0x80| 0,  0,  0,0x80| 0),
ft(  8,0x80| 0,  0,  0,0x80| 0),
ft(  9,0x80| 0,  0,  0,0x80| 0),
ft( 10,0x80| 0,  0,  0,0x80| 0),
ft( 11,0x80| 0,  0,  0,0x80| 0),
ft( 12,0x80| 0,  0,  0,0x80| 0),
ft( 13,0x80| 0,  0,  0,0x00| 0),
ft( 14,0x80| 0,  0,  0,0x80| 0),
ft( 15,0x80| 0,  0,  0,0x00| 0),
ft( 16,0x80| 0,  0,  0,0x00| 0),
ft( 17,0x80| 0,  0,  0,0x80| 0),
ft( 18,0x80| 0,  0,  0,0x00| 0),
ft( 19,0x80| 0,  3,  0,0x00| 0),
ft( 20,0x80| 0,  3,  0,0x00| 0),
ft( 21,0x80| 0,  3,  0,0x00| 0),
ft( 22,0x80| 0,  2,  0,0x00| 0),
ft( 23,0x80| 0,  3,  0,0x80| 0),
ft( 24,0x80| 0,  5,  0,0x00| 0),
ft( 25,0x80| 0,  5,  0,0x00| 0),
ft( 26,0x80| 0,  1,  0,0x80| 0),
ft( 27,0x80| 0,  2,  0,0x80| 0),
ft( 28,0x80| 0,  2,  0,0x80| 0),
ft( 29,0x80| 0,  1,  0,0x80| 0),
ft( 30,0x80| 0,  1,  0,0x00| 0),
ft( 31,0x80| 0,  2,  0,0x00| 0),
ft( 32,0x80| 0,  3,  0,0x00| 0),
ft( 33,0x80| 0,  3,  0,0x00| 0),
ft( 34,0x80| 0,  0,  0,0x80| 0),
ft( 35,0x80| 0,  2,  0,0x80| 0),
ft( 36,0x80| 0,  2,  0,0x80| 0),
ft( 37,0x80| 0,  1,  0,0x00| 0),
];

/// Where to draw the sword sprite for each sword pose, indexed by the low six
/// bits of a frame's `sword` field. Entry 0 means "no sword this frame" and an
/// `id` of 255 means "this pose has no separate sword sprite".
#[rustfmt::skip]
static SWORD_TBL: [sword_table_type; 51] = [
st(255,   0,   0),
st(  0,   0,  -9),
st(  5,  -9, -29),
st(  1,   7, -25),
st(  2,  17, -26),
st(  6,   7, -14),
st(  7,   0,  -5),
st(  3,  17, -16),
st(  4,  16, -19),
st( 30,  12,  -9),
st(  8,  13, -34),
st(  9,   7, -25),
st( 10,  10, -16),
st( 11,  10, -11),
st( 12,  22, -21),
st( 13,  28, -23),
st( 14,  13, -35),
st( 15,   0, -38),
st( 16,   0, -29),
st( 17,  21, -19),
st( 18,  14, -23),
st( 19,  21, -22),
st( 19,  22, -23),
st( 17,   7, -13),
st( 17,  15, -18),
st(  7,   0,  -8),
st(  1,   7, -27),
st( 28,  14, -28),
st(  8,   7, -27),
st(  4,   6, -23),
st(  4,   9, -21),
st( 10,  11, -18),
st( 13,  24, -23),
st( 13,  19, -23),
st( 13,  21, -23),
st( 20,   7, -32),
st( 21,  14, -32),
st( 22,  14, -31),
st( 23,  14, -29),
st( 24,  28, -28),
st( 25,  28, -28),
st( 26,  21, -25),
st( 27,  14, -22),
st(255,  14, -25),
st(255,  21, -25),
st( 29,   0, -16),
st(  8,   8, -37),
st( 31,  14, -24),
st( 32,  14, -24),
st( 33,   7, -14),
st(  8,   8, -37),
];

// ── obj2 state ────────────────────────────────────────────────────────────────

// A one-slot save area for the `obj_*` drawing globals, so a routine that
// needs to draw an extra sprite (a splash of blood, a dropped sword) can
// borrow them and put them back. File-local in seg006.c, so not in bindings.rs.
// See [`save_obj_impl`] / [`load_obj_impl`].

static mut obj2_tilepos:    u8  = 0;
static mut obj2_x:          u16 = 0;
static mut obj2_y:          u8  = 0;
static mut obj2_direction:  i8  = 0;
static mut obj2_id:         u8  = 0;
static mut obj2_chtab:      u8  = 0;
static mut obj2_clip_top:   i16 = 0;
static mut obj2_clip_bottom:i16 = 0;
static mut obj2_clip_left:  i16 = 0;
static mut obj2_clip_right: i16 = 0;

// ── Functions (ported from seg006.c) ─────────────────────────────────────────

/// Looks up the tile at (`col`, `row`) of `room`, following room links if the
/// coordinate points outside the room.
///
/// The return value is the tile type, but callers just as often want the
/// side effects: `curr_room`, `tile_col` and `tile_row` end up holding the
/// coordinate *after* it has been resolved into a real room, `curr_tilepos` is
/// the flat 0..29 index within that room, and `curr_tile2` is the tile type
/// (the low five bits of the stored byte — the top three are the modifier).
///
/// A coordinate that resolves to no room at all (falling off the level, or the
/// `room = -1` that `check_chomped_kid` can pass) reads as
/// `custom->level_edge_hit_tile`, normally a wall.
unsafe fn get_tile_impl(state: &mut State, room: c_int, col: c_int, row: c_int) -> c_int {
    *state.curr_room() = room as i16;
    *state.tile_col()  = col as i16;
    *state.tile_row()  = row as i16;
    *state.curr_room() = find_room_of_tile_impl(state) as i16;
    if *state.curr_room() > 0 {
        let cr = *state.curr_room();
        get_room_address(cr as c_int);
        let tr = *state.tile_row();
        let tc = *state.tile_col();
        *state.curr_tilepos() = (tbl_line_at(tr as usize) as i32 + tc as i32) as u8;
        let ctp = *state.curr_tilepos();
        *state.curr_tile2()   = *curr_room_tiles.add(ctp as usize) & 0x1F;
    } else {
        *state.curr_tile2() = (*custom).level_edge_hit_tile;
    }
    *state.curr_tile2() as c_int
}

/// Looks up the tile at (`col`, `row`) of `room`. See [`get_tile_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_tile(room: c_int, col: c_int, row: c_int) -> c_int {
    get_tile_impl(&mut State, room, col, row)
}

/// Normalises `tile_col` / `tile_row` into the room they actually belong to,
/// and returns that room.
///
/// A coordinate one step outside `curr_room` is brought back inside by adding
/// or subtracting a room's worth of columns (10) or rows (3) and stepping to
/// the linked neighbour. This repeats — the C source loops with `goto again` —
/// so a coordinate two rooms away resolves in two passes. Stepping off the map
/// yields room 0, and from then on every further step stays at 0.
///
/// The row check comes first (`FIX_CORNER_GRAB`): that is what lets the Prince
/// grab a ledge at the bottom-right corner of a room with no room below it.
unsafe fn find_room_of_tile_impl(state: &mut State) -> c_int {
    // One "step to the neighbouring room" as written four times in the C
    // source: `if (curr_room) curr_room = level.roomlinks[curr_room - 1].<dir>;`
    macro_rules! step_to_neighbour {
        ($link:ident) => {{
            let cr = *state.curr_room();
            let neighbour = if cr > 0 {
                state.level().roomlinks[(cr - 1) as usize].$link as i16
            } else {
                0
            };
            *state.curr_room() = neighbour;
        }};
    }
    loop {
        if *state.tile_row() < 0 {
            *state.tile_row() += 3;
            step_to_neighbour!(up);
        } else if *state.tile_col() < 0 {
            *state.tile_col() += 10;
            step_to_neighbour!(left);
        } else if *state.tile_col() >= 10 {
            *state.tile_col() -= 10;
            step_to_neighbour!(right);
        } else if *state.tile_row() >= 3 {
            *state.tile_row() -= 3;
            step_to_neighbour!(down);
        } else {
            return *state.curr_room() as c_int;
        }
    }
}

/// Normalises `tile_col` / `tile_row` into a real room and returns it.
#[no_mangle]
pub unsafe extern "C" fn find_room_of_tile() -> c_int {
    find_room_of_tile_impl(&mut State)
}

/// Flattens a (column, row) within one room into a 0..29 tile index.
///
/// A row above the room returns the negative sentinel `-(col + 1)`; anything
/// else out of bounds returns 30, one past the last tile, which the rest of
/// the game treats as "nowhere".
#[no_mangle]
pub unsafe extern "C" fn get_tilepos(tcol: c_int, trow: c_int) -> c_int {
    if trow < 0 {
        -(tcol + 1)
    } else if trow >= 3 || tcol >= 10 || tcol < 0 {
        30
    } else {
        tbl_line_at(trow as usize) as c_int + tcol
    }
}

/// Like [`get_tilepos`], but folds the "above the room" sentinel into the
/// plain out-of-bounds answer 30.
#[no_mangle]
pub unsafe extern "C" fn get_tilepos_nominus(tcol: c_int, trow: c_int) -> c_int {
    let tp = get_tilepos(tcol, trow);
    if tp < 0 { 30 } else { tp }
}

/// Loads the current character's frame and recomputes which column he is in.
///
/// Called after anything that moves a character, since both his sprite and the
/// tile he counts as occupying depend on the frame.
unsafe fn load_fram_det_col_impl(state: &mut State) {
    load_frame_impl(state);
    determine_col_impl(state);
}

/// Loads the current character's frame and recomputes his column.
#[no_mangle]
pub unsafe extern "C" fn load_fram_det_col() {
    load_fram_det_col_impl(&mut State);
}

/// Sets `Char.curr_col` to the column the character's centre of gravity is in.
///
/// It is the *weighted* x ([`dx_weight_impl`]), not the sprite's x, that
/// decides which tile a character stands on — which is why he can lean over a
/// ledge without falling.
unsafe fn determine_col_impl(state: &mut State) {
    let dxw = dx_weight_impl(state);
    state.Char().curr_col = get_tile_div_mod_m7(dxw) as i8;
}

/// Recomputes the current character's column from his weighted x-position.
#[no_mangle]
pub unsafe extern "C" fn determine_col() {
    determine_col_impl(&mut State);
}

/// Copies `frame_table[frame]` into `cur_frame`, or a blank frame (image 255,
/// no offsets, no flags) if the index is out of range.
unsafe fn get_frame_internal_impl(state: &mut State, frame_table: &[frame_type], frame: c_int) {
    if frame >= 0 && frame < frame_table.len() as c_int {
        *state.cur_frame() = frame_table[frame as usize];
    } else {
        *state.cur_frame() = frame_type { image: 255, sword: 0, dx: 0, dy: 0, flags: 0 };
    }
}

/// Fills `cur_frame` from the frame table that matches the current character.
///
/// The Kid, the mouse and (outside its sword frames) the shadow share
/// [`FRAME_TABLE_KID`]; guards and the skeleton use [`FRAME_TBL_GUARD`] with
/// the index biased by -149; the princess and Jaffar use [`FRAME_TBL_CUTS`].
/// Frames 102..106 of a guard are the Kid's fall frames re-used, hence the
/// `add_frame` of 70 that maps them onto the guard table's own fall frames.
unsafe fn load_frame_impl(state: &mut State) {
    let frame = state.Char().frame as c_int;
    // The C source shares one label between the guard case (which may set
    // add_frame) and the shadow's sword case (which cannot), so add_frame is
    // always 0 on the shadow path.
    let mut add_frame: c_int = 0;
    match state.Char().charid {
        c if c == charids_charid_0_kid as u8 || c == charids_charid_24_mouse as u8 => {
            get_frame_internal_impl(state, &FRAME_TABLE_KID, frame);
        }
        c if c == charids_charid_2_guard as u8 || c == charids_charid_4_skeleton as u8 => {
            if (102..107).contains(&frame) { add_frame = 70; }
            get_frame_internal_impl(state, &FRAME_TBL_GUARD, frame + add_frame - 149);
        }
        c if c == charids_charid_1_shadow as u8 => {
            if frame < 150 || frame >= 190 {
                get_frame_internal_impl(state, &FRAME_TABLE_KID, frame);
            } else {
                get_frame_internal_impl(state, &FRAME_TBL_GUARD, frame + add_frame - 149);
            }
        }
        c if c == charids_charid_5_princess as u8 || c == charids_charid_6_vizier as u8 => {
            get_frame_internal_impl(state, &FRAME_TBL_CUTS, frame);
        }
        _ => {}
    }
}

/// Fills `cur_frame` for the current character's current frame.
#[no_mangle]
pub unsafe extern "C" fn load_frame() {
    load_frame_impl(&mut State);
}

/// The x-position of the current character's centre of gravity.
///
/// The frame's `dx` places the sprite; subtracting the weight offset stored in
/// the low five bits of its flags moves the reference point back to where the
/// character's weight actually is. [`determine_col_impl`] turns this into his
/// column, so a pose that leans far forward still stands on the tile behind it.
unsafe fn dx_weight_impl(state: &mut State) -> c_int {
    let offset = state.cur_frame().dx as i32 - (state.cur_frame().flags & frame_flags_FRAME_WEIGHT_X as u8) as i32;
    char_dx_forward_impl(state, offset)
}

/// The x-position of the current character's centre of gravity.
#[no_mangle]
pub unsafe extern "C" fn dx_weight() -> c_int {
    dx_weight_impl(&mut State)
}

/// `Char.x` moved `delta_x` units in the character's *facing* direction —
/// negated when he faces left. Does not store the result.
unsafe fn char_dx_forward_impl(state: &mut State, mut delta_x: c_int) -> c_int {
    if (state.Char().direction as i32) < directions_dir_0_right as i32 {
        delta_x = -delta_x;
    }
    delta_x + state.Char().x as i32
}

/// `Char.x` moved `delta_x` units in the character's facing direction.
#[no_mangle]
pub unsafe extern "C" fn char_dx_forward(delta_x: c_int) -> c_int {
    char_dx_forward_impl(&mut State, delta_x)
}

/// Moves the sprite being drawn (`obj_x`) `delta_x` units in its facing
/// direction, stores it, and returns the new value. Unlike
/// [`char_dx_forward_impl`], this one writes.
unsafe fn obj_dx_forward_impl(state: &mut State, mut delta_x: c_int) -> c_int {
    if (*state.obj_direction() as i32) < directions_dir_0_right as i32 {
        delta_x = -delta_x;
    }
    *state.obj_x() = (*state.obj_x() as i32 + delta_x) as i16;
    *state.obj_x() as c_int
}

/// Moves the sprite being drawn `delta_x` units in its facing direction.
#[no_mangle]
pub unsafe extern "C" fn obj_dx_forward(delta_x: c_int) -> c_int {
    obj_dx_forward_impl(&mut State, delta_x)
}

/// Runs the current character's animation program until it produces a frame.
///
/// `Char.curr_seq` is a program counter into the bytecode in `seqtbl.c`. Every
/// byte from 0xF1 upwards is one of the `SEQ_*` opcodes below; anything else is
/// a frame number, which is stored in `Char.frame` and ends the call. So one
/// call executes all the movement, sound and bookkeeping opcodes that lead up
/// to the next drawn frame, and stops there.
unsafe fn play_seq_impl(state: &mut State) {
    loop {
        let command = seq_next(state);
        match command {
            SEQ_DX => {
                let val = seq_next(state) as i32;
                let dx = char_dx_forward_impl(state, val);
                state.Char().x = dx as u8;
            }
            SEQ_DY => {
                let dy = seq_next(state);
                state.Char().y = state.Char().y.wrapping_add(dy);
            }
            SEQ_FLIP => {
                // C's `~Char.direction` on an sbyte; Rust's `!` on i8 is the
                // same bitwise complement (dir_0_right 0 <-> dir_FF_left -1).
                state.Char().direction = !state.Char().direction;
            }
            SEQ_JMP_IF_FEATHER => {
                if *state.is_feather_fall() == 0 {
                    // Skip the two-byte target. The C source falls through into
                    // SEQ_JMP when feather fall *is* active, hence seq_jump.
                    state.Char().curr_seq = state.Char().curr_seq.wrapping_add(2);
                } else {
                    seq_jump(state);
                }
            }
            SEQ_JMP => {
                seq_jump(state);
            }
            SEQ_UP => {
                state.Char().curr_row -= 1;
                start_chompers();
            }
            SEQ_DOWN => {
                inc_curr_row_impl(state);
                start_chompers();
            }
            SEQ_ACTION => {
                state.Char().action = seq_next(state);
            }
            SEQ_SET_FALL => {
                state.Char().fall_x = seq_next(state) as i8;
                state.Char().fall_y = seq_next(state) as i8;
            }
            SEQ_KNOCK_UP => {
                *state.knock() = 1;
            }
            SEQ_KNOCK_DOWN => {
                *state.knock() = -1;
            }
            SEQ_SOUND => {
                let which_sound = seq_next(state);
                match which_sound {
                    SND_SILENT => {
                        *state.is_guard_notice() = 1;
                    }
                    SND_FOOTSTEP => {
                        play_sound(soundids_sound_23_footstep as c_int);
                        *state.is_guard_notice() = 1;
                    }
                    SND_BUMP => {
                        play_sound(soundids_sound_8_bumped as c_int);
                        *state.is_guard_notice() = 1;
                    }
                    SND_DRINK => {
                        play_sound(soundids_sound_18_drink as c_int);
                    }
                    SND_LEVEL => {
                        // USE_REPLAY: don't do end level music in replays
                        if recording != 0 || replaying != 0 { /* skip */ }
                        else if is_sound_on != 0 {
                            if *state.current_level() == (*custom).mirror_level as u16 {
                                play_sound(soundids_sound_32_shadow_music as c_int);
                            } else if *state.current_level() != 13 && *state.current_level() != 15 {
                                play_sound(soundids_sound_41_end_level_music as c_int);
                            }
                        }
                    }
                    _ => {}
                }
            }
            SEQ_END_LEVEL => {
                *state.next_level() += 1;
                // USE_REPLAY
                *state.keep_last_seed() = 1;
                if replaying != 0 && skipping_replay != 0 { stop_sounds(); }
            }
            SEQ_GET_ITEM => {
                let which_item = seq_next(state) as c_int;
                if which_item == 1 {
                    proc_get_object_impl(state);
                }
                // USE_TELEPORTS
                if which_item == 2 {
                    teleport();
                }
            }
            SEQ_DIE => { /* nop */ }
            _ => {
                state.Char().frame = command;
                return;
            }
        }
    }
}

/// Runs the current character's animation program up to the next frame.
#[no_mangle]
pub unsafe extern "C" fn play_seq() {
    play_seq_impl(&mut State);
}

/// [`get_tile_div_mod`] measured from the middle of the character rather than
/// his left edge — the form almost every caller wants.
#[no_mangle]
pub unsafe extern "C" fn get_tile_div_mod_m7(xpos: c_int) -> c_int {
    get_tile_div_mod(xpos - 7)
}

/// Splits a character x-position into a tile column (returned) and the offset
/// within that tile, 0..13, left in `obj_xl`.
///
/// Columns are 14 units wide starting at `SCREENSPACE_X`; negative positions
/// round *down* to the next column rather than towards zero, so column -1 is
/// the tile off the left edge of the room.
///
/// The DOS original did this with a pair of 256-byte lookup tables indexed by
/// the raw x, and the game genuinely asks about positions outside 0..255. Those
/// reads ran off the ends of the tables into whatever was next to them in
/// memory, so for positions outside that range the table results are replaced
/// with the bytes recorded in [`BOGUS_BEFORE`] / [`BOGUS_AFTER`] (and, for the
/// negative side, [`TILE_DIV_TBL`] itself, which is what sat before
/// [`TILE_MOD_TBL`]).
unsafe fn get_tile_div_mod_impl(state: &mut State, xpos: c_int) -> c_int {
    let x = xpos - SCREENSPACE_X;
    let mut xl = x % TILE_SIZEX;
    let mut xh = x / TILE_SIZEX;
    if xl < 0 {
        xh -= 1;
        xl += TILE_SIZEX;
    }
    if xpos < 0 {
        let bogus_len = BOGUS_BEFORE.len() as i32;
        if bogus_len + xpos >= 0 {
            xh = BOGUS_BEFORE[(bogus_len + xpos) as usize] as i32;
            xl = TILE_DIV_TBL[(256 + xpos) as usize] as i32;
        }
    }
    let tbl_size: i32 = 256;
    if xpos >= tbl_size {
        let off = (xpos - tbl_size) as usize;
        if off < BOGUS_AFTER.len() {
            xh = TILE_MOD_TBL[(xpos - tbl_size) as usize] as i32;
            xl = BOGUS_AFTER[off] as i32;
        }
    }
    *state.obj_xl() = xl as u8;
    xh
}

/// Splits a character x-position into a tile column and a within-tile offset.
#[no_mangle]
pub unsafe extern "C" fn get_tile_div_mod(xpos: c_int) -> c_int {
    get_tile_div_mod_impl(&mut State, xpos)
}

/// Turns a pixel y-position into a room row: 0, 1 or 2, or -1 for a position
/// above the room. Rows are 63 pixels tall and the first floor line is at 55.
#[no_mangle]
pub unsafe extern "C" fn y_to_row_mod4(ypos: c_int) -> c_int {
    (ypos + 60) / TILE_SIZEY % 4 - 1
}

// The load*/save* pairs swap whole `char_type` records in and out of `Char`,
// the single "character being processed this frame" slot that every function
// in this module works on. `Opp` is the same idea for whoever the current
// character is fighting.

/// Makes the Kid the character being processed.
unsafe fn loadkid_impl(state: &mut State) {
    *state.Char() = *state.Kid();
}

/// Makes the Kid the character being processed.
#[no_mangle]
pub unsafe extern "C" fn loadkid() {
    loadkid_impl(&mut State);
}

/// Writes the character being processed back to the Kid.
unsafe fn savekid_impl(state: &mut State) {
    *state.Kid() = *state.Char();
}

/// Writes the character being processed back to the Kid.
#[no_mangle]
pub unsafe extern "C" fn savekid() {
    savekid_impl(&mut State);
}

/// Makes the guard (or shadow) the character being processed.
unsafe fn loadshad_impl(state: &mut State) {
    *state.Char() = *state.Guard();
}

/// Makes the guard the character being processed.
#[no_mangle]
pub unsafe extern "C" fn loadshad() {
    loadshad_impl(&mut State);
}

/// Writes the character being processed back to the guard.
unsafe fn saveshad_impl(state: &mut State) {
    *state.Guard() = *state.Char();
}

/// Writes the character being processed back to the guard.
#[no_mangle]
pub unsafe extern "C" fn saveshad() {
    saveshad_impl(&mut State);
}

/// Makes the Kid current, with the guard as his opponent.
unsafe fn loadkid_and_opp_impl(state: &mut State) {
    loadkid_impl(state);
    *state.Opp() = *state.Guard();
}

/// Makes the Kid current, with the guard as his opponent.
#[no_mangle]
pub unsafe extern "C" fn loadkid_and_opp() {
    loadkid_and_opp_impl(&mut State);
}

/// Writes the Kid and his opponent back to the Kid and the guard.
unsafe fn savekid_and_opp_impl(state: &mut State) {
    savekid_impl(state);
    *state.Guard() = *state.Opp();
}

/// Writes the Kid and his opponent back out.
#[no_mangle]
pub unsafe extern "C" fn savekid_and_opp() {
    savekid_and_opp_impl(&mut State);
}

/// Makes the guard current, with the Kid as his opponent.
unsafe fn loadshad_and_opp_impl(state: &mut State) {
    loadshad_impl(state);
    *state.Opp() = *state.Kid();
}

/// Makes the guard current, with the Kid as his opponent.
#[no_mangle]
pub unsafe extern "C" fn loadshad_and_opp() {
    loadshad_and_opp_impl(&mut State);
}

/// Writes the guard and his opponent back to the guard and the Kid.
unsafe fn saveshad_and_opp_impl(state: &mut State) {
    saveshad_impl(state);
    *state.Kid() = *state.Opp();
}

/// Writes the guard and his opponent back out.
#[no_mangle]
pub unsafe extern "C" fn saveshad_and_opp() {
    saveshad_and_opp_impl(&mut State);
}

/// Opens the sprite clipping rectangle back up to the whole play field, undoing
/// whatever [`clip_char_impl`] last narrowed it to.
unsafe fn reset_obj_clip_impl(state: &mut State) {
    *state.obj_clip_left()   = 0;
    *state.obj_clip_top()    = 0;
    *state.obj_clip_right()  = 320;
    *state.obj_clip_bottom() = 192;
}

/// Opens the sprite clipping rectangle back up to the whole play field.
#[no_mangle]
pub unsafe extern "C" fn reset_obj_clip() {
    reset_obj_clip_impl(&mut State);
}

/// Splits a sprite x-position into a byte-aligned part and a 0..7 shift, the
/// form the blitter wants. `FIX_SPRITE_XPOS` is on, so this is a plain
/// arithmetic shift and mask rather than the original's sign-dependent version.
///
/// # Safety
/// `xh_addr` and `xl_addr` must be valid, writable pointers.
#[no_mangle]
pub unsafe extern "C" fn x_to_xh_and_xl(xpos: c_int, xh_addr: *mut i8, xl_addr: *mut i8) {
    // FIX_SPRITE_XPOS active
    *xh_addr = (xpos >> 3) as i8;
    *xl_addr = (xpos & 7) as i8;
}

/// Applies gravity for one frame: speeds a free-falling character up towards
/// terminal velocity.
///
/// A feather potion swaps in the much gentler
/// [`FALLING_SPEED_ACCEL_FEATHER`] / [`FALLING_SPEED_MAX_FEATHER`] pair.
/// With `fix_feather_fall_affects_guards` on, that only applies to the Kid —
/// unfixed, a guard falling while the Kid is floating floats too.
unsafe fn fall_accel_impl(state: &mut State) {
    if state.Char().action == actions_actions_4_in_freefall as u8 {
        if *state.is_feather_fall() != 0
            // FIX_FEATHER_FALL_AFFECTS_GUARDS: only kid affected
            && ((*fixes).fix_feather_fall_affects_guards == 0 || state.Char().charid == charids_charid_0_kid as u8)
        {
            state.Char().fall_y += FALLING_SPEED_ACCEL_FEATHER;
            if state.Char().fall_y > FALLING_SPEED_MAX_FEATHER {
                state.Char().fall_y = FALLING_SPEED_MAX_FEATHER;
            }
        } else {
            state.Char().fall_y += FALLING_SPEED_ACCEL;
            if state.Char().fall_y > FALLING_SPEED_MAX {
                state.Char().fall_y = FALLING_SPEED_MAX;
            }
        }
    }
}

/// Applies gravity to the current character for one frame.
#[no_mangle]
pub unsafe extern "C" fn fall_accel() {
    fall_accel_impl(&mut State);
}

/// Moves a falling character by his current fall velocity.
///
/// The vertical part always applies. The horizontal part — the forward drift
/// that carries a character out of a jump — only applies in true freefall, and
/// is suppressed during a super high jump so the Kid goes straight up.
unsafe fn fall_speed_impl(state: &mut State) {
    state.Char().y = state.Char().y.wrapping_add(state.Char().fall_y as u8);
    // USE_SUPER_HIGH_JUMP
    if state.Char().action == actions_actions_4_in_freefall as u8
        && ((*fixes).enable_super_high_jump == 0 || *state.super_jump_fall() == 0)
    {
        let fx = state.Char().fall_x as i32;
        let dx = char_dx_forward_impl(state, fx);
        state.Char().x = dx as u8;
        load_fram_det_col_impl(state);
    }
}

/// Moves the current character by his fall velocity.
#[no_mangle]
pub unsafe extern "C" fn fall_speed() {
    fall_speed_impl(&mut State);
}

/// Once-per-frame check on what the current character's pose implies.
///
/// Standing, walking and turning poses need a floor, so they go to
/// [`check_on_floor_impl`], which starts a fall if there is none. Poses already
/// in mid-air either watch for a ledge to grab ([`check_grab`], during the
/// first few frames of a fall, when a grab is still possible) or keep falling
/// (`do_fall`). Climbing poses are exempt entirely — they are holding on.
///
/// With `enable_jump_grab`, a jump plus Shift can grab a ledge mid-flight, and
/// a successful grab ends the check.
unsafe fn check_action_impl(state: &mut State) {
    let action = state.Char().action;
    let frame  = state.Char().frame;
    // USE_JUMP_GRAB
    if (*fixes).enable_jump_grab != 0
        && action == actions_actions_1_run_jump as u8
        && *state.control_shift() == CONTROL_HELD as i8
        && check_grab_run_jump() != 0
    {
        return;
    }
    if action == actions_actions_6_hang_straight as u8 || action == actions_actions_5_bumped as u8 {
        let standing_up = frameids_frame_110_stand_up_from_crouch_1 as u8
            ..=frameids_frame_119_stand_up_from_crouch_10 as u8;
        let lying_dead = frameids_frame_177_spiked as u8..=frameids_frame_185_dead as u8;
        if frame == frameids_frame_109_crouch as u8
            || ((*fixes).fix_stand_on_thin_air != 0 && standing_up.contains(&frame))
            // action == actions_5_bumped is also the pose of a corpse.
            || ((*fixes).fix_dead_floating_in_air != 0 && lying_dead.contains(&frame))
        {
            check_on_floor_impl(state);
        }
    } else if action == actions_actions_4_in_freefall as u8 {
        do_fall();
    } else if action == actions_actions_3_in_midair as u8 {
        // Frames 102..105: the start of a fall, while a grab is still possible.
        if (frameids_frame_102_start_fall_1 as u8..frameids_frame_106_fall as u8).contains(&frame) {
            check_grab();
        }
    } else if action != actions_actions_2_hang_climb as u8 {
        check_on_floor_impl(state);
    }
}

/// Once-per-frame check on what the current character's pose implies.
#[no_mangle]
pub unsafe extern "C" fn check_action() {
    check_action_impl(&mut State);
}

/// Whether a character can stand on this tile type.
///
/// Everything is walkable except empty space, the top of a big pillar, a door
/// top, a wall, and the four lattice pieces — all of which are things you fall
/// through or bump into rather than stand on.
#[no_mangle]
pub unsafe extern "C" fn tile_is_floor(tiletype: c_int) -> c_int {
    match tiletype as u32 {
        x if x == tiles_tiles_0_empty as u32
          || x == tiles_tiles_9_bigpillar_top as u32
          || x == tiles_tiles_12_doortop as u32
          || x == tiles_tiles_20_wall as u32
          || x == tiles_tiles_26_lattice_down as u32
          || x == tiles_tiles_27_lattice_small as u32
          || x == tiles_tiles_28_lattice_left as u32
          || x == tiles_tiles_29_lattice_right as u32 => 0,
        _ => 1,
    }
}

/// Impales the current character if he is moving fast across a live spike tile.
///
/// Standing still on spikes is safe; running over them (frames 7..14 and the
/// run-jump take-off 34..39) is fatal only while the blades are fully out
/// (`is_spike_harmful() >= 2`), and landing on them from either kind of jump is
/// fatal whenever they are out at all.
unsafe fn check_spiked_impl(state: &mut State) {
    let frame = state.Char().frame;
    let room = state.Char().room;
    let curr_col = state.Char().curr_col;
    let curr_row = state.Char().curr_row;
    if get_tile_impl(state, room as c_int, curr_col as c_int, curr_row as c_int) == tiles_tiles_2_spike as c_int {
        let harmful = is_spike_harmful();
        let running = (frameids_frame_7_run as u8..15).contains(&frame);
        let taking_off = (frameids_frame_34_start_run_jump_1 as u8..40).contains(&frame);
        let landing = frame == frameids_frame_43_running_jump_4 as u8
            || frame == frameids_frame_26_standing_jump_11 as u8;
        if (harmful >= 2 && (running || taking_off)) || (landing && harmful != 0) {
            spiked();
        }
    }
}

/// Impales the current character if he is moving across a live spike tile.
#[no_mangle]
pub unsafe extern "C" fn check_spiked() {
    check_spiked_impl(&mut State);
}

/// Queues `count` points of damage against the current character, and reports
/// whether it kills him.
///
/// The damage is not applied here — it goes into `hitp_delta` (Kid) or
/// `guardhp_delta` (anyone else) and is settled later in the frame, which is
/// what lets the health meter animate. A hit larger than the remaining health
/// is clamped to exactly the remaining health.
unsafe fn take_hp_impl(state: &mut State, count: c_int) -> c_int {
    let dead = if state.Char().charid == charids_charid_0_kid as u8 {
        if count >= *state.hitp_curr() as i32 {
            *state.hitp_delta() = -(*state.hitp_curr() as i32) as i16;
            true
        } else {
            *state.hitp_delta() = -(count as i16);
            false
        }
    } else if count >= *state.guardhp_curr() as i32 {
        *state.guardhp_delta() = -(*state.guardhp_curr() as i32) as i16;
        true
    } else {
        *state.guardhp_delta() = -(count as i16);
        false
    };
    dead as c_int
}

/// Queues `count` points of damage against the current character.
#[no_mangle]
pub unsafe extern "C" fn take_hp(count: c_int) -> c_int {
    take_hp_impl(&mut State, count)
}

/// The tile the current character is standing on.
unsafe fn get_tile_at_char_impl(state: &mut State) -> c_int {
    let room = state.Char().room;
    let curr_col = state.Char().curr_col;
    let curr_row = state.Char().curr_row;
    get_tile_impl(state, room as c_int, curr_col as c_int, curr_row as c_int)
}

/// The tile the current character is standing on.
#[no_mangle]
pub unsafe extern "C" fn get_tile_at_char() -> c_int {
    get_tile_at_char_impl(&mut State)
}

/// Works out the current character's bounding box on screen from the sprite of
/// the frame he is in.
///
/// Fills in the `char_*` globals: the left/right screen x, the top y, the rows
/// and columns those fall in, and a separate left/right pair *for collision*.
/// A `FRAME_THIN` frame narrows the collision pair by 4 pixels on each side so
/// that a lunging pose does not count as occupying the tile it reaches into.
unsafe fn set_char_collision_impl(state: &mut State) {
    let image = get_image(*state.obj_chtab() as c_short, *state.obj_id() as c_int);
    if image.is_null() {
        *state.char_width_half() = 0;
        *state.char_height()     = 0;
    } else {
        let (image_w, image_h) = crate::platform::sdl::shared_renderer().surface_size(image);
        *state.char_width_half() = ((image_w as i32 + 1) / 2) as u16;
        *state.char_height()     = image_h as u16;
    }
    *state.char_x_left() = (*state.obj_x() as i32 / 2 + 58) as i16;
    if state.Char().direction >= directions_dir_0_right as i8 {
        *state.char_x_left() -= *state.char_width_half() as i16;
    }
    *state.char_x_left_coll() = *state.char_x_left();
    *state.char_x_right()     = (*state.char_x_left() as i32 + *state.char_width_half() as i32) as i16;
    *state.char_x_right_coll() = *state.char_x_right();
    *state.char_top_y() = (*state.obj_y() as i32 - *state.char_height() as i32 + 1) as i16;
    if *state.char_top_y() >= 192 {
        *state.char_top_y() = 0;
    }
    let cty = *state.char_top_y();
    *state.char_top_row()    = y_to_row_mod4(cty as c_int) as i16;
    let oy = *state.obj_y();
    *state.char_bottom_row() = y_to_row_mod4(oy as c_int) as i16;
    if *state.char_bottom_row() == -1 {
        *state.char_bottom_row() = 3;
    }
    let cxl = *state.char_x_left() as c_int;
    let cxr = *state.char_x_right() as c_int;
    *state.char_col_left()  = get_tile_div_mod_impl(state, cxl).max(0) as i16;
    *state.char_col_right() = get_tile_div_mod_impl(state, cxr).min(9) as i16;
    if state.cur_frame().flags & frame_flags_FRAME_THIN as u8 != 0 {
        *state.char_x_left_coll()  += 4;
        *state.char_x_right_coll() -= 4;
    }
}

/// Works out the current character's on-screen bounding box.
#[no_mangle]
pub unsafe extern "C" fn set_char_collision() {
    set_char_collision_impl(&mut State);
}

/// Starts the current character falling if his pose needs a floor and there
/// isn't one.
///
/// Poses that do not carry `FRAME_NEEDS_FLOOR` (hanging, climbing, already
/// falling) are left alone. If the tile turns out to be a wall he is standing
/// inside it, which [`in_wall_impl`] pushes him out of.
///
/// Two special cases interrupt the fall. The level 12 "floors appear" event:
/// while the Kid is united with his shadow, stepping into the empty top row of
/// room 2 or the right-hand side of room 13 materialises a floor under him
/// instead. And with `fix_stand_on_thin_air`, a character standing up from a
/// crouch is not allowed to step off a ledge, so the fall is cancelled if
/// there is still floor two units behind him.
unsafe fn check_on_floor_impl(state: &mut State) {
    if state.cur_frame().flags & frame_flags_FRAME_NEEDS_FLOOR as u8 == 0 { return; }
    // FIX_FALLING_THROUGH_FLOOR_DURING_SWORD_STRIKE: frame 153 is the only
    // frame of the strike that needs a floor, so it alone would fall.
    if (*fixes).fix_falling_through_floor_during_sword_strike != 0
        && state.Char().frame == frameids_frame_153_strike_3 as u8
    {
        return;
    }
    if get_tile_at_char_impl(state) == tiles_tiles_20_wall as c_int {
        in_wall_impl(state);
    }
    if tile_is_floor(*state.curr_tile2() as c_int) != 0 { return; }
    // Special event: floors appear (level 12)
    if *state.current_level() == 12
        && (*state.united_with_shadow() < 0
            || ((*fixes).fix_hidden_floors_during_flashing != 0 && *state.united_with_shadow() > 0))
        && state.Char().curr_row == 0
        && (state.Char().room == 2 || (state.Char().room == 13 && *state.tile_col() >= 6))
    {
        *curr_room_tiles.add(*state.curr_tilepos() as usize) = tiles_tiles_1_floor as u8;
        set_wipe(*state.curr_tilepos() as c_short, 1);
        set_redraw_full(*state.curr_tilepos() as c_short, 1);
        *state.curr_tilepos() += 1;
        set_wipe(*state.curr_tilepos() as c_short, 1);
        set_redraw_full(*state.curr_tilepos() as c_short, 1);
        return;
    }
    // FIX_STAND_ON_THIN_AIR
    if (*fixes).fix_stand_on_thin_air != 0
        && (frameids_frame_110_stand_up_from_crouch_1 as u8
            ..=frameids_frame_119_stand_up_from_crouch_10 as u8)
            .contains(&state.Char().frame)
    {
        let dxw = dx_weight_impl(state);
        let bdx = back_delta_x_impl(state, 2);
        let col = get_tile_div_mod_m7(dxw + bdx);
        let room = state.Char().room;
        let curr_row = state.Char().curr_row;
        let t = get_tile_impl(state, room as c_int, col, curr_row as c_int);
        if tile_is_floor(t) != 0 {
            return;
        }
    }
    start_fall_impl(state);
}

/// Starts the current character falling if his pose needs a floor there isn't.
#[no_mangle]
pub unsafe extern "C" fn check_on_floor() {
    check_on_floor_impl(&mut State);
}

/// Drops the current character into the room below and picks the fall
/// animation that matches what he was doing.
///
/// The sword goes away, the character's row moves down one, and the pose he
/// fell from selects the sequence: each running frame, each kind of jump
/// landing and the hang-drop frames have their own, and a character with his
/// sword out (frames 150..179) gets one of the "pushed off the ledge"
/// sequences instead — with `Char.fall_x` distinguishing being shoved backwards
/// from walking off forwards. A guard who falls off the bottom-right corner of
/// a room is simply removed.
///
/// Afterwards the landing spot is re-checked: falling inside a wall pushes the
/// character out, and falling flush against one either nudges him a unit clear
/// or, if he is too close after a running jump, swaps in the "fall in front of
/// a wall" sequence.
unsafe fn start_fall_impl(state: &mut State) {
    let frame = state.Char().frame;
    state.Char().sword = sword_status_sword_0_sheathed as u8;
    inc_curr_row_impl(state);
    start_chompers();
    *state.fall_frame() = frame;
    let seq_id: u16 = if frame == frameids_frame_9_run as u8 {
        seqids_seq_7_fall as u16
    } else if frame == frameids_frame_13_run as u8 {
        seqids_seq_19_fall as u16
    } else if frame == frameids_frame_26_standing_jump_11 as u8 {
        seqids_seq_18_fall_after_standing_jump as u16
    } else if frame == frameids_frame_44_running_jump_5 as u8 {
        seqids_seq_21_fall_after_running_jump as u16
    } else if (frameids_frame_81_hangdrop_1 as u8..86).contains(&frame) {
        // Frames 81..85: landing after a jump up.
        let dx = char_dx_forward_impl(state, 5);
        state.Char().x = dx as u8;
        load_fram_det_col_impl(state);
        seqids_seq_19_fall as u16
    } else if (150..180).contains(&frame) {
        // Frames 150..179: sword out, falling, or dead.
        if state.Char().charid == charids_charid_2_guard as u8 {
            if state.Char().curr_row == 3 && state.Char().curr_col == 10 {
                clear_char_impl(state);
                return;
            }
            if (state.Char().fall_x as i32) < 0 {
                if state.Char().direction < directions_dir_0_right as i8 && distance_to_edge_weight_impl(state) <= 7 {
                    let dx = char_dx_forward_impl(state, -5);
                    state.Char().x = dx as u8;
                }
                seqids_seq_82_guard_pushed_off_ledge as u16
            } else {
                *state.droppedout() = 0;
                seqids_seq_83_guard_fall as u16
            }
        } else {
            *state.droppedout() = 1;
            if state.Char().direction < directions_dir_0_right as i8 && distance_to_edge_weight_impl(state) <= 7 {
                let dx = char_dx_forward_impl(state, -5);
                state.Char().x = dx as u8;
            }
            seqids_seq_81_kid_pushed_off_ledge as u16
        }
    } else {
        seqids_seq_7_fall as u16
    };
    seqtbl_offset_char(seq_id as c_short);
    play_seq_impl(state);
    load_fram_det_col_impl(state);
    if get_tile_at_char_impl(state) == tiles_tiles_20_wall as c_int {
        in_wall_impl(state);
        return;
    }
    let tile = get_tile_infrontof_char_impl(state);
    if tile == tiles_tiles_20_wall as c_int
        || ((*fixes).fix_running_jump_through_tapestry != 0
            && state.Char().direction == directions_dir_FF_left as i8
            && (tile == tiles_tiles_12_doortop as c_int
                || tile == tiles_tiles_7_doortop_with_floor as c_int))
    {
        if *state.fall_frame() != frameids_frame_44_running_jump_5 as u8
            || distance_to_edge_weight_impl(state) >= 6
        {
            let dx = char_dx_forward_impl(state, -1);
            state.Char().x = dx as u8;
        } else {
            seqtbl_offset_char(seqids_seq_104_start_fall_in_front_of_wall as c_short);
            play_seq_impl(state);
        }
        load_fram_det_col_impl(state);
    }
}

/// Drops the current character into the room below.
#[no_mangle]
pub unsafe extern "C" fn start_fall() {
    start_fall_impl(&mut State);
}

/// Catches a ledge on the way down, if the player is asking for it.
///
/// Requires Shift held (or, during a super high jump, the up arrow), a fall
/// slow enough to catch — under 32 pixels a frame, or 30 with
/// `fix_grab_falling_speed` — a living character, and a floor line within 25
/// pixels below him. The character is then shifted back 8 units to test
/// whether there is really something grabbable in front of and above him
/// ([`can_grab_front_above_impl`]); if not he is put straight back where he
/// was, and if so he is snapped to the ledge and started on the mid-air grab.
unsafe fn check_grab_impl(state: &mut State) {
    // FIX_GRAB_FALLING_SPEED: max = 30 if fix enabled, else 32
    let max_grab_falling_speed: i8 = if (*fixes).fix_grab_falling_speed != 0 { 30 } else { 32 };
    if (*state.control_shift() == CONTROL_HELD as i8
            // USE_SUPER_HIGH_JUMP: also allow grabbing with up arrow during super jump
            || ((*fixes).enable_super_high_jump != 0
                && *state.super_jump_fall() != 0
                && *state.control_y() == CONTROL_HELD_UP as i8))
        && state.Char().fall_y < max_grab_falling_speed
        && state.Char().alive < 0
        // C compares these as `word`; the wraparound is the condition, not an
        // accident — a floor line above the character reads as a huge u16 and
        // so fails the test.
        && (y_land_at((state.Char().curr_row + 1) as usize) as u16) <= (state.Char().y as i32 + 25) as u16
    {
        let old_x = state.Char().x;
        let super_delta: i32 = if (*fixes).enable_super_high_jump != 0 && *state.super_jump_fall() != 0 {
            if state.Char().direction == directions_dir_FF_left as i8 { 3 } else { 4 }
        } else { 0 };
        let dx = char_dx_forward_impl(state, -8 + super_delta);
        state.Char().x = dx as u8;
        load_fram_det_col_impl(state);
        if can_grab_front_above_impl(state) == 0 {
            state.Char().x = old_x;
        } else {
            let dew = distance_to_edge_weight_impl(state);
            let dx = char_dx_forward_impl(state, dew - super_delta);
            state.Char().x = dx as u8;
            let curr_row = state.Char().curr_row;
            state.Char().y = y_land_at((curr_row + 1) as usize) as u8;
            state.Char().fall_y = 0;
            seqtbl_offset_char(seqids_seq_15_grab_ledge_midair as c_short);
            play_seq_impl(state);
            *state.grab_timer() = 12;
            play_sound(soundids_sound_9_grab as c_int);
            *state.is_screaming() = 0;
            // FIX_CHOMPERS_NOT_STARTING
            if (*fixes).fix_chompers_not_starting != 0 { start_chompers(); }
        }
    }
}

/// Catches a ledge on the way down, if the player is asking for it.
#[no_mangle]
pub unsafe extern "C" fn check_grab() {
    check_grab_impl(&mut State);
}

/// Grabs a ledge in mid-jump (`enable_jump_grab`).
///
/// Only during the two frames of a standing jump and the three of a running
/// jump where the hands are up, and only with the up arrow held and the
/// direction stick centred, so it cannot fire by accident during an ordinary
/// jump. Grabbing distance therefore works out at about a tile for a standing
/// jump and two and a half for a running one.
///
/// The character is snapped onto the grabbed tile by column rather than nudged,
/// so the column has to be corrected when the tile turns out to be in a
/// neighbouring room — otherwise the Kid teleports across the level. Grabbing a
/// button tile presses it and grabbing a loose floor knocks it down, because
/// [`check_press_impl`] will not get the chance next frame.
///
/// Returns 1 if a ledge was grabbed.
unsafe fn check_grab_run_jump_impl(state: &mut State) -> c_int {
    let frame = state.Char().frame as u32;
    let is_jump = (frameids_frame_22_standing_jump_7 as u32
        ..=frameids_frame_23_standing_jump_8 as u32).contains(&frame);
    let is_running_jump = (frameids_frame_39_start_run_jump_6 as u32
        ..=frameids_frame_41_running_jump_2 as u32).contains(&frame);
    let char_room_m1 = state.Char().room - 1;
    if state.Char().action == actions_actions_1_run_jump as u8
        && (is_jump || is_running_jump)
        && *state.control_x() == CONTROL_RELEASED as i8
        && *state.control_y() == CONTROL_HELD_UP as i8
        // can_grab_front_above_impl has side effects, but && short-circuits
        // over exactly the calls the nested C ifs did, in the same order.
        && can_grab_front_above_impl(state) != 0
    {
        let grab_tile = *state.curr_tile2();
        let mut grab_col = *state.tile_col();
        let char_room = state.Char().room;
        // Nothing below writes curr_room, so one read serves the whole chain.
        let tile_room = *state.curr_room();
        if tile_room != char_room as i16 {
            let left_room  = state.level().roomlinks[char_room_m1 as usize].left;
            let right_room = state.level().roomlinks[char_room_m1 as usize].right;
            let up_room    = state.level().roomlinks[char_room_m1 as usize].up;
            if tile_room == right_room as i16 {
                grab_col += 10;
            } else if tile_room == left_room as i16 {
                grab_col -= 10;
            } else if right_room != 0 && tile_room == state.level().roomlinks[(right_room - 1) as usize].up as i16 {
                grab_col += 10;
            } else if left_room != 0 && tile_room == state.level().roomlinks[(left_room - 1) as usize].up as i16 {
                grab_col -= 10;
            } else if up_room != 0 && tile_room == state.level().roomlinks[(up_room - 1) as usize].right as i16 {
                grab_col += 10;
            } else if up_room != 0 && tile_room == state.level().roomlinks[(up_room - 1) as usize].left as i16 {
                grab_col -= 10;
            }
        }
        // Direct screen-position assignment -- NOT routed through char_dx_forward
        // (unlike the dir_delta assignment right below).
        state.Char().x = (x_bump_at((grab_col + FIRST_ONSCREEN_COLUMN as i16) as usize) as i32 + TILE_MIDX) as u8;
        let dir_delta: i32 = if state.Char().direction == directions_dir_FF_left as i8 { -12 } else { 2 };
        let dx = char_dx_forward_impl(state, dir_delta);
        state.Char().x = dx as u8;
        let curr_row = state.Char().curr_row;
        state.Char().y = y_land_at((curr_row + 1) as usize) as u8;
        seqtbl_offset_char(seqids_seq_9_grab_while_jumping as c_short);
        play_seq_impl(state);
        *state.grab_timer() = 12;
        play_sound(soundids_sound_9_grab as c_int);
        if grab_tile == tiles_tiles_15_opener as u8 || grab_tile == tiles_tiles_6_closer as u8 {
            trigger_button(1, 0, -1);
        } else if grab_tile == tiles_tiles_11_loose as u8 {
            *state.is_guard_notice() = 1;
            make_loose_fall(1);
        }
        return 1;
    }
    0
}

/// Grabs a ledge in mid-jump. Returns 1 if a ledge was grabbed.
#[no_mangle]
pub unsafe extern "C" fn check_grab_run_jump() -> c_int {
    check_grab_run_jump_impl(&mut State)
}

/// Whether the current character can pull himself up onto the tile diagonally
/// in front of and above him.
///
/// Sets up [`can_grab_impl`]'s two inputs: `through_tile` is the tile directly
/// above his head (which he would have to pass through) and `curr_tile2` is the
/// one in front of that (the ledge itself).
unsafe fn can_grab_front_above_impl(state: &mut State) -> c_int {
    let above = get_tile_above_char_impl(state) as u8;
    *state.through_tile() = above;
    get_tile_front_above_char_impl(state);
    can_grab_impl(state)
}

/// Whether the current character can grab the ledge above and in front of him.
#[no_mangle]
pub unsafe extern "C" fn can_grab_front_above() -> c_int {
    can_grab_front_above_impl(&mut State)
}

/// Pushes a character who has ended up inside a wall back out of it.
///
/// If he is already past the middle of the tile, or the next tile along is
/// wall too, he is bounced back to 6 units from the near edge; otherwise he is
/// nudged 4 units further in the direction he was going, out through the far
/// side.
unsafe fn in_wall_impl(state: &mut State) {
    let mut delta_x = distance_to_edge_weight_impl(state);
    if delta_x >= 8 || get_tile_infrontof_char_impl(state) == tiles_tiles_20_wall as c_int {
        delta_x = 6 - delta_x;
    } else {
        delta_x += 4;
    }
    let dx = char_dx_forward_impl(state, delta_x);
    state.Char().x = dx as u8;
    load_fram_det_col_impl(state);
    get_tile_at_char_impl(state);
}

/// Pushes the current character back out of a wall he is inside.
#[no_mangle]
pub unsafe extern "C" fn in_wall() {
    in_wall_impl(&mut State);
}

/// The tile one column in front of the current character, in his own row.
/// Also leaves that column in `infrontx`.
unsafe fn get_tile_infrontof_char_impl(state: &mut State) -> c_int {
    let d = dir_front_at((state.Char().direction as i8 + 1) as usize);
    *state.infrontx() = (d as i32 + state.Char().curr_col as i32) as i8;
    let room = state.Char().room;
    let ifx = *state.infrontx();
    let curr_row = state.Char().curr_row;
    get_tile_impl(state, room as c_int, ifx as c_int, curr_row as c_int)
}

/// The tile one column in front of the current character.
#[no_mangle]
pub unsafe extern "C" fn get_tile_infrontof_char() -> c_int {
    get_tile_infrontof_char_impl(&mut State)
}

/// The tile *two* columns in front of the current character — used to look one
/// tile further ahead than [`get_tile_infrontof_char_impl`].
unsafe fn get_tile_infrontof2_char_impl(state: &mut State) -> c_int {
    let direction = dir_front_at((state.Char().direction as i8 + 1) as usize);
    *state.infrontx() = ((direction as i32 * 2) + state.Char().curr_col as i32) as i8;
    let room = state.Char().room;
    let ifx = *state.infrontx();
    let curr_row = state.Char().curr_row;
    get_tile_impl(state, room as c_int, ifx as c_int, curr_row as c_int)
}

/// The tile two columns in front of the current character.
#[no_mangle]
pub unsafe extern "C" fn get_tile_infrontof2_char() -> c_int {
    get_tile_infrontof2_char_impl(&mut State)
}

/// The tile one column behind the current character, in his own row.
unsafe fn get_tile_behind_char_impl(state: &mut State) -> c_int {
    let d = dir_behind_at((state.Char().direction as i8 + 1) as usize);
    let room = state.Char().room;
    let curr_col = state.Char().curr_col;
    let curr_row = state.Char().curr_row;
    get_tile_impl(
        state,
        room as c_int,
        (d as i32 + curr_col as i32) as c_int,
        curr_row as c_int,
    )
}

/// The tile one column behind the current character.
#[no_mangle]
pub unsafe extern "C" fn get_tile_behind_char() -> c_int {
    get_tile_behind_char_impl(&mut State)
}

/// [`distance_to_edge_impl`] measured from the character's centre of gravity —
/// how far he can still move forward before his weight leaves this tile.
unsafe fn distance_to_edge_weight_impl(state: &mut State) -> c_int {
    let dxw = dx_weight_impl(state);
    distance_to_edge_impl(state, dxw)
}

/// How far the current character's weight is from the edge of its tile.
#[no_mangle]
pub unsafe extern "C" fn distance_to_edge_weight() -> c_int {
    distance_to_edge_weight_impl(&mut State)
}

/// How many units `xpos` is from the *forward* edge of the tile it is in:
/// counted from the left edge when facing left, from the right when facing
/// right. Leaves the within-tile offset in `obj_xl` as a side effect.
unsafe fn distance_to_edge_impl(state: &mut State, xpos: c_int) -> c_int {
    get_tile_div_mod_m7(xpos);
    let mut distance = *state.obj_xl() as c_int;
    if state.Char().direction == directions_dir_0_right as i8 {
        distance = TILE_RIGHTX - distance;
    }
    distance
}

/// How far `xpos` is from the forward edge of its tile.
#[no_mangle]
pub unsafe extern "C" fn distance_to_edge(xpos: c_int) -> c_int {
    distance_to_edge_impl(&mut State, xpos)
}

/// Kills a character who has fallen out of the level entirely (room 0).
unsafe fn fell_out_impl(state: &mut State) {
    if state.Char().alive < 0 && state.Char().room == 0 {
        take_hp_impl(state, 100);
        state.Char().alive = 0;
        erase_bottom_text(1);
        state.Char().frame = frameids_frame_185_dead as u8;
    }
}

/// Kills the current character if he has fallen out of the level.
#[no_mangle]
pub unsafe extern "C" fn fell_out() {
    fell_out_impl(&mut State);
}

/// Runs one frame of the Kid: input, and then the death sequence if he is
/// dead.
///
/// `Char.alive` counts frames since death. The count drives a small state
/// machine: a quicksave-style resurrection if `resurrect_time` is set; at 6,
/// the death music (suppressed on the demo and potions levels); at 7, either
/// the "Press Button to Continue" prompt or, on those two levels, a short
/// delay. Nothing advances while a sound other than a gate opening is still
/// playing, which is what keeps the music and the prompt from overlapping.
unsafe fn play_kid_impl(state: &mut State) {
    fell_out_impl(state);
    control_kid_impl(state);
    if state.Char().alive >= 0 && is_dead() != 0 {
        if *state.resurrect_time() != 0 {
            stop_sounds();
            loadkid_impl(state);
            *state.hitp_delta() = *state.hitp_max() as i16;
            seqtbl_offset_char(seqids_seq_2_stand as c_short);
            state.Char().x = state.Char().x.wrapping_add(8);
            play_seq_impl(state);
            load_fram_det_col_impl(state);
            set_start_pos();
        }
        if check_sound_playing() != 0 && current_sound != 5 {
            return;
        }
        *state.is_show_time() = 0;
        if state.Char().alive < 0 || state.Char().alive >= 6 {
            if state.Char().alive == 6 {
                if is_sound_on != 0
                    && *state.current_level() != 0
                    && *state.current_level() != 15
                {
                    play_death_music_impl(state);
                }
            } else {
                if state.Char().alive != 7 || check_sound_playing() != 0 { return; }
                if *state.rem_min() == 0 {
                    expired();
                }
                if *state.current_level() != 0 && *state.current_level() != 15 {
                    *state.text_time_remaining() = 288;
                    *state.text_time_total() = 288;
                    display_text_bottom(b"Press Button to Continue\0".as_ptr() as *const _);
                } else {
                    *state.text_time_remaining() = 36;
                    *state.text_time_total() = 36;
                }
            }
        }
        state.Char().alive += 1;
    }
}

/// Runs one frame of the Kid.
#[no_mangle]
pub unsafe extern "C" fn play_kid() {
    play_kid_impl(&mut State);
}

/// Feeds the Kid one frame's worth of control input.
///
/// On the title-screen demo level the input comes from the canned move list
/// (or from the guard AI, once the demo Kid has a sword) and any keypress
/// starts a real game. Otherwise the latched controls are restored, the replay
/// system gets its chance to record or substitute a move, and the input is
/// handed to [`user_control_impl`].
unsafe fn control_kid_impl(state: &mut State) {
    if state.Char().alive < 0 && *state.hitp_curr() == 0 {
        state.Char().alive = 0;
        if (*fixes).fix_quicksave_during_feather != 0 && *state.is_feather_fall() > 0 {
            *state.is_feather_fall() = 0;
            if check_sound_playing() != 0 {
                stop_sounds();
            }
        }
    }
    if *state.grab_timer() != 0 {
        *state.grab_timer() -= 1;
    }
    // USE_REPLAY: demo level check
    if *state.current_level() == 0 && *state.play_demo_level() == 0 && replaying == 0 {
        do_demo_impl(state);
        control();
        let key = key_test_quit();
        if key == (15i32 | key_modifiers_WITH_CTRL as i32) {
            if load_game() != 0 {
                start_game();
            }
        } else if key != 0 {
            start_level = (*custom).first_level as i16;
            start_game();
        }
    } else {
        rest_ctrl_1_impl(state);
        do_paused();
        if recording != 0 { add_replay_move(); }
        if replaying != 0 { do_replay_move(); }
        read_user_control_impl(state);
        user_control_impl(state);
        save_ctrl_1_impl(state);
    }
}

/// Feeds the Kid one frame's worth of control input.
#[no_mangle]
pub unsafe extern "C" fn control_kid() {
    control_kid_impl(&mut State);
}

/// Supplies the title-screen demo's input.
///
/// Normally the canned move list from the options; once the demo guard has
/// been killed (`checkpoint`) just "walk forward" so the Kid runs out of the
/// room; and while the demo Kid has his sword out, the guard AI drives him at
/// skill 10 so the fight looks like a fight.
unsafe fn do_demo_impl(state: &mut State) {
    if *state.checkpoint() != 0 {
        *state.control_shift2() = release_arrows() as i8;
        *state.control_forward() = CONTROL_HELD as i8;
        *state.control_x() = CONTROL_HELD_FORWARD as i8;
    } else if state.Char().sword != 0 {
        *state.guard_skill() = 10;
        autocontrol_opponent();
        *state.guard_skill() = 11;
    } else {
        do_auto_moves(core::ptr::addr_of!((*custom).demo_moves) as *const auto_move_type);
    }
}

/// Supplies the title-screen demo's input.
#[no_mangle]
pub unsafe extern "C" fn do_demo() {
    do_demo_impl(&mut State);
}

/// Runs one frame of the Kid's opponent.
///
/// The mouse is pure AI and nothing else applies to it. For a guard, running
/// out of health here is what turns "alive" off and fires the reward
/// ([`on_guard_killed_impl`]). A shadow that is not mid-death is removed
/// outright rather than left standing — the shadow never has a corpse.
unsafe fn play_guard_impl(state: &mut State) {
    if state.Char().charid == charids_charid_24_mouse as u8 {
        autocontrol_opponent();
    } else {
        // The C source jumps over the shadow check with a goto when the
        // character is wounded but still alive, hence the flag.
        let mut skip_shadow_check = false;
        if state.Char().alive < 0 {
            if *state.guardhp_curr() == 0 {
                state.Char().alive = 0;
                on_guard_killed();
            } else {
                skip_shadow_check = true;
            }
        }
        if !skip_shadow_check && state.Char().charid == charids_charid_1_shadow as u8 {
            clear_char_impl(state);
        }
        autocontrol_opponent();
        control();
    }
}

/// Runs one frame of the Kid's opponent.
#[no_mangle]
pub unsafe extern "C" fn play_guard() {
    play_guard_impl(&mut State);
}

/// Applies the player's input to the current character.
///
/// The controls are expressed relative to the character's facing, so for a
/// character facing right they have to be mirrored before [`control`] sees
/// them, and mirrored back afterwards. See [`flip_control_x_impl`].
unsafe fn user_control_impl(state: &mut State) {
    if state.Char().direction >= directions_dir_0_right as i8 {
        flip_control_x_impl(state);
        control();
        flip_control_x_impl(state);
    } else {
        control();
    }
}

/// Applies the player's input to the current character.
#[no_mangle]
pub unsafe extern "C" fn user_control() {
    user_control_impl(&mut State);
}

/// Mirrors the horizontal controls: left becomes right, and "forward" and
/// "backward" trade places. Applied twice around a call, it leaves everything
/// as it found it.
unsafe fn flip_control_x_impl(state: &mut State) {
    *state.control_x() = -*state.control_x();
    let temp = *state.control_forward();
    *state.control_forward() = *state.control_backward();
    *state.control_backward() = temp;
}

/// Mirrors the horizontal controls.
#[no_mangle]
pub unsafe extern "C" fn flip_control_x() {
    flip_control_x_impl(&mut State);
}

/// Clears all four direction latches and returns 1.
///
/// The return value exists so callers can write
/// `*state.control_up() = release_arrows() as i8` — clear everything, then
/// mark this one control as consumed — in a single statement, which is the
/// idiom the movement code in `seg005` is full of.
unsafe fn release_arrows_impl(state: &mut State) -> c_int {
    *state.control_backward() = CONTROL_RELEASED as i8;
    *state.control_forward()  = CONTROL_RELEASED as i8;
    *state.control_up()       = CONTROL_RELEASED as i8;
    *state.control_down()     = CONTROL_RELEASED as i8;
    1
}

/// Clears all four direction latches and returns 1.
#[no_mangle]
pub unsafe extern "C" fn release_arrows() -> c_int {
    release_arrows_impl(&mut State)
}

/// Stashes the latched controls so the next frame can restore them.
///
/// The latches have to survive the guard's turn, which runs between two of the
/// Kid's frames and overwrites the same globals.
unsafe fn save_ctrl_1_impl(state: &mut State) {
    *state.ctrl1_forward()  = *state.control_forward();
    *state.ctrl1_backward() = *state.control_backward();
    *state.ctrl1_up()       = *state.control_up();
    *state.ctrl1_down()     = *state.control_down();
    *state.ctrl1_shift2()   = *state.control_shift2();
}

/// Stashes the latched controls for the next frame.
#[no_mangle]
pub unsafe extern "C" fn save_ctrl_1() {
    save_ctrl_1_impl(&mut State);
}

/// Restores the latched controls stashed by [`save_ctrl_1_impl`].
unsafe fn rest_ctrl_1_impl(state: &mut State) {
    *state.control_forward()  = *state.ctrl1_forward();
    *state.control_backward() = *state.ctrl1_backward();
    *state.control_up()       = *state.ctrl1_up();
    *state.control_down()     = *state.ctrl1_down();
    *state.control_shift2()   = *state.ctrl1_shift2();
}

/// Restores the latched controls.
#[no_mangle]
pub unsafe extern "C" fn rest_ctrl_1() {
    rest_ctrl_1_impl(&mut State);
}

/// Forgets the stashed controls, so nothing held before a level change or a
/// reload carries over into it.
unsafe fn clear_saved_ctrl_impl(state: &mut State) {
    *state.ctrl1_forward()  = CONTROL_RELEASED as i8;
    *state.ctrl1_backward() = CONTROL_RELEASED as i8;
    *state.ctrl1_up()       = CONTROL_RELEASED as i8;
    *state.ctrl1_down()     = CONTROL_RELEASED as i8;
    *state.ctrl1_shift2()   = CONTROL_RELEASED as i8;
}

/// Forgets the stashed controls.
#[no_mangle]
pub unsafe extern "C" fn clear_saved_ctrl() {
    clear_saved_ctrl_impl(&mut State);
}

/// Advances one latched control given whether its key is currently down.
///
/// A latch already at `CONTROL_IGNORE` (below `CONTROL_RELEASED`) is left
/// alone: a move has consumed that press and it stays consumed until the key
/// is physically released. Otherwise the latch tracks the key, and the moment
/// it goes down it reads `CONTROL_HELD` — the "just pressed" state the
/// movement code tests for.
unsafe fn latch_control(latch: &mut i8, key_is_down: bool) {
    if *latch >= CONTROL_RELEASED as i8 {
        if key_is_down {
            if *latch == CONTROL_RELEASED as i8 {
                *latch = CONTROL_HELD as i8;
            }
        } else {
            *latch = CONTROL_RELEASED as i8;
        }
    }
}

/// Turns the raw control state into the five latched controls for this frame.
///
/// `control_x` / `control_y` say which way the direction stick is pushed right
/// now and `control_shift` whether Shift is down; the latches
/// (`control_forward` and friends) remember whether that is a *new* press. See
/// [`latch_control`].
unsafe fn read_user_control_impl(state: &mut State) {
    // Nothing below writes control_x/control_y/control_shift, so one read of
    // each serves all five latches.
    let stick_x = *state.control_x();
    let stick_y = *state.control_y();
    let shift_held = *state.control_shift() == CONTROL_HELD as i8;
    latch_control(state.control_forward(),  stick_x == CONTROL_HELD_FORWARD as i8);
    latch_control(state.control_backward(), stick_x == CONTROL_HELD_BACKWARD as i8);
    latch_control(state.control_up(),       stick_y == CONTROL_HELD_UP as i8);
    latch_control(state.control_down(),     stick_y == CONTROL_HELD_DOWN as i8);
    latch_control(state.control_shift2(),   shift_held);
}

/// Turns the raw control state into this frame's latched controls.
#[no_mangle]
pub unsafe extern "C" fn read_user_control() {
    read_user_control_impl(&mut State);
}

/// Whether the current character can grab `curr_tile2` by reaching through
/// `through_tile`.
///
/// Both tiles have to cooperate: you cannot reach through a wall, a floor, or
/// (facing right) a door top, and the tile you land on has to be a floor. Two
/// tiles are directional — a door top with a floor can only be taken from the
/// left — and a loose floor that is already shaking cannot be grabbed at all,
/// unless the level's `loose_floor_delay` has been raised above the default,
/// in which case there is time.
unsafe fn can_grab_impl(state: &mut State) -> c_int {
    let modifier = *curr_room_modif.add(*state.curr_tilepos() as usize);
    if *state.through_tile() == tiles_tiles_20_wall as u8 { return 0; }
    if *state.through_tile() == tiles_tiles_12_doortop as u8
        && state.Char().direction >= directions_dir_0_right as i8
    {
        return 0;
    }
    if tile_is_floor(*state.through_tile() as c_int) != 0 { return 0; }
    if *state.curr_tile2() == tiles_tiles_11_loose as u8
        && modifier != 0
        && !((*custom).loose_floor_delay > 11)
    {
        return 0;
    }
    if *state.curr_tile2() == tiles_tiles_7_doortop_with_floor as u8
        && state.Char().direction < directions_dir_0_right as i8
    {
        return 0;
    }
    if tile_is_floor(*state.curr_tile2() as c_int) == 0 { return 0; }
    1
}

/// Whether the current character can grab `curr_tile2` through `through_tile`.
#[no_mangle]
pub unsafe extern "C" fn can_grab() -> c_int {
    can_grab_impl(&mut State)
}

/// Which side(s) of a tile block movement: 0 none, 1 right, 2 left, 3 chomper
/// at the left, 4 both. Gates and door tops block from the right, a mirror from
/// the left, and a wall from both.
#[no_mangle]
pub unsafe extern "C" fn wall_type(tiletype: u8) -> c_int {
    match tiletype as u32 {
        x if x == tiles_tiles_4_gate as u32
          || x == tiles_tiles_7_doortop_with_floor as u32
          || x == tiles_tiles_12_doortop as u32 => 1,
        x if x == tiles_tiles_13_mirror as u32 => 2,
        x if x == tiles_tiles_18_chomper as u32 => 3,
        x if x == tiles_tiles_20_wall as u32 => 4,
        _ => 0,
    }
}

/// The tile directly above the current character's head.
unsafe fn get_tile_above_char_impl(state: &mut State) -> c_int {
    let room = state.Char().room;
    let curr_col = state.Char().curr_col;
    let curr_row = state.Char().curr_row;
    get_tile_impl(state, room as c_int, curr_col as c_int, curr_row as c_int - 1)
}

/// The tile directly above the current character's head.
#[no_mangle]
pub unsafe extern "C" fn get_tile_above_char() -> c_int {
    get_tile_above_char_impl(&mut State)
}

/// The tile above and one column behind the current character.
unsafe fn get_tile_behind_above_char_impl(state: &mut State) -> c_int {
    let d = dir_behind_at((state.Char().direction as i8 + 1) as usize);
    let room = state.Char().room;
    let curr_col = state.Char().curr_col;
    let curr_row = state.Char().curr_row;
    get_tile_impl(
        state,
        room as c_int,
        (d as i32 + curr_col as i32) as c_int,
        curr_row as c_int - 1,
    )
}

/// The tile above and one column behind the current character.
#[no_mangle]
pub unsafe extern "C" fn get_tile_behind_above_char() -> c_int {
    get_tile_behind_above_char_impl(&mut State)
}

/// The tile above and one column in front of the current character — the ledge
/// he would pull himself onto. Also leaves that column in `infrontx`.
unsafe fn get_tile_front_above_char_impl(state: &mut State) -> c_int {
    let d = dir_front_at((state.Char().direction as i8 + 1) as usize);
    *state.infrontx() = (d as i32 + state.Char().curr_col as i32) as i8;
    let room = state.Char().room;
    let ifx = *state.infrontx();
    let curr_row = state.Char().curr_row;
    get_tile_impl(state, room as c_int, ifx as c_int, curr_row as c_int - 1)
}

/// The tile above and one column in front of the current character.
#[no_mangle]
pub unsafe extern "C" fn get_tile_front_above_char() -> c_int {
    get_tile_front_above_char_impl(&mut State)
}

/// `delta_x` signed so that it points *backwards* relative to the current
/// character — the mirror image of [`char_dx_forward_impl`]'s sign rule.
unsafe fn back_delta_x_impl(state: &mut State, delta_x: c_int) -> c_int {
    if state.Char().direction < directions_dir_0_right as i8 {
        delta_x
    } else {
        -delta_x
    }
}

/// `delta_x` signed to point backwards relative to the current character.
#[no_mangle]
pub unsafe extern "C" fn back_delta_x(delta_x: c_int) -> c_int {
    back_delta_x_impl(&mut State, delta_x)
}

/// Picks up whatever is on the current tile: records what it was, erases it
/// from the room, and marks the tile for redraw.
///
/// The effect is not applied here — that happens when the drinking animation
/// reaches its `SEQ_GET_ITEM` opcode and calls [`proc_get_object_impl`].
/// `control_shift2` is set to `CONTROL_IGNORE` so the same Shift press cannot
/// pick anything else up.
unsafe fn do_pickup_impl(state: &mut State, obj_type: c_int) {
    *state.pickup_obj_type() = obj_type as i16;
    *state.control_shift2() = CONTROL_IGNORE as i8;
    *curr_room_tiles.add(*state.curr_tilepos() as usize) = tiles_tiles_1_floor as u8;
    *curr_room_modif.add(*state.curr_tilepos() as usize) = 0;
    *state.redraw_height() = 35;
    set_wipe(*state.curr_tilepos() as c_short, 1);
    set_redraw_full(*state.curr_tilepos() as c_short, 1);
}

/// Picks up the object on the current tile.
#[no_mangle]
pub unsafe extern "C" fn do_pickup(obj_type: c_int) {
    do_pickup_impl(&mut State, obj_type);
}

/// Applies the current character's weight to the tile he is pressing on.
///
/// Which tile that is depends on his pose: while hanging or climbing it is the
/// one he is holding on to (above him), otherwise the one under his feet — and
/// only if his frame needs a floor at all, so a character in mid-air presses
/// nothing. Jumping up into a loose floor knocks it down from below. Pressing a
/// button triggers it, unless he is dead, in which case his corpse triggers it
/// once via `died_on_button`.
unsafe fn check_press_impl(state: &mut State) {
    let frame  = state.Char().frame;
    let action = state.Char().action;
    // Frames 87..99: hanging. Frames 135..140: starting to climb up.
    if (frameids_frame_87_hanging_1 as u8..100).contains(&frame)
        || (frameids_frame_135_climbing_1 as u8..frameids_frame_141_climbing_7 as u8).contains(&frame)
    {
        get_tile_above_char_impl(state);
    } else if action == actions_actions_7_turn as u8
        || action == actions_actions_5_bumped as u8
        || (action as u8) < actions_actions_2_hang_climb as u8
    {
        if frame == frameids_frame_79_jumphang as u8 && get_tile_above_char_impl(state) == tiles_tiles_11_loose as c_int {
            make_loose_fall(1);
        } else {
            if state.cur_frame().flags & frame_flags_FRAME_NEEDS_FLOOR as u8 == 0 { return; }
            // FIX_PRESS_THROUGH_CLOSED_GATES
            if (*fixes).fix_press_through_closed_gates != 0 { determine_col_impl(state); }
            get_tile_at_char_impl(state);
        }
    } else {
        return;
    }
    if *state.curr_tile2() == tiles_tiles_15_opener as u8 || *state.curr_tile2() == tiles_tiles_6_closer as u8 {
        if state.Char().alive < 0 {
            trigger_button(1, 0, -1);
        } else {
            died_on_button();
        }
    } else if *state.curr_tile2() == tiles_tiles_11_loose as u8 {
        *state.is_guard_notice() = 1;
        make_loose_fall(1);
    }
}

/// Applies the current character's weight to the tile he is pressing on.
#[no_mangle]
pub unsafe extern "C" fn check_press() {
    check_press_impl(&mut State);
}

/// Wakes any spikes underneath the current character.
///
/// Every column his sprite spans is scanned downwards from his own row: through
/// empty space, and stopping at the first spike (which is triggered) or the
/// first floor. Without `fix_infinite_down_bug` the descent is bounded by
/// "still the same room" rather than by the row number, which is the bug the
/// fix names — the row-based bound is the one that terminates reliably.
unsafe fn check_spike_below_impl(state: &mut State) {
    let right_col = get_tile_div_mod_m7(*state.char_x_right() as c_int);
    if right_col < 0 { return; }
    let room = state.Char().room;
    let left_col = get_tile_div_mod_m7(*state.char_x_left() as c_int);
    for col in left_col..=right_col {
        let mut row = state.Char().curr_row;
        loop {
            if get_tile_impl(state, room as c_int, col, row as c_int) == tiles_tiles_2_spike as c_int {
                start_anim_spike(*state.curr_room(), *state.curr_tilepos() as c_short);
                break;
            }
            let keep_descending = tile_is_floor(*state.curr_tile2() as c_int) == 0
                && *state.curr_room() != 0
                && if (*fixes).fix_infinite_down_bug != 0 { row <= 2 } else { room as i16 == *state.curr_room() };
            if !keep_descending { break; }
            row += 1;
        }
    }
}

/// Wakes any spikes underneath the current character.
#[no_mangle]
pub unsafe extern "C" fn check_spike_below() {
    check_spike_below_impl(&mut State);
}

/// Narrows the sprite clipping rectangle so the current character is cut off
/// by whatever is in front of or above him.
///
/// Two edges get adjusted. The *top*, when there is a wall or a floor above his
/// head: he is clipped to the floor line of the row he is in, so a character
/// hanging under a ceiling or standing in a doorway does not draw through it.
/// And the *right*, against a wall, a mirror, or a door top — a door top with
/// no floor clips a half-tile further right than a wall does, which is what
/// lets the Kid's head show through an open doorway. Frames 224..228 are
/// climbing the level door and are clipped to the door itself instead.
///
/// With `enable_super_high_jump`, jumping up into a lattice-backed door top
/// clips against that instead of anything else.
unsafe fn clip_char_impl(state: &mut State) {
    let frame  = state.Char().frame;
    let action = state.Char().action;
    let room   = state.Char().room;
    let row    = state.Char().curr_row;
    reset_obj_clip_impl(state);
    // USE_SUPER_HIGH_JUMP: clip during super jump
    if (*fixes).enable_super_high_jump != 0
        && (frame == frameids_frame_79_jumphang as u8 || frame == frameids_frame_106_fall as u8)
    {
        let ccl = *state.char_col_left();
        let cty = *state.char_top_y();
        let top_left_tile = get_tile_impl(
            state,
            room as c_int,
            ccl as c_int - 1,
            y_to_row_mod4(cty as c_int + 10),
        );
        // Modifier 0 on a door top means it is backed by lattice.
        if top_left_tile == tiles_tiles_12_doortop as c_int
            && *curr_room_modif.add(*state.curr_tilepos() as usize) == 0
        {
            let tr = *state.tile_row();
            // y_land, not y_clip: seg006.c:1762 indexes the floor-line table
            // here, unlike the clip line used further down this function.
            *state.obj_clip_top() = y_land_at((tr + 1) as usize) - 22;
            return;
        }
    }
    if frame >= frameids_frame_224_exit_stairs_8 as u8 && frame < 229 {
        *state.obj_clip_top()   = *state.leveldoor_ybottom() as i16 + 1;
        *state.obj_clip_right() = *state.leveldoor_right() as i16;
    } else {
        let ccl = *state.char_col_left();
        let ctr = *state.char_top_row();
        if get_tile_impl(state, room as c_int, ccl as c_int, ctr as c_int) == tiles_tiles_20_wall as c_int
            || tile_is_floor(*state.curr_tile2() as c_int) != 0
        {
            let ccr = *state.char_col_right();
            if (action == actions_actions_0_stand as u8
                    && (frame == frameids_frame_79_jumphang as u8
                        || frame == frameids_frame_81_hangdrop_1 as u8))
                || get_tile_impl(state, room as c_int, ccr as c_int, ctr as c_int) == tiles_tiles_20_wall as c_int
                || tile_is_floor(*state.curr_tile2() as c_int) != 0
            {
                let clip_row = row + 1;
                let clip_y = y_clip_at(clip_row as usize);
                let oy = *state.obj_y();
                let cty = *state.char_top_y();
                if clip_row == 1 || (clip_y < oy as i16 && clip_y - 15 < cty) {
                    *state.char_top_y() = clip_y;
                    *state.obj_clip_top() = clip_y;
                }
            }
        }
        let cxlc = *state.char_x_left_coll() as c_int;
        let col = get_tile_div_mod_impl(state, cxlc - 4);
        if get_tile_impl(state, room as c_int, col + 1, row as c_int) == tiles_tiles_7_doortop_with_floor as c_int
            || *state.curr_tile2() == tiles_tiles_12_doortop as u8
        {
            // tile_col must be read here, AFTER the get_tile_impl call above: get_tile
            // mutates it as a side effect (via find_room_of_tile), and the original C
            // reads the post-call value, not the pre-call one.
            let tc = *state.tile_col();
            *state.obj_clip_right() = (tc << 5) + 32;
        } else if (get_tile_impl(state, room as c_int, col, row as c_int) != tiles_tiles_7_doortop_with_floor as c_int
                && *state.curr_tile2() != tiles_tiles_12_doortop as u8)
            || action == actions_actions_3_in_midair as u8
            || (action == actions_actions_4_in_freefall as u8 && frame == frameids_frame_106_fall as u8)
            || (action == actions_actions_5_bumped as u8 && frame == frameids_frame_107_fall_land_1 as u8)
            || (state.Char().direction < directions_dir_0_right as i8 && (
                action == actions_actions_2_hang_climb as u8
                || action == actions_actions_6_hang_straight as u8
                || (action == actions_actions_1_run_jump as u8
                    && frame >= frameids_frame_137_climbing_3 as u8
                    && frame < frameids_frame_140_climbing_6 as u8)
            ))
        {
            let cxrc = *state.char_x_right_coll() as c_int;
            let col2 = get_tile_div_mod_impl(state, cxrc);
            let ctr = *state.char_top_row();
            if (get_tile_impl(state, room as c_int, col2, row as c_int) == tiles_tiles_20_wall as c_int
                    || (*state.curr_tile2() == tiles_tiles_13_mirror as u8
                        && state.Char().direction == directions_dir_0_right as i8))
                && (get_tile_impl(state, room as c_int, col2, ctr as c_int) == tiles_tiles_20_wall as c_int
                    || *state.curr_tile2() == tiles_tiles_13_mirror as u8)
                && room as i16 == *state.curr_room()
            {
                // Same ordering requirement as above: tile_col must reflect the LAST
                // get_tile_impl call in this condition (the col2/ctr one), so read it
                // only here, not before the calls.
                let tc = *state.tile_col();
                *state.obj_clip_right() = tc << 5;
            }
        } else {
            let tc = *state.tile_col();
            *state.obj_clip_right() = (tc << 5) + 32;
        }
    }
}

/// Narrows the sprite clipping rectangle around the current character.
#[no_mangle]
pub unsafe extern "C" fn clip_char() {
    clip_char_impl(&mut State);
}

/// Sinks a character one pixel per frame into a "stuck" tile, the slow-sand
/// effect used where the Kid is meant to be held fast.
unsafe fn stuck_lower_impl(state: &mut State) {
    if get_tile_at_char_impl(state) == tiles_tiles_5_stuck as c_int {
        state.Char().y = state.Char().y.wrapping_add(1);
    }
}

/// Sinks the current character one pixel into a "stuck" tile.
#[no_mangle]
pub unsafe extern "C" fn stuck_lower() {
    stuck_lower_impl(&mut State);
}

/// Records which tile the current character's sprite belongs to, so the
/// renderer knows when to draw him relative to the scenery.
///
/// Normally that is the tile he stands on. Mid-jump it is the tile his sprite's
/// bottom-left corner falls in, and any pose that reaches upwards — climbing,
/// hanging, in mid-air — is attributed one column to the left, so the sprite
/// draws behind the tile it is reaching for rather than over it.
unsafe fn set_objtile_at_char_impl(state: &mut State) {
    let char_frame  = state.Char().frame;
    let char_action = state.Char().action;
    if char_action == actions_actions_1_run_jump as u8 {
        *state.tile_row() = *state.char_bottom_row();
        *state.tile_col() = *state.char_col_left();
    } else {
        *state.tile_row() = state.Char().curr_row as i16;
        *state.tile_col() = state.Char().curr_col as i16;
    }
    // Frames 135..148: climbing.
    if (frameids_frame_135_climbing_1 as u8..149).contains(&char_frame)
        || char_action == actions_actions_2_hang_climb as u8
        || char_action == actions_actions_3_in_midair as u8
        || char_action == actions_actions_4_in_freefall as u8
        || char_action == actions_actions_6_hang_straight as u8
    {
        *state.tile_col() -= 1;
    }
    let tc = *state.tile_col();
    let tr = *state.tile_row();
    *state.obj_tilepos() = get_tilepos_nominus(tc as c_int, tr as c_int) as u8;
}

/// Records which tile the current character's sprite belongs to.
#[no_mangle]
pub unsafe extern "C" fn set_objtile_at_char() {
    set_objtile_at_char_impl(&mut State);
}

/// Applies the effect of the object the Kid just picked up.
///
/// Reached from the `SEQ_GET_ITEM` opcode partway through the drinking
/// animation, so the potion takes effect on the gulp rather than on contact.
/// Type -1 is the sword; the rest are potions: heal one point, add a life,
/// feather fall, invert the screen, open every gate on the level, or hurt.
/// Only the Kid is affected at all — a guard walking over a potion ignores it.
unsafe fn proc_get_object_impl(state: &mut State) {
    if state.Char().charid != charids_charid_0_kid as u8 || *state.pickup_obj_type() == 0 { return; }
    if *state.pickup_obj_type() == -1 {
        *state.have_sword() = u16::MAX;
        play_sound(soundids_sound_37_victory as c_int);
        *state.flash_color() = colorids_color_14_brightyellow as u16;
        *state.flash_time() = 8;
    } else {
        match *state.pickup_obj_type() {
            1 => { // health
                if *state.hitp_curr() != *state.hitp_max() {
                    stop_sounds();
                    play_sound(soundids_sound_33_small_potion as c_int);
                    *state.hitp_delta() = 1;
                    *state.flash_color() = colorids_color_4_red as u16;
                    *state.flash_time() = 2;
                }
            }
            2 => { // life
                stop_sounds();
                play_sound(soundids_sound_30_big_potion as c_int);
                *state.flash_color() = colorids_color_4_red as u16;
                *state.flash_time() = 4;
                add_life();
            }
            3 => { // feather
                feather_fall();
            }
            4 => { // invert
                toggle_upside();
            }
            6 => { // open
                get_tile_impl(state, 8, 0, 0);
                trigger_button(0, 0, -1);
            }
            5 => { // hurt
                stop_sounds();
                play_sound(soundids_sound_13_kid_hurt as c_int);
                // Special event: the blue potions on the potions level take
                // half the Kid's maximum health rather than a single point.
                if *state.current_level() == 15 {
                    *state.hitp_delta() = -((*state.hitp_max() as i32 + 1) >> 1) as i16;
                } else {
                    *state.hitp_delta() = -1;
                }
            }
            _ => {}
        }
    }
}

/// Applies the effect of the object the Kid just picked up.
#[no_mangle]
pub unsafe extern "C" fn proc_get_object() {
    proc_get_object_impl(&mut State);
}

/// Whether the current character is in one of the three death poses: spiked
/// (177), chomped (178) or simply dead (185).
#[no_mangle]
pub unsafe extern "C" fn is_dead() -> c_int {
    (Char.frame >= frameids_frame_177_spiked as u8
        && (Char.frame <= frameids_frame_178_chomped as u8
            || Char.frame == frameids_frame_185_dead as u8)) as c_int
}

/// Plays the death music that fits how the Kid died: killed by the shadow,
/// killed in a sword fight, or killed by anything else.
unsafe fn play_death_music_impl(state: &mut State) {
    let sound_id: u32 = if state.Guard().charid == charids_charid_1_shadow as u8 {
        soundids_sound_32_shadow_music
    } else if *state.holding_sword() != 0 {
        soundids_sound_28_death_in_fight
    } else {
        soundids_sound_24_death_regular
    };
    play_sound(sound_id as c_int);
}

/// Plays the death music that fits how the Kid died.
#[no_mangle]
pub unsafe extern "C" fn play_death_music() {
    play_death_music_impl(&mut State);
}

/// Rewards the player for killing the current character's opponent.
///
/// Usually just the victory chime. On the demo level it instead sets
/// `checkpoint`, which is the demo's cue to run out of the room. On Jaffar's
/// level it flashes the screen white, starts the clock display and opens the
/// exit — Jaffar's death is what unlocks the level.
unsafe fn on_guard_killed_impl(state: &mut State) {
    if *state.current_level() == 0 {
        *state.checkpoint() = 1;
        *state.demo_index() = 0;
        *state.demo_time() = 0;
    } else if *state.current_level() == (*custom).jaffar_victory_level as u16 {
        *state.flash_color() = colorids_color_15_brightwhite as u16;
        *state.flash_time() = (*custom).jaffar_victory_flash_time as u16;
        *state.is_show_time() = 1;
        *state.leveldoor_open() = 2;
        play_sound(soundids_sound_43_victory_Jaffar as c_int);
    } else if state.Char().charid != charids_charid_1_shadow as u8 {
        play_sound(soundids_sound_37_victory as c_int);
    }
}

/// Rewards the player for killing the current character's opponent.
#[no_mangle]
pub unsafe extern "C" fn on_guard_killed() {
    on_guard_killed_impl(&mut State);
}

/// Removes the current character from the level: faces him nowhere, marks him
/// not alive, and erases his health meter.
unsafe fn clear_char_impl(state: &mut State) {
    state.Char().direction = directions_dir_56_none as i8;
    state.Char().alive     = 0;
    state.Char().action    = 0;
    draw_guard_hp(0, *state.guardhp_curr() as c_short);
    *state.guardhp_curr() = 0;
}

/// Removes the current character from the level.
#[no_mangle]
pub unsafe extern "C" fn clear_char() {
    clear_char_impl(&mut State);
}

/// Stashes the sprite-drawing globals so a caller can borrow them for an extra
/// sprite. Paired with [`load_obj_impl`]; only one level deep.
unsafe fn save_obj_impl(state: &mut State) {
    obj2_tilepos    = *state.obj_tilepos();
    obj2_x          = *state.obj_x() as u16;
    obj2_y          = *state.obj_y();
    obj2_direction  = *state.obj_direction();
    obj2_id         = *state.obj_id();
    obj2_chtab      = *state.obj_chtab();
    obj2_clip_top    = *state.obj_clip_top();
    obj2_clip_bottom = *state.obj_clip_bottom();
    obj2_clip_left   = *state.obj_clip_left();
    obj2_clip_right  = *state.obj_clip_right();
}

/// Stashes the sprite-drawing globals.
#[no_mangle]
pub unsafe extern "C" fn save_obj() {
    save_obj_impl(&mut State);
}

/// Restores the sprite-drawing globals stashed by [`save_obj_impl`].
unsafe fn load_obj_impl(state: &mut State) {
    *state.obj_tilepos()    = obj2_tilepos;
    *state.obj_x()          = obj2_x as i16;
    *state.obj_y()          = obj2_y;
    *state.obj_direction()  = obj2_direction;
    *state.obj_id()         = obj2_id;
    *state.obj_chtab()      = obj2_chtab;
    *state.obj_clip_top()    = obj2_clip_top;
    *state.obj_clip_bottom() = obj2_clip_bottom;
    *state.obj_clip_left()   = obj2_clip_left;
    *state.obj_clip_right()  = obj2_clip_right;
}

/// Restores the sprite-drawing globals.
#[no_mangle]
pub unsafe extern "C" fn load_obj() {
    load_obj_impl(&mut State);
}

/// Queues the splash of blood for a character who has just been hurt.
///
/// Where it goes depends on the injury: at his feet if he fell or is dead, in
/// front of him if he was spiked, and at chest height otherwise (a little
/// higher for the Kid, who is shorter than a guard). Being chomped has its own
/// animation and gets no splash. The drawing globals are borrowed and put back,
/// so the character's own sprite is unaffected.
unsafe fn draw_hurt_splash_impl(state: &mut State) {
    let frame = state.Char().frame;
    if frame == frameids_frame_178_chomped as u8 { return; }
    save_obj_impl(state);
    *state.obj_tilepos() = u8::MAX; // -1 as byte
    // Frame 185: dead. Frames 106..110: falling and landing.
    if frame == frameids_frame_185_dead as u8
        || (frameids_frame_106_fall as u8..111).contains(&frame)
    {
        *state.obj_y() = state.obj_y().wrapping_add(4);
        obj_dx_forward_impl(state, 5);
    } else if frame == frameids_frame_177_spiked as u8 {
        obj_dx_forward_impl(state, -5);
    } else {
        let oy = *state.obj_y();
        *state.obj_y() = (oy as i32 - ((state.Char().charid == charids_charid_0_kid as u8) as i32 * 4) - 11) as u8;
        obj_dx_forward_impl(state, 5);
    }
    if state.Char().charid == charids_charid_0_kid as u8 {
        *state.obj_chtab() = chtabs_id_chtab_2_kid as u8;
        *state.obj_id() = 218;
    } else {
        *state.obj_chtab() = chtabs_id_chtab_5_guard as u8;
        *state.obj_id() = 1;
    }
    reset_obj_clip_impl(state);
    add_objtable(5);
    load_obj_impl(state);
}

/// Queues the splash of blood for the current character.
#[no_mangle]
pub unsafe extern "C" fn draw_hurt_splash() {
    draw_hurt_splash_impl(&mut State);
}

/// Special event on level 12: killing the shadow kills the Kid too.
///
/// `Char.charid | Opp.charid` equals the shadow's id exactly when one of the
/// pair is the shadow (id 1) and the other is the Kid (id 0) — a compact way of
/// asking "is this the Kid and his shadow?".
unsafe fn check_killed_shadow_impl(state: &mut State) {
    if *state.current_level() == 12
        && (state.Char().charid | state.Opp().charid) == charids_charid_1_shadow as u8
        && state.Char().alive < 0
        && state.Opp().alive >= 0
    {
        *state.flash_color() = colorids_color_15_brightwhite as u16;
        *state.flash_time() = 5;
        take_hp_impl(state, 100);
    }
}

/// Special event on level 12: killing the shadow kills the Kid too.
#[no_mangle]
pub unsafe extern "C" fn check_killed_shadow() {
    check_killed_shadow_impl(&mut State);
}

/// Queues the current character's sword as a separate sprite, if this frame
/// shows one.
///
/// A sword is drawn while it is out, while it is being picked up or put away
/// (frames 229..237), and for a dead guard, who drops it. The frame's `sword`
/// field indexes [`SWORD_TBL`] for the sprite and its offset from the wielder;
/// 0 means this pose draws no sword and 255 means the pose already includes it.
unsafe fn add_sword_to_objtable_impl(state: &mut State) {
    let frame = state.Char().frame;
    if !((frameids_frame_229_found_sword as u8..238).contains(&frame)
        || state.Char().sword != sword_status_sword_0_sheathed as u8
        || (state.Char().charid == charids_charid_2_guard as u8 && state.Char().alive < 0))
    {
        return;
    }
    let sword_frame = (state.cur_frame().sword & 0x3F) as usize;
    if sword_frame == 0 { return; }
    *state.obj_id() = SWORD_TBL[sword_frame].id;
    if *state.obj_id() == 0xFF { return; }
    let ox = *state.obj_x();
    *state.obj_x() = calc_screen_x_coord(ox);
    obj_dx_forward_impl(state, SWORD_TBL[sword_frame].x as c_int);
    let oy = *state.obj_y();
    *state.obj_y() = (oy as i32 + SWORD_TBL[sword_frame].y as i32) as u8;
    *state.obj_chtab() = chtabs_id_chtab_0_sword as u8;
    add_objtable(3);
}

/// Queues the current character's sword as a separate sprite.
#[no_mangle]
pub unsafe extern "C" fn add_sword_to_objtable() {
    add_sword_to_objtable_impl(&mut State);
}

/// Lets an inactive guard react: with the down arrow held, he either draws his
/// sword (if forward is held too) or turns around to face the other way.
unsafe fn control_guard_inactive_impl(state: &mut State) {
    if state.Char().frame == frameids_frame_166_stand_inactive as u8
        && *state.control_down() == CONTROL_HELD as i8
    {
        if *state.control_forward() == CONTROL_HELD as i8 {
            draw_sword();
        } else {
            *state.control_down() = CONTROL_IGNORE as i8;
            seqtbl_offset_char(seqids_seq_80_stand_flipped as c_short);
        }
    }
}

/// Lets an inactive guard draw his sword or turn around.
#[no_mangle]
pub unsafe extern "C" fn control_guard_inactive() {
    control_guard_inactive_impl(&mut State);
}

/// Signed distance from the current character to his opponent: positive if the
/// opponent is in front of him, negative if behind, and 999 if they are not
/// even in the same room.
///
/// When the two face the same way — one has his back to the other — 13 units
/// are added, because the distance then has to cover a whole extra body width
/// before they are actually within reach.
unsafe fn char_opp_dist_impl(state: &mut State) -> c_int {
    if state.Char().room != state.Opp().room {
        return 999;
    }
    let mut distance = state.Opp().x as i16 - state.Char().x as i16;
    if state.Char().direction < directions_dir_0_right as i8 {
        distance = -distance;
    }
    if distance >= 0 && state.Char().direction != state.Opp().direction {
        distance += 13;
    }
    distance as c_int
}

/// Signed distance from the current character to his opponent.
#[no_mangle]
pub unsafe extern "C" fn char_opp_dist() -> c_int {
    char_opp_dist_impl(&mut State)
}

/// Moves the current character down one tile row.
unsafe fn inc_curr_row_impl(state: &mut State) {
    state.Char().curr_row += 1;
}

/// Moves the current character down one tile row.
#[no_mangle]
pub unsafe extern "C" fn inc_curr_row() {
    inc_curr_row_impl(&mut State);
}

#[cfg(test)]
#[allow(static_mut_refs)]
mod tests {
    use super::*;
    use std::os::raw::c_int;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // TILE_DIV_TBL and TILE_MOD_TBL each cover the full 0-255 byte range used
    // by the DOS version's tile_div_tbl/tile_mod_tbl.  The frame tables are
    // indexed directly by frame number so wrong sizes silently truncate
    // animations.  SWORD_TBL has one entry per sword frame (0-50).
    #[test]
    fn table_sizes_are_correct() {
        assert_eq!(TILE_DIV_TBL.len(), 256);
        assert_eq!(TILE_MOD_TBL.len(), 256);
        assert_eq!(FRAME_TABLE_KID.len(), 241);
        assert_eq!(FRAME_TBL_GUARD.len(),  41);
        assert_eq!(SWORD_TBL.len(),        51);
    }

    // get_tile_div_mod converts a pixel x-position into a tile column (return
    // value) and a sub-tile pixel offset stored in obj_xl (0..13).
    // The screen coordinate origin is SCREENSPACE_X=58; tiles are 14 px wide.
    #[test]
    fn get_tile_div_mod_column_and_offset() {
        // (xpos, expected_col, expected_obj_xl)
        let cases: &[(c_int, c_int, u8)] = &[
            (58,   0,  0),  // leftmost pixel of column 0
            (65,   0,  7),  // mid-column 0 (65-58=7)
            (71,   0, 13),  // rightmost pixel of column 0
            (72,   1,  0),  // leftmost pixel of column 1
            (100,  3,  0),  // (100-58)=42, 42/14=3 rem 0
            (101,  3,  1),  // offset 1 within column 3
            (226, 12,  0),  // (226-58)=168, 168/14=12
            (44,  -1,  0),  // (44-58)=-14, column -1 (off-screen left)
            (30,  -2,  0),  // (30-58)=-28, column -2
        ];
        unsafe {
            for &(xpos, want_col, want_xl) in cases {
                let col = get_tile_div_mod(xpos);
                assert_eq!(col,    want_col, "xpos={xpos}: column");
                assert_eq!(obj_xl, want_xl,  "xpos={xpos}: obj_xl");
            }
        }
    }

    // y_to_row_mod4 maps a pixel y-position to a tile row in 0..2, or -1 for
    // positions above the room.  Anchored at the exact y_land[] floor values
    // { -8, 55, 118, 181 } which correspond to rows -1, 0, 1, 2.
    #[test]
    fn y_to_row_mod4_at_floor_positions() {
        // (ypos, expected_row)
        let cases: &[(c_int, c_int)] = &[
            ( -8, -1),  // above row 0 (y_land[0])
            ( 55,  0),  // row 0 floor (y_land[1])
            (118,  1),  // row 1 floor (y_land[2])
            (181,  2),  // row 2 floor (y_land[3])
        ];
        unsafe {
            for &(ypos, want) in cases {
                assert_eq!(y_to_row_mod4(ypos), want, "ypos={ypos}");
            }
        }
    }

    // tile_is_floor returns 0 for the eight tile types that have no walkable
    // surface (empty, big-pillar top, door top, wall, four lattice variants)
    // and 1 for everything else.
    #[test]
    fn tile_is_floor_classification() {
        unsafe {
            let non_floor = [
                (tiles_tiles_0_empty         as c_int, "empty"),
                (tiles_tiles_9_bigpillar_top as c_int, "bigpillar_top"),
                (tiles_tiles_12_doortop      as c_int, "doortop"),
                (tiles_tiles_20_wall         as c_int, "wall"),
                (tiles_tiles_26_lattice_down  as c_int, "lattice_down"),
                (tiles_tiles_27_lattice_small as c_int, "lattice_small"),
                (tiles_tiles_28_lattice_left  as c_int, "lattice_left"),
                (tiles_tiles_29_lattice_right as c_int, "lattice_right"),
            ];
            for (t, name) in non_floor {
                assert_eq!(tile_is_floor(t), 0, "{name}");
            }
            let floor = [
                (tiles_tiles_1_floor  as c_int, "floor"),
                (tiles_tiles_2_spike  as c_int, "spike"),
                (tiles_tiles_3_pillar as c_int, "pillar"),
            ];
            for (t, name) in floor {
                assert_eq!(tile_is_floor(t), 1, "{name}");
            }
        }
    }

    // get_tilepos maps (col, row) to a flat tile index in 0..29.
    // Row r begins at r*10 (tbl_line = {0, 10, 20}).
    // Negative rows return -(col+1) as an "above room" sentinel.
    // Any out-of-bounds coord (col<0, col>=10, row>=3) returns 30.
    #[test]
    fn get_tilepos_normal_and_boundary() {
        // (col, row, expected)
        let cases: &[(c_int, c_int, c_int)] = &[
            ( 0,  0,  0),   // top-left
            ( 9,  0,  9),   // top-right
            ( 0,  1, 10),   // row 1 start
            ( 5,  1, 15),
            ( 0,  2, 20),   // row 2 start
            ( 9,  2, 29),   // bottom-right
            ( 0, -1, -1),   // above row 0: -(0+1)
            ( 5, -1, -6),   // above row 0: -(5+1)
            (-1,  0, 30),   // left OOB
            (10,  0, 30),   // right OOB
            ( 0,  3, 30),   // below last row
        ];
        unsafe {
            for &(col, row, want) in cases {
                assert_eq!(get_tilepos(col, row), want, "col={col} row={row}");
            }
        }
    }

    // char_dx_forward adds delta_x to Char.x, negating when facing left.
    // The result is an i32 pixel position (not wrapped to u8).
    #[test]
    fn char_dx_forward_right_and_left() {
        // (direction, char_x, delta, expected)
        let cases: &[(i8, u8, c_int, c_int)] = &[
            (directions_dir_0_right as i8, 100,  5, 105),
            (directions_dir_0_right as i8, 100, -3,  97),
            (directions_dir_FF_left as i8, 100,  5,  95),
            (directions_dir_FF_left as i8, 100, -3, 103),
        ];
        unsafe {
            for &(dir, x, delta, want) in cases {
                Char.direction = dir;
                Char.x = x;
                assert_eq!(char_dx_forward(delta), want, "dir={dir} x={x} delta={delta}");
            }
        }
    }

    // load_frame dispatches to FRAME_TABLE_KID for kid/mouse/shadow and to
    // FRAME_TBL_GUARD for guard/skeleton (with frame -= 149).
    // Kid frame 7 is the first running step: FRAME_TABLE_KID[7] = ft(6,0,0,0,0x4A).
    // Guard frame 150 → FRAME_TBL_GUARD[1]                       = ft(12,0xCD,2,1,0).
    #[test]
    fn load_frame_dispatches_by_charid() {
        unsafe {
            // Kid frame 7
            Char.charid = charids_charid_0_kid as u8;
            Char.frame  = 7;
            load_frame();
            assert_eq!(cur_frame.image,  6,    "kid7: image");
            assert_eq!(cur_frame.dx,     0,    "kid7: dx");
            assert_eq!(cur_frame.flags, 0x4A,  "kid7: flags");

            // Guard frame 150 → index 1 in FRAME_TBL_GUARD
            Char.charid = charids_charid_2_guard as u8;
            Char.frame  = 150;
            load_frame();
            assert_eq!(cur_frame.image, 12,    "guard150: image");
            assert_eq!(cur_frame.dx,     2,    "guard150: dx");
            assert_eq!(cur_frame.dy,     1,    "guard150: dy");

            // Out-of-bounds frame → sentinel image=255
            Char.charid = charids_charid_0_kid as u8;
            Char.frame  = 255;
            load_frame();
            assert_eq!(cur_frame.image, 255, "oob frame → sentinel");
        }
    }

    // fall_accel increments Char.fall_y by FALLING_SPEED_ACCEL (3) each tick
    // while in freefall, capping at FALLING_SPEED_MAX (33).  With feather fall
    // active the increment is 1 and the cap is 4.  Outside freefall: no change.
    #[test]
    fn fall_accel_normal_and_feather() {
        setup();
        unsafe {
            Char.charid = charids_charid_0_kid as u8;

            // Not in freefall → no change
            Char.action = actions_actions_0_stand as u8;
            Char.fall_y = 5;
            fall_accel();
            assert_eq!(Char.fall_y, 5, "stand: fall_y unchanged");

            // Normal freefall: +3 per tick
            Char.action = actions_actions_4_in_freefall as u8;
            is_feather_fall = 0;
            Char.fall_y = 0;
            fall_accel();
            assert_eq!(Char.fall_y, 3, "normal: +3 on first tick");

            // Clamp at 33 (31+3=34 → 33)
            Char.fall_y = 31;
            fall_accel();
            assert_eq!(Char.fall_y, 33, "normal: clamped at 33");
            fall_accel();
            assert_eq!(Char.fall_y, 33, "normal: stays at 33");

            // Feather fall: +1 per tick, cap 4
            is_feather_fall = 1;
            Char.fall_y = 0;
            fall_accel();
            assert_eq!(Char.fall_y, 1, "feather: +1 on first tick");
            Char.fall_y = 4;
            fall_accel();
            assert_eq!(Char.fall_y, 4, "feather: clamped at 4");
        }
    }
}
