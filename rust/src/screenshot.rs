// Screenshot capture — ported from screenshot.c (USE_SCREENSHOT).
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_short, c_uint, c_void};
use super::*;

extern "C" {
    fn snprintf(buf: *mut c_char, size: usize, fmt: *const c_char, ...) -> c_int;
    fn printf(fmt: *const c_char, ...) -> c_int;
    fn fprintf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    fn mkdir(path: *const c_char, mode: c_uint) -> c_int;
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    fn exit(code: c_int) -> !;
    fn SDL_CreateRGBSurface(
        flags: u32,
        width: c_int,
        height: c_int,
        depth: c_int,
        Rmask: u32,
        Gmask: u32,
        Bmask: u32,
        Amask: u32,
    ) -> *mut SDL_Surface;
    fn SDL_UpperBlit(
        src: *mut SDL_Surface,
        srcrect: *const SDL_Rect,
        dst: *mut SDL_Surface,
        dstrect: *mut SDL_Rect,
    ) -> c_int;
    fn SDL_FreeSurface(surface: *mut SDL_Surface);
    fn IMG_SavePNG(surface: *mut SDL_Surface, file: *const c_char) -> c_int;
    // IMG_GetError is a macro for SDL_GetError in SDL_image.h.
    fn SDL_GetError() -> *const c_char;
    static mut stderr: *mut FILE;
}

// File-scope globals (defined in screenshot.c, not exported via data.h).
const fn cstr_buf<const N: usize>(s: &str) -> [c_char; N] {
    let mut buf = [0 as c_char; N];
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        buf[i] = b[i] as c_char;
        i += 1;
    }
    buf
}

static mut screenshots_folder: [c_char; POP_MAX_PATH as usize] =
    cstr_buf::<{ POP_MAX_PATH as usize }>("screenshots");
static mut screenshot_filename: [c_char; POP_MAX_PATH as usize] =
    cstr_buf::<{ POP_MAX_PATH as usize }>("screenshot.png");
static mut screenshot_index: c_int = 0;

const EVENT_OFFSET: c_int = 0;
const NUMBER_OF_ROOMS: c_int = 24;

static mut event_used: [bool; 256] = [false; 256];
static mut has_trigger_potion: bool = false;

// delta vectors for room links
static dx: [c_int; 4] = [-1, 1, 0, 0];
static dy: [c_int; 4] = [0, 0, -1, 1];

static mut xpos: [c_int; (NUMBER_OF_ROOMS + 1) as usize] = [0; (NUMBER_OF_ROOMS + 1) as usize];
static mut ypos: [c_int; (NUMBER_OF_ROOMS + 1) as usize] = [0; (NUMBER_OF_ROOMS + 1) as usize];

static mut want_auto: bool = false;
static mut want_auto_whole_level: bool = false;
static mut want_auto_extras: bool = false;

// Helper: build a rect_type {top, left, bottom, right}.
fn rect(top: c_int, left: c_int, bottom: c_int, right: c_int) -> rect_type {
    rect_type {
        top: top as c_short,
        left: left as c_short,
        bottom: bottom as c_short,
        right: right as c_short,
    }
}

// Use incrementing numbers and a separate folder, like DOSBox.
unsafe fn make_screenshot_filename() {
    // Create the screenshots directory in SDLPoP's directory, even if the current directory is something else.
    let mut lf_buf = [0 as c_char; POP_MAX_PATH as usize];
    let located = locate_file_(
        b"screenshots\0".as_ptr() as *const c_char,
        lf_buf.as_mut_ptr(),
        POP_MAX_PATH as c_int,
    );
    let len = snprintf(
        screenshots_folder.as_mut_ptr(),
        POP_MAX_PATH as usize,
        b"%s\0".as_ptr() as *const c_char,
        located,
    );
    if len < 0 || len >= POP_MAX_PATH as c_int {
        fprintf(
            stderr,
            b"%s: buffer truncation detected!\n\0".as_ptr() as *const c_char,
            b"make_screenshot_filename\0".as_ptr() as *const c_char,
        );
        quit(2);
    }
    // Create the folder if it doesn't exist yet:
    mkdir(screenshots_folder.as_ptr() as *const c_char, 0o700);
    // Find the first unused filename:
    loop {
        let len = snprintf(
            screenshot_filename.as_mut_ptr(),
            POP_MAX_PATH as usize,
            b"%s/screenshot_%03d.png\0".as_ptr() as *const c_char,
            screenshots_folder.as_ptr(),
            screenshot_index,
        );
        if len < 0 || len >= POP_MAX_PATH as c_int {
            fprintf(
                stderr,
                b"%s: buffer truncation detected!\n\0".as_ptr() as *const c_char,
                b"make_screenshot_filename\0".as_ptr() as *const c_char,
            );
            quit(2);
        }
        if !file_exists(screenshot_filename.as_ptr() as *const c_char) {
            return;
        }
        screenshot_index += 1;
    }
}

unsafe fn show_result(result: c_int, what: *const c_char) {
    let mut sprintf_temp = [0 as c_char; 100];
    if result == 0 {
        printf(
            b"Saved %s to \"%s\".\n\0".as_ptr() as *const c_char,
            what,
            screenshot_filename.as_ptr(),
        );
        snprintf(
            sprintf_temp.as_mut_ptr(),
            100,
            b"Saved %s\0".as_ptr() as *const c_char,
            what,
        );
    } else {
        printf(
            b"Could not save %s to \"%s\". Error: %s\n\0".as_ptr() as *const c_char,
            what,
            screenshot_filename.as_ptr(),
            SDL_GetError(),
        );
        snprintf(
            sprintf_temp.as_mut_ptr(),
            100,
            b"Could not save %s\0".as_ptr() as *const c_char,
            what,
        );
    }
    display_text_bottom(sprintf_temp.as_ptr());
    text_time_total = 24;
    text_time_remaining = 24;
}

// Save a screenshot.
#[no_mangle]
pub unsafe extern "C" fn save_screenshot() {
    make_screenshot_filename();
    let result = IMG_SavePNG(get_final_surface(), screenshot_filename.as_ptr() as *const c_char);
    show_result(result, b"screenshot\0".as_ptr() as *const c_char);
}

// Switch to the given room and draw it.
unsafe fn switch_to_room(room: c_int) {
    drawn_room = room as word;
    load_room_links();

    if core::ptr::addr_of!((*custom).tbl_level_type[current_level as usize]).read_unaligned() != 0 {
        gen_palace_wall_colors();
    }

    // for guards
    Guard.direction = directions_dir_56_none as sbyte;
    guardhp_curr = 0; // otherwise guard HPs stay on screen
    draw_guard_hp(0, 10); // otherwise guard HPs still stay on screen if some guards have extra HP
    enter_guard(); // otherwise the guard won't show up
    check_shadow(); // otherwise the shadow won't appear on level 6

    // for potion bubbles
    for tilepos in 0..30 {
        let tile_type = (*curr_room_tiles.add(tilepos as usize) & 0x1F) as c_int;
        if tile_type == tiles_tiles_10_potion as c_int {
            let modifier = *curr_room_modif.add(tilepos as usize) as c_int;
            if (modifier & 7) == 0 {
                *curr_room_modif.add(tilepos as usize) =
                    (*curr_room_modif.add(tilepos as usize)).wrapping_add(1);
            }
        }
    }

    redraw_screen(1);
}

// Show annotations for non-visible things.
unsafe fn draw_extras() {
    macro_rules! cu {
        ($f:ident) => {
            core::ptr::addr_of!((*custom).$f).read_unaligned() as c_int
        };
    }
    macro_rules! cua {
        ($f:ident, $i:expr) => {
            core::ptr::addr_of!((*custom).$f[$i]).read_unaligned() as c_int
        };
    }

    // ambiguous tiles
    for tilepos in 0..30 {
        let tile_type = (*curr_room_tiles.add(tilepos as usize) & 0x1F) as c_int;
        let modifier = *curr_room_modif.add(tilepos as usize) as c_int;
        let row = tilepos / 10;
        let col = tilepos % 10;
        let y = row * 63 + 3;
        let x = col * 32;

        // special floors
        let mut floor_rect = rect(y + 60 - 3, x, y + 63 - 3, x + 32);

        // loose floors
        if tile_type == tiles_tiles_11_loose as c_int {
            let mut color = colorids_color_15_brightwhite as c_int;
            if *curr_room_tiles.add(tilepos as usize) & 0x20 != 0 {
                color = colorids_color_13_brightmagenta as c_int; // stable loose floor
            }
            show_text_with_color(
                &floor_rect,
                halign_center,
                valign_top,
                b"~~~~\0".as_ptr() as *const c_char,
                color,
            );
        }

        // buttons
        if tile_type == tiles_tiles_15_opener as c_int {
            show_text_with_color(
                &floor_rect,
                halign_center,
                valign_top,
                b"^^^^\0".as_ptr() as *const c_char,
                colorids_color_10_brightgreen as c_int,
            );
        }
        if tile_type == tiles_tiles_6_closer as c_int {
            floor_rect.top -= 2;
            show_text_with_color(
                &floor_rect,
                halign_center,
                valign_top,
                b"xxxx\0".as_ptr() as *const c_char,
                colorids_color_12_brightred as c_int,
            );
        }

        let mut is_trob_here = false;
        for index in 0..trobs_count {
            trob = trobs[index as usize];
            if trob.room as c_int == drawn_room as c_int && trob.tilepos as c_int == tilepos {
                is_trob_here = true;
                break;
            }
        }

        if !is_trob_here {
            // harmless spikes
            if tile_type == tiles_tiles_2_spike as c_int {
                if modifier >= 5 {
                    let spike_rect = rect(y + 50, x, y + 60, x + 32);
                    show_text_with_color(
                        &spike_rect,
                        halign_center,
                        valign_top,
                        b"safe\0".as_ptr() as *const c_char,
                        colorids_color_10_brightgreen as c_int,
                    );
                }
            }

            // stuck chompers
            if tile_type == tiles_tiles_18_chomper as c_int {
                let frame = modifier & 0x7F;
                if frame != 0 {
                    let chomper_rect = rect(y, x - 10, y + 60, x + 32 + 10);
                    let mut color = colorids_color_10_brightgreen as c_int;
                    if frame == 2 {
                        color = colorids_color_12_brightred as c_int;
                    }
                    show_text_with_color(
                        &chomper_rect,
                        halign_center,
                        valign_middle,
                        b"stuck\0".as_ptr() as *const c_char,
                        color,
                    );
                }
            }
        }

        // potion types
        if tile_type == tiles_tiles_10_potion as c_int {
            let pot_types: [(c_int, *const c_char); 7] = [
                (colorids_color_7_lightgray as c_int, b"x\0".as_ptr() as *const c_char), // empty
                (colorids_color_12_brightred as c_int, b"+1\0".as_ptr() as *const c_char), // heal
                (colorids_color_12_brightred as c_int, b"+++\0".as_ptr() as *const c_char), // life
                (
                    colorids_color_10_brightgreen as c_int,
                    b"slow\nfall\0".as_ptr() as *const c_char,
                ), // slow fall
                (colorids_color_10_brightgreen as c_int, b"flip\0".as_ptr() as *const c_char), // upside down
                (colorids_color_9_brightblue as c_int, b"-1\0".as_ptr() as *const c_char), // hurt
                (colorids_color_9_brightblue as c_int, b"trig\0".as_ptr() as *const c_char), // open
            ];
            let potion_type = modifier >> 3;
            let color: c_int;
            let text: *const c_char;
            let mut temp_text = [0 as c_char; 4];
            if potion_type >= 0 && potion_type < 7 {
                color = pot_types[potion_type as usize].0;
                text = pot_types[potion_type as usize].1;
            } else {
                color = colorids_color_15_brightwhite as c_int;
                snprintf(
                    temp_text.as_mut_ptr(),
                    4,
                    b"%d\0".as_ptr() as *const c_char,
                    potion_type,
                );
                text = temp_text.as_ptr();
            }
            let pot_rect = rect(y + 40, x, y + 60, x + 32);
            show_text_with_color(&pot_rect, halign_center, valign_top, text, color);
        }

        // triggered door events
        if tile_type == tiles_tiles_6_closer as c_int
            || tile_type == tiles_tiles_15_opener as c_int
            || (has_trigger_potion && drawn_room as c_int == 8 && tilepos == 0)
        {
            let first_event = modifier;
            let mut last_event = modifier;
            while last_event < 256 && get_doorlink_next(last_event as c_short) != 0 {
                last_event += 1;
            }
            let mut events = [0 as c_char; 256 * 4];
            let mut events_pos: c_int = 0;
            let mut event = first_event;
            while event <= last_event && events_pos < events.len() as c_int {
                let len = snprintf(
                    events.as_mut_ptr().add(events_pos as usize),
                    events.len() - events_pos as usize,
                    b"%d \0".as_ptr() as *const c_char,
                    event + EVENT_OFFSET,
                );
                if len < 0 {
                    break;
                }
                events_pos += len;
                event += 1;
            }
            events_pos -= 1;
            if events_pos > 0 && events_pos < events.len() as c_int {
                events[events_pos as usize] = 0; // trim trailing space
            }
            let buttonmod_rect = rect(y, x, y + 60 - 3, x + 32);
            show_text_with_color(
                &buttonmod_rect,
                halign_center,
                valign_bottom,
                events.as_ptr(),
                colorids_color_14_brightyellow as c_int,
            );
        }

        // door events that point here
        let mut events = [0 as c_char; 256 * 4];
        let mut events_pos: c_int = 0;
        let mut event = 0;
        while event < 256 && events_pos < events.len() as c_int {
            if event_used[event as usize]
                && get_doorlink_room(event as c_short) as c_int == drawn_room as c_int
                && get_doorlink_tile(event as c_short) as c_int == tilepos
            {
                let len = snprintf(
                    events.as_mut_ptr().add(events_pos as usize),
                    events.len() - events_pos as usize,
                    b"%d \0".as_ptr() as *const c_char,
                    event + EVENT_OFFSET,
                );
                if len < 0 {
                    break;
                }
                events_pos += len;
            }
            event += 1;
        }
        events_pos -= 1;
        if events_pos > 0 && events_pos < events.len() as c_int {
            events[events_pos as usize] = 0; // trim trailing space
        }
        if events[0] != 0 {
            let events_rect = rect(y, x, y + 63 - 3, x + 32 - 7);
            show_text_with_color(
                &events_rect,
                halign_center,
                valign_bottom,
                events.as_ptr(),
                colorids_color_14_brightyellow as c_int,
            );
        }

        // USE_TELEPORTS
        if tile_type == tiles_tiles_23_balcony_left as c_int && modifier != 0 {
            // snprintf(events, sizeof(number)=4, "%d", modifier) -- faithful: writes into events with size 4
            snprintf(
                events.as_mut_ptr(),
                4,
                b"%d\0".as_ptr() as *const c_char,
                modifier,
            );
            let number_rect = rect(y, x + 32, y + 63, x + 64);
            show_text_with_color(
                &number_rect,
                halign_center,
                valign_top,
                events.as_ptr(),
                colorids_color_14_brightyellow as c_int,
            );
        }

        // special events
        let mut special_event: *const c_char = core::ptr::null();
        let cl = current_level as c_int;
        let dr = drawn_room as c_int;

        if cl == 0 && dr == cu!(demo_end_room) {
            special_event = b"exit\0".as_ptr() as *const c_char;
        }

        if cl == 1 && dr == 5 && tilepos == 2 {
            special_event = b"start\ntrig\0".as_ptr() as *const c_char;
        }

        if cl == 3 && dr == 7 && col == 0 {
            special_event = b"<-\nchk point\0".as_ptr() as *const c_char;
        }

        if cl == cu!(checkpoint_level)
            && dr == cu!(checkpoint_clear_tile_room)
            && tilepos == cu!(checkpoint_clear_tile_col) * 10 + cu!(checkpoint_clear_tile_row)
        {
            special_event = b"removed\0".as_ptr() as *const c_char;
        }

        if cl == 3 && dr == 2 && tile_type == tiles_tiles_4_gate as c_int {
            special_event = b"loud\0".as_ptr() as *const c_char;
        }

        if cl == cu!(checkpoint_level)
            && dr == cu!(checkpoint_respawn_room)
            && tilepos == cu!(checkpoint_respawn_tilepos)
        {
            special_event = b"check point\0".as_ptr() as *const c_char;
        }

        if cl == cu!(skeleton_level)
            && dr == cu!(skeleton_room)
            && tilepos == cu!(skeleton_row) * 10 + cu!(skeleton_column)
            && tile_type == tiles_tiles_21_skeleton as c_int
        {
            special_event = b"skel wake\0".as_ptr() as *const c_char;
        }

        if cl == cu!(skeleton_level)
            && dr == cu!(skeleton_reappear_room)
            && tilepos == cu!(skeleton_reappear_row) * 10 + (cu!(skeleton_reappear_x) - 58) / 14
        {
            special_event = b"skel cont\0".as_ptr() as *const c_char;
        }

        if cl == cu!(mirror_level)
            && dr == cu!(mirror_room)
            && tilepos == cu!(mirror_row) * 10 + cu!(mirror_column)
        {
            special_event = b"mirror\0".as_ptr() as *const c_char;
        }

        if cl == cu!(shadow_steal_level)
            && dr == cu!(shadow_steal_room)
            && tilepos == 3
            && tile_type == tiles_tiles_10_potion as c_int
        {
            special_event = b"stolen\0".as_ptr() as *const c_char;
        }

        if cl == cu!(falling_exit_level) && dr == cu!(falling_exit_room) && row == 2 {
            special_event = b"exit\ndown\0".as_ptr() as *const c_char;
        }

        if cl == cu!(mouse_level) && dr == cu!(mouse_room) && tilepos == 9 {
            special_event = b"mouse\0".as_ptr() as *const c_char;
        }

        if cl == 12 && dr == 15 && tilepos == 1 && tile_type == tiles_tiles_22_sword as c_int {
            special_event = b"disapp\0".as_ptr() as *const c_char;
        }

        if cl == 12 && dr == 18 && col == 9 {
            special_event = b"disapp\n->\0".as_ptr() as *const c_char;
        }

        if cl == 12 && row == 0 && (dr == 2 || (dr == 13 && col >= 6)) {
            special_event = b"floor\0".as_ptr() as *const c_char;
        }

        if dr == cua!(tbl_seamless_exit, cl as usize) {
            special_event = b"exit\0".as_ptr() as *const c_char;
        }

        if cl == cu!(loose_tiles_level)
            && (dr == level.roomlinks[(cu!(loose_tiles_room_1) - 1) as usize].up as c_int
                || dr == level.roomlinks[(cu!(loose_tiles_room_2) - 1) as usize].up as c_int)
            && (tilepos >= cu!(loose_tiles_first_tile) && tilepos <= cu!(loose_tiles_last_tile))
        {
            special_event = b"fall\0".as_ptr() as *const c_char;
        }

        if cl == 13 && dr == 3 && col == 9 {
            special_event = b"meet\n->\0".as_ptr() as *const c_char;
        }

        if cl == 13 && dr == 24 && tilepos == 0 {
            special_event = b"Jffr\ntrig\0".as_ptr() as *const c_char;
        }

        if cl == cu!(win_level) && dr == cu!(win_room) {
            special_event = b"end\0".as_ptr() as *const c_char;
        }

        if has_trigger_potion && dr == 8 && tilepos == 0 {
            special_event = b"blue\ntrig\0".as_ptr() as *const c_char;
        }

        if !special_event.is_null() {
            let event_rect = rect(y, x - 10, y + 63, x + 32 + 10);
            show_text_with_color(
                &event_rect,
                halign_center,
                valign_middle,
                special_event,
                colorids_color_14_brightyellow as c_int,
            );
        }

        // Attempt to show broken room links:
        let roomlinks = core::ptr::addr_of!(level.roomlinks[(dr - 1) as usize]) as *const u8;
        for direction in 0..4 {
            let other_room = *roomlinks.add(direction as usize) as c_int;
            if other_room >= 1 && other_room <= NUMBER_OF_ROOMS {
                let other_x = xpos[dr as usize] + dx[direction as usize];
                let other_y = ypos[dr as usize] + dy[direction as usize];
                if xpos[other_room as usize] != other_x || ypos[other_room as usize] != other_y {
                    let center_x = 160 + dx[direction as usize] * 150;
                    let center_y = 96 + dy[direction as usize] * 85;
                    let text_rect = rect(center_y - 6, center_x - 10, center_y + 6, center_x + 10);
                    let mut room_num = [0 as c_char; 4];
                    snprintf(
                        room_num.as_mut_ptr(),
                        4,
                        b"%d\0".as_ptr() as *const c_char,
                        other_room,
                    );
                    method_5_rect(&text_rect, 0, colorids_color_4_red as byte);
                    show_text_with_color(
                        &text_rect,
                        halign_center,
                        valign_middle,
                        room_num.as_ptr(),
                        colorids_color_15_brightwhite as c_int,
                    );
                }
            }
        }

        // start pos
        if level.start_room as c_int == dr && level.start_pos as c_int == tilepos {
            let mut start_dir: u8 = level.start_dir as u8;
            if cl == 1 || cl == 13 {
                start_dir ^= 0xFF; // falling/running entry
            }
            let start_text = if start_dir == directions_dir_0_right as u8 {
                b"start\n->\0".as_ptr() as *const c_char
            } else {
                b"start\n<-\0".as_ptr() as *const c_char
            };
            let start_rect = rect(y, x - 10, y + 63, x + 32 + 10);
            show_text_with_color(
                &start_rect,
                halign_center,
                valign_middle,
                start_text,
                colorids_color_14_brightyellow as c_int,
            );
        }

        // guard info
        if Guard.direction as c_int != directions_dir_56_none as c_int
            && tilepos == Guard.curr_row as c_int * 10 + Guard.curr_col as c_int
        {
            loadshad();
            load_frame_to_obj();
            let mut screen_x = calc_screen_x_coord(obj_x) as c_int;
            // Put it above the guard's head.
            if Guard.direction as c_int == directions_dir_0_right as c_int {
                screen_x -= 10;
            } else {
                screen_x += 10;
            }

            let event_rect = rect(y + 2, screen_x - 16 - 10, y + 63, screen_x + 16 + 10);
            let mut guard_info = [0 as c_char; 20];
            snprintf(
                guard_info.as_mut_ptr(),
                20,
                b"s%d h%d\0".as_ptr() as *const c_char,
                guard_skill as c_int,
                guardhp_max as c_int,
            );
            show_text_with_color(
                &event_rect,
                halign_center,
                valign_top,
                guard_info.as_ptr(),
                colorids_color_14_brightyellow as c_int,
            );
        }
    }

    // room number
    let mut room_num = [0 as c_char; 6];
    snprintf(
        room_num.as_mut_ptr(),
        6,
        b"%d\0".as_ptr() as *const c_char,
        drawn_room as c_int,
    );
    let text_rect = rect(10, 10, 21, 30);
    method_5_rect(&text_rect, 0, colorids_color_8_darkgray as byte);
    show_text_with_color(
        &text_rect,
        halign_center,
        valign_middle,
        room_num.as_ptr(),
        colorids_color_15_brightwhite as c_int,
    );

    // grid lines
    let vline = rect(0, 0, 192, 1);
    method_5_rect(&vline, 0, colorids_color_12_brightred as byte);
    let hline = rect(3, 0, 4, 320);
    method_5_rect(&hline, 0, colorids_color_12_brightred as byte);
}

// Save a "screenshot" of the whole level.
#[no_mangle]
pub unsafe extern "C" fn save_level_screenshot(want_extras: bool) {
    // Restrict this to cheat mode.
    if cheats_enabled == 0 {
        return;
    }

    upside_down = 0;

    // First, figure out where to put each room.
    let mut processed = [false; (NUMBER_OF_ROOMS + 1) as usize];
    for room in 1..=NUMBER_OF_ROOMS {
        xpos[room as usize] = 0;
        ypos[room as usize] = 0;
    }
    xpos[drawn_room as usize] = 0;
    ypos[drawn_room as usize] = 0;
    processed[drawn_room as usize] = true;
    let mut queue = [0 as c_int; NUMBER_OF_ROOMS as usize];
    queue[0] = drawn_room as c_int;
    let mut queue_start: c_int = 0;
    let mut queue_end: c_int = 1;

    // Assemble a map based on room links.
    while queue_start < queue_end {
        let room = queue[queue_start as usize];
        queue_start += 1;
        let roomlinks = core::ptr::addr_of!(level.roomlinks[(room - 1) as usize]) as *const u8;
        for direction in 0..4 {
            let other_room = *roomlinks.add(direction as usize) as c_int;
            if other_room >= 1
                && other_room <= NUMBER_OF_ROOMS
                && !processed[other_room as usize]
            {
                let other_x = xpos[room as usize] + dx[direction as usize];
                let other_y = ypos[room as usize] + dy[direction as usize];
                xpos[other_room as usize] = other_x;
                ypos[other_room as usize] = other_y;
                processed[other_room as usize] = true;
                printf(b"Adding room %d to map.\n\0".as_ptr() as *const c_char, other_room);
                if queue_end >= NUMBER_OF_ROOMS {
                    printf(b"Queue overflow!\n\0".as_ptr() as *const c_char);
                    break;
                }
                queue[queue_end as usize] = other_room;
                queue_end += 1;
            }
        }
    }

    // Find the bounds of the level.
    let mut min_x: c_int = 0;
    let mut max_x: c_int = 0;
    let mut min_y: c_int = 0;
    let mut max_y: c_int = 0;
    for room in 1..=NUMBER_OF_ROOMS {
        if xpos[room as usize] < min_x {
            min_x = xpos[room as usize];
        }
        if xpos[room as usize] > max_x {
            max_x = xpos[room as usize];
        }
        if ypos[room as usize] < min_y {
            min_y = ypos[room as usize];
        }
        if ypos[room as usize] > max_y {
            max_y = ypos[room as usize];
        }
    }

    // Position for rooms that would clash with other rooms.
    let clash_y = max_y + 1;
    let mut clash_x = min_x;

    const MAX_MAP_SIZE: c_int = NUMBER_OF_ROOMS;
    let mut map = [[0 as c_int; MAX_MAP_SIZE as usize]; MAX_MAP_SIZE as usize];
    for room in 1..=NUMBER_OF_ROOMS {
        if processed[room as usize] {
            'again: loop {
                let y = ypos[room as usize] - min_y;
                let x = xpos[room as usize] - min_x;
                if x >= 0 && y >= 0 && x < MAX_MAP_SIZE && y < MAX_MAP_SIZE {
                    if map[y as usize][x as usize] != 0 {
                        printf(
                            b"Warning: room %d was mapped to the same place as room %d!\n\0".as_ptr()
                                as *const c_char,
                            room,
                            map[y as usize][x as usize],
                        );
                        // Try to find some other place for this room.
                        xpos[room as usize] = clash_x;
                        ypos[room as usize] = clash_y;
                        clash_x += 1;
                        if xpos[room as usize] > max_x {
                            max_x = xpos[room as usize];
                        }
                        if ypos[room as usize] > max_y {
                            max_y = ypos[room as usize];
                        }
                        continue 'again; // Force bounds check, just to be sure.
                    }
                    map[y as usize][x as usize] = room;
                } else {
                    printf(
                        b"Warning: room %d was mapped outside the map: x = %d, y = %d.\n\0".as_ptr()
                            as *const c_char,
                        room,
                        x,
                        y,
                    );
                }
                break;
            }
        }
    }

    let map_width = max_x - min_x + 1;
    let map_height = max_y - min_y + 1;

    // Now we have the arrangement, let's make the picture!
    let image_width = map_width * 320;
    let image_height = map_height * 189 + 3 + 8;

    let map_surface = SDL_CreateRGBSurface(
        0,
        image_width,
        image_height,
        32,
        Rmsk,
        Gmsk,
        Bmsk,
        Amsk,
    );
    if map_surface.is_null() {
        sdlperror(b"SDL_CreateRGBSurface (map_surface)\0".as_ptr() as *const c_char);
        return;
    }

    has_trigger_potion = false;

    // Is there a trigger potion on the level?
    for room in 1..=NUMBER_OF_ROOMS {
        if processed[room as usize] {
            get_room_address(room);
            for tilepos in 0..30 {
                let tile_type = (*curr_room_tiles.add(tilepos as usize) & 0x1F) as c_int;
                if tile_type == tiles_tiles_10_potion as c_int
                    && (*curr_room_modif.add(tilepos as usize) >> 3) as c_int == 6
                {
                    has_trigger_potion = true;
                }
            }
        }
    }

    memset(event_used.as_mut_ptr() as *mut c_void, 0, 256);

    // Find out which door events are used:
    for room in 1..=NUMBER_OF_ROOMS {
        if processed[room as usize] {
            get_room_address(room);
            for tilepos in 0..30 {
                let tile_type = (*curr_room_tiles.add(tilepos as usize) & 0x1F) as c_int;
                if tile_type == tiles_tiles_6_closer as c_int
                    || tile_type == tiles_tiles_15_opener as c_int
                    || (has_trigger_potion && room == 8 && tilepos == 0)
                {
                    let modifier = *curr_room_modif.add(tilepos as usize) as c_int;
                    let mut index = modifier;
                    while index < 256 {
                        event_used[index as usize] = true;
                        if get_doorlink_next(index as c_short) == 0 {
                            break;
                        }
                        index += 1;
                    }
                }
            }
        }
    }

    let old_room = drawn_room as c_int;
    for y in 0..map_height {
        for x in 0..map_width {
            let room = map[y as usize][x as usize];
            if room != 0 {
                let mut dest_rect = SDL_Rect {
                    x: x * 320,
                    y: y * 189,
                    w: 0,
                    h: 0,
                };
                switch_to_room(room);

                if want_extras {
                    draw_extras();
                }

                SDL_UpperBlit(onscreen_surface_, core::ptr::null(), map_surface, &mut dest_rect);
            }
        }
    }
    switch_to_room(old_room);

    make_screenshot_filename();
    let result = IMG_SavePNG(map_surface, screenshot_filename.as_ptr() as *const c_char);
    show_result(result, b"level map\0".as_ptr() as *const c_char);

    SDL_FreeSurface(map_surface);
}

#[no_mangle]
pub unsafe extern "C" fn init_screenshot() {
    // Command-line options to automatically save a screenshot at startup.
    let screenshot_param = check_param(b"--screenshot\0".as_ptr() as *const c_char);
    if !screenshot_param.is_null() {
        // We require megahit+levelnumber.
        if start_level < 0 {
            printf(
                b"You must supply a level number if you want to make an automatic screenshot!\n\0"
                    .as_ptr() as *const c_char,
            );
            exit(1);
        } else {
            want_auto = true;
            want_auto_whole_level =
                !check_param(b"--screenshot-level\0".as_ptr() as *const c_char).is_null();
            want_auto_extras =
                !check_param(b"--screenshot-level-extras\0".as_ptr() as *const c_char).is_null();
        }
    }
}

// To skip cutscenes, etc.
#[no_mangle]
pub unsafe extern "C" fn want_auto_screenshot() -> bool {
    want_auto
}

// Called when the level is drawn for the first time.
#[no_mangle]
pub unsafe extern "C" fn auto_screenshot() {
    if !want_auto {
        return;
    }

    if want_auto_whole_level {
        save_level_screenshot(want_auto_extras);
    } else {
        save_screenshot();
    }

    quit(1);
}
