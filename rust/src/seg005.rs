//! Moving, falling, landing and fencing: what a character does next.
//!
//! Every character in the game — the Kid, the guards, the shadow — is at all
//! times playing back a short animation *sequence*: a byte-coded program in
//! `seqtbl.c` that says "show this frame, move this many pixels, now jump to
//! that sequence". This module is where a new sequence gets chosen. Almost
//! every function here ends the same way, by calling
//! [`seqtbl_offset_char_impl`] to point the character at a different sequence;
//! the sequence interpreter in `seg006` then plays it out frame by frame.
//!
//! There are three broad groups of decisions:
//!
//! * **Gravity.** [`do_fall`] runs once per frame while a character is in
//!   mid-air: it starts the falling scream, watches for a ledge to catch
//!   ([`check_grab`]), and when the ground arrives hands over to [`land`].
//!   Landing decides between a soft landing, a landing that costs a hit point,
//!   and being crushed, based purely on how far the character fell
//!   (`Char.fall_y`: under 22 pixels is one storey, under 33 is two, more is
//!   fatal). Landing on live spikes short-circuits all of that
//!   ([`spiked`]).
//!
//! * **Player input.** [`control`] is the top of the input funnel, called once
//!   per frame for whichever character is current. It dispatches on what the
//!   character is *currently doing* — standing, turning, starting a run,
//!   running, jumping up, hanging from a ledge, crouching — to a
//!   `control_*` function that reads the abstract control state
//!   (`control_x`/`control_y` for the direction stick, `control_shift` for the
//!   careful-step modifier, and the latched `control_up`/`control_down`/
//!   `control_forward`/`control_backward` flags) and picks a move. Pressing
//!   "up" against an open level door walks through it ([`up_pressed`]); with
//!   the teleport feature, a balcony tile with a nonzero modifier is a
//!   teleporter, and [`teleport`] finds the matching balcony elsewhere in the
//!   level and puts the Kid there.
//!
//!   Note the latching convention: a control is `CONTROL_HELD` while pressed,
//!   and a move that consumes it writes `CONTROL_IGNORE` so the same press
//!   cannot trigger the move twice. [`release_arrows`] clears the direction
//!   latches and returns the value to store, which is why so many lines read
//!   `*state.control_up() = release_arrows() as i8`.
//!
//! * **Swordplay.** Once the sword is out, [`control_with_sword`] replaces the
//!   normal movement controls: it measures the distance to the opponent and
//!   either turns to face him, closes to fencing range, or (for the Kid alone)
//!   sheathes the sword when nobody is around. Inside fencing range
//!   [`swordfight`] maps the same controls onto attack ([`sword_strike`]),
//!   block ([`parry`]), advance ([`forward_with_sword`]) and retreat
//!   ([`back_with_sword`]). The Kid draws automatically when a guard notices
//!   him ([`draw_sword`]), unless he is "offguard" — the state left behind by
//!   deliberately putting the sword away in front of a guard.
//!
//! # Step D migration
//!
//! Each function's logic lives in a private `*_impl(state: &mut State, ...)`
//! that reaches shared game state through the `State` facade (see
//! [`crate::state`]) instead of bare globals. The original
//! `#[no_mangle] pub unsafe extern "C" fn` names are kept as thin wrappers that
//! construct a `State` handle and forward. `State` borrows the same
//! `static mut` globals these wrappers used to touch directly, so there is only
//! ever one copy of the data and nothing here can diverge from unmigrated
//! callers (e.g. `get_tile`, `char_dx_forward` and the other `seg006` helpers
//! this file calls, which still read the globals directly).

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;
use crate::state::State;

// seqtbl_offsets is an extern const array from seqtbl.c (not exposed by bindgen)
extern "C" {
    pub static seqtbl_offsets: [u16; 0];
}

/// Byte offset of sequence `idx` within the sequence-table bytecode.
///
/// `seqtbl_offsets` is an incomplete extern array (`[u16; 0]` to bindgen), so
/// it has to be indexed through a raw pointer.
unsafe fn seqtbl_offsets_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(seqtbl_offsets).cast::<u16>().add(idx)
}

/// Column offset of the tile in front of a character facing `direction`,
/// indexed by `direction + 1` (0 = left, 2 = right).
///
/// Incomplete extern array — see [`seqtbl_offsets_at`].
unsafe fn dir_front_at(idx: usize) -> i8 {
    *core::ptr::addr_of!(dir_front).cast::<i8>().add(idx)
}

/// The copy-protection potions' rooms, indexed 0..14. Mutable: a potion that
/// has been drunk has its room zeroed so it is not drawn again.
///
/// Incomplete extern array — see [`seqtbl_offsets_at`].
unsafe fn copyprot_room_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_room).cast::<u16>().add(idx)
}

/// Marks copy-protection potion `idx` as consumed.
unsafe fn set_copyprot_room(idx: usize, value: u16) {
    core::ptr::addr_of_mut!(copyprot_room).cast::<u16>().add(idx).write(value);
}

/// The copy-protection potions' tile positions, indexed 0..14.
///
/// Incomplete extern array — see [`seqtbl_offsets_at`]. Note the element type
/// is `word`, not `byte`.
unsafe fn copyprot_tile_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_tile).cast::<u16>().add(idx)
}

/// Starts the current character on animation sequence `seq_index`.
///
/// This is the single verb the whole module is built around: nothing here
/// moves a character directly, it only points him at a different sequence,
/// which the interpreter then plays out.
// seg005:000A
unsafe fn seqtbl_offset_char_impl(state: &mut State, seq_index: c_short) {
    state.Char().curr_seq = seqtbl_offsets_at(seq_index as usize);
}

/// Starts the current character on animation sequence `seq_index`.
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_char(seq_index: c_short) {
    seqtbl_offset_char_impl(&mut State, seq_index);
}

/// Starts the current character's *opponent* on animation sequence
/// `seq_index` — used when a hit lands and the victim's reaction has to be
/// forced from the attacker's code.
// seg005:001D
unsafe fn seqtbl_offset_opp_impl(state: &mut State, seq_index: c_int) {
    state.Opp().curr_seq = seqtbl_offsets_at(seq_index as usize);
}

/// Starts the current character's opponent on animation sequence `seq_index`.
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_opp(seq_index: c_int) {
    seqtbl_offset_opp_impl(&mut State, seq_index);
}

/// Advances a falling character by one frame.
///
/// While there is still air below the character's row this only watches for a
/// ledge to grab and (with `fix_glide_through_wall`) stops him from drifting
/// sideways into a wall. Once the fall reaches the next floor line, the tile
/// under him decides the outcome: a wall means he is inside it
/// ([`in_wall`]), a floor means [`land_impl`], anything else means he keeps
/// falling into the room below.
// seg005:0030
unsafe fn do_fall_impl(state: &mut State) {
    if *state.is_screaming() == 0 && state.Char().fall_y >= 31 {
        play_sound(soundids_sound_1_falling as c_int);
        *state.is_screaming() = 1;
    }
    let curr_row = state.Char().curr_row;
    if (y_land_at(curr_row as usize + 1) as i32) > (state.Char().y as i32) {
        check_grab();

        // FIX_GLIDE_THROUGH_WALL
        if (*fixes).fix_glide_through_wall != 0 {
            determine_col();
            get_tile_at_char();
            if curr_tile2 == tiles_tiles_20_wall as u8
                || ((curr_tile2 == tiles_tiles_12_doortop as u8
                    || curr_tile2 == tiles_tiles_7_doortop_with_floor as u8)
                    && state.Char().direction == (directions_dir_FF_left as i8))
            {
                let delta_x = distance_to_edge_weight();
                const delta_x_reference: i32 = 10;
                if delta_x >= 8 {
                    let adj_delta_x = -5 + delta_x - delta_x_reference;
                    state.Char().x = char_dx_forward(adj_delta_x) as u8;
                    state.Char().fall_x = 0;
                }
            }
        }
    } else {
        // FIX_JUMP_THROUGH_WALL_ABOVE_GATE
        if (*fixes).fix_jump_through_wall_above_gate != 0 {
            if get_tile_at_char() != tiles_tiles_4_gate as c_int {
                determine_col();
            }
        }

        if get_tile_at_char() == tiles_tiles_20_wall as c_int {
            in_wall();
        }
        // FIX_DROP_THROUGH_TAPESTRY
        else if (*fixes).fix_drop_through_tapestry != 0
            && get_tile_at_char() == tiles_tiles_12_doortop as c_int
            && state.Char().direction == (directions_dir_FF_left as i8)
        {
            if distance_to_edge_weight() >= 8 {
                in_wall();
            }
        }

        if tile_is_floor(curr_tile2 as c_int) != 0 {
            land_impl(state);
        } else {
            inc_curr_row();
        }
    }
}

/// Advances the current character's fall by one frame.
#[no_mangle]
pub unsafe extern "C" fn do_fall() {
    do_fall_impl(&mut State);
}

/// Puts a falling character back on the ground and works out what it cost him.
///
/// Snaps him to the floor line, nudges him back from a brink or a closed gate
/// he would otherwise be standing inside, and then picks the landing
/// animation from `Char.fall_y`, the height of the fall in pixels: under 22 is
/// one storey (free), under 33 is two (one hit point for the Kid, instant
/// death for a guard), anything more is fatal. Live spikes at the landing spot
/// override all of it — see [`spiked_impl`].
// seg005:0090
unsafe fn land_impl(state: &mut State) {
    *state.is_screaming() = 0;

    // USE_SUPER_HIGH_JUMP
    if (*fixes).enable_super_high_jump != 0 {
        *state.super_jump_fall() = 0;
    }

    let curr_row = state.Char().curr_row;
    state.Char().y = y_land_at(curr_row as usize + 1) as u8;

    if get_tile_at_char() != tiles_tiles_2_spike as c_int {
        if tile_is_floor(get_tile_infrontof_char()) == 0
            && distance_to_edge_weight() < 3
        {
            state.Char().x = char_dx_forward(-3) as u8;
        }
        // FIX_LAND_AGAINST_GATE_OR_TAPESTRY
        else if (*fixes).fix_land_against_gate_or_tapestry != 0 {
            get_tile_infrontof_char();
            if state.Char().direction == (directions_dir_FF_left as i8)
                && (((curr_tile2 == tiles_tiles_4_gate as u8) && can_bump_into_gate() != 0)
                    || (curr_tile2 == tiles_tiles_7_doortop_with_floor as u8))
                && distance_to_edge_weight() < 3
            {
                state.Char().x = char_dx_forward(-3) as u8;
            }
        }

        start_chompers();
    } else {
        // fell on spikes
        if is_spike_harmful() != 0 {
            spiked_impl(state);
            return;
        }
        // FIX_SAFE_LANDING_ON_SPIKES
        else if (*fixes).fix_safe_landing_on_spikes != 0
            && curr_room_modif.add(curr_tilepos as usize).read() == 0
        {
            spiked_impl(state);
            return;
        }
    }

    // The `soft_land` blocks below are the C source's `loc_5EFD`, reached both
    // by falling through and by a `goto` from the shadow case; the duplication
    // mirrors the original rather than hoisting a helper, so the two paths can
    // never drift apart from the disassembly.
    let seq_id: u16 = if state.Char().alive < 0 {
        // alive
        if (distance_to_edge_weight() >= 12 && get_tile_behind_char() == tiles_tiles_2_spike as c_int)
            || get_tile_at_char() == tiles_tiles_2_spike as c_int
        {
            // fell on spikes
            if is_spike_harmful() != 0 {
                spiked_impl(state);
                return;
            }
            // FIX_SAFE_LANDING_ON_SPIKES
            else if (*fixes).fix_safe_landing_on_spikes != 0
                && curr_room_modif.add(curr_tilepos as usize).read() == 0
            {
                spiked_impl(state);
                return;
            }
        }

        if state.Char().fall_y < 22 {
            // fell 1 row (loc_5EFD)
            let seq = if state.Char().charid >= charids_charid_2_guard as u8
                || state.Char().sword == sword_status_sword_2_drawn as u8
            {
                state.Char().sword = sword_status_sword_2_drawn as u8;
                seqids_seq_63_guard_active_after_fall as u16
            } else {
                seqids_seq_17_soft_land as u16
            };
            if state.Char().charid == charids_charid_0_kid as u8 {
                play_sound(soundids_sound_17_soft_land as c_int);
                *state.is_guard_notice() = 1;
            }
            seq
        } else if state.Char().fall_y < 33 {
            // fell 2 rows
            if state.Char().charid == charids_charid_1_shadow as u8 {
                // goto loc_5EFD
                let seq = if state.Char().charid >= charids_charid_2_guard as u8
                    || state.Char().sword == sword_status_sword_2_drawn as u8
                {
                    state.Char().sword = sword_status_sword_2_drawn as u8;
                    seqids_seq_63_guard_active_after_fall as u16
                } else {
                    seqids_seq_17_soft_land as u16
                };
                if state.Char().charid == charids_charid_0_kid as u8 {
                    play_sound(soundids_sound_17_soft_land as c_int);
                    *state.is_guard_notice() = 1;
                }
                seq
            } else if state.Char().charid == charids_charid_2_guard as u8 {
                // a guard dies from a two-storey fall
                take_hp(100);
                play_sound(soundids_sound_0_fell_to_death as c_int);
                seqids_seq_22_crushed as u16
            } else {
                // kid (or skeleton (bug!))
                if take_hp(1) == 0 {
                    // still alive
                    play_sound(soundids_sound_16_medium_land as c_int);
                    *state.is_guard_notice() = 1;
                    seqids_seq_20_medium_land as u16
                } else {
                    // dead (this was the last HP)
                    take_hp(100);
                    play_sound(soundids_sound_0_fell_to_death as c_int);
                    seqids_seq_22_crushed as u16
                }
            }
        } else {
            // fell 3 or more rows
            take_hp(100);
            play_sound(soundids_sound_0_fell_to_death as c_int);
            seqids_seq_22_crushed as u16
        }
    } else {
        // dead
        take_hp(100);
        play_sound(soundids_sound_0_fell_to_death as c_int);
        seqids_seq_22_crushed as u16
    };

    seqtbl_offset_char_impl(state, seq_id as c_short);
    play_seq();
    state.Char().fall_y = 0;
}

/// Lands the current character on the floor he has just reached.
#[no_mangle]
pub unsafe extern "C" fn land() {
    land_impl(&mut State);
}

/// Impales the current character on the spikes he just landed on.
///
/// Marks the spike tile as bloodied (modifier 0xFF), centres the body on the
/// spike tile and takes all remaining hit points. With
/// `fix_offscreen_guards_disappearing`, a character impaled on spikes in a
/// *neighbouring* room has the spike column shifted by a screen width so he is
/// positioned in his own room's coordinates rather than ten columns away.
// seg005:01B7
unsafe fn spiked_impl(state: &mut State) {
    curr_room_modif.add(curr_tilepos as usize).write(0xFF as u8);
    let curr_row = state.Char().curr_row;
    state.Char().y = y_land_at(curr_row as usize + 1) as u8;

    // FIX_OFFSCREEN_GUARDS_DISAPPEARING
    let char_room = state.Char().room;
    let spike_col = if (*fixes).fix_offscreen_guards_disappearing != 0 && curr_room != char_room as i16 {
        if curr_room == state.level().roomlinks[char_room as usize - 1].right as i16 {
            tile_col + 10
        } else if curr_room == state.level().roomlinks[char_room as usize - 1].left as i16 {
            tile_col - 10
        } else {
            tile_col
        }
    } else {
        tile_col
    };

    state.Char().x = x_bump_at((spike_col + FIRST_ONSCREEN_COLUMN as i16) as usize) as u8 + 10;
    state.Char().x = char_dx_forward(8) as u8;
    state.Char().fall_y = 0;
    play_sound(soundids_sound_48_spiked as c_int);
    take_hp(100);
    seqtbl_offset_char_impl(state, seqids_seq_51_spiked as c_short);
    play_seq();
}

/// Kills the current character on the spikes under him.
#[no_mangle]
pub unsafe extern "C" fn spiked() {
    spiked_impl(&mut State);
}

/// Reads this frame's controls and, if they call for it, starts a new move.
///
/// The dispatch is on what the character is doing right now: a dead character
/// only ever slumps, a bumped or falling one has his controls flushed, a
/// character with his sword out goes through the fencing controls, and
/// otherwise the current animation frame selects the right `control_*`
/// handler (standing, turning, starting to run, running, jumping up, hanging,
/// crouching). Frames that are not listed — mid-jump, drinking, sheathing —
/// simply cannot be steered, which is what the trailing fix blocks enforce.
// seg005:0213
unsafe fn control_impl(state: &mut State) {
    let char_frame = state.Char().frame;
    if state.Char().alive >= 0 {
        if char_frame == frameids_frame_15_stand as u8
            || char_frame == frameids_frame_166_stand_inactive as u8
            || char_frame == frameids_frame_158_stand_with_sword as u8
            || char_frame == frameids_frame_171_stand_with_sword as u8
        {
            seqtbl_offset_char_impl(state, seqids_seq_71_dying as c_short);
        }
    } else {
        let char_action = state.Char().action;
        if char_action == actions_actions_5_bumped as u8
            || char_action == actions_actions_4_in_freefall as u8
        {
            release_arrows();
        } else if state.Char().sword == sword_status_sword_2_drawn as u8 {
            control_with_sword_impl(state);
        } else if state.Char().charid >= charids_charid_2_guard as u8 {
            control_guard_inactive();
        } else if char_frame == frameids_frame_15_stand as u8
            || (frameids_frame_50_turn as u8..53).contains(&char_frame)
        {
            control_standing_impl(state);
        } else if char_frame == frameids_frame_48_turn as u8 {
            control_turning_impl(state);
        } else if char_frame < 4 {
            control_startrun_impl(state);
        } else if (frameids_frame_67_start_jump_up_1 as u8..frameids_frame_70_jumphang as u8)
            .contains(&char_frame)
        {
            control_jumpup_impl(state);
        } else if char_frame < 15 {
            control_running_impl(state);
        } else if (frameids_frame_87_hanging_1 as u8..100).contains(&char_frame) {
            control_hanging_impl(state);
        } else if char_frame == frameids_frame_109_crouch as u8 {
            control_crouched_impl(state);
        }

        // ALLOW_CROUCH_AFTER_CLIMBING
        if (*fixes).enable_crouch_after_climbing != 0
            && (seqtbl_offsets_at(seqids_seq_50_crouch as usize)
                ..seqtbl_offsets_at(seqids_seq_49_stand_up_from_crouch as usize))
                .contains(&state.Char().curr_seq)
            && *state.control_forward() != CONTROL_IGNORE as i8
        {
            *state.control_forward() = CONTROL_RELEASED as i8;
        }

        // FIX_MOVE_AFTER_DRINK
        if (*fixes).fix_move_after_drink != 0
            && (frameids_frame_191_drink as u8..=frameids_frame_205_drink as u8)
                .contains(&char_frame)
        {
            release_arrows();
        }

        // FIX_MOVE_AFTER_SHEATHE
        if (*fixes).fix_move_after_sheathe != 0
            && (seqtbl_offsets_at(seqids_seq_92_put_sword_away as usize)
                ..seqtbl_offsets_at(seqids_seq_93_put_sword_away_fast as usize))
                .contains(&state.Char().curr_seq)
        {
            release_arrows();
        }
    }
}

/// Runs one frame of control input for the current character.
#[no_mangle]
pub unsafe extern "C" fn control() {
    control_impl(&mut State);
}

// ── File-scoped statics (for USE_TELEPORTS feature) ──────────────────────────

/// Modifier of the balcony tile the character stepped into. Two balconies with
/// the same nonzero modifier are a teleporter pair.
static mut source_modifier: c_int = 0;
/// Room of the balcony the character stepped into, so [`teleport_impl`] can
/// skip it while looking for the other end of the pair.
static mut source_room: c_int = 0;
/// Tile position within [`source_room`] of the balcony stepped into.
static mut source_tilepos: c_int = 0;

/// Controls while crouching: stand back up, hop forward, or pick up an item.
///
/// This is also where the level-1 introduction music is triggered — crouching
/// at the start of the level is the cue — which is why the whole function is
/// bypassed while that music is still pending.
// seg005:02EB
unsafe fn control_crouched_impl(state: &mut State) {
    if *state.need_level1_music() != 0 && *state.current_level() == (*custom).intro_music_level as u16 {
        // Special event: music when crouching
        if check_sound_playing() == 0 {
            if *state.need_level1_music() == 1 {
                play_sound(soundids_sound_25_presentation as c_int);
                *state.need_level1_music() = 2;
            } else {
                // USE_REPLAY
                if recording != 0 {
                    special_move = replay_special_moves_MOVE_EFFECT_END as u8;
                }
                if replaying == 0 {
                    *state.need_level1_music() = 0;
                }
            }
        }
    } else {
        *state.need_level1_music() = 0;
        if *state.control_shift2() == CONTROL_HELD as i8 && check_get_item_impl(state) != 0 {
            return;
        }
        if *state.control_y() != CONTROL_HELD_DOWN as i8 {
            seqtbl_offset_char_impl(state, seqids_seq_49_stand_up_from_crouch as c_short);
        } else if *state.control_forward() == CONTROL_HELD as i8 {
            *state.control_forward() = CONTROL_IGNORE as i8;
            seqtbl_offset_char_impl(state, seqids_seq_79_crouch_hop as c_short);
        }
    }
}

/// Runs one frame of control input for a crouching character.
#[no_mangle]
pub unsafe extern "C" fn control_crouched() {
    control_crouched_impl(&mut State);
}

/// Controls while standing still — the widest set of moves in the game.
///
/// First the special cases: picking an item up off the floor, a guard drawing
/// his sword, and the Kid drawing his automatically when a guard has spotted
/// him and closed to within about a tile and a half. `offguard` (the Kid
/// deliberately sheathed in front of a guard) suppresses the automatic draw
/// until the guard closes right in. Then the ordinary directions: shift is the
/// "careful" modifier, turning a step into a measured [`safe_step`], and in
/// keyboard mode forward+up together mean a standing jump.
// seg005:0358
unsafe fn control_standing_impl(state: &mut State) {
    if *state.control_shift2() == CONTROL_HELD as i8
        && *state.control_shift() == CONTROL_HELD as i8
        && check_get_item_impl(state) != 0
    {
        return;
    }
    if state.Char().charid != charids_charid_0_kid as u8
        && *state.control_down() == CONTROL_HELD as i8
        && *state.control_forward() == CONTROL_HELD as i8
    {
        draw_sword_impl(state);
        return;
    }

    if *state.have_sword() != 0 {
        if *state.offguard() != 0 && *state.control_shift() >= CONTROL_RELEASED as i8 {
            // goto loc_6213
        } else if *state.can_guard_see_kid() >= 2 {
            let distance = char_opp_dist();
            if distance >= -10 && distance < 90 {
                *state.holding_sword() = 1;
                // C compares as `word`, so a distance of -6..=-1 wraps to a
                // huge value and takes the `else` (turn to face) branch. Kept
                // as an explicit u16 comparison — the wrap is the condition.
                if (distance as u16) < ((-6i32) as u16) {
                    if state.Opp().charid == charids_charid_1_shadow as u8
                        && (state.Opp().action == actions_actions_3_in_midair as u8
                            || (state.Opp().frame >= frameids_frame_107_fall_land_1 as u8 && state.Opp().frame < 118))
                    {
                        *state.offguard() = 0;
                    } else {
                        draw_sword_impl(state);
                        return;
                    }
                } else {
                    back_pressed_impl(state);
                    return;
                }
            }
        } else {
            *state.offguard() = 0;
        }
    }

    // loc_6213:
    if *state.control_shift() == CONTROL_HELD as i8 {
        if *state.control_backward() == CONTROL_HELD as i8 {
            back_pressed_impl(state);
        } else if *state.control_up() == CONTROL_HELD as i8 {
            up_pressed_impl(state);
        } else if *state.control_down() == CONTROL_HELD as i8 {
            down_pressed_impl(state);
        } else if *state.control_x() == CONTROL_HELD_FORWARD as i8 && *state.control_forward() == CONTROL_HELD as i8 {
            safe_step_impl(state);
        }
    } else if *state.control_forward() == CONTROL_HELD as i8 {
        if is_keyboard_mode != 0 && *state.control_up() == CONTROL_HELD as i8 {
            standing_jump_impl(state);
        } else {
            forward_pressed_impl(state);
        }
    } else if *state.control_backward() == CONTROL_HELD as i8 {
        back_pressed_impl(state);
    } else if *state.control_up() == CONTROL_HELD as i8 {
        if is_keyboard_mode != 0 && *state.control_forward() == CONTROL_HELD as i8 {
            standing_jump_impl(state);
        } else {
            up_pressed_impl(state);
        }
    } else if *state.control_down() == CONTROL_HELD as i8 {
        down_pressed_impl(state);
    } else if *state.control_x() == CONTROL_HELD_FORWARD as i8 {
        forward_pressed_impl(state);
    }
}

/// Runs one frame of control input for a standing character.
#[no_mangle]
pub unsafe extern "C" fn control_standing() {
    control_standing_impl(&mut State);
}

/// Handles "up" from a standing character: leave the level, teleport, or jump.
///
/// The three tiles a standing character can be said to occupy — the one he is
/// on, the one behind him and the one in front — are tested in turn for an
/// exit door and then for a balcony. Each `get_tile_*` call leaves its result
/// in `curr_tilepos`, so the position of whichever tile matched is read back
/// from there afterwards. Failing both, "up" just means jump.
// seg005:0482
unsafe fn up_pressed_impl(state: &mut State) {
    // If there is an open level door nearby, enter it.
    // The `||` chain reproduces the original if/else-if exactly: the tile
    // lookups stop at the first match, and curr_tilepos is that tile's.
    let leveldoor_tilepos: c_int = if get_tile_at_char() == tiles_tiles_16_level_door_left as c_int
        || get_tile_behind_char() == tiles_tiles_16_level_door_left as c_int
        || get_tile_infrontof_char() == tiles_tiles_16_level_door_left as c_int
    {
        curr_tilepos as c_int
    } else {
        -1
    };
    if leveldoor_tilepos != -1
        && (state.level().start_room as u16) != *state.drawn_room()
        && (if (*fixes).fix_exit_door != 0 {
            curr_room_modif.add(leveldoor_tilepos as usize).read() >= 42
        } else {
            *state.leveldoor_open() != 0
        })
    {
        go_up_leveldoor_impl(state);
        return;
    }

    // USE_TELEPORTS
    // This detection is not perfect...
    let balcony_tilepos: c_int = if get_tile_at_char() == tiles_tiles_23_balcony_left as c_int
        || get_tile_behind_char() == tiles_tiles_23_balcony_left as c_int
        || get_tile_infrontof_char() == tiles_tiles_23_balcony_left as c_int
    {
        curr_tilepos as c_int
    } else {
        -1
    };
    if balcony_tilepos != -1 {
        // We reuse pickup_obj_type for storing the identifier of the teleporter.
        *state.pickup_obj_type() = curr_room_modif.add(curr_tilepos as usize).read() as i16;
        // Balconies with zero modifiers remain regular balconies.
        if *state.pickup_obj_type() > 0 {
            source_modifier = *state.pickup_obj_type() as c_int;
            source_room = curr_room as c_int;
            source_tilepos = curr_tilepos as c_int;
            go_up_leveldoor_impl(state);
            seqtbl_offset_char_impl(state, seqids_seq_teleport as c_short);
            return;
        }
    }

    // Else just jump up.
    if *state.control_x() == CONTROL_HELD_FORWARD as i8 {
        standing_jump_impl(state);
    } else {
        check_jump_up_impl(state);
    }
}

/// Acts on "up" for the current standing character.
#[no_mangle]
pub unsafe extern "C" fn up_pressed() {
    up_pressed_impl(&mut State);
}

/// Handles "down" from a standing character: step back from a brink, climb
/// down over a ledge behind him, or simply crouch.
///
/// Climbing down needs floor missing behind him and a grabbable edge there;
/// a closed gate is not grabbable from the left, hence the gate-openness test.
// seg005:04C7
unsafe fn down_pressed_impl(state: &mut State) {
    *state.control_down() = CONTROL_IGNORE as i8;
    if tile_is_floor(get_tile_infrontof_char()) == 0 && distance_to_edge_weight() < 3 {
        state.Char().x = char_dx_forward(5) as u8;
        load_fram_det_col();
    } else if tile_is_floor(get_tile_behind_char()) == 0 && distance_to_edge_weight() >= 8 {
        *state.through_tile() = get_tile_behind_char() as u8;
        get_tile_at_char();
        if can_grab() != 0
            && (!((*fixes).enable_crouch_after_climbing != 0 && *state.control_forward() == CONTROL_HELD as i8))
            && (state.Char().direction >= directions_dir_0_right as i8
                || get_tile_at_char() != tiles_tiles_4_gate as c_int
                || (curr_room_modif.add(curr_tilepos as usize).read() as i32) >> 2 >= 6)
        {
            state.Char().x = char_dx_forward(distance_to_edge_weight() - 9) as u8;
            seqtbl_offset_char_impl(state, seqids_seq_68_climb_down as c_short);
        } else {
            crouch_impl(state);
        }
    } else {
        crouch_impl(state);
    }
}

/// Acts on "down" for the current standing character.
#[no_mangle]
pub unsafe extern "C" fn down_pressed() {
    down_pressed_impl(&mut State);
}

/// Lines the character up with the level door (or teleporter balcony) and
/// starts the walk-in animation. Always entered facing left, because that is
/// the only direction the animation exists in.
// seg005:0574
unsafe fn go_up_leveldoor_impl(state: &mut State) {
    state.Char().x = x_bump_at((tile_col + FIRST_ONSCREEN_COLUMN as i16) as usize) as u8 + 10;
    state.Char().direction = directions_dir_FF_left as i8;
    seqtbl_offset_char_impl(state, seqids_seq_70_go_up_on_level_door as c_short);
}

/// Sends the current character up through the level door in front of him.
#[no_mangle]
pub unsafe extern "C" fn go_up_leveldoor() {
    go_up_leveldoor_impl(&mut State);
}

/// Controls during the turn-around animation: holding the stick forward as the
/// turn finishes rolls straight into a run, unless a wall is close enough that
/// running would just bump into it (`fix_turn_running_near_wall`).
///
/// The joystick block afterwards de-latches directions the stick has already
/// returned from, so a flick does not queue up a second move.
// seg005:058F
unsafe fn control_turning_impl(state: &mut State) {
    if *state.control_shift() >= CONTROL_RELEASED as i8
        && *state.control_x() == CONTROL_HELD_FORWARD as i8
        && *state.control_y() >= CONTROL_RELEASED as i8
    {
        // FIX_TURN_RUN_NEAR_WALL
        if (*fixes).fix_turn_running_near_wall != 0 {
            let distance = get_edge_distance();
            if edge_type == EDGE_TYPE_WALL as u8 && curr_tile2 != tiles_tiles_18_chomper as u8 && distance < 8 {
                *state.control_forward() = CONTROL_HELD as i8;
            } else {
                seqtbl_offset_char_impl(state, seqids_seq_43_start_run_after_turn as c_short);
            }
        } else {
            seqtbl_offset_char_impl(state, seqids_seq_43_start_run_after_turn as c_short);
        }
    }

    // Added: joystick mode handling
    if is_joyst_mode != 0 {
        if *state.control_up() == CONTROL_HELD as i8 && *state.control_y() >= CONTROL_RELEASED as i8 {
            *state.control_up() = CONTROL_RELEASED as i8;
        }
        if *state.control_down() == CONTROL_HELD as i8 && *state.control_y() <= CONTROL_RELEASED as i8 {
            *state.control_down() = CONTROL_RELEASED as i8;
        }
        if *state.control_backward() == CONTROL_HELD as i8 && *state.control_x() == CONTROL_RELEASED as i8 {
            *state.control_backward() = CONTROL_RELEASED as i8;
        }
    }
}

/// Runs one frame of control input for a character mid-turn.
#[no_mangle]
pub unsafe extern "C" fn control_turning() {
    control_turning_impl(&mut State);
}

/// Drops the character into a crouch and consumes the "down" press.
// seg005:05AD
unsafe fn crouch_impl(state: &mut State) {
    seqtbl_offset_char_impl(state, seqids_seq_50_crouch as c_short);
    *state.control_down() = release_arrows() as i8;
}

/// Makes the current character crouch.
#[no_mangle]
pub unsafe extern "C" fn crouch() {
    crouch_impl(&mut State);
}

/// Handles "back": turn around, drawing the sword during the turn if the Kid
/// is turning to face a guard who has already spotted him and is behind him
/// with room enough to fence.
// seg005:05BE
unsafe fn back_pressed_impl(state: &mut State) {
    *state.control_backward() = release_arrows() as i8;
    // After turn, Kid will draw sword if ...
    let seq_id: u16 = if *state.have_sword() == 0
        || *state.can_guard_see_kid() < 2
        || char_opp_dist() > 0
        || distance_to_edge_weight() < 2
    {
        seqids_seq_5_turn as u16
    } else {
        state.Char().sword = sword_status_sword_2_drawn as u8;
        *state.offguard() = 0;
        seqids_seq_89_turn_draw_sword as u16
    };
    seqtbl_offset_char_impl(state, seq_id as c_short);
}

/// Acts on "back" for the current standing character.
#[no_mangle]
pub unsafe extern "C" fn back_pressed() {
    back_pressed_impl(&mut State);
}

/// Handles "forward": break into a run, unless there is a wall within eight
/// pixels, in which case take a single measured step instead. An open chomper
/// reads as a wall to the edge scan but must not stop the run, hence the
/// explicit exception.
// seg005:060F
unsafe fn forward_pressed_impl(state: &mut State) {
    let distance = get_edge_distance();

    // ALLOW_CROUCH_AFTER_CLIMBING
    if (*fixes).enable_crouch_after_climbing != 0 && *state.control_down() == CONTROL_HELD as i8 {
        down_pressed_impl(state);
        *state.control_forward() = CONTROL_RELEASED as i8;
        return;
    }

    if edge_type == EDGE_TYPE_WALL as u8
        && curr_tile2 != tiles_tiles_18_chomper as u8
        && distance < 8
    {
        // If char is near a wall, step instead of run.
        if *state.control_forward() == CONTROL_HELD as i8 {
            safe_step_impl(state);
        }
    } else {
        seqtbl_offset_char_impl(state, seqids_seq_1_start_run as c_short);
    }
}

/// Acts on "forward" for the current standing character.
#[no_mangle]
pub unsafe extern "C" fn forward_pressed() {
    forward_pressed_impl(&mut State);
}

/// Controls while running: stop, turn (the running turn), running jump, or
/// tuck into a roll. Stopping is only allowed on the two frames of the run
/// cycle where the feet are in the right place for the skid animation.
// seg005:0649
unsafe fn control_running_impl(state: &mut State) {
    if *state.control_x() == CONTROL_RELEASED as i8
        && (state.Char().frame == frameids_frame_7_run as u8 || state.Char().frame == frameids_frame_11_run as u8)
    {
        *state.control_forward() = release_arrows() as i8;
        seqtbl_offset_char_impl(state, seqids_seq_13_stop_run as c_short);
    } else if *state.control_x() == CONTROL_HELD_BACKWARD as i8 {
        *state.control_backward() = release_arrows() as i8;
        seqtbl_offset_char_impl(state, seqids_seq_6_run_turn as c_short);
    } else if *state.control_y() == CONTROL_HELD_UP as i8 && *state.control_up() == CONTROL_HELD as i8 {
        run_jump_impl(state);
    } else if *state.control_down() == CONTROL_HELD as i8 {
        *state.control_down() = CONTROL_IGNORE as i8;
        seqtbl_offset_char_impl(state, seqids_seq_26_crouch_while_running as c_short);
    }
}

/// Runs one frame of control input for a running character.
#[no_mangle]
pub unsafe extern "C" fn control_running() {
    control_running_impl(&mut State);
}

/// Takes one careful step, sized to land exactly on the edge ahead.
///
/// There is a separate animation sequence for every step length from 1 to 11
/// pixels, laid out consecutively in the sequence table starting at index 29 —
/// which is what the `distance + 28` arithmetic selects. A step of length zero
/// means the character is already at the edge: he either steps *onto* the very
/// brink (once, guarded by `Char.repeat` so it cannot be spammed) or takes the
/// full-length step.
// seg005:06A8
unsafe fn safe_step_impl(state: &mut State) {
    *state.control_shift2() = CONTROL_IGNORE as i8;
    *state.control_forward() = CONTROL_IGNORE as i8;
    let distance = get_edge_distance();
    if distance != 0 {
        state.Char().repeat = 1;
        seqtbl_offset_char_impl(state, (distance + 28) as c_short);
    } else if edge_type != EDGE_TYPE_WALL as u8 && state.Char().repeat != 0 {
        state.Char().repeat = 0;
        seqtbl_offset_char_impl(state, seqids_seq_44_step_on_edge as c_short);
    } else {
        seqtbl_offset_char_impl(state, seqids_seq_39_safe_step_11 as c_short);
    }
}

/// Makes the current character take one careful step forward.
#[no_mangle]
pub unsafe extern "C" fn safe_step() {
    safe_step_impl(&mut State);
}

/// Nonzero if there is a potion or a sword within reach, in which case the
/// pickup has been started.
///
/// An item on the character's *own* tile is picked up by first shuffling back
/// a tile so it ends up in front of him — but only if there is floor behind to
/// shuffle onto.
// seg005:06F0
unsafe fn check_get_item_impl(state: &mut State) -> c_int {
    if get_tile_at_char() == tiles_tiles_10_potion as c_int || curr_tile2 == tiles_tiles_22_sword as u8 {
        if tile_is_floor(get_tile_behind_char()) == 0 {
            return 0;
        }
        state.Char().x = char_dx_forward(-14) as u8;
        load_fram_det_col();
    }
    if get_tile_infrontof_char() == tiles_tiles_10_potion as c_int
        || curr_tile2 == tiles_tiles_22_sword as u8
    {
        get_item_impl(state);
        return 1;
    }
    0
}

/// Nonzero if the current character has started picking an item up.
#[no_mangle]
pub unsafe extern "C" fn check_get_item() -> c_int {
    check_get_item_impl(&mut State)
}

/// Picks up the item in front of the character.
///
/// If he is not crouching yet, this only lines him up with the item and
/// crouches; the next frame comes back here and actually takes it. Swords are
/// picked up outright; a potion's effect is the top five bits of its tile
/// modifier, passed to [`do_pickup`]. On the copy-protection level, drinking
/// one of the marked potions removes it from the table so it is not offered
/// again.
// seg005:073E
unsafe fn get_item_impl(state: &mut State) {
    if state.Char().frame != frameids_frame_109_crouch as u8 {
        // crouching
        let distance = get_edge_distance();
        if edge_type != EDGE_TYPE_FLOOR as u8 {
            state.Char().x = char_dx_forward(distance) as u8;
        }
        if state.Char().direction >= directions_dir_0_right as i8 {
            state.Char().x =
                char_dx_forward(if curr_tile2 == tiles_tiles_10_potion as u8 { 1 } else { 0 } - 2)
                    as u8;
        }
        crouch_impl(state);
    } else if curr_tile2 == tiles_tiles_22_sword as u8 {
        do_pickup(-1);
        seqtbl_offset_char_impl(state, seqids_seq_91_get_sword as c_short);
    } else {
        // potion
        do_pickup((curr_room_modif.add(curr_tilepos as usize).read() as i32) >> 3);
        seqtbl_offset_char_impl(state, seqids_seq_78_drink as c_short);
        // USE_COPYPROT
        if *state.current_level() == 15 {
            for index in 0..14 {
                if copyprot_room_at(index) as i16 == curr_room
                    && copyprot_tile_at(index) == curr_tilepos as u16
                {
                    set_copyprot_room(index, 0);
                    break;
                }
            }
        }
    }
}

/// Picks up whatever is in front of the current character.
#[no_mangle]
pub unsafe extern "C" fn get_item() {
    get_item_impl(&mut State);
}

/// Controls during the first few frames of a run: pushing the stick
/// diagonally forward-and-up here converts the start of the run into a
/// standing jump.
// seg005:07FF
unsafe fn control_startrun_impl(state: &mut State) {
    if *state.control_y() == CONTROL_HELD_UP as i8 && *state.control_x() == CONTROL_HELD_FORWARD as i8 {
        standing_jump_impl(state);
    }
}

/// Runs one frame of control input for a character starting to run.
#[no_mangle]
pub unsafe extern "C" fn control_startrun() {
    control_startrun_impl(&mut State);
}

/// Controls during a straight-up jump: adding "forward" mid-jump converts it
/// into a standing jump.
// seg005:0812
unsafe fn control_jumpup_impl(state: &mut State) {
    if *state.control_x() == CONTROL_HELD_FORWARD as i8 || *state.control_forward() == CONTROL_HELD as i8 {
        standing_jump_impl(state);
    }
}

/// Runs one frame of control input for a character jumping straight up.
#[no_mangle]
pub unsafe extern "C" fn control_jumpup() {
    control_jumpup_impl(&mut State);
}

/// Starts a standing (forward) jump, consuming both the up and forward
/// presses so the jump cannot be re-triggered while it plays.
// seg005:0825
unsafe fn standing_jump_impl(state: &mut State) {
    *state.control_up() = CONTROL_IGNORE as i8;
    *state.control_forward() = CONTROL_IGNORE as i8;
    seqtbl_offset_char_impl(state, seqids_seq_3_standing_jump as c_short);
}

/// Starts a standing jump for the current character.
#[no_mangle]
pub unsafe extern "C" fn standing_jump() {
    standing_jump_impl(&mut State);
}

/// Decides what jumping up here actually means: grabbing the ledge in front,
/// grabbing the one overhead, or just a hop.
///
/// `through_tile` is set before each test because the grab check has to know
/// which tile the character would be passing through on the way up.
// seg005:0836
unsafe fn check_jump_up_impl(state: &mut State) {
    *state.control_up() = release_arrows() as i8;
    *state.through_tile() = get_tile_above_char() as u8;
    get_tile_front_above_char();
    if can_grab() != 0 {
        grab_up_with_floor_behind_impl(state);
    } else {
        *state.through_tile() = get_tile_behind_above_char() as u8;
        get_tile_above_char();
        if can_grab() != 0 {
            jump_up_or_grab_impl(state);
        } else {
            jump_up_impl(state);
        }
    }
}

/// Starts whichever kind of upward jump fits the current surroundings.
#[no_mangle]
pub unsafe extern "C" fn check_jump_up() {
    check_jump_up_impl(&mut State);
}

/// Chooses between a plain hop and a grab at the ledge overhead, depending on
/// how far into the tile the character is standing and whether there is floor
/// behind him to back onto first.
// seg005:087B
unsafe fn jump_up_or_grab_impl(state: &mut State) {
    let distance = distance_to_edge_weight();
    if distance < 6 {
        jump_up_impl(state);
    } else if tile_is_floor(get_tile_behind_char()) == 0 {
        // There is not floor behind char.
        grab_up_no_floor_behind_impl(state);
    } else {
        // There is floor behind char, go back a bit.
        state.Char().x = char_dx_forward(distance - TILE_SIZEX as i32) as u8;
        load_fram_det_col();
        grab_up_with_floor_behind_impl(state);
    }
}

/// Jumps up, grabbing the ledge overhead if the character is placed for it.
#[no_mangle]
pub unsafe extern "C" fn jump_up_or_grab() {
    jump_up_or_grab_impl(&mut State);
}

/// Jumps up and grabs the ledge while standing on the brink — nudge to a fixed
/// offset from the edge first so the grab animation lines up.
// seg005:08C7
unsafe fn grab_up_no_floor_behind_impl(state: &mut State) {
    get_tile_above_char();
    state.Char().x = char_dx_forward(distance_to_edge_weight() - 10) as u8;
    seqtbl_offset_char_impl(state, seqids_seq_16_jump_up_and_grab as c_short);
}

/// Jumps up and grabs the ledge from the very edge of the floor.
#[no_mangle]
pub unsafe extern "C" fn grab_up_no_floor_behind() {
    grab_up_no_floor_behind_impl(&mut State);
}

/// Jumps straight up with nothing to grab.
///
/// Which of the three jump sequences plays depends on what is over the
/// character's head: a ceiling (bonk), open air (a plain hop), or — in feather
/// mode with the super-high-jump enhancement — open air he can actually reach,
/// in which case the jump is aimed at a specific tile two rows up and
/// `super_jump_timer` counts the ascent out. The timer is one frame longer
/// when the tile up there is *not* solid, so he clears it rather than
/// stopping under it.
// seg005:08E6
unsafe fn jump_up_impl(state: &mut State) {
    *state.control_up() = release_arrows() as i8;
    let distance = get_edge_distance();
    if distance < 4 && edge_type == EDGE_TYPE_WALL as u8 {
        state.Char().x = char_dx_forward(distance - 3) as u8;
    }
    // FIX_JUMP_DISTANCE_AT_EDGE
    if (*fixes).fix_jump_distance_at_edge != 0 && distance == 3 && edge_type == EDGE_TYPE_CLOSER as u8 {
        state.Char().x = char_dx_forward(-1) as u8;
    }

    // USE_SUPER_HIGH_JUMP
    let delta_x: u16 = if *state.is_feather_fall() != 0
        && tile_is_floor(get_tile_above_char()) == 0
        && curr_tile2 != tiles_tiles_20_wall as u8
    {
        if state.Char().direction == directions_dir_FF_left as i8 { 1 } else { 3 }
    } else {
        0
    };
    let char_col = get_tile_div_mod(back_delta_x(delta_x as c_int) + dx_weight() as i32 - 6);
    let char_room = state.Char().room;
    let curr_row = state.Char().curr_row;
    get_tile(char_room as c_int, char_col, curr_row as c_int - 1);
    if curr_tile2 != tiles_tiles_20_wall as u8 && tile_is_floor(curr_tile2 as c_int) == 0 {
        if (*fixes).enable_super_high_jump != 0 && *state.is_feather_fall() != 0 {
            // super high jump can only happen in feather mode
            if curr_room == 0 && curr_row == 0 {
                // there is no room above
                seqtbl_offset_char_impl(state, seqids_seq_14_jump_up_into_ceiling as c_short);
            } else {
                get_tile(char_room as c_int, char_col, curr_row as c_int - 2); // the target top tile
                let mut is_top_floor =
                    tile_is_floor(curr_tile2 as c_int) != 0 || curr_tile2 == tiles_tiles_20_wall as u8;
                // A loose floor that has already been shaken out (bit 5 clear)
                // will not be there to stand on.
                if is_top_floor
                    && curr_tile2 == tiles_tiles_11_loose as u8
                    && (curr_room_tiles.add(curr_tilepos as usize).read() & 0x20) == 0
                {
                    is_top_floor = false;
                }
                // kid should jump slightly higher if the top tile is not a floor
                *state.super_jump_timer() = if is_top_floor { 22 } else { 24 };
                *state.super_jump_room() = curr_room as u8;
                *state.super_jump_col() = tile_col as i8;
                *state.super_jump_row() = tile_row as i8;
                seqtbl_offset_char_impl(state, seqids_seq_48_super_high_jump as c_short);
            }
        } else {
            seqtbl_offset_char_impl(state, seqids_seq_28_jump_up_with_nothing_above as c_short);
        }
    } else {
        seqtbl_offset_char_impl(state, seqids_seq_14_jump_up_into_ceiling as c_short);
    }
}

/// Makes the current character jump straight up.
#[no_mangle]
pub unsafe extern "C" fn jump_up() {
    jump_up_impl(&mut State);
}

/// Controls while hanging from a ledge: pull up, hang on, or let go.
///
/// Pulling up is only offered once `grab_timer` has run out — the brief pause
/// after catching a ledge. Holding shift keeps hold; if the tile the character
/// is hanging against is a wall or a tapestry rather than an open face, he
/// swings flat against it instead. Anything else drops him.
// seg005:0968
unsafe fn control_hanging_impl(state: &mut State) {
    if state.Char().alive < 0 {
        if *state.grab_timer() == 0 && *state.control_y() == CONTROL_HELD as i8 {
            can_climb_up_impl(state);
        } else if *state.control_shift() == CONTROL_HELD as i8
            || ((*fixes).enable_super_high_jump != 0 && *state.super_jump_fall() != 0 && *state.control_y() == CONTROL_HELD as i8)
        {
            // hanging against a wall or a doortop
            if state.Char().action != actions_actions_6_hang_straight as u8
                && (get_tile_at_char() == tiles_tiles_20_wall as c_int
                    || (state.Char().direction == directions_dir_FF_left as i8
                        && ((curr_tile2 == tiles_tiles_7_doortop_with_floor as u8)
                            || (curr_tile2 == tiles_tiles_12_doortop as u8))))
            {
                if *state.grab_timer() == 0 {
                    play_sound(soundids_sound_8_bumped as c_int);
                }
                seqtbl_offset_char_impl(state, seqids_seq_25_hang_against_wall as c_short);
            } else if tile_is_floor(get_tile_above_char()) == 0 {
                hang_fall_impl(state);
            }
        } else {
            hang_fall_impl(state);
        }
    } else {
        hang_fall_impl(state);
    }
}

/// Runs one frame of control input for a character hanging from a ledge.
#[no_mangle]
pub unsafe extern "C" fn control_hanging() {
    control_hanging_impl(&mut State);
}

/// Climbs up from a hang, ducking under whatever is above the ledge.
///
/// The normal climb stands the character upright, which does not fit if the
/// tile up there is a mirror, a chomper, or a gate that is still mostly shut —
/// those get the shorter "climb up to closed gate" variant instead.
// seg005:09DF
unsafe fn can_climb_up_impl(state: &mut State) {
    let mut seq_id = seqids_seq_10_climb_up as u16;
    // C: control_up = control_shift2 = release_arrows();  — a single chained
    // assignment (release_arrows() called ONCE). Splitting into two calls would
    // let the second release_arrows() side-effect reset control_up back to 0.
    *state.control_shift2() = release_arrows() as i8;
    *state.control_up() = *state.control_shift2();
    // USE_SUPER_HIGH_JUMP
    if (*fixes).enable_super_high_jump != 0 {
        *state.super_jump_fall() = 0;
    }

    get_tile_above_char();
    if ((curr_tile2 == tiles_tiles_13_mirror as u8 || curr_tile2 == tiles_tiles_18_chomper as u8)
        && state.Char().direction == directions_dir_0_right as i8)
        || (curr_tile2 == tiles_tiles_4_gate as u8
            && state.Char().direction != directions_dir_0_right as i8
            && (curr_room_modif.add(curr_tilepos as usize).read() as i32) >> 2 < 6)
    {
        seq_id = seqids_seq_73_climb_up_to_closed_gate as u16;
    }
    seqtbl_offset_char_impl(state, seq_id as c_short);
}

/// Makes the current hanging character climb up onto the ledge.
#[no_mangle]
pub unsafe extern "C" fn can_climb_up() {
    can_climb_up_impl(&mut State);
}

/// Lets go of the ledge. If there is floor immediately below he lands on it;
/// otherwise he drops into a fall. Letting go while pressed against a wall or
/// tapestry backs him off it first so he does not land inside it.
// seg005:0A46
unsafe fn hang_fall_impl(state: &mut State) {
    *state.control_down() = release_arrows() as i8;
    // USE_SUPER_HIGH_JUMP
    if (*fixes).enable_super_high_jump != 0 {
        *state.super_jump_fall() = 0;
    }

    if tile_is_floor(get_tile_behind_char()) == 0 && tile_is_floor(get_tile_at_char()) == 0 {
        seqtbl_offset_char_impl(state, seqids_seq_23_release_ledge_and_fall as c_short);
    } else {
        if get_tile_at_char() == tiles_tiles_20_wall as c_int
            || (state.Char().direction < directions_dir_0_right as i8
                && ((curr_tile2 == tiles_tiles_7_doortop_with_floor as u8)
                    || (curr_tile2 == tiles_tiles_12_doortop as u8)))
        {
            state.Char().x = char_dx_forward(-7) as u8;
        }
        seqtbl_offset_char_impl(state, seqids_seq_11_release_ledge_and_land as c_short);
    }
}

/// Makes the current hanging character let go.
#[no_mangle]
pub unsafe extern "C" fn hang_fall() {
    hang_fall_impl(&mut State);
}

/// Jumps up to grab the ledge from solid ground, choosing between the
/// straight-up grab and the forward one according to how far from the edge the
/// character is standing.
// seg005:0AA8
unsafe fn grab_up_with_floor_behind_impl(state: &mut State) {
    let distance = distance_to_edge_weight();

    // The global variable edge_type (which we need!) gets set as a side effect of get_edge_distance()
    let edge_distance = get_edge_distance();

    // FIX_EDGE_DISTANCE_CHECK_WHEN_CLIMBING
    let jump_straight_condition = if (*fixes).fix_edge_distance_check_when_climbing != 0 {
        distance < 4 && edge_type != EDGE_TYPE_WALL as u8
    } else {
        distance < 4 && edge_distance < 4 && edge_type != EDGE_TYPE_WALL as u8
    };

    if jump_straight_condition {
        state.Char().x = char_dx_forward(distance) as u8;
        seqtbl_offset_char_impl(state, seqids_seq_8_jump_up_and_grab_straight as c_short);
    } else {
        state.Char().x = char_dx_forward(distance - 4) as u8;
        seqtbl_offset_char_impl(state, seqids_seq_24_jump_up_and_grab_forward as c_short);
    }
}

/// Makes the current character jump up and grab the ledge ahead.
#[no_mangle]
pub unsafe extern "C" fn grab_up_with_floor_behind() {
    grab_up_with_floor_behind_impl(&mut State);
}

/// Starts a running jump, first shuffling the character so he takes off from
/// the right spot.
///
/// It looks one and then two tiles ahead for the gap (or the spikes) being
/// jumped, and adjusts his x so the take-off is a consistent distance from
/// that edge. If the required adjustment is too large to be a take-off tweak
/// the jump is abandoned and the run continues.
// seg005:0AF7
unsafe fn run_jump_impl(state: &mut State) {
    if state.Char().frame >= frameids_frame_7_run as u8 {
        // Align Kid to edge of floor.
        let xpos = char_dx_forward(4);
        let mut col = get_tile_div_mod_m7(xpos);
        let char_direction = state.Char().direction;
        for tiles_forward in 0..2 {
            col += dir_front_at((char_direction as i32 + 1) as usize) as i32;
            let char_room = state.Char().room;
            let curr_row = state.Char().curr_row;
            get_tile(char_room as c_int, col, curr_row as c_int);
            if curr_tile2 == tiles_tiles_2_spike as u8 || tile_is_floor(curr_tile2 as c_int) == 0 {
                let mut pos_adjustment =
                    distance_to_edge(xpos) + (TILE_SIZEX as i32) * tiles_forward - (TILE_SIZEX as i32);
                // Unsigned compare, as in C: adjustments in -8..=-1 wrap to
                // huge values and so fall through to the checks below.
                if (pos_adjustment as u32) < ((-8i32) as u32) || pos_adjustment >= 2 {
                    if pos_adjustment < 128 {
                        return;
                    }
                    pos_adjustment = -3;
                }
                state.Char().x = char_dx_forward(pos_adjustment + 4) as u8;
                break;
            }
        }
        *state.control_up() = release_arrows() as i8;
        seqtbl_offset_char_impl(state, seqids_seq_4_run_jump as c_short);
    }
}

/// Makes the current running character jump.
#[no_mangle]
pub unsafe extern "C" fn run_jump() {
    run_jump_impl(&mut State);
}

/// Retreats one pace while fencing. Only possible from the on-guard stance —
/// mid-attack or mid-parry frames ignore it.
// seg005:0BB5
unsafe fn back_with_sword_impl(state: &mut State) {
    let frame = state.Char().frame;
    if frame == frameids_frame_158_stand_with_sword as u8
        || frame == frameids_frame_170_stand_with_sword as u8
        || frame == frameids_frame_171_stand_with_sword as u8
    {
        *state.control_backward() = CONTROL_IGNORE as i8;
        seqtbl_offset_char_impl(state, seqids_seq_57_back_with_sword as c_short);
    }
}

/// Makes the current fencing character step back.
#[no_mangle]
pub unsafe extern "C" fn back_with_sword() {
    back_with_sword_impl(&mut State);
}

/// Advances one pace while fencing. Guards use a different (longer) advance
/// than the Kid.
// seg005:0BE3
unsafe fn forward_with_sword_impl(state: &mut State) {
    let frame = state.Char().frame;
    if frame == frameids_frame_158_stand_with_sword as u8
        || frame == frameids_frame_170_stand_with_sword as u8
        || frame == frameids_frame_171_stand_with_sword as u8
    {
        *state.control_forward() = CONTROL_IGNORE as i8;
        if state.Char().charid != charids_charid_0_kid as u8 {
            seqtbl_offset_char_impl(state, seqids_seq_56_guard_forward_with_sword as c_short);
        } else {
            seqtbl_offset_char_impl(state, seqids_seq_86_forward_with_sword as c_short);
        }
    }
}

/// Makes the current fencing character step forward.
#[no_mangle]
pub unsafe extern "C" fn forward_with_sword() {
    forward_with_sword_impl(&mut State);
}

/// Draws the sword and goes on guard.
///
/// The Kid and the shadow get the drawing flourish; a guard just snaps to
/// en garde, since he had his sword out all along.
// seg005:0C1D
unsafe fn draw_sword_impl(state: &mut State) {
    let mut seq_id = seqids_seq_55_draw_sword as u16;
    // C: control_forward = control_shift2 = release_arrows();  — single chained
    // assignment (release_arrows() called ONCE). See can_climb_up for the trap.
    *state.control_shift2() = release_arrows() as i8;
    *state.control_forward() = *state.control_shift2();
    // FIX_UNINTENDED_SWORD_STRIKE
    if (*fixes).fix_unintended_sword_strike != 0 {
        *state.ctrl1_shift2() = CONTROL_IGNORE as i8;
    }

    if state.Char().charid == charids_charid_0_kid as u8 {
        play_sound(soundids_sound_19_draw_sword as c_int);
        *state.offguard() = 0;
    } else if state.Char().charid != charids_charid_1_shadow as u8 {
        seq_id = seqids_seq_90_en_garde as u16;
    }
    state.Char().sword = sword_status_sword_2_drawn as u8;
    seqtbl_offset_char_impl(state, seq_id as c_short);
}

/// Makes the current character draw his sword.
#[no_mangle]
pub unsafe extern "C" fn draw_sword() {
    draw_sword_impl(&mut State);
}

/// Controls while the sword is drawn.
///
/// Within about a tile and a half of the opponent this hands over to
/// [`swordfight_impl`]; if the opponent is behind the character he turns to
/// face him first. Out of range the Kid lowers his guard and puts the sword
/// away, while a guard keeps fencing at thin air (which is how he advances on
/// the Kid). Standing on a loose floor also counts as "in a fight", so a guard
/// does not sheathe while the tile under him is collapsing.
// seg005:0C67
unsafe fn control_with_sword_impl(state: &mut State) {
    if state.Char().action < actions_actions_2_hang_climb as u8 {
        if get_tile_at_char() == tiles_tiles_11_loose as c_int || *state.can_guard_see_kid() >= 2 {
            let distance = char_opp_dist();
            // Unsigned compares, as in C: a negative distance (opponent
            // behind) wraps past 90 and falls through to the `< 0` arm.
            if (distance as u32) < (90u32) {
                swordfight_impl(state);
                return;
            } else if distance < 0 {
                if (distance as u32) < ((-4i32) as u32) {
                    seqtbl_offset_char_impl(state, seqids_seq_60_turn_with_sword as c_short);
                    return;
                } else {
                    swordfight_impl(state);
                    return;
                }
            }
        }
        if state.Char().charid == charids_charid_0_kid as u8 && state.Char().alive < 0 {
            *state.holding_sword() = 0;
        }
        if (state.Char().charid as i32) < (charids_charid_2_guard as i32) {
            if state.Char().frame == frameids_frame_171_stand_with_sword as u8 {
                state.Char().sword = sword_status_sword_0_sheathed as u8;
                seqtbl_offset_char_impl(state, seqids_seq_92_put_sword_away as c_short);
            }
        } else {
            swordfight_impl(state);
        }
    }
}

/// Runs one frame of control input for a character with his sword out.
#[no_mangle]
pub unsafe extern "C" fn control_with_sword() {
    control_with_sword_impl(&mut State);
}

/// Maps the controls onto fencing moves while in range of the opponent.
///
/// Shift attacks, up parries, forward and back close and open the distance,
/// and down sheathes. The Kid sheathing in front of a guard is the deliberate
/// "offguard" gambit: it sets `offguard`, gives the guard a refractory period
/// and stops him from being counted as holding a sword.
// seg005:0CDB
unsafe fn swordfight_impl(state: &mut State) {
    let frame = state.Char().frame;
    let charid = state.Char().charid;
    // frame 161: parry
    if frame == frameids_frame_161_parry as u8 && *state.control_shift2() >= CONTROL_RELEASED as i8 {
        seqtbl_offset_char_impl(state, seqids_seq_57_back_with_sword as c_short);
        return;
    } else if *state.control_shift2() == CONTROL_HELD as i8 {
        if charid == charids_charid_0_kid as u8 {
            *state.kid_sword_strike() = 15;
        }
        sword_strike_impl(state);
        if *state.control_shift2() == CONTROL_IGNORE as i8 {
            return;
        }
    }
    if *state.control_down() == CONTROL_HELD as i8 {
        if frame == frameids_frame_158_stand_with_sword as u8
            || frame == frameids_frame_170_stand_with_sword as u8
            || frame == frameids_frame_171_stand_with_sword as u8
        {
            *state.control_down() = CONTROL_IGNORE as i8;
            state.Char().sword = sword_status_sword_0_sheathed as u8;
            let seq_id: u16 = if charid == charids_charid_0_kid as u8 {
                *state.offguard() = 1;
                *state.guard_refrac() = 9;
                *state.holding_sword() = 0;
                seqids_seq_93_put_sword_away_fast as u16
            } else if charid == charids_charid_1_shadow as u8 {
                seqids_seq_92_put_sword_away as u16
            } else {
                seqids_seq_87_guard_become_inactive as u16
            };
            seqtbl_offset_char_impl(state, seq_id as c_short);
        }
    } else if *state.control_up() == CONTROL_HELD as i8 {
        parry_impl(state);
    } else if *state.control_forward() == CONTROL_HELD as i8 {
        forward_with_sword_impl(state);
    } else if *state.control_backward() == CONTROL_HELD as i8 {
        back_with_sword_impl(state);
    }
}

/// Runs one frame of fencing input for the current character.
#[no_mangle]
pub unsafe extern "C" fn swordfight() {
    swordfight_impl(&mut State);
}

/// Attacks. Striking is only possible from the on-guard and walking stances,
/// or straight out of a successful parry (the riposte); from anything else the
/// press is dropped and the control is left latched.
// seg005:0DB0
unsafe fn sword_strike_impl(state: &mut State) {
    let frame = state.Char().frame;
    let seq_id: u16 = if frame == frameids_frame_157_walk_with_sword as u8
        || frame == frameids_frame_158_stand_with_sword as u8
        || frame == frameids_frame_170_stand_with_sword as u8
        || frame == frameids_frame_171_stand_with_sword as u8
        || frame == frameids_frame_165_walk_with_sword as u8
    {
        if state.Char().charid == charids_charid_0_kid as u8 {
            seqids_seq_75_strike as u16
        } else {
            seqids_seq_58_guard_strike as u16
        }
    } else if frame == frameids_frame_150_parry as u8 || frame == frameids_frame_161_parry as u8 {
        seqids_seq_66_strike_after_parry as u16
    } else {
        return;
    };
    *state.control_shift2() = CONTROL_IGNORE as i8;
    seqtbl_offset_char_impl(state, seq_id as c_short);
}

/// Makes the current character strike with his sword.
#[no_mangle]
pub unsafe extern "C" fn sword_strike() {
    sword_strike_impl(&mut State);
}

/// Blocks — but only against an attack that is actually coming.
///
/// The parry is refused unless the opponent is in one of his striking frames,
/// which is what makes blocking a matter of timing rather than of holding up.
/// A guard too far away to be struck backs off instead. Blocking the last
/// frame of a strike (frame 153) is late, and the block is played out
/// immediately with [`play_seq`] so it still lands in time.
// seg005:0E0F
unsafe fn parry_impl(state: &mut State) {
    let char_frame = state.Char().frame;
    let opp_frame = state.Opp().frame;
    let char_charid = state.Char().charid;
    let mut seq_id = seqids_seq_62_parry as u16;
    let mut do_play_seq = false;
    if char_frame == frameids_frame_158_stand_with_sword as u8
        || char_frame == frameids_frame_170_stand_with_sword as u8
        || char_frame == frameids_frame_171_stand_with_sword as u8
        || char_frame == frameids_frame_168_back as u8
        || char_frame == frameids_frame_165_walk_with_sword as u8
    {
        if char_opp_dist() >= 32 && char_charid != charids_charid_0_kid as u8 {
            back_with_sword_impl(state);
            return;
        } else if char_charid == charids_charid_0_kid as u8 {
            if opp_frame == frameids_frame_168_back as u8 {
                return;
            }
            if opp_frame != frameids_frame_151_strike_1 as u8
                && opp_frame != frameids_frame_152_strike_2 as u8
                && opp_frame != frameids_frame_162_block_to_strike as u8
            {
                if opp_frame == frameids_frame_153_strike_3 as u8 {
                    do_play_seq = true;
                } else if char_charid != charids_charid_0_kid as u8 {
                    back_with_sword_impl(state);
                    return;
                }
            }
        } else {
            if opp_frame != frameids_frame_152_strike_2 as u8 {
                return;
            }
        }
    } else if char_frame == frameids_frame_167_blocked as u8 {
        // Recovering from a blocked strike: parry straight out of the recoil.
        seq_id = seqids_seq_61_parry_after_strike as u16;
    } else {
        return;
    }
    *state.control_up() = CONTROL_IGNORE as i8;
    seqtbl_offset_char_impl(state, seq_id as c_short);
    if do_play_seq {
        play_seq();
    }
}

/// Makes the current character parry.
#[no_mangle]
pub unsafe extern "C" fn parry() {
    parry_impl(&mut State);
}

/// Steps out of one teleporter balcony and in at its pair.
///
/// Two balcony tiles anywhere in the level that carry the same nonzero tile
/// modifier are a teleporter pair; the entry side was recorded in
/// [`source_room`] / [`source_tilepos`] / [`source_modifier`] by
/// [`up_pressed_impl`]. The whole level is scanned for the other end; with no
/// pair to arrive at, the Kid drops on the spot and dies.
// USE_TELEPORTS
unsafe fn teleport_impl(state: &mut State) {
    // Find the pair of the teleport which the prince entered.
    // The `&&` chain matches the original nested ifs exactly: get_curr_tile is
    // only called for tiles that are not the source, and it is what leaves the
    // modifier the next test reads in curr_modifier.
    let mut destination: Option<(c_int, c_int)> = None;
    'search: for dest_room in 1..=24 {
        get_room_address(dest_room);
        for dest_tilepos in 0..30 {
            // Skip over the source teleport; the pair is a balcony tile with
            // the same modifier.
            if (dest_room != source_room || dest_tilepos != source_tilepos)
                && get_curr_tile(dest_tilepos as c_short) == tiles_tiles_23_balcony_left as c_short
                && *state.curr_modifier() as c_int == source_modifier
            {
                destination = Some((dest_room, dest_tilepos));
                break 'search;
            }
        }
    }

    if let Some((dest_room, dest_tilepos)) = destination {
        // We found a pair. Put the kid there.
        // Based on do_startpos().
        state.Char().room = dest_room as u8;
        state.Char().curr_col = (dest_tilepos % 10) as i8;
        state.Char().curr_row = (dest_tilepos / 10) as i8;
        let curr_col = state.Char().curr_col;
        let curr_row = state.Char().curr_row;
        state.Char().x = (x_bump_at((curr_col as i32 + 5) as usize) as i32 + 14 + 7) as u8; // Center on the destination teleport.
        state.Char().y = y_land_at((curr_row as usize) + 1) as u8;
        *state.next_room() = state.Char().room as u16;
        clear_coll_rooms(); // Without this, the prince will sometimes end up at the wrong place.
        // FIX_DISAPPEARING_GUARD_B
        if *state.next_room() != *state.drawn_room() {
            leave_guard();
        }
        // FIX_DISAPPEARING_GUARD_A
        if *state.next_room() == *state.drawn_room() {
            *state.drawn_room() = 0;
        }
        seqtbl_offset_char_impl(state, seqids_seq_5_turn as c_short);
        play_sound(soundids_sound_45_jump_through_mirror as c_int);
    } else {
        // No pair found.
        let curr_col = state.Char().curr_col;
        let curr_row = state.Char().curr_row;
        state.Char().x = (x_bump_at((curr_col as i32 + 5) as usize) as i32 + 14) as u8;
        state.Char().y = y_land_at((curr_row as usize) + 1) as u8;
        seqtbl_offset_char_impl(state, seqids_seq_17_soft_land as c_short);
        play_sound(soundids_sound_0_fell_to_death as c_int);
    }
}

/// Teleports the current character to the balcony paired with the one he
/// stepped into.
#[no_mangle]
pub unsafe extern "C" fn teleport() {
    teleport_impl(&mut State);
}
