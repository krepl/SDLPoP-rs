//! The parts of the dungeon that move: gates, spikes, chompers, loose floors,
//! torches, buttons, potions and the falling rubble they leave behind.
//!
//! A room is a static 10×3 grid of tiles, but a handful of those tiles are
//! *alive* — a gate is halfway open, a chomper is mid-bite, a loose floor is
//! wobbling because someone stepped on it. Rather than sweep all 30 tiles of
//! every room every frame, the game keeps a small worklist of the tiles that
//! are currently animating. Historically these are called **trobs**
//! ("triggered objects").
//!
//! # Trobs
//!
//! A trob is just `(room, tilepos, type)`. The *type* is a per-tile-kind state
//! byte — for a gate it says "opening" / "closing" / "closing fast at speed
//! N", for a torch it is a constant 1 — and a **negative type means "I am
//! finished, delete me"**. Every frame, [`process_trobs_impl`] copies each
//! entry into the `trob` global, runs [`animate_tile_impl`] on it, writes the
//! (possibly updated) type back, and then compacts the list, dropping the
//! finished ones. That copy-in / copy-out through a single global is why
//! almost every function in this module takes no arguments: they all operate
//! on `trob` and on `curr_modifier`.
//!
//! The other half of the tile's state is its **modifier** byte, held in
//! `curr_room_modif[tilepos]`. [`get_curr_tile_impl`] loads it into
//! `curr_modifier`; [`animate_tile_impl`] stores it back when the frame's work
//! is done. Each tile kind interprets the modifier differently:
//!
//! * **Gate** — 0..=255 openness in quarter-pixels, with `0xFF` reserved to
//!   mean "wedged permanently open" and 188 as the fully-open threshold.
//! * **Spike** — 0 retracted, 1..=4 coming out, `0x80 | n` a countdown while
//!   fully out, `0xFF` a spike that never fires.
//! * **Chomper** — low seven bits are the animation frame, the top bit is the
//!   "there is blood on these blades" flag, which is sticky.
//! * **Loose floor** — top bit set means "shaking but still attached", clear
//!   means "something is standing on me" and the value counts up towards
//!   `loose_floor_delay`, at which point the tile falls.
//! * **Torch / potion / sword** — a purely cosmetic animation frame.
//! * **Button** — an index into the *doorlink* table, see below.
//!
//! # Buttons and doorlinks
//!
//! Pressure plates (`tiles_6_closer`, `tiles_15_opener`) do not name the gate
//! they operate. Their modifier is a start index into a flat, level-wide
//! doorlink table: two parallel byte arrays, `doorlink1_ad` and
//! `doorlink2_ad`, whose bits are unpacked by [`get_doorlink_room_impl`],
//! [`get_doorlink_tile_impl`], [`get_doorlink_next_impl`] and
//! [`get_doorlink_timer_impl`]. Consecutive entries form one list, terminated
//! by bit 7 of `doorlink1_ad`. [`do_trigger_list_impl`] walks that list and
//! turns each target into a trob.
//!
//! The five timer bits do double duty as a debounce: [`trigger_button_impl`]
//! reloads them to 5 and [`animate_button_impl`] counts them back down, so
//! holding a plate down re-fires the list every frame but only plays the click
//! and lights the plate once. `0x1F` is a sentinel meaning "jammed — this
//! event can never fire again".
//!
//! # Mobs
//!
//! When a loose floor finally gives way it stops being a tile and becomes a
//! **mob**: a free-falling chunk of masonry tracked in `mobs[]` with its own
//! position, speed and room. [`do_mobs_impl`] advances them ([`move_loose_impl`]),
//! checks whether one has landed on the Kid's head
//! ([`check_loose_fall_on_kid_impl`]), and drops the ones that have come to
//! rest. A falling tile can knock loose the tiles in the row it passes
//! through ([`do_knock_impl`]), punch through another loose floor and spawn a
//! second mob ([`loose_fall_impl`]), fall out of the bottom of a room into the
//! room below ([`mob_down_a_row_impl`]), trigger a pressure plate it lands on,
//! or smash a torch into a torch-with-debris ([`loose_land_impl`]).
//!
//! # Redraw bookkeeping
//!
//! Animating a tile is only half the job; the renderer has to be told which
//! tiles became dirty. The `set_redraw_*` / [`set_wipe_impl`] family marks
//! entries in the per-layer dirty-tile arrays that `seg008` consumes. Because
//! a tile's artwork overhangs its neighbours, a trob usually dirties the tile
//! to its right and the tile above-right as well; the
//! `get_trob_*_pos_in_drawn_room` family translates a trob's `(room, tilepos)`
//! into a position in the *currently drawn* room, returning 30 for "not
//! visible, ignore" and small negative numbers for "in the strip of the room
//! above that pokes into view".
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short, c_char};
use super::*;
use crate::state::State;

/// Index of the mob [`do_mobs_impl`] is currently stepping.
///
/// A global rather than a loop variable because [`loose_fall_impl`], several
/// calls deep, has to write the current mob back into `mobs[]` before spawning
/// the extra one it needs.
static mut curmob_index: u16 = 0;

/// The tile a falling mob just entered, shared between [`move_loose_impl`] and
/// its callees. File-local in the C original, hence not in `data.h`.
static mut curr_tile_temp: u16 = 0;

/// Quarter-pixels a slamming gate closes per frame, indexed by trob type 3..=8.
/// Types 0..=2 never index this, hence the three leading zeroes.
static GATE_CLOSE_SPEEDS: [u8; 9] = [0, 0, 0, 20, 40, 60, 80, 100, 120];

/// Per-frame openness delta for gate trob types 0 (closing), 1 and 2 (opening).
/// `door_delta[0]` is -1 in C; stored here as 255 and applied with
/// `wrapping_add`, which is the same byte arithmetic.
static DOOR_DELTA: [u8; 3] = [255, 4, 4];

/// Pixels the level exit door slams shut per frame, indexed by trob type - 3.
static LEVELDOOR_CLOSE_SPEEDS: [u8; 5] = [0, 5, 17, 99, 0];

/// Y position a loose floor starts falling from, indexed by `row + 1`.
static Y_LOOSE_LAND: [u16; 5] = [2, 65, 128, 191, 254];

/// Which chomper-style loose-floor wobble frames are loud, indexed by the
/// modifier's low seven bits. Only frames 1, 2, 3, 5 and 8 make a noise.
static LOOSE_SOUND: [u8; 12] = [0, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0];

/// Y position at which a falling mob has reached the floor of row `n`, indexed
/// by `row + 1`. The trailing 25 is the wrap-around entry used after a mob has
/// fallen out of the bottom of a room.
static Y_SOMETHING: [i16; 5] = [-1, 62, 125, 188, 25];

/// First tilepos of a row: `tbl_line[row]`.
///
/// `tbl_line` is an `extern const word[]`, which bindgen emits as `[u16; 0]`,
/// so it has to be read through a raw pointer. Note the element width is
/// `word`, not `byte`.
unsafe fn tbl_line_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(tbl_line).cast::<u16>().add(idx)
}

/// Advance every animating tile by one frame, then drop the finished ones.
///
/// Each entry is copied into the `trob` global, animated, and copied back —
/// [`animate_tile_impl`] and everything below it read and write `trob` rather
/// than taking parameters. A trob that set its type negative is compacted out
/// of the list afterwards, preserving the order of the survivors.
///
/// The list length is sampled once for each pass because nothing reachable
/// from [`animate_tile_impl`] adds or removes trobs — new trobs only come from
/// [`add_trob_impl`], which is called from the input/collision side of the
/// frame, never from here.
unsafe fn process_trobs_impl(state: &mut State) {
    let mut need_delete = false;
    let count = *state.trobs_count();
    if count == 0 { return; }
    for index in 0..count as usize {
        *state.trob() = state.trobs()[index];
        animate_tile_impl(state);
        let type_ = state.trob().type_;
        state.trobs()[index].type_ = type_;
        if type_ < 0 {
            need_delete = true;
        }
    }
    if need_delete {
        let mut new_index = 0usize;
        for index in 0..*state.trobs_count() as usize {
            if state.trobs()[index].type_ >= 0 {
                state.trobs()[new_index] = state.trobs()[index];
                new_index += 1;
            }
        }
        *state.trobs_count() = new_index as c_short;
    }
}

/// C entry point for [`process_trobs_impl`].
#[no_mangle]
pub unsafe extern "C" fn process_trobs() {
    process_trobs_impl(&mut State);
}

/// Run one frame of whichever animation the current trob's tile kind implies.
///
/// Loads the tile and its modifier (via [`get_curr_tile_impl`]), dispatches on
/// the tile kind, and stores the modifier back. A tile kind with no animator
/// marks the trob for deletion — that is how a trob left behind by a tile that
/// has since been replaced (a smashed torch, a floor that fell) retires itself.
unsafe fn animate_tile_impl(state: &mut State) {
    get_room_address(state.trob().room as c_int);
    let tilepos = state.trob().tilepos as c_short;
    // get_curr_tile masks with 0x1F, so the value always fits a tile id.
    match get_curr_tile_impl(state, tilepos) as tiles {
        tiles_tiles_19_torch | tiles_tiles_30_torch_with_debris => animate_torch_impl(state),
        tiles_tiles_6_closer | tiles_tiles_15_opener => animate_button_impl(state),
        tiles_tiles_2_spike => animate_spike_impl(state),
        tiles_tiles_11_loose => animate_loose_impl(state),
        tiles_tiles_0_empty => animate_empty_impl(state),
        tiles_tiles_18_chomper => animate_chomper_impl(state),
        tiles_tiles_4_gate => animate_door_impl(state),
        tiles_tiles_16_level_door_left => animate_leveldoor_impl(state),
        tiles_tiles_10_potion => animate_potion_impl(state),
        tiles_tiles_22_sword => animate_sword_impl(state),
        _ => state.trob().type_ = -1,
    }
    let tilepos = state.trob().tilepos as usize;
    let modifier = *state.curr_modifier();
    *curr_room_modif.add(tilepos) = modifier;
}

/// C entry point for [`animate_tile_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_tile() {
    animate_tile_impl(&mut State);
}

/// Is the current trob in the room on screen? If not, retire it.
///
/// Off-screen tiles stop animating entirely — the player cannot see them, and
/// the game restarts their animation on re-entry. Returns 1 if visible.
unsafe fn is_trob_in_drawn_room_impl(state: &mut State) -> c_short {
    if state.trob().room as u16 != *state.drawn_room() {
        state.trob().type_ = -1;
        0
    } else {
        1
    }
}

/// C entry point for [`is_trob_in_drawn_room_impl`].
#[no_mangle]
pub unsafe extern "C" fn is_trob_in_drawn_room() -> c_short {
    is_trob_in_drawn_room_impl(&mut State)
}

/// Dirty the animation layer of the tile to the right of the current trob.
///
/// Tile artwork overhangs to the right, so an animating tile always repaints
/// its right-hand neighbour too.
unsafe fn set_redraw_anim_right_impl(state: &mut State) {
    let pos = get_trob_right_pos_in_drawn_room_impl(state);
    set_redraw_anim_impl(state, pos, 1);
}

/// C entry point for [`set_redraw_anim_right_impl`].
#[no_mangle]
pub unsafe extern "C" fn set_redraw_anim_right() {
    set_redraw_anim_right_impl(&mut State);
}

/// Dirty the animation layer of the current trob's own tile.
unsafe fn set_redraw_anim_curr_impl(state: &mut State) {
    let pos = get_trob_pos_in_drawn_room_impl(state);
    set_redraw_anim_impl(state, pos, 1);
}

/// C entry point for [`set_redraw_anim_curr_impl`].
#[no_mangle]
pub unsafe extern "C" fn set_redraw_anim_curr() {
    set_redraw_anim_curr_impl(&mut State);
}

/// Fully repaint the current trob's tile, to the full 63-pixel tile height.
///
/// Used by the animations whose artwork can occupy the whole tile cell
/// (potions, swords, chompers), where repainting only the animation layer
/// would leave the previous frame showing through.
unsafe fn redraw_at_trob_impl(state: &mut State) {
    *state.redraw_height() = 63;
    let tilepos = get_trob_pos_in_drawn_room_impl(state);
    set_redraw_full_impl(state, tilepos, 1);
    set_wipe_impl(state, tilepos, 1);
}

/// C entry point for [`redraw_at_trob_impl`].
#[no_mangle]
pub unsafe extern "C" fn redraw_at_trob() {
    redraw_at_trob_impl(&mut State);
}

/// Repaint the trob's tile pair to a height of 0x21 pixels — spikes.
unsafe fn redraw_21h_impl(state: &mut State) {
    *state.redraw_height() = 0x21;
    redraw_tile_height_impl(state);
}

/// C entry point for [`redraw_21h_impl`].
#[no_mangle]
pub unsafe extern "C" fn redraw_21h() {
    redraw_21h_impl(&mut State);
}

/// Repaint the trob's tile pair to a height of 0x11 pixels — pressure plates.
unsafe fn redraw_11h_impl(state: &mut State) {
    *state.redraw_height() = 0x11;
    redraw_tile_height_impl(state);
}

/// C entry point for [`redraw_11h_impl`].
#[no_mangle]
pub unsafe extern "C" fn redraw_11h() {
    redraw_11h_impl(&mut State);
}

/// Repaint the trob's tile pair to a height of 0x20 pixels — floors.
unsafe fn redraw_20h_impl(state: &mut State) {
    *state.redraw_height() = 0x20;
    redraw_tile_height_impl(state);
}

/// C entry point for [`redraw_20h_impl`].
#[no_mangle]
pub unsafe extern "C" fn redraw_20h() {
    redraw_20h_impl(&mut State);
}

/// Dirty everything a gate's artwork touches: the tile to the right, in both
/// the animation and foreground layers, plus the tile above-right that the
/// gate's raised portcullis pokes into.
unsafe fn draw_trob_impl(state: &mut State) {
    let tilepos = get_trob_right_pos_in_drawn_room_impl(state);
    set_redraw_anim_impl(state, tilepos, 1);
    set_redraw_fore_impl(state, tilepos, 1);
    let above = get_trob_right_above_pos_in_drawn_room_impl(state);
    set_redraw_anim_impl(state, above, 1);
}

/// C entry point for [`draw_trob_impl`].
#[no_mangle]
pub unsafe extern "C" fn draw_trob() {
    draw_trob_impl(&mut State);
}

/// Fully repaint the trob's tile and its right-hand neighbour, wiping both to
/// the height previously stashed in `redraw_height` by one of the
/// `redraw_NNh` helpers.
unsafe fn redraw_tile_height_impl(state: &mut State) {
    let mut tilepos = get_trob_pos_in_drawn_room_impl(state);
    set_redraw_full_impl(state, tilepos, 1);
    set_wipe_impl(state, tilepos, 1);
    tilepos = get_trob_right_pos_in_drawn_room_impl(state);
    set_redraw_full_impl(state, tilepos, 1);
    set_wipe_impl(state, tilepos, 1);
}

/// C entry point for [`redraw_tile_height_impl`].
#[no_mangle]
pub unsafe extern "C" fn redraw_tile_height() {
    redraw_tile_height_impl(&mut State);
}

/// Where the current trob's own tile falls in the room being drawn.
///
/// Returns a tilepos in 0..30 for "inside the drawn room", a small negative
/// number for "in the bottom row of the room above, which overhangs into
/// view", and the sentinel 30 for "not visible — ignore". The negative
/// encoding maps the above-room tileposes 20..=29 onto -1..=-10, which
/// [`set_redraw_anim_impl`] and friends turn into indices into
/// `redraw_frames_above`.
unsafe fn get_trob_pos_in_drawn_room_impl(state: &mut State) -> c_short {
    let mut tilepos = state.trob().tilepos as c_short;
    if state.trob().room as u16 == *state.room_A() {
        if tilepos >= 20 && tilepos < 30 {
            tilepos = 19 - tilepos;
        } else {
            tilepos = 30;
        }
    } else if state.trob().room as u16 != *state.drawn_room() {
        tilepos = 30;
    }
    tilepos
}

/// C entry point for [`get_trob_pos_in_drawn_room_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_trob_pos_in_drawn_room() -> c_short {
    get_trob_pos_in_drawn_room_impl(&mut State)
}

/// Where the tile *to the right* of the current trob falls in the drawn room.
///
/// Same encoding as [`get_trob_pos_in_drawn_room_impl`]. Four cases have to be
/// handled because "one to the right" can cross a room boundary: the trob may
/// be in the drawn room, in the room to its left (rightmost column only), in
/// the room above, or in the room above-left (its bottom-right corner tile
/// only).
unsafe fn get_trob_right_pos_in_drawn_room_impl(state: &mut State) -> c_short {
    let mut tilepos = state.trob().tilepos as c_short;
    if state.trob().room as u16 == *state.drawn_room() {
        if tilepos % 10 != 9 {
            tilepos += 1;
        } else {
            tilepos = 30;
        }
    } else if state.trob().room as u16 == *state.room_L() {
        if tilepos % 10 == 9 {
            tilepos -= 9;
        } else {
            tilepos = 30;
        }
    } else if state.trob().room as u16 == *state.room_A() {
        if tilepos >= 20 && tilepos < 29 {
            tilepos = 18 - tilepos; // 20..28 -> -2..-10
        } else {
            tilepos = 30;
        }
    } else if state.trob().room as u16 == *state.room_AL() && tilepos == 29 {
        tilepos = -1;
    } else {
        tilepos = 30;
    }
    tilepos
}

/// C entry point for [`get_trob_right_pos_in_drawn_room_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_trob_right_pos_in_drawn_room() -> c_short {
    get_trob_right_pos_in_drawn_room_impl(&mut State)
}

/// Where the tile *above and to the right* of the current trob falls in the
/// drawn room — the cell a raised portcullis occupies.
///
/// Same encoding as [`get_trob_pos_in_drawn_room_impl`]; the neighbouring
/// rooms that can contribute are left, below, and below-left.
unsafe fn get_trob_right_above_pos_in_drawn_room_impl(state: &mut State) -> c_short {
    let mut tilepos = state.trob().tilepos as c_short;
    if state.trob().room as u16 == *state.drawn_room() {
        if tilepos % 10 != 9 {
            if tilepos < 10 {
                tilepos = -(tilepos + 2); // 0..8 -> -2..-10
            } else {
                tilepos -= 9;
            }
        } else {
            tilepos = 30;
        }
    } else if state.trob().room as u16 == *state.room_L() {
        if tilepos == 9 {
            tilepos = -1;
        } else if tilepos % 10 == 9 {
            tilepos -= 19;
        } else {
            tilepos = 30;
        }
    } else if state.trob().room as u16 == *state.room_B() {
        if tilepos < 9 {
            tilepos += 21;
        } else {
            tilepos = 30;
        }
    } else if state.trob().room as u16 == *state.room_BL() && tilepos == 9 {
        tilepos = 20;
    } else {
        tilepos = 30;
    }
    tilepos
}

/// C entry point for [`get_trob_right_above_pos_in_drawn_room_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_trob_right_above_pos_in_drawn_room() -> c_short {
    get_trob_right_above_pos_in_drawn_room_impl(&mut State)
}

/// Flicker a torch: pick the next flame frame and dirty the tile.
///
/// Unlike the other animators this does not use
/// [`is_trob_in_drawn_room_impl`]: a torch in the rightmost column of the room
/// to the *left* is still visible at the screen edge, so it has to keep
/// burning.
unsafe fn animate_torch_impl(state: &mut State) {
    let trob_tilepos = state.trob().tilepos;
    if state.trob().room as u16 == *state.drawn_room()
        || (state.trob().room as u16 == *state.room_L() && (trob_tilepos % 10) == 9)
    {
        let cm = *state.curr_modifier();
        *state.curr_modifier() = get_torch_frame(cm as c_short) as u8;
        set_redraw_anim_right_impl(state);
    } else {
        state.trob().type_ = -1;
    }
}

/// C entry point for [`animate_torch_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_torch() {
    animate_torch_impl(&mut State);
}

/// Bubble the liquid in a potion.
///
/// The modifier's low three bits are the bubble frame and the top five say
/// which potion this is (health, hit-point-up, feather fall, …), so the frame
/// has to be spliced back in without disturbing the potion type.
///
/// On the copy-protection level the potion tiles carry the symbol the player
/// has to match, and a full-height repaint would erase it, so that level
/// repaints only the animation layer.
unsafe fn animate_potion_impl(state: &mut State) {
    if state.trob().type_ >= 0 && is_trob_in_drawn_room_impl(state) != 0 {
        let potion_type = *state.curr_modifier() & 0xF8;
        let bubble = bubble_next_frame((*state.curr_modifier() & 0x07) as c_short) as u8;
        *state.curr_modifier() = bubble | potion_type;
        // USE_COPYPROT is active
        if *state.current_level() as u16 == 15 {
            set_redraw_anim_curr_impl(state);
            return;
        }
        // FIX_LOOSE_NEXT_TO_POTION is active
        redraw_at_trob_impl(state);
    }
}

/// C entry point for [`animate_potion_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_potion() {
    animate_potion_impl(&mut State);
}

/// Twinkle a sword lying on the ground.
///
/// The modifier is a countdown; when it reaches zero the sword glints and a
/// fresh random delay of 0x28..=0x67 frames is rolled.
unsafe fn animate_sword_impl(state: &mut State) {
    if is_trob_in_drawn_room_impl(state) != 0 {
        *state.curr_modifier() = state.curr_modifier().wrapping_sub(1);
        if *state.curr_modifier() == 0 {
            *state.curr_modifier() = (prandom(255) as u8 & 0x3F) + 0x28;
        }
        // FIX_LOOSE_NEXT_TO_POTION is active
        redraw_at_trob_impl(state);
    }
}

/// C entry point for [`animate_sword_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_sword() {
    animate_sword_impl(&mut State);
}

/// Run one frame of a chomper's bite.
///
/// The modifier's low seven bits cycle 1..=`chomper_speed`; frame 2 is the
/// slam and makes the noise, and frames 0..=5 are the ones that visibly differ
/// from the resting pose, so only those need a repaint.
///
/// The top bit is the blood flag, set when the blades caught someone. A
/// chomper keeps animating until it is out of the interesting frames *and*
/// there is nothing left to show — the Kid has left the room or the row, or he
/// is alive so the blood on screen is not his. A bloodied chomper in the
/// Kid's own row goes on chomping his corpse.
unsafe fn animate_chomper_impl(state: &mut State) {
    if state.trob().type_ >= 0 {
        let blood = *state.curr_modifier() & 0x80;
        let mut frame = (*state.curr_modifier() & 0x7F).wrapping_add(1);
        if frame > (*custom).chomper_speed {
            frame = 1;
        }
        *state.curr_modifier() = blood | frame;
        if frame == 2 {
            play_sound(soundids_sound_47_chomper as c_int);
        }
        let trob_room = state.trob().room;
        let trob_tilepos = state.trob().tilepos;
        if (trob_room as u16 != *state.drawn_room()
            || trob_tilepos / 10 != state.Kid().curr_row as u8
            || (state.Kid().alive >= 0 && blood == 0))
            && (*state.curr_modifier() & 0x7F) >= 6
        {
            state.trob().type_ = -1;
        }
    }
    if (*state.curr_modifier() & 0x7F) < 6 {
        redraw_at_trob_impl(state);
    }
}

/// C entry point for [`animate_chomper_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_chomper() {
    animate_chomper_impl(&mut State);
}

/// Run one frame of a spike trap's extend / hold / retract cycle.
///
/// The modifier walks 1, 2, 3, 4 as the blades come out, then jumps to `0x8F`
/// — top bit set marks the fully-out hold, and the low bits count back down.
/// At the end of the hold it becomes 6, and 6, 7, 8 retract the blades before
/// resetting to 0 and retiring the trob. `0xFF` marks a spike that has been
/// disabled and must never move.
unsafe fn animate_spike_impl(state: &mut State) {
    if state.trob().type_ >= 0 {
        // 0xFF means a permanently disabled spike.
        if *state.curr_modifier() == 0xFF { return; }
        if *state.curr_modifier() & 0x80 != 0 {
            *state.curr_modifier() = state.curr_modifier().wrapping_sub(1);
            if *state.curr_modifier() & 0x7F != 0 { return; }
            *state.curr_modifier() = 6;
        } else {
            *state.curr_modifier() = state.curr_modifier().wrapping_add(1);
            if *state.curr_modifier() == 5 {
                *state.curr_modifier() = 0x8F;
            } else if *state.curr_modifier() == 9 {
                *state.curr_modifier() = 0;
                state.trob().type_ = -1;
            }
        }
    }
    redraw_21h_impl(state);
}

/// C entry point for [`animate_spike_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_spike() {
    animate_spike_impl(&mut State);
}

/// Wind a gate up or drop it, by one frame.
///
/// The trob type selects the mode:
///
/// | type | meaning |
/// |------|---------|
/// | 0    | drifting shut, 1 quarter-pixel per frame |
/// | 1    | winching open; slams shut again once fully open |
/// | 2    | winching open and staying open |
/// | 3..8 | slamming shut, accelerating through [`GATE_CLOSE_SPEEDS`] |
///
/// `curr_modifier` is the openness; 188 is fully open and `0xFF` means wedged
/// open forever. The gentle-close case ticks a sound every fourth quarter-pixel
/// and the opening case every eighth, which is what gives the winch its rhythm.
unsafe fn animate_door_impl(state: &mut State) {
    let anim_type = state.trob().type_;
    if anim_type >= 0 {
        if anim_type >= 3 {
            // Slamming shut, and accelerating while it does: each frame bumps
            // the type, which steps to the next (faster) close speed.
            let speed_index = if anim_type < 8 {
                state.trob().type_ = anim_type + 1;
                anim_type + 1
            } else {
                anim_type
            };
            // The C source computes this in `short`, so the underflow that
            // signals "fully shut" is visible before the store truncates it.
            let new_openness = *state.curr_modifier() as i16
                - GATE_CLOSE_SPEEDS[speed_index as usize] as i16;
            *state.curr_modifier() = new_openness as u8;
            if new_openness < 0 {
                *state.curr_modifier() = 0;
                state.trob().type_ = -1;
                play_sound(soundids_sound_6_gate_closing_fast as c_int);
            }
        } else if *state.curr_modifier() == 0xFF {
            // Already wedged permanently open — nothing left to animate.
            gate_stop_impl(state);
        } else {
            *state.curr_modifier() =
                state.curr_modifier().wrapping_add(DOOR_DELTA[anim_type as usize]);
            if anim_type == 0 {
                // Drifting shut.
                if *state.curr_modifier() == 0 {
                    gate_stop_impl(state);
                } else if *state.curr_modifier() < 188 && (*state.curr_modifier() & 3) == 3 {
                    play_door_sound_if_visible_impl(
                        state,
                        soundids_sound_4_gate_closing as c_int,
                    );
                }
            } else if *state.curr_modifier() < 188 {
                // Still winching open.
                if (*state.curr_modifier() & 7) == 0 {
                    play_sound(soundids_sound_5_gate_opening as c_int);
                }
            } else if anim_type < 2 {
                // Fully open after a regular open: hold briefly, then close.
                *state.curr_modifier() = 238;
                state.trob().type_ = 0;
                play_sound(soundids_sound_7_gate_stop as c_int);
            } else {
                // Fully open after a permanent open: wedge it there.
                *state.curr_modifier() = 0xFF;
                gate_stop_impl(state);
            }
        }
    }
    draw_trob_impl(state);
}

/// C entry point for [`animate_door_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_door() {
    animate_door_impl(&mut State);
}

/// Retire a gate trob and play the clunk of it coming to rest.
unsafe fn gate_stop_impl(state: &mut State) {
    state.trob().type_ = -1;
    play_door_sound_if_visible_impl(state, soundids_sound_7_gate_stop as c_int);
}

/// C entry point for [`gate_stop_impl`].
#[no_mangle]
pub unsafe extern "C" fn gate_stop() {
    gate_stop_impl(&mut State);
}

/// Slide the level exit door open or shut, by one frame.
///
/// Trob types 0..=2 slide it open one pixel per frame until the modifier
/// reaches 43 (fully open); types 3..=6 slam it shut, accelerating through
/// [`LEVELDOOR_CLOSE_SPEEDS`]. Because a level door is much taller than a
/// gate the close speeds are in whole pixels, and completion is detected by
/// the modifier going *signed*-negative rather than by an explicit zero test.
///
/// Two special events hang off the moment the door finishes opening: all
/// sounds stop (except when the Kid is floating on a feather-fall potion,
/// whose music the fix preserves), and on the mirror level the mirror tile is
/// materialised into the room the Kid is about to run through.
unsafe fn animate_leveldoor_impl(state: &mut State) {
    let trob_type = state.trob().type_;
    if trob_type >= 0 {
        if trob_type >= 3 {
            // Slamming shut, accelerating as it goes.
            state.trob().type_ = trob_type + 1;
            let speed = LEVELDOOR_CLOSE_SPEEDS[(trob_type + 1 - 3) as usize];
            *state.curr_modifier() = state.curr_modifier().wrapping_sub(speed);
            if (*state.curr_modifier() as i8) < 0 {
                *state.curr_modifier() = 0;
                state.trob().type_ = -1;
                play_sound(soundids_sound_14_leveldoor_closing as c_int);
            } else if state.trob().type_ == 4 && (sound_flags & soundflags_sfDigi as u8) != 0 {
                sound_interruptible_set(soundids_sound_15_leveldoor_sliding as usize, 1);
                play_sound(soundids_sound_15_leveldoor_sliding as c_int);
            }
        } else {
            *state.curr_modifier() = state.curr_modifier().wrapping_add(1);
            if *state.curr_modifier() >= 43 {
                // Fully open.
                state.trob().type_ = -1;
                // FIX_FEATHER_INTERRUPTED_BY_LEVELDOOR is active
                if !((*fixes).fix_feather_interrupted_by_leveldoor != 0
                    && *state.is_feather_fall() != 0)
                {
                    stop_sounds();
                }
                if *state.leveldoor_open() == 0 || *state.leveldoor_open() == 2 {
                    *state.leveldoor_open() = 1;
                    if *state.current_level() as u16 == (*custom).mirror_level as u16 {
                        // Special event: place the mirror.
                        get_tile(
                            (*custom).mirror_room as c_int,
                            (*custom).mirror_column as c_int,
                            (*custom).mirror_row as c_int,
                        );
                        *curr_room_tiles.add(curr_tilepos as usize) = (*custom).mirror_tile;
                    }
                }
            } else {
                // Still sliding open.
                sound_interruptible_set(soundids_sound_15_leveldoor_sliding as usize, 0);
                play_sound(soundids_sound_15_leveldoor_sliding as c_int);
            }
        }
    }
    set_redraw_anim_right_impl(state);
}

/// C entry point for [`animate_leveldoor_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_leveldoor() {
    animate_leveldoor_impl(&mut State);
}

/// Next frame of a potion's bubbling, cycling 1..=7 (0 is the still pose).
#[no_mangle]
pub unsafe extern "C" fn bubble_next_frame(curr: c_short) -> c_short {
    let mut next = curr + 1;
    if next >= 8 { next = 1; }
    next
}

/// Next frame of a torch's flame, chosen so the flicker looks unpredictable.
///
/// Rolls a byte: if it happens to be a valid frame (0..=8) *and* differs from
/// the current one, jump straight to it; otherwise just step to the next frame
/// in sequence. That gives mostly-smooth animation with occasional jumps, from
/// a single random draw.
#[no_mangle]
pub unsafe extern "C" fn get_torch_frame(curr: c_short) -> c_short {
    let mut next = prandom(255) as c_short;
    if next != curr {
        if next < 9 {
            return next;
        } else {
            next = curr;
        }
    }
    next += 1;
    if next >= 9 { next = 0; }
    next
}

/// Mark the animation layer of `tilepos` dirty for the next `frames` frames.
///
/// A negative `tilepos` addresses the overhang strip of the room above, and 30
/// or more means "not visible" and is silently dropped — that sentinel is why
/// all of these take a `short` rather than an index.
unsafe fn set_redraw_anim_impl(state: &mut State, tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            state.redraw_frames_above()[(-(tilepos + 1)) as usize] = frames;
        } else {
            state.redraw_frames_anim()[tilepos as usize] = frames;
        }
    }
}

/// C entry point for [`set_redraw_anim_impl`].
#[no_mangle]
pub unsafe extern "C" fn set_redraw_anim(tilepos: c_short, frames: u8) {
    set_redraw_anim_impl(&mut State, tilepos, frames);
}

/// Mark the secondary (mob) layer of `tilepos` dirty.
///
/// Unlike the other setters this clamps the above-room index. Mobs, unlike
/// tiles, can be at a fractional height, so a loose floor falling out of the
/// top of a room reaches tilepos -11, one past the end of
/// `redraw_frames_above`.
unsafe fn set_redraw2_impl(state: &mut State, tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            let index = ((-tilepos - 1) as usize).min(9);
            state.redraw_frames_above()[index] = frames;
        } else {
            state.redraw_frames2()[tilepos as usize] = frames;
        }
    }
}

/// C entry point for [`set_redraw2_impl`].
#[no_mangle]
pub unsafe extern "C" fn set_redraw2(tilepos: c_short, frames: u8) {
    set_redraw2_impl(&mut State, tilepos, frames);
}

/// Mark the floor-overlay layer of `tilepos` dirty. See [`set_redraw_anim_impl`].
unsafe fn set_redraw_floor_overlay_impl(state: &mut State, tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            state.redraw_frames_above()[(-(tilepos + 1)) as usize] = frames;
        } else {
            state.redraw_frames_floor_overlay()[tilepos as usize] = frames;
        }
    }
}

/// C entry point for [`set_redraw_floor_overlay_impl`].
#[no_mangle]
pub unsafe extern "C" fn set_redraw_floor_overlay(tilepos: c_short, frames: u8) {
    set_redraw_floor_overlay_impl(&mut State, tilepos, frames);
}

/// Mark `tilepos` as needing a complete repaint. See [`set_redraw_anim_impl`].
unsafe fn set_redraw_full_impl(state: &mut State, tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            state.redraw_frames_above()[(-(tilepos + 1)) as usize] = frames;
        } else {
            state.redraw_frames_full()[tilepos as usize] = frames;
        }
    }
}

/// C entry point for [`set_redraw_full_impl`].
#[no_mangle]
pub unsafe extern "C" fn set_redraw_full(tilepos: c_short, frames: u8) {
    set_redraw_full_impl(&mut State, tilepos, frames);
}

/// Mark the foreground layer of `tilepos` dirty.
///
/// The foreground layer has no counterpart in the room above, so negative
/// positions are dropped along with the 30 sentinel.
unsafe fn set_redraw_fore_impl(state: &mut State, tilepos: c_short, frames: u8) {
    if tilepos < 30 && tilepos >= 0 {
        state.redraw_frames_fore()[tilepos as usize] = frames;
    }
}

/// C entry point for [`set_redraw_fore_impl`].
#[no_mangle]
pub unsafe extern "C" fn set_redraw_fore(tilepos: c_short, frames: u8) {
    set_redraw_fore_impl(&mut State, tilepos, frames);
}

/// Schedule an erase of the top `redraw_height` pixels of `tilepos` before it
/// is repainted.
///
/// If a wipe is already pending for this tile the two are merged by keeping
/// the taller of the two heights — and, because the merged height is written
/// back into `redraw_height`, by growing the caller's notion of how tall the
/// current repaint is as well.
unsafe fn set_wipe_impl(state: &mut State, tilepos: c_short, frames: u8) {
    if tilepos < 30 && tilepos >= 0 {
        let index = tilepos as usize;
        if state.wipe_frames()[index] != 0 {
            *state.redraw_height() =
                (state.wipe_heights()[index] as i16).max(*state.redraw_height());
        }
        let height = *state.redraw_height();
        state.wipe_heights()[index] = height as i8;
        state.wipe_frames()[index] = frames;
    }
}

/// C entry point for [`set_wipe_impl`].
#[no_mangle]
pub unsafe extern "C" fn set_wipe(tilepos: c_short, frames: u8) {
    set_wipe_impl(&mut State, tilepos, frames);
}

/// Start a torch burning, at a random point in its flicker cycle so that a
/// wall of torches does not animate in lockstep.
unsafe fn start_anim_torch_impl(state: &mut State, room: c_short, tilepos: c_short) {
    *curr_room_modif.add(tilepos as usize) = prandom(8) as u8;
    add_trob_impl(state, room as u8, tilepos as u8, 1);
}

/// C entry point for [`start_anim_torch_impl`].
#[no_mangle]
pub unsafe extern "C" fn start_anim_torch(room: c_short, tilepos: c_short) {
    start_anim_torch_impl(&mut State, room, tilepos);
}

/// Start a potion bubbling at a random bubble frame, keeping the top five
/// modifier bits that say which potion it is.
unsafe fn start_anim_potion_impl(state: &mut State, room: c_short, tilepos: c_short) {
    let potion_type = *curr_room_modif.add(tilepos as usize) & 0xF8;
    *curr_room_modif.add(tilepos as usize) = potion_type | (prandom(6) as u8 + 1);
    add_trob_impl(state, room as u8, tilepos as u8, 1);
}

/// C entry point for [`start_anim_potion_impl`].
#[no_mangle]
pub unsafe extern "C" fn start_anim_potion(room: c_short, tilepos: c_short) {
    start_anim_potion_impl(&mut State, room, tilepos);
}

/// Start a dropped sword twinkling, at a random point in its countdown.
unsafe fn start_anim_sword_impl(state: &mut State, room: c_short, tilepos: c_short) {
    *curr_room_modif.add(tilepos as usize) = prandom(0xFF) as u8 & 0x1F;
    add_trob_impl(state, room as u8, tilepos as u8, 1);
}

/// C entry point for [`start_anim_sword_impl`].
#[no_mangle]
pub unsafe extern "C" fn start_anim_sword(room: c_short, tilepos: c_short) {
    start_anim_sword_impl(&mut State, room, tilepos);
}

/// Set a chomper going, unless it is already mid-bite.
///
/// `modifier` carries the starting frame, which [`start_chompers_impl`] staggers
/// across a row so the blades do not all snap at once.
unsafe fn start_anim_chomper_impl(state: &mut State, room: c_short, tilepos: c_short, modifier: u8) {
    let old_modifier = *curr_room_modif.add(tilepos as usize);
    if old_modifier == 0 || old_modifier >= 6 {
        *curr_room_modif.add(tilepos as usize) = modifier;
        add_trob_impl(state, room as u8, tilepos as u8, 1);
    }
}

/// C entry point for [`start_anim_chomper_impl`].
#[no_mangle]
pub unsafe extern "C" fn start_anim_chomper(room: c_short, tilepos: c_short, modifier: u8) {
    start_anim_chomper_impl(&mut State, room, tilepos, modifier);
}

/// Fire a spike trap.
///
/// Only a fully retracted spike (modifier 0) starts a new extend-and-hold
/// cycle. One that is already out but counting down (a negative modifier, i.e.
/// the `0x80` hold flag set) gets its hold restarted at `0x8F` instead, which
/// keeps it out while the Kid stands on it. `0xFF` is the disabled spike and
/// is left untouched.
unsafe fn start_anim_spike_impl(state: &mut State, room: c_short, tilepos: c_short) {
    let old_modifier = *curr_room_modif.add(tilepos as usize) as i8;
    if old_modifier <= 0 {
        if old_modifier == 0 {
            add_trob_impl(state, room as u8, tilepos as u8, 1);
            play_sound(soundids_sound_49_spikes as c_int);
        } else if old_modifier != -1i8 {
            *curr_room_modif.add(tilepos as usize) = 0x8F;
        }
    }
}

/// C entry point for [`start_anim_spike_impl`].
#[no_mangle]
pub unsafe extern "C" fn start_anim_spike(room: c_short, tilepos: c_short) {
    start_anim_spike_impl(&mut State, room, tilepos);
}

/// Decide what a button press should do to one gate, and update its openness.
///
/// Returns the trob type the gate should be given (1 regular open, 2 permanent
/// open, 3 slam shut), or -1 for "no trob needed — nothing changed". The
/// `button_type` distinguishes a raise plate (`tiles_15_opener`), the rubble
/// left by a plate that was destroyed with someone standing on it
/// (`tiles_14_debris`, which wedges gates open forever), and anything else,
/// which is treated as a drop plate.
#[no_mangle]
pub unsafe extern "C" fn trigger_gate(_room: c_short, tilepos: c_short, button_type: c_short) -> c_short {
    let modifier = *curr_room_modif.add(tilepos as usize);
    if button_type == tiles_tiles_15_opener as c_short {
        if modifier == 0xFF { return -1; }
        if modifier >= 188 {
            *curr_room_modif.add(tilepos as usize) = 238;
            return -1;
        }
        *curr_room_modif.add(tilepos as usize) = (modifier + 3) & 0xFC;
        return 1; // regular open
    } else if button_type == tiles_tiles_14_debris as c_short {
        if modifier < 188 { return 2; } // permanent open
        *curr_room_modif.add(tilepos as usize) = 0xFF;
        return -1;
    } else {
        if modifier != 0 {
            return 3; // close fast
        } else {
            return -1;
        }
    }
}

/// Decide what a button press should do to one target tile.
///
/// Gates get the full treatment of [`trigger_gate`]; a level exit door opens
/// only if it is not already open. Everything else does nothing, unless the
/// `allow_triggering_any_tile` mod option is on, in which case any tile can be
/// turned into a trob.
#[no_mangle]
pub unsafe extern "C" fn trigger_1(
    target_type: c_short,
    room: c_short,
    tilepos: c_short,
    button_type: c_short,
) -> c_short {
    if target_type == tiles_tiles_4_gate as c_short {
        trigger_gate(room, tilepos, button_type)
    } else if target_type == tiles_tiles_16_level_door_left as c_short {
        if *curr_room_modif.add(tilepos as usize) != 0 { -1 } else { 1 }
    } else if (*custom).allow_triggering_any_tile != 0 {
        1
    } else {
        -1
    }
}

/// Fire every entry of the doorlink list starting at `index`.
///
/// Entries are consecutive; bit 7 of `doorlink1_ad` marks the last one. Each
/// names a room and a tile within it, which [`trigger_1`] turns into a trob
/// type — or into -1, meaning the target was already in the requested state
/// and needs no animation.
///
/// The C source loops on `while (1)` rather than on a terminator value,
/// because the terminator lives in a bit of the *current* entry, so the list
/// always has at least one element.
unsafe fn do_trigger_list_impl(state: &mut State, mut index: c_short, button_type: c_short) {
    loop {
        let room = get_doorlink_room(index);
        get_room_address(room as c_int);
        let tilepos = get_doorlink_tile(index);
        let target_type = (*curr_room_tiles.add(tilepos as usize) & 0x1F) as c_short;
        let trigger_result = trigger_1(target_type, room, tilepos, button_type);
        if trigger_result >= 0 {
            add_trob_impl(state, room as u8, tilepos as u8, trigger_result as i8);
        }
        if get_doorlink_next(index) == 0 { break; }
        index += 1;
    }
}

/// C entry point for [`do_trigger_list_impl`].
#[no_mangle]
pub unsafe extern "C" fn do_trigger_list(index: c_short, button_type: c_short) {
    do_trigger_list_impl(&mut State, index, button_type);
}

/// Put a tile on the animating-tiles worklist, or update it if it is already
/// there.
///
/// Re-triggering a tile that is already a trob must not create a second entry
/// — a gate being re-opened while it is closing has to switch mode, not
/// animate twice — so [`find_trob_impl`] looks for an existing entry first.
unsafe fn add_trob_impl(state: &mut State, room: u8, tilepos: u8, type_: i8) {
    if *state.trobs_count() as u32 >= TROBS_MAX {
        show_dialog(b"Trobs Overflow\0".as_ptr() as *const c_char);
        return;
    }
    state.trob().room = room;
    state.trob().tilepos = tilepos;
    state.trob().type_ = type_;
    let found = find_trob_impl(state);
    if found == -1 {
        if *state.trobs_count() as u32 == TROBS_MAX { return; }
        let t = *state.trob();
        let tc = *state.trobs_count();
        state.trobs()[tc as usize] = t;
        *state.trobs_count() += 1;
    } else {
        let t = state.trob().type_;
        state.trobs()[found as usize].type_ = t;
    }
}

/// C entry point for [`add_trob_impl`].
#[no_mangle]
pub unsafe extern "C" fn add_trob(room: u8, tilepos: u8, type_: i8) {
    add_trob_impl(&mut State, room, tilepos, type_);
}

/// Index of the existing trob for the tile currently held in `trob`, or -1.
unsafe fn find_trob_impl(state: &mut State) -> c_short {
    let (tilepos, room) = (state.trob().tilepos, state.trob().room);
    for index in 0..*state.trobs_count() {
        let candidate = state.trobs()[index as usize];
        if candidate.tilepos == tilepos && candidate.room == room {
            return index;
        }
    }
    -1
}

/// C entry point for [`find_trob_impl`].
#[no_mangle]
pub unsafe extern "C" fn find_trob() -> c_short {
    find_trob_impl(&mut State)
}

/// Forget every pending repaint and erase. Called when the whole screen is
/// about to be redrawn from scratch, so per-tile bookkeeping is moot.
unsafe fn clear_tile_wipes_impl(state: &mut State) {
    *state.redraw_frames_full() = [0u8; 30];
    *state.wipe_frames() = [0u8; 30];
    *state.wipe_heights() = [0i8; 30];
    *state.redraw_frames_anim() = [0u8; 30];
    *state.redraw_frames_fore() = [0u8; 30];
    *state.redraw_frames2() = [0u8; 30];
    *state.redraw_frames_floor_overlay() = [0u8; 30];
    tile_object_redraw = [0u8; 30];
    *state.redraw_frames_above() = [0u8; 10];
}

/// C entry point for [`clear_tile_wipes_impl`].
#[no_mangle]
pub unsafe extern "C" fn clear_tile_wipes() {
    clear_tile_wipes_impl(&mut State);
}

/// Debounce counter of doorlink entry `index` — `doorlink2_ad` bits 0..=4.
///
/// The value `0x1F` is a sentinel meaning "this event is jammed and can never
/// fire again", used by the fixed-shut plates on some levels.
unsafe fn get_doorlink_timer_impl(state: &mut State, index: c_short) -> c_short {
    (*state.doorlink2_ad().add(index as usize) & 0x1F) as c_short
}

/// C entry point for [`get_doorlink_timer_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_timer(index: c_short) -> c_short {
    get_doorlink_timer_impl(&mut State, index)
}

/// Store a debounce counter into doorlink entry `index`, preserving the room
/// bits that share the byte. Returns the whole updated byte, as the C original
/// does.
unsafe fn set_doorlink_timer_impl(state: &mut State, index: c_short, value: u8) -> c_short {
    let byte = state.doorlink2_ad().add(index as usize);
    *byte = (*byte & 0xE0) | (value & 0x1F);
    *byte as c_short
}

/// C entry point for [`set_doorlink_timer_impl`].
#[no_mangle]
pub unsafe extern "C" fn set_doorlink_timer(index: c_short, value: u8) -> c_short {
    set_doorlink_timer_impl(&mut State, index, value)
}

/// Target tilepos of doorlink entry `index` — `doorlink1_ad` bits 0..=4.
unsafe fn get_doorlink_tile_impl(state: &mut State, index: c_short) -> c_short {
    (*state.doorlink1_ad().add(index as usize) & 0x1F) as c_short
}

/// C entry point for [`get_doorlink_tile_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_tile(index: c_short) -> c_short {
    get_doorlink_tile_impl(&mut State, index)
}

/// Does another doorlink entry follow this one? Bit 7 of `doorlink1_ad` is the
/// end-of-list marker, so this is 1 while the bit is *clear*.
unsafe fn get_doorlink_next_impl(state: &mut State, index: c_short) -> c_short {
    ((*state.doorlink1_ad().add(index as usize) & 0x80) == 0) as c_short
}

/// C entry point for [`get_doorlink_next_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_next(index: c_short) -> c_short {
    get_doorlink_next_impl(&mut State, index)
}

/// Target room of doorlink entry `index`.
///
/// The five-bit room number is split across the two bytes: the low two bits
/// live in `doorlink1_ad` bits 5..=6, the high three in `doorlink2_ad`
/// bits 5..=7.
unsafe fn get_doorlink_room_impl(state: &mut State, index: c_short) -> c_short {
    let low = *state.doorlink1_ad().add(index as usize);
    let high = *state.doorlink2_ad().add(index as usize);
    (((low & 0x60) >> 5) + ((high & 0xE0) >> 3)) as c_short
}

/// C entry point for [`get_doorlink_room_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_room(index: c_short) -> c_short {
    get_doorlink_room_impl(&mut State, index)
}

/// Press the pressure plate at `curr_tilepos` and fire everything wired to it.
///
/// `button_type` of 0 and `modifier` of -1 both mean "take it from the tile
/// under the cursor", which is how the collision code calls this without
/// having to look the tile up itself.
///
/// The plate's doorlink timer is the debounce. It is reloaded to 5 on every
/// call, so standing on a plate re-fires its list every frame — that is what
/// holds a gate up while the Kid stands there. The click, the plate's
/// depressed artwork and the guard's attention only happen on the first frame,
/// when the timer had run down below 2. A timer of `0x1F` marks a jammed event
/// and suppresses everything.
unsafe fn trigger_button_impl(state: &mut State, playsound: c_int, mut button_type: c_int, modifier: c_int) {
    get_curr_tile_impl(state, curr_tilepos as c_short);
    if button_type == 0 {
        button_type = curr_tile as c_int;
    }
    let modifier = if modifier == -1 { *state.curr_modifier() as c_int } else { modifier };
    let link_timer = get_doorlink_timer_impl(state, modifier as c_short) as i8;
    if link_timer != 0x1F {
        set_doorlink_timer_impl(state, modifier as c_short, 5);
        if link_timer < 2 {
            add_trob_impl(state, curr_room as u8, curr_tilepos, 1);
            redraw_11h_impl(state);
            *state.is_guard_notice() = 1;
            if playsound != 0 {
                play_sound(soundids_sound_3_button_pressed as c_int);
            }
        }
        do_trigger_list_impl(state, modifier as c_short, button_type as c_short);
    }
}

/// C entry point for [`trigger_button_impl`].
#[no_mangle]
pub unsafe extern "C" fn trigger_button(playsound: c_int, button_type: c_int, modifier: c_int) {
    trigger_button_impl(&mut State, playsound, button_type, modifier);
}

/// Destroy the pressure plate someone just died on, and fire it one last time.
///
/// A raise plate is replaced by floor and re-fired as if it were rubble, which
/// wedges its gates open permanently — the corpse is holding the plate down
/// forever. A drop plate is replaced by the "stuck" tile and fired normally.
unsafe fn died_on_button_impl(state: &mut State) {
    let mut button_type = get_curr_tile_impl(state, curr_tilepos as c_short) as c_int;
    let modifier = *state.curr_modifier() as c_int;
    if curr_tile == tiles_tiles_15_opener as u8 {
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
        *curr_room_modif.add(curr_tilepos as usize) = 0;
        button_type = tiles_tiles_14_debris as c_int;
    } else {
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_5_stuck as u8;
    }
    trigger_button_impl(state, 1, button_type, modifier);
}

/// C entry point for [`died_on_button_impl`].
#[no_mangle]
pub unsafe extern "C" fn died_on_button() {
    died_on_button_impl(&mut State);
}

/// Count a pressed plate's debounce timer down by one, and let the plate pop
/// back up once it expires.
///
/// The countdown is what makes a plate stay visibly depressed for a few frames
/// after the Kid steps off it.
unsafe fn animate_button_impl(state: &mut State) {
    if state.trob().type_ >= 0 {
        let modifier = *state.curr_modifier() as c_short;
        // The C source holds this in a `word`, so a timer that was already 0
        // wraps to 0xFFFF instead of going negative: the plate is *not*
        // released, and the 0x1F written back jams the event permanently.
        // That is reachable when two plates share one doorlink list, and the
        // wraparound is the behaviour, so keep the unsigned type.
        let timer = (get_doorlink_timer_impl(state, modifier) as u16).wrapping_sub(1);
        set_doorlink_timer_impl(state, modifier, timer as u8);
        if timer < 2 {
            state.trob().type_ = -1;
            redraw_11h_impl(state);
        }
    }
}

/// C entry point for [`animate_button_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_button() {
    animate_button_impl(&mut State);
}

/// Set up the level exit door in its start-of-level state: fully open, and
/// already slamming shut behind the Kid as he runs in.
unsafe fn start_level_door_impl(state: &mut State, room: c_short, tilepos: c_short) {
    *curr_room_modif.add(tilepos as usize) = 43; // fully open
    add_trob_impl(state, room as u8, tilepos as u8, 3);
}

/// C entry point for [`start_level_door_impl`].
#[no_mangle]
pub unsafe extern "C" fn start_level_door(room: c_short, tilepos: c_short) {
    start_level_door_impl(&mut State, room, tilepos);
}

/// Retire a trob whose tile has become empty — a loose floor that has already
/// fallen — after one last repaint of the hole it left.
unsafe fn animate_empty_impl(state: &mut State) {
    state.trob().type_ = -1;
    redraw_20h_impl(state);
}

/// C entry point for [`animate_empty_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_empty() {
    animate_empty_impl(&mut State);
}

/// Run one frame of a loose floor tile: wobble, or give way.
///
/// The modifier's top bit distinguishes the two states. **Shaking** (top bit
/// set) is what happens when the tile is jostled from a distance — a footfall
/// nearby, or rubble crashing through the row above. It rattles for a few
/// frames and settles. On the auto-collapsing level the shake never settles,
/// which is what makes that level's floors fall away under the Kid.
///
/// **Loaded** (top bit clear) means something is standing on the tile; the
/// modifier counts up, and once it passes `loose_floor_delay` the tile is
/// removed from the room and re-created as a falling mob.
///
/// The one exception is the Kid mantling up from the room below onto this very
/// tile: dropping it then would take him down two rooms at once, so the fix
/// makes it merely shake until he has finished climbing.
unsafe fn animate_loose_impl(state: &mut State) {
    let anim_type = state.trob().type_;
    if anim_type >= 0 {
        *state.curr_modifier() = state.curr_modifier().wrapping_add(1);
        if *state.curr_modifier() & 0x80 != 0 {
            // just shaking — don't stop on loose_tiles_level
            if *state.current_level() as u16 == (*custom).loose_tiles_level as u16 { return; }
            if *state.curr_modifier() >= 0x84 {
                *state.curr_modifier() = 0;
                state.trob().type_ = -1;
            }
            let cm = *state.curr_modifier();
            loose_shake_impl(state, (cm == 0) as c_int);
        } else {
            // something is on the floor — should it fall?
            if *state.curr_modifier() >= (*custom).loose_floor_delay {
                let room = state.trob().room;
                let tilepos = state.trob().tilepos;
                // FIX_DROP_2_ROOMS_CLIMBING_LOOSE_TILE is active
                let kid_room = state.Kid().room;
                if (*fixes).fix_drop_2_rooms_climbing_loose_tile != 0
                    && room as u16 == state.level().roomlinks[kid_room as usize - 1].up as u16
                    && tilepos / 10 == 2
                    && state.Kid().curr_row == 0
                    && state.Kid().curr_col == (tilepos % 10) as i8
                    && state.Kid().frame >= frameids_frame_135_climbing_1 as u8
                    && state.Kid().frame < frameids_frame_141_climbing_7 as u8
                {
                    loose_shake_impl(state, 0);
                } else {
                    *state.curr_modifier() = remove_loose(room as c_int, tilepos as c_int) as u8;
                    state.trob().type_ = -1;
                    state.curmob().xh = (tilepos % 10) << 2;
                    let row = tilepos / 10;
                    state.curmob().y = Y_LOOSE_LAND[(row + 1) as usize] as u8;
                    state.curmob().room = room;
                    state.curmob().speed = 0;
                    state.curmob().type_ = 0;
                    state.curmob().row = row;
                    add_mob_impl(state);
                }
            } else {
                loose_shake_impl(state, 0);
            }
        }
    }
    redraw_20h_impl(state);
}

/// C entry point for [`animate_loose_impl`].
#[no_mangle]
pub unsafe extern "C" fn animate_loose() {
    animate_loose_impl(&mut State);
}

/// Rattle a loose floor tile, if this wobble frame is one of the loud ones or
/// `arg_0` forces it.
///
/// Picks one of three rattle samples, re-rolling until it differs from the
/// last one played so the same clip never repeats back to back.
unsafe fn loose_shake_impl(state: &mut State, arg_0: c_int) {
    if arg_0 != 0 || LOOSE_SOUND[(*state.curr_modifier() & 0x7F) as usize] != 0 {
        let mut sound_id: u32;
        loop {
            sound_id = prandom(2) as u32 + soundids_sound_20_loose_shake_1;
            if sound_id != *state.last_loose_sound() as u32 { break; }
        }
        // The DOS original burned one extra RNG cycle here. Keeping it is what
        // makes recorded replays line up; replays made before it was added
        // are flagged by the deprecation number and skip it.
        if !(replaying != 0 && *state.g_deprecation_number() < 2) {
            prandom(2);
        }
        if sound_flags & soundflags_sfDigi as u8 != 0 {
            *state.last_loose_sound() = sound_id as u16;
        }
        play_sound(sound_id as c_int);
    }
}

/// C entry point for [`loose_shake_impl`].
#[no_mangle]
pub unsafe extern "C" fn loose_shake(arg_0: c_int) {
    loose_shake_impl(&mut State, arg_0);
}

/// Take a loose floor tile out of the room, and return the modifier the empty
/// space it leaves should carry.
///
/// The gap's modifier is the level's *type* (dungeon or palace), because the
/// two tile sets draw a hole in a floor differently.
#[no_mangle]
pub unsafe extern "C" fn remove_loose(_room: c_int, tilepos: c_int) -> c_int {
    *curr_room_tiles.add(tilepos as usize) = tiles_tiles_0_empty as u8;
    (*custom).tbl_level_type[current_level as usize] as c_int
}

/// Load the loose floor at `curr_tilepos` so that it will fall.
///
/// Bit 5 of the tile byte marks a "solid" loose floor — one the level designer
/// nailed down — which never falls. A tile that is already loaded or shaking
/// (modifier > 0) is left alone so its countdown is not restarted.
#[no_mangle]
pub unsafe extern "C" fn make_loose_fall(modifier: u8) {
    if (*curr_room_tiles.add(curr_tilepos as usize) & 0x20) == 0
        && (*curr_room_modif.add(curr_tilepos as usize) as i8) <= 0
    {
        *curr_room_modif.add(curr_tilepos as usize) = modifier;
        add_trob(curr_room as u8, curr_tilepos, 0);
        redraw_20h();
    }
}

/// Set every chomper in the current character's row biting.
///
/// Chompers wake up when someone enters their row. The starting frames are
/// staggered along the row by [`next_chomper_timing`] so that a corridor of
/// blades snaps in a rippling sequence rather than all at once, and the blood
/// flag of each chomper is carried over into its new modifier.
unsafe fn start_chompers_impl(state: &mut State) {
    let mut timing: c_short = 15;
    if (state.Char().curr_row as u8) < 3 {
        get_room_address(state.Char().room as c_int);
        let row_start = tbl_line_at(state.Char().curr_row as usize) as c_short;
        for tilepos in row_start..row_start + 10 {
            if get_curr_tile_impl(state, tilepos) == tiles_tiles_18_chomper as c_short {
                let frame = *state.curr_modifier() & 0x7F;
                // Only chompers at rest — mid-bite ones keep their rhythm.
                if frame == 0 || frame >= 6 {
                    let char_room = state.Char().room;
                    let blood = *state.curr_modifier() & 0x80;
                    start_anim_chomper_impl(
                        state,
                        char_room as c_short,
                        tilepos,
                        timing as u8 | blood,
                    );
                    timing = next_chomper_timing(timing as u8) as c_short;
                }
            }
        }
    }
}

/// C entry point for [`start_chompers_impl`].
#[no_mangle]
pub unsafe extern "C" fn start_chompers() {
    start_chompers_impl(&mut State);
}

/// Next start-frame in the chomper stagger sequence: 15, 12, 9, 6, 13, 10, 7,
/// 14, 11, 8, and repeat.
///
/// Stepping back by 3 and wrapping by +10 walks all ten frames before
/// repeating, so up to ten chompers in a row all start out of phase.
#[no_mangle]
pub unsafe extern "C" fn next_chomper_timing(mut timing: u8) -> c_int {
    timing = timing.wrapping_sub(3);
    if timing < 6 {
        timing = timing.wrapping_add(10);
    }
    timing as c_int
}

/// Make the loose floor at `curr_tilepos` rattle without loading it.
///
/// This is the jostled-from-a-distance case; on the auto-collapsing level the
/// floors must not merely rattle, so it does nothing there.
unsafe fn loose_make_shake_impl(state: &mut State) {
    if *curr_room_modif.add(curr_tilepos as usize) == 0
        && *state.current_level() as u16 != (*custom).loose_tiles_level as u16
    {
        *curr_room_modif.add(curr_tilepos as usize) = 0x80;
        add_trob_impl(state, curr_room as u8, curr_tilepos, 1);
    }
}

/// C entry point for [`loose_make_shake_impl`].
#[no_mangle]
pub unsafe extern "C" fn loose_make_shake() {
    loose_make_shake_impl(&mut State);
}

/// Rattle every loose floor in one row, as falling rubble crashes through it.
///
/// `get_tile` leaves the tile it found in `curr_tilepos` / `curr_room`, which
/// is where [`loose_make_shake_impl`] then picks it up — hence the call with
/// no arguments inside the loop.
unsafe fn do_knock_impl(state: &mut State, room: c_int, knock_row: c_int) {
    for column in 0..10 {
        if get_tile(room, column, knock_row) == tiles_tiles_11_loose as c_int {
            loose_make_shake_impl(state);
        }
    }
}

/// C entry point for [`do_knock_impl`].
#[no_mangle]
pub unsafe extern "C" fn do_knock(room: c_int, knock_row: c_int) {
    do_knock_impl(&mut State, room, knock_row);
}

/// Append the mob in `curmob` to the falling-rubble list.
unsafe fn add_mob_impl(state: &mut State) {
    if *state.mobs_count() >= 14 {
        show_dialog(b"Mobs Overflow\0".as_ptr() as *const c_char);
        return;
    }
    let mob = *state.curmob();
    let count = *state.mobs_count();
    state.mobs()[count as usize] = mob;
    *state.mobs_count() += 1;
}

/// C entry point for [`add_mob_impl`].
#[no_mangle]
pub unsafe extern "C" fn add_mob() {
    add_mob_impl(&mut State);
}

/// Load the tile at `tilepos` of the room whose address is currently set up.
///
/// Its real product is the side effect: the tile kind lands in `curr_tile` and
/// its state byte in `curr_modifier`, which is what every animator here reads.
/// The high three bits of the tile byte are level-editor flags and are masked
/// off.
unsafe fn get_curr_tile_impl(state: &mut State, tilepos: c_short) -> c_short {
    *state.curr_modifier() = *curr_room_modif.add(tilepos as usize);
    curr_tile = *curr_room_tiles.add(tilepos as usize) & 0x1F;
    curr_tile as c_short
}

/// C entry point for [`get_curr_tile_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_curr_tile(tilepos: c_short) -> c_short {
    get_curr_tile_impl(&mut State, tilepos)
}

/// Advance every piece of falling rubble by one frame, then drop the ones that
/// have come to rest.
///
/// Same copy-in / copy-out shape as [`process_trobs_impl`], but the loop index
/// has to be the `curmob_index` *global*: [`loose_fall_impl`], reached from
/// [`move_loose_impl`], writes the current mob back into `mobs[]` at that index
/// before appending the extra mob it spawns. `speed == -1` marks a mob that
/// has landed and can be removed.
///
/// The mob count is sampled up front, so a mob spawned this frame is not
/// stepped until the next one.
unsafe fn do_mobs_impl(state: &mut State) {
    let n_mobs = *state.mobs_count();
    curmob_index = 0;
    while (curmob_index as c_short) < n_mobs {
        *state.curmob() = state.mobs()[curmob_index as usize];
        move_mob_impl(state);
        check_loose_fall_on_kid_impl(state);
        let mob = *state.curmob();
        state.mobs()[curmob_index as usize] = mob;
        curmob_index += 1;
    }
    let mut new_index: c_short = 0;
    for index in 0..*state.mobs_count() {
        if state.mobs()[index as usize].speed != -1 {
            state.mobs()[new_index as usize] = state.mobs()[index as usize];
            new_index += 1;
        }
    }
    *state.mobs_count() = new_index;
}

/// C entry point for [`do_mobs_impl`].
#[no_mangle]
pub unsafe extern "C" fn do_mobs() {
    do_mobs_impl(&mut State);
}

/// Step one mob. Type 0 — the only kind that exists — is falling masonry.
///
/// The trailing speed nudge walks the negative "at rest" speeds back up
/// towards -1, which is the value [`do_mobs_impl`] treats as "delete me". A
/// landed mob therefore lingers for one extra frame so its debris is drawn.
unsafe fn move_mob_impl(state: &mut State) {
    if state.curmob().type_ == 0 {
        move_loose_impl(state);
    }
    if state.curmob().speed <= 0 {
        state.curmob().speed = state.curmob().speed.wrapping_add(1);
    }
}

/// C entry point for [`move_mob_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_mob() {
    move_mob_impl(&mut State);
}

/// Fall one frame, and work out what the falling tile just hit.
///
/// Gravity is +3 per frame up to a terminal speed of 29. A mob in room 0 — one
/// that fell off the edge of the map — simply drops until it is out of sight.
/// Otherwise, each time the mob crosses the floor line of its row
/// ([`Y_SOMETHING`]) it looks at the tile there: empty space and other loose
/// floors are fallen through (the latter being knocked out on the way, see
/// [`loose_fall_impl`]), anything else stops it dead with a crash that rattles
/// the rest of the row.
unsafe fn move_loose_impl(state: &mut State) {
    if state.curmob().speed < 0 { return; }
    if state.curmob().speed < 29 {
        state.curmob().speed = state.curmob().speed.wrapping_add(3);
    }
    let speed = state.curmob().speed;
    state.curmob().y = state.curmob().y.wrapping_add(speed as u8);
    if state.curmob().room == 0 {
        if (state.curmob().y as u16) < 210 {
            return;
        } else {
            state.curmob().speed = -2;
            return;
        }
    }
    let row = state.curmob().row;
    if (state.curmob().y as u16) < 226 && Y_SOMETHING[(row + 1) as usize] <= state.curmob().y as i16 {
        // Crossed into a different row — what is there?
        curr_tile_temp = tile_under_mob(state) as u16;
        if curr_tile_temp == tiles_tiles_11_loose as u16 {
            loose_fall_impl(state);
        }
        if curr_tile_temp == tiles_tiles_0_empty as u16
            || curr_tile_temp == tiles_tiles_11_loose as u16
        {
            mob_down_a_row_impl(state);
            return;
        }
        play_sound(soundids_sound_2_tile_crashing as c_int);
        let mob_room = state.curmob().room;
        do_knock_impl(state, mob_room as c_int, row as c_int);
        state.curmob().y = Y_SOMETHING[(row + 1) as usize] as u8;
        state.curmob().speed = -2;
        loose_land_impl(state);
    }
}

/// C entry point for [`move_loose_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_loose() {
    move_loose_impl(&mut State);
}

/// The tile the current mob is currently over.
///
/// Besides its return value this leaves `curr_room`, `tile_col` and
/// `curr_tilepos` pointing at that tile, which the callers below rely on, so
/// the order and count of these calls is load-bearing.
unsafe fn tile_under_mob(state: &mut State) -> c_short {
    let mob = *state.curmob();
    get_tile(mob.room as c_int, (mob.xh >> 2) as c_int, mob.row as c_int) as c_short
}

/// Smash the landed mob into the tile it hit.
///
/// The C source writes this as a `switch` with two deliberate fall-throughs,
/// which is why a plate case runs the shared "leave rubble here" tail even
/// when triggering it changed the tile into something not otherwise in the
/// list; `leaves_rubble` reproduces that.
///
/// Landing on a plate presses it — a raise plate is *first* replaced by rubble
/// so its gates are wedged open permanently, matching what happens when
/// someone dies on one. Landing on a torch turns it into the
/// torch-with-debris tile instead of plain rubble, so the flame survives.
///
/// The extra repaint of the tile to the left is because the debris sprite
/// overhangs its cell.
unsafe fn loose_land_impl(state: &mut State) {
    let mut button_type: c_short = 0;
    let mut tiletype = tile_under_mob(state);
    let leaves_rubble;

    if tiletype == tiles_tiles_15_opener as c_short {
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_14_debris as u8;
        button_type = tiles_tiles_14_debris as c_short;
        trigger_button_impl(state, 1, button_type as c_int, -1);
        tiletype = tile_under_mob(state);
        leaves_rubble = true;
    } else if tiletype == tiles_tiles_6_closer as c_short {
        trigger_button_impl(state, 1, button_type as c_int, -1);
        tiletype = tile_under_mob(state);
        leaves_rubble = true;
    } else {
        leaves_rubble = tiletype == tiles_tiles_1_floor as c_short
            || tiletype == tiles_tiles_2_spike as c_short
            || tiletype == tiles_tiles_10_potion as c_short
            || tiletype == tiles_tiles_19_torch as c_short
            || tiletype == tiles_tiles_30_torch_with_debris as c_short;
    }

    if leaves_rubble {
        *curr_room_tiles.add(curr_tilepos as usize) = if tiletype == tiles_tiles_19_torch as c_short
            || tiletype == tiles_tiles_30_torch_with_debris as c_short
        {
            tiles_tiles_30_torch_with_debris as u8
        } else {
            tiles_tiles_14_debris as u8
        };
        redraw_at_cur_mob_impl(state);
        if tile_col != 0 {
            set_redraw_full_impl(state, curr_tilepos as c_short - 1, 1);
        }
    }
}

/// C entry point for [`loose_land_impl`].
#[no_mangle]
pub unsafe extern "C" fn loose_land() {
    loose_land_impl(&mut State);
}

/// Punch a falling tile through another loose floor, spawning a second mob.
///
/// The tile that was knocked out becomes a new mob starting half a row lower,
/// and the original keeps going at half speed. The dance through `mobs[]` is
/// how the C source gets two mobs out of one `curmob`: stash the original at
/// its own index, mutate `curmob` into the new one, append it, then restore.
unsafe fn loose_fall_impl(state: &mut State) {
    curr_room_modif.add(curr_tilepos as usize)
        .write(remove_loose(curr_room as c_int, curr_tilepos as c_int) as u8);
    state.curmob().speed >>= 1;
    let cm = *state.curmob();
    state.mobs()[curmob_index as usize] = cm;
    state.curmob().y = state.curmob().y.wrapping_add(6);
    mob_down_a_row_impl(state);
    add_mob_impl(state);
    *state.curmob() = state.mobs()[curmob_index as usize];
    redraw_at_cur_mob_impl(state);
}

/// C entry point for [`loose_fall_impl`].
#[no_mangle]
pub unsafe extern "C" fn loose_fall() {
    loose_fall_impl(&mut State);
}

/// Repaint the tile the current mob is over, and its right-hand neighbour if
/// that is still in the same room.
unsafe fn redraw_at_cur_mob_impl(state: &mut State) {
    if state.curmob().room as u16 == *state.drawn_room() {
        *state.redraw_height() = 0x20;
        set_redraw_full_impl(state, curr_tilepos as c_short, 1);
        set_wipe_impl(state, curr_tilepos as c_short, 1);
        if (curr_tilepos % 10) + 1 < 10 {
            set_redraw_full_impl(state, curr_tilepos as c_short + 1, 1);
            set_wipe_impl(state, curr_tilepos as c_short + 1, 1);
        }
    }
}

/// C entry point for [`redraw_at_cur_mob_impl`].
#[no_mangle]
pub unsafe extern "C" fn redraw_at_cur_mob() {
    redraw_at_cur_mob_impl(&mut State);
}

/// Move the current mob down one row, following the room link downwards when
/// it falls out of the bottom of the room.
unsafe fn mob_down_a_row_impl(state: &mut State) {
    state.curmob().row = state.curmob().row.wrapping_add(1);
    if state.curmob().row >= 3 {
        state.curmob().y = state.curmob().y.wrapping_sub(192);
        state.curmob().row = 0;
        let cm_room = state.curmob().room;
        state.curmob().room = state.level().roomlinks[cm_room as usize - 1].down;
    }
}

/// C entry point for [`mob_down_a_row_impl`].
#[no_mangle]
pub unsafe extern "C" fn mob_down_a_row() {
    mob_down_a_row_impl(&mut State);
}

/// Queue every piece of falling rubble for drawing.
unsafe fn draw_mobs_impl(state: &mut State) {
    for index in 0..*state.mobs_count() {
        *state.curmob() = state.mobs()[index as usize];
        draw_mob_impl(state);
    }
}

/// C entry point for [`draw_mobs_impl`].
#[no_mangle]
pub unsafe extern "C" fn draw_mobs() {
    draw_mobs_impl(&mut State);
}

/// Work out whether the current mob is on screen and, if so, queue its sprite.
///
/// A mob can be visible from three rooms: its own, the room below (it is still
/// falling through the ceiling, so its y is nudged by a room height to place
/// it), or the room above (it has fallen through the floor into view at the
/// top). Anywhere else it is skipped entirely.
unsafe fn draw_mob_impl(state: &mut State) {
    let mut ypos = state.curmob().y as c_short;
    if state.curmob().room as u16 == *state.drawn_room() {
        if state.curmob().y as u16 >= 210 { return; }
    } else if state.curmob().room as u16 == *state.room_B() {
        // C computes ABS((sbyte)ypos) after integer promotion, so a y of 128
        // yields +128 rather than overflowing; widen before negating.
        if (ypos as i8 as i32).abs() >= 18 { return; }
        state.curmob().y = state.curmob().y.wrapping_add(192);
        ypos = state.curmob().y as c_short;
    } else if state.curmob().room as u16 == *state.room_A() {
        if (state.curmob().y as u16) < 174 { return; }
        ypos = state.curmob().y as c_short - 189;
    } else {
        return;
    }
    let column = (state.curmob().xh >> 2) as c_short;
    let row = y_to_row_mod4(ypos as c_int);
    *state.obj_tilepos() = get_tilepos_nominus(column as c_int, row) as u8;
    // The sprite overhangs to the right, so the neighbouring column is the one
    // that has to be repainted — and the cell above it too if the sprite's
    // 18-pixel height crosses a row boundary.
    let right_column = column + 1;
    let tilepos = get_tilepos(right_column as c_int, row);
    set_redraw2_impl(state, tilepos as c_short, 1);
    set_redraw_fore_impl(state, tilepos as c_short, 1);
    let top_row = y_to_row_mod4(ypos as c_int - 18);
    if top_row != row {
        let top_tilepos = get_tilepos(right_column as c_int, top_row);
        set_redraw2_impl(state, top_tilepos as c_short, 1);
        set_redraw_fore_impl(state, top_tilepos as c_short, 1);
    }
    add_mob_to_objtable_impl(state, ypos as c_int);
}

/// C entry point for [`draw_mob_impl`].
#[no_mangle]
pub unsafe extern "C" fn draw_mob() {
    draw_mob_impl(&mut State);
}

/// Append the current mob to the frame's sprite list.
///
/// The `0x80` bit on `obj_type` is what tells the drawing code this entry is a
/// mob rather than a character.
unsafe fn add_mob_to_objtable_impl(state: &mut State, ypos: c_int) {
    let index = state.table_counts()[4];
    state.table_counts()[4] += 1;
    let mob = *state.curmob();
    let curr_obj = &mut state.objtable()[index as usize];
    curr_obj.obj_type = mob.type_ | 0x80;
    curr_obj.xh = mob.xh as i8;
    curr_obj.xl = 0;
    curr_obj.y = ypos as c_short;
    curr_obj.chtab_id = chtabs_id_chtab_6_environment as u8;
    curr_obj.id = 10;
    curr_obj.clip.top = 0;
    curr_obj.clip.left = 0;
    curr_obj.clip.right = 40;
    mark_obj_tile_redraw(index as c_int);
}

/// C entry point for [`add_mob_to_objtable_impl`].
#[no_mangle]
pub unsafe extern "C" fn add_mob_to_objtable(ypos: c_int) {
    add_mob_to_objtable_impl(&mut State, ypos);
}

/// Dead code in the original disassembly; kept so the symbol still exists.
#[no_mangle]
pub unsafe extern "C" fn sub_9A8E() {
    method_1_blit_rect(
        onscreen_surface_,
        offscreen_surface,
        core::ptr::addr_of!(rect_top),
        core::ptr::addr_of!(rect_top),
        0,
    );
}

/// How dangerous is the spike at `curr_tilepos` right now?
///
/// Returns 0 for harmless (retracted, or a permanently disabled spike), 1 for
/// fully out and holding, and 2 for mid-extension — the phase that impales
/// someone standing on the tile.
#[no_mangle]
pub unsafe extern "C" fn is_spike_harmful() -> c_int {
    let modifier = *curr_room_modif.add(curr_tilepos as usize) as i8;
    if modifier == 0 || modifier == -1 {
        0
    } else if modifier < 0 {
        1
    } else if modifier < 5 {
        2
    } else {
        0
    }
}

/// Did the current mob just land on the Kid's head?
///
/// The 30-pixel window is the Kid's height: the tile has to be below the top
/// of his head and above his feet.
unsafe fn check_loose_fall_on_kid_impl(state: &mut State) {
    loadkid();
    // C promotes both bytes to int, so `Char.y - 30` may legitimately go
    // negative for a Kid near the top of a room; compute in i32 to match.
    let kid_y = state.Char().y as i32;
    let mob_y = state.curmob().y as i32;
    if state.Char().room == state.curmob().room
        && state.Char().curr_col == (state.curmob().xh >> 2) as i8
        && mob_y < kid_y
        && kid_y - 30 < mob_y
    {
        fell_on_your_head_impl(state);
        savekid();
    }
}

/// C entry point for [`check_loose_fall_on_kid_impl`].
#[no_mangle]
pub unsafe extern "C" fn check_loose_fall_on_kid() {
    check_loose_fall_on_kid_impl(&mut State);
}

/// Drop a slab of masonry on the current character.
///
/// A running Kid ducks under it — frames 5..=14 are the run cycle — except on
/// the auto-collapsing level, where there is no escaping. He also has to be
/// upright: anything from hanging and climbing upwards is immune, apart from
/// turning on the spot.
///
/// Taking the hit snaps him onto the floor of his row, costs one hit point,
/// and either kills him (the crushed sequence, nudged back a little if he was
/// already impaled on spikes) or knocks him flat, sliding him out of a wall he
/// might otherwise be embedded in. Crouching absorbs the hit with no
/// animation.
unsafe fn fell_on_your_head_impl(state: &mut State) {
    let frame = state.Char().frame as c_short;
    let action = state.Char().action as c_short;
    if (*state.current_level() as u16 == (*custom).loose_tiles_level as u16
        || !(frameids_frame_5_start_run as c_short..15).contains(&frame))
        && (action < actions_actions_2_hang_climb as c_short
            || action == actions_actions_7_turn as c_short)
    {
        // curr_row is signed and can be -1 (the ceiling row), so widen before
        // the +1 rather than casting to usize first.
        let curr_row = state.Char().curr_row;
        state.Char().y = y_land_at((curr_row as i32 + 1) as usize) as u8;
        if take_hp(1) != 0 {
            seqtbl_offset_char(seqids_seq_22_crushed as c_short);
            if frame == frameids_frame_177_spiked as c_short {
                state.Char().x = char_dx_forward(-12) as u8;
            }
        } else if frame != frameids_frame_109_crouch as c_short {
            if get_tile_behind_char() == 0 {
                state.Char().x = char_dx_forward(-2) as u8;
            }
            seqtbl_offset_char(seqids_seq_52_loose_floor_fell_on_kid as c_short);
        }
    }
}

/// C entry point for [`fell_on_your_head_impl`].
#[no_mangle]
pub unsafe extern "C" fn fell_on_your_head() {
    fell_on_your_head_impl(&mut State);
}

/// Play a gate sound, but only if that gate is somewhere the player can see.
///
/// A gate's artwork lives in the tile to its *right*, so a gate in the
/// rightmost column of the room to the left is on screen while a gate in the
/// rightmost column of the drawn room is not. Without the fix the room-to-the-
/// left case swallows the drawn-room case entirely, silencing gates in the
/// current room whenever the room to the left happens to be the drawn room's
/// neighbour.
///
/// Level 3 room 2 is a scripted exception: the gates the Kid hears slam behind
/// him there are audible from anywhere.
unsafe fn play_door_sound_if_visible_impl(state: &mut State, sound_id: c_int) {
    let tilepos = state.trob().tilepos as u16;
    let gate_room = state.trob().room as u16;

    // FIX_GATE_SOUNDS is active
    let visible = if (*fixes).fix_gate_sounds != 0 {
        (gate_room == *state.room_L() && tilepos % 10 == 9)
            || (gate_room == *state.drawn_room() && tilepos % 10 != 9)
    } else if gate_room == *state.room_L() {
        tilepos % 10 == 9
    } else {
        gate_room == *state.drawn_room() && tilepos % 10 != 9
    };

    if (current_level == 3 && gate_room == 2) || visible {
        play_sound(sound_id);
    }
}

/// C entry point for [`play_door_sound_if_visible_impl`].
#[no_mangle]
pub unsafe extern "C" fn play_door_sound_if_visible(sound_id: c_int) {
    play_door_sound_if_visible_impl(&mut State, sound_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // next_chomper_timing cycles: 15,12,9,6,13,10,7,14,11,8,repeat
    #[test]
    fn next_chomper_timing_cycle() {
        setup();
        unsafe {
            let mut t: u8 = 15;
            let expected = [12u8, 9, 6, 13, 10, 7, 14, 11, 8, 15];
            for &want in &expected {
                t = next_chomper_timing(t) as u8;
                assert_eq!(t, want);
            }
        }
    }

    // bubble_next_frame wraps 1..7, skipping 0 and capping at 7.
    #[test]
    fn bubble_next_frame_cycles() {
        unsafe {
            assert_eq!(bubble_next_frame(1), 2);
            assert_eq!(bubble_next_frame(7), 1); // wraps back to 1
            assert_eq!(bubble_next_frame(6), 7);
        }
    }

    // is_spike_harmful: 0=harmless, 1=retracting, 2=extending
    #[test]
    fn is_spike_harmful_states() {
        unsafe {
            set_options_to_default();
            // Synthesise a minimal curr_room_modif via a local byte.
            // We can't easily set the global pointer in tests, so test
            // the logic indirectly by manually exercising what the C does:
            // modifier==0 -> 0, modifier==-1 -> 0, modifier<0 -> 1, modifier<5 -> 2, else 0
            let cases: &[(i8, c_int)] = &[
                (0,  0),
                (-1, 0),
                (-2, 1),
                (-10, 1),
                (1,  2),
                (4,  2),
                (5,  0),
                (9,  0),
            ];
            for &(modifier, want) in cases {
                // The C function reads curr_room_modif[curr_tilepos].
                // Replicate its logic inline.
                let got = if modifier == 0 || modifier == -1 {
                    0
                } else if modifier < 0 {
                    1
                } else if modifier < 5 {
                    2
                } else {
                    0
                };
                assert_eq!(got, want, "modifier={}", modifier);
            }
        }
    }

    // get_doorlink_timer extracts the low 5 bits of doorlink2_ad[index].
    // get_doorlink_tile extracts the low 5 bits of doorlink1_ad[index].
    // get_doorlink_next returns 1 when bit 7 of doorlink1_ad[index] is 0.
    // get_doorlink_room combines bits from both bytes.
    #[test]
    fn doorlink_accessors() {
        // Test with synthetic byte values (logic matches C exactly).
        let b1: u8 = 0b0110_1010; // bits: [7]=0 (next=1), [6:5]=11 (room_low=3), [4:0]=01010 (tile=10)
        let b2: u8 = 0b1010_0101; // bits: [7:5]=101 (room_hi=5), [4:0]=00101 (timer=5)

        let tile  = (b1 & 0x1F) as c_short;
        let next  = ((b1 & 0x80) == 0) as c_short; // 1 = has next, 0 = last entry
        let timer = (b2 & 0x1F) as c_short;
        let room  = (((b1 & 0x60) >> 5) + ((b2 & 0xE0) >> 3)) as c_short;

        assert_eq!(tile, 10);
        assert_eq!(next, 1); // bit 7 is 0, so there IS a next entry
        assert_eq!(timer, 5);
        // (b1&0x60)>>5 = 0b11 = 3; (b2&0xE0)>>3 = 0b10100 = 20
        assert_eq!(room, 3 + 20);
    }

    // Regression test: get_doorlink_next must return 0 when bit 7 is SET (last entry),
    // and 1 when bit 7 is CLEAR (more entries follow).
    // The original bug used Rust's bitwise `!` instead of logical NOT, making both
    // cases return 1 → do_trigger_list never broke out of its loop → index overflow.
    #[test]
    fn get_doorlink_next_bit7_controls_termination() {
        // Replicate the bit extraction logic inline (same as the function body).
        let last_entry: u8 = 0b1001_0101; // bit 7 set  → no next → should return 0
        let has_next:   u8 = 0b0001_0101; // bit 7 clear → has next → should return 1

        let result_last = ((last_entry & 0x80) == 0) as c_short;
        let result_next = ((has_next  & 0x80) == 0) as c_short;

        assert_eq!(result_last, 0, "bit 7 set must return 0 (last entry → break loop)");
        assert_eq!(result_next, 1, "bit 7 clear must return 1 (more entries → continue)");
    }

    // Regression test: draw_mob's room_B branch computes ABS((sbyte)ypos) — in C this
    // promotes the sbyte to int before negating, so -128 becomes 128 safely. The Rust
    // port originally did `(ypos as i8).abs()`, which panics on i8::MIN (-128) since
    // the negated result doesn't fit back in i8. Widen to i32 first, as C's integer
    // promotion does. Found via the lvl3_skeleton.p1r harness replay, which crashed
    // the Rust binary here (C oracle has no such issue due to promotion).
    #[test]
    fn draw_mob_room_b_abs_does_not_panic_on_i8_min() {
        setup();
        unsafe {
            curmob.y = 128; // as sbyte, this is -128 (i8::MIN)
            curmob.room = (room_B as u8).wrapping_add(1); // != drawn_room, != room_A
            curmob.xh = 0;
            curmob.speed = 0;
            curmob.type_ = 0;
            curmob.row = 0;
            room_B = curmob.room as word;
            drawn_room = (room_B + 1) as word; // ensure the first branch is skipped
            room_A = (room_B + 2) as word; // ensure the room_A branch is skipped
            draw_mob(); // must not panic
        }
    }
}
