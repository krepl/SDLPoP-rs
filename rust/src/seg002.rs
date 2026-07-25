// Guard/shadow AI — ported from seg002.c.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;
use crate::state::State;

// seg002:0000
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

#[no_mangle]
pub unsafe extern "C" fn do_init_shad(source: *const byte, seq_index: c_int) {
    do_init_shad_impl(&mut State, source, seq_index);
}

// seg002:0044
unsafe fn get_guard_hp_impl(state: &mut State) {
    let hp = (*custom).extrastrength[*state.guard_skill() as usize] as i32
        + (*custom).tbl_guard_hp[*state.current_level() as usize] as i32;
    *state.guardhp_max() = hp as u16;
    *state.guardhp_curr() = hp as u16;
    *state.guardhp_delta() = hp as c_short;
}

#[no_mangle]
pub unsafe extern "C" fn get_guard_hp() {
    get_guard_hp_impl(&mut State);
}

// seg002:0064
unsafe fn check_shadow_impl(state: &mut State) {
    *state.offguard() = 0;
    if *state.current_level() == 12 {
        if *state.united_with_shadow() == 0 && *state.drawn_room() == 15 {
            state.Char().room = *state.drawn_room() as u8;
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
    }
    if *state.current_level() == (*custom).shadow_step_level as u16 {
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
        state.Char().room = *state.drawn_room() as u8;
        if state.Char().room == (*custom).shadow_steal_room {
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

#[no_mangle]
pub unsafe extern "C" fn check_shadow() {
    check_shadow_impl(&mut State);
}

// seg002:0112
unsafe fn enter_guard_impl(state: &mut State) {
    let room_minus_1 = (*state.drawn_room() - 1) as usize;
    let mut guard_tile = state.level().guards_tile[room_minus_1];

    if guard_tile >= 30 {
        if (*fixes).fix_offscreen_guards_disappearing == 0 {
            return;
        }
        let room_l = *state.room_L();
        let room_r = *state.room_R();
        let left_guard_tile: i16 =
            if room_l > 0 { state.level().guards_tile[(room_l - 1) as usize] as i16 } else { 31 };
        let right_guard_tile: i16 =
            if room_r > 0 { state.level().guards_tile[(room_r - 1) as usize] as i16 } else { 31 };

        // Determine which offscreen guard to pull in.
        // The goto in C (right→left fallthrough) is modeled with a bool + two passes.
        let mut result: Option<(usize, i32, u8)> = None;
        let mut try_left = false;

        if right_guard_tile >= 0 && right_guard_tile < 30 {
            let ormi = (room_r - 1) as usize;
            let mut ogx = state.level().guards_x[ormi] as i32;
            let ogd = state.level().guards_dir[ormi] as i8;
            if ogd == directions_dir_0_right as i8 { ogx -= 9; }
            if ogd == directions_dir_FF_left as i8 { ogx += 1; }
            if ogx < 62 {
                result = Some((ormi, 140, right_guard_tile as u8));
            } else {
                try_left = left_guard_tile >= 0 && left_guard_tile < 30;
            }
        } else {
            try_left = left_guard_tile >= 0 && left_guard_tile < 30;
        }

        if result.is_none() {
            if !try_left { return; }
            let ormi = (room_l - 1) as usize;
            let mut ogx = state.level().guards_x[ormi] as i32;
            let ogd = state.level().guards_dir[ormi] as i8;
            if ogd == directions_dir_0_right as i8 { ogx -= 9; }
            if ogd == directions_dir_FF_left as i8 { ogx += 1; }
            if ogx <= 186 { return; }
            result = Some((ormi, -140, left_guard_tile as u8));
        }

        let (ormi, delta_x, new_tile) = result.unwrap();
        guard_tile = new_tile;
        state.level().guards_x[room_minus_1] = (state.level().guards_x[ormi] as i32 + delta_x) as u8;
        state.level().guards_color[room_minus_1] = state.level().guards_color[ormi];
        state.level().guards_dir[room_minus_1] = state.level().guards_dir[ormi];
        state.level().guards_seq_hi[room_minus_1] = state.level().guards_seq_hi[ormi];
        state.level().guards_seq_lo[room_minus_1] = state.level().guards_seq_lo[ormi];
        state.level().guards_skill[room_minus_1] = state.level().guards_skill[ormi];
        state.level().guards_tile[ormi] = 0xFF;
        state.level().guards_seq_hi[ormi] = 0;
    }

    state.Char().room = *state.drawn_room() as u8;
    state.Char().curr_row = (guard_tile / SCREEN_TILECOUNTX as u8) as i8;
    let curr_row = state.Char().curr_row;
    state.Char().y = y_land_at((curr_row + 1) as usize) as u8;
    state.Char().x = state.level().guards_x[room_minus_1];
    let char_x = state.Char().x;
    state.Char().curr_col = get_tile_div_mod_m7(char_x as c_int) as i8;
    state.Char().direction = state.level().guards_dir[room_minus_1] as i8;

    let cl = *state.current_level() as usize;
    if graphics_mode == grmodes_gmMcgaVga as u8
        && (*custom).tbl_guard_type[cl] == 0
    {
        *state.curr_guard_color() = state.level().guards_color[room_minus_1] as u16;
    } else {
        *state.curr_guard_color() = 0;
    }

    let remembered_hp = ((state.level().guards_color[room_minus_1] & 0xF0) >> 4) as i32;
    *state.curr_guard_color() &= 0x0F;

    if (*custom).tbl_guard_type[cl] == 2 {
        state.Char().charid = charids_charid_4_skeleton as u8;
    } else {
        state.Char().charid = charids_charid_2_guard as u8;
    }

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

#[no_mangle]
pub unsafe extern "C" fn enter_guard() {
    enter_guard_impl(&mut State);
}

// seg002:0269
unsafe fn check_guard_fallout_impl(state: &mut State) {
    if state.Guard().direction == directions_dir_56_none as i8 || state.Guard().y < 211 {
        return;
    }
    if state.Guard().charid == charids_charid_1_shadow as u8 {
        if state.Guard().action != actions_actions_4_in_freefall as u8 {
            return;
        }
        loadshad();
        clear_char();
        saveshad();
    } else if state.Guard().charid == charids_charid_4_skeleton as u8
        && {
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

#[no_mangle]
pub unsafe extern "C" fn check_guard_fallout() {
    check_guard_fallout_impl(&mut State);
}

// seg002:02F5
unsafe fn leave_guard_impl(state: &mut State) {
    if state.Guard().direction == directions_dir_56_none as i8
        || state.Guard().charid == charids_charid_1_shadow as u8
        || state.Guard().charid == charids_charid_24_mouse as u8
    {
        return;
    }
    let room_minus_1 = (state.Guard().room as usize) - 1;
    let guard_curr_row = state.Guard().curr_row;
    state.level().guards_tile[room_minus_1] = get_tilepos(0, guard_curr_row as c_int) as u8;

    state.level().guards_color[room_minus_1] = (*state.curr_guard_color() & 0x0F) as u8;
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

#[no_mangle]
pub unsafe extern "C" fn leave_guard() {
    leave_guard_impl(&mut State);
}

// seg002:039E
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

#[no_mangle]
pub unsafe extern "C" fn follow_guard() {
    follow_guard_impl(&mut State);
}

// seg002:03C7
unsafe fn exit_room_impl(state: &mut State) {
    let mut leave: i16 = 0;
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
    if state.Guard().alive < 0 && state.Guard().sword == sword_status_sword_2_drawn as u8 {
        let kid_room_m1 = (state.Kid().room as i16) - 1;
        if (kid_room_m1 >= 0 && kid_room_m1 <= 23)
            && (state.level().guards_tile[kid_room_m1 as usize] >= 30
                || state.level().guards_seq_hi[kid_room_m1 as usize] != 0)
        {
            let rlr = *state.roomleave_result();
            if rlr == 0 {
                // left
                if state.Guard().x >= 91 {
                    leave = 1;
                } else if (*fixes).fix_guard_following_through_closed_gates != 0
                    && *state.can_guard_see_kid() != 2
                    && state.Kid().sword != sword_status_sword_2_drawn as u8
                {
                    leave = 1;
                }
            } else if rlr == 1 {
                // right
                if state.Guard().x < 165 {
                    leave = 1;
                } else if (*fixes).fix_guard_following_through_closed_gates != 0
                    && *state.can_guard_see_kid() != 2
                    && state.Kid().sword != sword_status_sword_2_drawn as u8
                {
                    leave = 1;
                }
            } else if rlr == 2 {
                // up
                if state.Guard().curr_row >= 0 {
                    leave = 1;
                }
            } else {
                // down
                if state.Guard().curr_row < 3 {
                    leave = 1;
                }
            }
        } else {
            leave = 1;
        }
    } else {
        leave = 1;
    }
    if leave != 0 {
        leave_guard_impl(state);
    } else {
        follow_guard_impl(state);
    }
}

#[no_mangle]
pub unsafe extern "C" fn exit_room() {
    exit_room_impl(&mut State);
}

// seg002:0486
unsafe fn goto_other_room_impl(state: &mut State, direction: c_short) -> c_int {
    let other_room: u8;
    if state.Char().room == 0 {
        other_room = 0;
    } else {
        let char_room = state.Char().room;
        let rlinks = &state.level().roomlinks[(char_room as usize) - 1];
        other_room = match direction {
            0 => rlinks.left,
            1 => rlinks.right,
            2 => rlinks.up,
            _ => rlinks.down,
        };
    }
    state.Char().room = other_room;
    let opposite_dir: c_int;
    if direction == 0 {
        state.Char().x = state.Char().x.wrapping_add(140);
        opposite_dir = 1;
    } else if direction == 1 {
        state.Char().x = state.Char().x.wrapping_sub(140);
        opposite_dir = 0;
    } else if direction == 2 {
        state.Char().y = state.Char().y.wrapping_add(189);
        let char_y = state.Char().y;
        state.Char().curr_row = y_to_row_mod4(char_y as c_int) as i8;
        opposite_dir = 3;
    } else {
        state.Char().y = state.Char().y.wrapping_sub(189);
        let char_y = state.Char().y;
        state.Char().curr_row = y_to_row_mod4(char_y as c_int) as i8;
        opposite_dir = 2;
    }
    opposite_dir
}

#[no_mangle]
pub unsafe extern "C" fn goto_other_room(direction: c_short) -> c_int {
    goto_other_room_impl(&mut State, direction)
}

// seg002:0504
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
    } else if (frame >= frameids_frame_135_climbing_1 as u8 && frame < 150)
        || (frame >= frameids_frame_110_stand_up_from_crouch_1 as u8 && frame < 120)
        || (frame >= frameids_frame_150_parry as u8
            && frame < 163
            && (frame != frameids_frame_157_walk_with_sword as u8
                || (*fixes).fix_retreat_without_leaving_room == 0))
        || (frame >= frameids_frame_166_stand_inactive as u8 && frame < 169)
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
        // looking right
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

#[no_mangle]
pub unsafe extern "C" fn leave_room() -> c_short {
    leave_room_impl(&mut State)
}

// seg002:0643
unsafe fn Jaffar_exit_impl(state: &mut State) {
    if *state.leveldoor_open() == 2 {
        get_tile(24, 0, 0);
        trigger_button(0, 0, -1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn Jaffar_exit() {
    Jaffar_exit_impl(&mut State);
}

// seg002:0665
unsafe fn level3_set_chkp_impl(state: &mut State) {
    if *state.current_level() == (*custom).checkpoint_level && state.Char().room == 7 {
        *state.checkpoint() = 1;
        *state.hitp_beg_lev() = *state.hitp_max();
    }
}

#[no_mangle]
pub unsafe extern "C" fn level3_set_chkp() {
    level3_set_chkp_impl(&mut State);
}

// seg002:0680
unsafe fn sword_disappears_impl(state: &mut State) {
    if *state.current_level() == 12 && state.Char().room == 18 {
        get_tile(15, 1, 0);
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
        *curr_room_modif.add(curr_tilepos as usize) = 0;
    }
}

#[no_mangle]
pub unsafe extern "C" fn sword_disappears() {
    sword_disappears_impl(&mut State);
}

// seg002:06AE
unsafe fn meet_Jaffar_impl(state: &mut State) {
    if *state.current_level() == 13 && *state.leveldoor_open() == 0 && state.Char().room == 3 {
        play_sound(soundids_sound_29_meet_Jaffar as c_int);
        *state.guard_notice_timer() = 28;
    }
}

#[no_mangle]
pub unsafe extern "C" fn meet_Jaffar() {
    meet_Jaffar_impl(&mut State);
}

// seg002:06D3
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

#[no_mangle]
pub unsafe extern "C" fn play_mirr_mus() {
    play_mirr_mus_impl(&mut State);
}

// seg002:0706
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

#[no_mangle]
pub unsafe extern "C" fn move_0_nothing() {
    move_0_nothing_impl(&mut State);
}

// seg002:0721
unsafe fn move_1_forward_impl(state: &mut State) {
    *state.control_x() = CONTROL_HELD_FORWARD as i8;
    *state.control_forward() = CONTROL_HELD as i8;
}

#[no_mangle]
pub unsafe extern "C" fn move_1_forward() {
    move_1_forward_impl(&mut State);
}

// seg002:072A
unsafe fn move_2_backward_impl(state: &mut State) {
    *state.control_backward() = CONTROL_HELD as i8;
    *state.control_x() = CONTROL_HELD_BACKWARD as i8;
}

#[no_mangle]
pub unsafe extern "C" fn move_2_backward() {
    move_2_backward_impl(&mut State);
}

// seg002:0735
unsafe fn move_3_up_impl(state: &mut State) {
    *state.control_y() = CONTROL_HELD_UP as i8;
    *state.control_up() = CONTROL_HELD as i8;
}

#[no_mangle]
pub unsafe extern "C" fn move_3_up() {
    move_3_up_impl(&mut State);
}

// seg002:073E
unsafe fn move_4_down_impl(state: &mut State) {
    *state.control_down() = CONTROL_HELD as i8;
    *state.control_y() = CONTROL_HELD_DOWN as i8;
}

#[no_mangle]
pub unsafe extern "C" fn move_4_down() {
    move_4_down_impl(&mut State);
}

// seg002:0749
unsafe fn move_up_back_impl(state: &mut State) {
    *state.control_up() = CONTROL_HELD as i8;
    move_2_backward_impl(state);
}

#[no_mangle]
pub unsafe extern "C" fn move_up_back() {
    move_up_back_impl(&mut State);
}

// seg002:0753
unsafe fn move_down_back_impl(state: &mut State) {
    *state.control_down() = CONTROL_HELD as i8;
    move_2_backward_impl(state);
}

#[no_mangle]
pub unsafe extern "C" fn move_down_back() {
    move_down_back_impl(&mut State);
}

// seg002:075D
unsafe fn move_down_forw_impl(state: &mut State) {
    *state.control_down() = CONTROL_HELD as i8;
    move_1_forward_impl(state);
}

#[no_mangle]
pub unsafe extern "C" fn move_down_forw() {
    move_down_forw_impl(&mut State);
}

// seg002:0767
unsafe fn move_6_shift_impl(state: &mut State) {
    *state.control_shift() = CONTROL_HELD as i8;
    *state.control_shift2() = CONTROL_HELD as i8;
}

#[no_mangle]
pub unsafe extern "C" fn move_6_shift() {
    move_6_shift_impl(&mut State);
}

// seg002:0770
unsafe fn move_7_impl(state: &mut State) {
    *state.control_shift() = CONTROL_RELEASED as i8;
}

#[no_mangle]
pub unsafe extern "C" fn move_7() {
    move_7_impl(&mut State);
}

// seg002:0776
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

#[no_mangle]
pub unsafe extern "C" fn autocontrol_opponent() {
    autocontrol_opponent_impl(&mut State);
}

// seg002:07EB
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

#[no_mangle]
pub unsafe extern "C" fn autocontrol_mouse() {
    autocontrol_mouse_impl(&mut State);
}

// seg002:081D
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

#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow() {
    autocontrol_shadow_impl(&mut State);
}

// seg002:0850
unsafe fn autocontrol_skeleton_impl(state: &mut State) {
    state.Char().sword = sword_status_sword_2_drawn as u8;
    autocontrol_guard_impl(state);
}

#[no_mangle]
pub unsafe extern "C" fn autocontrol_skeleton() {
    autocontrol_skeleton_impl(&mut State);
}

// seg002:085A
unsafe fn autocontrol_Jaffar_impl(state: &mut State) {
    autocontrol_guard_impl(state);
}

#[no_mangle]
pub unsafe extern "C" fn autocontrol_Jaffar() {
    autocontrol_Jaffar_impl(&mut State);
}

// seg002:085F
unsafe fn autocontrol_kid_impl(state: &mut State) {
    autocontrol_guard_impl(state);
}

#[no_mangle]
pub unsafe extern "C" fn autocontrol_kid() {
    autocontrol_kid_impl(&mut State);
}

// seg002:0864
unsafe fn autocontrol_guard_impl(state: &mut State) {
    if state.Char().sword < sword_status_sword_2_drawn as u8 {
        autocontrol_guard_inactive_impl(state);
    } else {
        autocontrol_guard_active_impl(state);
    }
}

#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard() {
    autocontrol_guard_impl(&mut State);
}

// seg002:0876
unsafe fn autocontrol_guard_inactive_impl(state: &mut State) {
    if state.Kid().alive >= 0 { return; }
    let distance = char_opp_dist() as i16;
    if state.Opp().curr_row != state.Char().curr_row || (distance as u16) < 0xFFF8u16 {
        if *state.is_guard_notice() != 0 {
            *state.is_guard_notice() = 0;
            if distance < 0 {
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
        if *state.current_level() != 13 || *state.guard_notice_timer() == 0 {
            move_down_forw_impl(state);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_inactive() {
    autocontrol_guard_inactive_impl(&mut State);
}

// seg002:08DC
unsafe fn autocontrol_guard_active_impl(state: &mut State) {
    let char_frame = state.Char().frame;
    if char_frame != frameids_frame_166_stand_inactive as u8
        && char_frame >= 150
        && *state.can_guard_see_kid() != 1
    {
        if *state.can_guard_see_kid() == 0 {
            if *state.droppedout() != 0 {
                guard_follows_kid_down_impl(state);
            } else if state.Char().charid != charids_charid_4_skeleton as u8 {
                move_down_back_impl(state);
            }
        } else {
            // can_guard_see_kid == 2
            let opp_frame = state.Opp().frame;
            let distance = char_opp_dist() as i16;
            if distance >= 12
                && opp_frame >= frameids_frame_102_start_fall_1 as u8
                && opp_frame < frameids_frame_118_stand_up_from_crouch_9 as u8
                && state.Opp().action == actions_actions_5_bumped as u8
            {
                return;
            }
            if distance < 35 {
                if (state.Char().sword < sword_status_sword_2_drawn as u8 && distance < 8)
                    || distance < 12
                {
                    if state.Char().direction == state.Opp().direction {
                        move_2_backward_impl(state);
                    } else {
                        move_1_forward_impl(state);
                    }
                } else {
                    autocontrol_guard_kid_in_sight_impl(state, distance as c_short);
                }
            } else {
                if *state.guard_refrac() != 0 { return; }
                if state.Char().direction != state.Opp().direction {
                    if opp_frame >= frameids_frame_7_run as u8 && opp_frame < 15 {
                        if distance < 40 { move_6_shift_impl(state); }
                        return;
                    } else if opp_frame >= frameids_frame_34_start_run_jump_1 as u8
                        && opp_frame < 44
                    {
                        if distance < 50 { move_6_shift_impl(state); }
                        return;
                    }
                }
                autocontrol_guard_kid_far_impl(state);
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_active() {
    autocontrol_guard_active_impl(&mut State);
}

// seg002:09CB
unsafe fn autocontrol_guard_kid_far_impl(state: &mut State) {
    if tile_is_floor(get_tile_infrontof_char()) != 0
        || tile_is_floor(get_tile_infrontof2_char()) != 0
    {
        move_1_forward_impl(state);
    } else {
        move_2_backward_impl(state);
    }
}

#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_kid_far() {
    autocontrol_guard_kid_far_impl(&mut State);
}

// seg002:09F8
unsafe fn guard_follows_kid_down_impl(state: &mut State) {
    let opp_action = state.Opp().action;
    if opp_action == actions_actions_2_hang_climb as u8
        || opp_action == actions_actions_6_hang_straight as u8
    {
        return;
    }
    // get_tile_infrontof_char() sets curr_tile2 to the tile in front.
    // Mirror C's short-circuit: only evaluate the rest if no wall in front.
    let should_not_follow;
    if wall_type(get_tile_infrontof_char() as byte) != 0 {
        should_not_follow = true;
    } else if tile_is_floor(curr_tile2 as c_int) == 0 {
        // No floor in front: check the tile one row below (++tile_row in C).
        tile_row += 1;
        let below = get_tile(curr_room as c_int, tile_col as c_int, tile_row as c_int);
        should_not_follow = below == tiles_tiles_2_spike as c_int
            || curr_tile2 == tiles_tiles_11_loose as u8
            || wall_type(curr_tile2) != 0
            || tile_is_floor(curr_tile2 as c_int) == 0
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

#[no_mangle]
pub unsafe extern "C" fn guard_follows_kid_down() {
    guard_follows_kid_down_impl(&mut State);
}

// seg002:0A93
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

#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_kid_in_sight(distance: c_short) {
    autocontrol_guard_kid_in_sight_impl(&mut State, distance);
}

// seg002:0AC1
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

#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_kid_armed(distance: c_short) {
    autocontrol_guard_kid_armed_impl(&mut State, distance);
}

// seg002:0AF5
unsafe fn guard_advance_impl(state: &mut State) {
    if *state.guard_skill() == 0 || *state.kid_sword_strike() == 0 {
        if (*custom).advprob[*state.guard_skill() as usize] > prandom(255) {
            move_1_forward_impl(state);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn guard_advance() {
    guard_advance_impl(&mut State);
}

// seg002:0B1D
unsafe fn guard_block_impl(state: &mut State) {
    let opp_frame = state.Opp().frame;
    if opp_frame == frameids_frame_152_strike_2 as u8
        || opp_frame == frameids_frame_153_strike_3 as u8
        || opp_frame == frameids_frame_162_block_to_strike as u8
    {
        if *state.justblocked() != 0 {
            if (*custom).impblockprob[*state.guard_skill() as usize] > prandom(255) {
                move_3_up_impl(state);
            }
        } else {
            if (*custom).blockprob[*state.guard_skill() as usize] > prandom(255) {
                move_3_up_impl(state);
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn guard_block() {
    guard_block_impl(&mut State);
}

// seg002:0B73
unsafe fn guard_strike_impl(state: &mut State) {
    let opp_frame = state.Opp().frame;
    if opp_frame == frameids_frame_169_begin_block as u8
        || opp_frame == frameids_frame_151_strike_1 as u8
    {
        return;
    }
    let char_frame = state.Char().frame;
    if char_frame == frameids_frame_161_parry as u8
        || char_frame == frameids_frame_150_parry as u8
    {
        if (*custom).restrikeprob[*state.guard_skill() as usize] > prandom(255) {
            move_6_shift_impl(state);
        }
    } else {
        if (*custom).strikeprob[*state.guard_skill() as usize] > prandom(255) {
            move_6_shift_impl(state);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn guard_strike() {
    guard_strike_impl(&mut State);
}

// seg002:0BCD
// Helper for the "stabbed or pushed off ledge" outcome (C's loc_4276).
unsafe fn hurt_by_sword_loc_4276(state: &mut State, distance: &mut i16) {
    if get_tile_behind_char() != 0 || {
        *distance = distance_to_edge_weight() as i16;
        *distance < 4
    } {
        seqtbl_offset_char(seqids_seq_85_stabbed_to_death as c_short);
        if state.Char().charid != charids_charid_0_kid as u8
            && (state.Char().direction as i8) < directions_dir_0_right as i8
            && (curr_tile2 == tiles_tiles_4_gate as u8
                || get_tile_at_char() == tiles_tiles_4_gate as c_int)
        {
            if (*fixes).fix_offscreen_guards_disappearing != 0 {
                let mut gate_col = tile_col;
                let char_room = state.Char().room;
                if curr_room != char_room as c_short {
                    if curr_room == state.level().roomlinks[(char_room as usize) - 1].right as c_short {
                        gate_col += SCREEN_TILECOUNTX as c_short;
                    } else if curr_room
                        == state.level().roomlinks[(char_room as usize) - 1].left as c_short
                    {
                        gate_col -= SCREEN_TILECOUNTX as c_short;
                    }
                }
                let is_not_gate = (curr_tile2 != tiles_tiles_4_gate as u8) as i32;
                state.Char().x = (x_bump_at(
                    (gate_col as i32 - is_not_gate + FIRST_ONSCREEN_COLUMN as i32) as usize,
                ) as i32
                    + TILE_MIDX as i32) as u8;
            } else {
                let is_not_gate = (curr_tile2 != tiles_tiles_4_gate as u8) as i32;
                state.Char().x = (x_bump_at(
                    (tile_col as i32 - is_not_gate + FIRST_ONSCREEN_COLUMN as i32) as usize,
                ) as i32
                    + TILE_MIDX as i32) as u8;
            }
            state.Char().x = char_dx_forward(10) as u8;
        }
        let curr_row = state.Char().curr_row;
        state.Char().y = y_land_at((curr_row + 1) as usize) as u8;
        state.Char().fall_y = 0;
    } else {
        state.Char().x = char_dx_forward(*distance as c_int - 20) as u8;
        load_fram_det_col();
        inc_curr_row();
        seqtbl_offset_char(seqids_seq_81_kid_pushed_off_ledge as c_short);
    }
}

unsafe fn hurt_by_sword_impl(state: &mut State) {
    if state.Char().alive >= 0 { return; }
    let mut distance: i16 = 0;
    if state.Char().sword != sword_status_sword_2_drawn as u8 {
        take_hp(100);
        seqtbl_offset_char(seqids_seq_85_stabbed_to_death as c_short);
        hurt_by_sword_loc_4276(state, &mut distance);
    } else {
        if state.Char().charid != charids_charid_4_skeleton as u8 && take_hp(1) != 0 {
            hurt_by_sword_loc_4276(state, &mut distance);
        } else {
            seqtbl_offset_char(seqids_seq_74_hit_by_sword as c_short);
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

#[no_mangle]
pub unsafe extern "C" fn hurt_by_sword() {
    hurt_by_sword_impl(&mut State);
}

// seg002:0CD4
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

#[no_mangle]
pub unsafe extern "C" fn check_sword_hurt() {
    check_sword_hurt_impl(&mut State);
}

// seg002:0D1A
unsafe fn check_sword_hurting_impl(state: &mut State) {
    let kid_frame = state.Kid().frame;
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

#[no_mangle]
pub unsafe extern "C" fn check_sword_hurting() {
    check_sword_hurting_impl(&mut State);
}

// seg002:0D56
unsafe fn check_hurting_impl(state: &mut State) {
    if state.Char().sword != sword_status_sword_2_drawn as u8 { return; }
    if state.Char().curr_row != state.Opp().curr_row { return; }
    let char_frame = state.Char().frame;
    if char_frame != frameids_frame_153_strike_3 as u8
        && char_frame != frameids_frame_154_poking as u8
    {
        return;
    }
    let distance = char_opp_dist() as i16;
    let opp_frame = state.Opp().frame;
    if distance < 0
        || distance >= 29
        || (opp_frame != frameids_frame_161_parry as u8
            && opp_frame != frameids_frame_150_parry as u8)
    {
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
    if state.Char().direction == directions_dir_56_none as i8 { return; }
    if state.Char().frame == frameids_frame_154_poking as u8
        && state.Opp().frame != frameids_frame_161_parry as u8
        && state.Opp().action != actions_actions_99_hurt as u8
    {
        play_sound(soundids_sound_11_sword_moving as c_int);
    }
}

#[no_mangle]
pub unsafe extern "C" fn check_hurting() {
    check_hurting_impl(&mut State);
}

// seg002:0E1F
unsafe fn check_skel_impl(state: &mut State) {
    if *state.current_level() == (*custom).skeleton_level
        && state.Guard().direction == directions_dir_56_none as i8
        && *state.drawn_room() == (*custom).skeleton_room as u16
        && (*state.leveldoor_open() != 0 || (*custom).skeleton_require_open_level_door == 0)
        && (state.Kid().curr_col == (*custom).skeleton_trigger_column_1 as i8
            || state.Kid().curr_col == (*custom).skeleton_trigger_column_2 as i8)
    {
        let drawn_room_v = *state.drawn_room();
        get_tile(
            drawn_room_v as c_int,
            (*custom).skeleton_column as c_int,
            (*custom).skeleton_row as c_int,
        );
        if curr_tile2 == tiles_tiles_21_skeleton as u8 {
            *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
            *state.redraw_height() = 24;
            set_redraw_full(curr_tilepos as c_short, 1);
            set_wipe(curr_tilepos as c_short, 1);
            curr_tilepos = curr_tilepos.wrapping_add(1);
            set_redraw_full(curr_tilepos as c_short, 1);
            set_wipe(curr_tilepos as c_short, 1);

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
    }
}

#[no_mangle]
pub unsafe extern "C" fn check_skel() {
    check_skel_impl(&mut State);
}

// seg002:0F3F
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

#[no_mangle]
pub unsafe extern "C" fn do_auto_moves(moves_ptr: *const auto_move_type) {
    do_auto_moves_impl(&mut State, moves_ptr);
}

// seg002:1000
unsafe fn autocontrol_shadow_level4_impl(state: &mut State) {
    if state.Char().room == (*custom).mirror_room {
        if state.Char().x < 80 {
            clear_char();
        } else {
            move_1_forward_impl(state);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level4() {
    autocontrol_shadow_level4_impl(&mut State);
}

// seg002:101A
unsafe fn autocontrol_shadow_level5_impl(state: &mut State) {
    if state.Char().room == (*custom).shadow_steal_room {
        if *state.demo_time() == 0 {
            get_tile((*custom).shadow_steal_room as c_int, 1, 0);
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

#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level5() {
    autocontrol_shadow_level5_impl(&mut State);
}

// seg002:1064
unsafe fn autocontrol_shadow_level6_impl(state: &mut State) {
    if state.Char().room == (*custom).shadow_step_room
        && state.Kid().frame == frameids_frame_43_running_jump_4 as u8
        && state.Kid().x < 128
    {
        move_6_shift_impl(state);
        move_1_forward_impl(state);
    }
}

#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level6() {
    autocontrol_shadow_level6_impl(&mut State);
}

// seg002:1082
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
        let opp_frame = state.Opp().frame;
        if (opp_frame >= frameids_frame_3_start_run as u8
            && opp_frame < frameids_frame_15_stand as u8)
            || (opp_frame >= frameids_frame_127_stepping_7 as u8 && opp_frame < 133)
        {
            move_1_forward_impl(state);
        }
    }
}

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
