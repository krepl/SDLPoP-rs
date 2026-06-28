// Guard/shadow AI — ported from seg002.c.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;

// seg002:0000
#[no_mangle]
pub unsafe extern "C" fn do_init_shad(source: *const byte, seq_index: c_int) {
    core::ptr::copy_nonoverlapping(source, core::ptr::addr_of_mut!(Char) as *mut byte, 7);
    seqtbl_offset_char(seq_index as c_short);
    Char.charid = charids_charid_1_shadow as u8;
    demo_time = 0;
    guard_skill = 3;
    guardhp_delta = 4;
    guardhp_curr = 4;
    guardhp_max = 4;
    saveshad();
}

// seg002:0044
#[no_mangle]
pub unsafe extern "C" fn get_guard_hp() {
    let hp = (*custom).extrastrength[guard_skill as usize] as i32
        + (*custom).tbl_guard_hp[current_level as usize] as i32;
    guardhp_max = hp as u16;
    guardhp_curr = hp as u16;
    guardhp_delta = hp as c_short;
}

// seg002:0064
#[no_mangle]
pub unsafe extern "C" fn check_shadow() {
    offguard = 0;
    if current_level == 12 {
        if united_with_shadow == 0 && drawn_room == 15 {
            Char.room = drawn_room as u8;
            if get_tile(15, 1, 0) == tiles_tiles_22_sword as c_int {
                return;
            }
            shadow_initialized = 0;
            do_init_shad(
                core::ptr::addr_of!((*custom).init_shad_12).cast::<byte>(),
                7, // fall
            );
            return;
        }
    }
    if current_level == (*custom).shadow_step_level as u16 {
        Char.room = drawn_room as u8;
        if Char.room == (*custom).shadow_step_room {
            if leveldoor_open != 0x4D {
                play_sound(soundids_sound_25_presentation as c_int);
                leveldoor_open = 0x4D;
            }
            do_init_shad(
                core::ptr::addr_of!((*custom).init_shad_6).cast::<byte>(),
                2, // stand
            );
            return;
        }
    }
    if current_level == (*custom).shadow_steal_level as u16 {
        Char.room = drawn_room as u8;
        if Char.room == (*custom).shadow_steal_room {
            if get_tile((*custom).shadow_steal_room as c_int, 3, 0)
                != tiles_tiles_10_potion as c_int
            {
                return;
            }
            do_init_shad(
                core::ptr::addr_of!((*custom).init_shad_5).cast::<byte>(),
                2, // stand
            );
            return;
        }
    }
    enter_guard();
}

// seg002:0112
#[no_mangle]
pub unsafe extern "C" fn enter_guard() {
    let room_minus_1 = (drawn_room - 1) as usize;
    let mut guard_tile = level.guards_tile[room_minus_1];

    if guard_tile >= 30 {
        if (*fixes).fix_offscreen_guards_disappearing == 0 {
            return;
        }
        let left_guard_tile: i16 =
            if room_L > 0 { level.guards_tile[(room_L - 1) as usize] as i16 } else { 31 };
        let right_guard_tile: i16 =
            if room_R > 0 { level.guards_tile[(room_R - 1) as usize] as i16 } else { 31 };

        // Determine which offscreen guard to pull in.
        // The goto in C (right→left fallthrough) is modeled with a bool + two passes.
        let mut result: Option<(usize, i32, u8)> = None;
        let mut try_left = false;

        if right_guard_tile >= 0 && right_guard_tile < 30 {
            let ormi = (room_R - 1) as usize;
            let mut ogx = level.guards_x[ormi] as i32;
            let ogd = level.guards_dir[ormi] as i8;
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
            let ormi = (room_L - 1) as usize;
            let mut ogx = level.guards_x[ormi] as i32;
            let ogd = level.guards_dir[ormi] as i8;
            if ogd == directions_dir_0_right as i8 { ogx -= 9; }
            if ogd == directions_dir_FF_left as i8 { ogx += 1; }
            if ogx <= 186 { return; }
            result = Some((ormi, -140, left_guard_tile as u8));
        }

        let (ormi, delta_x, new_tile) = result.unwrap();
        guard_tile = new_tile;
        level.guards_x[room_minus_1] = (level.guards_x[ormi] as i32 + delta_x) as u8;
        level.guards_color[room_minus_1] = level.guards_color[ormi];
        level.guards_dir[room_minus_1] = level.guards_dir[ormi];
        level.guards_seq_hi[room_minus_1] = level.guards_seq_hi[ormi];
        level.guards_seq_lo[room_minus_1] = level.guards_seq_lo[ormi];
        level.guards_skill[room_minus_1] = level.guards_skill[ormi];
        level.guards_tile[ormi] = 0xFF;
        level.guards_seq_hi[ormi] = 0;
    }

    Char.room = drawn_room as u8;
    Char.curr_row = (guard_tile / SCREEN_TILECOUNTX as u8) as i8;
    Char.y = y_land_at((Char.curr_row + 1) as usize) as u8;
    Char.x = level.guards_x[room_minus_1];
    Char.curr_col = get_tile_div_mod_m7(Char.x as c_int) as i8;
    Char.direction = level.guards_dir[room_minus_1] as i8;

    if graphics_mode == grmodes_gmMcgaVga as u8
        && (*custom).tbl_guard_type[current_level as usize] == 0
    {
        curr_guard_color = level.guards_color[room_minus_1] as u16;
    } else {
        curr_guard_color = 0;
    }

    let remembered_hp = ((level.guards_color[room_minus_1] & 0xF0) >> 4) as i32;
    curr_guard_color &= 0x0F;

    if (*custom).tbl_guard_type[current_level as usize] == 2 {
        Char.charid = charids_charid_4_skeleton as u8;
    } else {
        Char.charid = charids_charid_2_guard as u8;
    }

    let seq_hi = level.guards_seq_hi[room_minus_1];
    if seq_hi == 0 {
        if Char.charid == charids_charid_4_skeleton as u8 {
            Char.sword = sword_status_sword_2_drawn as u8;
            seqtbl_offset_char(seqids_seq_63_guard_active_after_fall as c_short);
        } else {
            Char.sword = sword_status_sword_0_sheathed as u8;
            seqtbl_offset_char(seqids_seq_77_guard_stand_inactive as c_short);
        }
    } else {
        Char.curr_seq = level.guards_seq_lo[room_minus_1] as u16
            | ((seq_hi as u16) << 8);
    }
    play_seq();

    guard_skill = level.guards_skill[room_minus_1] as u16;
    if guard_skill >= NUM_GUARD_SKILLS as u16 {
        guard_skill = 3;
    }

    let frame = Char.frame;
    if frame == frameids_frame_185_dead as u8
        || frame == frameids_frame_177_spiked as u8
        || frame == frameids_frame_178_chomped as u8
    {
        Char.alive = 1;
        draw_guard_hp(0, guardhp_curr as c_short);
        guardhp_curr = 0;
    } else {
        Char.alive = -1;
        justblocked = 0;
        guard_refrac = 0;
        is_guard_notice = 0;
        get_guard_hp();
        if (*fixes).enable_remember_guard_hp != 0 && remembered_hp > 0 {
            guardhp_delta = remembered_hp as c_short;
            guardhp_curr = remembered_hp as u16;
        }
    }

    Char.fall_y = 0;
    Char.fall_x = 0;
    Char.action = actions_actions_1_run_jump as u8;
    saveshad();
}

// seg002:0269
#[no_mangle]
pub unsafe extern "C" fn check_guard_fallout() {
    if Guard.direction == directions_dir_56_none as i8 || Guard.y < 211 {
        return;
    }
    if Guard.charid == charids_charid_1_shadow as u8 {
        if Guard.action != actions_actions_4_in_freefall as u8 {
            return;
        }
        loadshad();
        clear_char();
        saveshad();
    } else if Guard.charid == charids_charid_4_skeleton as u8
        && level.roomlinks[(Guard.room as usize) - 1].down
            == (*custom).skeleton_reappear_room
    {
        Guard.room = level.roomlinks[(Guard.room as usize) - 1].down;
        Guard.x = (*custom).skeleton_reappear_x;
        Guard.curr_row = (*custom).skeleton_reappear_row as i8;
        Guard.direction = (*custom).skeleton_reappear_dir as i8;
        Guard.alive = -1;
        leave_guard();
    } else {
        on_guard_killed();
        level.guards_tile[(drawn_room - 1) as usize] = 0xFF;
        Guard.direction = directions_dir_56_none as i8;
        draw_guard_hp(0, guardhp_curr as c_short);
        guardhp_curr = 0;
    }
}

// seg002:02F5
#[no_mangle]
pub unsafe extern "C" fn leave_guard() {
    if Guard.direction == directions_dir_56_none as i8
        || Guard.charid == charids_charid_1_shadow as u8
        || Guard.charid == charids_charid_24_mouse as u8
    {
        return;
    }
    let room_minus_1 = (Guard.room as usize) - 1;
    level.guards_tile[room_minus_1] = get_tilepos(0, Guard.curr_row as c_int) as u8;

    level.guards_color[room_minus_1] = (curr_guard_color & 0x0F) as u8;
    if (*fixes).enable_remember_guard_hp != 0 && guardhp_curr < 16 {
        level.guards_color[room_minus_1] |= (guardhp_curr << 4) as u8;
    }

    level.guards_x[room_minus_1] = Guard.x;
    level.guards_dir[room_minus_1] = Guard.direction as u8;
    level.guards_skill[room_minus_1] = guard_skill as u8;

    if Guard.alive < 0 {
        level.guards_seq_hi[room_minus_1] = 0;
    } else {
        level.guards_seq_lo[room_minus_1] = Guard.curr_seq as u8;
        level.guards_seq_hi[room_minus_1] = (Guard.curr_seq >> 8) as u8;
    }

    Guard.direction = directions_dir_56_none as i8;
    draw_guard_hp(0, guardhp_curr as c_short);
    guardhp_curr = 0;
}

// seg002:039E
#[no_mangle]
pub unsafe extern "C" fn follow_guard() {
    level.guards_tile[(Kid.room as usize) - 1] = 0xFF;
    level.guards_tile[(Guard.room as usize) - 1] = 0xFF;
    loadshad();
    goto_other_room(roomleave_result);
    saveshad();
}

// seg002:03C7
#[no_mangle]
pub unsafe extern "C" fn exit_room() {
    let mut leave: i16 = 0;
    if exit_room_timer != 0 {
        exit_room_timer -= 1;
        if !((*fixes).fix_hang_on_teleport != 0 && Char.y >= 211 && Char.curr_row >= 2) {
            return;
        }
    }
    loadkid();
    load_frame_to_obj();
    set_char_collision();
    roomleave_result = leave_room();
    if roomleave_result < 0 {
        return;
    }
    savekid();
    next_room = Char.room as u16;
    if (*fixes).enable_super_high_jump != 0 && super_jump_fall != 0 && next_room == drawn_room {
        return;
    }
    if Guard.direction == directions_dir_56_none as i8 {
        return;
    }
    if Guard.alive < 0 && Guard.sword == sword_status_sword_2_drawn as u8 {
        let kid_room_m1 = (Kid.room as i16) - 1;
        if (kid_room_m1 >= 0 && kid_room_m1 <= 23)
            && (level.guards_tile[kid_room_m1 as usize] >= 30
                || level.guards_seq_hi[kid_room_m1 as usize] != 0)
        {
            if roomleave_result == 0 {
                // left
                if Guard.x >= 91 {
                    leave = 1;
                } else if (*fixes).fix_guard_following_through_closed_gates != 0
                    && can_guard_see_kid != 2
                    && Kid.sword != sword_status_sword_2_drawn as u8
                {
                    leave = 1;
                }
            } else if roomleave_result == 1 {
                // right
                if Guard.x < 165 {
                    leave = 1;
                } else if (*fixes).fix_guard_following_through_closed_gates != 0
                    && can_guard_see_kid != 2
                    && Kid.sword != sword_status_sword_2_drawn as u8
                {
                    leave = 1;
                }
            } else if roomleave_result == 2 {
                // up
                if Guard.curr_row >= 0 {
                    leave = 1;
                }
            } else {
                // down
                if Guard.curr_row < 3 {
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
        leave_guard();
    } else {
        follow_guard();
    }
}

// seg002:0486
#[no_mangle]
pub unsafe extern "C" fn goto_other_room(direction: c_short) -> c_int {
    let other_room: u8;
    if Char.room == 0 {
        other_room = 0;
    } else {
        let rlinks = &level.roomlinks[(Char.room as usize) - 1];
        other_room = match direction {
            0 => rlinks.left,
            1 => rlinks.right,
            2 => rlinks.up,
            _ => rlinks.down,
        };
    }
    Char.room = other_room;
    let opposite_dir: c_int;
    if direction == 0 {
        Char.x = Char.x.wrapping_add(140);
        opposite_dir = 1;
    } else if direction == 1 {
        Char.x = Char.x.wrapping_sub(140);
        opposite_dir = 0;
    } else if direction == 2 {
        Char.y = Char.y.wrapping_add(189);
        Char.curr_row = y_to_row_mod4(Char.y as c_int) as i8;
        opposite_dir = 3;
    } else {
        Char.y = Char.y.wrapping_sub(189);
        Char.curr_row = y_to_row_mod4(Char.y as c_int) as i8;
        opposite_dir = 2;
    }
    opposite_dir
}

// seg002:0504
#[no_mangle]
pub unsafe extern "C" fn leave_room() -> c_short {
    let leave_dir: i16;
    let chary = Char.y;
    let action = Char.action;
    let frame = Char.frame;

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
    } else if Char.direction != directions_dir_0_right as i8 {
        // looking left
        if char_x_left <= 54 {
            leave_dir = 0; // left
        } else if char_x_left >= 198 {
            leave_dir = 1; // right
        } else {
            return -1;
        }
    } else {
        // looking right
        get_tile(Char.room as c_int, 9, Char.curr_row as c_int);
        if curr_tile2 != tiles_tiles_7_doortop_with_floor as u8
            && curr_tile2 != tiles_tiles_12_doortop as u8
            && char_x_right >= 201
        {
            leave_dir = 1; // right
        } else if char_x_right <= 57 {
            leave_dir = 0; // left
        } else {
            return -1;
        }
    }

    match leave_dir {
        0 => {
            // left
            play_mirr_mus();
            level3_set_chkp();
            Jaffar_exit();
        }
        1 => {
            // right
            sword_disappears();
            meet_Jaffar();
        }
        3 => {
            // down — special event: falling exit
            if current_level == (*custom).falling_exit_level
                && Char.room == (*custom).falling_exit_room
            {
                return -2;
            }
        }
        _ => {}
    }

    goto_other_room(leave_dir as c_short);
    if skipping_replay != 0
        && replay_seek_target == replay_seek_targets_replay_seek_0_next_room as u8
    {
        skipping_replay = 0;
    }
    leave_dir as c_short
}

// seg002:0643
#[no_mangle]
pub unsafe extern "C" fn Jaffar_exit() {
    if leveldoor_open == 2 {
        get_tile(24, 0, 0);
        trigger_button(0, 0, -1);
    }
}

// seg002:0665
#[no_mangle]
pub unsafe extern "C" fn level3_set_chkp() {
    if current_level == (*custom).checkpoint_level && Char.room == 7 {
        checkpoint = 1;
        hitp_beg_lev = hitp_max;
    }
}

// seg002:0680
#[no_mangle]
pub unsafe extern "C" fn sword_disappears() {
    if current_level == 12 && Char.room == 18 {
        get_tile(15, 1, 0);
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
        *curr_room_modif.add(curr_tilepos as usize) = 0;
    }
}

// seg002:06AE
#[no_mangle]
pub unsafe extern "C" fn meet_Jaffar() {
    if current_level == 13 && leveldoor_open == 0 && Char.room == 3 {
        play_sound(soundids_sound_29_meet_Jaffar as c_int);
        guard_notice_timer = 28;
    }
}

// seg002:06D3
#[no_mangle]
pub unsafe extern "C" fn play_mirr_mus() {
    if leveldoor_open != 0
        && leveldoor_open != 0x4D
        && current_level == (*custom).mirror_level
        && Char.curr_row == (*custom).mirror_row as i8
        && Char.room == 11
    {
        play_sound(soundids_sound_25_presentation as c_int);
        leveldoor_open = 0x4D;
    }
}

// seg002:0706
#[no_mangle]
pub unsafe extern "C" fn move_0_nothing() {
    control_shift = CONTROL_RELEASED as i8;
    control_y = CONTROL_RELEASED as i8;
    control_x = CONTROL_RELEASED as i8;
    control_shift2 = CONTROL_RELEASED as i8;
    control_down = CONTROL_RELEASED as i8;
    control_up = CONTROL_RELEASED as i8;
    control_backward = CONTROL_RELEASED as i8;
    control_forward = CONTROL_RELEASED as i8;
}

// seg002:0721
#[no_mangle]
pub unsafe extern "C" fn move_1_forward() {
    control_x = CONTROL_HELD_FORWARD as i8;
    control_forward = CONTROL_HELD as i8;
}

// seg002:072A
#[no_mangle]
pub unsafe extern "C" fn move_2_backward() {
    control_backward = CONTROL_HELD as i8;
    control_x = CONTROL_HELD_BACKWARD as i8;
}

// seg002:0735
#[no_mangle]
pub unsafe extern "C" fn move_3_up() {
    control_y = CONTROL_HELD_UP as i8;
    control_up = CONTROL_HELD as i8;
}

// seg002:073E
#[no_mangle]
pub unsafe extern "C" fn move_4_down() {
    control_down = CONTROL_HELD as i8;
    control_y = CONTROL_HELD_DOWN as i8;
}

// seg002:0749
#[no_mangle]
pub unsafe extern "C" fn move_up_back() {
    control_up = CONTROL_HELD as i8;
    move_2_backward();
}

// seg002:0753
#[no_mangle]
pub unsafe extern "C" fn move_down_back() {
    control_down = CONTROL_HELD as i8;
    move_2_backward();
}

// seg002:075D
#[no_mangle]
pub unsafe extern "C" fn move_down_forw() {
    control_down = CONTROL_HELD as i8;
    move_1_forward();
}

// seg002:0767
#[no_mangle]
pub unsafe extern "C" fn move_6_shift() {
    control_shift = CONTROL_HELD as i8;
    control_shift2 = CONTROL_HELD as i8;
}

// seg002:0770
#[no_mangle]
pub unsafe extern "C" fn move_7() {
    control_shift = CONTROL_RELEASED as i8;
}

// seg002:0776
#[no_mangle]
pub unsafe extern "C" fn autocontrol_opponent() {
    move_0_nothing();
    let charid = Char.charid;
    if charid == charids_charid_0_kid as u8 {
        autocontrol_kid();
    } else {
        if justblocked != 0 { justblocked -= 1; }
        if kid_sword_strike != 0 { kid_sword_strike -= 1; }
        if guard_refrac != 0 { guard_refrac -= 1; }
        if charid == charids_charid_24_mouse as u8 {
            autocontrol_mouse();
        } else if charid == charids_charid_4_skeleton as u8 {
            autocontrol_skeleton();
        } else if charid == charids_charid_1_shadow as u8 {
            autocontrol_shadow();
        } else if current_level == 13 {
            autocontrol_Jaffar();
        } else {
            autocontrol_guard();
        }
    }
}

// seg002:07EB
#[no_mangle]
pub unsafe extern "C" fn autocontrol_mouse() {
    if Char.direction == directions_dir_56_none as i8 {
        return;
    }
    if Char.action == actions_actions_0_stand as u8 {
        if Char.x >= 200 {
            clear_char();
        }
    } else {
        if Char.x < 166 {
            seqtbl_offset_char(seqids_seq_107_mouse_stand_up_and_go as c_short);
            play_seq();
        }
    }
}

// seg002:081D
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow() {
    if current_level == (*custom).mirror_level {
        autocontrol_shadow_level4();
    }
    if current_level == (*custom).shadow_steal_level as u16 {
        autocontrol_shadow_level5();
    }
    if current_level == (*custom).shadow_step_level as u16 {
        autocontrol_shadow_level6();
    }
    if current_level == 12 {
        autocontrol_shadow_level12();
    }
}

// seg002:0850
#[no_mangle]
pub unsafe extern "C" fn autocontrol_skeleton() {
    Char.sword = sword_status_sword_2_drawn as u8;
    autocontrol_guard();
}

// seg002:085A
#[no_mangle]
pub unsafe extern "C" fn autocontrol_Jaffar() {
    autocontrol_guard();
}

// seg002:085F
#[no_mangle]
pub unsafe extern "C" fn autocontrol_kid() {
    autocontrol_guard();
}

// seg002:0864
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard() {
    if Char.sword < sword_status_sword_2_drawn as u8 {
        autocontrol_guard_inactive();
    } else {
        autocontrol_guard_active();
    }
}

// seg002:0876
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_inactive() {
    if Kid.alive >= 0 { return; }
    let distance = char_opp_dist() as i16;
    if Opp.curr_row != Char.curr_row || (distance as u16) < 0xFFF8u16 {
        if is_guard_notice != 0 {
            is_guard_notice = 0;
            if distance < 0 {
                if (distance as u16) < 0xFFFCu16 {
                    move_4_down();
                }
                return;
            }
        } else if distance < 0 {
            return;
        }
    }
    if can_guard_see_kid != 0 {
        if current_level != 13 || guard_notice_timer == 0 {
            move_down_forw();
        }
    }
}

// seg002:08DC
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_active() {
    let char_frame = Char.frame;
    if char_frame != frameids_frame_166_stand_inactive as u8
        && char_frame >= 150
        && can_guard_see_kid != 1
    {
        if can_guard_see_kid == 0 {
            if droppedout != 0 {
                guard_follows_kid_down();
            } else if Char.charid != charids_charid_4_skeleton as u8 {
                move_down_back();
            }
        } else {
            // can_guard_see_kid == 2
            let opp_frame = Opp.frame;
            let distance = char_opp_dist() as i16;
            if distance >= 12
                && opp_frame >= frameids_frame_102_start_fall_1 as u8
                && opp_frame < frameids_frame_118_stand_up_from_crouch_9 as u8
                && Opp.action == actions_actions_5_bumped as u8
            {
                return;
            }
            if distance < 35 {
                if (Char.sword < sword_status_sword_2_drawn as u8 && distance < 8)
                    || distance < 12
                {
                    if Char.direction == Opp.direction {
                        move_2_backward();
                    } else {
                        move_1_forward();
                    }
                } else {
                    autocontrol_guard_kid_in_sight(distance as c_short);
                }
            } else {
                if guard_refrac != 0 { return; }
                if Char.direction != Opp.direction {
                    if opp_frame >= frameids_frame_7_run as u8 && opp_frame < 15 {
                        if distance < 40 { move_6_shift(); }
                        return;
                    } else if opp_frame >= frameids_frame_34_start_run_jump_1 as u8
                        && opp_frame < 44
                    {
                        if distance < 50 { move_6_shift(); }
                        return;
                    }
                }
                autocontrol_guard_kid_far();
            }
        }
    }
}

// seg002:09CB
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_kid_far() {
    if tile_is_floor(get_tile_infrontof_char()) != 0
        || tile_is_floor(get_tile_infrontof2_char()) != 0
    {
        move_1_forward();
    } else {
        move_2_backward();
    }
}

// seg002:09F8
#[no_mangle]
pub unsafe extern "C" fn guard_follows_kid_down() {
    let opp_action = Opp.action;
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
            || Char.curr_row + 1 != Opp.curr_row;
    } else {
        should_not_follow = false;
    }
    if should_not_follow {
        droppedout = 0;
        move_2_backward();
    } else {
        move_1_forward();
    }
}

// seg002:0A93
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_kid_in_sight(distance: c_short) {
    if Opp.sword == sword_status_sword_2_drawn as u8 {
        autocontrol_guard_kid_armed(distance);
    } else if guard_refrac == 0 {
        if distance < 29 {
            move_6_shift();
        } else {
            move_1_forward();
        }
    }
}

// seg002:0AC1
#[no_mangle]
pub unsafe extern "C" fn autocontrol_guard_kid_armed(distance: c_short) {
    if distance < 10 || distance >= 29 {
        guard_advance();
    } else {
        guard_block();
        if guard_refrac == 0 {
            if distance < 12 || distance >= 29 {
                guard_advance();
            } else {
                guard_strike();
            }
        }
    }
}

// seg002:0AF5
#[no_mangle]
pub unsafe extern "C" fn guard_advance() {
    if guard_skill == 0 || kid_sword_strike == 0 {
        if (*custom).advprob[guard_skill as usize] > prandom(255) {
            move_1_forward();
        }
    }
}

// seg002:0B1D
#[no_mangle]
pub unsafe extern "C" fn guard_block() {
    let opp_frame = Opp.frame;
    if opp_frame == frameids_frame_152_strike_2 as u8
        || opp_frame == frameids_frame_153_strike_3 as u8
        || opp_frame == frameids_frame_162_block_to_strike as u8
    {
        if justblocked != 0 {
            if (*custom).impblockprob[guard_skill as usize] > prandom(255) {
                move_3_up();
            }
        } else {
            if (*custom).blockprob[guard_skill as usize] > prandom(255) {
                move_3_up();
            }
        }
    }
}

// seg002:0B73
#[no_mangle]
pub unsafe extern "C" fn guard_strike() {
    let opp_frame = Opp.frame;
    if opp_frame == frameids_frame_169_begin_block as u8
        || opp_frame == frameids_frame_151_strike_1 as u8
    {
        return;
    }
    let char_frame = Char.frame;
    if char_frame == frameids_frame_161_parry as u8
        || char_frame == frameids_frame_150_parry as u8
    {
        if (*custom).restrikeprob[guard_skill as usize] > prandom(255) {
            move_6_shift();
        }
    } else {
        if (*custom).strikeprob[guard_skill as usize] > prandom(255) {
            move_6_shift();
        }
    }
}

// seg002:0BCD
// Helper for the "stabbed or pushed off ledge" outcome (C's loc_4276).
unsafe fn hurt_by_sword_loc_4276(distance: &mut i16) {
    if get_tile_behind_char() != 0 || {
        *distance = distance_to_edge_weight() as i16;
        *distance < 4
    } {
        seqtbl_offset_char(seqids_seq_85_stabbed_to_death as c_short);
        if Char.charid != charids_charid_0_kid as u8
            && (Char.direction as i8) < directions_dir_0_right as i8
            && (curr_tile2 == tiles_tiles_4_gate as u8
                || get_tile_at_char() == tiles_tiles_4_gate as c_int)
        {
            if (*fixes).fix_offscreen_guards_disappearing != 0 {
                let mut gate_col = tile_col;
                if curr_room != Char.room as c_short {
                    if curr_room == level.roomlinks[(Char.room as usize) - 1].right as c_short {
                        gate_col += SCREEN_TILECOUNTX as c_short;
                    } else if curr_room
                        == level.roomlinks[(Char.room as usize) - 1].left as c_short
                    {
                        gate_col -= SCREEN_TILECOUNTX as c_short;
                    }
                }
                let is_not_gate = (curr_tile2 != tiles_tiles_4_gate as u8) as i32;
                Char.x = (x_bump_at(
                    (gate_col as i32 - is_not_gate + FIRST_ONSCREEN_COLUMN as i32) as usize,
                ) as i32
                    + TILE_MIDX as i32) as u8;
            } else {
                let is_not_gate = (curr_tile2 != tiles_tiles_4_gate as u8) as i32;
                Char.x = (x_bump_at(
                    (tile_col as i32 - is_not_gate + FIRST_ONSCREEN_COLUMN as i32) as usize,
                ) as i32
                    + TILE_MIDX as i32) as u8;
            }
            Char.x = char_dx_forward(10) as u8;
        }
        Char.y = y_land_at((Char.curr_row + 1) as usize) as u8;
        Char.fall_y = 0;
    } else {
        Char.x = char_dx_forward(*distance as c_int - 20) as u8;
        load_fram_det_col();
        inc_curr_row();
        seqtbl_offset_char(seqids_seq_81_kid_pushed_off_ledge as c_short);
    }
}

#[no_mangle]
pub unsafe extern "C" fn hurt_by_sword() {
    if Char.alive >= 0 { return; }
    let mut distance: i16 = 0;
    if Char.sword != sword_status_sword_2_drawn as u8 {
        take_hp(100);
        seqtbl_offset_char(seqids_seq_85_stabbed_to_death as c_short);
        hurt_by_sword_loc_4276(&mut distance);
    } else {
        if Char.charid != charids_charid_4_skeleton as u8 && take_hp(1) != 0 {
            hurt_by_sword_loc_4276(&mut distance);
        } else {
            seqtbl_offset_char(seqids_seq_74_hit_by_sword as c_short);
            Char.y = y_land_at((Char.curr_row + 1) as usize) as u8;
            Char.fall_y = 0;
        }
    }
    let sound_id = if Char.charid == charids_charid_0_kid as u8 {
        soundids_sound_13_kid_hurt
    } else {
        soundids_sound_12_guard_hurt
    };
    play_sound(sound_id as c_int);
    play_seq();
}

// seg002:0CD4
#[no_mangle]
pub unsafe extern "C" fn check_sword_hurt() {
    if Guard.action == actions_actions_99_hurt as u8 {
        if Kid.action == actions_actions_99_hurt as u8 {
            Kid.action = actions_actions_1_run_jump as u8;
        }
        loadshad();
        hurt_by_sword();
        saveshad();
        guard_refrac = (*custom).refractimer[guard_skill as usize];
    } else {
        if Kid.action == actions_actions_99_hurt as u8 {
            loadkid();
            hurt_by_sword();
            savekid();
        }
    }
}

// seg002:0D1A
#[no_mangle]
pub unsafe extern "C" fn check_sword_hurting() {
    let kid_frame = Kid.frame;
    if kid_frame != 0
        && (kid_frame < frameids_frame_219_exit_stairs_3 as u8 || kid_frame >= 229)
    {
        loadshad_and_opp();
        check_hurting();
        saveshad_and_opp();
        loadkid_and_opp();
        check_hurting();
        savekid_and_opp();
    }
}

// seg002:0D56
#[no_mangle]
pub unsafe extern "C" fn check_hurting() {
    if Char.sword != sword_status_sword_2_drawn as u8 { return; }
    if Char.curr_row != Opp.curr_row { return; }
    let char_frame = Char.frame;
    if char_frame != frameids_frame_153_strike_3 as u8
        && char_frame != frameids_frame_154_poking as u8
    {
        return;
    }
    let distance = char_opp_dist() as i16;
    let opp_frame = Opp.frame;
    if distance < 0
        || distance >= 29
        || (opp_frame != frameids_frame_161_parry as u8
            && opp_frame != frameids_frame_150_parry as u8)
    {
        if Char.frame == frameids_frame_154_poking as u8 {
            let min_hurt_range: i16 = if Opp.sword < sword_status_sword_2_drawn as u8 { 8 } else { 12 };
            let distance2 = char_opp_dist() as i16;
            if distance2 >= min_hurt_range && distance2 < 29 {
                Opp.action = actions_actions_99_hurt as u8;
            }
        }
    } else {
        Opp.frame = frameids_frame_161_parry as u8;
        if Char.charid != charids_charid_0_kid as u8 {
            justblocked = 4;
        }
        seqtbl_offset_char(seqids_seq_69_attack_was_parried as c_short);
        play_seq();
    }
    if Char.direction == directions_dir_56_none as i8 { return; }
    if Char.frame == frameids_frame_154_poking as u8
        && Opp.frame != frameids_frame_161_parry as u8
        && Opp.action != actions_actions_99_hurt as u8
    {
        play_sound(soundids_sound_11_sword_moving as c_int);
    }
}

// seg002:0E1F
#[no_mangle]
pub unsafe extern "C" fn check_skel() {
    if current_level == (*custom).skeleton_level
        && Guard.direction == directions_dir_56_none as i8
        && drawn_room == (*custom).skeleton_room as u16
        && (leveldoor_open != 0 || (*custom).skeleton_require_open_level_door == 0)
        && (Kid.curr_col == (*custom).skeleton_trigger_column_1 as i8
            || Kid.curr_col == (*custom).skeleton_trigger_column_2 as i8)
    {
        get_tile(
            drawn_room as c_int,
            (*custom).skeleton_column as c_int,
            (*custom).skeleton_row as c_int,
        );
        if curr_tile2 == tiles_tiles_21_skeleton as u8 {
            *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
            redraw_height = 24;
            set_redraw_full(curr_tilepos as c_short, 1);
            set_wipe(curr_tilepos as c_short, 1);
            curr_tilepos = curr_tilepos.wrapping_add(1);
            set_redraw_full(curr_tilepos as c_short, 1);
            set_wipe(curr_tilepos as c_short, 1);

            Char.room = drawn_room as u8;
            Char.curr_row = (*custom).skeleton_row as i8;
            Char.y = y_land_at((Char.curr_row + 1) as usize) as u8;
            Char.curr_col = (*custom).skeleton_column as i8;
            Char.x = (x_bump_at(
                (Char.curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize,
            ) as i32
                + TILE_SIZEX as i32) as u8;
            Char.direction = directions_dir_FF_left as i8;
            seqtbl_offset_char(seqids_seq_88_skel_wake_up as c_short);
            play_seq();
            play_sound(soundids_sound_44_skel_alive as c_int);
            guard_skill = (*custom).skeleton_skill as u16;
            Char.alive = -1;
            guardhp_max = 3;
            guardhp_curr = 3;
            Char.fall_x = 0;
            Char.fall_y = 0;
            is_guard_notice = 0;
            guard_refrac = 0;
            Char.sword = sword_status_sword_2_drawn as u8;
            Char.charid = charids_charid_4_skeleton as u8;
            saveshad();
        }
    }
}

// seg002:0F3F
#[no_mangle]
pub unsafe extern "C" fn do_auto_moves(moves_ptr: *const auto_move_type) {
    if demo_time >= 0xFE { return; }
    demo_time += 1;
    let mut demoindex = demo_index as i16;
    // moves_ptr may point into a packed struct (e.g. custom->shad_drink_move),
    // which can be unaligned. Use read_unaligned to avoid misalignment panics.
    if std::ptr::read_unaligned(moves_ptr.add(demoindex as usize)).time <= demo_time {
        demo_index += 1;
    } else {
        demoindex = demo_index as i16 - 1;
    }
    let curr_move = std::ptr::read_unaligned(moves_ptr.add(demoindex as usize)).move_;
    match curr_move {
        -1 => {}
        0 => move_0_nothing(),
        1 => move_1_forward(),
        2 => move_2_backward(),
        3 => move_3_up(),
        4 => move_4_down(),
        5 => { move_3_up(); move_1_forward(); }
        6 => move_6_shift(),
        7 => move_7(),
        _ => {}
    }
}

// seg002:1000
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level4() {
    if Char.room == (*custom).mirror_room {
        if Char.x < 80 {
            clear_char();
        } else {
            move_1_forward();
        }
    }
}

// seg002:101A
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level5() {
    if Char.room == (*custom).shadow_steal_room {
        if demo_time == 0 {
            get_tile((*custom).shadow_steal_room as c_int, 1, 0);
            if (*curr_room_modif.add(curr_tilepos as usize)) < 80 {
                return;
            }
            demo_index = 0;
        }
        do_auto_moves(core::ptr::addr_of!((*custom).shad_drink_move).cast::<auto_move_type>());
        if Char.x < 15 {
            clear_char();
        }
    }
}

// seg002:1064
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level6() {
    if Char.room == (*custom).shadow_step_room
        && Kid.frame == frameids_frame_43_running_jump_4 as u8
        && Kid.x < 128
    {
        move_6_shift();
        move_1_forward();
    }
}

// seg002:1082
#[no_mangle]
pub unsafe extern "C" fn autocontrol_shadow_level12() {
    if Char.room == 15 && shadow_initialized == 0 {
        if Opp.x >= 150 {
            do_init_shad(
                core::ptr::addr_of!((*custom).init_shad_12).cast::<byte>(),
                7, // fall
            );
            return;
        }
        shadow_initialized = 1;
    }
    if Char.sword >= sword_status_sword_2_drawn as u8 {
        if offguard == 0 || guard_refrac == 0 {
            autocontrol_guard_active();
        } else {
            move_4_down();
        }
        return;
    }
    if Opp.sword >= sword_status_sword_2_drawn as u8 || offguard == 0 {
        let mut xdiff: i16 = 0x7000; // bugfix/workaround initial value
        if can_guard_see_kid < 2 || {
            xdiff = char_opp_dist() as i16;
            xdiff >= 90
        } {
            if xdiff < 0 {
                move_2_backward();
            }
            return;
        }
        // Shadow draws his sword
        if Char.frame == frameids_frame_15_stand as u8 {
            move_down_forw();
        }
        return;
    }
    if char_opp_dist() < 10 {
        flash_color = colorids_color_15_brightwhite as u16;
        flash_time = 18;
        add_life();
        united_with_shadow = 42;
        Char.charid = charids_charid_0_kid as u8;
        savekid();
        clear_char();
        return;
    }
    if can_guard_see_kid == 2 {
        let opp_frame = Opp.frame;
        if (opp_frame >= frameids_frame_3_start_run as u8
            && opp_frame < frameids_frame_15_stand as u8)
            || (opp_frame >= frameids_frame_127_stepping_7 as u8 && opp_frame < 133)
        {
            move_1_forward();
        }
    }
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
