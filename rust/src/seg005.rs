#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;

// seqtbl_offsets is an extern const array from seqtbl.c (not exposed by bindgen)
extern "C" {
    pub static seqtbl_offsets: [u16; 0];
}

// Helper to access incomplete extern array seqtbl_offsets
unsafe fn seqtbl_offsets_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(seqtbl_offsets).cast::<u16>().add(idx)
}

// seg005:000A
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_char(seq_index: c_short) {
    Char.curr_seq = seqtbl_offsets_at(seq_index as usize);
}

// seg005:001D
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_opp(seq_index: c_int) {
    Opp.curr_seq = seqtbl_offsets_at(seq_index as usize);
}

// seg005:0030
#[no_mangle]
pub unsafe extern "C" fn do_fall() {
    if is_screaming == 0 && Char.fall_y >= 31 {
        play_sound(soundids_sound_1_falling as c_int);
        is_screaming = 1;
    }
    if (y_land_at(Char.curr_row as usize + 1) as i32) > (Char.y as i32) {
        check_grab();

        // FIX_GLIDE_THROUGH_WALL
        if (*fixes).fix_glide_through_wall != 0 {
            determine_col();
            get_tile_at_char();
            if curr_tile2 == tiles_tiles_20_wall as u8
                || ((curr_tile2 == tiles_tiles_12_doortop as u8
                    || curr_tile2 == tiles_tiles_7_doortop_with_floor as u8)
                    && Char.direction == (directions_dir_FF_left as i8))
            {
                let delta_x = distance_to_edge_weight();
                const delta_x_reference: i32 = 10;
                if delta_x >= 8 {
                    let adj_delta_x = -5 + delta_x - delta_x_reference;
                    Char.x = char_dx_forward(adj_delta_x) as u8;
                    Char.fall_x = 0;
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
            && Char.direction == (directions_dir_FF_left as i8)
        {
            if distance_to_edge_weight() >= 8 {
                in_wall();
            }
        }

        if tile_is_floor(curr_tile2 as c_int) != 0 {
            land();
        } else {
            inc_curr_row();
        }
    }
}

// seg005:0090
#[no_mangle]
pub unsafe extern "C" fn land() {
    let seq_id: u16;
    is_screaming = 0;

    // USE_SUPER_HIGH_JUMP
    if (*fixes).enable_super_high_jump != 0 {
        super_jump_fall = 0;
    }

    Char.y = y_land_at(Char.curr_row as usize + 1) as u8;

    if get_tile_at_char() != tiles_tiles_2_spike as c_int {
        if tile_is_floor(get_tile_infrontof_char()) == 0
            && distance_to_edge_weight() < 3
        {
            Char.x = char_dx_forward(-3) as u8;
        }
        // FIX_LAND_AGAINST_GATE_OR_TAPESTRY
        else if (*fixes).fix_land_against_gate_or_tapestry != 0 {
            get_tile_infrontof_char();
            if Char.direction == (directions_dir_FF_left as i8)
                && (((curr_tile2 == tiles_tiles_4_gate as u8) && can_bump_into_gate() != 0)
                    || (curr_tile2 == tiles_tiles_7_doortop_with_floor as u8))
                && distance_to_edge_weight() < 3
            {
                Char.x = char_dx_forward(-3) as u8;
            }
        }

        start_chompers();
    } else {
        // fell on spikes
        if is_spike_harmful() != 0 {
            spiked();
            return;
        }
        // FIX_SAFE_LANDING_ON_SPIKES
        else if (*fixes).fix_safe_landing_on_spikes != 0
            && curr_room_modif.add(curr_tilepos as usize).read() == 0
        {
            spiked();
            return;
        }
    }

    if Char.alive < 0 {
        // alive
        if (distance_to_edge_weight() >= 12 && get_tile_behind_char() == tiles_tiles_2_spike as c_int)
            || get_tile_at_char() == tiles_tiles_2_spike as c_int
        {
            // fell on spikes
            if is_spike_harmful() != 0 {
                spiked();
                return;
            }
            // FIX_SAFE_LANDING_ON_SPIKES
            else if (*fixes).fix_safe_landing_on_spikes != 0
                && curr_room_modif.add(curr_tilepos as usize).read() == 0
            {
                spiked();
                return;
            }
        }

        if Char.fall_y < 22 {
            // fell 1 row
            if Char.charid >= charids_charid_2_guard as u8
                || Char.sword == sword_status_sword_2_drawn as u8
            {
                Char.sword = sword_status_sword_2_drawn as u8;
                seq_id = seqids_seq_63_guard_active_after_fall as u16;
            } else {
                seq_id = seqids_seq_17_soft_land as u16;
            }
            if Char.charid == charids_charid_0_kid as u8 {
                play_sound(soundids_sound_17_soft_land as c_int);
                is_guard_notice = 1;
            }
        } else if Char.fall_y < 33 {
            // fell 2 rows
            if Char.charid == charids_charid_1_shadow as u8 {
                if Char.charid >= charids_charid_2_guard as u8
                    || Char.sword == sword_status_sword_2_drawn as u8
                {
                    Char.sword = sword_status_sword_2_drawn as u8;
                    seq_id = seqids_seq_63_guard_active_after_fall as u16;
                } else {
                    seq_id = seqids_seq_17_soft_land as u16;
                }
                if Char.charid == charids_charid_0_kid as u8 {
                    play_sound(soundids_sound_17_soft_land as c_int);
                    is_guard_notice = 1;
                }
            } else if Char.charid == charids_charid_2_guard as u8 {
                // fell 3 or more rows
                take_hp(100);
                play_sound(soundids_sound_0_fell_to_death as c_int);
                seq_id = seqids_seq_22_crushed as u16;
            } else {
                // kid (or skeleton (bug!))
                if take_hp(1) == 0 {
                    // still alive
                    play_sound(soundids_sound_16_medium_land as c_int);
                    is_guard_notice = 1;
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

    seqtbl_offset_char(seq_id as c_short);
    play_seq();
    Char.fall_y = 0;
}

// seg005:01B7
#[no_mangle]
pub unsafe extern "C" fn spiked() {
    curr_room_modif.add(curr_tilepos as usize).write(0xFF as u8);
    Char.y = y_land_at(Char.curr_row as usize + 1) as u8;

    // FIX_OFFSCREEN_GUARDS_DISAPPEARING
    let spike_col = if (*fixes).fix_offscreen_guards_disappearing != 0 && curr_room != Char.room as i16 {
        if curr_room == level.roomlinks[Char.room as usize - 1].right as i16 {
            tile_col + 10
        } else if curr_room == level.roomlinks[Char.room as usize - 1].left as i16 {
            tile_col - 10
        } else {
            tile_col
        }
    } else {
        tile_col
    };

    Char.x = x_bump_at((spike_col + FIRST_ONSCREEN_COLUMN as i16) as usize) as u8 + 10;
    Char.x = char_dx_forward(8) as u8;
    Char.fall_y = 0;
    play_sound(soundids_sound_48_spiked as c_int);
    take_hp(100);
    seqtbl_offset_char(seqids_seq_51_spiked as c_short);
    play_seq();
}

// seg005:0213
#[no_mangle]
pub unsafe extern "C" fn control() {
    let char_frame = Char.frame;
    if Char.alive >= 0 {
        if char_frame == frameids_frame_15_stand as u8
            || char_frame == frameids_frame_166_stand_inactive as u8
            || char_frame == frameids_frame_158_stand_with_sword as u8
            || char_frame == frameids_frame_171_stand_with_sword as u8
        {
            seqtbl_offset_char(seqids_seq_71_dying as c_short);
        }
    } else {
        let char_action = Char.action;
        if char_action == actions_actions_5_bumped as u8
            || char_action == actions_actions_4_in_freefall as u8
        {
            release_arrows();
        } else if Char.sword == sword_status_sword_2_drawn as u8 {
            control_with_sword();
        } else if Char.charid >= charids_charid_2_guard as u8 {
            control_guard_inactive();
        } else if char_frame == frameids_frame_15_stand as u8
            || (char_frame >= frameids_frame_50_turn as u8 && char_frame < 53)
        {
            control_standing();
        } else if char_frame == frameids_frame_48_turn as u8 {
            control_turning();
        } else if char_frame < 4 {
            control_startrun();
        } else if char_frame >= frameids_frame_67_start_jump_up_1 as u8
            && char_frame < frameids_frame_70_jumphang as u8
        {
            control_jumpup();
        } else if char_frame < 15 {
            control_running();
        } else if char_frame >= frameids_frame_87_hanging_1 as u8 && char_frame < 100 {
            control_hanging();
        } else if char_frame == frameids_frame_109_crouch as u8 {
            control_crouched();
        }

        // ALLOW_CROUCH_AFTER_CLIMBING
        if (*fixes).enable_crouch_after_climbing != 0
            && Char.curr_seq >= seqtbl_offsets_at(seqids_seq_50_crouch as usize)
            && Char.curr_seq < seqtbl_offsets_at(seqids_seq_49_stand_up_from_crouch as usize)
        {
            if control_forward != CONTROL_IGNORE as i8 {
                control_forward = CONTROL_RELEASED as i8;
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
            && Char.curr_seq >= seqtbl_offsets_at(seqids_seq_92_put_sword_away as usize)
            && Char.curr_seq < seqtbl_offsets_at(seqids_seq_93_put_sword_away_fast as usize)
        {
            release_arrows();
        }
    }
}

// ── File-scoped statics (for USE_TELEPORTS feature) ──────────────────────────
static mut source_modifier: c_int = 0;
static mut source_room: c_int = 0;
static mut source_tilepos: c_int = 0;

// seg005:02EB
#[no_mangle]
pub unsafe extern "C" fn control_crouched() {
    if need_level1_music != 0 && current_level == (*custom).intro_music_level as u16 {
        // Special event: music when crouching
        if check_sound_playing() == 0 {
            if need_level1_music == 1 {
                play_sound(soundids_sound_25_presentation as c_int);
                need_level1_music = 2;
            } else {
                // USE_REPLAY
                if recording != 0 {
                    special_move = replay_special_moves_MOVE_EFFECT_END as u8;
                }
                if replaying == 0 {
                    need_level1_music = 0;
                }
            }
        }
    } else {
        need_level1_music = 0;
        if control_shift2 == CONTROL_HELD as i8 && check_get_item() != 0 {
            return;
        }
        if control_y != CONTROL_HELD_DOWN as i8 {
            seqtbl_offset_char(seqids_seq_49_stand_up_from_crouch as c_short);
        } else if control_forward == CONTROL_HELD as i8 {
            control_forward = CONTROL_IGNORE as i8;
            seqtbl_offset_char(seqids_seq_79_crouch_hop as c_short);
        }
    }
}

// seg005:0358
#[no_mangle]
pub unsafe extern "C" fn control_standing() {
    if control_shift2 == CONTROL_HELD as i8
        && control_shift == CONTROL_HELD as i8
        && check_get_item() != 0
    {
        return;
    }
    if Char.charid != charids_charid_0_kid as u8
        && control_down == CONTROL_HELD as i8
        && control_forward == CONTROL_HELD as i8
    {
        draw_sword();
        return;
    }

    if have_sword != 0 {
        if offguard != 0 && control_shift >= CONTROL_RELEASED as i8 {
            // goto loc_6213
        } else if can_guard_see_kid >= 2 {
            let distance = char_opp_dist();
            if distance >= -10 && distance < 90 {
                holding_sword = 1;
                if (distance as u16) < ((-6i32) as u16) {
                    if Opp.charid == charids_charid_1_shadow as u8
                        && (Opp.action == actions_actions_3_in_midair as u8
                            || (Opp.frame >= frameids_frame_107_fall_land_1 as u8 && Opp.frame < 118))
                    {
                        offguard = 0;
                    } else {
                        draw_sword();
                        return;
                    }
                } else {
                    back_pressed();
                    return;
                }
            }
        } else {
            offguard = 0;
        }
    }

    // loc_6213:
    if control_shift == CONTROL_HELD as i8 {
        if control_backward == CONTROL_HELD as i8 {
            back_pressed();
        } else if control_up == CONTROL_HELD as i8 {
            up_pressed();
        } else if control_down == CONTROL_HELD as i8 {
            down_pressed();
        } else if control_x == CONTROL_HELD_FORWARD as i8 && control_forward == CONTROL_HELD as i8 {
            safe_step();
        }
    } else if control_forward == CONTROL_HELD as i8 {
        if is_keyboard_mode != 0 && control_up == CONTROL_HELD as i8 {
            standing_jump();
        } else {
            forward_pressed();
        }
    } else if control_backward == CONTROL_HELD as i8 {
        back_pressed();
    } else if control_up == CONTROL_HELD as i8 {
        if is_keyboard_mode != 0 && control_forward == CONTROL_HELD as i8 {
            standing_jump();
        } else {
            up_pressed();
        }
    } else if control_down == CONTROL_HELD as i8 {
        down_pressed();
    } else if control_x == CONTROL_HELD_FORWARD as i8 {
        forward_pressed();
    }
}

// seg005:0482
#[no_mangle]
pub unsafe extern "C" fn up_pressed() {
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
        && (level.start_room as u16) != drawn_room
        && (if (*fixes).fix_exit_door != 0 {
            curr_room_modif.add(leveldoor_tilepos as usize).read() >= 42
        } else {
            leveldoor_open != 0
        })
    {
        go_up_leveldoor();
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
        pickup_obj_type = curr_room_modif.add(curr_tilepos as usize).read() as i16;
        // Balconies with zero modifiers remain regular balconies.
        if pickup_obj_type > 0 {
            source_modifier = pickup_obj_type as c_int;
            source_room = curr_room as c_int;
            source_tilepos = curr_tilepos as c_int;
            go_up_leveldoor();
            seqtbl_offset_char(seqids_seq_teleport as c_short);
            return;
        }
    }

    // Else just jump up.
    if control_x == CONTROL_HELD_FORWARD as i8 {
        standing_jump();
    } else {
        check_jump_up();
    }
}

// seg005:04C7
#[no_mangle]
pub unsafe extern "C" fn down_pressed() {
    control_down = CONTROL_IGNORE as i8;
    if tile_is_floor(get_tile_infrontof_char()) == 0 && distance_to_edge_weight() < 3 {
        Char.x = char_dx_forward(5) as u8;
        load_fram_det_col();
    } else if tile_is_floor(get_tile_behind_char()) == 0 && distance_to_edge_weight() >= 8 {
        through_tile = get_tile_behind_char() as u8;
        get_tile_at_char();
        if can_grab() != 0
            && (!((*fixes).enable_crouch_after_climbing != 0 && control_forward == CONTROL_HELD as i8))
            && (Char.direction >= directions_dir_0_right as i8
                || get_tile_at_char() != tiles_tiles_4_gate as c_int
                || (curr_room_modif.add(curr_tilepos as usize).read() as i32) >> 2 >= 6)
        {
            Char.x = char_dx_forward(distance_to_edge_weight() - 9) as u8;
            seqtbl_offset_char(seqids_seq_68_climb_down as c_short);
        } else {
            crouch();
        }
    } else {
        crouch();
    }
}

// seg005:0574
#[no_mangle]
pub unsafe extern "C" fn go_up_leveldoor() {
    Char.x = x_bump_at((tile_col + FIRST_ONSCREEN_COLUMN as i16) as usize) as u8 + 10;
    Char.direction = directions_dir_FF_left as i8;
    seqtbl_offset_char(seqids_seq_70_go_up_on_level_door as c_short);
}

// seg005:058F
#[no_mangle]
pub unsafe extern "C" fn control_turning() {
    if control_shift >= CONTROL_RELEASED as i8
        && control_x == CONTROL_HELD_FORWARD as i8
        && control_y >= CONTROL_RELEASED as i8
    {
        // FIX_TURN_RUN_NEAR_WALL
        if (*fixes).fix_turn_running_near_wall != 0 {
            let distance = get_edge_distance();
            if edge_type == EDGE_TYPE_WALL as u8 && curr_tile2 != tiles_tiles_18_chomper as u8 && distance < 8 {
                control_forward = CONTROL_HELD as i8;
            } else {
                seqtbl_offset_char(seqids_seq_43_start_run_after_turn as c_short);
            }
        } else {
            seqtbl_offset_char(seqids_seq_43_start_run_after_turn as c_short);
        }
    }

    // Added: joystick mode handling
    if is_joyst_mode != 0 {
        if control_up == CONTROL_HELD as i8 && control_y >= CONTROL_RELEASED as i8 {
            control_up = CONTROL_RELEASED as i8;
        }
        if control_down == CONTROL_HELD as i8 && control_y <= CONTROL_RELEASED as i8 {
            control_down = CONTROL_RELEASED as i8;
        }
        if control_backward == CONTROL_HELD as i8 && control_x == CONTROL_RELEASED as i8 {
            control_backward = CONTROL_RELEASED as i8;
        }
    }
}

// seg005:05AD
#[no_mangle]
pub unsafe extern "C" fn crouch() {
    seqtbl_offset_char(seqids_seq_50_crouch as c_short);
    control_down = release_arrows() as i8;
}

// seg005:05BE
#[no_mangle]
pub unsafe extern "C" fn back_pressed() {
    let seq_id: u16;
    control_backward = release_arrows() as i8;
    // After turn, Kid will draw sword if ...
    if have_sword == 0
        || can_guard_see_kid < 2
        || char_opp_dist() > 0
        || distance_to_edge_weight() < 2
    {
        seq_id = seqids_seq_5_turn as u16;
    } else {
        Char.sword = sword_status_sword_2_drawn as u8;
        offguard = 0;
        seq_id = seqids_seq_89_turn_draw_sword as u16;
    }
    seqtbl_offset_char(seq_id as c_short);
}

// seg005:060F
#[no_mangle]
pub unsafe extern "C" fn forward_pressed() {
    let distance = get_edge_distance();

    // ALLOW_CROUCH_AFTER_CLIMBING
    if (*fixes).enable_crouch_after_climbing != 0 && control_down == CONTROL_HELD as i8 {
        down_pressed();
        control_forward = CONTROL_RELEASED as i8;
        return;
    }

    if edge_type == EDGE_TYPE_WALL as u8
        && curr_tile2 != tiles_tiles_18_chomper as u8
        && distance < 8
    {
        // If char is near a wall, step instead of run.
        if control_forward == CONTROL_HELD as i8 {
            safe_step();
        }
    } else {
        seqtbl_offset_char(seqids_seq_1_start_run as c_short);
    }
}

// seg005:0649
#[no_mangle]
pub unsafe extern "C" fn control_running() {
    if control_x == CONTROL_RELEASED as i8
        && (Char.frame == frameids_frame_7_run as u8 || Char.frame == frameids_frame_11_run as u8)
    {
        control_forward = release_arrows() as i8;
        seqtbl_offset_char(seqids_seq_13_stop_run as c_short);
    } else if control_x == CONTROL_HELD_BACKWARD as i8 {
        control_backward = release_arrows() as i8;
        seqtbl_offset_char(seqids_seq_6_run_turn as c_short);
    } else if control_y == CONTROL_HELD_UP as i8 && control_up == CONTROL_HELD as i8 {
        run_jump();
    } else if control_down == CONTROL_HELD as i8 {
        control_down = CONTROL_IGNORE as i8;
        seqtbl_offset_char(seqids_seq_26_crouch_while_running as c_short);
    }
}

// seg005:06A8
#[no_mangle]
pub unsafe extern "C" fn safe_step() {
    control_shift2 = CONTROL_IGNORE as i8;
    control_forward = CONTROL_IGNORE as i8;
    let distance = get_edge_distance();
    if distance != 0 {
        Char.repeat = 1;
        seqtbl_offset_char((distance + 28) as c_short);
    } else if edge_type != EDGE_TYPE_WALL as u8 && Char.repeat != 0 {
        Char.repeat = 0;
        seqtbl_offset_char(seqids_seq_44_step_on_edge as c_short);
    } else {
        seqtbl_offset_char(seqids_seq_39_safe_step_11 as c_short);
    }
}


// seg005:06F0
#[no_mangle]
pub unsafe extern "C" fn check_get_item() -> c_int {
    if get_tile_at_char() == tiles_tiles_10_potion as c_int || curr_tile2 == tiles_tiles_22_sword as u8 {
        if tile_is_floor(get_tile_behind_char()) == 0 {
            return 0;
        }
        Char.x = char_dx_forward(-14) as u8;
        load_fram_det_col();
    }
    if get_tile_infrontof_char() == tiles_tiles_10_potion as c_int
        || curr_tile2 == tiles_tiles_22_sword as u8
    {
        get_item();
        return 1;
    }
    0
}

// seg005:073E
#[no_mangle]
pub unsafe extern "C" fn get_item() {
    if Char.frame != frameids_frame_109_crouch as u8 {
        // crouching
        let distance = get_edge_distance();
        if edge_type != EDGE_TYPE_FLOOR as u8 {
            Char.x = char_dx_forward(distance) as u8;
        }
        if Char.direction >= directions_dir_0_right as i8 {
            Char.x =
                char_dx_forward(if curr_tile2 == tiles_tiles_10_potion as u8 { 1 } else { 0 } - 2)
                    as u8;
        }
        crouch();
    } else if curr_tile2 == tiles_tiles_22_sword as u8 {
        do_pickup(-1);
        seqtbl_offset_char(seqids_seq_91_get_sword as c_short);
    } else {
        // potion
        do_pickup((curr_room_modif.add(curr_tilepos as usize).read() as i32) >> 3);
        seqtbl_offset_char(seqids_seq_78_drink as c_short);
        // USE_COPYPROT
        if current_level == 15 {
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

// seg005:07FF
#[no_mangle]
pub unsafe extern "C" fn control_startrun() {
    if control_y == CONTROL_HELD_UP as i8 && control_x == CONTROL_HELD_FORWARD as i8 {
        standing_jump();
    }
}

// seg005:0812
#[no_mangle]
pub unsafe extern "C" fn control_jumpup() {
    if control_x == CONTROL_HELD_FORWARD as i8 || control_forward == CONTROL_HELD as i8 {
        standing_jump();
    }
}

// seg005:0825
#[no_mangle]
pub unsafe extern "C" fn standing_jump() {
    control_up = CONTROL_IGNORE as i8;
    control_forward = CONTROL_IGNORE as i8;
    seqtbl_offset_char(seqids_seq_3_standing_jump as c_short);
}

// seg005:0836
#[no_mangle]
pub unsafe extern "C" fn check_jump_up() {
    control_up = release_arrows() as i8;
    through_tile = get_tile_above_char() as u8;
    get_tile_front_above_char();
    if can_grab() != 0 {
        grab_up_with_floor_behind();
    } else {
        through_tile = get_tile_behind_above_char() as u8;
        get_tile_above_char();
        if can_grab() != 0 {
            jump_up_or_grab();
        } else {
            jump_up();
        }
    }
}

// seg005:087B
#[no_mangle]
pub unsafe extern "C" fn jump_up_or_grab() {
    let distance = distance_to_edge_weight();
    if distance < 6 {
        jump_up();
    } else if tile_is_floor(get_tile_behind_char()) == 0 {
        // There is not floor behind char.
        grab_up_no_floor_behind();
    } else {
        // There is floor behind char, go back a bit.
        Char.x = char_dx_forward(distance - TILE_SIZEX as i32) as u8;
        load_fram_det_col();
        grab_up_with_floor_behind();
    }
}

// seg005:08C7
#[no_mangle]
pub unsafe extern "C" fn grab_up_no_floor_behind() {
    get_tile_above_char();
    Char.x = char_dx_forward(distance_to_edge_weight() - 10) as u8;
    seqtbl_offset_char(seqids_seq_16_jump_up_and_grab as c_short);
}


// seg005:08E6
#[no_mangle]
pub unsafe extern "C" fn jump_up() {
    let delta_x: u16;
    control_up = release_arrows() as i8;
    let distance = get_edge_distance();
    if distance < 4 && edge_type == EDGE_TYPE_WALL as u8 {
        Char.x = char_dx_forward(distance - 3) as u8;
    }
    // FIX_JUMP_DISTANCE_AT_EDGE
    if (*fixes).fix_jump_distance_at_edge != 0 && distance == 3 && edge_type == EDGE_TYPE_CLOSER as u8 {
        Char.x = char_dx_forward(-1) as u8;
    }

    // USE_SUPER_HIGH_JUMP
    if is_feather_fall != 0 && tile_is_floor(get_tile_above_char()) == 0 && curr_tile2 != tiles_tiles_20_wall as u8 {
        delta_x = if Char.direction == directions_dir_FF_left as i8 { 1 } else { 3 };
    } else {
        delta_x = 0;
    }
    let char_col = get_tile_div_mod(back_delta_x(delta_x as c_int) + dx_weight() as i32 - 6);
    get_tile(Char.room as c_int, char_col, Char.curr_row as c_int - 1);
    if curr_tile2 != tiles_tiles_20_wall as u8 && tile_is_floor(curr_tile2 as c_int) == 0 {
        if (*fixes).enable_super_high_jump != 0 && is_feather_fall != 0 {
            // super high jump can only happen in feather mode
            if curr_room == 0 && Char.curr_row == 0 {
                // there is no room above
                seqtbl_offset_char(seqids_seq_14_jump_up_into_ceiling as c_short);
            } else {
                get_tile(Char.room as c_int, char_col, Char.curr_row as c_int - 2); // the target top tile
                let is_top_floor = tile_is_floor(curr_tile2 as c_int) != 0 || curr_tile2 == tiles_tiles_20_wall as u8;
                let mut is_top_floor_final = is_top_floor;
                if is_top_floor && curr_tile2 == tiles_tiles_11_loose as u8 && (curr_room_tiles.add(curr_tilepos as usize).read() & 0x20) == 0 {
                    is_top_floor_final = false;
                }
                // kid should jump slightly higher if the top tile is not a floor
                super_jump_timer = if is_top_floor_final { 22 } else { 24 };
                super_jump_room = curr_room as u8;
                super_jump_col = tile_col as i8;
                super_jump_row = tile_row as i8;
                seqtbl_offset_char(seqids_seq_48_super_high_jump as c_short);
            }
        } else {
            seqtbl_offset_char(seqids_seq_28_jump_up_with_nothing_above as c_short);
        }
    } else {
        seqtbl_offset_char(seqids_seq_14_jump_up_into_ceiling as c_short);
    }
}

// seg005:0968
#[no_mangle]
pub unsafe extern "C" fn control_hanging() {
    if Char.alive < 0 {
        if grab_timer == 0 && control_y == CONTROL_HELD as i8 {
            can_climb_up();
        } else if control_shift == CONTROL_HELD as i8
            || ((*fixes).enable_super_high_jump != 0 && super_jump_fall != 0 && control_y == CONTROL_HELD as i8)
        {
            // hanging against a wall or a doortop
            if Char.action != actions_actions_6_hang_straight as u8
                && (get_tile_at_char() == tiles_tiles_20_wall as c_int
                    || (Char.direction == directions_dir_FF_left as i8
                        && ((curr_tile2 == tiles_tiles_7_doortop_with_floor as u8)
                            || (curr_tile2 == tiles_tiles_12_doortop as u8))))
            {
                if grab_timer == 0 {
                    play_sound(soundids_sound_8_bumped as c_int);
                }
                seqtbl_offset_char(seqids_seq_25_hang_against_wall as c_short);
            } else if tile_is_floor(get_tile_above_char()) == 0 {
                hang_fall();
            }
        } else {
            hang_fall();
        }
    } else {
        hang_fall();
    }
}

// seg005:09DF
#[no_mangle]
pub unsafe extern "C" fn can_climb_up() {
    let mut seq_id = seqids_seq_10_climb_up as u16;
    control_up = release_arrows() as i8;
    control_shift2 = release_arrows() as i8;
    // USE_SUPER_HIGH_JUMP
    if (*fixes).enable_super_high_jump != 0 {
        super_jump_fall = 0;
    }

    get_tile_above_char();
    if ((curr_tile2 == tiles_tiles_13_mirror as u8 || curr_tile2 == tiles_tiles_18_chomper as u8)
        && Char.direction == directions_dir_0_right as i8)
        || (curr_tile2 == tiles_tiles_4_gate as u8
            && Char.direction != directions_dir_0_right as i8
            && (curr_room_modif.add(curr_tilepos as usize).read() as i32) >> 2 < 6)
    {
        seq_id = seqids_seq_73_climb_up_to_closed_gate as u16;
    }
    seqtbl_offset_char(seq_id as c_short);
}

// seg005:0A46
#[no_mangle]
pub unsafe extern "C" fn hang_fall() {
    control_down = release_arrows() as i8;
    // USE_SUPER_HIGH_JUMP
    if (*fixes).enable_super_high_jump != 0 {
        super_jump_fall = 0;
    }

    if tile_is_floor(get_tile_behind_char()) == 0 && tile_is_floor(get_tile_at_char()) == 0 {
        seqtbl_offset_char(seqids_seq_23_release_ledge_and_fall as c_short);
    } else {
        if get_tile_at_char() == tiles_tiles_20_wall as c_int
            || (Char.direction < directions_dir_0_right as i8
                && ((curr_tile2 == tiles_tiles_7_doortop_with_floor as u8)
                    || (curr_tile2 == tiles_tiles_12_doortop as u8)))
        {
            Char.x = char_dx_forward(-7) as u8;
        }
        seqtbl_offset_char(seqids_seq_11_release_ledge_and_land as c_short);
    }
}


// seg005:0AA8
#[no_mangle]
pub unsafe extern "C" fn grab_up_with_floor_behind() {
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
        Char.x = char_dx_forward(distance) as u8;
        seqtbl_offset_char(seqids_seq_8_jump_up_and_grab_straight as c_short);
    } else {
        Char.x = char_dx_forward(distance - 4) as u8;
        seqtbl_offset_char(seqids_seq_24_jump_up_and_grab_forward as c_short);
    }
}

// seg005:0AF7
#[no_mangle]
pub unsafe extern "C" fn run_jump() {
    if Char.frame >= frameids_frame_7_run as u8 {
        // Align Kid to edge of floor.
        let xpos = char_dx_forward(4);
        let mut col = get_tile_div_mod_m7(xpos);
        for tiles_forward in 0..2 {
            col += *core::ptr::addr_of!(dir_front).cast::<i8>().add((Char.direction as i8 as i32 + 1) as usize) as i32;
            get_tile(Char.room as c_int, col, Char.curr_row as c_int);
            if curr_tile2 == tiles_tiles_2_spike as u8 || tile_is_floor(curr_tile2 as c_int) == 0 {
                let mut pos_adjustment =
                    distance_to_edge(xpos) + (TILE_SIZEX as i32) * tiles_forward - (TILE_SIZEX as i32);
                if (pos_adjustment as u32) < ((-8i32) as u32) || pos_adjustment >= 2 {
                    if pos_adjustment < 128 {
                        return;
                    }
                    pos_adjustment = -3;
                }
                Char.x = char_dx_forward(pos_adjustment + 4) as u8;
                break;
            }
        }
        control_up = release_arrows() as i8;
        seqtbl_offset_char(seqids_seq_4_run_jump as c_short);
    }
}

// seg005:0BB5
#[no_mangle]
pub unsafe extern "C" fn back_with_sword() {
    let frame = Char.frame;
    if frame == frameids_frame_158_stand_with_sword as u8
        || frame == frameids_frame_170_stand_with_sword as u8
        || frame == frameids_frame_171_stand_with_sword as u8
    {
        control_backward = CONTROL_IGNORE as i8;
        seqtbl_offset_char(seqids_seq_57_back_with_sword as c_short);
    }
}

// seg005:0BE3
#[no_mangle]
pub unsafe extern "C" fn forward_with_sword() {
    let frame = Char.frame;
    if frame == frameids_frame_158_stand_with_sword as u8
        || frame == frameids_frame_170_stand_with_sword as u8
        || frame == frameids_frame_171_stand_with_sword as u8
    {
        control_forward = CONTROL_IGNORE as i8;
        if Char.charid != charids_charid_0_kid as u8 {
            seqtbl_offset_char(seqids_seq_56_guard_forward_with_sword as c_short);
        } else {
            seqtbl_offset_char(seqids_seq_86_forward_with_sword as c_short);
        }
    }
}

// seg005:0C1D
#[no_mangle]
pub unsafe extern "C" fn draw_sword() {
    let mut seq_id = seqids_seq_55_draw_sword as u16;
    control_forward = release_arrows() as i8;
    control_shift2 = release_arrows() as i8;
    // FIX_UNINTENDED_SWORD_STRIKE
    if (*fixes).fix_unintended_sword_strike != 0 {
        ctrl1_shift2 = CONTROL_IGNORE as i8;
    }

    if Char.charid == charids_charid_0_kid as u8 {
        play_sound(soundids_sound_19_draw_sword as c_int);
        offguard = 0;
    } else if Char.charid != charids_charid_1_shadow as u8 {
        seq_id = seqids_seq_90_en_garde as u16;
    }
    Char.sword = sword_status_sword_2_drawn as u8;
    seqtbl_offset_char(seq_id as c_short);
}

// seg005:0C67
#[no_mangle]
pub unsafe extern "C" fn control_with_sword() {
    if Char.action < actions_actions_2_hang_climb as u8 {
        if get_tile_at_char() == tiles_tiles_11_loose as c_int || can_guard_see_kid >= 2 {
            let distance = char_opp_dist();
            if (distance as u32) < (90u32) {
                swordfight();
                return;
            } else if distance < 0 {
                if (distance as u32) < ((-4i32) as u32) {
                    seqtbl_offset_char(seqids_seq_60_turn_with_sword as c_short);
                    return;
                } else {
                    swordfight();
                    return;
                }
            }
        }
        if Char.charid == charids_charid_0_kid as u8 && Char.alive < 0 {
            holding_sword = 0;
        }
        if (Char.charid as i32) < (charids_charid_2_guard as i32) {
            if Char.frame == frameids_frame_171_stand_with_sword as u8 {
                Char.sword = sword_status_sword_0_sheathed as u8;
                seqtbl_offset_char(seqids_seq_92_put_sword_away as c_short);
            }
        } else {
            swordfight();
        }
    }
}

// seg005:0CDB
#[no_mangle]
pub unsafe extern "C" fn swordfight() {
    let seq_id: u16;
    let frame = Char.frame;
    let charid = Char.charid;
    // frame 161: parry
    if frame == frameids_frame_161_parry as u8 && control_shift2 >= CONTROL_RELEASED as i8 {
        seqtbl_offset_char(seqids_seq_57_back_with_sword as c_short);
        return;
    } else if control_shift2 == CONTROL_HELD as i8 {
        if charid == charids_charid_0_kid as u8 {
            kid_sword_strike = 15;
        }
        sword_strike();
        if control_shift2 == CONTROL_IGNORE as i8 {
            return;
        }
    }
    if control_down == CONTROL_HELD as i8 {
        if frame == frameids_frame_158_stand_with_sword as u8
            || frame == frameids_frame_170_stand_with_sword as u8
            || frame == frameids_frame_171_stand_with_sword as u8
        {
            control_down = CONTROL_IGNORE as i8;
            Char.sword = sword_status_sword_0_sheathed as u8;
            if charid == charids_charid_0_kid as u8 {
                offguard = 1;
                guard_refrac = 9;
                holding_sword = 0;
                seq_id = seqids_seq_93_put_sword_away_fast as u16;
            } else if charid == charids_charid_1_shadow as u8 {
                seq_id = seqids_seq_92_put_sword_away as u16;
            } else {
                seq_id = seqids_seq_87_guard_become_inactive as u16;
            }
            seqtbl_offset_char(seq_id as c_short);
        }
    } else if control_up == CONTROL_HELD as i8 {
        parry();
    } else if control_forward == CONTROL_HELD as i8 {
        forward_with_sword();
    } else if control_backward == CONTROL_HELD as i8 {
        back_with_sword();
    }
}

// seg005:0DB0
#[no_mangle]
pub unsafe extern "C" fn sword_strike() {
    let frame = Char.frame;
    let seq_id: u16;
    if frame == frameids_frame_157_walk_with_sword as u8
        || frame == frameids_frame_158_stand_with_sword as u8
        || frame == frameids_frame_170_stand_with_sword as u8
        || frame == frameids_frame_171_stand_with_sword as u8
        || frame == frameids_frame_165_walk_with_sword as u8
    {
        if Char.charid == charids_charid_0_kid as u8 {
            seq_id = seqids_seq_75_strike as u16;
        } else {
            seq_id = seqids_seq_58_guard_strike as u16;
        }
    } else if frame == frameids_frame_150_parry as u8 || frame == frameids_frame_161_parry as u8 {
        seq_id = seqids_seq_66_strike_after_parry as u16;
    } else {
        return;
    }
    control_shift2 = CONTROL_IGNORE as i8;
    seqtbl_offset_char(seq_id as c_short);
}

// seg005:0E0F
#[no_mangle]
pub unsafe extern "C" fn parry() {
    let char_frame = Char.frame;
    let opp_frame = Opp.frame;
    let char_charid = Char.charid;
    let mut seq_id = seqids_seq_62_parry as u16;
    let mut do_play_seq: i32 = 0;
    if char_frame == frameids_frame_158_stand_with_sword as u8
        || char_frame == frameids_frame_170_stand_with_sword as u8
        || char_frame == frameids_frame_171_stand_with_sword as u8
        || char_frame == frameids_frame_168_back as u8
        || char_frame == frameids_frame_165_walk_with_sword as u8
    {
        if char_opp_dist() >= 32 && char_charid != charids_charid_0_kid as u8 {
            back_with_sword();
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
                    back_with_sword();
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
    control_up = CONTROL_IGNORE as i8;
    seqtbl_offset_char(seq_id as c_short);
    if do_play_seq != 0 {
        play_seq();
    }
}

// USE_TELEPORTS
#[no_mangle]
pub unsafe extern "C" fn teleport() {
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
                    && curr_modifier as c_int == source_modifier
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
        Char.room = dest_room as u8;
        Char.curr_col = (dest_tilepos % 10) as i8;
        Char.curr_row = (dest_tilepos / 10) as i8;
        Char.x = (x_bump_at((Char.curr_col as i32 + 5) as usize) as i32 + 14 + 7) as u8; // Center on the destination teleport.
        Char.y = y_land_at((Char.curr_row as usize) + 1) as u8;
        next_room = Char.room as u16;
        clear_coll_rooms(); // Without this, the prince will sometimes end up at the wrong place.
        // FIX_DISAPPEARING_GUARD_B
        if next_room != drawn_room {
            leave_guard();
        }
        // FIX_DISAPPEARING_GUARD_A
        if next_room == drawn_room {
            drawn_room = 0;
        }
        seqtbl_offset_char(seqids_seq_5_turn as c_short);
        play_sound(soundids_sound_45_jump_through_mirror as c_int);
    } else {
        // No pair found.
        Char.x = (x_bump_at((Char.curr_col as i32 + 5) as usize) as i32 + 14) as u8;
        Char.y = y_land_at((Char.curr_row as usize) + 1) as u8;
        seqtbl_offset_char(seqids_seq_17_soft_land as c_short);
        play_sound(soundids_sound_0_fell_to_death as c_int);
    }
}

