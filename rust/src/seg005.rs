// Character movement — ported from seg005.c.
// All 37 public functions are #[no_mangle] extern "C" for transparent C linkage.

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;

// ── File-private state ────────────────────────────────────────────────────────

static mut source_modifier: c_int = 0;
static mut source_room:     c_int = 0;
static mut source_tilepos:  c_int = 0;

// ── Raw-pointer helpers for incomplete-array globals ─────────────────────────

// copyprot_room/tile are extern word[] (incomplete), so bindgen emits [u16; 0].
unsafe fn copyprot_room_read(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_room).cast::<u16>().add(idx)
}
unsafe fn copyprot_room_write(idx: usize, val: u16) {
    *core::ptr::addr_of_mut!(copyprot_room).cast::<u16>().add(idx) = val;
}
unsafe fn copyprot_tile_read(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_tile).cast::<u16>().add(idx)
}

// dir_front is extern const sbyte[] (incomplete), bindgen emits [i8; 0].
unsafe fn dir_front_at(idx: usize) -> i8 {
    *core::ptr::addr_of!(dir_front).cast::<i8>().add(idx)
}

// ── Exported functions ────────────────────────────────────────────────────────

// seg005:000A
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_char(seq_index: c_short) {
    Char.curr_seq = seqtbl::seqtbl_offsets[seq_index as usize];
}

// seg005:001D
#[no_mangle]
pub unsafe extern "C" fn seqtbl_offset_opp(seq_index: c_int) {
    Opp.curr_seq = seqtbl::seqtbl_offsets[seq_index as usize];
}

// seg005:0030
#[no_mangle]
pub unsafe extern "C" fn do_fall() {
    if is_screaming == 0 && Char.fall_y >= 31 {
        play_sound(soundids_sound_1_falling as c_int);
        is_screaming = 1;
    }
    if (y_land[(Char.curr_row as i32 + 1) as usize] as u16) > (Char.y as u16) {
        check_grab();

        // FIX_GLIDE_THROUGH_WALL
        if (*fixes).fix_glide_through_wall != 0 {
            determine_col();
            get_tile_at_char();
            if curr_tile2 == tiles_tiles_20_wall as u8
                || ((curr_tile2 == tiles_tiles_12_doortop as u8
                    || curr_tile2 == tiles_tiles_7_doortop_with_floor as u8)
                    && Char.direction == directions_dir_FF_left as i8)
            {
                let mut delta_x = distance_to_edge_weight() as c_int;
                if delta_x >= 8 {
                    delta_x = -5 + delta_x - 10; // delta_x_reference = 10
                    Char.x = char_dx_forward(delta_x) as u8;
                    Char.fall_x = 0;
                }
            }
        }
    } else {
        // FIX_JUMP_THROUGH_WALL_ABOVE_GATE
        if (*fixes).fix_jump_through_wall_above_gate != 0 {
            if get_tile_at_char() as u8 != tiles_tiles_4_gate as u8 {
                determine_col();
            }
        }

        if get_tile_at_char() as u8 == tiles_tiles_20_wall as u8 {
            in_wall();
        } else if (*fixes).fix_drop_through_tapestry != 0
            && get_tile_at_char() as u8 == tiles_tiles_12_doortop as u8
            && Char.direction == directions_dir_FF_left as i8
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
    let mut seq_id: u16 = 0;
    is_screaming = 0;

    // USE_SUPER_HIGH_JUMP
    if (*fixes).enable_super_high_jump != 0 {
        super_jump_fall = 0;
    }

    Char.y = y_land[(Char.curr_row as i32 + 1) as usize] as u8;

    let on_spike_tile = get_tile_at_char() as u8 == tiles_tiles_2_spike as u8;

    if !on_spike_tile {
        if tile_is_floor(get_tile_infrontof_char() as c_int) == 0
            && distance_to_edge_weight() < 3
        {
            Char.x = char_dx_forward(-3) as u8;
        } else if (*fixes).fix_land_against_gate_or_tapestry != 0 {
            // FIX_LAND_AGAINST_GATE_OR_TAPESTRY
            get_tile_infrontof_char();
            if Char.direction == directions_dir_FF_left as i8
                && ((curr_tile2 == tiles_tiles_4_gate as u8 && can_bump_into_gate() != 0)
                    || curr_tile2 == tiles_tiles_7_doortop_with_floor as u8)
                && distance_to_edge_weight() < 3
            {
                Char.x = char_dx_forward(-3) as u8;
            }
        }
        start_chompers();
    }

    // Spike check: entered when on_spike_tile (bypasses alive check, matching C goto)
    // or when alive and near/on a spike.
    let check_spikes = on_spike_tile
        || (Char.alive < 0
            && ((distance_to_edge_weight() >= 12
                && get_tile_behind_char() as u8 == tiles_tiles_2_spike as u8)
                || get_tile_at_char() as u8 == tiles_tiles_2_spike as u8));

    if check_spikes {
        if is_spike_harmful() != 0 {
            spiked();
            return;
        }
        // FIX_SAFE_LANDING_ON_SPIKES
        if (*fixes).fix_safe_landing_on_spikes != 0
            && *curr_room_modif.add(curr_tilepos as usize) == 0
        {
            spiked();
            return;
        }
    }

    // When on_spike_tile the original C jumps into the alive-block via goto,
    // bypassing the alive check. Replicate by skipping the dead path.
    let mut dead = false;
    let mut skip_take_hp = false;

    if !on_spike_tile && Char.alive >= 0 {
        // Character is dead — go directly to the death sequence.
        dead = true;
    } else {
        // Alive path (or spike-tile path which skips the alive check).
        let fall_y = Char.fall_y;
        let charid = Char.charid;

        // Advisor correction: shadow shortcut applies only for fall_y in [22, 32].
        let soft_land = fall_y < 22
            || (fall_y < 33 && charid == charids_charid_1_shadow as u8);

        if soft_land {
            if charid >= charids_charid_2_guard as u8
                || Char.sword == sword_status_sword_2_drawn as u8
            {
                Char.sword = sword_status_sword_2_drawn as u8;
                seq_id = seqids_seq_63_guard_active_after_fall as u16;
            } else {
                seq_id = seqids_seq_17_soft_land as u16;
            }
            if charid == charids_charid_0_kid as u8 {
                play_sound(soundids_sound_17_soft_land as c_int);
                is_guard_notice = 1;
            }
        } else if fall_y < 33 {
            // Not shadow (already handled by soft_land above).
            if charid == charids_charid_2_guard as u8 {
                dead = true;
            } else {
                // Kid (or skeleton — original bug preserved).
                if take_hp(1) != 0 {
                    // Dead — take_hp already consumed the last HP.
                    dead = true;
                    skip_take_hp = true;
                } else {
                    play_sound(soundids_sound_16_medium_land as c_int);
                    is_guard_notice = 1;
                    seq_id = seqids_seq_20_medium_land as u16;
                }
            }
        } else {
            // Fell 3 or more rows.
            dead = true;
        }
    }

    if dead {
        if !skip_take_hp {
            take_hp(100);
        }
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
    *curr_room_modif.add(curr_tilepos as usize) = 0xFF;
    Char.y = y_land[(Char.curr_row as i32 + 1) as usize] as u8;

    // FIX_OFFSCREEN_GUARDS_DISAPPEARING
    if (*fixes).fix_offscreen_guards_disappearing != 0 {
        let mut spike_col = tile_col as i32;
        if curr_room != Char.room as i16 {
            if curr_room == level.roomlinks[(Char.room as usize) - 1].right as i16 {
                spike_col += 10;
            } else if curr_room == level.roomlinks[(Char.room as usize) - 1].left as i16 {
                spike_col -= 10;
            }
        }
        Char.x = (x_bump_at((spike_col + FIRST_ONSCREEN_COLUMN as i32) as usize) as i32 + 10) as u8;
    } else {
        Char.x = (x_bump_at((tile_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i32 + 10) as u8;
    }

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
        else if (*fixes).enable_crouch_after_climbing != 0
            && Char.curr_seq >= seqtbl::seqtbl_offsets[seqids_seq_50_crouch as usize]
            && Char.curr_seq < seqtbl::seqtbl_offsets[seqids_seq_49_stand_up_from_crouch as usize]
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
            && Char.curr_seq >= seqtbl::seqtbl_offsets[seqids_seq_92_put_sword_away as usize]
            && Char.curr_seq < seqtbl::seqtbl_offsets[seqids_seq_93_put_sword_away_fast as usize]
        {
            release_arrows();
        }
    }
}

// seg005:02EB
#[no_mangle]
pub unsafe extern "C" fn control_crouched() {
    if need_level1_music != 0 && current_level == (*custom).intro_music_level {
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
        } else {
            if control_forward == CONTROL_HELD as i8 {
                control_forward = CONTROL_IGNORE as i8;
                seqtbl_offset_char(seqids_seq_79_crouch_hop as c_short);
            }
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

    // Advisor correction: at_loc_6213 flag replicates "goto loc_6213" which
    // jumps past both the have_sword block and the shift-held check.
    let mut at_loc_6213 = false;

    if have_sword != 0 {
        if offguard != 0 && control_shift >= CONTROL_RELEASED as i8 {
            at_loc_6213 = true;
        } else if can_guard_see_kid >= 2 {
            let distance = char_opp_dist();
            if distance >= -10 && distance < 90 {
                holding_sword = 1;
                if (distance as u16) < ((-6i16) as u16) {
                    if Opp.charid == charids_charid_1_shadow as u8
                        && (Opp.action == actions_actions_3_in_midair as u8
                            || (Opp.frame >= frameids_frame_107_fall_land_1 as u8
                                && Opp.frame < 118))
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

    if !at_loc_6213 && control_shift == CONTROL_HELD as i8 {
        if control_backward == CONTROL_HELD as i8 {
            back_pressed();
        } else if control_up == CONTROL_HELD as i8 {
            up_pressed();
        } else if control_down == CONTROL_HELD as i8 {
            down_pressed();
        } else if control_x == CONTROL_HELD_FORWARD as i8 && control_forward == CONTROL_HELD as i8 {
            safe_step();
        }
    } else {
        // loc_6213:
        if control_forward == CONTROL_HELD as i8 {
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
}

// seg005:0482
#[no_mangle]
pub unsafe extern "C" fn up_pressed() {
    let mut leveldoor_tilepos: c_int = -1;
    if get_tile_at_char() as u8 == tiles_tiles_16_level_door_left as u8 {
        leveldoor_tilepos = curr_tilepos as c_int;
    } else if get_tile_behind_char() as u8 == tiles_tiles_16_level_door_left as u8 {
        leveldoor_tilepos = curr_tilepos as c_int;
    } else if get_tile_infrontof_char() as u8 == tiles_tiles_16_level_door_left as u8 {
        leveldoor_tilepos = curr_tilepos as c_int;
    }
    if leveldoor_tilepos != -1
        && level.start_room != drawn_room as u8
        && (if (*fixes).fix_exit_door != 0 {
            *curr_room_modif.add(leveldoor_tilepos as usize) >= 42
        } else {
            leveldoor_open != 0
        })
    {
        go_up_leveldoor();
        return;
    }

    // USE_TELEPORTS
    {
        leveldoor_tilepos = -1;
        if get_tile_at_char() as u8 == tiles_tiles_23_balcony_left as u8 {
            leveldoor_tilepos = curr_tilepos as c_int;
        } else if get_tile_behind_char() as u8 == tiles_tiles_23_balcony_left as u8 {
            leveldoor_tilepos = curr_tilepos as c_int;
        } else if get_tile_infrontof_char() as u8 == tiles_tiles_23_balcony_left as u8 {
            leveldoor_tilepos = curr_tilepos as c_int;
        }
        if leveldoor_tilepos != -1 {
            pickup_obj_type = curr_modifier as i16;
            if pickup_obj_type > 0 {
                source_modifier = pickup_obj_type as c_int;
                source_room = curr_room as c_int;
                source_tilepos = curr_tilepos as c_int;
                go_up_leveldoor();
                seqtbl_offset_char(seqids_seq_teleport as c_short);
                return;
            }
        }
    }

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
    if tile_is_floor(get_tile_infrontof_char() as c_int) == 0
        && distance_to_edge_weight() < 3
    {
        Char.x = char_dx_forward(5) as u8;
        load_fram_det_col();
    } else if tile_is_floor(get_tile_behind_char() as c_int) == 0
        && distance_to_edge_weight() >= 8
    {
        through_tile = get_tile_behind_char() as u8;
        get_tile_at_char();
        if can_grab() != 0
            && !((*fixes).enable_crouch_after_climbing != 0
                && control_forward == CONTROL_HELD as i8)
            && (Char.direction >= directions_dir_0_right as i8
                || get_tile_at_char() as u8 != tiles_tiles_4_gate as u8
                || (*curr_room_modif.add(curr_tilepos as usize)) >> 2 >= 6)
        {
            Char.x = char_dx_forward(distance_to_edge_weight() as c_int - 9) as u8;
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
    Char.x = (x_bump_at((tile_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i32 + 10) as u8;
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
            if edge_type == EDGE_TYPE_WALL as u8
                && curr_tile2 != tiles_tiles_18_chomper as u8
                && distance < 8
            {
                control_forward = CONTROL_HELD as i8;
            } else {
                seqtbl_offset_char(seqids_seq_43_start_run_after_turn as c_short);
            }
        } else {
            seqtbl_offset_char(seqids_seq_43_start_run_after_turn as c_short);
        }
    }

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
        seqtbl_offset_char((distance + 28) as c_short); // 29..42: safe step to edge
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
    if get_tile_at_char() as u8 == tiles_tiles_10_potion as u8
        || curr_tile2 == tiles_tiles_22_sword as u8
    {
        if tile_is_floor(get_tile_behind_char() as c_int) == 0 {
            return 0;
        }
        Char.x = char_dx_forward(-14) as u8;
        load_fram_det_col();
    }
    if get_tile_infrontof_char() as u8 == tiles_tiles_10_potion as u8
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
        let distance = get_edge_distance();
        if edge_type != EDGE_TYPE_FLOOR as u8 {
            Char.x = char_dx_forward(distance as c_int) as u8;
        }
        if Char.direction >= directions_dir_0_right as i8 {
            let adj = (curr_tile2 == tiles_tiles_10_potion as u8) as c_int - 2;
            Char.x = char_dx_forward(adj) as u8;
        }
        crouch();
    } else if curr_tile2 == tiles_tiles_22_sword as u8 {
        do_pickup(-1);
        seqtbl_offset_char(seqids_seq_91_get_sword as c_short);
    } else {
        // potion
        do_pickup((*curr_room_modif.add(curr_tilepos as usize) >> 3) as c_int);
        seqtbl_offset_char(seqids_seq_78_drink as c_short);

        // USE_COPYPROT
        if current_level == 15 {
            let mut index = 0i16;
            while index < 14 {
                if copyprot_room_read(index as usize) == curr_room as u16
                    && copyprot_tile_read(index as usize) == curr_tilepos as u16
                {
                    copyprot_room_write(index as usize, 0);
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
    } else if tile_is_floor(get_tile_behind_char() as c_int) == 0 {
        grab_up_no_floor_behind();
    } else {
        Char.x = char_dx_forward(distance as c_int - TILE_SIZEX as c_int) as u8;
        load_fram_det_col();
        grab_up_with_floor_behind();
    }
}

// seg005:08C7
#[no_mangle]
pub unsafe extern "C" fn grab_up_no_floor_behind() {
    get_tile_above_char();
    Char.x = char_dx_forward(distance_to_edge_weight() as c_int - 10) as u8;
    seqtbl_offset_char(seqids_seq_16_jump_up_and_grab as c_short);
}

// seg005:08E6
#[no_mangle]
pub unsafe extern "C" fn jump_up() {
    control_up = release_arrows() as i8;
    let distance = get_edge_distance();
    if distance < 4 && edge_type == EDGE_TYPE_WALL as u8 {
        Char.x = char_dx_forward(distance as c_int - 3) as u8;
    }

    // FIX_JUMP_DISTANCE_AT_EDGE
    if (*fixes).fix_jump_distance_at_edge != 0 && distance == 3
        && edge_type == EDGE_TYPE_CLOSER as u8
    {
        Char.x = char_dx_forward(-1) as u8;
    }

    // USE_SUPER_HIGH_JUMP
    let delta_x: u16 = if is_feather_fall != 0
        && tile_is_floor(get_tile_above_char() as c_int) == 0
        && curr_tile2 != tiles_tiles_20_wall as u8
    {
        if Char.direction == directions_dir_FF_left as i8 { 1 } else { 3 }
    } else {
        0
    };
    let char_col = get_tile_div_mod(
        back_delta_x(delta_x as c_int) + dx_weight() as c_int - 6,
    );
    get_tile(
        Char.room as c_int,
        char_col,
        Char.curr_row as c_int - 1,
    );
    if curr_tile2 != tiles_tiles_20_wall as u8 && tile_is_floor(curr_tile2 as c_int) == 0 {
        if (*fixes).enable_super_high_jump != 0 && is_feather_fall != 0 {
            if curr_room == 0 && Char.curr_row == 0 {
                seqtbl_offset_char(seqids_seq_14_jump_up_into_ceiling as c_short);
            } else {
                get_tile(Char.room as c_int, char_col, Char.curr_row as c_int - 2);
                let mut is_top_floor = tile_is_floor(curr_tile2 as c_int) != 0
                    || curr_tile2 == tiles_tiles_20_wall as u8;
                if is_top_floor
                    && curr_tile2 == tiles_tiles_11_loose as u8
                    && (*curr_room_tiles.add(curr_tilepos as usize) & 0x20) == 0
                {
                    is_top_floor = false;
                }
                super_jump_timer = if is_top_floor { 22 } else { 24 };
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
            || ((*fixes).enable_super_high_jump != 0
                && super_jump_fall != 0
                && control_y == CONTROL_HELD as i8)
        {
            // Hanging against a wall or doortop.
            if Char.action != actions_actions_6_hang_straight as u8
                && (get_tile_at_char() as u8 == tiles_tiles_20_wall as u8
                    || (Char.direction == directions_dir_FF_left as i8
                        && (curr_tile2 == tiles_tiles_7_doortop_with_floor as u8
                            || curr_tile2 == tiles_tiles_12_doortop as u8)))
            {
                if grab_timer == 0 {
                    play_sound(soundids_sound_8_bumped as c_int);
                }
                seqtbl_offset_char(seqids_seq_25_hang_against_wall as c_short);
            } else {
                if tile_is_floor(get_tile_above_char() as c_int) == 0 {
                    hang_fall();
                }
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
    let mut seq_id = seqids_seq_10_climb_up as c_short;
    control_up = release_arrows() as i8;
    control_shift2 = control_up;

    // USE_SUPER_HIGH_JUMP
    if (*fixes).enable_super_high_jump != 0 {
        super_jump_fall = 0;
    }

    get_tile_above_char();
    if ((curr_tile2 == tiles_tiles_13_mirror as u8 || curr_tile2 == tiles_tiles_18_chomper as u8)
        && Char.direction == directions_dir_0_right as i8)
        || (curr_tile2 == tiles_tiles_4_gate as u8
            && Char.direction != directions_dir_0_right as i8
            && (*curr_room_modif.add(curr_tilepos as usize)) >> 2 < 6)
    {
        seq_id = seqids_seq_73_climb_up_to_closed_gate as c_short;
    }
    seqtbl_offset_char(seq_id);
}

// seg005:0A46
#[no_mangle]
pub unsafe extern "C" fn hang_fall() {
    control_down = release_arrows() as i8;

    // USE_SUPER_HIGH_JUMP
    if (*fixes).enable_super_high_jump != 0 {
        super_jump_fall = 0;
    }

    if tile_is_floor(get_tile_behind_char() as c_int) == 0
        && tile_is_floor(get_tile_at_char() as c_int) == 0
    {
        seqtbl_offset_char(seqids_seq_23_release_ledge_and_fall as c_short);
    } else {
        if get_tile_at_char() as u8 == tiles_tiles_20_wall as u8
            || (Char.direction < directions_dir_0_right as i8
                && (curr_tile2 == tiles_tiles_7_doortop_with_floor as u8
                    || curr_tile2 == tiles_tiles_12_doortop as u8))
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
    let edge_distance = get_edge_distance();

    // Expand JUMP_STRAIGHT_CONDITION macro.
    let jump_straight = if (*fixes).fix_edge_distance_check_when_climbing != 0 {
        distance < 4 && edge_type != EDGE_TYPE_WALL as u8
    } else {
        distance < 4 && edge_distance < 4 && edge_type != EDGE_TYPE_WALL as u8
    };

    if jump_straight {
        Char.x = char_dx_forward(distance as c_int) as u8;
        seqtbl_offset_char(seqids_seq_8_jump_up_and_grab_straight as c_short);
    } else {
        Char.x = char_dx_forward(distance as c_int - 4) as u8;
        seqtbl_offset_char(seqids_seq_24_jump_up_and_grab_forward as c_short);
    }
}

// seg005:0AF7
#[no_mangle]
pub unsafe extern "C" fn run_jump() {
    if Char.frame >= frameids_frame_7_run as u8 {
        let xpos = char_dx_forward(4) as c_short;
        let mut col = get_tile_div_mod_m7(xpos as c_int) as c_short;
        let mut tiles_forward: c_short = 0;
        while tiles_forward < 2 {
            col += dir_front_at((Char.direction as i32 + 1) as usize) as c_short;
            get_tile(
                Char.room as c_int,
                col as c_int,
                Char.curr_row as c_int,
            );
            if curr_tile2 == tiles_tiles_2_spike as u8
                || tile_is_floor(curr_tile2 as c_int) == 0
            {
                let mut pos_adjustment = (distance_to_edge(xpos as c_int)
                    + TILE_SIZEX as c_int * tiles_forward as c_int
                    - TILE_SIZEX as c_int) as c_short;
                if (pos_adjustment as u16) < ((-8i16) as u16) || pos_adjustment >= 2 {
                    if pos_adjustment < 128 {
                        return;
                    }
                    pos_adjustment = -3;
                }
                Char.x = char_dx_forward(pos_adjustment as c_int + 4) as u8;
                break;
            }
            tiles_forward += 1;
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
    let mut seq_id = seqids_seq_55_draw_sword as c_short;
    control_forward = release_arrows() as i8;
    control_shift2 = control_forward;

    // FIX_UNINTENDED_SWORD_STRIKE
    if (*fixes).fix_unintended_sword_strike != 0 {
        ctrl1_shift2 = CONTROL_IGNORE as i8;
    }

    if Char.charid == charids_charid_0_kid as u8 {
        play_sound(soundids_sound_19_draw_sword as c_int);
        offguard = 0;
    } else if Char.charid != charids_charid_1_shadow as u8 {
        seq_id = seqids_seq_90_en_garde as c_short;
    }
    Char.sword = sword_status_sword_2_drawn as u8;
    seqtbl_offset_char(seq_id);
}

// seg005:0C67
#[no_mangle]
pub unsafe extern "C" fn control_with_sword() {
    if Char.action < actions_actions_2_hang_climb as u8 {
        if get_tile_at_char() as u8 == tiles_tiles_11_loose as u8
            || can_guard_see_kid >= 2
        {
            let distance = char_opp_dist();
            if (distance as u16) < 90u16 {
                swordfight();
                return;
            } else if distance < 0 {
                if (distance as u16) < ((-4i16) as u16) {
                    seqtbl_offset_char(seqids_seq_60_turn_with_sword as c_short);
                    return;
                } else {
                    swordfight();
                    return;
                }
            }
        }
        {
            if Char.charid == charids_charid_0_kid as u8 && Char.alive < 0 {
                holding_sword = 0;
            }
            if Char.charid < charids_charid_2_guard as u8 {
                if Char.frame == frameids_frame_171_stand_with_sword as u8 {
                    Char.sword = sword_status_sword_0_sheathed as u8;
                    seqtbl_offset_char(seqids_seq_92_put_sword_away as c_short);
                }
            } else {
                swordfight();
            }
        }
    }
}

// seg005:0CDB
#[no_mangle]
pub unsafe extern "C" fn swordfight() {
    let seq_id: c_short;
    let frame = Char.frame;
    let charid = Char.charid;

    if frame == frameids_frame_161_parry as u8 && control_shift2 >= 0 {
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
                seq_id = seqids_seq_93_put_sword_away_fast as c_short;
            } else if charid == charids_charid_1_shadow as u8 {
                seq_id = seqids_seq_92_put_sword_away as c_short;
            } else {
                seq_id = seqids_seq_87_guard_become_inactive as c_short;
            }
            seqtbl_offset_char(seq_id);
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
    let seq_id: c_short;
    let frame = Char.frame;
    if frame == frameids_frame_157_walk_with_sword as u8
        || frame == frameids_frame_158_stand_with_sword as u8
        || frame == frameids_frame_170_stand_with_sword as u8
        || frame == frameids_frame_171_stand_with_sword as u8
        || frame == frameids_frame_165_walk_with_sword as u8
    {
        if Char.charid == charids_charid_0_kid as u8 {
            seq_id = seqids_seq_75_strike as c_short;
        } else {
            seq_id = seqids_seq_58_guard_strike as c_short;
        }
    } else if frame == frameids_frame_150_parry as u8
        || frame == frameids_frame_161_parry as u8
    {
        seq_id = seqids_seq_66_strike_after_parry as c_short;
    } else {
        return;
    }
    control_shift2 = CONTROL_IGNORE as i8;
    seqtbl_offset_char(seq_id);
}

// seg005:0E0F
#[no_mangle]
pub unsafe extern "C" fn parry() {
    let char_frame = Char.frame;
    let opp_frame = Opp.frame;
    let char_charid = Char.charid;
    let mut seq_id = seqids_seq_62_parry as c_short;
    let mut do_play_seq: c_int = 0;

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
        seq_id = seqids_seq_61_parry_after_strike as c_short;
    }

    control_up = CONTROL_IGNORE as i8;
    seqtbl_offset_char(seq_id);
    if do_play_seq != 0 {
        play_seq();
    }
}

// seg005:1108 — guarded by USE_TELEPORTS (defined in config.h)
#[no_mangle]
pub unsafe extern "C" fn teleport() {
    let (found, dest_room, dest_tilepos) = 'search: {
        let mut dest_room_i = 1i32;
        while dest_room_i <= 24 {
            get_room_address(dest_room_i);
            let mut dest_tp = 0i32;
            while dest_tp < 30 {
                if dest_room_i == source_room && dest_tp == source_tilepos {
                    dest_tp += 1;
                    continue;
                }
                if get_curr_tile(dest_tp as c_short) as u8 == tiles_tiles_23_balcony_left as u8
                    && curr_modifier as c_int == source_modifier
                {
                    break 'search (true, dest_room_i, dest_tp);
                }
                dest_tp += 1;
            }
            dest_room_i += 1;
        }
        (false, 0i32, 0i32)
    };

    if found {
        Char.room = dest_room as u8;
        Char.curr_col = (dest_tilepos % 10) as i8;
        Char.curr_row = (dest_tilepos / 10) as i8;
        Char.x = (x_bump_at((Char.curr_col as i32 + 5) as usize) as i32 + 14 + 7) as u8;
        Char.y = y_land[(Char.curr_row as i32 + 1) as usize] as u8;
        next_room = Char.room as u16;
        clear_coll_rooms();
        // FIX_DISAPPEARING_GUARD_B is not defined — leave_guard() always called.
        leave_guard();
        // FIX_DISAPPEARING_GUARD_A is not defined — no drawn_room manipulation.
        seqtbl_offset_char(seqids_seq_5_turn as c_short);
        play_sound(soundids_sound_45_jump_through_mirror as c_int);
    } else {
        let msg = std::ffi::CString::new(
            format!(
                "Error: There is no other teleport with modifier {}.",
                pickup_obj_type
            )
        ).unwrap();
        show_dialog(msg.as_ptr());
        Char.x = (x_bump_at((Char.curr_col as i32 + 5) as usize) as i32 + 14) as u8;
        Char.y = y_land[(Char.curr_row as i32 + 1) as usize] as u8;
        seqtbl_offset_char(seqids_seq_17_soft_land as c_short);
        play_sound(soundids_sound_0_fell_to_death as c_int);
    }
}
