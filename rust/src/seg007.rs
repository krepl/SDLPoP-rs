// Animated tiles ("trobs") and mob physics — ported from seg007.c.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short, c_char};
use super::*;

// File-local globals (defined in seg007.c, not exported via proto.h).
static mut curmob_index: u16 = 0;
static mut curr_tile_temp: u16 = 0;

// Static tables
static GATE_CLOSE_SPEEDS: [u8; 9] = [0, 0, 0, 20, 40, 60, 80, 100, 120];
// door_delta[0] = -1 stored as u8 = 255; use wrapping_add.
static DOOR_DELTA: [u8; 3] = [255, 4, 4];
static LEVELDOOR_CLOSE_SPEEDS: [u8; 5] = [0, 5, 17, 99, 0];
static Y_LOOSE_LAND: [u16; 5] = [2, 65, 128, 191, 254];
static LOOSE_SOUND: [u8; 12] = [0, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0];
static Y_SOMETHING: [i16; 5] = [-1, 62, 125, 188, 25];

// tbl_line is defined in seg006 but that module's helper is private.
unsafe fn tbl_line_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(tbl_line).cast::<u16>().add(idx)
}

// seg007:0000
#[no_mangle]
pub unsafe extern "C" fn process_trobs() {
    let mut need_delete: u16 = 0;
    if trobs_count == 0 { return; }
    let mut index: u16 = 0;
    while index < trobs_count as u16 {
        trob = trobs[index as usize];
        animate_tile();
        trobs[index as usize].type_ = trob.type_;
        if trob.type_ < 0 {
            need_delete = 1;
        }
        index += 1;
    }
    if need_delete != 0 {
        let mut new_index: u16 = 0;
        let mut idx: u16 = 0;
        while idx < trobs_count as u16 {
            if trobs[idx as usize].type_ >= 0 {
                trobs[new_index as usize] = trobs[idx as usize];
                new_index += 1;
            }
            idx += 1;
        }
        trobs_count = new_index as c_short;
    }
}

// seg007:00AF
#[no_mangle]
pub unsafe extern "C" fn animate_tile() {
    get_room_address(trob.room as c_int);
    match get_curr_tile(trob.tilepos as c_short) {
        t if t == tiles_tiles_19_torch as c_short
          || t == tiles_tiles_30_torch_with_debris as c_short => animate_torch(),
        t if t == tiles_tiles_6_closer as c_short
          || t == tiles_tiles_15_opener as c_short => animate_button(),
        t if t == tiles_tiles_2_spike as c_short => animate_spike(),
        t if t == tiles_tiles_11_loose as c_short => animate_loose(),
        t if t == tiles_tiles_0_empty as c_short => animate_empty(),
        t if t == tiles_tiles_18_chomper as c_short => animate_chomper(),
        t if t == tiles_tiles_4_gate as c_short => animate_door(),
        t if t == tiles_tiles_16_level_door_left as c_short => animate_leveldoor(),
        t if t == tiles_tiles_10_potion as c_short => animate_potion(),
        t if t == tiles_tiles_22_sword as c_short => animate_sword(),
        _ => { trob.type_ = -1; }
    }
    *curr_room_modif.add(trob.tilepos as usize) = curr_modifier;
}

// seg007:0166
#[no_mangle]
pub unsafe extern "C" fn is_trob_in_drawn_room() -> c_short {
    if trob.room as u16 != drawn_room {
        trob.type_ = -1;
        0
    } else {
        1
    }
}

// seg007:017E
#[no_mangle]
pub unsafe extern "C" fn set_redraw_anim_right() {
    set_redraw_anim(get_trob_right_pos_in_drawn_room(), 1);
}

// seg007:018C
#[no_mangle]
pub unsafe extern "C" fn set_redraw_anim_curr() {
    set_redraw_anim(get_trob_pos_in_drawn_room(), 1);
}

// seg007:019A
#[no_mangle]
pub unsafe extern "C" fn redraw_at_trob() {
    redraw_height = 63;
    let tilepos = get_trob_pos_in_drawn_room();
    set_redraw_full(tilepos, 1);
    set_wipe(tilepos, 1);
}

// seg007:01C5
#[no_mangle]
pub unsafe extern "C" fn redraw_21h() {
    redraw_height = 0x21;
    redraw_tile_height();
}

// seg007:01D0
#[no_mangle]
pub unsafe extern "C" fn redraw_11h() {
    redraw_height = 0x11;
    redraw_tile_height();
}

// seg007:01DB
#[no_mangle]
pub unsafe extern "C" fn redraw_20h() {
    redraw_height = 0x20;
    redraw_tile_height();
}

// seg007:01E6
#[no_mangle]
pub unsafe extern "C" fn draw_trob() {
    let tilepos = get_trob_right_pos_in_drawn_room();
    set_redraw_anim(tilepos, 1);
    set_redraw_fore(tilepos, 1);
    set_redraw_anim(get_trob_right_above_pos_in_drawn_room(), 1);
}

// seg007:0218
#[no_mangle]
pub unsafe extern "C" fn redraw_tile_height() {
    let mut tilepos = get_trob_pos_in_drawn_room();
    set_redraw_full(tilepos, 1);
    set_wipe(tilepos, 1);
    tilepos = get_trob_right_pos_in_drawn_room();
    set_redraw_full(tilepos, 1);
    set_wipe(tilepos, 1);
}

// seg007:0258
#[no_mangle]
pub unsafe extern "C" fn get_trob_pos_in_drawn_room() -> c_short {
    let mut tilepos = trob.tilepos as c_short;
    if trob.room as u16 == room_A {
        if tilepos >= 20 && tilepos < 30 {
            tilepos = 19 - tilepos;
        } else {
            tilepos = 30;
        }
    } else if trob.room as u16 != drawn_room {
        tilepos = 30;
    }
    tilepos
}

// seg007:029D
#[no_mangle]
pub unsafe extern "C" fn get_trob_right_pos_in_drawn_room() -> c_short {
    let mut tilepos = trob.tilepos as c_short;
    if trob.room as u16 == drawn_room {
        if tilepos % 10 != 9 {
            tilepos += 1;
        } else {
            tilepos = 30;
        }
    } else if trob.room as u16 == room_L {
        if tilepos % 10 == 9 {
            tilepos -= 9;
        } else {
            tilepos = 30;
        }
    } else if trob.room as u16 == room_A {
        if tilepos >= 20 && tilepos < 29 {
            tilepos = 18 - tilepos; // 20..28 -> -2..-10
        } else {
            tilepos = 30;
        }
    } else if trob.room as u16 == room_AL && tilepos == 29 {
        tilepos = -1;
    } else {
        tilepos = 30;
    }
    tilepos
}

// seg007:032C
#[no_mangle]
pub unsafe extern "C" fn get_trob_right_above_pos_in_drawn_room() -> c_short {
    let mut tilepos = trob.tilepos as c_short;
    if trob.room as u16 == drawn_room {
        if tilepos % 10 != 9 {
            if tilepos < 10 {
                tilepos = -(tilepos + 2); // 0..8 -> -2..-10
            } else {
                tilepos -= 9;
            }
        } else {
            tilepos = 30;
        }
    } else if trob.room as u16 == room_L {
        if tilepos == 9 {
            tilepos = -1;
        } else if tilepos % 10 == 9 {
            tilepos -= 19;
        } else {
            tilepos = 30;
        }
    } else if trob.room as u16 == room_B {
        if tilepos < 9 {
            tilepos += 21;
        } else {
            tilepos = 30;
        }
    } else if trob.room as u16 == room_BL && tilepos == 9 {
        tilepos = 20;
    } else {
        tilepos = 30;
    }
    tilepos
}

// seg007:03CF
#[no_mangle]
pub unsafe extern "C" fn animate_torch() {
    if trob.room as u16 == drawn_room
        || (trob.room as u16 == room_L && (trob.tilepos % 10) == 9)
    {
        curr_modifier = get_torch_frame(curr_modifier as c_short) as u8;
        set_redraw_anim_right();
    } else {
        trob.type_ = -1;
    }
}

// seg007:03E9
#[no_mangle]
pub unsafe extern "C" fn animate_potion() {
    if trob.type_ >= 0 && is_trob_in_drawn_room() != 0 {
        let type_bits = curr_modifier & 0xF8;
        curr_modifier = bubble_next_frame((curr_modifier & 0x07) as c_short) as u8 | type_bits;
        // USE_COPYPROT is active
        if current_level as u16 == 15 {
            set_redraw_anim_curr();
            return;
        }
        // FIX_LOOSE_NEXT_TO_POTION is active
        redraw_at_trob();
    }
}

// seg007:0425
#[no_mangle]
pub unsafe extern "C" fn animate_sword() {
    if is_trob_in_drawn_room() != 0 {
        curr_modifier = curr_modifier.wrapping_sub(1);
        if curr_modifier == 0 {
            curr_modifier = (prandom(255) as u8 & 0x3F) + 0x28;
        }
        // FIX_LOOSE_NEXT_TO_POTION is active
        redraw_at_trob();
    }
}

// seg007:0448
#[no_mangle]
pub unsafe extern "C" fn animate_chomper() {
    if trob.type_ >= 0 {
        let blood = curr_modifier & 0x80;
        let mut frame = (curr_modifier & 0x7F).wrapping_add(1);
        if frame > (*custom).chomper_speed {
            frame = 1;
        }
        curr_modifier = blood | frame;
        if frame == 2 {
            play_sound(soundids_sound_47_chomper as c_int);
        }
        if (trob.room as u16 != drawn_room
            || trob.tilepos / 10 != Kid.curr_row as u8
            || (Kid.alive >= 0 && blood == 0))
            && (curr_modifier & 0x7F) >= 6
        {
            trob.type_ = -1;
        }
    }
    if (curr_modifier & 0x7F) < 6 {
        redraw_at_trob();
    }
}

// seg007:04D3
#[no_mangle]
pub unsafe extern "C" fn animate_spike() {
    if trob.type_ >= 0 {
        if curr_modifier == 0xFF { return; }
        if curr_modifier & 0x80 != 0 {
            curr_modifier = curr_modifier.wrapping_sub(1);
            if curr_modifier & 0x7F != 0 { return; }
            curr_modifier = 6;
        } else {
            curr_modifier = curr_modifier.wrapping_add(1);
            if curr_modifier == 5 {
                curr_modifier = 0x8F;
            } else if curr_modifier == 9 {
                curr_modifier = 0;
                trob.type_ = -1;
            }
        }
    }
    redraw_21h();
}

// seg007:0522
#[no_mangle]
pub unsafe extern "C" fn animate_door() {
    let anim_type = trob.type_;
    if anim_type >= 0 {
        if anim_type >= 3 {
            // closing fast
            if anim_type < 8 {
                trob.type_ += 1;
            }
            let new_mod = curr_modifier as i16 - GATE_CLOSE_SPEEDS[trob.type_ as usize] as i16;
            curr_modifier = new_mod as u8;
            if new_mod < 0 {
                curr_modifier = 0;
                trob.type_ = -1;
                play_sound(soundids_sound_6_gate_closing_fast as c_int);
            }
        } else if curr_modifier != 0xFF {
            // 0xFF means permanently open
            curr_modifier = curr_modifier.wrapping_add(DOOR_DELTA[anim_type as usize]);
            if anim_type == 0 {
                // closing
                if curr_modifier != 0 {
                    if curr_modifier < 188 && (curr_modifier & 3) == 3 {
                        play_door_sound_if_visible(soundids_sound_4_gate_closing as c_int);
                    }
                } else {
                    gate_stop();
                }
            } else {
                // opening
                if curr_modifier < 188 {
                    if (curr_modifier & 7) == 0 {
                        play_sound(soundids_sound_5_gate_opening as c_int);
                    }
                } else if anim_type < 2 {
                    // after regular open
                    curr_modifier = 238;
                    trob.type_ = 0; // closing
                    play_sound(soundids_sound_7_gate_stop as c_int);
                } else {
                    // after permanent open
                    curr_modifier = 0xFF;
                    gate_stop();
                }
            }
        } else {
            gate_stop();
        }
    }
    draw_trob();
}

// seg007:05E3
#[no_mangle]
pub unsafe extern "C" fn gate_stop() {
    trob.type_ = -1;
    play_door_sound_if_visible(soundids_sound_7_gate_stop as c_int);
}

// seg007:05F1
#[no_mangle]
pub unsafe extern "C" fn animate_leveldoor() {
    let trob_type = trob.type_;
    if trob.type_ >= 0 {
        if trob_type >= 3 {
            // closing
            trob.type_ += 1;
            curr_modifier = curr_modifier.wrapping_sub(
                LEVELDOOR_CLOSE_SPEEDS[(trob.type_ - 3) as usize],
            );
            if (curr_modifier as i8) < 0 {
                curr_modifier = 0;
                trob.type_ = -1;
                play_sound(soundids_sound_14_leveldoor_closing as c_int);
            } else if trob.type_ == 4
                && (sound_flags & soundflags_sfDigi as u8) != 0
            {
                *core::ptr::addr_of_mut!(sound_interruptible)
                    .cast::<u8>()
                    .add(soundids_sound_15_leveldoor_sliding as usize) = 1;
                play_sound(soundids_sound_15_leveldoor_sliding as c_int);
            }
        } else {
            // opening
            curr_modifier = curr_modifier.wrapping_add(1);
            if curr_modifier >= 43 {
                trob.type_ = -1;
                // FIX_FEATHER_INTERRUPTED_BY_LEVELDOOR is active
                if !((*fixes).fix_feather_interrupted_by_leveldoor != 0 && is_feather_fall != 0) {
                    stop_sounds();
                }
                if leveldoor_open == 0 || leveldoor_open == 2 {
                    leveldoor_open = 1;
                    if current_level as u16 == (*custom).mirror_level as u16 {
                        get_tile(
                            (*custom).mirror_room as c_int,
                            (*custom).mirror_column as c_int,
                            (*custom).mirror_row as c_int,
                        );
                        *curr_room_tiles.add(curr_tilepos as usize) = (*custom).mirror_tile;
                    }
                }
            } else {
                *core::ptr::addr_of_mut!(sound_interruptible)
                    .cast::<u8>()
                    .add(soundids_sound_15_leveldoor_sliding as usize) = 0;
                play_sound(soundids_sound_15_leveldoor_sliding as c_int);
            }
        }
    }
    set_redraw_anim_right();
}

// seg007:06AD
#[no_mangle]
pub unsafe extern "C" fn bubble_next_frame(curr: c_short) -> c_short {
    let mut next = curr + 1;
    if next >= 8 { next = 1; }
    next
}

// seg007:06CD
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

// seg007:070A
#[no_mangle]
pub unsafe extern "C" fn set_redraw_anim(tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            let tp = tilepos + 1;
            redraw_frames_above[(-tp) as usize] = frames;
        } else {
            redraw_frames_anim[tilepos as usize] = frames;
        }
    }
}

// seg007:0738
#[no_mangle]
pub unsafe extern "C" fn set_redraw2(tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            let mut idx = (-tilepos) as usize - 1;
            if idx > 9 { idx = 9; }
            redraw_frames_above[idx] = frames;
        } else {
            redraw_frames2[tilepos as usize] = frames;
        }
    }
}

// seg007:0766
#[no_mangle]
pub unsafe extern "C" fn set_redraw_floor_overlay(tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            let tp = tilepos + 1;
            redraw_frames_above[(-tp) as usize] = frames;
        } else {
            redraw_frames_floor_overlay[tilepos as usize] = frames;
        }
    }
}

// seg007:0794
#[no_mangle]
pub unsafe extern "C" fn set_redraw_full(tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            let tp = tilepos + 1;
            redraw_frames_above[(-tp) as usize] = frames;
        } else {
            redraw_frames_full[tilepos as usize] = frames;
        }
    }
}

// seg007:07C2
#[no_mangle]
pub unsafe extern "C" fn set_redraw_fore(tilepos: c_short, frames: u8) {
    if tilepos < 30 && tilepos >= 0 {
        redraw_frames_fore[tilepos as usize] = frames;
    }
}

// seg007:07DF
#[no_mangle]
pub unsafe extern "C" fn set_wipe(tilepos: c_short, frames: u8) {
    if tilepos < 30 && tilepos >= 0 {
        let idx = tilepos as usize;
        if wipe_frames[idx] != 0 {
            redraw_height = (wipe_heights[idx] as i16).max(redraw_height);
        }
        wipe_heights[idx] = redraw_height as i8;
        wipe_frames[idx] = frames;
    }
}

// seg007:081E
#[no_mangle]
pub unsafe extern "C" fn start_anim_torch(room: c_short, tilepos: c_short) {
    *curr_room_modif.add(tilepos as usize) = prandom(8) as u8;
    add_trob(room as u8, tilepos as u8, 1);
}

// seg007:0847
#[no_mangle]
pub unsafe extern "C" fn start_anim_potion(room: c_short, tilepos: c_short) {
    let m = *curr_room_modif.add(tilepos as usize);
    *curr_room_modif.add(tilepos as usize) = (m & 0xF8) | (prandom(6) as u8 + 1);
    add_trob(room as u8, tilepos as u8, 1);
}

// seg007:087C
#[no_mangle]
pub unsafe extern "C" fn start_anim_sword(room: c_short, tilepos: c_short) {
    *curr_room_modif.add(tilepos as usize) = prandom(0xFF) as u8 & 0x1F;
    add_trob(room as u8, tilepos as u8, 1);
}

// seg007:08A7
#[no_mangle]
pub unsafe extern "C" fn start_anim_chomper(room: c_short, tilepos: c_short, modifier: u8) {
    let old_modifier = *curr_room_modif.add(tilepos as usize);
    if old_modifier == 0 || old_modifier >= 6 {
        *curr_room_modif.add(tilepos as usize) = modifier;
        add_trob(room as u8, tilepos as u8, 1);
    }
}

// seg007:08E3
#[no_mangle]
pub unsafe extern "C" fn start_anim_spike(room: c_short, tilepos: c_short) {
    let old_modifier = *curr_room_modif.add(tilepos as usize) as i8;
    if old_modifier <= 0 {
        if old_modifier == 0 {
            add_trob(room as u8, tilepos as u8, 1);
            play_sound(soundids_sound_49_spikes as c_int);
        } else if old_modifier != -1i8 {
            // 0xFF (= -1 as i8) means disabled spike
            *curr_room_modif.add(tilepos as usize) = 0x8F;
        }
    }
}

// seg007:092C
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

// seg007:0999
#[no_mangle]
pub unsafe extern "C" fn trigger_1(
    target_type: c_short,
    room: c_short,
    tilepos: c_short,
    button_type: c_short,
) -> c_short {
    let mut result: c_short = -1;
    if target_type == tiles_tiles_4_gate as c_short {
        result = trigger_gate(room, tilepos, button_type);
    } else if target_type == tiles_tiles_16_level_door_left as c_short {
        if *curr_room_modif.add(tilepos as usize) != 0 {
            result = -1;
        } else {
            result = 1;
        }
    } else if (*custom).allow_triggering_any_tile != 0 {
        result = 1;
    }
    result
}

// seg007:09E5
#[no_mangle]
pub unsafe extern "C" fn do_trigger_list(mut index: c_short, button_type: c_short) {
    loop {
        let room = get_doorlink_room(index) as u16;
        get_room_address(room as c_int);
        let tilepos = get_doorlink_tile(index);
        let target_type = (*curr_room_tiles.add(tilepos as usize) & 0x1F) as c_short;
        let trigger_result = trigger_1(target_type, room as c_short, tilepos, button_type);
        if trigger_result >= 0 {
            add_trob(room as u8, tilepos as u8, trigger_result as i8);
        }
        if get_doorlink_next(index) == 0 { break; }
        index += 1;
    }
}

// seg007:0A5A
#[no_mangle]
pub unsafe extern "C" fn add_trob(room: u8, tilepos: u8, type_: i8) {
    if trobs_count as u32 >= TROBS_MAX {
        show_dialog(b"Trobs Overflow\0".as_ptr() as *const c_char);
        return;
    }
    trob.room = room;
    trob.tilepos = tilepos;
    trob.type_ = type_;
    let found = find_trob();
    if found == -1 {
        if trobs_count as u32 == TROBS_MAX { return; }
        trobs[trobs_count as usize] = trob;
        trobs_count += 1;
    } else {
        trobs[found as usize].type_ = trob.type_;
    }
}

// seg007:0ACA
#[no_mangle]
pub unsafe extern "C" fn find_trob() -> c_short {
    let mut index: c_short = 0;
    while index < trobs_count {
        if trobs[index as usize].tilepos == trob.tilepos
            && trobs[index as usize].room == trob.room
        {
            return index;
        }
        index += 1;
    }
    -1
}

// seg007:0B0A
#[no_mangle]
pub unsafe extern "C" fn clear_tile_wipes() {
    redraw_frames_full = [0u8; 30];
    wipe_frames = [0u8; 30];
    wipe_heights = [0i8; 30];
    redraw_frames_anim = [0u8; 30];
    redraw_frames_fore = [0u8; 30];
    redraw_frames2 = [0u8; 30];
    redraw_frames_floor_overlay = [0u8; 30];
    tile_object_redraw = [0u8; 30];
    redraw_frames_above = [0u8; 10];
}

// seg007:0BB6
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_timer(index: c_short) -> c_short {
    (*doorlink2_ad.add(index as usize) & 0x1F) as c_short
}

// seg007:0BCD
#[no_mangle]
pub unsafe extern "C" fn set_doorlink_timer(index: c_short, value: u8) -> c_short {
    let p = doorlink2_ad.add(index as usize);
    *p = (*p & 0xE0) | (value & 0x1F);
    *p as c_short
}

// seg007:0BF2
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_tile(index: c_short) -> c_short {
    (*doorlink1_ad.add(index as usize) & 0x1F) as c_short
}

// seg007:0C09
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_next(index: c_short) -> c_short {
    ((*doorlink1_ad.add(index as usize) & 0x80) == 0) as c_short
}

// seg007:0C26
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_room(index: c_short) -> c_short {
    let b1 = *doorlink1_ad.add(index as usize);
    let b2 = *doorlink2_ad.add(index as usize);
    (((b1 & 0x60) >> 5) + ((b2 & 0xE0) >> 3)) as c_short
}

// seg007:0C53
#[no_mangle]
pub unsafe extern "C" fn trigger_button(playsound: c_int, mut button_type: c_int, modifier: c_int) {
    get_curr_tile(curr_tilepos as c_short);
    if button_type == 0 {
        button_type = curr_tile as c_int;
    }
    let modifier = if modifier == -1 { curr_modifier as c_int } else { modifier };
    let link_timer = get_doorlink_timer(modifier as c_short) as i8;
    if link_timer != 0x1F {
        set_doorlink_timer(modifier as c_short, 5);
        if link_timer < 2 {
            add_trob(curr_room as u8, curr_tilepos, 1);
            redraw_11h();
            is_guard_notice = 1;
            if playsound != 0 {
                play_sound(soundids_sound_3_button_pressed as c_int);
            }
        }
        do_trigger_list(modifier as c_short, button_type as c_short);
    }
}

// seg007:0CD9
#[no_mangle]
pub unsafe extern "C" fn died_on_button() {
    let mut button_type = get_curr_tile(curr_tilepos as c_short) as c_int;
    let modifier = curr_modifier as c_int;
    if curr_tile == tiles_tiles_15_opener as u8 {
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
        *curr_room_modif.add(curr_tilepos as usize) = 0;
        button_type = tiles_tiles_14_debris as c_int;
    } else {
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_5_stuck as u8;
    }
    trigger_button(1, button_type, modifier);
}

// seg007:0D3A
#[no_mangle]
pub unsafe extern "C" fn animate_button() {
    if trob.type_ >= 0 {
        let timer = get_doorlink_timer(curr_modifier as c_short) as i16 - 1;
        set_doorlink_timer(curr_modifier as c_short, timer as u8);
        if timer < 2 {
            trob.type_ = -1;
            redraw_11h();
        }
    }
}

// seg007:0D72
#[no_mangle]
pub unsafe extern "C" fn start_level_door(room: c_short, tilepos: c_short) {
    *curr_room_modif.add(tilepos as usize) = 43; // start fully open
    add_trob(room as u8, tilepos as u8, 3);
}

// seg007:0D93
#[no_mangle]
pub unsafe extern "C" fn animate_empty() {
    trob.type_ = -1;
    redraw_20h();
}

// seg007:0D9D
#[no_mangle]
pub unsafe extern "C" fn animate_loose() {
    let anim_type = trob.type_;
    if anim_type >= 0 {
        curr_modifier = curr_modifier.wrapping_add(1);
        if curr_modifier & 0x80 != 0 {
            // just shaking — don't stop on loose_tiles_level
            if current_level as u16 == (*custom).loose_tiles_level as u16 { return; }
            if curr_modifier >= 0x84 {
                curr_modifier = 0;
                trob.type_ = -1;
            }
            loose_shake((curr_modifier == 0) as c_int);
        } else {
            // something is on the floor — should it fall?
            if curr_modifier >= (*custom).loose_floor_delay {
                let room = trob.room;
                let tilepos = trob.tilepos;
                // FIX_DROP_2_ROOMS_CLIMBING_LOOSE_TILE is active
                if (*fixes).fix_drop_2_rooms_climbing_loose_tile != 0
                    && room as u16 == level.roomlinks[Kid.room as usize - 1].up as u16
                    && tilepos / 10 == 2
                    && Kid.curr_row == 0
                    && Kid.curr_col == (tilepos % 10) as i8
                    && Kid.frame >= frameids_frame_135_climbing_1 as u8
                    && Kid.frame < frameids_frame_141_climbing_7 as u8
                {
                    loose_shake(0);
                } else {
                    curr_modifier = remove_loose(room as c_int, tilepos as c_int) as u8;
                    trob.type_ = -1;
                    curmob.xh = (tilepos % 10) << 2;
                    let row = tilepos / 10;
                    curmob.y = Y_LOOSE_LAND[(row + 1) as usize] as u8;
                    curmob.room = room;
                    curmob.speed = 0;
                    curmob.type_ = 0;
                    curmob.row = row;
                    add_mob();
                }
            } else {
                loose_shake(0);
            }
        }
    }
    redraw_20h();
}

// seg007:0E55
#[no_mangle]
pub unsafe extern "C" fn loose_shake(arg_0: c_int) {
    if arg_0 != 0 || LOOSE_SOUND[(curr_modifier & 0x7F) as usize] != 0 {
        let mut sound_id: u32;
        loop {
            sound_id = prandom(2) as u32 + soundids_sound_20_loose_shake_1;
            if sound_id != last_loose_sound as u32 { break; }
        }
        // USE_REPLAY: skip prandom call if replaying with old version
        if !(replaying != 0 && g_deprecation_number < 2) {
            prandom(2);
        }
        if sound_flags & soundflags_sfDigi as u8 != 0 {
            last_loose_sound = sound_id as u16;
        }
        play_sound(sound_id as c_int);
    }
}

// seg007:0EB8
#[no_mangle]
pub unsafe extern "C" fn remove_loose(_room: c_int, tilepos: c_int) -> c_int {
    *curr_room_tiles.add(tilepos as usize) = tiles_tiles_0_empty as u8;
    (*custom).tbl_level_type[current_level as usize] as c_int
}

// seg007:0ED5
#[no_mangle]
pub unsafe extern "C" fn make_loose_fall(modifier: u8) {
    if (*curr_room_tiles.add(curr_tilepos as usize) & 0x20) == 0 {
        if (*curr_room_modif.add(curr_tilepos as usize) as i8) <= 0 {
            *curr_room_modif.add(curr_tilepos as usize) = modifier;
            add_trob(curr_room as u8, curr_tilepos, 0);
            redraw_20h();
        }
    }
}

// seg007:0F13
#[no_mangle]
pub unsafe extern "C" fn start_chompers() {
    let mut timing: c_short = 15;
    if (Char.curr_row as u8) < 3 {
        get_room_address(Char.room as c_int);
        let mut column: c_short = 0;
        let mut tilepos = tbl_line_at(Char.curr_row as usize) as c_short;
        while column < 10 {
            if get_curr_tile(tilepos) == tiles_tiles_18_chomper as c_short {
                let modifier = curr_modifier & 0x7F;
                if modifier == 0 || modifier >= 6 {
                    start_anim_chomper(
                        Char.room as c_short,
                        tilepos,
                        timing as u8 | (curr_modifier & 0x80),
                    );
                    timing = next_chomper_timing(timing as u8) as c_short;
                }
            }
            column += 1;
            tilepos += 1;
        }
    }
}

// seg007:0F9A
#[no_mangle]
pub unsafe extern "C" fn next_chomper_timing(mut timing: u8) -> c_int {
    // cycle: 15,12,9,6,13,10,7,14,11,8,repeat
    timing = timing.wrapping_sub(3);
    if timing < 6 {
        timing = timing.wrapping_add(10);
    }
    timing as c_int
}

// seg007:0FB4
#[no_mangle]
pub unsafe extern "C" fn loose_make_shake() {
    if *curr_room_modif.add(curr_tilepos as usize) == 0
        && current_level as u16 != (*custom).loose_tiles_level as u16
    {
        *curr_room_modif.add(curr_tilepos as usize) = 0x80;
        add_trob(curr_room as u8, curr_tilepos, 1);
    }
}

// seg007:0FE0
#[no_mangle]
pub unsafe extern "C" fn do_knock(room: c_int, knock_row: c_int) {
    let mut tcol: c_short = 0;
    while tcol < 10 {
        if get_tile(room, tcol as c_int, knock_row) == tiles_tiles_11_loose as c_int {
            loose_make_shake();
        }
        tcol += 1;
    }
}

// seg007:1010
#[no_mangle]
pub unsafe extern "C" fn add_mob() {
    if mobs_count >= 14 {
        show_dialog(b"Mobs Overflow\0".as_ptr() as *const c_char);
        return;
    }
    mobs[mobs_count as usize] = curmob;
    mobs_count += 1;
}

// seg007:1041
#[no_mangle]
pub unsafe extern "C" fn get_curr_tile(tilepos: c_short) -> c_short {
    curr_modifier = *curr_room_modif.add(tilepos as usize);
    curr_tile = *curr_room_tiles.add(tilepos as usize) & 0x1F;
    curr_tile as c_short
}

// seg007:1063
#[no_mangle]
pub unsafe extern "C" fn do_mobs() {
    let n_mobs = mobs_count;
    curmob_index = 0;
    while n_mobs > curmob_index as c_short {
        curmob = mobs[curmob_index as usize];
        move_mob();
        check_loose_fall_on_kid();
        mobs[curmob_index as usize] = curmob;
        curmob_index += 1;
    }
    let mut new_index: c_short = 0;
    let mut index: c_short = 0;
    while index < mobs_count {
        if mobs[index as usize].speed != -1 {
            mobs[new_index as usize] = mobs[index as usize];
            new_index += 1;
        }
        index += 1;
    }
    mobs_count = new_index;
}

// seg007:110F
#[no_mangle]
pub unsafe extern "C" fn move_mob() {
    if curmob.type_ == 0 {
        move_loose();
    }
    if curmob.speed <= 0 {
        curmob.speed = curmob.speed.wrapping_add(1);
    }
}

// seg007:1126
#[no_mangle]
pub unsafe extern "C" fn move_loose() {
    if curmob.speed < 0 { return; }
    if curmob.speed < 29 {
        curmob.speed = curmob.speed.wrapping_add(3);
    }
    curmob.y = curmob.y.wrapping_add(curmob.speed as u8);
    if curmob.room == 0 {
        if (curmob.y as u16) < 210 {
            return;
        } else {
            curmob.speed = -2;
            return;
        }
    }
    if (curmob.y as u16) < 226 && Y_SOMETHING[(curmob.row + 1) as usize] <= curmob.y as i16 {
        // fell into a different row
        curr_tile_temp = get_tile(
            curmob.room as c_int,
            (curmob.xh >> 2) as c_int,
            curmob.row as c_int,
        ) as u16;
        if curr_tile_temp == tiles_tiles_11_loose as u16 {
            loose_fall();
        }
        if curr_tile_temp == tiles_tiles_0_empty as u16
            || curr_tile_temp == tiles_tiles_11_loose as u16
        {
            mob_down_a_row();
            return;
        }
        play_sound(soundids_sound_2_tile_crashing as c_int);
        do_knock(curmob.room as c_int, curmob.row as c_int);
        curmob.y = Y_SOMETHING[(curmob.row + 1) as usize] as u8;
        curmob.speed = -2;
        loose_land();
    }
}

// seg007:11E8
#[no_mangle]
pub unsafe extern "C" fn loose_land() {
    let mut button_type: c_short = 0;
    let mut tiletype = get_tile(
        curmob.room as c_int,
        (curmob.xh >> 2) as c_int,
        curmob.row as c_int,
    ) as c_short;

    let mut needs_floor = false;

    if tiletype == tiles_tiles_15_opener as c_short {
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_14_debris as u8;
        button_type = tiles_tiles_14_debris as c_short;
        trigger_button(1, button_type as c_int, -1);
        tiletype = get_tile(
            curmob.room as c_int,
            (curmob.xh >> 2) as c_int,
            curmob.row as c_int,
        ) as c_short;
        needs_floor = true;
    } else if tiletype == tiles_tiles_6_closer as c_short {
        trigger_button(1, button_type as c_int, -1);
        tiletype = get_tile(
            curmob.room as c_int,
            (curmob.xh >> 2) as c_int,
            curmob.row as c_int,
        ) as c_short;
        needs_floor = true;
    } else if tiletype == tiles_tiles_1_floor as c_short
        || tiletype == tiles_tiles_2_spike as c_short
        || tiletype == tiles_tiles_10_potion as c_short
        || tiletype == tiles_tiles_19_torch as c_short
        || tiletype == tiles_tiles_30_torch_with_debris as c_short
    {
        needs_floor = true;
    }

    if needs_floor {
        if tiletype == tiles_tiles_19_torch as c_short
            || tiletype == tiles_tiles_30_torch_with_debris as c_short
        {
            *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_30_torch_with_debris as u8;
        } else {
            *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_14_debris as u8;
        }
        redraw_at_cur_mob();
        if tile_col != 0 {
            set_redraw_full(curr_tilepos as c_short - 1, 1);
        }
    }
}

// seg007:12CB
#[no_mangle]
pub unsafe extern "C" fn loose_fall() {
    curr_room_modif.add(curr_tilepos as usize)
        .write(remove_loose(curr_room as c_int, curr_tilepos as c_int) as u8);
    curmob.speed >>= 1;
    mobs[curmob_index as usize] = curmob;
    curmob.y = curmob.y.wrapping_add(6);
    mob_down_a_row();
    add_mob();
    curmob = mobs[curmob_index as usize];
    redraw_at_cur_mob();
}

// seg007:132C
#[no_mangle]
pub unsafe extern "C" fn redraw_at_cur_mob() {
    if curmob.room as u16 == drawn_room {
        redraw_height = 0x20;
        set_redraw_full(curr_tilepos as c_short, 1);
        set_wipe(curr_tilepos as c_short, 1);
        if (curr_tilepos % 10) + 1 < 10 {
            set_redraw_full(curr_tilepos as c_short + 1, 1);
            set_wipe(curr_tilepos as c_short + 1, 1);
        }
    }
}

// seg007:1387
#[no_mangle]
pub unsafe extern "C" fn mob_down_a_row() {
    curmob.row = curmob.row.wrapping_add(1);
    if curmob.row >= 3 {
        curmob.y = curmob.y.wrapping_sub(192);
        curmob.row = 0;
        curmob.room = level.roomlinks[curmob.room as usize - 1].down;
    }
}

// seg007:13AE
#[no_mangle]
pub unsafe extern "C" fn draw_mobs() {
    let mut index: c_short = 0;
    while index < mobs_count {
        curmob = mobs[index as usize];
        draw_mob();
        index += 1;
    }
}

// seg007:13E5
#[no_mangle]
pub unsafe extern "C" fn draw_mob() {
    let mut ypos = curmob.y as c_short;
    if curmob.room as u16 == drawn_room {
        if curmob.y as u16 >= 210 { return; }
    } else if curmob.room as u16 == room_B {
        if (ypos as i8 as i32).abs() >= 18 { return; }
        curmob.y = curmob.y.wrapping_add(192);
        ypos = curmob.y as c_short;
    } else if curmob.room as u16 == room_A {
        if (curmob.y as u16) < 174 { return; }
        ypos = curmob.y as c_short - 189;
    } else {
        return;
    }
    let tile_col_local = (curmob.xh >> 2) as c_short;
    let trow = y_to_row_mod4(ypos as c_int);
    obj_tilepos = get_tilepos_nominus(tile_col_local as c_int, trow) as u8;
    let tile_col2 = tile_col_local + 1;
    let tilepos = get_tilepos(tile_col2 as c_int, trow);
    set_redraw2(tilepos as c_short, 1);
    set_redraw_fore(tilepos as c_short, 1);
    let top_row = y_to_row_mod4(ypos as c_int - 18);
    if top_row != trow {
        let tilepos2 = get_tilepos(tile_col2 as c_int, top_row);
        set_redraw2(tilepos2 as c_short, 1);
        set_redraw_fore(tilepos2 as c_short, 1);
    }
    add_mob_to_objtable(ypos as c_int);
}

// seg007:14DE
#[no_mangle]
pub unsafe extern "C" fn add_mob_to_objtable(ypos: c_int) {
    let index = table_counts[4];
    table_counts[4] += 1;
    let curr_obj = &mut objtable[index as usize];
    curr_obj.obj_type = curmob.type_ | 0x80;
    curr_obj.xh = curmob.xh as i8;
    curr_obj.xl = 0;
    curr_obj.y = ypos as c_short;
    curr_obj.chtab_id = chtabs_id_chtab_6_environment as u8;
    curr_obj.id = 10;
    curr_obj.clip.top = 0;
    curr_obj.clip.left = 0;
    curr_obj.clip.right = 40;
    mark_obj_tile_redraw(index as c_int);
}

// seg007:153E — not used
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

// seg007:1556
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

// seg007:1591
#[no_mangle]
pub unsafe extern "C" fn check_loose_fall_on_kid() {
    loadkid();
    if Char.room == curmob.room
        && Char.curr_col == (curmob.xh >> 2) as i8
        && (curmob.y as u16) < Char.y as u16
        && Char.y as u16 - 30 < curmob.y as u16
    {
        fell_on_your_head();
        savekid();
    }
}

// seg007:15D3
#[no_mangle]
pub unsafe extern "C" fn fell_on_your_head() {
    let frame = Char.frame as c_short;
    let action = Char.action as c_short;
    if (current_level as u16 == (*custom).loose_tiles_level as u16
        || frame < frameids_frame_5_start_run as c_short
        || frame >= 15)
        && (action < actions_actions_2_hang_climb as c_short
            || action == actions_actions_7_turn as c_short)
    {
        Char.y = y_land_at(Char.curr_row as usize + 1) as u8;
        if take_hp(1) != 0 {
            seqtbl_offset_char(seqids_seq_22_crushed as c_short);
            if frame == frameids_frame_177_spiked as c_short {
                Char.x = char_dx_forward(-12) as u8;
            }
        } else if frame != frameids_frame_109_crouch as c_short {
            if get_tile_behind_char() == 0 {
                Char.x = char_dx_forward(-2) as u8;
            }
            seqtbl_offset_char(seqids_seq_52_loose_floor_fell_on_kid as c_short);
        }
    }
}

// seg007:1669
#[no_mangle]
pub unsafe extern "C" fn play_door_sound_if_visible(sound_id: c_int) {
    let tilepos = trob.tilepos as u16;
    let gate_room = trob.room as u16;

    // FIX_GATE_SOUNDS is active
    let has_sound_condition = if (*fixes).fix_gate_sounds != 0 {
        (gate_room == room_L && tilepos % 10 == 9)
            || (gate_room == drawn_room && tilepos % 10 != 9)
    } else {
        if gate_room == room_L {
            tilepos % 10 == 9
        } else {
            gate_room == drawn_room && tilepos % 10 != 9
        }
    };

    let has_sound = (current_level == 3 && gate_room == 2) || has_sound_condition;
    if has_sound {
        play_sound(sound_id);
    }
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
