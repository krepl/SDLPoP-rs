#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short, c_uint};
use super::*;

// File-scoped statics
static mut curmob_index: u16 = 0;

// Constants from the C source
const gate_close_speeds: [u8; 9] = [0, 0, 0, 20, 40, 60, 80, 100, 120];
const door_delta: [i8; 3] = [-1, 4, 4];
const leveldoor_close_speeds: [u8; 5] = [0, 5, 17, 99, 0];
const y_loose_land: [u16; 5] = [2, 65, 128, 191, 254];
const loose_sound: [u8; 12] = [0, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0];
const y_something: [i16; 5] = [-1, 62, 125, 188, 25];

// seg007:0000
#[no_mangle]
pub unsafe extern "C" fn process_trobs() {
    let mut need_delete: i32 = 0;
    if trobs_count == 0 {
        return;
    }
    for index in 0..trobs_count {
        trob = trobs[index as usize];
        animate_tile();
        trobs[index as usize].type_ = trob.type_;
        if trob.type_ < 0 {
            need_delete = 1;
        }
    }
    if need_delete != 0 {
        let mut new_index: c_short = 0;
        for index in 0..trobs_count {
            if trobs[index as usize].type_ >= 0 {
                trobs[new_index as usize] = trobs[index as usize];
                new_index += 1;
            }
        }
        trobs_count = new_index;
    }
}

// seg007:00AF
#[no_mangle]
pub unsafe extern "C" fn animate_tile() {
    get_room_address(trob.room as c_int);
    let tiletype = get_curr_tile(trob.tilepos as c_short);
    match tiletype as i32 {
        x if x == (tiles_tiles_19_torch as i32) => {
            animate_torch();
        },
        x if x == (tiles_tiles_30_torch_with_debris as i32) => {
            animate_torch();
        },
        x if x == (tiles_tiles_6_closer as i32) => {
            animate_button();
        },
        x if x == (tiles_tiles_15_opener as i32) => {
            animate_button();
        },
        x if x == (tiles_tiles_2_spike as i32) => {
            animate_spike();
        },
        x if x == (tiles_tiles_11_loose as i32) => {
            animate_loose();
        },
        x if x == (tiles_tiles_0_empty as i32) => {
            animate_empty();
        },
        x if x == (tiles_tiles_18_chomper as i32) => {
            animate_chomper();
        },
        x if x == (tiles_tiles_4_gate as i32) => {
            animate_door();
        },
        x if x == (tiles_tiles_16_level_door_left as i32) => {
            animate_leveldoor();
        },
        x if x == (tiles_tiles_10_potion as i32) => {
            animate_potion();
        },
        x if x == (tiles_tiles_22_sword as i32) => {
            animate_sword();
        },
        _ => {
            trob.type_ = -1;
        },
    }
    *curr_room_modif.add(trob.tilepos as usize) = curr_modifier;
}

// seg007:0166
#[no_mangle]
pub unsafe extern "C" fn is_trob_in_drawn_room() -> c_short {
    if (trob.room as u16) != drawn_room {
        trob.type_ = -1;
        return 0;
    } else {
        return 1;
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
    let tilepos: c_short = get_trob_pos_in_drawn_room();
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
    let tilepos: c_short = get_trob_right_pos_in_drawn_room();
    set_redraw_anim(tilepos, 1);
    set_redraw_fore(tilepos, 1);
    set_redraw_anim(get_trob_right_above_pos_in_drawn_room(), 1);
}

// seg007:0218
#[no_mangle]
pub unsafe extern "C" fn redraw_tile_height() {
    let mut tilepos: c_short = get_trob_pos_in_drawn_room();
    set_redraw_full(tilepos, 1);
    set_wipe(tilepos, 1);
    tilepos = get_trob_right_pos_in_drawn_room();
    set_redraw_full(tilepos, 1);
    set_wipe(tilepos, 1);
}

// seg007:0258
#[no_mangle]
pub unsafe extern "C" fn get_trob_pos_in_drawn_room() -> c_short {
    let mut tilepos: c_short = trob.tilepos as c_short;
    if (trob.room as u16) == room_A {
        if tilepos >= 20 && tilepos < 30 {
            // 20..29 -> -1..-10
            tilepos = 19 - tilepos;
        } else {
            tilepos = 30;
        }
    } else {
        if (trob.room as u16) != drawn_room {
            tilepos = 30;
        }
    }
    return tilepos;
}

// seg007:029D
#[no_mangle]
pub unsafe extern "C" fn get_trob_right_pos_in_drawn_room() -> c_short {
    let mut tilepos: c_short = trob.tilepos as c_short;
    if (trob.room as u16) == drawn_room {
        if (tilepos as u16) % 10 != 9 {
            tilepos += 1;
        } else {
            tilepos = 30;
        }
    } else if (trob.room as u16) == room_L {
        if (tilepos as u16) % 10 == 9 {
            tilepos -= 9;
        } else {
            tilepos = 30;
        }
    } else if (trob.room as u16) == room_A {
        if (tilepos as u16) >= 20 && (tilepos as u16) < 29 {
            // 20..28 -> -2..-10
            tilepos = 18 - tilepos;
        } else {
            tilepos = 30;
        }
    } else if (trob.room as u16) == room_AL && (tilepos as u16) == 29 {
        tilepos = -1;
    } else {
        tilepos = 30;
    }
    return tilepos;
}

// seg007:032C
#[no_mangle]
pub unsafe extern "C" fn get_trob_right_above_pos_in_drawn_room() -> c_short {
    let mut tilepos: c_short = trob.tilepos as c_short;
    if (trob.room as u16) == drawn_room {
        if (tilepos as u16) % 10 != 9 {
            if (tilepos as u16) < 10 {
                // 0..8 -> -2..-10
                tilepos = -((tilepos as u16 + 2) as c_short);
            } else {
                tilepos -= 9;
            }
        } else {
            tilepos = 30;
        }
    } else if (trob.room as u16) == room_L {
        if (tilepos as u16) == 9 {
            tilepos = -1;
        } else {
            if (tilepos as u16) % 10 == 9 {
                tilepos -= 19;
            } else {
                tilepos = 30;
            }
        }
    } else if (trob.room as u16) == room_B {
        if (tilepos as u16) < 9 {
            tilepos += 21;
        } else {
            tilepos = 30;
        }
    } else if (trob.room as u16) == room_BL && (tilepos as u16) == 9 {
        tilepos = 20;
    } else {
        tilepos = 30;
    }
    return tilepos;
}

// seg007:03CF
#[no_mangle]
pub unsafe extern "C" fn animate_torch() {
    if (trob.room as u16) == drawn_room || ((trob.room as u16) == room_L && (trob.tilepos as u16 % 10) == 9) {
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
        let type_: u16 = (curr_modifier as u16) & 0xF8;
        curr_modifier = (bubble_next_frame((curr_modifier as c_short) & 0x07) as u16 | type_) as u8;
        // FIX_LOOSE_NEXT_TO_POTION is always on
        redraw_at_trob();
    }
}

// seg007:0425
#[no_mangle]
pub unsafe extern "C" fn animate_sword() {
    if is_trob_in_drawn_room() != 0 {
        curr_modifier = curr_modifier.wrapping_sub(1);
        if curr_modifier == 0 {
            curr_modifier = ((prandom(255) as u16) & 0x3F) as u8 + 0x28;
        }
        // FIX_LOOSE_NEXT_TO_POTION is always on
        redraw_at_trob();
    }
}

// seg007:0448
#[no_mangle]
pub unsafe extern "C" fn animate_chomper() {
    if trob.type_ >= 0 {
        let blood: u16 = (curr_modifier as u16) & 0x80;
        let mut frame: u16 = ((curr_modifier as u16) & 0x7F) + 1;
        if frame > (*custom).chomper_speed as u16 {
            frame = 1;
        }
        curr_modifier = (blood | frame) as u8;
        if frame == 2 {
            play_sound(soundids_sound_47_chomper as c_int); // chomper
        }
        if ((trob.room as u16) != drawn_room || ((trob.tilepos as u16) / 10) != (Kid.curr_row as u16) ||
            (Kid.alive >= 0 && blood == 0)) && ((curr_modifier as u16) & 0x7F) >= 6 {
            trob.type_ = -1;
        }
    }
    if ((curr_modifier as u16) & 0x7F) < 6 {
        redraw_at_trob();
    }
}

// seg007:04D3
#[no_mangle]
pub unsafe extern "C" fn animate_spike() {
    if trob.type_ >= 0 {
        // 0xFF means a disabled spike.
        if curr_modifier == 0xFF {
            return;
        }
        if (curr_modifier as u16) & 0x80 != 0 {
            curr_modifier = curr_modifier.wrapping_sub(1);
            if (curr_modifier as u16) & 0x7F == 0 {
                return;
            }
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
    let mut anim_type: i8 = trob.type_;
    if anim_type >= 0 {
        if anim_type >= 3 {
            // closing fast
            if anim_type < 8 {
                anim_type += 1;
                trob.type_ = anim_type;
            }
            let new_mod: i16 = (curr_modifier as i16) - (gate_close_speeds[anim_type as usize] as i16);
            curr_modifier = new_mod as u8;
            if new_mod < 0 {
                curr_modifier = 0;
                trob.type_ = -1;
                play_sound(soundids_sound_6_gate_closing_fast as c_int); // gate closing fast
            }
        } else {
            if curr_modifier != 0xFF {
                // 0xFF means permanently open.
                curr_modifier = ((curr_modifier as i16) + (door_delta[anim_type as usize] as i16)) as u8;
                if anim_type == 0 {
                    // closing
                    if curr_modifier != 0 {
                        if (curr_modifier as u16) < 188 {
                            if ((curr_modifier as u16) & 3) == 3 {
                                play_door_sound_if_visible(soundids_sound_4_gate_closing as c_int); // gate closing
                            }
                        }
                    } else {
                        gate_stop();
                    }
                } else {
                    // opening
                    if (curr_modifier as u16) < 188 {
                        if ((curr_modifier as u16) & 7) == 0 {
                            play_sound(soundids_sound_5_gate_opening as c_int); // gate opening
                        }
                    } else {
                        // stop
                        if anim_type < 2 {
                            // after regular open
                            curr_modifier = 238;
                            trob.type_ = 0; // closing
                            play_sound(soundids_sound_7_gate_stop as c_int); // gate stop (after opening)
                        } else {
                            // after permanent open
                            curr_modifier = 0xFF; // keep open
                            gate_stop();
                        }
                    }
                }
            } else {
                gate_stop();
            }
        }
    }
    draw_trob();
}

// seg007:05E3
#[no_mangle]
pub unsafe extern "C" fn gate_stop() {
    trob.type_ = -1;
    play_door_sound_if_visible(soundids_sound_7_gate_stop as c_int); // gate stop (after closing)
}

// seg007:05F1
#[no_mangle]
pub unsafe extern "C" fn animate_leveldoor() {
    let trob_type: i8 = trob.type_;
    if trob.type_ >= 0 {
        if trob_type >= 3 {
            // closing
            trob.type_ += 1;
            curr_modifier = ((curr_modifier as i16) - (leveldoor_close_speeds[(trob.type_ - 3) as usize] as i16)) as u8;
            if (curr_modifier as i8) < 0 {
                curr_modifier = 0;
                trob.type_ = -1;
                play_sound(soundids_sound_14_leveldoor_closing as c_int); // level door closing
            } else {
                if trob.type_ == 4 && ((sound_flags as c_uint) & (soundflags_sfDigi as c_uint)) != 0 {
                    sound_interruptible_set(soundids_sound_15_leveldoor_sliding as usize, 1);
                    play_sound(soundids_sound_15_leveldoor_sliding as c_int); // level door sliding (closing)
                }
            }
        } else {
            // opening
            curr_modifier = curr_modifier.wrapping_add(1);
            if curr_modifier >= 43 {
                trob.type_ = -1;
                // FIX_FEATHER_INTERRUPTED_BY_LEVELDOOR is always on
                if !((*fixes).fix_feather_interrupted_by_leveldoor != 0 && is_feather_fall != 0) {
                    stop_sounds();
                }
                if leveldoor_open == 0 || leveldoor_open == 2 {
                    leveldoor_open = 1;
                    if (current_level as u16) == ((*custom).mirror_level as u16) {
                        // Special event: place mirror
                        get_tile((*custom).mirror_room as c_int, (*custom).mirror_column as c_int, (*custom).mirror_row as c_int);
                        *curr_room_tiles.add(curr_tilepos as usize) = (*custom).mirror_tile;
                    }
                }
            } else {
                sound_interruptible_set(soundids_sound_15_leveldoor_sliding as usize, 0);
                play_sound(soundids_sound_15_leveldoor_sliding as c_int); // level door sliding (opening)
            }
        }
    }
    set_redraw_anim_right();
}

// seg007:06AD - this is already provided by C FFI
// (bubble_next_frame is already declared in bindings)

// seg007:06CD - this is already provided by C FFI
// (get_torch_frame is already declared in bindings)

// seg007:070A
#[no_mangle]
pub unsafe extern "C" fn set_redraw_anim(tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            let idx = (-tilepos) as usize;
            redraw_frames_above[idx] = frames;
        } else {
            redraw_frames_anim[tilepos as usize] = frames;
        }
    }
}

// seg007:0738
#[no_mangle]
pub unsafe extern "C" fn set_redraw2(mut tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            // trying to draw a mob at a negative tilepos, in the range -1 .. -10
            // used e.g. when the kid is climbing up to the room above
            // however, loose tiles falling out of the room end up with a negative tilepos {-2 .. -11} !
            tilepos = (-tilepos) - 1;
            if tilepos > 9 {
                tilepos = 9; // prevent array index out of bounds!
            }
            redraw_frames_above[tilepos as usize] = frames;
        } else {
            redraw_frames2[tilepos as usize] = frames;
        }
    }
}

// seg007:0766
#[no_mangle]
pub unsafe extern "C" fn set_redraw_floor_overlay(mut tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            tilepos += 1;
            let idx = (-tilepos) as usize;
            redraw_frames_above[idx] = frames;
        } else {
            redraw_frames_floor_overlay[tilepos as usize] = frames;
        }
    }
}

// seg007:0794
#[no_mangle]
pub unsafe extern "C" fn set_redraw_full(mut tilepos: c_short, frames: u8) {
    if tilepos < 30 {
        if tilepos < 0 {
            tilepos += 1;
            let idx = (-tilepos) as usize;
            redraw_frames_above[idx] = frames;
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
        if wipe_frames[tilepos as usize] != 0 {
            let wh = wipe_heights[tilepos as usize];
            redraw_height = if (wh as i16) > redraw_height { wh as i16 } else { redraw_height };
        }
        wipe_heights[tilepos as usize] = redraw_height as i8;
        wipe_frames[tilepos as usize] = frames;
    }
}

// seg007:081E
#[no_mangle]
pub unsafe extern "C" fn start_anim_torch(room: c_short, tilepos: c_short) {
    *curr_room_modif.add(tilepos as usize) = (prandom(8) as u16 & 0xFF) as u8;
    add_trob(room as u8, tilepos as u8, 1);
}

// seg007:0847
#[no_mangle]
pub unsafe extern "C" fn start_anim_potion(room: c_short, tilepos: c_short) {
    let val = *curr_room_modif.add(tilepos as usize);
    *curr_room_modif.add(tilepos as usize) = val & 0xF8;
    *curr_room_modif.add(tilepos as usize) |= ((prandom(6) as u16 & 0xFF) + 1) as u8;
    add_trob(room as u8, tilepos as u8, 1);
}

// seg007:087C
#[no_mangle]
pub unsafe extern "C" fn start_anim_sword(room: c_short, tilepos: c_short) {
    *curr_room_modif.add(tilepos as usize) = ((prandom(0xFF) as u16) & 0x1F) as u8;
    add_trob(room as u8, tilepos as u8, 1);
}

// seg007:08A7
#[no_mangle]
pub unsafe extern "C" fn start_anim_chomper(room: c_short, tilepos: c_short, modifier: u8) {
    let old_modifier: c_short = *curr_room_modif.add(tilepos as usize) as c_short;
    if old_modifier == 0 || old_modifier >= 6 {
        *curr_room_modif.add(tilepos as usize) = modifier;
        add_trob(room as u8, tilepos as u8, 1);
    }
}

// seg007:08E3
#[no_mangle]
pub unsafe extern "C" fn start_anim_spike(room: c_short, tilepos: c_short) {
    let old_modifier: i8 = *curr_room_modif.add(tilepos as usize) as i8;
    if old_modifier <= 0 {
        if old_modifier == 0 {
            add_trob(room as u8, tilepos as u8, 1);
            play_sound(soundids_sound_49_spikes as c_int); // spikes
        } else {
            // 0xFF means a disabled spike.
            if old_modifier != (-1 as i8) {
                *curr_room_modif.add(tilepos as usize) = 0x8F;
            }
        }
    }
}

// seg007:092C
#[no_mangle]
pub unsafe extern "C" fn trigger_gate(_room: c_short, tilepos: c_short, button_type: c_short) -> c_short {
    let modifier: u8 = *curr_room_modif.add(tilepos as usize);
    if button_type as u16 == (tiles_tiles_15_opener as u16) {
        // If the gate is permanently open, don't do anything.
        if modifier == 0xFF {
            return -1;
        }
        if (modifier as u16) >= 188 {
            // if it's already open
            *curr_room_modif.add(tilepos as usize) = 238; // keep it open for a while
            return -1;
        }
        *curr_room_modif.add(tilepos as usize) = ((modifier as u16 + 3) & 0xFC) as u8;
        return 1; // regular open
    } else if button_type as u16 == (tiles_tiles_14_debris as u16) {
        // If it's not fully open:
        if (modifier as u16) < 188 {
            return 2;
        } // permanent open
        *curr_room_modif.add(tilepos as usize) = 0xFF; // keep open
        return -1;
    } else {
        if modifier != 0 {
            return 3; // close fast
        } else {
            // already closed
            return -1;
        }
    }
}

// seg007:0999
#[no_mangle]
pub unsafe extern "C" fn trigger_1(target_type: c_short, room: c_short, tilepos: c_short, button_type: c_short) -> c_short {
    let mut result: c_short = -1;
    if target_type as u16 == (tiles_tiles_4_gate as u16) {
        result = trigger_gate(room, tilepos, button_type);
    } else if target_type as u16 == (tiles_tiles_16_level_door_left as u16) {
        if *curr_room_modif.add(tilepos as usize) != 0 {
            result = -1;
        } else {
            result = 1;
        }
    } else if (*custom).allow_triggering_any_tile != 0 {
        //allow_triggering_any_tile hack
        result = 1;
    }
    return result;
}

// seg007:09E5
#[no_mangle]
pub unsafe extern "C" fn do_trigger_list(mut index: c_short, button_type: c_short) {
    loop {
        let room: u16 = get_doorlink_room(index) as u16;
        get_room_address(room as c_int);
        let tilepos: u16 = get_doorlink_tile(index) as u16;
        let target_type: u8 = (*curr_room_tiles.add(tilepos as usize)) & 0x1F;
        let trigger_result: i8 = trigger_1(target_type as c_short, room as c_short, tilepos as c_short, button_type) as i8;
        if trigger_result >= 0 {
            add_trob(room as u8, tilepos as u8, trigger_result);
        }
        if get_doorlink_next(index) == 0 {
            break;
        }
        index += 1;
    }
}

// seg007:0A5A
#[no_mangle]
pub unsafe extern "C" fn add_trob(room: u8, tilepos: u8, type_: i8) {
    if trobs_count as u16 >= 30 {
        show_dialog(b"Trobs Overflow\0".as_ptr() as *const i8);
        return;
    }
    trob.room = room;
    trob.tilepos = tilepos;
    trob.type_ = type_;
    let found: c_short = find_trob();
    if found == -1 {
        // add new
        if trobs_count as u16 == 30 {
            return;
        }
        trobs[trobs_count as usize] = trob;
        trobs_count += 1;
    } else {
        // change existing
        trobs[found as usize].type_ = trob.type_;
    }
}

// seg007:0ACA
#[no_mangle]
pub unsafe extern "C" fn find_trob() -> c_short {
    for index in 0..trobs_count {
        if trobs[index as usize].tilepos == trob.tilepos && trobs[index as usize].room == trob.room {
            return index;
        }
    }
    return -1;
}

// seg007:0B0A
#[no_mangle]
pub unsafe extern "C" fn clear_tile_wipes() {
    for i in 0..redraw_frames_full.len() {
        redraw_frames_full[i] = 0;
    }
    for i in 0..wipe_frames.len() {
        wipe_frames[i] = 0;
    }
    for i in 0..wipe_heights.len() {
        wipe_heights[i] = 0;
    }
    for i in 0..redraw_frames_anim.len() {
        redraw_frames_anim[i] = 0;
    }
    for i in 0..redraw_frames_fore.len() {
        redraw_frames_fore[i] = 0;
    }
    for i in 0..redraw_frames2.len() {
        redraw_frames2[i] = 0;
    }
    for i in 0..redraw_frames_floor_overlay.len() {
        redraw_frames_floor_overlay[i] = 0;
    }
    for i in 0..tile_object_redraw.len() {
        tile_object_redraw[i] = 0;
    }
    for i in 0..redraw_frames_above.len() {
        redraw_frames_above[i] = 0;
    }
}

// seg007:0BB6
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_timer(index: c_short) -> c_short {
    (doorlink2_ad_at(index as usize) & 0x1F) as c_short
}

// seg007:0BCD
#[no_mangle]
pub unsafe extern "C" fn set_doorlink_timer(index: c_short, value: u8) -> c_short {
    let idx = index as usize;
    let addr = core::ptr::addr_of_mut!(doorlink2_ad).cast::<u8>().add(idx);
    let mut val = *addr;
    val &= 0xE0;
    val |= value & 0x1F;
    *addr = val;
    val as c_short
}

// seg007:0BF2 - get_doorlink_tile is provided via FFI
// (but let me add our helper function since it seems to be used)

// seg007:0C09
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_next(index: c_short) -> c_short {
    let val = doorlink1_ad_at(index as usize);
    if (val & 0x80) == 0 { 1 } else { 0 }
}

// seg007:0C26
#[no_mangle]
pub unsafe extern "C" fn get_doorlink_room(index: c_short) -> c_short {
    let doorlink1 = doorlink1_ad_at(index as usize);
    let doorlink2 = doorlink2_ad_at(index as usize);
    ((doorlink1 & 0x60) >> 5) as c_short + ((doorlink2 & 0xE0) >> 3) as c_short
}

// seg007:0C53
#[no_mangle]
pub unsafe extern "C" fn trigger_button(playsound: c_int, mut button_type: c_int, mut modifier: c_int) {
    get_curr_tile(curr_tilepos as c_short);
    if button_type == 0 {
        // 0 means currently selected
        button_type = curr_tile as c_int;
    }
    if modifier == -1 {
        // -1 means currently selected
        modifier = curr_modifier as c_int;
    }
    let link_timer: i8 = get_doorlink_timer(modifier as c_short) as i8;
    // is the event jammed?
    if link_timer != 0x1F as i8 {
        set_doorlink_timer(modifier as c_short, 5);
        if (link_timer as i16) < 2 {
            add_trob(curr_room as u8, curr_tilepos as u8, 1);
            redraw_11h();
            is_guard_notice = 1;
            if playsound != 0 {
                play_sound(soundids_sound_3_button_pressed as c_int); // button pressed
            }
        }
        do_trigger_list(modifier as c_short, button_type as c_short);
    }
}

// seg007:0CD9
#[no_mangle]
pub unsafe extern "C" fn died_on_button() {
    let button_type: u16 = get_curr_tile(curr_tilepos as c_short) as u16;
    let modifier: u16 = curr_modifier as u16;
    if (curr_tile as u16) == (tiles_tiles_15_opener as u16) {
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
        *curr_room_modif.add(curr_tilepos as usize) = 0;
        let button_type_new: c_int = tiles_tiles_14_debris as c_int; // force permanent open
        trigger_button(1, button_type_new, modifier as c_int);
    } else {
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_5_stuck as u8;
        trigger_button(1, button_type as c_int, modifier as c_int);
    }
}

// seg007:0D3A
#[no_mangle]
pub unsafe extern "C" fn animate_button() {
    if trob.type_ >= 0 {
        let timer: u16 = (get_doorlink_timer(curr_modifier as c_short) as u16).wrapping_sub(1);
        set_doorlink_timer(curr_modifier as c_short, (timer & 0xFF) as u8);
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
    let anim_type: i8 = trob.type_;
    if anim_type >= 0 {
        curr_modifier = curr_modifier.wrapping_add(1);
        if (curr_modifier as u16) & 0x80 != 0 {
            // just shaking
            // don't stop on level 13, needed for the auto-falling floors
            if (current_level as u16) == ((*custom).loose_tiles_level as u16) {
                return;
            }
            if curr_modifier >= 0x84 {
                curr_modifier = 0;
                trob.type_ = -1;
            }
            loose_shake(if curr_modifier == 0 { 1 } else { 0 });
        } else {
            // something is on the floor
            // should it fall already?
            if (curr_modifier as u16) >= ((*custom).loose_floor_delay as u16) {
                let room: u16 = trob.room as u16;
                let tilepos: u16 = trob.tilepos as u16;
                // FIX_DROP_2_ROOMS_CLIMBING_LOOSE_TILE is always on
                if (*fixes).fix_drop_2_rooms_climbing_loose_tile != 0 &&
                    room == (level.roomlinks[(Kid.room as usize).wrapping_sub(1)].up as u16) && // the tile is in the room above
                    (tilepos / 10) == 2 && // at row 2
                    (Kid.curr_row as u16) == 0 && // prince is at a row 0 of the room below
                    (Kid.curr_col as u16) == (tilepos % 10) && // and at the same column
                    (Kid.frame as u16) >= (frameids_frame_135_climbing_1 as u16) && // and is climbing
                    (Kid.frame as u16) < (frameids_frame_141_climbing_7 as u16)
                {
                    // prince's row gets changed in the sequence before the frame 141
                    loose_shake(0);
                } else {
                    curr_modifier = remove_loose(room as c_int, tilepos as c_int) as u8;
                    trob.type_ = -1;
                    curmob.xh = ((tilepos % 10) << 2) as u8;
                    let row: u16 = tilepos / 10;
                    curmob.y = (y_loose_land[(row + 1) as usize] & 0xFF) as u8;
                    curmob.room = room as u8;
                    curmob.speed = 0;
                    curmob.type_ = 0;
                    curmob.row = row as u8;
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
    let mut sound_id: u16;
    if arg_0 != 0 || loose_sound[(curr_modifier as u16 & 0x7F) as usize] != 0 {
        loop {
            // Sounds 20,21,22: loose floor shaking
            sound_id = (prandom(2) as u16) + (soundids_sound_20_loose_shake_1 as u16);
            if sound_id != last_loose_sound {
                break;
            }
        }
        // USE_REPLAY is always on
        // Skip this prandom call if we are replaying, and the replay file was made with an old version of SDLPoP (which didn't have this call).
        if !(replaying != 0 && g_deprecation_number < 2) {
            prandom(2); // For vanilla pop compatibility, an RNG cycle is wasted here
            // Note: In DOS PoP, it's wasted a few lines below.
        }
        if ((sound_flags as c_uint) & (soundflags_sfDigi as c_uint)) != 0 {
            last_loose_sound = sound_id;
            // random sample rate (10500..11500)
            //sound_pointers[sound_id]->samplerate = prandom(1000) + 10500;
        }
        play_sound(sound_id as c_int);
    }
}

// seg007:0EB8
#[no_mangle]
pub unsafe extern "C" fn remove_loose(_room: c_int, tilepos: c_int) -> c_int {
    *curr_room_tiles.add(tilepos as usize) = tiles_tiles_0_empty as u8;
    // note: the level type is used to determine the modifier of the empty space left behind
    (*custom).tbl_level_type[(current_level as usize) as usize] as c_int
}

// seg007:0ED5
#[no_mangle]
pub unsafe extern "C" fn make_loose_fall(modifier: u8) {
    // is it a "solid" loose floor?
    if ((*curr_room_tiles.add(curr_tilepos as usize)) & 0x20) == 0 {
        if (*curr_room_modif.add(curr_tilepos as usize) as i8) <= 0 {
            *curr_room_modif.add(curr_tilepos as usize) = modifier;
            add_trob(curr_room as u8, curr_tilepos as u8, 0);
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
        let mut tilepos: u16 = tbl_line_at(Char.curr_row as usize) as u16;
        for _column in 0..10 {
            if get_curr_tile(tilepos as c_short) as u16 == (tiles_tiles_18_chomper as u16) {
                let modifier: c_short = (curr_modifier as c_short) & 0x7F;
                if modifier == 0 || modifier >= 6 {
                    start_anim_chomper(Char.room as c_short, tilepos as c_short, (timing as u16 | ((curr_modifier as u16) & 0x80)) as u8);
                    timing = next_chomper_timing(timing as u8) as c_short;
                }
            }
            tilepos += 1;
        }
    }
}

// seg007:0F9A
#[no_mangle]
pub unsafe extern "C" fn next_chomper_timing(mut timing: u8) -> c_int {
    // 15,12,9,6,13,10,7,14,11,8,repeat
    timing = timing.wrapping_sub(3);
    if timing < 6 {
        timing = timing.wrapping_add(10);
    }
    timing as c_int
}

// seg007:0FB4
#[no_mangle]
pub unsafe extern "C" fn loose_make_shake() {
    // don't shake on level 13
    if (*curr_room_modif.add(curr_tilepos as usize)) == 0 && (current_level as u16) != ((*custom).loose_tiles_level as u16) {
        *curr_room_modif.add(curr_tilepos as usize) = 0x80;
        add_trob(curr_room as u8, curr_tilepos as u8, 1);
    }
}

// seg007:0FE0
#[no_mangle]
pub unsafe extern "C" fn do_knock(room: c_int, row: c_int) {
    for col in 0..10 {
        if get_tile(room, col, row) as u16 == (tiles_tiles_11_loose as u16) {
            loose_make_shake();
        }
    }
}

// seg007:1010
#[no_mangle]
pub unsafe extern "C" fn add_mob() {
    if mobs_count >= 14 {
        show_dialog(b"Mobs Overflow\0".as_ptr() as *const i8);
        return;
    }
    mobs[mobs_count as usize] = curmob;
    mobs_count += 1;
}

// seg007:1041
#[no_mangle]
pub unsafe extern "C" fn get_curr_tile(tilepos: c_short) -> c_short {
    curr_modifier = *curr_room_modif.add(tilepos as usize);
    let tile_val: u8 = (*curr_room_tiles.add(tilepos as usize)) & 0x1F;
    curr_tile = tile_val;
    return tile_val as c_short;
}

// seg007:1063
#[no_mangle]
pub unsafe extern "C" fn do_mobs() {
    let n_mobs: c_short = mobs_count;
    curmob_index = 0;
    while n_mobs > curmob_index as i16 {
        curmob = mobs[curmob_index as usize];
        move_mob();
        check_loose_fall_on_kid();
        mobs[curmob_index as usize] = curmob;
        curmob_index += 1;
    }
    let mut new_index: c_short = 0;
    for index in 0..mobs_count {
        if mobs[index as usize].speed != -1 {
            mobs[new_index as usize] = mobs[index as usize];
            new_index += 1;
        }
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
        curmob.speed += 1;
    }
}

// seg007:1126
#[no_mangle]
pub unsafe extern "C" fn move_loose() {
    if curmob.speed < 0 {
        return;
    }
    if curmob.speed < 29 {
        curmob.speed += 3;
    }
    curmob.y = (curmob.y as i16 + curmob.speed as i16) as u8;
    if curmob.room == 0 {
        if (curmob.y as i16) < 210 {
            return;
        } else {
            curmob.speed = -2;
            return;
        }
    }
    if (curmob.y as i16) < 226 && y_something[(curmob.row as usize) + 1] as i16 <= (curmob.y as i16) {
        // fell into a different row
        let curr_tile_temp: c_int = get_tile(curmob.room as c_int, ((curmob.xh as u16) >> 2) as c_int, curmob.row as c_int);
        if curr_tile_temp as u16 == (tiles_tiles_11_loose as u16) {
            loose_fall();
        }
        if curr_tile_temp as u16 == (tiles_tiles_0_empty as u16) ||
            curr_tile_temp as u16 == (tiles_tiles_11_loose as u16) {
            mob_down_a_row();
            return;
        }
        play_sound(soundids_sound_2_tile_crashing as c_int); // tile crashing
        do_knock(curmob.room as c_int, curmob.row as c_int);
        curmob.y = (y_something[(curmob.row as usize) + 1] & 0xFF) as u8;
        curmob.speed = -2;
        loose_land();
    }
}

// seg007:11E8
#[no_mangle]
pub unsafe extern "C" fn loose_land() {
    let mut button_type: c_short = 0;
    let tiletype: c_int = get_tile(curmob.room as c_int, ((curmob.xh as u16) >> 2) as c_int, curmob.row as c_int);
    match tiletype as u16 {
        x if x == (tiles_tiles_15_opener as u16) => {
            *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_14_debris as u8;
            button_type = tiles_tiles_14_debris as c_short;
            // fallthrough to the next case - handled by the next if
        },
        _ => {},
    }
    if tiletype as u16 == (tiles_tiles_15_opener as u16) || tiletype as u16 == (tiles_tiles_6_closer as u16) {
        trigger_button(1, button_type as c_int, -1);
        let tiletype = get_tile(curmob.room as c_int, ((curmob.xh as u16) >> 2) as c_int, curmob.row as c_int);
        // fallthrough to the final block
        if tiletype as u16 == (tiles_tiles_1_floor as u16) ||
            tiletype as u16 == (tiles_tiles_2_spike as u16) ||
            tiletype as u16 == (tiles_tiles_10_potion as u16) ||
            tiletype as u16 == (tiles_tiles_19_torch as u16) ||
            tiletype as u16 == (tiles_tiles_30_torch_with_debris as u16) {
            if tiletype as u16 == (tiles_tiles_19_torch as u16) ||
                tiletype as u16 == (tiles_tiles_30_torch_with_debris as u16) {
                *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_30_torch_with_debris as u8;
            } else {
                *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_14_debris as u8;
            }
            redraw_at_cur_mob();
            if tile_col != 0 {
                set_redraw_full((curr_tilepos as i16 - 1) as c_short, 1);
            }
        }
    } else if tiletype as u16 == (tiles_tiles_1_floor as u16) ||
        tiletype as u16 == (tiles_tiles_2_spike as u16) ||
        tiletype as u16 == (tiles_tiles_10_potion as u16) ||
        tiletype as u16 == (tiles_tiles_19_torch as u16) ||
        tiletype as u16 == (tiles_tiles_30_torch_with_debris as u16) {
        if tiletype as u16 == (tiles_tiles_19_torch as u16) ||
            tiletype as u16 == (tiles_tiles_30_torch_with_debris as u16) {
            *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_30_torch_with_debris as u8;
        } else {
            *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_14_debris as u8;
        }
        redraw_at_cur_mob();
        if tile_col != 0 {
            set_redraw_full((curr_tilepos as i16 - 1) as c_short, 1);
        }
    }
}

// seg007:12CB
#[no_mangle]
pub unsafe extern "C" fn loose_fall() {
    *curr_room_modif.add(curr_tilepos as usize) = remove_loose(curr_room as c_int, curr_tilepos as c_int) as u8;
    curmob.speed >>= 1;
    mobs[curmob_index as usize] = curmob;
    curmob.y = (curmob.y as i16 + 6) as u8;
    mob_down_a_row();
    add_mob();
    curmob = mobs[curmob_index as usize];
    redraw_at_cur_mob();
}

// seg007:132C
#[no_mangle]
pub unsafe extern "C" fn redraw_at_cur_mob() {
    if (curmob.room as u16) == drawn_room {
        redraw_height = 0x20;
        set_redraw_full(curr_tilepos as c_short, 1);
        set_wipe(curr_tilepos as c_short, 1);
        // Redraw tile to the right only if it's in the same room.
        if ((curr_tilepos as u16 % 10) + 1) < 10 {
            set_redraw_full((curr_tilepos as i16 + 1) as c_short, 1);
            set_wipe((curr_tilepos as i16 + 1) as c_short, 1);
        }
    }
}

// seg007:1387
#[no_mangle]
pub unsafe extern "C" fn mob_down_a_row() {
    curmob.row += 1;
    if curmob.row >= 3 {
        curmob.y = (curmob.y as i16 - 192) as u8;
        curmob.row = 0;
        curmob.room = (level.roomlinks[(curmob.room as i16 - 1) as usize].down as u16) as u8;
    }
}

// seg007:13AE
#[no_mangle]
pub unsafe extern "C" fn draw_mobs() {
    for index in 0..mobs_count {
        curmob = mobs[index as usize];
        draw_mob();
    }
}

// seg007:13E5
#[no_mangle]
pub unsafe extern "C" fn draw_mob() {
    let mut ypos: c_short = curmob.y as c_short;
    if (curmob.room as u16) == drawn_room {
        if (curmob.y as u16) >= 210 {
            return;
        }
    } else if (curmob.room as u16) == room_B {
        let abs_ypos = if ypos < 0 { -ypos } else { ypos };
        if abs_ypos >= 18 {
            return;
        }
        curmob.y = (ypos as i16 + 192) as u8;
        ypos = curmob.y as c_short;
    } else if (curmob.room as u16) == room_A {
        if (curmob.y as u16) < 174 {
            return;
        }
        ypos = ((curmob.y as i16) - 189) as c_short;
    } else {
        return;
    }
    let col: c_short = ((curmob.xh as u16) >> 2) as c_short;
    let row: c_short = y_to_row_mod4(ypos as c_int) as c_short;
    obj_tilepos = get_tilepos_nominus(col as c_int, row as c_int) as u8;
    let col_next: c_short = col + 1;
    let mut tilepos: c_short = get_tilepos(col_next as c_int, row as c_int) as c_short;
    set_redraw2(tilepos, 1);
    set_redraw_fore(tilepos, 1);
    let top_row: c_short = y_to_row_mod4((ypos as i16 - 18) as c_int) as c_short;
    if top_row != row {
        tilepos = get_tilepos(col_next as c_int, top_row as c_int) as c_short;
        set_redraw2(tilepos, 1);
        set_redraw_fore(tilepos, 1);
    }
    add_mob_to_objtable(ypos as c_int);
}

// seg007:14DE
#[no_mangle]
pub unsafe extern "C" fn add_mob_to_objtable(ypos: c_int) {
    let index: u16 = table_counts[4] as u16;
    table_counts[4] += 1;
    let curr_obj = &mut objtable[index as usize];
    curr_obj.obj_type = (curmob.type_ as u16 | 0x80) as u8;
    curr_obj.xh = curmob.xh as i8;
    curr_obj.xl = 0;
    curr_obj.y = ypos as i16;
    curr_obj.chtab_id = chtabs_id_chtab_6_environment as u8;
    curr_obj.id = 10;
    curr_obj.clip.top = 0;
    curr_obj.clip.left = 0;
    curr_obj.clip.right = 40;
    mark_obj_tile_redraw(index as c_int);
}

// seg007:153E
#[no_mangle]
pub unsafe extern "C" fn sub_9A8E() {
    // This function is not used.
    // method_1_blit_rect(onscreen_surface_, offscreen_surface, &rect_top, &rect_top, 0);
}

// seg007:1556
#[no_mangle]
pub unsafe extern "C" fn is_spike_harmful() -> c_int {
    let modifier: i8 = *curr_room_modif.add(curr_tilepos as usize) as i8;
    if modifier == 0 || modifier == -1 {
        return 0;
    } else if modifier < 0 {
        return 1;
    } else if modifier < 5 {
        return 2;
    } else {
        return 0;
    }
}

// seg007:1591
#[no_mangle]
pub unsafe extern "C" fn check_loose_fall_on_kid() {
    loadkid();
    if (Char.room as u16) == (curmob.room as u16) &&
        (Char.curr_col as u16) == ((curmob.xh as u16) >> 2) &&
        (curmob.y as u16) < (Char.y as u16) &&
        ((Char.y as u16).wrapping_sub(30)) < (curmob.y as u16) {
        fell_on_your_head();
        savekid();
    }
}

// seg007:15D3
#[no_mangle]
pub unsafe extern "C" fn fell_on_your_head() {
    let frame: u8 = Char.frame;
    let action: u8 = Char.action;
    // loose floors hurt you in frames 5..14 (running) only on level 13
    if ((current_level as u16) == ((*custom).loose_tiles_level as u16) || (frame < (frameids_frame_5_start_run as u8) || frame >= 15)) &&
        (action < (actions_actions_2_hang_climb as u8) || action == (actions_actions_7_turn as u8)) {
        Char.y = y_land_at((Char.curr_row as usize) + 1) as u8;
        if take_hp(1) != 0 {
            seqtbl_offset_char(seqids_seq_22_crushed as c_short); // dead (because of loose floor)
            if frame == (frameids_frame_177_spiked as u8) {
                // spiked
                Char.x = (char_dx_forward(-12) & 0xFF) as u8;
            }
        } else {
            if frame != (frameids_frame_109_crouch as u8) {
                // crouching
                if get_tile_behind_char() == 0 {
                    Char.x = (char_dx_forward(-2) & 0xFF) as u8;
                }
                seqtbl_offset_char(seqids_seq_52_loose_floor_fell_on_kid as c_short); // loose floor fell on Kid
            }
        }
    }
}

// seg007:1669
#[no_mangle]
pub unsafe extern "C" fn play_door_sound_if_visible(sound_id: c_int) {
    let tilepos: u16 = trob.tilepos as u16;
    let gate_room: u16 = trob.room as u16;
    let mut has_sound: u16 = 0;

    // FIX_GATE_SOUNDS is always on
    let has_sound_condition: i8 = if (*fixes).fix_gate_sounds != 0 {
        if (gate_room == room_L && (tilepos % 10) == 9) || (gate_room == drawn_room && (tilepos % 10) != 9) { 1 } else { 0 }
    } else {
        if gate_room == room_L {
            if (tilepos % 10) == 9 { 1 } else { 0 }
        } else {
            if gate_room == drawn_room && (tilepos % 10) != 9 { 1 } else { 0 }
        }
    };
    // Special event: sound of closing gates
    if ((current_level as u16) == 3 && gate_room == 2) || has_sound_condition != 0 {
        has_sound = 1;
    }
    if has_sound != 0 {
        play_sound(sound_id);
    }
}
