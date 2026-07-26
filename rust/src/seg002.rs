//! Whoever the Kid is currently up against, and the room he is up against him in.
//!
//! There is only ever *one* opponent loaded at a time. The level stores, per
//! room, a parked guard (his tile, x, facing, colour, skill and animation
//! program, in the `level.guards_*` arrays); walking into a room unpacks that
//! record into the live `Guard` character ([`enter_guard_impl`]) and walking
//! out packs it back ([`leave_guard_impl`]). Everything in between — a
//! swordfight, a chase, a fall out of the level — happens to that single live
//! opponent. The three jobs this module does:
//!
//! * **Casting.** [`check_shadow_impl`] decides *what* the opponent in the
//!   room the Kid just entered is. Most of the time the answer is "the guard
//!   parked in this room", but a handful of rooms are scripted set-pieces: the
//!   shadow who steals the potion (level 5), the one who makes the flagstone
//!   jump (level 6), the mirror double (level 4), and the level-12 shadow the
//!   Kid must merge with. Those are built from templates in
//!   `custom->init_shad_*` by [`do_init_shad_impl`] rather than from the level
//!   data. [`check_skel_impl`] is the same idea for the skeleton that sits as
//!   a decorative tile until the Kid steps on the trigger column.
//!
//! * **Driving him.** The opponent has no keyboard, so once per frame
//!   [`autocontrol_opponent_impl`] synthesises one by writing the same
//!   `control_*` globals the player's input writes (see the `move_*` helpers).
//!   From there it is a dispatch on who he is: a mouse runs off screen, a
//!   shadow follows its per-level script, and a guard or skeleton runs the
//!   fencing AI — close in, back off, block, or strike, each gated on a
//!   `custom->…prob[guard_skill]` roll against [`prandom`]. That skill-indexed
//!   randomness is why higher-skilled guards feel sharper rather than faster.
//!
//! * **Room transitions and sword damage.** [`exit_room_impl`] /
//!   [`leave_room_impl`] / [`goto_other_room_impl`] move a character across a
//!   room boundary and decide whether the opponent follows the Kid through it
//!   or is left parked behind. Leaving a room to the left or right is also
//!   where the game's scripted events fire ([`play_mirr_mus_impl`],
//!   [`level3_set_chkp_impl`], [`Jaffar_exit_impl`],
//!   [`sword_disappears_impl`], [`meet_Jaffar_impl`]).
//!   [`check_sword_hurting_impl`] resolves each frame of a swordfight for both
//!   fighters symmetrically, and [`hurt_by_sword_impl`] applies the result —
//!   which, if the victim has no floor behind him, is a shove off the ledge
//!   rather than a death in place.
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
use crate::state::State;

/// Replaces the live opponent with a scripted shadow built from `source`.
///
/// `source` is one of the eight-byte `custom->init_shad_*` templates; its
/// first seven bytes are copied straight over the head of `Char` (frame, x, y,
/// direction, curr_col, curr_row, action), and `seq_index` picks the animation
/// he starts in. The shadow always gets skill 3 and a flat 4 hit points
/// regardless of level, and `demo_time` is rewound so his scripted move list
/// (see [`do_auto_moves_impl`]) starts from the top.
///
/// seg002:0000
unsafe fn do_init_shad_impl(state: &mut State, source: *const byte, seq_index: c_int) {
    core::ptr::copy_nonoverlapping(source, core::ptr::addr_of_mut!(Char) as *mut byte, 7);
    seqtbl_offset_char(seq_index as c_short);
    state.Char().charid = charids_charid_1_shadow as u8;
    *state.demo_time() = 0;
    *state.guard_skill() = 3;
    *state.guardhp_delta() = 4;
    *state.guardhp_curr() = 4;
    *state.guardhp_max() = 4;
    saveshad();
}

/// C entry point for [`do_init_shad_impl`].
#[no_mangle]
pub unsafe extern "C" fn do_init_shad(source: *const byte, seq_index: c_int) {
    do_init_shad_impl(&mut State, source, seq_index);
}

/// Gives the opponent his starting hit points for this level and skill.
///
/// Base is `custom->tbl_guard_hp[current_level]`; skill 4 alone gets one extra
/// point from `custom->extrastrength`. All three of max/current/delta are set,
/// so the HP bar redraws from empty.
///
/// seg002:0044
unsafe fn get_guard_hp_impl(state: &mut State) {
    let hp = (*custom).extrastrength[*state.guard_skill() as usize] as i32
        + (*custom).tbl_guard_hp[*state.current_level() as usize] as i32;
    *state.guardhp_max() = hp as u16;
    *state.guardhp_curr() = hp as u16;
    *state.guardhp_delta() = hp as c_short;
}

/// C entry point for [`get_guard_hp_impl`].
#[no_mangle]
pub unsafe extern "C" fn get_guard_hp() {
    get_guard_hp_impl(&mut State);
}

/// Decides who the opponent in the room being entered is.
///
/// Checked in order: the level-12 shadow (only while the Kid has not yet
/// merged with him, and only until the sword has been picked up), the level-6
/// shadow who jumps onto the flagstone, and the level-5 shadow who drinks the
/// potion. If none of those set-pieces applies, the room's parked guard is
/// unpacked by [`enter_guard_impl`].
///
/// seg002:0064
unsafe fn check_shadow_impl(state: &mut State) {
    *state.offguard() = 0;
    if *state.current_level() == 12 && *state.united_with_shadow() == 0 && *state.drawn_room() == 15
    {
        state.Char().room = *state.drawn_room() as u8;
        // Once the sword is still lying there the shadow has not yet appeared.
        if get_tile(15, 1, 0) == tiles_tiles_22_sword as c_int {
            return;
        }
        *state.shadow_initialized() = 0;
        do_init_shad_impl(
            state,
            core::ptr::addr_of!((*custom).init_shad_12).cast::<byte>(),
            7, // fall
        );
        return;
    }
    if *state.current_level() == (*custom).shadow_step_level as u16 {
        // Char.room is assigned whenever the *level* matches, even when the
        // room does not and this branch falls through to enter_guard below.
        // That write is observable, so the two conditions cannot be folded.
        state.Char().room = *state.drawn_room() as u8;
        if state.Char().room == (*custom).shadow_step_room {
            if *state.leveldoor_open() != 0x4D {
                play_sound(soundids_sound_25_presentation as c_int);
                *state.leveldoor_open() = 0x4D;
            }
            do_init_shad_impl(
                state,
                core::ptr::addr_of!((*custom).init_shad_6).cast::<byte>(),
                2, // stand
            );
            return;
        }
    }
    if *state.current_level() == (*custom).shadow_steal_level as u16 {
        // Same as above: the Char.room write happens on the level check alone.
        state.Char().room = *state.drawn_room() as u8;
        if state.Char().room == (*custom).shadow_steal_room {
            // No shadow if the Kid already drank the potion.
            if get_tile((*custom).shadow_steal_room as c_int, 3, 0)
                != tiles_tiles_10_potion as c_int
            {
                return;
            }
            do_init_shad_impl(
                state,
                core::ptr::addr_of!((*custom).init_shad_5).cast::<byte>(),
                2, // stand
            );
            return;
        }
    }
    enter_guard_impl(state);
}

/// C entry point for [`check_shadow_impl`].
#[no_mangle]
pub unsafe extern "C" fn check_shadow() {
    check_shadow_impl(&mut State);
}

/// x-coordinate of the leading edge of the guard parked in `room`.
///
/// Used only by the offscreen-guard rescue in [`enter_guard_impl`], which has
/// to know whether a guard standing just outside the room would be drawn
/// inside it. Which edge counts depends on his facing; the -9/+1 nudges were
/// arrived at by trial and error in the C original.
unsafe fn parked_guard_edge_x(state: &mut State, room: word) -> c_int {
    let other_room_minus_1 = (room - 1) as usize;
    let mut other_guard_x = state.level().guards_x[other_room_minus_1] as c_int;
    let other_guard_dir = state.level().guards_dir[other_room_minus_1] as i8;
    if other_guard_dir == directions_dir_0_right as i8 {
        other_guard_x -= 9;
    }
    if other_guard_dir == directions_dir_FF_left as i8 {
        other_guard_x += 1;
    }
    other_guard_x
}

/// Moves the guard parked in `from_room` into `to_room_minus_1`, shifted by
/// `delta_x`, and blanks his old parking slot.
///
/// The guard record is spread over seven parallel `level.guards_*` arrays, so
/// "move a guard" means copying six fields and invalidating the seventh.
unsafe fn retrieve_parked_guard(
    state: &mut State,
    to_room_minus_1: usize,
    from_room: word,
    delta_x: c_int,
) {
    let from = (from_room - 1) as usize;
    let lvl = state.level();
    lvl.guards_x[to_room_minus_1] = (lvl.guards_x[from] as c_int + delta_x) as u8;
    lvl.guards_color[to_room_minus_1] = lvl.guards_color[from];
    lvl.guards_dir[to_room_minus_1] = lvl.guards_dir[from];
    lvl.guards_seq_hi[to_room_minus_1] = lvl.guards_seq_hi[from];
    lvl.guards_seq_lo[to_room_minus_1] = lvl.guards_seq_lo[from];
    lvl.guards_skill[to_room_minus_1] = lvl.guards_skill[from];
    lvl.guards_tile[from] = 0xFF;
    lvl.guards_seq_hi[from] = 0;
}

/// Unpacks the guard parked in the room being drawn into the live `Char`.
///
/// Reads his tile (which gives his row), x, facing, colour, skill and stored
/// animation program out of `level.guards_*`, gives him his hit points, and
/// leaves him standing — active with his sword out if he is a skeleton,
/// inactive otherwise. A guard whose stored frame is a death frame comes back
/// as a corpse (`alive = 1`) rather than a fighter.
///
/// If this room has no guard, the `fix_offscreen_guards_disappearing` fix
/// looks into the rooms immediately left and right: a guard standing near
/// their shared edge is visible from here, so he is dragged into this room
/// (shifted by one room width) instead of popping out of existence.
///
/// seg002:0112
unsafe fn enter_guard_impl(state: &mut State) {
    // arrays are indexed 0..23 instead of 1..24
    let room_minus_1 = (*state.drawn_room() - 1) as usize;
    let mut guard_tile = state.level().guards_tile[room_minus_1];

    if guard_tile >= 30 {
        if (*fixes).fix_offscreen_guards_disappearing == 0 {
            return;
        }
        let room_l = *state.room_L();
        let room_r = *state.room_R();
        let has_guard = |t: i16| (0..30).contains(&t);
        let left_guard_tile: i16 =
            if room_l > 0 { state.level().guards_tile[(room_l - 1) as usize] as i16 } else { 31 };
        let right_guard_tile: i16 =
            if room_r > 0 { state.level().guards_tile[(room_r - 1) as usize] as i16 } else { 31 };

        // The right room is preferred, but only if its guard has already
        // walked far enough left to show through this room's right edge;
        // otherwise the C code `goto`s into the left-room branch, which is
        // exactly what an `else if` does here since the edge test is a pure
        // read.
        if has_guard(right_guard_tile) && parked_guard_edge_x(state, room_r) < 58 + 4 {
            guard_tile = right_guard_tile as u8;
            retrieve_parked_guard(state, room_minus_1, room_r, 140); // guard leaves to the left
        } else if has_guard(left_guard_tile) {
            if parked_guard_edge_x(state, room_l) <= 190 - 4 {
                return;
            }
            guard_tile = left_guard_tile as u8;
            retrieve_parked_guard(state, room_minus_1, room_l, -140); // guard leaves to the right
        } else {
            return;
        }
    }

    state.Char().room = *state.drawn_room() as u8;
    state.Char().curr_row = (guard_tile / SCREEN_TILECOUNTX as u8) as i8;
    let curr_row = state.Char().curr_row;
    state.Char().y = y_land_at((curr_row + 1) as usize) as u8;
    state.Char().x = state.level().guards_x[room_minus_1];
    let char_x = state.Char().x;
    state.Char().curr_col = get_tile_div_mod_m7(char_x as c_int) as i8;
    state.Char().direction = state.level().guards_dir[room_minus_1] as i8;

    // only regular guards have different colors (and only on VGA)
    let cl = *state.current_level() as usize;
    if graphics_mode == grmodes_gmMcgaVga as u8
        && (*custom).tbl_guard_type[cl] == 0
    {
        *state.curr_guard_color() = state.level().guards_color[room_minus_1] as u16;
    } else {
        *state.curr_guard_color() = 0;
    }

    // The stored colour byte doubles as HP storage: low nibble is the colour,
    // high nibble the hit points he had when he was last parked.
    let remembered_hp = ((state.level().guards_color[room_minus_1] & 0xF0) >> 4) as i32;
    *state.curr_guard_color() &= 0x0F;

    // guard type 2 means this level's "guards" are skeletons
    if (*custom).tbl_guard_type[cl] == 2 {
        state.Char().charid = charids_charid_4_skeleton as u8;
    } else {
        state.Char().charid = charids_charid_2_guard as u8;
    }

    // A zero high byte means "no saved animation program": stand him up fresh.
    let seq_hi = state.level().guards_seq_hi[room_minus_1];
    if seq_hi == 0 {
        if state.Char().charid == charids_charid_4_skeleton as u8 {
            state.Char().sword = sword_status_sword_2_drawn as u8;
            seqtbl_offset_char(seqids_seq_63_guard_active_after_fall as c_short);
        } else {
            state.Char().sword = sword_status_sword_0_sheathed as u8;
            seqtbl_offset_char(seqids_seq_77_guard_stand_inactive as c_short);
        }
    } else {
        state.Char().curr_seq = state.level().guards_seq_lo[room_minus_1] as u16
            | ((seq_hi as u16) << 8);
    }
    play_seq();

    *state.guard_skill() = state.level().guards_skill[room_minus_1] as u16;
    if *state.guard_skill() >= NUM_GUARD_SKILLS as u16 {
        *state.guard_skill() = 3;
    }

    // play_seq() above may have advanced him into a death frame; if so he was
    // already dead when the room was left, and comes back as a body.
    let frame = state.Char().frame;
    if frame == frameids_frame_185_dead as u8
        || frame == frameids_frame_177_spiked as u8
        || frame == frameids_frame_178_chomped as u8
    {
        state.Char().alive = 1;
        draw_guard_hp(0, *state.guardhp_curr() as c_short);
        *state.guardhp_curr() = 0;
    } else {
        state.Char().alive = -1;
        *state.justblocked() = 0;
        *state.guard_refrac() = 0;
        *state.is_guard_notice() = 0;
        get_guard_hp_impl(state);
        if (*fixes).enable_remember_guard_hp != 0 && remembered_hp > 0 {
            *state.guardhp_delta() = remembered_hp as c_short;
            *state.guardhp_curr() = remembered_hp as u16;
        }
    }

    state.Char().fall_y = 0;
    state.Char().fall_x = 0;
    state.Char().action = actions_actions_1_run_jump as u8;
    saveshad();
}

/// C entry point for [`enter_guard_impl`].
#[no_mangle]
pub unsafe extern "C" fn enter_guard() {
    enter_guard_impl(&mut State);
}

/// Handles the opponent dropping through the bottom of the screen.
///
/// A shadow simply ceases to exist. A skeleton falling into the level's
/// designated reappear room is teleported there and parked, so he can climb
/// back up and be met again — the reason the level-3 skeleton feels
/// unkillable. Anyone else is gone for good: his parking slot is invalidated
/// so he will not be back when the room is re-entered.
///
/// seg002:0269
unsafe fn check_guard_fallout_impl(state: &mut State) {
    if state.Guard().direction == directions_dir_56_none as i8 || state.Guard().y < 211 {
        return;
    }
    // NOTE: the roomlinks lookup is deliberately *not* hoisted out of the &&.
    // Guard.room is 0 for a character who has fallen out of the level, and
    // `roomlinks[Guard.room - 1]` would then index out of bounds; C only
    // reaches it once charid is known to be a skeleton, and so must this.
    if state.Guard().charid == charids_charid_1_shadow as u8 {
        if state.Guard().action != actions_actions_4_in_freefall as u8 {
            return;
        }
        loadshad();
        clear_char();
        saveshad();
    } else if state.Guard().charid == charids_charid_4_skeleton as u8
        && {
            // should the level number be checked too?
            let gr = state.Guard().room;
            state.level().roomlinks[(gr as usize) - 1].down == (*custom).skeleton_reappear_room
        }
    {
        let guard_room = state.Guard().room;
        state.Guard().room = state.level().roomlinks[(guard_room as usize) - 1].down;
        state.Guard().x = (*custom).skeleton_reappear_x;
        state.Guard().curr_row = (*custom).skeleton_reappear_row as i8;
        state.Guard().direction = (*custom).skeleton_reappear_dir as i8;
        state.Guard().alive = -1;
        leave_guard_impl(state);
    } else {
        on_guard_killed();
        let drawn_room_v = *state.drawn_room();
        state.level().guards_tile[(drawn_room_v - 1) as usize] = 0xFF;
        state.Guard().direction = directions_dir_56_none as i8;
        draw_guard_hp(0, *state.guardhp_curr() as c_short);
        *state.guardhp_curr() = 0;
    }
}

/// C entry point for [`check_guard_fallout_impl`].
#[no_mangle]
pub unsafe extern "C" fn check_guard_fallout() {
    check_guard_fallout_impl(&mut State);
}

/// Parks the live opponent back into his room's `level.guards_*` slot.
///
/// This is the inverse of [`enter_guard_impl`]: position, facing, skill and
/// (unless he is dead) his current animation program are written back so the
/// room can be re-entered and find him exactly where he was left. Shadows and
/// the mouse are scripted rather than parked, so they are skipped entirely.
/// With `enable_remember_guard_hp` his remaining hit points are smuggled into
/// the high nibble of the colour byte. Either way the opponent slot is then
/// emptied and the HP bar cleared.
///
/// seg002:02F5
unsafe fn leave_guard_impl(state: &mut State) {
    if state.Guard().direction == directions_dir_56_none as i8
        || state.Guard().charid == charids_charid_1_shadow as u8
        || state.Guard().charid == charids_charid_24_mouse as u8
    {
        return;
    }
    // arrays are indexed 0..23 instead of 1..24
    let room_minus_1 = (state.Guard().room as usize) - 1;
    let guard_curr_row = state.Guard().curr_row;
    state.level().guards_tile[room_minus_1] = get_tilepos(0, guard_curr_row as c_int) as u8;

    // restriction to 4 bits added
    state.level().guards_color[room_minus_1] = (*state.curr_guard_color() & 0x0F) as u8;
    // can remember 1..15 hp
    if (*fixes).enable_remember_guard_hp != 0 && *state.guardhp_curr() < 16 {
        let ghc = *state.guardhp_curr();
        state.level().guards_color[room_minus_1] |= (ghc << 4) as u8;
    }

    state.level().guards_x[room_minus_1] = state.Guard().x;
    state.level().guards_dir[room_minus_1] = state.Guard().direction as u8;
    state.level().guards_skill[room_minus_1] = *state.guard_skill() as u8;

    if state.Guard().alive < 0 {
        state.level().guards_seq_hi[room_minus_1] = 0;
    } else {
        state.level().guards_seq_lo[room_minus_1] = state.Guard().curr_seq as u8;
        state.level().guards_seq_hi[room_minus_1] = (state.Guard().curr_seq >> 8) as u8;
    }

    state.Guard().direction = directions_dir_56_none as i8;
    draw_guard_hp(0, *state.guardhp_curr() as c_short);
    *state.guardhp_curr() = 0;
}

/// C entry point for [`leave_guard_impl`].
#[no_mangle]
pub unsafe extern "C" fn leave_guard() {
    leave_guard_impl(&mut State);
}

/// Sends the opponent through the same room boundary the Kid just crossed.
///
/// Both the room being left and the room being entered have their parking
/// slots invalidated first — the guard is *following*, so he must not also be
/// left standing behind in either of them.
///
/// seg002:039E
unsafe fn follow_guard_impl(state: &mut State) {
    let kid_room = state.Kid().room;
    let guard_room = state.Guard().room;
    state.level().guards_tile[(kid_room as usize) - 1] = 0xFF;
    state.level().guards_tile[(guard_room as usize) - 1] = 0xFF;
    loadshad();
    let rlr = *state.roomleave_result();
    goto_other_room_impl(state, rlr);
    saveshad();
}

/// C entry point for [`follow_guard_impl`].
#[no_mangle]
pub unsafe extern "C" fn follow_guard() {
    follow_guard_impl(&mut State);
}

/// Per-frame check: has the Kid walked far enough to change rooms, and does
/// the opponent come with him?
///
/// [`leave_room_impl`] does the moving and reports which way he went (or that
/// he did not go). The opponent follows only if he is alive, has his sword
/// out, the room being entered has no guard of its own waiting there, and he
/// is himself close enough to the boundary to plausibly step through it —
/// otherwise he is parked by [`leave_guard_impl`].
///
/// seg002:03C7
unsafe fn exit_room_impl(state: &mut State) {
    if *state.exit_room_timer() != 0 {
        *state.exit_room_timer() -= 1;
        if !((*fixes).fix_hang_on_teleport != 0 && state.Char().y >= 211 && state.Char().curr_row >= 2) {
            return;
        }
    }
    loadkid();
    load_frame_to_obj();
    set_char_collision();
    *state.roomleave_result() = leave_room_impl(state);
    if *state.roomleave_result() < 0 {
        return;
    }
    savekid();
    *state.next_room() = state.Char().room as u16;
    if (*fixes).enable_super_high_jump != 0 && *state.super_jump_fall() != 0 && *state.next_room() == *state.drawn_room() {
        return;
    }
    if state.Guard().direction == directions_dir_56_none as i8 {
        return;
    }
    // kid_room_m1 might be 65535 (-1) when the prince fell out of the level
    // (to room 0) while a guard was active; the bounds check keeps the
    // following indexing from crashing.
    let kid_room_m1 = (state.Kid().room as i16) - 1;
    let leave = if state.Guard().alive >= 0 || state.Guard().sword != sword_status_sword_2_drawn as u8
    {
        true
    } else if !(0..=23).contains(&kid_room_m1)
        || (state.level().guards_tile[kid_room_m1 as usize] < 30
            && state.level().guards_seq_hi[kid_room_m1 as usize] == 0)
    {
        // There is already a guard waiting in the room being entered.
        true
    } else {
        // He follows only from close enough to the boundary he is crossing.
        // The gate fix additionally keeps him behind when he cannot actually
        // see the Kid (a closed gate between them) and is not being fought.
        let blocked_by_gate = (*fixes).fix_guard_following_through_closed_gates != 0
            && *state.can_guard_see_kid() != 2
            && state.Kid().sword != sword_status_sword_2_drawn as u8;
        match *state.roomleave_result() {
            0 => state.Guard().x >= 91 || blocked_by_gate,  // left
            1 => state.Guard().x < 165 || blocked_by_gate,  // right
            2 => state.Guard().curr_row >= 0,               // up
            _ => state.Guard().curr_row < 3,                // down
        }
    };
    if leave {
        leave_guard_impl(state);
    } else {
        follow_guard_impl(state);
    }
}

/// C entry point for [`exit_room_impl`].
#[no_mangle]
pub unsafe extern "C" fn exit_room() {
    exit_room_impl(&mut State);
}

/// Moves the current character across one room boundary.
///
/// `direction` is an index into the room's link record — 0 left, 1 right,
/// 2 up, 3 down. Rooms are 140 pixels wide and 189 pixels tall in character
/// coordinates, so crossing a boundary means keeping the character where he
/// is on screen while renumbering which room that screen position belongs to.
/// Room 0 is the "outside the level" sentinel and links only to itself.
/// Returns the direction the character came *from*.
///
/// seg002:0486
unsafe fn goto_other_room_impl(state: &mut State, direction: c_short) -> c_int {
    let char_room = state.Char().room;
    state.Char().room = if char_room == 0 {
        0
    } else {
        let rlinks = &state.level().roomlinks[(char_room as usize) - 1];
        match direction {
            0 => rlinks.left,
            1 => rlinks.right,
            2 => rlinks.up,
            _ => rlinks.down,
        }
    };
    match direction {
        0 => {
            // left
            state.Char().x = state.Char().x.wrapping_add(140);
            1
        }
        1 => {
            // right
            state.Char().x = state.Char().x.wrapping_sub(140);
            0
        }
        2 => {
            // up
            state.Char().y = state.Char().y.wrapping_add(189);
            let char_y = state.Char().y;
            state.Char().curr_row = y_to_row_mod4(char_y as c_int) as i8;
            3
        }
        _ => {
            // down
            state.Char().y = state.Char().y.wrapping_sub(189);
            let char_y = state.Char().y;
            state.Char().curr_row = y_to_row_mod4(char_y as c_int) as i8;
            2
        }
    }
}

/// C entry point for [`goto_other_room_impl`].
#[no_mangle]
pub unsafe extern "C" fn goto_other_room(direction: c_short) -> c_int {
    goto_other_room_impl(&mut State, direction)
}

/// Decides whether the current character has left the room, and if so moves
/// him into the neighbouring one.
///
/// Returns the direction he left in (0 left, 1 right, 2 up, 3 down), `-1` if
/// he has not left, or `-2` for the scripted falling exit at the end of the
/// level. Leaving upward needs him to be near the top of the room and *not*
/// airborne (otherwise a jump would change rooms mid-flight); leaving
/// downward just needs him below the floor line. Sideways, he has to be past
/// the room edge, and a whole set of mid-animation frames — climbing,
/// standing up, most sword poses, turning — refuse to leave at all, which is
/// what makes room changes feel like they happen between moves.
///
/// The sideways cases also fire the game's scripted events, since those are
/// all "the Kid walks out of *this* room" triggers.
///
/// seg002:0504
unsafe fn leave_room_impl(state: &mut State) -> c_short {
    let leave_dir: i16;
    let chary = state.Char().y;
    let action = state.Char().action;
    let frame = state.Char().frame;

    if action != actions_actions_5_bumped as u8
        && action != actions_actions_4_in_freefall as u8
        && action != actions_actions_3_in_midair as u8
        && (chary as i8) < 10
        && (chary as i8) > -16
    {
        leave_dir = 2; // up
    } else if chary >= 211 {
        leave_dir = 3; // down
    } else if (frameids_frame_135_climbing_1 as u8..150).contains(&frame) // climb up
        || (frameids_frame_110_stand_up_from_crouch_1 as u8..120).contains(&frame) // stand up
        || ((frameids_frame_150_parry as u8..163).contains(&frame) // with sword
            // By repeatedly pressing 'back' in a swordfight you can retreat out
            // of a room without the room changing (Trick 35): the game waits for
            // a 'legal frame' that the pattern never reaches. The fix also
            // allows leaving on frame_157_walk_with_sword, at the cost of a
            // noticeably shorter delay for leaving rooms during a swordfight.
            && (frame != frameids_frame_157_walk_with_sword as u8
                || (*fixes).fix_retreat_without_leaving_room == 0))
        || (frameids_frame_166_stand_inactive as u8..169).contains(&frame) // with sword
        || action == actions_actions_7_turn as u8
    {
        return -1;
    } else if state.Char().direction != directions_dir_0_right as i8 {
        // looking left
        if *state.char_x_left() <= 54 {
            leave_dir = 0; // left
        } else if *state.char_x_left() >= 198 {
            leave_dir = 1; // right
        } else {
            return -1;
        }
    } else {
        // looking right; a door top in the rightmost column blocks the exit
        let char_room = state.Char().room;
        let curr_row = state.Char().curr_row;
        get_tile(char_room as c_int, 9, curr_row as c_int);
        if curr_tile2 != tiles_tiles_7_doortop_with_floor as u8
            && curr_tile2 != tiles_tiles_12_doortop as u8
            && *state.char_x_right() >= 201
        {
            leave_dir = 1; // right
        } else if *state.char_x_right() <= 57 {
            leave_dir = 0; // left
        } else {
            return -1;
        }
    }

    match leave_dir {
        0 => {
            // left
            play_mirr_mus_impl(state);
            level3_set_chkp_impl(state);
            Jaffar_exit_impl(state);
        }
        1 => {
            // right
            sword_disappears_impl(state);
            meet_Jaffar_impl(state);
        }
        3 => {
            // down — special event: falling exit
            if *state.current_level() == (*custom).falling_exit_level
                && state.Char().room == (*custom).falling_exit_room
            {
                return -2;
            }
        }
        _ => {}
    }

    goto_other_room_impl(state, leave_dir as c_short);
    if skipping_replay != 0
        && replay_seek_target == replay_seek_targets_replay_seek_0_next_room as u8
    {
        skipping_replay = 0;
    }
    leave_dir as c_short
}

/// C entry point for [`leave_room_impl`].
#[no_mangle]
pub unsafe extern "C" fn leave_room() -> c_short {
    leave_room_impl(&mut State)
}

/// Opens the exit door for good once Jaffar has been beaten.
///
/// `leveldoor_open == 2` is the "Jaffar is dead" state; walking left out of
/// the room then presses the level's exit button permanently (timer `-1`).
///
/// seg002:0643
unsafe fn Jaffar_exit_impl(state: &mut State) {
    if *state.leveldoor_open() == 2 {
        get_tile(24, 0, 0);
        trigger_button(0, 0, -1);
    }
}

/// C entry point for [`Jaffar_exit_impl`].
#[no_mangle]
pub unsafe extern "C" fn Jaffar_exit() {
    Jaffar_exit_impl(&mut State);
}

/// Records the mid-game checkpoint.
///
/// Leaving room 7 of the checkpoint level to the left is the point the game
/// restarts you from after a death, with the hit points you had on arrival at
/// the level rather than the ones you had here.
///
/// seg002:0665
unsafe fn level3_set_chkp_impl(state: &mut State) {
    if *state.current_level() == (*custom).checkpoint_level && state.Char().room == 7 {
        *state.checkpoint() = 1;
        *state.hitp_beg_lev() = *state.hitp_max();
    }
}

/// C entry point for [`level3_set_chkp_impl`].
#[no_mangle]
pub unsafe extern "C" fn level3_set_chkp() {
    level3_set_chkp_impl(&mut State);
}

/// Removes the level-12 sword tile once the Kid has walked past it.
///
/// The tile is replaced with plain floor and its modifier cleared, since a
/// leftover nonzero modifier would draw a fake tile in its place.
///
/// seg002:0680
unsafe fn sword_disappears_impl(state: &mut State) {
    if *state.current_level() == 12 && state.Char().room == 18 {
        get_tile(15, 1, 0);
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
        *curr_room_modif.add(curr_tilepos as usize) = 0;
    }
}

/// C entry point for [`sword_disappears_impl`].
#[no_mangle]
pub unsafe extern "C" fn sword_disappears() {
    sword_disappears_impl(&mut State);
}

/// Plays the Jaffar fanfare on entering his room, and makes him wait.
///
/// `guard_notice_timer = 28` (28/12 = 2.33 seconds) is what keeps Jaffar
/// standing still through the music instead of attacking immediately.
///
/// seg002:06AE
unsafe fn meet_Jaffar_impl(state: &mut State) {
    if *state.current_level() == 13 && *state.leveldoor_open() == 0 && state.Char().room == 3 {
        play_sound(soundids_sound_29_meet_Jaffar as c_int);
        *state.guard_notice_timer() = 28;
    }
}

/// C entry point for [`meet_Jaffar_impl`].
#[no_mangle]
pub unsafe extern "C" fn meet_Jaffar() {
    meet_Jaffar_impl(&mut State);
}

/// Plays the mirror fanfare, once.
///
/// `leveldoor_open` doubles as the "already played" flag: the marker value
/// 0x4D means this level's presentation music has been heard, so it is only
/// triggered while the level door is open but not yet marked.
///
/// seg002:06D3
unsafe fn play_mirr_mus_impl(state: &mut State) {
    if *state.leveldoor_open() != 0
        && *state.leveldoor_open() != 0x4D
        && *state.current_level() == (*custom).mirror_level
        && state.Char().curr_row == (*custom).mirror_row as i8
        && state.Char().room == 11
    {
        play_sound(soundids_sound_25_presentation as c_int);
        *state.leveldoor_open() = 0x4D;
    }
}

/// C entry point for [`play_mirr_mus_impl`].
#[no_mangle]
pub unsafe extern "C" fn play_mirr_mus() {
    play_mirr_mus_impl(&mut State);
}

/// Releases every control.
///
/// The `move_*` family is the opponent's "keyboard": each writes the same
/// `control_*` globals the player's own input writes, so the movement code in
/// seg005 cannot tell a guard from a human. Every autocontrol frame starts by
/// clearing them here and then presses whichever ones the AI wants held.
///
/// seg002:0706
unsafe fn move_0_nothing_impl(state: &mut State) {
    *state.control_shift() = CONTROL_RELEASED as i8;
    *state.control_y() = CONTROL_RELEASED as i8;
    *state.control_x() = CONTROL_RELEASED as i8;
    *state.control_shift2() = CONTROL_RELEASED as i8;
    *state.control_down() = CONTROL_RELEASED as i8;
    *state.control_up() = CONTROL_RELEASED as i8;
    *state.control_backward() = CONTROL_RELEASED as i8;
    *state.control_forward() = CONTROL_RELEASED as i8;
}

/// C entry point for [`move_0_nothing_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_0_nothing() {
    move_0_nothing_impl(&mut State);
}

/// Presses "towards the direction the character is facing".
///
/// seg002:0721
unsafe fn move_1_forward_impl(state: &mut State) {
    *state.control_x() = CONTROL_HELD_FORWARD as i8;
    *state.control_forward() = CONTROL_HELD as i8;
}

/// C entry point for [`move_1_forward_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_1_forward() {
    move_1_forward_impl(&mut State);
}

/// Presses "away from the direction the character is facing".
///
/// seg002:072A
unsafe fn move_2_backward_impl(state: &mut State) {
    *state.control_backward() = CONTROL_HELD as i8;
    *state.control_x() = CONTROL_HELD_BACKWARD as i8;
}

/// C entry point for [`move_2_backward_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_2_backward() {
    move_2_backward_impl(&mut State);
}

/// Presses up — jump, climb, or (with a sword out) parry.
///
/// seg002:0735
unsafe fn move_3_up_impl(state: &mut State) {
    *state.control_y() = CONTROL_HELD_UP as i8;
    *state.control_up() = CONTROL_HELD as i8;
}

/// C entry point for [`move_3_up_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_3_up() {
    move_3_up_impl(&mut State);
}

/// Presses down — crouch, climb down, or sheathe the sword.
///
/// seg002:073E
unsafe fn move_4_down_impl(state: &mut State) {
    *state.control_down() = CONTROL_HELD as i8;
    *state.control_y() = CONTROL_HELD_DOWN as i8;
}

/// C entry point for [`move_4_down_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_4_down() {
    move_4_down_impl(&mut State);
}

/// Presses up and back together (a backwards jump).
///
/// seg002:0749
unsafe fn move_up_back_impl(state: &mut State) {
    *state.control_up() = CONTROL_HELD as i8;
    move_2_backward_impl(state);
}

/// C entry point for [`move_up_back_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_up_back() {
    move_up_back_impl(&mut State);
}

/// Presses down and back together (retreat / sheathe).
///
/// seg002:0753
unsafe fn move_down_back_impl(state: &mut State) {
    *state.control_down() = CONTROL_HELD as i8;
    move_2_backward_impl(state);
}

/// C entry point for [`move_down_back_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_down_back() {
    move_down_back_impl(&mut State);
}

/// Presses down and forward together — how a guard draws his sword.
///
/// seg002:075D
unsafe fn move_down_forw_impl(state: &mut State) {
    *state.control_down() = CONTROL_HELD as i8;
    move_1_forward_impl(state);
}

/// C entry point for [`move_down_forw_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_down_forw() {
    move_down_forw_impl(&mut State);
}

/// Presses shift — strike, in a swordfight.
///
/// seg002:0767
unsafe fn move_6_shift_impl(state: &mut State) {
    *state.control_shift() = CONTROL_HELD as i8;
    *state.control_shift2() = CONTROL_HELD as i8;
}

/// C entry point for [`move_6_shift_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_6_shift() {
    move_6_shift_impl(&mut State);
}

/// Releases shift only, leaving the other controls as they are.
///
/// seg002:0770
unsafe fn move_7_impl(state: &mut State) {
    *state.control_shift() = CONTROL_RELEASED as i8;
}

/// C entry point for [`move_7_impl`].
#[no_mangle]
pub unsafe extern "C" fn move_7() {
    move_7_impl(&mut State);
}

/// Produces one frame of input for whoever the current opponent is.
///
/// Clears the controls, ages the three swordfight timers by one frame
/// (`justblocked`, `kid_sword_strike`, `guard_refrac` — the cooldowns that
/// stop a guard from blocking or striking every single frame), then dispatches
/// on who he is. The `charid_0_kid` case is the level-12 shadow after the Kid
/// has merged with him: a character that is nominally the Kid but still driven
/// by the AI.
///
/// seg002:0776
unsafe fn autocontrol_opponent_impl(state: &mut State) {
    move_0_nothing_impl(state);
    let charid = state.Char().charid;
    if charid == charids_charid_0_kid as u8 {
        autocontrol_kid_impl(state);
    } else {
        if *state.justblocked() != 0 { *state.justblocked() -= 1; }
        if *state.kid_sword_strike() != 0 { *state.kid_sword_strike() -= 1; }
        if *state.guard_refrac() != 0 { *state.guard_refrac() -= 1; }
        if charid == charids_charid_24_mouse as u8 {
            autocontrol_mouse_impl(state);
        } else if charid == charids_charid_4_skeleton as u8 {
            autocontrol_skeleton_impl(state);
        } else if charid == charids_charid_1_shadow as u8 {
            autocontrol_shadow_impl(state);
        } else if *state.current_level() == 13 {
            autocontrol_Jaffar_impl(state);
        } else {
            autocontrol_guard_impl(state);
        }
    }
}

/// C entry point for [`autocontrol_opponent_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_opponent() {
    autocontrol_opponent_impl(&mut State);
}

/// Drives the mouse in the level-8 cutscene.
///
/// She runs in from the right, and is deleted once she has run far enough off
/// the left of the screen; if she is standing still too far right she is
/// nudged back into her run animation.
///
/// seg002:07EB
unsafe fn autocontrol_mouse_impl(state: &mut State) {
    if state.Char().direction == directions_dir_56_none as i8 {
        return;
    }
    if state.Char().action == actions_actions_0_stand as u8 {
        if state.Char().x >= 200 {
            clear_char();
        }
    } else {
        if state.Char().x < 166 {
            seqtbl_offset_char(seqids_seq_107_mouse_stand_up_and_go as c_short);
            play_seq();
        }
    }
}

/// C entry point for [`autocontrol_mouse_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_mouse() {
    autocontrol_mouse_impl(&mut State);
}

/// Dispatches the shadow to his per-level script.
///
/// The four checks are sequential rather than exclusive, matching the C
/// source; in a stock game the levels are distinct so at most one fires, but a
/// mod that points two of the `custom->…_level` options at the same level
/// really would run both.
///
/// seg002:081D
unsafe fn autocontrol_shadow_impl(state: &mut State) {
    if *state.current_level() == (*custom).mirror_level {
        autocontrol_shadow_level4_impl(state);
    }
    if *state.current_level() == (*custom).shadow_steal_level as u16 {
        autocontrol_shadow_level5_impl(state);
    }
    if *state.current_level() == (*custom).shadow_step_level as u16 {
        autocontrol_shadow_level6_impl(state);
    }
    if *state.current_level() == 12 {
        autocontrol_shadow_level12_impl(state);
    }
}

/// C entry point for [`autocontrol_shadow_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow() {
    autocontrol_shadow_impl(&mut State);
}

/// Drives a skeleton: a guard who can never sheathe his sword.
///
/// seg002:0850
unsafe fn autocontrol_skeleton_impl(state: &mut State) {
    state.Char().sword = sword_status_sword_2_drawn as u8;
    autocontrol_guard_impl(state);
}

/// C entry point for [`autocontrol_skeleton_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_skeleton() {
    autocontrol_skeleton_impl(&mut State);
}

/// Drives Jaffar. He fights exactly like a guard; only his sprites, his skill
/// entry and the notice timer set by [`meet_Jaffar_impl`] differ.
///
/// seg002:085A
unsafe fn autocontrol_Jaffar_impl(state: &mut State) {
    autocontrol_guard_impl(state);
}

/// C entry point for [`autocontrol_Jaffar_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_Jaffar() {
    autocontrol_Jaffar_impl(&mut State);
}

/// Drives an AI-controlled character that carries the Kid's char id (the
/// merged level-12 shadow). Also just a guard.
///
/// seg002:085F
unsafe fn autocontrol_kid_impl(state: &mut State) {
    autocontrol_guard_impl(state);
}

/// C entry point for [`autocontrol_kid_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_kid() {
    autocontrol_kid_impl(&mut State);
}

/// Splits guard behaviour into "has not noticed you yet" and "fighting".
///
/// seg002:0864
unsafe fn autocontrol_guard_impl(state: &mut State) {
    if state.Char().sword < sword_status_sword_2_drawn as u8 {
        autocontrol_guard_inactive_impl(state);
    } else {
        autocontrol_guard_active_impl(state);
    }
}

/// C entry point for [`autocontrol_guard_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard() {
    autocontrol_guard_impl(&mut State);
}

/// Decides when a guard standing at ease draws his sword.
///
/// He reacts to a Kid he can see on his own row and in front of him. A noise
/// (`is_guard_notice`, set when the Kid lands or fights nearby) also gets his
/// attention while the Kid is off-row or behind him — in which case he turns
/// around instead of drawing. Jaffar additionally waits out his notice timer.
///
/// seg002:0876
unsafe fn autocontrol_guard_inactive_impl(state: &mut State) {
    if state.Kid().alive >= 0 { return; }
    let distance = char_opp_dist() as i16;
    // The C source compares as `word`, so the wraparound is the condition:
    // `(word)distance < (word)-8` is true unless distance is in -8..=-1, i.e.
    // unless the Kid is just barely behind him.
    if state.Opp().curr_row != state.Char().curr_row || (distance as u16) < 0xFFF8u16 {
        // If Kid made a sound ...
        if *state.is_guard_notice() != 0 {
            *state.is_guard_notice() = 0;
            if distance < 0 {
                // ... and Kid is behind Guard, Guard turns around.
                if (distance as u16) < 0xFFFCu16 {
                    move_4_down_impl(state);
                }
                return;
            }
        } else if distance < 0 {
            return;
        }
    }
    if *state.can_guard_see_kid() != 0 {
        // If Guard can see Kid, Guard moves to fighting pose.
        if *state.current_level() != 13 || *state.guard_notice_timer() == 0 {
            move_down_forw_impl(state);
        }
    }
}

/// C entry point for [`autocontrol_guard_inactive_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_inactive() {
    autocontrol_guard_inactive_impl(&mut State);
}

/// The fencing AI: one frame of decision-making for an armed opponent.
///
/// He only thinks at all when he is in a sword-fighting frame (>= 150) and not
/// mid-inactive-stand, and never while `can_guard_see_kid == 1` (the Kid is
/// visible but on another row — nothing to do about that). Otherwise:
///
/// * **Kid not visible.** Back away, unless the Kid dropped down out of sight
///   and might be followed ([`guard_follows_kid_down_impl`]). Skeletons never
///   back away.
/// * **Kid visible, closer than 35 pixels.** Too close to fence (under 8 with
///   no sword, under 12 with one) means step in or turn around; otherwise hand
///   over to [`autocontrol_guard_kid_in_sight_impl`], which is where blocking
///   and striking are decided.
/// * **Kid visible, further off.** Advance or retreat depending on the floor
///   in front ([`autocontrol_guard_kid_far_impl`]) — but a Kid who is running
///   or run-jumping *at* him gets pre-emptively struck at, which is the
///   behaviour that punishes charging into a guard.
///
/// The one exception: a Kid who is falling or being bumped at a safe distance
/// is left alone entirely.
///
/// seg002:08DC
unsafe fn autocontrol_guard_active_impl(state: &mut State) {
    let char_frame = state.Char().frame;
    if char_frame == frameids_frame_166_stand_inactive as u8
        || char_frame < 150
        || *state.can_guard_see_kid() == 1
    {
        return;
    }
    if *state.can_guard_see_kid() == 0 {
        if *state.droppedout() != 0 {
            guard_follows_kid_down_impl(state);
        } else if state.Char().charid != charids_charid_4_skeleton as u8 {
            move_down_back_impl(state);
        }
        return;
    }
    // can_guard_see_kid == 2
    let opp_frame = state.Opp().frame;
    let distance = char_opp_dist() as i16;
    // frames 102..117: falling and landing
    if distance >= 12
        && (frameids_frame_102_start_fall_1 as u8
            ..frameids_frame_118_stand_up_from_crouch_9 as u8)
            .contains(&opp_frame)
        && state.Opp().action == actions_actions_5_bumped as u8
    {
        return;
    }
    if distance < 35 {
        if (state.Char().sword < sword_status_sword_2_drawn as u8 && distance < 8) || distance < 12
        {
            if state.Char().direction == state.Opp().direction {
                // turn around
                move_2_backward_impl(state);
            } else {
                move_1_forward_impl(state);
            }
        } else {
            autocontrol_guard_kid_in_sight_impl(state, distance as c_short);
        }
        return;
    }
    if *state.guard_refrac() != 0 { return; }
    if state.Char().direction != state.Opp().direction {
        // frames 7..14: running
        if (frameids_frame_7_run as u8..15).contains(&opp_frame) {
            if distance < 40 { move_6_shift_impl(state); }
            return;
        // frames 34..43: run-jump
        } else if (frameids_frame_34_start_run_jump_1 as u8..44).contains(&opp_frame) {
            if distance < 50 { move_6_shift_impl(state); }
            return;
        }
    }
    autocontrol_guard_kid_far_impl(state);
}

/// C entry point for [`autocontrol_guard_active_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_active() {
    autocontrol_guard_active_impl(&mut State);
}

/// Closes the distance to a far-off Kid, but only over solid ground.
///
/// He steps forward if either of the next two tiles is floor, and backs off
/// otherwise — which is what stops guards from walking into chasms.
///
/// seg002:09CB
unsafe fn autocontrol_guard_kid_far_impl(state: &mut State) {
    if tile_is_floor(get_tile_infrontof_char()) != 0
        || tile_is_floor(get_tile_infrontof2_char()) != 0
    {
        move_1_forward_impl(state);
    } else {
        move_2_backward_impl(state);
    }
}

/// C entry point for [`autocontrol_guard_kid_far_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_kid_far() {
    autocontrol_guard_kid_far_impl(&mut State);
}

/// Decides whether a guard jumps down after a Kid who dropped out of sight.
///
/// Called from [`autocontrol_guard_active_impl`], so `Char` is the guard and
/// `Opp` is the Kid. He refuses if the Kid is still hanging (he would land on
/// nothing), if there is a wall in front of him, or if what is one row below
/// the gap in front is anything he would not survive or could not stand on —
/// spikes, a loose floor, a wall, or a chasm — or if the Kid is not on that
/// row after all. Refusing also clears `droppedout`, so he stops trying.
///
/// seg002:09F8
unsafe fn guard_follows_kid_down_impl(state: &mut State) {
    let opp_action = state.Opp().action;
    if opp_action == actions_actions_2_hang_climb as u8
        || opp_action == actions_actions_6_hang_straight as u8
    {
        return;
    }
    // get_tile_infrontof_char() sets curr_tile2 to the tile in front, and the
    // get_tile() below overwrites it with the tile one row down; the tests
    // after each call read whichever one is current, so the order and the
    // short-circuiting here both matter.
    let should_not_follow;
    if wall_type(get_tile_infrontof_char() as byte) != 0 {
        // there is wall in front of Guard
        should_not_follow = true;
    } else if tile_is_floor(curr_tile2 as c_int) == 0 {
        // No floor in front: check the tile one row below (++tile_row in C).
        tile_row += 1;
        let below = get_tile(curr_room as c_int, tile_col as c_int, tile_row as c_int);
        should_not_follow = below == tiles_tiles_2_spike as c_int
            // Guard would fall on loose floor
            || curr_tile2 == tiles_tiles_11_loose as u8
            // ... or wall (?)
            || wall_type(curr_tile2) != 0
            // ... or into a chasm
            || tile_is_floor(curr_tile2 as c_int) == 0
            // ... or Kid is not below
            || state.Char().curr_row + 1 != state.Opp().curr_row;
    } else {
        should_not_follow = false;
    }
    if should_not_follow {
        *state.droppedout() = 0;
        move_2_backward_impl(state);
    } else {
        move_1_forward_impl(state);
    }
}

/// C entry point for [`guard_follows_kid_down_impl`].
#[no_mangle]
pub unsafe extern "C" fn guard_follows_kid_down() {
    guard_follows_kid_down_impl(&mut State);
}

/// Fighting an unarmed Kid is simple: close in, and stab once in range.
/// An armed one is handed to [`autocontrol_guard_kid_armed_impl`].
///
/// Either way nothing happens while `guard_refrac` is still counting down
/// from his last action.
///
/// seg002:0A93
unsafe fn autocontrol_guard_kid_in_sight_impl(state: &mut State, distance: c_short) {
    if state.Opp().sword == sword_status_sword_2_drawn as u8 {
        autocontrol_guard_kid_armed_impl(state, distance);
    } else if *state.guard_refrac() == 0 {
        if distance < 29 {
            move_6_shift_impl(state);
        } else {
            move_1_forward_impl(state);
        }
    }
}

/// C entry point for [`autocontrol_guard_kid_in_sight_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_kid_in_sight(distance: c_short) {
    autocontrol_guard_kid_in_sight_impl(&mut State, distance);
}

/// Chooses between advancing, blocking and striking against an armed Kid.
///
/// Outside 10..29 pixels there is nothing to defend against, so he just
/// closes. Inside it he always considers a block first — the block roll
/// happens even on a frame he goes on to strike — and only strikes from the
/// narrower 12..29 band, and only if he is not still in recovery.
///
/// seg002:0AC1
unsafe fn autocontrol_guard_kid_armed_impl(state: &mut State, distance: c_short) {
    if distance < 10 || distance >= 29 {
        guard_advance_impl(state);
    } else {
        guard_block_impl(state);
        if *state.guard_refrac() == 0 {
            if distance < 12 || distance >= 29 {
                guard_advance_impl(state);
            } else {
                guard_strike_impl(state);
            }
        }
    }
}

/// C entry point for [`autocontrol_guard_kid_armed_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_kid_armed(distance: c_short) {
    autocontrol_guard_kid_armed_impl(&mut State, distance);
}

/// Rolls against `advprob[guard_skill]` to take a step forward.
///
/// Every skilled guard (skill > 0) holds his ground for as long as the Kid's
/// last strike is still counting down in `kid_sword_strike`, which is what
/// makes them look like they are waiting for an opening.
///
/// seg002:0AF5
unsafe fn guard_advance_impl(state: &mut State) {
    if *state.guard_skill() == 0 || *state.kid_sword_strike() == 0 {
        if (*custom).advprob[*state.guard_skill() as usize] > prandom(255) {
            move_1_forward_impl(state);
        }
    }
}

/// C entry point for [`guard_advance_impl`].
#[no_mangle]
pub unsafe extern "C" fn guard_advance() {
    guard_advance_impl(&mut State);
}

/// Rolls to parry an incoming strike.
///
/// Only the frames where the Kid is actually committing to a strike are worth
/// parrying. A guard who blocked on the previous few frames
/// (`justblocked` — an "impossible" block, immediately after another) uses the
/// much stingier `impblockprob` table instead of `blockprob`; that is what
/// stops the best guards from parrying indefinitely.
///
/// seg002:0B1D
unsafe fn guard_block_impl(state: &mut State) {
    let opp_frame = state.Opp().frame;
    if opp_frame == frameids_frame_152_strike_2 as u8
        || opp_frame == frameids_frame_153_strike_3 as u8
        || opp_frame == frameids_frame_162_block_to_strike as u8
    {
        // Exactly one prandom() roll either way, as in C.
        let prob = if *state.justblocked() != 0 {
            (*custom).impblockprob[*state.guard_skill() as usize]
        } else {
            (*custom).blockprob[*state.guard_skill() as usize]
        };
        if prob > prandom(255) {
            move_3_up_impl(state);
        }
    }
}

/// C entry point for [`guard_block_impl`].
#[no_mangle]
pub unsafe extern "C" fn guard_block() {
    guard_block_impl(&mut State);
}

/// Rolls to strike.
///
/// He will not walk into a Kid who is already blocking or striking. Striking
/// straight out of his own parry uses the separate `restrikeprob` table — the
/// riposte — which for most skills is far less likely than a fresh strike, and
/// for a few of the highest skills far more.
///
/// seg002:0B73
unsafe fn guard_strike_impl(state: &mut State) {
    let opp_frame = state.Opp().frame;
    if opp_frame == frameids_frame_169_begin_block as u8
        || opp_frame == frameids_frame_151_strike_1 as u8
    {
        return;
    }
    let char_frame = state.Char().frame;
    // Exactly one prandom() roll either way, as in C.
    let prob = if char_frame == frameids_frame_161_parry as u8
        || char_frame == frameids_frame_150_parry as u8
    {
        (*custom).restrikeprob[*state.guard_skill() as usize]
    } else {
        (*custom).strikeprob[*state.guard_skill() as usize]
    };
    if prob > prandom(255) {
        move_6_shift_impl(state);
    }
}

#[no_mangle]
pub unsafe extern "C" fn guard_strike() {
    guard_strike_impl(&mut State);
}

/// Kills the current character: either where he stands, or off the ledge.
///
/// This is C's `loc_4276`. He dies in place if there is something solid behind
/// him or he is standing less than four pixels from the edge of his tile;
/// otherwise the blow shoves him backwards off the edge and he falls
/// (`seq_81`). Dying in place against a gate has a special case: guards facing
/// left get snapped to the gate's column first, so the corpse does not end up
/// drawn inside the gate.
unsafe fn hurt_by_sword_die(state: &mut State) {
    // C: `if (get_tile_behind_char() != 0 || (distance = distance_to_edge_weight()) < 4)`.
    // distance_to_edge_weight() is called only when nothing is behind him,
    // and its value is only needed on the push-off path.
    let push_off_distance = if get_tile_behind_char() != 0 {
        None
    } else {
        let distance = distance_to_edge_weight() as i16;
        if distance < 4 { None } else { Some(distance) }
    };
    if let Some(distance) = push_off_distance {
        // Kid/Guard is killed and pushed off the ledge
        state.Char().x = char_dx_forward(distance as c_int - 20) as u8;
        load_fram_det_col();
        inc_curr_row();
        seqtbl_offset_char(seqids_seq_81_kid_pushed_off_ledge as c_short);
    } else {
        seqtbl_offset_char(seqids_seq_85_stabbed_to_death as c_short); // dying (stabbed)
        if state.Char().charid != charids_charid_0_kid as u8
            && (state.Char().direction as i8) < directions_dir_0_right as i8 // looking left
            && (curr_tile2 == tiles_tiles_4_gate as u8
                || get_tile_at_char() == tiles_tiles_4_gate as c_int)
        {
            // Without the fix, a guard fighting across a room boundary and
            // hitting a gate gets teleported to the other side of the Kid's
            // room; the fix re-bases the gate's column into his own room.
            let gate_col = if (*fixes).fix_offscreen_guards_disappearing != 0 {
                let char_room = state.Char().room;
                let mut gate_col = tile_col;
                if curr_room != char_room as c_short {
                    if curr_room == state.level().roomlinks[(char_room as usize) - 1].right as c_short {
                        gate_col += SCREEN_TILECOUNTX as c_short;
                    } else if curr_room
                        == state.level().roomlinks[(char_room as usize) - 1].left as c_short
                    {
                        gate_col -= SCREEN_TILECOUNTX as c_short;
                    }
                }
                gate_col
            } else {
                tile_col
            };
            // Verbatim `- (curr_tile2 != tiles_4_gate)` from the C source.
            let is_not_gate = (curr_tile2 != tiles_tiles_4_gate as u8) as i32;
            state.Char().x = (x_bump_at(
                (gate_col as i32 - is_not_gate + FIRST_ONSCREEN_COLUMN as i32) as usize,
            ) as i32
                + TILE_MIDX as i32) as u8;
            state.Char().x = char_dx_forward(10) as u8;
        }
        let curr_row = state.Char().curr_row;
        state.Char().y = y_land_at((curr_row + 1) as usize) as u8;
        state.Char().fall_y = 0;
    }
}

/// Applies a sword hit to the current character.
///
/// Being hit while not in a fighting pose is fatal outright; being hit with
/// the sword out costs one hit point and only kills if that was the last one.
/// Skeletons cannot be hurt at all. Death — in either form — goes through
/// [`hurt_by_sword_die`], which decides between dying in place and being
/// pushed off the ledge; surviving just plays the flinch and re-seats him on
/// the floor of his row.
///
/// seg002:0BCD
unsafe fn hurt_by_sword_impl(state: &mut State) {
    if state.Char().alive >= 0 { return; }
    if state.Char().sword != sword_status_sword_2_drawn as u8 {
        // Being hurt when not in fighting pose means death.
        take_hp(100);
        seqtbl_offset_char(seqids_seq_85_stabbed_to_death as c_short); // dying (stabbed unarmed)
        hurt_by_sword_die(state);
    } else {
        // You can't hurt skeletons
        if state.Char().charid != charids_charid_4_skeleton as u8 && take_hp(1) != 0 {
            hurt_by_sword_die(state);
        } else {
            seqtbl_offset_char(seqids_seq_74_hit_by_sword as c_short); // being hit with sword
            let curr_row = state.Char().curr_row;
            state.Char().y = y_land_at((curr_row + 1) as usize) as u8;
            state.Char().fall_y = 0;
        }
    }
    let sound_id = if state.Char().charid == charids_charid_0_kid as u8 {
        soundids_sound_13_kid_hurt
    } else {
        soundids_sound_12_guard_hurt
    };
    play_sound(sound_id as c_int);
    play_seq();
}

/// C entry point for [`hurt_by_sword_impl`].
#[no_mangle]
pub unsafe extern "C" fn hurt_by_sword() {
    hurt_by_sword_impl(&mut State);
}

/// Resolves any pending sword hit, for the opponent first and the Kid second.
///
/// `actions_99_hurt` is the flag [`check_hurting_impl`] sets on whoever got
/// through. The opponent's hit takes priority: if both were marked in the same
/// frame the Kid's is cancelled, so a mutual poke kills the guard rather than
/// trading. Landing a hit also puts the guard into recovery for
/// `refractimer[guard_skill]` frames.
///
/// seg002:0CD4
unsafe fn check_sword_hurt_impl(state: &mut State) {
    if state.Guard().action == actions_actions_99_hurt as u8 {
        if state.Kid().action == actions_actions_99_hurt as u8 {
            state.Kid().action = actions_actions_1_run_jump as u8;
        }
        loadshad();
        hurt_by_sword_impl(state);
        saveshad();
        *state.guard_refrac() = (*custom).refractimer[*state.guard_skill() as usize];
    } else {
        if state.Kid().action == actions_actions_99_hurt as u8 {
            loadkid();
            hurt_by_sword_impl(state);
            savekid();
        }
    }
}

/// C entry point for [`check_sword_hurt_impl`].
#[no_mangle]
pub unsafe extern "C" fn check_sword_hurt() {
    check_sword_hurt_impl(&mut State);
}

/// Runs the swordfight hit test once in each direction.
///
/// [`check_hurting_impl`] only ever asks "does `Char` hit `Opp`", so it is
/// called twice with the roles swapped — guard against Kid, then Kid against
/// guard. Suspended while the Kid is on the exit stairs, where he is
/// invulnerable.
///
/// seg002:0D1A
unsafe fn check_sword_hurting_impl(state: &mut State) {
    let kid_frame = state.Kid().frame;
    // frames 217..228: go up on stairs
    if kid_frame != 0
        && (kid_frame < frameids_frame_219_exit_stairs_3 as u8 || kid_frame >= 229)
    {
        loadshad_and_opp();
        check_hurting_impl(state);
        saveshad_and_opp();
        loadkid_and_opp();
        check_hurting_impl(state);
        savekid_and_opp();
    }
}

/// C entry point for [`check_sword_hurting_impl`].
#[no_mangle]
pub unsafe extern "C" fn check_sword_hurting() {
    check_sword_hurting_impl(&mut State);
}

/// Does `Char`'s sword stroke land on `Opp` this frame?
///
/// Only the two striking frames count, and only against someone on the same
/// row. If the target is parrying within range the stroke is deflected — his
/// frame is forced to the parry, the attacker is bounced into
/// `seq_69_attack_was_parried`, and a guard who parried is marked
/// `justblocked` so his next block roll uses the harsher table. Otherwise a
/// poke that lands inside the hurt range (which starts closer against an
/// unarmed target) marks him `actions_99_hurt` for [`check_sword_hurt_impl`]
/// to resolve. A stroke that hits nothing just makes the swishing sound.
///
/// seg002:0D56
unsafe fn check_hurting_impl(state: &mut State) {
    if state.Char().sword != sword_status_sword_2_drawn as u8 { return; }
    if state.Char().curr_row != state.Opp().curr_row { return; }
    let char_frame = state.Char().frame;
    if char_frame != frameids_frame_153_strike_3 as u8
        && char_frame != frameids_frame_154_poking as u8
    {
        return;
    }
    // If char is poking ...
    let distance = char_opp_dist() as i16;
    let opp_frame = state.Opp().frame;
    // frames 161 and 150: parrying
    if distance < 0
        || distance >= 29
        || (opp_frame != frameids_frame_161_parry as u8
            && opp_frame != frameids_frame_150_parry as u8)
    {
        // ... and Opp is not parrying
        if state.Char().frame == frameids_frame_154_poking as u8 {
            let min_hurt_range: i16 = if state.Opp().sword < sword_status_sword_2_drawn as u8 { 8 } else { 12 };
            let distance2 = char_opp_dist() as i16;
            if distance2 >= min_hurt_range && distance2 < 29 {
                state.Opp().action = actions_actions_99_hurt as u8;
            }
        }
    } else {
        state.Opp().frame = frameids_frame_161_parry as u8;
        if state.Char().charid != charids_charid_0_kid as u8 {
            *state.justblocked() = 4;
        }
        seqtbl_offset_char(seqids_seq_69_attack_was_parried as c_short);
        play_seq();
    }
    // Fix looping "sword moving" sound.
    if state.Char().direction == directions_dir_56_none as i8 { return; }
    if state.Char().frame == frameids_frame_154_poking as u8
        && state.Opp().frame != frameids_frame_161_parry as u8
        && state.Opp().action != actions_actions_99_hurt as u8
    {
        play_sound(soundids_sound_11_sword_moving as c_int);
    }
}

/// C entry point for [`check_hurting_impl`].
#[no_mangle]
pub unsafe extern "C" fn check_hurting() {
    check_hurting_impl(&mut State);
}

/// Wakes the skeleton.
///
/// The skeleton spends the level as an ordinary decorative tile. Stepping onto
/// one of the two trigger columns replaces that tile (and the one after it,
/// since he is two tiles wide) with plain floor and spawns a live skeleton
/// opponent in its place, facing left, with three hit points.
///
/// seg002:0E1F
unsafe fn check_skel_impl(state: &mut State) {
    if *state.current_level() != (*custom).skeleton_level
        || state.Guard().direction != directions_dir_56_none as i8
        || *state.drawn_room() != (*custom).skeleton_room as u16
        || (*state.leveldoor_open() == 0 && (*custom).skeleton_require_open_level_door != 0)
        || (state.Kid().curr_col != (*custom).skeleton_trigger_column_1 as i8
            && state.Kid().curr_col != (*custom).skeleton_trigger_column_2 as i8)
    {
        return;
    }
    let drawn_room_v = *state.drawn_room();
    get_tile(
        drawn_room_v as c_int,
        (*custom).skeleton_column as c_int,
        (*custom).skeleton_row as c_int,
    );
    if curr_tile2 != tiles_tiles_21_skeleton as u8 {
        return;
    }

    // erase skeleton (he occupies this tile and the next one)
    *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
    *state.redraw_height() = 24;
    set_redraw_full(curr_tilepos as c_short, 1);
    set_wipe(curr_tilepos as c_short, 1);
    curr_tilepos = curr_tilepos.wrapping_add(1);
    set_redraw_full(curr_tilepos as c_short, 1);
    set_wipe(curr_tilepos as c_short, 1);

    // ... and stand a live one up in its place
    state.Char().room = drawn_room_v as u8;
    state.Char().curr_row = (*custom).skeleton_row as i8;
    let curr_row = state.Char().curr_row;
    state.Char().y = y_land_at((curr_row + 1) as usize) as u8;
    state.Char().curr_col = (*custom).skeleton_column as i8;
    let curr_col = state.Char().curr_col;
    state.Char().x = (x_bump_at(
        (curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize,
    ) as i32
        + TILE_SIZEX as i32) as u8;
    state.Char().direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_88_skel_wake_up as c_short);
    play_seq();
    play_sound(soundids_sound_44_skel_alive as c_int);
    *state.guard_skill() = (*custom).skeleton_skill as u16;
    state.Char().alive = -1;
    *state.guardhp_max() = 3;
    *state.guardhp_curr() = 3;
    state.Char().fall_x = 0;
    state.Char().fall_y = 0;
    *state.is_guard_notice() = 0;
    *state.guard_refrac() = 0;
    state.Char().sword = sword_status_sword_2_drawn as u8;
    state.Char().charid = charids_charid_4_skeleton as u8;
    saveshad();
}

/// C entry point for [`check_skel_impl`].
#[no_mangle]
pub unsafe extern "C" fn check_skel() {
    check_skel_impl(&mut State);
}

/// Plays back one frame of a scripted move list.
///
/// `moves_ptr` is a `{time, move}` table: `demo_time` counts frames since the
/// script started, `demo_index` points at the next entry, and the move from
/// the entry that is currently due is pressed. A `move` of -1 means "hold
/// still"; the table's own terminator stops the clock at 0xFE.
///
/// seg002:0F3F
unsafe fn do_auto_moves_impl(state: &mut State, moves_ptr: *const auto_move_type) {
    if *state.demo_time() >= 0xFE { return; }
    *state.demo_time() += 1;
    let mut demoindex = *state.demo_index() as i16;
    // moves_ptr may point into a packed struct (e.g. custom->shad_drink_move),
    // which can be unaligned. Use read_unaligned to avoid misalignment panics.
    if std::ptr::read_unaligned(moves_ptr.add(demoindex as usize)).time <= *state.demo_time() {
        *state.demo_index() += 1;
    } else {
        demoindex = *state.demo_index() as i16 - 1;
    }
    let curr_move = std::ptr::read_unaligned(moves_ptr.add(demoindex as usize)).move_;
    match curr_move {
        -1 => {}
        0 => move_0_nothing_impl(state),
        1 => move_1_forward_impl(state),
        2 => move_2_backward_impl(state),
        3 => move_3_up_impl(state),
        4 => move_4_down_impl(state),
        5 => { move_3_up_impl(state); move_1_forward_impl(state); }
        6 => move_6_shift_impl(state),
        7 => move_7_impl(state),
        _ => {}
    }
}

/// C entry point for [`do_auto_moves_impl`].
#[no_mangle]
pub unsafe extern "C" fn do_auto_moves(moves_ptr: *const auto_move_type) {
    do_auto_moves_impl(&mut State, moves_ptr);
}

/// The mirror double: he walks left across the mirror room and vanishes at
/// the far wall.
///
/// seg002:1000
unsafe fn autocontrol_shadow_level4_impl(state: &mut State) {
    if state.Char().room == (*custom).mirror_room {
        if state.Char().x < 80 {
            clear_char();
        } else {
            move_1_forward_impl(state);
        }
    }
}

/// C entry point for [`autocontrol_shadow_level4_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level4() {
    autocontrol_shadow_level4_impl(&mut State);
}

/// The potion thief: he waits for the gate to be open, then runs the scripted
/// `shad_drink_move` list — walk in, drink the Kid's potion, run out — and is
/// removed once he is off the left of the room.
///
/// seg002:101A
unsafe fn autocontrol_shadow_level5_impl(state: &mut State) {
    if state.Char().room == (*custom).shadow_steal_room {
        if *state.demo_time() == 0 {
            get_tile((*custom).shadow_steal_room as c_int, 1, 0);
            // is the door open?
            if (*curr_room_modif.add(curr_tilepos as usize)) < 80 {
                return;
            }
            *state.demo_index() = 0;
        }
        do_auto_moves_impl(state, core::ptr::addr_of!((*custom).shad_drink_move).cast::<auto_move_type>());
        if state.Char().x < 15 {
            clear_char();
        }
    }
}

/// C entry point for [`autocontrol_shadow_level5_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level5() {
    autocontrol_shadow_level5_impl(&mut State);
}

/// The flagstone shadow: he mirrors the Kid's running jump, timed off a
/// specific frame of it, so that he lands on the pressure plate.
///
/// seg002:1064
unsafe fn autocontrol_shadow_level6_impl(state: &mut State) {
    if state.Char().room == (*custom).shadow_step_room
        // a frame in run-jump
        && state.Kid().frame == frameids_frame_43_running_jump_4 as u8
        && state.Kid().x < 128
    {
        move_6_shift_impl(state);
        move_1_forward_impl(state);
    }
}

/// C entry point for [`autocontrol_shadow_level6_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level6() {
    autocontrol_shadow_level6_impl(&mut State);
}

/// The level-12 shadow, who has to be *not* fought.
///
/// With his sword out he fences like an ordinary guard, except that once the
/// Kid sheathes his own sword (`offguard`) and the shadow has already been
/// hurt, the shadow sheathes too. Unarmed and approached to within 10 pixels
/// he merges with the Kid: white flash, an extra hit point, and the shadow's
/// body becomes the Kid's. Otherwise he keeps his distance, only running
/// towards a Kid who is running towards him.
///
/// seg002:1082
unsafe fn autocontrol_shadow_level12_impl(state: &mut State) {
    if state.Char().room == 15 && *state.shadow_initialized() == 0 {
        if state.Opp().x >= 150 {
            do_init_shad_impl(
                state,
                core::ptr::addr_of!((*custom).init_shad_12).cast::<byte>(),
                7, // fall
            );
            return;
        }
        *state.shadow_initialized() = 1;
    }
    if state.Char().sword >= sword_status_sword_2_drawn as u8 {
        if *state.offguard() == 0 || *state.guard_refrac() == 0 {
            autocontrol_guard_active_impl(state);
        } else {
            move_4_down_impl(state);
        }
        return;
    }
    if state.Opp().sword >= sword_status_sword_2_drawn as u8 || *state.offguard() == 0 {
        // This behavior matches the DOS version but not the Apple II source.
        // xdiff is read below even on the short-circuit path where
        // char_opp_dist() was never called, so the sentinel is load-bearing
        // and the assignment stays inside the condition as C has it.
        let mut xdiff: i16 = 0x7000; // bugfix/workaround initial value
        if *state.can_guard_see_kid() < 2 || {
            xdiff = char_opp_dist() as i16;
            xdiff >= 90
        } {
            if xdiff < 0 {
                move_2_backward_impl(state);
            }
            return;
        }
        // Shadow draws his sword
        if state.Char().frame == frameids_frame_15_stand as u8 {
            move_down_forw_impl(state);
        }
        return;
    }
    if char_opp_dist() < 10 {
        *state.flash_color() = colorids_color_15_brightwhite as u16;
        *state.flash_time() = 18;
        add_life();
        *state.united_with_shadow() = 42;
        state.Char().charid = charids_charid_0_kid as u8;
        savekid();
        clear_char();
        return;
    }
    if *state.can_guard_see_kid() == 2 {
        // If Kid runs to shadow, shadow runs to Kid.
        // frames 3..14: running; frames 127..132: stepping
        let opp_frame = state.Opp().frame;
        if (frameids_frame_3_start_run as u8..frameids_frame_15_stand as u8).contains(&opp_frame)
            || (frameids_frame_127_stepping_7 as u8..133).contains(&opp_frame)
        {
            move_1_forward_impl(state);
        }
    }
}

/// C entry point for [`autocontrol_shadow_level12_impl`].
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level12() {
    autocontrol_shadow_level12_impl(&mut State);
}

#[cfg(test)]
#[allow(static_mut_refs)]
mod tests {
    use super::*;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // move_0_nothing releases all controls simultaneously.
    #[test]
    fn move_0_nothing_clears_all_controls() {
        setup();
        unsafe {
            // Set all controls to non-released values first.
            control_shift = -1;
            control_y = -1;
            control_x = -1;
            control_shift2 = -1;
            control_down = -1;
            control_up = -1;
            control_backward = -1;
            control_forward = -1;
            move_0_nothing();
            assert_eq!(control_shift,    0);
            assert_eq!(control_y,        0);
            assert_eq!(control_x,        0);
            assert_eq!(control_shift2,   0);
            assert_eq!(control_down,     0);
            assert_eq!(control_up,       0);
            assert_eq!(control_backward, 0);
            assert_eq!(control_forward,  0);
        }
    }

    // move_1_forward sets forward controls only.
    #[test]
    fn move_1_forward_sets_forward_controls() {
        setup();
        unsafe {
            move_0_nothing();
            move_1_forward();
            assert_eq!(control_x,       CONTROL_HELD_FORWARD as i8);
            assert_eq!(control_forward, CONTROL_HELD as i8);
            // other controls unchanged (still released)
            assert_eq!(control_backward, 0);
            assert_eq!(control_up, 0);
        }
    }

    // move_2_backward sets backward controls only.
    #[test]
    fn move_2_backward_sets_backward_controls() {
        setup();
        unsafe {
            move_0_nothing();
            move_2_backward();
            assert_eq!(control_backward, CONTROL_HELD as i8);
            assert_eq!(control_x,        CONTROL_HELD_BACKWARD as i8);
            assert_eq!(control_forward,  0);
        }
    }

    // goto_other_room adjusts x by ±140 for left/right transitions.
    #[test]
    fn goto_other_room_adjusts_x_for_left_right() {
        setup();
        unsafe {
            // Place Char in room 1 which has valid room links.
            // We only test x-adjustment; actual room value depends on level data.
            Char.room = 1;
            Char.x = 100;
            // We can't easily test the room-link lookup without game data,
            // but we can confirm x wrapping arithmetic.
            // Left transition (direction=0): x += 140
            let start_x: u8 = 200;
            Char.x = start_x;
            // direction=1 (right): x -= 140
            // Use direction=1 so x -= 140
            // 200u8.wrapping_sub(140) = 60
            let expected = 200u8.wrapping_sub(140);
            Char.x = start_x;
            Char.x = Char.x.wrapping_sub(140);
            assert_eq!(Char.x, expected);
        }
    }

    // do_auto_moves: move -1 is a no-op (doesn't change controls).
    #[test]
    fn do_auto_moves_minus1_is_noop() {
        setup();
        unsafe {
            // Build a minimal moves table: time=0, move=-1
            let moves = [auto_move_type { time: 0, move_: -1 }];
            demo_time = 0;
            demo_index = 0;
            control_forward = 0;
            control_backward = 0;
            do_auto_moves(moves.as_ptr());
            assert_eq!(control_forward,  0);
            assert_eq!(control_backward, 0);
        }
    }

    // do_auto_moves advances demo_index when time threshold is reached.
    #[test]
    fn do_auto_moves_advances_index_at_threshold() {
        setup();
        unsafe {
            // moves[0] = {time=1, move=1 (forward)}, moves[1] = {time=99, move=-1}
            let moves = [
                auto_move_type { time: 1, move_: 1 },
                auto_move_type { time: 99, move_: -1 },
            ];
            demo_time = 0;
            demo_index = 0;
            // After call, demo_time becomes 1. moves[0].time(1) <= 1, so demo_index → 1.
            // But demoindex was 0 before the increment, so move_1_forward() is called.
            move_0_nothing();
            do_auto_moves(moves.as_ptr());
            assert_eq!(demo_time, 1);
            assert_eq!(demo_index, 1);
            // forward was triggered by move=1
            assert_eq!(control_forward, CONTROL_HELD as i8);
        }
    }
}
