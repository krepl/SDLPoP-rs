//! Saving pictures of the game: one screen, or a whole level laid out as a map.
//!
//! Two very different features share this module because they share a filename
//! generator and a "did it save?" toast:
//!
//! * **Screenshot** ([`save_screenshot`]) is the cheap one — hand the final
//!   post-scaling surface to `IMG_SavePNG` and report the result.
//!
//! * **Level map** ([`save_level_screenshot`]) is the interesting one. The
//!   original game has no map: a level is 24 rooms wired together by
//!   `level.roomlinks`, four neighbour room numbers per room, with no stored
//!   coordinates. So the map is *reconstructed* by breadth-first search from
//!   the room the Kid is standing in — that room is pinned at (0, 0) and every
//!   room reached through a link is placed one cell away in the corresponding
//!   direction ([`dx`] / [`dy`]). Because the links are hand-authored and not
//!   required to be geometrically consistent, two rooms can land on the same
//!   cell; the loser is parked in a spare row below the map rather than
//!   dropped. Rooms unreachable from the start room never get placed at all.
//!   Then each placed room is drawn for real — via [`switch_to_room`], which
//!   drives the ordinary renderer rather than reimplementing it — and blitted
//!   into one big surface.
//!
//! ## Extras
//!
//! With `want_extras`, [`draw_extras`] overlays the things a screenshot cannot
//! show, which is most of what actually makes a level: which loose floors are
//! stable, which spikes are already harmless, which chompers are jammed, what
//! is in each potion, which door events a pressure plate fires and which
//! events point *at* a given gate, where the level's scripted moments live
//! (checkpoints, the skeleton, the mirror, the shadow's theft, Jaffar), where
//! the Kid starts and which way he faces, and each guard's skill and HP. It
//! also flags *broken* room links: if a neighbour link points at a room that
//! the BFS placed somewhere else, the offending room number is stamped in red
//! on that edge. This is deliberately more information than the game gives
//! you, which is why the whole feature is gated behind cheat mode.
//!
//! ## Automatic capture
//!
//! [`init_screenshot`] parses `--screenshot`, `--screenshot-level` and
//! `--screenshot-level-extras`; when one is present the game skips cutscenes
//! (see [`want_auto_screenshot`]), and [`auto_screenshot`] fires once the
//! level is first drawn and then exits. That makes the binary usable as a
//! batch level-map renderer.

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_short, c_uint};
use super::*;
use crate::platform::Renderer;

extern "C" {
    fn snprintf(buf: *mut c_char, size: usize, fmt: *const c_char, ...) -> c_int;
    fn printf(fmt: *const c_char, ...) -> c_int;
    fn fprintf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    fn mkdir(path: *const c_char, mode: c_uint) -> c_int;
    fn exit(code: c_int) -> !;
    static mut stderr: *mut FILE;
}

// File-scope globals (defined in screenshot.c, not exported via data.h).

/// Builds a NUL-padded fixed-size C string buffer at compile time, so the
/// statics below can carry the same initialisers the C file gives them.
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
/// Serial number of the next screenshot; scanned upwards past files that
/// already exist so a session never overwrites an earlier one.
static mut screenshot_index: c_int = 0;

/// Added to every displayed event number. Use 1 for Apoplexy compatibility.
const EVENT_OFFSET: c_int = 0;

/// Rooms per level, and therefore also the widest and tallest a reconstructed
/// map can be (a 24-room chain laid out in a straight line).
const NUMBER_OF_ROOMS: c_int = 24;

/// Which of the 256 door events are reached from some pressure plate on this
/// level. Filled in by [`save_level_screenshot`] before any room is drawn,
/// because [`draw_extras`] annotates a gate with the events pointing at it and
/// those events live in rooms it has not visited yet.
static mut event_used: [bool; 256] = [false; 256];

/// Whether this level contains an "open" potion, which triggers the event list
/// starting at room 8 tile 0 when drunk. Same reason as `event_used`: known
/// only after a whole-level sweep.
static mut has_trigger_potion: bool = false;

/// Delta vectors for room links, indexed by the link's slot in `roomlinks`:
/// left, right, up, down.
static dx: [c_int; 4] = [-1, 1, 0, 0];
static dy: [c_int; 4] = [0, 0, -1, 1];

/// Map cell assigned to each room by the BFS in [`save_level_screenshot`], in
/// map units (one room wide/tall), relative to the starting room at (0, 0).
/// Read back by [`draw_extras`] to detect links that disagree with the layout.
static mut xpos: [c_int; (NUMBER_OF_ROOMS + 1) as usize] = [0; (NUMBER_OF_ROOMS + 1) as usize];
static mut ypos: [c_int; (NUMBER_OF_ROOMS + 1) as usize] = [0; (NUMBER_OF_ROOMS + 1) as usize];

/// Command-line driven automatic capture, set up by [`init_screenshot`].
static mut want_auto: bool = false;
static mut want_auto_whole_level: bool = false;
static mut want_auto_extras: bool = false;

/// Builds a `rect_type`, whose fields are declared in C in the order
/// `{top, left, bottom, right}` — the order the brace initialisers in
/// `screenshot.c` rely on.
fn rect(top: c_int, left: c_int, bottom: c_int, right: c_int) -> rect_type {
    rect_type {
        top: top as c_short,
        left: left as c_short,
        bottom: bottom as c_short,
        right: right as c_short,
    }
}

/// The `snprintf_check` macro from `common.h`: format, and abort the game if
/// the result did not fit. `func` stands in for C's `__func__`.
macro_rules! snprintf_check {
    ($func:literal, $dst:expr, $size:expr, $($arg:tt)*) => {{
        let len = snprintf($dst, $size as usize, $($arg)*);
        if len < 0 || len >= $size as c_int {
            fprintf(
                stderr,
                b"%s: buffer truncation detected!\n\0".as_ptr() as *const c_char,
                concat!($func, "\0").as_ptr() as *const c_char,
            );
            quit(2);
        }
    }};
}

/// Picks the filename for the next capture: `screenshots/screenshot_NNN.png`,
/// using incrementing numbers and a separate folder, like DOSBox.
///
/// The folder is resolved through `locate_file` so it lands next to the
/// executable rather than in whatever the current directory happens to be
/// (replay playback, for one, `chdir`s away).
unsafe fn make_screenshot_filename() {
    // Create the screenshots directory in SDLPoP's directory, even if the current directory is something else.
    let mut lf_buf = [0 as c_char; POP_MAX_PATH as usize];
    let located = locate_file_(
        b"screenshots\0".as_ptr() as *const c_char,
        lf_buf.as_mut_ptr(),
        POP_MAX_PATH as c_int,
    );
    snprintf_check!(
        "make_screenshot_filename",
        screenshots_folder.as_mut_ptr(),
        POP_MAX_PATH,
        b"%s\0".as_ptr() as *const c_char,
        located
    );
    // Create the folder if it doesn't exist yet:
    mkdir(screenshots_folder.as_ptr() as *const c_char, 0o700);
    // Find the first unused filename:
    loop {
        snprintf_check!(
            "make_screenshot_filename",
            screenshot_filename.as_mut_ptr(),
            POP_MAX_PATH,
            b"%s/screenshot_%03d.png\0".as_ptr() as *const c_char,
            screenshots_folder.as_ptr(),
            screenshot_index
        );
        if !file_exists(screenshot_filename.as_ptr() as *const c_char) {
            return;
        }
        screenshot_index += 1;
    }
}

/// Reports the outcome of an `IMG_SavePNG` (`result == 0` means success) both
/// on stdout, with the full path, and as a short toast on the status bar.
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
            crate::platform::sdl::shared_renderer().get_error(),
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

/// Saves a screenshot of what is currently on screen.
///
/// Captures the *final* surface — after scaling and any lighting/fade effects
/// — so the PNG matches what the player sees rather than the 320x200 original.
#[no_mangle]
pub unsafe extern "C" fn save_screenshot() {
    make_screenshot_filename();
    let result = crate::platform::sdl::shared_renderer().save_png(get_final_surface(), std::ffi::CStr::from_ptr(screenshot_filename.as_ptr()));
    show_result(result, b"screenshot\0".as_ptr() as *const c_char);
}

/// Makes `room` the drawn room and renders it into `onscreen_surface_`.
///
/// The map is built by really drawing each room with the ordinary renderer, so
/// this has to reproduce the parts of a room transition that the renderer
/// depends on but that would normally be done by gameplay: reload the
/// neighbour links, regenerate the palace wall pattern on palace levels, and
/// re-seat the opponent. Each step also undoes state left over from the
/// *previous* room in the sweep — see the per-line notes, which come from the
/// C source.
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
    // A potion's low three modifier bits are its bubble animation phase; phase 0
    // draws no bubbles at all, so nudge those potions to phase 1 to make them
    // visible in a still picture. This edits the live level data, but the map is
    // only ever taken in cheat mode.
    for tilepos in 0..30usize {
        let tile_type = (*curr_room_tiles.add(tilepos) & 0x1F) as c_int;
        if tile_type == tiles_tiles_10_potion as c_int {
            let modifier = *curr_room_modif.add(tilepos) as c_int;
            if (modifier & 7) == 0 {
                *curr_room_modif.add(tilepos) = (*curr_room_modif.add(tilepos)).wrapping_add(1);
            }
        }
    }

    redraw_screen(1);
}

/// Appends `"<number> "` to `events` at `events_pos`, advancing it.
///
/// Returns false if `snprintf` failed, which the C code treats as a reason to
/// stop building the list. Note that, as in C, `events_pos` advances by the
/// length `snprintf` *would* have written, so it can end up past the text that
/// actually fits; callers guard the next iteration against the buffer size.
unsafe fn append_event_number(events: &mut [c_char], events_pos: &mut c_int, event: c_int) -> bool {
    let len = snprintf(
        events.as_mut_ptr().add(*events_pos as usize),
        events.len() - *events_pos as usize,
        b"%d \0".as_ptr() as *const c_char,
        event + EVENT_OFFSET,
    );
    if len < 0 {
        return false; // snprintf might return -1 if the buffer is too small.
    }
    *events_pos += len;
    true
}

/// Drops the trailing space left by the last [`append_event_number`].
fn trim_trailing_space(events: &mut [c_char], events_pos: c_int) {
    let last = events_pos - 1;
    if last > 0 && last < events.len() as c_int {
        events[last as usize] = 0;
    }
}

/// Overlays annotations for everything a picture of the room cannot show:
/// room bounds and number, door events, loose floors, potion types, special
/// events, guard stats, and broken room links.
///
/// (This makes the level map even more like a cheat, which is why the caller
/// requires cheat mode.)
///
/// Operates on the room [`switch_to_room`] just drew, and writes over
/// `onscreen_surface_` before it is blitted into the map.
///
/// TODO: fake tiles?
unsafe fn draw_extras() {
    // Read a scalar field of the packed `custom_options_type` as an `int`.
    // `custom` is 1-byte packed, so a field must be reached through addr_of! +
    // read_unaligned rather than a reference. Widening to c_int mirrors C's
    // integer promotion, and matters for the arithmetic some fields feed into.
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
    // The editor branch has something similar...
    for tilepos in 0..30 {
        let tile_type = (*curr_room_tiles.add(tilepos as usize) & 0x1F) as c_int;
        let modifier = *curr_room_modif.add(tilepos as usize) as c_int;
        let row = tilepos / 10;
        let col = tilepos % 10;
        let y = row * 63 + 3;
        let x = col * 32;

        // special floors
        let mut floor_rect = rect(y + 60 - 3, x, y + 63 - 3, x + 32);

        // loose floors and buttons: all three are floor tiles that look alike.
        if tile_type == tiles_tiles_11_loose as c_int {
            let color = if *curr_room_tiles.add(tilepos as usize) & 0x20 != 0 {
                colorids_color_13_brightmagenta as c_int // stable loose floor
            } else {
                colorids_color_15_brightwhite as c_int
            };
            show_text_with_color(
                &floor_rect,
                halign_center,
                valign_top,
                b"~~~~\0".as_ptr() as *const c_char,
                color,
            );
        } else if tile_type == tiles_tiles_15_opener as c_int {
            show_text_with_color(
                &floor_rect,
                halign_center,
                valign_top,
                b"^^^^\0".as_ptr() as *const c_char,
                colorids_color_10_brightgreen as c_int,
            );
        } else if tile_type == tiles_tiles_6_closer as c_int {
            floor_rect.top -= 2;
            // Only the top half is visible, looks like an inverted "^" or a tiny "v".
            show_text_with_color(
                &floor_rect,
                halign_center,
                valign_top,
                b"xxxx\0".as_ptr() as *const c_char,
                colorids_color_12_brightred as c_int,
            );
        }

        // Is this tile currently being animated? `trob` is left pointing at the
        // last entry examined, exactly as the C loop leaves it; the search
        // short-circuits, so `any` keeps that side effect identical.
        let is_trob_here = (0..trobs_count).any(|index| {
            trob = trobs[index as usize];
            trob.room as c_int == drawn_room as c_int && trob.tilepos as c_int == tilepos
        });

        if !is_trob_here {
            // It's not stuck if it's currently animated.
            // harmless spikes
            if tile_type == tiles_tiles_2_spike as c_int && modifier >= 5 {
                let spike_rect = rect(y + 50, x, y + 60, x + 32);
                show_text_with_color(
                    &spike_rect,
                    halign_center,
                    valign_top,
                    b"safe\0".as_ptr() as *const c_char,
                    colorids_color_10_brightgreen as c_int,
                );
            }

            // stuck chompers
            if tile_type == tiles_tiles_18_chomper as c_int {
                let frame = modifier & 0x7F;
                if frame != 0 {
                    let chomper_rect = rect(y, x - 10, y + 60, x + 32 + 10);
                    // Frame 2 is the closed position, i.e. stuck lethal.
                    let color = if frame == 2 {
                        colorids_color_12_brightred as c_int
                    } else {
                        colorids_color_10_brightgreen as c_int
                    };
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
            // `temp_text` must outlive the `text` pointer that may point into it.
            let mut temp_text = [0 as c_char; 4];
            let (color, text) = if (0..7).contains(&potion_type) {
                pot_types[potion_type as usize]
            } else {
                // Unknown potion: just print the number.
                snprintf(
                    temp_text.as_mut_ptr(),
                    4,
                    b"%d\0".as_ptr() as *const c_char,
                    potion_type,
                );
                (
                    colorids_color_15_brightwhite as c_int,
                    temp_text.as_ptr() as *const c_char,
                )
            };
            let pot_rect = rect(y + 40, x, y + 60, x + 32);
            show_text_with_color(&pot_rect, halign_center, valign_top, text, color);
        }

        // triggered door events
        // A pressure plate's modifier is an index into the flat doorlink table,
        // and the "next" flag chains consecutive entries, so one plate can fire a
        // whole run of events. List the run.
        if tile_type == tiles_tiles_6_closer as c_int
            || tile_type == tiles_tiles_15_opener as c_int
            // These tiles are triggered even if they are not buttons!
            // triggered when player drinks an open potion:
            || (has_trigger_potion && drawn_room as c_int == 8 && tilepos == 0)
        {
            let first_event = modifier;
            let mut last_event = modifier;
            while last_event < 256 && get_doorlink_next(last_event as c_short) != 0 {
                last_event += 1;
            }
            // More than enough space to list all the numbers from 0 to 255.
            let mut events = [0 as c_char; 256 * 4];
            let mut events_pos: c_int = 0;
            for event in first_event..=last_event {
                if events_pos >= events.len() as c_int
                    || !append_event_number(&mut events, &mut events_pos, event)
                {
                    break;
                }
            }
            trim_trailing_space(&mut events, events_pos);
            let buttonmod_rect = rect(y, x, y + 60 - 3, x + 32);
            show_text_with_color(
                &buttonmod_rect,
                halign_center,
                valign_bottom,
                events.as_ptr(),
                colorids_color_14_brightyellow as c_int,
            );
        }

        // TODO: Add an option to merge events pointing to the same tile?

        // door events that point here
        // The reverse lookup: which of the level's live events target this tile.
        // This is what tells you which plate opens a given gate.
        let mut events = [0 as c_char; 256 * 4];
        let mut events_pos: c_int = 0;
        for event in 0..256 {
            if events_pos >= events.len() as c_int {
                break;
            }
            if event_used[event as usize]
                && get_doorlink_room(event as c_short) as c_int == drawn_room as c_int
                && get_doorlink_tile(event as c_short) as c_int == tilepos
                && !append_event_number(&mut events, &mut events_pos, event)
            {
                break;
            }
        }
        trim_trailing_space(&mut events, events_pos);
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

        // USE_TELEPORTS: a balcony-left tile with a non-zero modifier is a
        // teleport; show its destination number next to it.
        if tile_type == tiles_tiles_23_balcony_left as c_int && modifier != 0 {
            // screenshot.c writes into `events` but passes `sizeof(number)` (4)
            // as the size, `number` being an unused local. Reproduced as-is: the
            // effect is that the number is truncated to three digits, and it also
            // clobbers the start of whatever the reverse-lookup left in `events`.
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

        // Special events: the scripted moments hard-coded into the gameplay code
        // rather than stored in the level. Kept as a flat run of independent
        // `if`s, in C's order, because they are not mutually exclusive and the
        // last one that matches wins. Most of the room/tile numbers are
        // `custom->` fields so that mods can move the set-pieces; the commented
        // numbers in screenshot.c are the original game's values.
        let mut special_event: *const c_char = core::ptr::null();
        let cl = current_level as c_int;
        let dr = drawn_room as c_int;

        if cl == 0 && dr == cu!(demo_end_room) {
            special_event = b"exit\0".as_ptr() as *const c_char; // exit by entering this room
        }

        // not marked: level 1 falling entry

        if cl == 1 && dr == 5 && tilepos == 2 {
            special_event = b"start\ntrig\0".as_ptr() as *const c_char; // triggered at start
        }

        if cl == 3 && dr == 7 && col == 0 {
            special_event = b"<-\nchk point\0".as_ptr() as *const c_char; // checkpoint activation
        }

        if cl == cu!(checkpoint_level)
            && dr == cu!(checkpoint_clear_tile_room)
            && tilepos == cu!(checkpoint_clear_tile_col) * 10 + cu!(checkpoint_clear_tile_row)
        {
            // this loose floor is removed when restarting at the checkpoint
            special_event = b"removed\0".as_ptr() as *const c_char;
        }

        if cl == 3 && dr == 2 && tile_type == tiles_tiles_4_gate as c_int {
            special_event = b"loud\0".as_ptr() as *const c_char; // closing can be heard everywhere
        }

        if cl == cu!(checkpoint_level)
            && dr == cu!(checkpoint_respawn_room)
            && tilepos == cu!(checkpoint_respawn_tilepos)
        {
            special_event = b"check point\0".as_ptr() as *const c_char; // restart at checkpoint
            // TODO: Show this room (and connected rooms) even if it is unreachable
            // from the start via room links?
        }

        if cl == cu!(skeleton_level)
            && dr == cu!(skeleton_room)
            && tilepos == cu!(skeleton_row) * 10 + cu!(skeleton_column)
            && tile_type == tiles_tiles_21_skeleton as c_int
        {
            special_event = b"skel wake\0".as_ptr() as *const c_char; // skeleton wakes
        }

        if cl == cu!(skeleton_level)
            && dr == cu!(skeleton_reappear_room)
            // skeleton_reappear_x is a pixel column; 58 is the left edge of tile 0
            // and 14 the tile width in the same units.
            && tilepos == cu!(skeleton_reappear_row) * 10 + (cu!(skeleton_reappear_x) - 58) / 14
        {
            // skeleton continues here if it falls into this room
            special_event = b"skel cont\0".as_ptr() as *const c_char;
        }

        if cl == cu!(mirror_level)
            && dr == cu!(mirror_room)
            && tilepos == cu!(mirror_row) * 10 + cu!(mirror_column)
        {
            special_event = b"mirror\0".as_ptr() as *const c_char; // mirror appears
        }

        // not marked: level 4 mirror clip
        // not marked: level 5 shadow, required opening gate

        if cl == cu!(shadow_steal_level)
            && dr == cu!(shadow_steal_room)
            && tilepos == 3
            && tile_type == tiles_tiles_10_potion as c_int
        {
            special_event = b"stolen\0".as_ptr() as *const c_char; // stolen potion
        }

        // not marked: level 6 shadow (it's already visible)

        if cl == cu!(falling_exit_level) && dr == cu!(falling_exit_room) && row == 2 {
            special_event = b"exit\ndown\0".as_ptr() as *const c_char; // exit by falling
        }

        // not marked: level 7 falling entry

        // tilepos 9 is the top right corner
        if cl == cu!(mouse_level) && dr == cu!(mouse_room) && tilepos == 9 {
            special_event = b"mouse\0".as_ptr() as *const c_char; // mouse comes
        }

        if cl == 12 && dr == 15 && tilepos == 1 && tile_type == tiles_tiles_22_sword as c_int {
            special_event = b"disapp\0".as_ptr() as *const c_char; // the sword disappears from here
        }

        if cl == 12 && dr == 18 && col == 9 {
            // the sword disappears if you exit this room
            special_event = b"disapp\n->\0".as_ptr() as *const c_char;
        }

        // not marked: level 12 shadow

        if cl == 12 && row == 0 && (dr == 2 || (dr == 13 && col >= 6)) {
            special_event = b"floor\0".as_ptr() as *const c_char; // floors appear
        }

        if dr == cua!(tbl_seamless_exit, cl as usize) {
            special_event = b"exit\0".as_ptr() as *const c_char; // exit by entering this room
        }

        if cl == cu!(loose_tiles_level)
            && (dr == level.roomlinks[(cu!(loose_tiles_room_1) - 1) as usize].up as c_int
                || dr == level.roomlinks[(cu!(loose_tiles_room_2) - 1) as usize].up as c_int)
            && (cu!(loose_tiles_first_tile)..=cu!(loose_tiles_last_tile)).contains(&tilepos)
        {
            special_event = b"fall\0".as_ptr() as *const c_char; // falling loose floors
        }

        if cl == 13 && dr == 3 && col == 9 {
            special_event = b"meet\n->\0".as_ptr() as *const c_char; // meet Jaffar
        }

        // not marked: flash

        if cl == 13 && dr == 24 && tilepos == 0 {
            // triggered when player enters any room from the right after Jaffar died
            special_event = b"Jffr\ntrig\0".as_ptr() as *const c_char;
        }

        if cl == cu!(win_level) && dr == cu!(win_room) {
            special_event = b"end\0".as_ptr() as *const c_char; // end of game
        }

        if has_trigger_potion && dr == 8 && tilepos == 0 {
            // triggered when player drinks an open potion
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
        // `roomlinks[room]` is four bytes — left, right, up, down — so it can be
        // walked as a byte array parallel to dx/dy. If a neighbour link points at
        // a room the BFS placed somewhere other than the adjacent cell, the link
        // is inconsistent; stamp the linked room's number on that edge in red.
        let roomlinks = core::ptr::addr_of!(level.roomlinks[(dr - 1) as usize]) as *const u8;
        for direction in 0..4usize {
            let other_room = *roomlinks.add(direction) as c_int;
            if (1..=NUMBER_OF_ROOMS).contains(&other_room) {
                let other_x = xpos[dr as usize] + dx[direction];
                let other_y = ypos[dr as usize] + dy[direction];
                if xpos[other_room as usize] != other_x || ypos[other_room as usize] != other_y {
                    let center_x = 160 + dx[direction] * 150;
                    let center_y = 96 + dy[direction] * 85;
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
            // loadshad + load_frame_to_obj put the guard's current animation
            // frame into the obj_* globals, which is where his screen x lives.
            loadshad();
            load_frame_to_obj();
            // Put it above the guard's head, offset away from his sword arm.
            let screen_x = calc_screen_x_coord(obj_x) as c_int
                + if Guard.direction as c_int == directions_dir_0_right as c_int {
                    -10
                } else {
                    10
                };

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
                // Yellow text is more readable than red.
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

    // grid lines: a one-pixel red border down the left edge and along the top,
    // so adjacent rooms in the map stay visually separated.
    let vline = rect(0, 0, 192, 1);
    method_5_rect(&vline, 0, colorids_color_12_brightred as byte);
    let hline = rect(3, 0, 4, 320);
    method_5_rect(&hline, 0, colorids_color_12_brightred as byte);
}

/// Saves a "screenshot" of the whole level: every room reachable from the one
/// the Kid is in, laid out as a map and written to a single PNG.
///
/// With `want_extras`, each room is also annotated by [`draw_extras`].
///
/// TODO: Disable in the intro or if a cutscene is active?
#[no_mangle]
pub unsafe extern "C" fn save_level_screenshot(want_extras: bool) {
    // Restrict this to cheat mode. After all, it's like using H/J/U/N or opening
    // the level in an editor.
    if cheats_enabled == 0 {
        return;
    }

    upside_down = 0;

    // First, figure out where to put each room.
    // We don't stop on broken room links, because the resulting map might still
    // be usable.
    let mut processed = [false; (NUMBER_OF_ROOMS + 1) as usize];
    for room in 1..=NUMBER_OF_ROOMS as usize {
        xpos[room] = 0;
        ypos[room] = 0;
    }
    xpos[drawn_room as usize] = 0;
    ypos[drawn_room as usize] = 0;
    // Mark the current room as processed so we don't add it later again.
    // Otherwise, if the level has NUMBER_OF_ROOMS rooms, the queue will
    // eventually contain NUMBER_OF_ROOMS+1 items, overflowing the array.
    processed[drawn_room as usize] = true;
    let mut queue = [0 as c_int; NUMBER_OF_ROOMS as usize];
    queue[0] = drawn_room as c_int; // We start mapping from the current room.
    let mut queue_start: c_int = 0;
    let mut queue_end: c_int = 1;

    // Assemble a map based on room links: a breadth-first walk that places each
    // newly reached room one cell away from the room that reached it.
    while queue_start < queue_end {
        let room = queue[queue_start as usize];
        queue_start += 1;
        let roomlinks = core::ptr::addr_of!(level.roomlinks[(room - 1) as usize]) as *const u8;
        for direction in 0..4usize {
            let other_room = *roomlinks.add(direction) as c_int;
            if (1..=NUMBER_OF_ROOMS).contains(&other_room) && !processed[other_room as usize] {
                xpos[other_room as usize] = xpos[room as usize] + dx[direction];
                ypos[other_room as usize] = ypos[room as usize] + dy[direction];
                processed[other_room as usize] = true;
                printf(
                    b"Adding room %d to map.\n\0".as_ptr() as *const c_char,
                    other_room,
                );
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
    // The starting room is mapped to x=0,y=0, so 0 is a good initial value for
    // max and min.
    let mut min_x: c_int = 0;
    let mut max_x: c_int = 0;
    let mut min_y: c_int = 0;
    let mut max_y: c_int = 0;
    for room in 1..=NUMBER_OF_ROOMS as usize {
        min_x = min_x.min(xpos[room]);
        max_x = max_x.max(xpos[room]);
        min_y = min_y.min(ypos[room]);
        max_y = max_y.max(ypos[room]);
    }

    // Position for rooms that would clash with other rooms: Below the normally
    // mapped rooms.
    let clash_y = max_y + 1;
    let mut clash_x = min_x;

    const MAX_MAP_SIZE: c_int = NUMBER_OF_ROOMS;
    let mut map = [[0 as c_int; MAX_MAP_SIZE as usize]; MAX_MAP_SIZE as usize];
    for room in 1..=NUMBER_OF_ROOMS {
        if !processed[room as usize] {
            continue;
        }
        // C reaches this point again via `goto again` after relocating a clashing
        // room, so the relocated position is bounds-checked too.
        'again: loop {
            let y = ypos[room as usize] - min_y;
            let x = xpos[room as usize] - min_x;
            if !(0..MAX_MAP_SIZE).contains(&x) || !(0..MAX_MAP_SIZE).contains(&y) {
                // Probably impossible...
                printf(
                    b"Warning: room %d was mapped outside the map: x = %d, y = %d.\n\0".as_ptr()
                        as *const c_char,
                    room,
                    x,
                    y,
                );
                break 'again;
            }
            if map[y as usize][x as usize] != 0 {
                printf(
                    b"Warning: room %d was mapped to the same place as room %d!\n\0".as_ptr()
                        as *const c_char,
                    room,
                    map[y as usize][x as usize],
                );
                // Try to find some other place for this room:
                // Put this room to the bottom of the map.
                xpos[room as usize] = clash_x;
                ypos[room as usize] = clash_y;
                clash_x += 1;
                max_x = max_x.max(xpos[room as usize]);
                max_y = max_y.max(ypos[room as usize]);
                continue 'again; // Force bounds check, just to be sure.
            }
            map[y as usize][x as usize] = room;
            break 'again;
        }
    }

    let map_width = max_x - min_x + 1;
    let map_height = max_y - min_y + 1;

    // Now we have the arrangement, let's make the picture!
    // Rooms overlap vertically: a room is 192 pixels tall but only 189 of them
    // are new, and the +3+8 tail leaves space for the last room's bottom edge.
    let image_width = map_width * 320;
    let image_height = map_height * 189 + 3 + 8;

    let map_surface = crate::platform::sdl::shared_renderer().create_surface(
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

    // TODO: Background color for places where there is no room?
    // TODO: Add an option for displaying all unreachable rooms?

    has_trigger_potion = false;

    // Is there a trigger potion on the level?
    // Both of the next two sweeps run over the whole level before any room is
    // drawn, because draw_extras annotates one room at a time but needs answers
    // that depend on all of them.
    for room in 1..=NUMBER_OF_ROOMS {
        if processed[room as usize] {
            get_room_address(room);
            for tilepos in 0..30usize {
                let tile_type = (*curr_room_tiles.add(tilepos) & 0x1F) as c_int;
                if tile_type == tiles_tiles_10_potion as c_int
                    && (*curr_room_modif.add(tilepos) >> 3) as c_int == 6
                {
                    has_trigger_potion = true;
                }
            }
        }
    }

    event_used.fill(false);

    // Find out which door events are used:
    for room in 1..=NUMBER_OF_ROOMS {
        if processed[room as usize] {
            get_room_address(room);
            for tilepos in 0..30usize {
                let tile_type = (*curr_room_tiles.add(tilepos) & 0x1F) as c_int;
                if tile_type == tiles_tiles_6_closer as c_int
                    || tile_type == tiles_tiles_15_opener as c_int
                    // These tiles are triggered even if they are not buttons!
                    // TODO: Force displaying of special trigger rooms even if they
                    // are unreachable via room links?
                    // triggered when player drinks an open potion:
                    || (has_trigger_potion && room == 8 && tilepos == 0)
                {
                    // Walk the chain of doorlink entries this plate fires.
                    let modifier = *curr_room_modif.add(tilepos) as c_int;
                    for index in modifier..256 {
                        event_used[index as usize] = true;
                        if get_doorlink_next(index as c_short) == 0 {
                            break;
                        }
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
                // SDL_UpperBlit reads only x/y from dstrect and overwrites w/h
                // with the clipped source size, which is why C can leave them
                // uninitialised here.
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

                // TODO: Hide the status bar, or maybe show some custom text on it?
                crate::platform::sdl::shared_renderer().blit(onscreen_surface_, core::ptr::null(), map_surface, &mut dest_rect);
            }
        }
    }
    switch_to_room(old_room);

    make_screenshot_filename();
    let result = crate::platform::sdl::shared_renderer().save_png(map_surface, std::ffi::CStr::from_ptr(screenshot_filename.as_ptr()));
    show_result(result, b"level map\0".as_ptr() as *const c_char);

    crate::platform::sdl::shared_renderer().free_surface(map_surface);
}

/// Parses the automatic-screenshot command-line options at startup.
///
/// `--screenshot` alone captures the screen; `--screenshot-level` captures the
/// whole level map; `--screenshot-level-extras` adds the annotations. A level
/// number (via `megahit`) is required, since there is nothing to capture
/// otherwise.
///
/// TODO: Don't open a window if the user wants an auto screenshot.
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

/// Whether an automatic capture was requested. Queried by the startup path to
/// skip cutscenes, etc.
#[no_mangle]
pub unsafe extern "C" fn want_auto_screenshot() -> bool {
    want_auto
}

/// Takes the requested automatic capture and exits. Called when the level is
/// drawn for the first time; a no-op if no capture was requested.
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
