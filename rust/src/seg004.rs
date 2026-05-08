// Collision detection — ported from seg004.c.
// All 26 public functions are #[no_mangle] extern "C" for transparent C linkage.

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;

// ── File-private state ────────────────────────────────────────────────────────

static mut bump_col_left_of_wall:  i8  = 0;
static mut bump_col_right_of_wall: i8  = 0;
static mut right_checked_col:      i8  = 0;
static mut left_checked_col:       i8  = 0;
static mut coll_tile_left_xpos:    i16 = 0;

// Indexed by wall_type() return value (0–5).
const wall_dist_from_left:  [i8; 6] = [0, 10,  0, -1, 0, 0];
const wall_dist_from_right: [i8; 6] = [0,  0, 10, 13, 0, 0];

// ── Exported functions ────────────────────────────────────────────────────────

// seg004:0004
#[no_mangle]
pub unsafe extern "C" fn check_collisions() {
    bump_col_left_of_wall  = -1;
    bump_col_right_of_wall = -1;
    if Char.action == actions_actions_7_turn as u8 { return; }
    collision_row = Char.curr_row;
    move_coll_to_prev();
    prev_collision_row = collision_row;
    right_checked_col = (get_tile_div_mod_m7(char_x_right_coll as c_int) + 2).min(11) as i8;
    left_checked_col  = (get_tile_div_mod_m7(char_x_left_coll  as c_int) - 1)         as i8;
    get_row_collision_data(collision_row as c_short,
        curr_row_coll_room.as_mut_ptr(),  curr_row_coll_flags.as_mut_ptr());
    get_row_collision_data((collision_row as i16 + 1) as c_short,
        below_row_coll_room.as_mut_ptr(), below_row_coll_flags.as_mut_ptr());
    get_row_collision_data((collision_row as i16 - 1) as c_short,
        above_row_coll_room.as_mut_ptr(), above_row_coll_flags.as_mut_ptr());
    for column in (0..10i32).rev() {
        let col = column as usize;
        if curr_row_coll_room[col] >= 0
            && prev_coll_room[col] == curr_row_coll_room[col]
        {
            if (prev_coll_flags[col] & 0x0F) == 0
                && (curr_row_coll_flags[col] & 0x0F) != 0
            {
                bump_col_left_of_wall = column as i8;
            }
            if (prev_coll_flags[col] & 0xF0) == 0
                && (curr_row_coll_flags[col] & 0xF0) != 0
            {
                bump_col_right_of_wall = column as i8;
            }
        }
    }
}

// seg004:00DF
#[no_mangle]
pub unsafe extern "C" fn move_coll_to_prev() {
    let cr = collision_row as i32;
    let pr = prev_collision_row as i32;
    let source: u8 = if cr == pr || cr + 3 == pr || cr - 3 == pr {
        0 // curr
    } else if cr + 1 == pr || cr - 2 == pr {
        1 // above
    } else {
        2 // below
    };
    for col in 0..10usize {
        let (room_val, flags_val) = match source {
            0 => (curr_row_coll_room[col],  curr_row_coll_flags[col]),
            1 => (above_row_coll_room[col], above_row_coll_flags[col]),
            _ => (below_row_coll_room[col], below_row_coll_flags[col]),
        };
        prev_coll_room[col]       = room_val;
        prev_coll_flags[col]      = flags_val;
        below_row_coll_room[col]  = -1;
        above_row_coll_room[col]  = -1;
        curr_row_coll_room[col]   = -1;
        // FIX_COLL_FLAGS is disabled in config.h — skip the flag-reset blocks.
    }
}

// seg004:0185
#[no_mangle]
pub unsafe extern "C" fn get_row_collision_data(
    row: c_short,
    row_coll_room_ptr: *mut i8,
    row_coll_flags_ptr: *mut u8,
) {
    let room = Char.room as c_int;
    coll_tile_left_xpos =
        x_bump_at((left_checked_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i16
        + TILE_MIDX as i16;
    let mut col = left_checked_col as i32;
    while col <= right_checked_col as i32 {
        let left_wall_xpos  = get_left_wall_xpos(room, col, row as c_int);
        let right_wall_xpos = get_right_wall_xpos(room, col, row as c_int);
        let curr_flags =
            ((left_wall_xpos  < char_x_right_coll as c_int) as u8) * 0x0F
            | ((right_wall_xpos > char_x_left_coll  as c_int) as u8) * 0xF0;
        *row_coll_flags_ptr.add(tile_col as usize) = curr_flags;
        *row_coll_room_ptr.add(tile_col as usize)  = curr_room as i8;
        coll_tile_left_xpos += TILE_SIZEX as i16;
        col += 1;
    }
}

// seg004:0226
#[no_mangle]
pub unsafe extern "C" fn get_left_wall_xpos(room: c_int, column: c_int, row: c_int) -> c_int {
    let wtype = wall_type(get_tile(room, column, row) as u8) as i32;
    if wtype != 0 {
        wall_dist_from_left[wtype as usize] as i32 + coll_tile_left_xpos as i32
    } else {
        0xFF
    }
}

// seg004:025F
#[no_mangle]
pub unsafe extern "C" fn get_right_wall_xpos(room: c_int, column: c_int, row: c_int) -> c_int {
    let wtype = wall_type(get_tile(room, column, row) as u8) as i32;
    if wtype != 0 {
        coll_tile_left_xpos as i32 - wall_dist_from_right[wtype as usize] as i32 + TILE_RIGHTX as i32
    } else {
        0
    }
}

// seg004:029D
#[no_mangle]
pub unsafe extern "C" fn check_bumped() {
    if Char.action != actions_actions_2_hang_climb    as u8
        && Char.action != actions_actions_6_hang_straight as u8
        && (Char.frame < frameids_frame_135_climbing_1 as u8 || Char.frame >= 149)
    {
        // FIX_TWO_COLL_BUG is defined in config.h.
        if bump_col_left_of_wall >= 0 {
            check_bumped_look_right();
            if (*fixes).fix_two_coll_bug == 0 { return; }
        }
        if bump_col_right_of_wall >= 0 {
            check_bumped_look_left();
        }
    }
}

// seg004:02D2
#[no_mangle]
pub unsafe extern "C" fn check_bumped_look_left() {
    if (Char.sword == sword_status_sword_2_drawn as u8 || Char.direction < 0)
        && is_obstacle_at_col(bump_col_right_of_wall as c_int) != 0
    {
        // USE_JUMP_GRAB is defined in config.h.
        if (*fixes).enable_jump_grab != 0 && control_shift == CONTROL_HELD as i8 {
            if check_grab_run_jump() {
                return;
            }
            is_obstacle_at_col(bump_col_right_of_wall as c_int);
        }
        let xpos = get_right_wall_xpos(curr_room as c_int, tile_col as c_int, tile_row as c_int)
            - char_x_left_coll as c_int;
        bumped(xpos as i8, directions_dir_0_right as i8);
    }
}

// seg004:030A
#[no_mangle]
pub unsafe extern "C" fn check_bumped_look_right() {
    if (Char.sword == sword_status_sword_2_drawn as u8 || Char.direction == directions_dir_0_right as i8)
        && is_obstacle_at_col(bump_col_left_of_wall as c_int) != 0
    {
        // USE_JUMP_GRAB is defined in config.h.
        if (*fixes).enable_jump_grab != 0 && control_shift == CONTROL_HELD as i8 {
            if check_grab_run_jump() {
                return;
            }
            is_obstacle_at_col(bump_col_left_of_wall as c_int);
        }
        let xpos = get_left_wall_xpos(curr_room as c_int, tile_col as c_int, tile_row as c_int)
            - char_x_right_coll as c_int;
        bumped(xpos as i8, directions_dir_FF_left as i8);
    }
}

// seg004:0343
// The C parameter `tile_col` shadows the global; renamed to `col` here.
#[no_mangle]
pub unsafe extern "C" fn is_obstacle_at_col(col: c_int) -> c_int {
    let mut row = Char.curr_row as i32;
    if row < 0 { row += 3; }
    if row >= 3 { row -= 3; }
    get_tile(curr_row_coll_room[col as usize] as c_int, col, row);
    is_obstacle()
}

// seg004:037E
#[no_mangle]
pub unsafe extern "C" fn is_obstacle() -> c_int {
    if curr_tile2 == tiles_tiles_10_potion as u8 {
        return 0;
    } else if curr_tile2 == tiles_tiles_4_gate as u8 {
        if can_bump_into_gate() == 0 { return 0; }
    } else if curr_tile2 == tiles_tiles_18_chomper as u8 {
        if *curr_room_modif.add(curr_tilepos as usize) != 2 { return 0; }
    } else if curr_tile2 == tiles_tiles_13_mirror as u8
        && Char.charid == charids_charid_0_kid as u8
        && Char.frame >= frameids_frame_39_start_run_jump_6 as u8
        && Char.frame <  frameids_frame_44_running_jump_5  as u8
        && Char.direction < 0
    {
        *curr_room_modif.add(curr_tilepos as usize) = 0x56;
        jumped_through_mirror = -1;
        return 0;
    }
    coll_tile_left_xpos =
        xpos_in_drawn_room(x_bump_at((tile_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as c_int)
        as i16 + TILE_MIDX as i16;
    1
}

// seg004:0405
#[no_mangle]
pub unsafe extern "C" fn xpos_in_drawn_room(mut xpos: c_int) -> c_int {
    if curr_room as u16 != drawn_room {
        if curr_room as u16 == room_L || curr_room as u16 == room_BL {
            xpos -= (TILE_SIZEX * SCREEN_TILECOUNTX) as c_int;
        } else if curr_room as u16 == room_R || curr_room as u16 == room_BR {
            xpos += (TILE_SIZEX * SCREEN_TILECOUNTX) as c_int;
        }
    }
    xpos
}

// seg004:0448
#[no_mangle]
pub unsafe extern "C" fn bumped(delta_x: i8, push_direction: i8) {
    if Char.alive < 0 && Char.frame != frameids_frame_177_spiked as u8 {
        Char.x = Char.x.wrapping_add(delta_x as u8);
        if push_direction < 0 {
            // pushing left
            if curr_tile2 == tiles_tiles_20_wall as u8 {
                tile_col -= 1;
                get_tile(curr_room as c_int, tile_col as c_int, tile_row as c_int);
            }
        } else {
            // pushing right
            if curr_tile2 == tiles_tiles_12_doortop         as u8
                || curr_tile2 == tiles_tiles_7_doortop_with_floor as u8
                || curr_tile2 == tiles_tiles_20_wall              as u8
            {
                tile_col += 1;
                if curr_room == 0 && tile_col == 10 {
                    curr_room = Char.room as i16;
                    tile_col  = 0;
                }
                get_tile(curr_room as c_int, tile_col as c_int, tile_row as c_int);
            }
        }
        if tile_is_floor(curr_tile2 as c_int) != 0 {
            bumped_floor(push_direction);
        } else {
            bumped_fall();
        }
    }
}

// seg004:04E4
#[no_mangle]
pub unsafe extern "C" fn bumped_fall() {
    let action = Char.action;
    Char.x = char_dx_forward(-4) as u8;
    if action == actions_actions_4_in_freefall as u8 {
        Char.fall_x = 0;
    } else {
        seqtbl_offset_char(seqids_seq_45_bumpfall as c_short);
        play_seq();
    }
    bumped_sound();
}

// seg004:0520
#[no_mangle]
pub unsafe extern "C" fn bumped_floor(push_direction: i8) {
    let row_idx = (Char.curr_row as i32 + 1) as usize;
    if Char.sword != sword_status_sword_2_drawn as u8
        && (y_land[row_idx] as i16).wrapping_sub(Char.y as i16) as u16 >= 15
    {
        bumped_fall();
    } else {
        Char.y = y_land[row_idx] as u8;
        if Char.fall_y >= 22 {
            Char.x = char_dx_forward(-5) as u8;
        } else {
            Char.fall_y = 0;
            if Char.alive != 0 {
                let seq_index: i32;
                if Char.sword == sword_status_sword_2_drawn as u8 {
                    if push_direction == Char.direction {
                        seqtbl_offset_char(seqids_seq_65_bump_forward_with_sword as c_short);
                        play_seq();
                        Char.x = char_dx_forward(1) as u8;
                        return;
                    } else {
                        seq_index = seqids_seq_64_pushed_back_with_sword as i32;
                    }
                } else {
                    let frame = Char.frame as i32;
                    if frame == 24 || frame == 25
                        || (frame >= 40 && frame < 43)
                        || (frame >= frameids_frame_102_start_fall_1 as i32 && frame < 107)
                    {
                        seq_index = seqids_seq_46_hardbump as i32;
                    } else {
                        seq_index = seqids_seq_47_bump as i32;
                    }
                }
                seqtbl_offset_char(seq_index as c_short);
                play_seq();
                bumped_sound();
            }
        }
    }
}

// seg004:05F1
#[no_mangle]
pub unsafe extern "C" fn bumped_sound() {
    is_guard_notice = 1;
    play_sound(soundids_sound_8_bumped as c_int);
}

// seg004:0601
#[no_mangle]
pub unsafe extern "C" fn clear_coll_rooms() {
    prev_coll_room.fill(-1);
    curr_row_coll_room.fill(-1);
    below_row_coll_room.fill(-1);
    above_row_coll_room.fill(-1);
    // FIX_COLL_FLAGS disabled — skip flag array resets.
    prev_collision_row = -1;
}

// seg004:0657
#[no_mangle]
pub unsafe extern "C" fn can_bump_into_gate() -> c_int {
    ((*curr_room_modif.add(curr_tilepos as usize) >> 2) as i32 + 6
        < char_height as i32) as c_int
}

// seg004:067C
// The C function uses two goto labels; translated as flags to avoid control-flow
// gymnastics while preserving bit-equivalent behaviour.
#[no_mangle]
pub unsafe extern "C" fn get_edge_distance() -> c_int {
    let mut distance: c_int = 0;
    determine_col();
    load_frame_to_obj();
    set_char_collision();
    let mut tiletype = get_tile_at_char() as u8;

    let mut do_loc_59dd = false;
    let mut do_loc_59fb = false;

    if wall_type(tiletype) != 0 {
        tile_col = Char.curr_col as i16;
        distance = dist_from_wall_forward(tiletype);
        if distance >= 0 {
            do_loc_59dd = true;
        }
        // else: fall through to loc_59E8
    }

    if !do_loc_59dd {
        // loc_59E8:
        tiletype = get_tile_infrontof_char() as u8;
        if tiletype == tiles_tiles_12_doortop as u8 && Char.direction >= 0 {
            do_loc_59fb = true;
        } else {
            if wall_type(tiletype) != 0 {
                tile_col = infrontx as i16;
                distance = dist_from_wall_forward(tiletype);
                if distance >= 0 {
                    do_loc_59dd = true;
                }
            }
            if !do_loc_59dd {
                if tiletype == tiles_tiles_11_loose as u8 {
                    do_loc_59fb = true;
                } else if tiletype == tiles_tiles_6_closer  as u8
                    || tiletype == tiles_tiles_22_sword   as u8
                    || tiletype == tiles_tiles_10_potion  as u8
                {
                    distance = distance_to_edge_weight();
                    if distance != 0 {
                        edge_type = EDGE_TYPE_CLOSER as u8;
                    } else {
                        edge_type = EDGE_TYPE_FLOOR as u8;
                        distance  = 11;
                    }
                } else if tile_is_floor(tiletype as c_int) != 0 {
                    edge_type = EDGE_TYPE_FLOOR as u8;
                    distance  = 11;
                } else {
                    do_loc_59fb = true;
                }
            }
        }
    }

    if do_loc_59dd {
        // loc_59DD:
        if distance <= TILE_RIGHTX as c_int {
            edge_type = EDGE_TYPE_WALL as u8;
        } else {
            edge_type = EDGE_TYPE_FLOOR as u8;
            distance  = 11;
        }
    } else if do_loc_59fb {
        // loc_59FB:
        edge_type = EDGE_TYPE_CLOSER as u8;
        distance  = distance_to_edge_weight();
    }

    curr_tile2 = tiletype;
    distance
}

// seg004:076B
#[no_mangle]
pub unsafe extern "C" fn check_chomped_kid() {
    let row = Char.curr_row as i32;
    for col in 0..10i32 {
        if curr_row_coll_flags[col as usize] == 0xFF
            && get_tile(curr_row_coll_room[col as usize] as c_int, col, row) == tiles_tiles_18_chomper as c_int
            && (*curr_room_modif.add(curr_tilepos as usize) & 0x7F) == 2
        {
            chomped();
        }
    }
}

// seg004:07BF
#[no_mangle]
pub unsafe extern "C" fn chomped() {
    // FIX_SKELETON_CHOMPER_BLOOD defined in config.h.
    if !((*fixes).fix_skeleton_chomper_blood != 0
        && Char.charid == charids_charid_4_skeleton as u8)
    {
        *curr_room_modif.add(curr_tilepos as usize) |= 0x80;
    }
    if Char.frame != frameids_frame_178_chomped as u8 && Char.room as i16 == curr_room {
        // FIX_OFFSCREEN_GUARDS_DISAPPEARING defined in config.h.
        if (*fixes).fix_offscreen_guards_disappearing != 0 {
            let mut chomper_col = tile_col as i32;
            if curr_room != Char.room as i16 {
                let links = &level.roomlinks[(Char.room as usize) - 1];
                if curr_room as u8 == links.right {
                    chomper_col += SCREEN_TILECOUNTX as i32;
                } else if curr_room as u8 == links.left {
                    chomper_col -= SCREEN_TILECOUNTX as i32;
                }
            }
            Char.x = (x_bump_at((chomper_col + FIRST_ONSCREEN_COLUMN as i32) as usize) as i32
                + TILE_MIDX as i32) as u8;
        } else {
            Char.x = (x_bump_at((tile_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i32
                + TILE_MIDX as i32) as u8;
        }
        Char.x = char_dx_forward(7 - (Char.direction == 0) as c_int) as u8;
        Char.y = y_land[(Char.curr_row as i32 + 1) as usize] as u8;
        take_hp(100);
        play_sound(soundids_sound_46_chomped as c_int);
        seqtbl_offset_char(seqids_seq_54_chomped as c_short);
        play_seq();
    }
}

// seg004:0833
#[no_mangle]
pub unsafe extern "C" fn check_gate_push() {
    let frame = Char.frame as i32;
    if Char.action == actions_actions_7_turn as u8
        || frame == frameids_frame_15_stand as i32
        || (frame >= frameids_frame_108_fall_land_2 as i32 && frame < 111)
    {
        get_tile_at_char();
        let orig_col  = tile_col;
        let orig_room = curr_room;
        if (curr_tile2 == tiles_tiles_4_gate as u8
            || {
                tile_col -= 1;
                get_tile(curr_room as c_int, tile_col as c_int, tile_row as c_int)
                    == tiles_tiles_4_gate as c_int
            })
            && (curr_row_coll_flags[tile_col as usize] & prev_coll_flags[tile_col as usize]) == 0xFF
            && can_bump_into_gate() != 0
        {
            bumped_sound();
            // FIX_CAPED_PRINCE_SLIDING_THROUGH_GATE defined in config.h.
            if (*fixes).fix_caped_prince_sliding_through_gate != 0 {
                if curr_room as u8 == level.roomlinks[(orig_room as usize) - 1].right {
                    tile_col -= 10;
                    curr_room = orig_room;
                }
            }
            Char.x = Char.x.wrapping_add(
                (5i32 - (orig_col <= tile_col) as i32 * 10) as i8 as u8,
            );
        }
    }
}

// seg004:08C3
#[no_mangle]
pub unsafe extern "C" fn check_guard_bumped() {
    if Char.action == actions_actions_1_run_jump as u8
        && Char.alive < 0
        && Char.sword >= sword_status_sword_2_drawn as u8
    {
        // FIX_PUSH_GUARD_INTO_WALL defined in config.h.
        let behind_wall = (*fixes).fix_push_guard_into_wall != 0
            && get_tile_behind_char() == tiles_tiles_20_wall as c_int;
        if behind_wall
            || get_tile_at_char() == tiles_tiles_20_wall as c_int
            || curr_tile2 == tiles_tiles_7_doortop_with_floor as u8
            || (curr_tile2 == tiles_tiles_4_gate as u8 && can_bump_into_gate() != 0)
            || (Char.direction >= 0 && {
                tile_col -= 1;
                get_tile(curr_room as c_int, tile_col as c_int, tile_row as c_int)
                    == tiles_tiles_7_doortop_with_floor as c_int
                || (curr_tile2 == tiles_tiles_4_gate as u8 && can_bump_into_gate() != 0)
            })
        {
            load_frame_to_obj();
            set_char_collision();
            if is_obstacle() != 0 {
                let delta_x = dist_from_wall_behind(curr_tile2) as i16;
                if delta_x < 0 && delta_x > -13 {
                    Char.x = char_dx_forward(-delta_x as c_int) as u8;
                    seqtbl_offset_char(seqids_seq_65_bump_forward_with_sword as c_short);
                    play_seq();
                    load_fram_det_col();
                }
            }
        }
    }
}

// seg004:0989
#[no_mangle]
pub unsafe extern "C" fn check_chomped_guard() {
    get_tile_at_char();
    if check_chomped_here() == 0 {
        tile_col += 1;
        check_chomped_here();
    }
}

// seg004:09B0
#[no_mangle]
pub unsafe extern "C" fn check_chomped_here() -> c_int {
    if curr_tile2 == tiles_tiles_18_chomper as u8
        && (*curr_room_modif.add(curr_tilepos as usize) & 0x7F) == 2
    {
        coll_tile_left_xpos =
            x_bump_at((tile_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i16
            + TILE_MIDX as i16;
        if get_left_wall_xpos(curr_room as c_int, tile_col as c_int, tile_row as c_int)
               < char_x_right_coll as c_int
            && get_right_wall_xpos(curr_room as c_int, tile_col as c_int, tile_row as c_int)
               > char_x_left_coll as c_int
        {
            chomped();
            return 1;
        }
    }
    0
}

// seg004:0A10
#[no_mangle]
pub unsafe extern "C" fn dist_from_wall_forward(tiletype: u8) -> c_int {
    if tiletype == tiles_tiles_4_gate as u8 && can_bump_into_gate() == 0 {
        return -1;
    }
    coll_tile_left_xpos =
        x_bump_at((tile_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i16
        + TILE_MIDX as i16;
    let wtype = wall_type(tiletype) as i32;
    if wtype == 0 { return -1; }
    if Char.direction < 0 {
        // looking left
        char_x_left_coll as i32
            - (coll_tile_left_xpos as i32 + TILE_RIGHTX as i32
               - wall_dist_from_right[wtype as usize] as i32)
    } else {
        // looking right
        wall_dist_from_left[wtype as usize] as i32 + coll_tile_left_xpos as i32
            - char_x_right_coll as i32
    }
}

// seg004:0A7B
#[no_mangle]
pub unsafe extern "C" fn dist_from_wall_behind(tiletype: u8) -> c_int {
    let wtype = wall_type(tiletype) as i32;
    if wtype == 0 {
        return 99;
    }
    if Char.direction >= 0 {
        // looking right
        char_x_left_coll as i32
            - (coll_tile_left_xpos as i32 + TILE_RIGHTX as i32
               - wall_dist_from_right[wtype as usize] as i32)
    } else {
        // looking left
        wall_dist_from_left[wtype as usize] as i32 + coll_tile_left_xpos as i32
            - char_x_right_coll as i32
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(static_mut_refs)]
mod tests {
    use super::*;

    #[test]
    fn x_bump_readable_via_raw_pointer() {
        // x_bump is extern const byte x_bump[] — incomplete array, bindgen emits [u8; 0].
        // x_bump_at() must read through a raw pointer; any slice index would panic at runtime.
        // byte is Uint8, so the initialiser value -12 is stored as 244u8.
        unsafe {
            assert_eq!(x_bump_at(0),  244); // -12 as u8
            assert_eq!(x_bump_at(1),    2);
            assert_eq!(x_bump_at(2),   16);
            assert_eq!(x_bump_at(FIRST_ONSCREEN_COLUMN as usize), 58); // index 5
            assert_eq!(x_bump_at(19), 254); // last entry
        }
    }

    #[test]
    fn wall_dist_lookups_match_c_values() {
        assert_eq!(wall_dist_from_left,  [0, 10,  0, -1, 0, 0]);
        assert_eq!(wall_dist_from_right, [0,  0, 10, 13, 0, 0]);
    }

    #[test]
    #[allow(unused_assignments)] // writes to buf[0] are read via curr_room_modif (raw pointer alias)
    fn can_bump_into_gate_height_check() {
        unsafe {
            let mut buf = [0u8; 256];
            curr_room_modif = buf.as_mut_ptr();
            curr_tilepos = 0;

            // (modif >> 2) + 6 < char_height
            // 6 < 7 → true
            buf[0] = 0;
            char_height = 7;
            assert_eq!(can_bump_into_gate(), 1);

            // 6 < 6 → false
            char_height = 6;
            assert_eq!(can_bump_into_gate(), 0);

            // (4>>2)+6 = 7 < 8 → true
            buf[0] = 4;
            char_height = 8;
            assert_eq!(can_bump_into_gate(), 1);

            // (252>>2)+6 = 69 < 70 → true
            buf[0] = 252;
            char_height = 70;
            assert_eq!(can_bump_into_gate(), 1);

            // 69 < 69 → false
            char_height = 69;
            assert_eq!(can_bump_into_gate(), 0);
        }
    }

    #[test]
    fn clear_coll_rooms_resets_arrays() {
        unsafe {
            // Pre-populate with non-(-1) values.
            prev_coll_room.fill(5);
            curr_row_coll_room.fill(5);
            below_row_coll_room.fill(5);
            above_row_coll_room.fill(5);
            prev_collision_row = 2;

            clear_coll_rooms();

            assert!(prev_coll_room.iter().all(|&v| v == -1));
            assert!(curr_row_coll_room.iter().all(|&v| v == -1));
            assert!(below_row_coll_room.iter().all(|&v| v == -1));
            assert!(above_row_coll_room.iter().all(|&v| v == -1));
            assert_eq!(prev_collision_row, -1);
        }
    }

    #[test]
    fn bumped_sound_sets_guard_notice() {
        unsafe {
            is_guard_notice = 0;
            bumped_sound();
            assert_eq!(is_guard_notice, 1);
        }
    }

    #[test]
    fn xpos_in_drawn_room_identity() {
        unsafe {
            curr_room  = 5;
            drawn_room = 5;
            assert_eq!(xpos_in_drawn_room(100), 100);
        }
    }

    #[test]
    fn xpos_in_drawn_room_left_neighbour() {
        unsafe {
            // curr_room is room_L of drawn_room → subtract TILE_SIZEX * SCREEN_TILECOUNTX (140)
            curr_room  = 5;
            drawn_room = 9;
            room_L     = 5;
            room_R     = 99; // not matching
            room_BL    = 99;
            room_BR    = 99;
            assert_eq!(xpos_in_drawn_room(100), 100 - (TILE_SIZEX * SCREEN_TILECOUNTX) as i32);
        }
    }

    #[test]
    fn xpos_in_drawn_room_right_neighbour() {
        unsafe {
            curr_room  = 7;
            drawn_room = 9;
            room_L     = 99;
            room_R     = 7;
            room_BL    = 99;
            room_BR    = 99;
            assert_eq!(xpos_in_drawn_room(100), 100 + (TILE_SIZEX * SCREEN_TILECOUNTX) as i32);
        }
    }
}
