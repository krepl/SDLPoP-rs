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

// Helper to access incomplete extern array seqtbl_offsets
unsafe fn seqtbl_offsets_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(seqtbl_offsets).cast::<u16>().add(idx)
}

// seg005:000A
unsafe fn seqtbl_offset_char_impl(state: &mut State, seq_index: c_short) {
    state.Char().curr_seq = seqtbl_offsets_at(seq_index as usize);
}

#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_char(seq_index: c_short) {
    seqtbl_offset_char_impl(&mut State, seq_index);
}

// seg005:001D
unsafe fn seqtbl_offset_opp_impl(state: &mut State, seq_index: c_int) {
    state.Opp().curr_seq = seqtbl_offsets_at(seq_index as usize);
}

#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_opp(seq_index: c_int) {
    seqtbl_offset_opp_impl(&mut State, seq_index);
}

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

#[no_mangle]
pub unsafe extern "C" fn do_fall() {
    do_fall_impl(&mut State);
}

// seg005:0090
unsafe fn land_impl(state: &mut State) {
    let seq_id: u16;
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

    if state.Char().alive < 0 {
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
            // fell 1 row
            if state.Char().charid >= charids_charid_2_guard as u8
                || state.Char().sword == sword_status_sword_2_drawn as u8
            {
                state.Char().sword = sword_status_sword_2_drawn as u8;
                seq_id = seqids_seq_63_guard_active_after_fall as u16;
            } else {
                seq_id = seqids_seq_17_soft_land as u16;
            }
            if state.Char().charid == charids_charid_0_kid as u8 {
                play_sound(soundids_sound_17_soft_land as c_int);
                *state.is_guard_notice() = 1;
            }
        } else if state.Char().fall_y < 33 {
            // fell 2 rows
            if state.Char().charid == charids_charid_1_shadow as u8 {
                if state.Char().charid >= charids_charid_2_guard as u8
                    || state.Char().sword == sword_status_sword_2_drawn as u8
                {
                    state.Char().sword = sword_status_sword_2_drawn as u8;
                    seq_id = seqids_seq_63_guard_active_after_fall as u16;
                } else {
                    seq_id = seqids_seq_17_soft_land as u16;
                }
                if state.Char().charid == charids_charid_0_kid as u8 {
                    play_sound(soundids_sound_17_soft_land as c_int);
                    *state.is_guard_notice() = 1;
                }
            } else if state.Char().charid == charids_charid_2_guard as u8 {
                // fell 3 or more rows
                take_hp(100);
                play_sound(soundids_sound_0_fell_to_death as c_int);
                seq_id = seqids_seq_22_crushed as u16;
            } else {
                // kid (or skeleton (bug!))
                if take_hp(1) == 0 {
                    // still alive
                    play_sound(soundids_sound_16_medium_land as c_int);
                    *state.is_guard_notice() = 1;
                    seq_id = seqids_seq_20_medium_land as u16;
                } else {
                    // dead (this was the last HP)
                    take_hp(100);
                    play_sound(soundids_sound_0_fell_to_death as c_int);
                    seq_id = seqids_seq_22_crushed as u16;
                }
            }
        } else {
            // fell 3 or more rows
            take_hp(100);
            play_sound(soundids_sound_0_fell_to_death as c_int);
            seq_id = seqids_seq_22_crushed as u16;
        }
    } else {
        // dead
        take_hp(100);
        play_sound(soundids_sound_0_fell_to_death as c_int);
        seq_id = seqids_seq_22_crushed as u16;
    }

    seqtbl_offset_char_impl(state, seq_id as c_short);
    play_seq();
    state.Char().fall_y = 0;
}

#[no_mangle]
pub unsafe extern "C" fn land() {
    land_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn spiked() {
    spiked_impl(&mut State);
}

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
            || (char_frame >= frameids_frame_50_turn as u8 && char_frame < 53)
        {
            control_standing_impl(state);
        } else if char_frame == frameids_frame_48_turn as u8 {
            control_turning_impl(state);
        } else if char_frame < 4 {
            control_startrun_impl(state);
        } else if char_frame >= frameids_frame_67_start_jump_up_1 as u8
            && char_frame < frameids_frame_70_jumphang as u8
        {
            control_jumpup_impl(state);
        } else if char_frame < 15 {
            control_running_impl(state);
        } else if char_frame >= frameids_frame_87_hanging_1 as u8 && char_frame < 100 {
            control_hanging_impl(state);
        } else if char_frame == frameids_frame_109_crouch as u8 {
            control_crouched_impl(state);
        }

        // ALLOW_CROUCH_AFTER_CLIMBING
        if (*fixes).enable_crouch_after_climbing != 0
            && state.Char().curr_seq >= seqtbl_offsets_at(seqids_seq_50_crouch as usize)
            && state.Char().curr_seq < seqtbl_offsets_at(seqids_seq_49_stand_up_from_crouch as usize)
        {
            if *state.control_forward() != CONTROL_IGNORE as i8 {
                *state.control_forward() = CONTROL_RELEASED as i8;
            }
        }

        // FIX_MOVE_AFTER_DRINK
        if (*fixes).fix_move_after_drink != 0
            && char_frame >= frameids_frame_191_drink as u8
            && char_frame <= frameids_frame_205_drink as u8
        {
            release_arrows();
        }

        // FIX_MOVE_AFTER_SHEATHE
        if (*fixes).fix_move_after_sheathe != 0
            && state.Char().curr_seq >= seqtbl_offsets_at(seqids_seq_92_put_sword_away as usize)
            && state.Char().curr_seq < seqtbl_offsets_at(seqids_seq_93_put_sword_away_fast as usize)
        {
            release_arrows();
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn control() {
    control_impl(&mut State);
}

// ── File-scoped statics (for USE_TELEPORTS feature) ──────────────────────────
static mut source_modifier: c_int = 0;
static mut source_room: c_int = 0;
static mut source_tilepos: c_int = 0;

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

#[no_mangle]
pub unsafe extern "C" fn control_crouched() {
    control_crouched_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn control_standing() {
    control_standing_impl(&mut State);
}

// seg005:0482
unsafe fn up_pressed_impl(state: &mut State) {
    // If there is an open level door nearby, enter it.
    let mut leveldoor_tilepos: c_int = -1;
    if get_tile_at_char() == tiles_tiles_16_level_door_left as c_int {
        leveldoor_tilepos = curr_tilepos as c_int;
    } else if get_tile_behind_char() == tiles_tiles_16_level_door_left as c_int {
        leveldoor_tilepos = curr_tilepos as c_int;
    } else if get_tile_infrontof_char() == tiles_tiles_16_level_door_left as c_int {
        leveldoor_tilepos = curr_tilepos as c_int;
    }
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
    leveldoor_tilepos = -1;
    // This detection is not perfect...
    if get_tile_at_char() == tiles_tiles_23_balcony_left as c_int {
        leveldoor_tilepos = curr_tilepos as c_int;
    } else if get_tile_behind_char() == tiles_tiles_23_balcony_left as c_int {
        leveldoor_tilepos = curr_tilepos as c_int;
    } else if get_tile_infrontof_char() == tiles_tiles_23_balcony_left as c_int {
        leveldoor_tilepos = curr_tilepos as c_int;
    }
    if leveldoor_tilepos != -1 {
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

#[no_mangle]
pub unsafe extern "C" fn up_pressed() {
    up_pressed_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn down_pressed() {
    down_pressed_impl(&mut State);
}

// seg005:0574
unsafe fn go_up_leveldoor_impl(state: &mut State) {
    state.Char().x = x_bump_at((tile_col + FIRST_ONSCREEN_COLUMN as i16) as usize) as u8 + 10;
    state.Char().direction = directions_dir_FF_left as i8;
    seqtbl_offset_char_impl(state, seqids_seq_70_go_up_on_level_door as c_short);
}

#[no_mangle]
pub unsafe extern "C" fn go_up_leveldoor() {
    go_up_leveldoor_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn control_turning() {
    control_turning_impl(&mut State);
}

// seg005:05AD
unsafe fn crouch_impl(state: &mut State) {
    seqtbl_offset_char_impl(state, seqids_seq_50_crouch as c_short);
    *state.control_down() = release_arrows() as i8;
}

#[no_mangle]
pub unsafe extern "C" fn crouch() {
    crouch_impl(&mut State);
}

// seg005:05BE
unsafe fn back_pressed_impl(state: &mut State) {
    let seq_id: u16;
    *state.control_backward() = release_arrows() as i8;
    // After turn, Kid will draw sword if ...
    if *state.have_sword() == 0
        || *state.can_guard_see_kid() < 2
        || char_opp_dist() > 0
        || distance_to_edge_weight() < 2
    {
        seq_id = seqids_seq_5_turn as u16;
    } else {
        state.Char().sword = sword_status_sword_2_drawn as u8;
        *state.offguard() = 0;
        seq_id = seqids_seq_89_turn_draw_sword as u16;
    }
    seqtbl_offset_char_impl(state, seq_id as c_short);
}

#[no_mangle]
pub unsafe extern "C" fn back_pressed() {
    back_pressed_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn forward_pressed() {
    forward_pressed_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn control_running() {
    control_running_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn safe_step() {
    safe_step_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn check_get_item() -> c_int {
    check_get_item_impl(&mut State)
}

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
            let mut index = 0;
            while index < 14 {
                // Check copyprot_room and copyprot_tile (incomplete arrays)
                let copyprot_room_val =
                    *core::ptr::addr_of!(copyprot_room).cast::<u16>().add(index);
                let copyprot_tile_val =
                    *core::ptr::addr_of!(copyprot_tile).cast::<u8>().add(index);
                if (copyprot_room_val as i16) == curr_room && copyprot_tile_val == curr_tilepos {
                    core::ptr::addr_of_mut!(copyprot_room).cast::<u16>().add(index).write(0);
                    break;
                }
                index += 1;
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn get_item() {
    get_item_impl(&mut State);
}

// seg005:07FF
unsafe fn control_startrun_impl(state: &mut State) {
    if *state.control_y() == CONTROL_HELD_UP as i8 && *state.control_x() == CONTROL_HELD_FORWARD as i8 {
        standing_jump_impl(state);
    }
}

#[no_mangle]
pub unsafe extern "C" fn control_startrun() {
    control_startrun_impl(&mut State);
}

// seg005:0812
unsafe fn control_jumpup_impl(state: &mut State) {
    if *state.control_x() == CONTROL_HELD_FORWARD as i8 || *state.control_forward() == CONTROL_HELD as i8 {
        standing_jump_impl(state);
    }
}

#[no_mangle]
pub unsafe extern "C" fn control_jumpup() {
    control_jumpup_impl(&mut State);
}

// seg005:0825
unsafe fn standing_jump_impl(state: &mut State) {
    *state.control_up() = CONTROL_IGNORE as i8;
    *state.control_forward() = CONTROL_IGNORE as i8;
    seqtbl_offset_char_impl(state, seqids_seq_3_standing_jump as c_short);
}

#[no_mangle]
pub unsafe extern "C" fn standing_jump() {
    standing_jump_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn check_jump_up() {
    check_jump_up_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn jump_up_or_grab() {
    jump_up_or_grab_impl(&mut State);
}

// seg005:08C7
unsafe fn grab_up_no_floor_behind_impl(state: &mut State) {
    get_tile_above_char();
    state.Char().x = char_dx_forward(distance_to_edge_weight() - 10) as u8;
    seqtbl_offset_char_impl(state, seqids_seq_16_jump_up_and_grab as c_short);
}

#[no_mangle]
pub unsafe extern "C" fn grab_up_no_floor_behind() {
    grab_up_no_floor_behind_impl(&mut State);
}

// seg005:08E6
unsafe fn jump_up_impl(state: &mut State) {
    let delta_x: u16;
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
    if *state.is_feather_fall() != 0 && tile_is_floor(get_tile_above_char()) == 0 && curr_tile2 != tiles_tiles_20_wall as u8 {
        delta_x = if state.Char().direction == directions_dir_FF_left as i8 { 1 } else { 3 };
    } else {
        delta_x = 0;
    }
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
                let is_top_floor = tile_is_floor(curr_tile2 as c_int) != 0 || curr_tile2 == tiles_tiles_20_wall as u8;
                let mut is_top_floor_final = is_top_floor;
                if is_top_floor && curr_tile2 == tiles_tiles_11_loose as u8 && (curr_room_tiles.add(curr_tilepos as usize).read() & 0x20) == 0 {
                    is_top_floor_final = false;
                }
                // kid should jump slightly higher if the top tile is not a floor
                *state.super_jump_timer() = if is_top_floor_final { 22 } else { 24 };
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

#[no_mangle]
pub unsafe extern "C" fn jump_up() {
    jump_up_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn control_hanging() {
    control_hanging_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn can_climb_up() {
    can_climb_up_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn hang_fall() {
    hang_fall_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn grab_up_with_floor_behind() {
    grab_up_with_floor_behind_impl(&mut State);
}

// seg005:0AF7
unsafe fn run_jump_impl(state: &mut State) {
    if state.Char().frame >= frameids_frame_7_run as u8 {
        // Align Kid to edge of floor.
        let xpos = char_dx_forward(4);
        let mut col = get_tile_div_mod_m7(xpos);
        let char_direction = state.Char().direction;
        for tiles_forward in 0..2 {
            col += *core::ptr::addr_of!(dir_front).cast::<i8>().add((char_direction as i8 as i32 + 1) as usize) as i32;
            let char_room = state.Char().room;
            let curr_row = state.Char().curr_row;
            get_tile(char_room as c_int, col, curr_row as c_int);
            if curr_tile2 == tiles_tiles_2_spike as u8 || tile_is_floor(curr_tile2 as c_int) == 0 {
                let mut pos_adjustment =
                    distance_to_edge(xpos) + (TILE_SIZEX as i32) * tiles_forward - (TILE_SIZEX as i32);
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

#[no_mangle]
pub unsafe extern "C" fn run_jump() {
    run_jump_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn back_with_sword() {
    back_with_sword_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn forward_with_sword() {
    forward_with_sword_impl(&mut State);
}

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

#[no_mangle]
pub unsafe extern "C" fn draw_sword() {
    draw_sword_impl(&mut State);
}

// seg005:0C67
unsafe fn control_with_sword_impl(state: &mut State) {
    if state.Char().action < actions_actions_2_hang_climb as u8 {
        if get_tile_at_char() == tiles_tiles_11_loose as c_int || *state.can_guard_see_kid() >= 2 {
            let distance = char_opp_dist();
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

#[no_mangle]
pub unsafe extern "C" fn control_with_sword() {
    control_with_sword_impl(&mut State);
}

// seg005:0CDB
unsafe fn swordfight_impl(state: &mut State) {
    let seq_id: u16;
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
            if charid == charids_charid_0_kid as u8 {
                *state.offguard() = 1;
                *state.guard_refrac() = 9;
                *state.holding_sword() = 0;
                seq_id = seqids_seq_93_put_sword_away_fast as u16;
            } else if charid == charids_charid_1_shadow as u8 {
                seq_id = seqids_seq_92_put_sword_away as u16;
            } else {
                seq_id = seqids_seq_87_guard_become_inactive as u16;
            }
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

#[no_mangle]
pub unsafe extern "C" fn swordfight() {
    swordfight_impl(&mut State);
}

// seg005:0DB0
unsafe fn sword_strike_impl(state: &mut State) {
    let frame = state.Char().frame;
    let seq_id: u16;
    if frame == frameids_frame_157_walk_with_sword as u8
        || frame == frameids_frame_158_stand_with_sword as u8
        || frame == frameids_frame_170_stand_with_sword as u8
        || frame == frameids_frame_171_stand_with_sword as u8
        || frame == frameids_frame_165_walk_with_sword as u8
    {
        if state.Char().charid == charids_charid_0_kid as u8 {
            seq_id = seqids_seq_75_strike as u16;
        } else {
            seq_id = seqids_seq_58_guard_strike as u16;
        }
    } else if frame == frameids_frame_150_parry as u8 || frame == frameids_frame_161_parry as u8 {
        seq_id = seqids_seq_66_strike_after_parry as u16;
    } else {
        return;
    }
    *state.control_shift2() = CONTROL_IGNORE as i8;
    seqtbl_offset_char_impl(state, seq_id as c_short);
}

#[no_mangle]
pub unsafe extern "C" fn sword_strike() {
    sword_strike_impl(&mut State);
}

// seg005:0E0F
unsafe fn parry_impl(state: &mut State) {
    let char_frame = state.Char().frame;
    let opp_frame = state.Opp().frame;
    let char_charid = state.Char().charid;
    let mut seq_id = seqids_seq_62_parry as u16;
    let mut do_play_seq: i32 = 0;
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
                    do_play_seq = 1;
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
    } else {
        if char_frame != frameids_frame_167_blocked as u8 {
            return;
        }
        seq_id = seqids_seq_61_parry_after_strike as u16;
    }
    *state.control_up() = CONTROL_IGNORE as i8;
    seqtbl_offset_char_impl(state, seq_id as c_short);
    if do_play_seq != 0 {
        play_seq();
    }
}

#[no_mangle]
pub unsafe extern "C" fn parry() {
    parry_impl(&mut State);
}

// USE_TELEPORTS
unsafe fn teleport_impl(state: &mut State) {
    let mut found = false;
    let mut dest_room: c_int = 1;
    let mut dest_tilepos: c_int = 0;

    // Find the pair of the teleport which the prince entered.
    while dest_room <= 24 {
        get_room_address(dest_room);

        dest_tilepos = 0;
        while dest_tilepos < 30 {
            // Skip over the source teleport.
            if dest_room != source_room || dest_tilepos != source_tilepos {
                // The pair is a balcony tile with the same modifier.
                if get_curr_tile(dest_tilepos as c_short) == tiles_tiles_23_balcony_left as c_short
                    && *state.curr_modifier() as c_int == source_modifier
                {
                    found = true;
                    break;
                }
            }
            dest_tilepos += 1;
        }
        if found {
            break;
        }
        dest_room += 1;
    }

    if found {
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

#[no_mangle]
pub unsafe extern "C" fn teleport() {
    teleport_impl(&mut State);
}
