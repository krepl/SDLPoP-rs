//! Level lifecycle: starting a level, running its per-frame loop, and repainting
//! the screen.
//!
//! This module owns the outermost gameplay control flow. [`play_level`] is an
//! infinite loop over levels: it plays the intro cutscene, loads the level data
//! and sprites, resets all per-level state (guards, moving objects, timers,
//! sword status), places the Kid at the level's start position, then hands
//! control to [`play_level_2`] — the per-frame loop that ticks timers, advances
//! one animation frame, and paints the result. When that loop returns, its
//! return value is the next level number and the outer loop starts over. The
//! only way out is a `longjmp` back into `start_game` (see `seg000`).
//!
//! The rest of the module falls into three groups:
//!
//! * **Level entry** — [`init_game`], [`do_startpos`], [`set_start_pos`] and
//!   [`find_start_level_door`] put the Kid on the level's starting tile facing
//!   the right way, with the correct HP and entry animation (falling in,
//!   running in, or turning around), and open the level door he came through.
//!
//! * **Redrawing** — [`redraw_screen`] repaints the whole room from scratch:
//!   room tiles, lighting, then the object tables that hold moving characters.
//!   [`redraw_at_char`] and [`redraw_at_char2`] are the cheap incremental
//!   versions used every frame — they mark only the tiles a character currently
//!   overlaps (plus the tiles he overlapped last frame, for the Kid) as dirty.
//!
//! * **Per-frame checks** — [`timers`] runs every countdown in the game
//!   (shadow union, guard alertness, resurrection, feather fall, the mouse's
//!   entrance, the super-jump landing). [`check_can_guard_see_kid`] walks the
//!   tiles between guard and Kid to decide whether the guard notices him.
//!   [`check_mirror`] handles the level-4 mirror: drawing the Kid's reflection
//!   while he runs past it, and spawning the shadow when he jumps through it.
//!
//! Screen coordinates here are in the original DOS units: a room is 10 columns
//! by 3 rows of tiles, `TILE_SIZEX` pixels wide each, and the `x_bump` /
//! `y_land` tables map tile columns and rows to pixel positions.

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;
use crate::platform::{InputSource, Renderer};
use crate::state::State;

extern "C" { fn dump_frame_state(); fn dump_frame_pixels(); fn dump_frame_raw(); }

/// How far the Kid is from the mirror's collision line, in pixels, minus two.
///
/// Written by [`check_mirror_image`] and read back by [`check_mirror`]: a
/// negative value means the Kid has not reached the mirror yet, so no
/// reflection is drawn. File-local in `seg003.c`, so it is not a shared global.
static mut distance_mirror: i8 = 0;

/// Top clipping y-coordinate for tile row `idx`, from the `y_clip` table.
///
/// The table is an incomplete `extern` array in C, so bindgen gives it length
/// zero and it has to be read through a raw pointer.
unsafe fn y_clip_at(idx: usize) -> i16 {
    *core::ptr::addr_of!(y_clip).cast::<i16>().add(idx)
}

/// Room number holding the `idx`-th copy-protection potion (level 15 only).
unsafe fn copyprot_room_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_room).cast::<u16>().add(idx)
}

/// Tile position of the `idx`-th copy-protection potion (level 15 only).
unsafe fn copyprot_tile_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_tile).cast::<u16>().add(idx)
}

/// Sets up the offscreen buffer and the run-wide counters, then never returns:
/// falls through into the endless level loop.
// seg003:0000
unsafe fn init_game_impl(state: &mut State, lev: c_int) {
    if !offscreen_surface.is_null() {
        free_surface(offscreen_surface);
        offscreen_surface = core::ptr::null_mut();
    }
    offscreen_surface = make_offscreen_buffer(&rect_top);
    load_kid_sprite();
    *state.text_time_remaining() = 0;
    *state.text_time_total() = 0;
    *state.is_show_time() = 0;
    *state.checkpoint() = 0;
    *state.upside_down() = 0;
    *state.resurrect_time() = 0;
    if dont_reset_time == 0 {
        *state.rem_min() = (*custom).start_minutes_left as c_short;
        *state.rem_tick() = (*custom).start_ticks_left;
        *state.hitp_beg_lev() = (*custom).start_hitp;
    }
    *state.need_level1_music() = (lev as u16 == (*custom).intro_music_level) as u16;
    play_level_impl(state, lev);
}

/// Starts a fresh game at level `lev`.
///
/// Allocates the offscreen drawing buffer, loads the Kid's sprites, resets the
/// clock and starting HP (unless a quickload asked to keep them), and enters
/// the level loop. Does not return.
#[no_mangle]
pub unsafe extern "C" fn init_game(lev: c_int) {
    init_game_impl(&mut State, lev);
}

/// The level loop: play level, take the next level number it returns, repeat.
// seg003:005C
unsafe fn play_level_impl(state: &mut State, mut level_number: c_int) {
    if enable_copyprot != 0 && level_number as u16 == (*custom).copyprot_level {
        level_number = 15;
    }
    loop {
        if *state.demo_mode() != 0 && level_number > 2 {
            start_level = -1;
            *state.need_quotes() = 1;
            start_game();
        }
        if level_number != *state.current_level() as c_int {
            if level_number < 0 || level_number > 15 {
                eprintln!("Tried to load cutscene for level {}, not in 0..15", level_number);
                quit(1);
            }
            let cutscene_func: cutscene_ptr_type =
                tbl_cutscenes[(*custom).tbl_cutscenes_by_index[level_number as usize] as usize];
            if cutscene_func.is_some()
                && (recording == 0 && replaying == 0)
                && !want_auto_screenshot()
            {
                load_intro((level_number > 2) as c_int, cutscene_func, 1);
            }
        }
        if level_number != *state.current_level() as c_int {
            load_lev_spr(level_number);
        }
        load_level();
        pos_guards();
        clear_coll_rooms();
        clear_saved_ctrl();
        *state.drawn_room() = 0;
        *state.mobs_count() = 0;
        *state.trobs_count() = 0;
        *state.next_sound() = -1;
        *state.holding_sword() = 0;
        *state.grab_timer() = 0;
        *state.can_guard_see_kid() = 0;
        *state.united_with_shadow() = 0;
        *state.flash_time() = 0;
        *state.leveldoor_open() = 0;
        *state.demo_index() = 0;
        *state.demo_time() = 0;
        *state.guardhp_curr() = 0;
        *state.hitp_delta() = 0;
        state.Guard().charid = charids_charid_2_guard as u8;
        state.Guard().direction = directions_dir_56_none as i8;
        do_startpos_impl(state);
        *state.have_sword() = (level_number == 0
            || level_number as u16 >= (*custom).have_sword_from_level) as u16;
        find_start_level_door();
        while check_sound_playing() != 0 && do_paused() == 0 {
            idle();
        }
        stop_sounds();
        if replaying != 0 {
            replay_restore_level();
        }
        if skipping_replay != 0
            && (replay_seek_target
                == replay_seek_targets_replay_seek_0_next_room as u8
                || replay_seek_target
                    == replay_seek_targets_replay_seek_1_next_level as u8)
        {
            skipping_replay = 0;
        }
        draw_level_first_impl(state);
        show_copyprot(0);
        level_number = play_level_2_impl(state);
        if enable_copyprot != 0
            && level_number as u16 == (*custom).copyprot_level
            && *state.demo_mode() == 0
        {
            level_number = 15;
        } else if level_number == 16 {
            level_number = (*custom).copyprot_level as c_int;
            (*custom).copyprot_level = u16::MAX;
        }
        free_peels();
    }
}

/// Plays `level_number`, then whatever level it leads to, forever.
///
/// Each iteration shows the level's cutscene (skipped while recording or
/// replaying), loads level data and sprites, wipes every piece of per-level
/// state, positions the Kid and the guards, draws the first frame, and runs
/// [`play_level_2`] until the level ends. Level 15 is the copy-protection
/// level, which is substituted for the configured `copyprot_level` on the way
/// in and swapped back on the way out.
///
/// Never returns — leaving a game happens via `longjmp` in `start_game`.
#[no_mangle]
pub unsafe extern "C" fn play_level(level_number: c_int) {
    play_level_impl(&mut State, level_number);
}

/// Places the Kid at the level's start (or checkpoint) position and picks his
/// entry animation.
// seg003:01A3
unsafe fn do_startpos_impl(state: &mut State) {
    if *state.current_level() == (*custom).checkpoint_level && *state.checkpoint() != 0 {
        state.level().start_dir = (*custom).checkpoint_respawn_dir;
        state.level().start_room = (*custom).checkpoint_respawn_room;
        state.level().start_pos = (*custom).checkpoint_respawn_tilepos;
        get_tile(
            (*custom).checkpoint_clear_tile_room as c_int,
            (*custom).checkpoint_clear_tile_col as c_int,
            (*custom).checkpoint_clear_tile_row as c_int,
        );
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_0_empty as u8;
    }
    *state.next_room() = state.level().start_room as u16;
    state.Char().room = state.level().start_room;
    let start_pos = state.level().start_pos as u16;
    state.Char().curr_col = (start_pos % SCREEN_TILECOUNTX as u16) as i8;
    state.Char().curr_row = (start_pos / SCREEN_TILECOUNTX as u16) as i8;
    let curr_col = state.Char().curr_col;
    state.Char().x = x_bump_at((curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize)
        .wrapping_add(TILE_SIZEX as u8);
    state.Char().direction = !state.level().start_dir;
    // A seamless transition keeps the HP the Kid arrived with.
    if *state.seamless() == 0 {
        let hitp = if *state.current_level() != 0 {
            *state.hitp_beg_lev()
        } else {
            (*custom).demo_hitp
        };
        *state.hitp_max() = hitp;
        *state.hitp_curr() = hitp;
    }
    let cl = *state.current_level() as usize;
    match (*custom).tbl_entry_pose[cl] {
        // Falls in through the ceiling, and triggers the tile he lands on.
        1 => {
            get_tile(5, 2, 0);
            trigger_button(0, 0, -1);
            seqtbl_offset_char(seqids_seq_7_fall as c_short);
        }
        // Runs in from the level door.
        2 => seqtbl_offset_char(seqids_seq_84_run as c_short),
        // Default: stands still and turns to face into the room.
        _ => seqtbl_offset_char(seqids_seq_5_turn as c_short),
    }
    set_start_pos_impl(state);
}

/// Puts the Kid at the level's start position with the matching entry pose.
///
/// Honours the checkpoint if one is set for this level (level 7's crushed-by-
/// its-own-loose-floor checkpoint), in which case the respawn room, tile and
/// direction come from the custom options and the offending tile is cleared.
#[no_mangle]
pub unsafe extern "C" fn do_startpos() {
    do_startpos_impl(&mut State);
}

/// Finishes character setup once the position and entry sequence are chosen:
/// clears status effects, plays the first frame, saves the Kid's state.
// seg003:028A
unsafe fn set_start_pos_impl(state: &mut State) {
    let curr_row = state.Char().curr_row;
    state.Char().y = y_land_at(curr_row as usize + 1) as u8;
    state.Char().alive = -1;
    state.Char().charid = charids_charid_0_kid as u8;
    *state.is_screaming() = 0;
    *state.knock() = 0;
    *state.upside_down() = (*custom).start_upside_down as u16;
    *state.is_feather_fall() = 0;
    state.Char().fall_y = 0;
    state.Char().fall_x = 0;
    *state.offguard() = 0;
    state.Char().sword = sword_status_sword_0_sheathed as u8;
    *state.droppedout() = 0;
    play_seq();
    if *state.current_level() == (*custom).falling_entry_level
        && state.Char().room == (*custom).falling_entry_room
    {
        goto_other_room(3);
    }
    savekid();
}

/// Resets the Kid to a clean, alive, unarmed state at his current tile and
/// starts his entry animation.
///
/// Also handles the one level where the Kid enters by falling out of the room
/// above (`falling_entry_level` / `falling_entry_room`), by immediately moving
/// him into the room below.
#[no_mangle]
pub unsafe extern "C" fn set_start_pos() {
    set_start_pos_impl(&mut State);
}

/// Opens the level door the Kid entered through, in whichever tile of his
/// starting room holds one.
///
/// The low five bits of a tile byte are the tile id; the upper bits are flags,
/// hence the `& 0x1F`. All 30 tiles of the room are scanned, so a room with two
/// level doors would open both.
// seg003:02E6
#[no_mangle]
pub unsafe extern "C" fn find_start_level_door() {
    get_room_address(Kid.room as c_int);
    for tilepos in 0i16..30 {
        if *curr_room_tiles.add(tilepos as usize) & 0x1F
            == tiles_tiles_16_level_door_left as u8
        {
            start_level_door(Kid.room as c_short, tilepos);
        }
    }
}

/// Paints the very first frame of a level and waits one tick before play starts.
// seg003:0326
unsafe fn draw_level_first_impl(state: &mut State) {
    *state.next_room() = state.Kid().room as u16;
    check_the_end();
    let cl = *state.current_level() as usize;
    if (*custom).tbl_level_type[cl] != 0 {
        gen_palace_wall_colors();
    }
    draw_rect(&screen_rect, colorids_color_0_black as c_int);
    show_level();
    redraw_screen(0);
    draw_kid_hp(*state.hitp_curr() as c_short, *state.hitp_max() as c_short);
    check_quick_op();
    auto_screenshot();
    start_timer(timerids_timer_1 as c_int, 5);
    do_simple_wait(1);
}

/// Draws the opening screen of a level: black background, the level's rooms,
/// the Kid's HP, and (on palace levels) freshly generated wall colours.
#[no_mangle]
pub unsafe extern "C" fn draw_level_first() {
    draw_level_first_impl(&mut State);
}

/// Repaints the current room from scratch into the offscreen buffer and blits it.
// seg003:037B
unsafe fn redraw_screen_impl(state: &mut State, drawing_different_room: c_int) {
    if drawing_different_room != 0 {
        draw_rect(&rect_top, colorids_color_0_black as c_int);
        update_screen();
        crate::platform::sdl::shared_renderer().delay(100);
    }
    *state.different_room() = 0;
    if *state.is_blind_mode() != 0 {
        draw_rect(&rect_top, colorids_color_0_black as c_int);
    } else {
        if curr_guard_color != 0 {
            set_chtab_palette(
                chtab_addrs[chtabs_id_chtab_5_guard as usize],
                guard_palettes.add(0x30 * curr_guard_color as usize - 0x30),
                0x10,
            );
        }
        *state.need_drects() = 0;
        redraw_room();
        redraw_lighting();
        if is_keyboard_mode != 0 {
            clear_kbd_buf();
        }
        *state.is_blind_mode() = 1;
        draw_tables();
        if is_keyboard_mode != 0 {
            clear_kbd_buf();
        }
        // Level 15 is the copy-protection level: each potion is labelled with a
        // letter naming the manual page the player has to look the symbol up on.
        if *state.current_level() == 15 {
            current_target_surface = offscreen_surface;
            for i in 0..14usize {
                if copyprot_room_at(i) == *state.drawn_room() {
                    let tilepos = copyprot_tile_at(i);
                    set_curr_pos(
                        ((tilepos % 10) << 5) as c_int + 24,
                        (tilepos / 10 * 63 + 38) as c_int,
                    );
                    let letter_idx = cplevel_entr[i] as usize;
                    let letter =
                        *core::ptr::addr_of!(copyprot_letter).cast::<u8>().add(letter_idx);
                    draw_text_character(letter);
                }
            }
            current_target_surface = onscreen_surface_;
        }
        *state.is_blind_mode() = 0;
        state.table_counts().fill(0);
        draw_moving();
        draw_tables();
        if is_keyboard_mode != 0 {
            clear_kbd_buf();
        }
        *state.need_drects() = 1;
        if *state.upside_down() != 0 {
            flip_screen(offscreen_surface);
        }
        copy_screen_rect(&rect_top);
        if *state.upside_down() != 0 {
            flip_screen(offscreen_surface);
        }
        if is_keyboard_mode != 0 {
            clear_kbd_buf();
        }
    }
    *state.exit_room_timer() = 2;
}

/// Repaints the whole visible room: tiles, lighting, then every moving object.
///
/// Pass a non-zero `drawing_different_room` when the room being shown has just
/// changed; that blanks the screen and pauses briefly first, so the switch does
/// not look like a glitch. Blind mode (a debug cheat) short-circuits to a black
/// screen. Leaves `exit_room_timer` at 2 so the room-switch logic knows a full
/// repaint just happened.
#[no_mangle]
pub unsafe extern "C" fn redraw_screen(drawing_different_room: c_int) {
    redraw_screen_impl(&mut State, drawing_different_room);
}

/// The per-frame loop of a single level; returns the level number to play next.
// seg003:04F8
unsafe fn play_level_2_impl(state: &mut State) -> c_int {
    reset_timer(timerids_timer_1 as c_int);
    loop {
        check_quick_op();
        if need_replay_cycle != 0 {
            replay_cycle();
        }
        if state.Kid().sword == sword_status_sword_2_drawn as u8 {
            set_timer_length(timerids_timer_1 as c_int, (*custom).fight_speed as c_int);
        } else {
            set_timer_length(timerids_timer_1 as c_int, (*custom).base_speed as c_int);
        }
        *state.guardhp_delta() = 0;
        *state.hitp_delta() = 0;
        timers_impl(state);
        play_frame();
        dump_frame_state();
        if *state.keep_last_seed() == 1 {
            *state.preserved_seed() = *state.random_seed();
            *state.keep_last_seed() = -1;
        }
        if *state.is_restart_level() != 0 {
            *state.is_restart_level() = 0;
            return *state.current_level() as c_int;
        } else if *state.next_level() == *state.current_level() || check_sound_playing() != 0 {
            draw_game_frame();
            dump_frame_pixels();
            dump_frame_raw();
            flash_if_hurt_impl(state);
            remove_flash_if_hurt_impl(state);
            do_simple_wait(timerids_timer_1 as c_int);
        } else {
            stop_sounds();
            *state.hitp_beg_lev() = *state.hitp_max();
            *state.checkpoint() = 0;
            if *state.keep_last_seed() == -1 {
                *state.random_seed() = *state.preserved_seed();
                *state.keep_last_seed() = 0;
            }
            return *state.next_level() as c_int;
        }
    }
}

/// Runs one level to completion and returns the number of the next level.
///
/// Each iteration sets the frame length (fights run at `fight_speed`, everything
/// else at `base_speed`), ticks the timers, advances every character by one
/// animation frame, then either draws that frame and waits out the tick, or —
/// if the level number changed and no sound is still playing — tears the level
/// down and returns. A pending restart returns the current level instead.
#[no_mangle]
pub unsafe extern "C" fn play_level_2() -> c_int {
    play_level_2_impl(&mut State)
}

/// Marks the foreground of every tile the current character overlaps as dirty.
// seg003:0576
unsafe fn redraw_at_char_impl(state: &mut State) {
    if state.Char().sword >= sword_status_sword_2_drawn as u8 {
        if state.Char().direction >= directions_dir_0_right as i8 {
            *state.char_col_right() += 1;
            if *state.char_col_right() > 9 {
                *state.char_col_right() = 9;
            }
        } else {
            *state.char_col_left() -= 1;
            if *state.char_col_left() < 0 {
                *state.char_col_left() = 0;
            }
        }
    }
    // For the Kid, widen the dirty rectangle to also cover where he was last
    // frame, so his old sprite gets painted over.
    let (x_top_row, x_col_right, x_col_left) = if state.Char().charid == charids_charid_0_kid as u8
    {
        (
            (*state.char_top_row()).min(*state.prev_char_top_row()),
            (*state.char_col_right()).max(*state.prev_char_col_right()),
            (*state.char_col_left()).min(*state.prev_char_col_left()),
        )
    } else {
        (
            *state.char_top_row(),
            *state.char_col_right(),
            *state.char_col_left(),
        )
    };
    let bottom_row = *state.char_bottom_row();
    for trow in x_top_row..=bottom_row {
        for tcol in x_col_left..=x_col_right {
            set_redraw_fore(get_tilepos(tcol as c_int, trow as c_int) as c_short, 1);
        }
    }
    if state.Char().charid == charids_charid_0_kid as u8 {
        *state.prev_char_top_row() = *state.char_top_row();
        *state.prev_char_col_right() = *state.char_col_right();
        *state.prev_char_col_left() = *state.char_col_left();
    }
}

/// Marks the tiles the current character overlaps as dirty in the foreground
/// layer, including the tiles he occupied on the previous frame if he is the Kid.
///
/// Called once per character per frame. Also nudges the dirty rectangle one
/// column outwards in the direction a drawn sword points, since the blade
/// sticks out past the character's own tile.
#[no_mangle]
pub unsafe extern "C" fn redraw_at_char() {
    redraw_at_char_impl(&mut State);
}

/// Marks the tiles a character overlaps as dirty in the *overlay* layer — the
/// pass that draws things in front of the character.
// seg003:0645
unsafe fn redraw_at_char2_impl(state: &mut State) {
    let char_action = state.Char().action;
    let char_frame = state.Char().frame;
    let mut redraw_func: unsafe extern "C" fn(c_short, byte) = set_redraw2;
    // Frames 78..80 are the grab frames, which always use the default.
    if char_frame < frameids_frame_78_jumphang as u8
        || char_frame >= frameids_frame_80_jumphang as u8
    {
        // Frames 137..145: climbing up, which redraws the floor above him.
        if (frameids_frame_137_climbing_3 as u8..frameids_frame_145_climbing_11 as u8)
            .contains(&char_frame)
        {
            redraw_func = set_redraw_floor_overlay;
        // Otherwise only airborne and hanging characters need an overlay pass;
        // frames 102..106 are the start of a fall after being bumped.
        } else if char_action != actions_actions_2_hang_climb as u8
            && char_action != actions_actions_3_in_midair as u8
            && char_action != actions_actions_4_in_freefall as u8
            && char_action != actions_actions_6_hang_straight as u8
            && (char_action != actions_actions_5_bumped as u8
                || char_frame < frameids_frame_102_start_fall_1 as u8
                || char_frame > frameids_frame_106_fall as u8)
        {
            return;
        }
    }
    let cbr = *state.char_bottom_row();
    let ctr = *state.char_top_row();
    let ccl = *state.char_col_left();
    // `tile_col` is a shared global that other code reads after this function
    // returns, and the C original leaves it at one past the end of the sweep
    // (or untouched at char_col_right if the sweep never runs). Kept as an
    // explicit walk over the global rather than a `for` over a range so that
    // trailing value stays exactly what it was in C.
    *state.tile_col() = *state.char_col_right();
    while *state.tile_col() >= ccl {
        let tc = *state.tile_col();
        if char_action != 2 {
            redraw_func(
                get_tilepos(tc as c_int, cbr as c_int) as c_short,
                1,
            );
        }
        if ctr != cbr {
            redraw_func(
                get_tilepos(tc as c_int, ctr as c_int) as c_short,
                1,
            );
        }
        *state.tile_col() -= 1;
    }
}

/// Marks the tiles the current character overlaps as needing an overlay redraw
/// — the layer drawn in front of characters.
///
/// Which redraw routine is used depends on what the character is doing:
/// climbing up marks the floor overlay above him, while hanging, falling and
/// being bumped mark the general overlay. Characters doing anything else are
/// fully behind the scenery and need no overlay pass, so the call is a no-op.
#[no_mangle]
pub unsafe extern "C" fn redraw_at_char2() {
    redraw_at_char2_impl(&mut State);
}

/// Shakes loose floors in the row above (or at) the character, if something
/// knocked this frame.
// seg003:0706
unsafe fn check_knock_impl(state: &mut State) {
    if *state.knock() != 0 {
        let knock_v = *state.knock();
        do_knock(state.Char().room as c_int, state.Char().curr_row as c_int - (knock_v > 0) as c_int);
        *state.knock() = 0;
    }
}

/// Applies a pending knock: makes the loose floors in the affected room row
/// wobble and consumes the `knock` flag.
///
/// A positive `knock` (a landing) shakes the row above the character; a
/// negative one (a hit from below) shakes his own row.
#[no_mangle]
pub unsafe extern "C" fn check_knock() {
    check_knock_impl(&mut State);
}

/// Advances every per-frame countdown in the game by one tick.
// seg003:0735
unsafe fn timers_impl(state: &mut State) {
    // Counts down to zero, then skips straight past it to -1: zero is the
    // "not united" value, so it must never be observed on the way down.
    if *state.united_with_shadow() > 0 {
        *state.united_with_shadow() -= 1;
        if *state.united_with_shadow() == 0 {
            *state.united_with_shadow() -= 1;
        }
    }
    if *state.guard_notice_timer() > 0 {
        *state.guard_notice_timer() -= 1;
    }
    if *state.resurrect_time() > 0 {
        *state.resurrect_time() -= 1;
    }
    if (*fixes).fix_quicksave_during_feather != 0 {
        if *state.is_feather_fall() > 0 {
            *state.is_feather_fall() -= 1;
            if *state.is_feather_fall() == 0 {
                if check_sound_playing() != 0 {
                    stop_sounds();
                }
                if recording != 0 {
                    special_move = replay_special_moves_MOVE_EFFECT_END as u8;
                }
            }
        }
    } else {
        if *state.is_feather_fall() != 0 {
            *state.is_feather_fall() += 1;
        }
        if *state.is_feather_fall() != 0
            && (check_sound_playing() == 0 || *state.is_feather_fall() > 225)
        {
            if recording != 0 {
                special_move = replay_special_moves_MOVE_EFFECT_END as u8;
            }
            if replaying == 0 {
                *state.is_feather_fall() = 0;
            }
        }
    }
    if *state.current_level() == (*custom).mouse_level
        && state.Char().room == (*custom).mouse_room
        && *state.leveldoor_open() != 0
    {
        *state.leveldoor_open() += 1;
        if *state.leveldoor_open() == (*custom).mouse_delay {
            do_mouse_impl(state);
        }
    }
    if (*fixes).enable_super_high_jump != 0 && *state.super_jump_timer() > 0 {
        *state.super_jump_timer() -= 1;
        if *state.super_jump_timer() == 0 && state.Kid().frame == frameids_frame_79_jumphang as u8 {
            let sj_room = *state.super_jump_room();
            let sj_col = *state.super_jump_col();
            let sj_row = *state.super_jump_row();
            if get_tile(sj_room as c_int, sj_col as c_int, sj_row as c_int) == tiles_tiles_11_loose as c_int
                && *curr_room_tiles.add(curr_tilepos as usize) & 0x20 == 0
            {
                make_loose_fall(1);
                do_knock(sj_room as c_int, sj_row as c_int);
            } else if curr_tile2 == tiles_tiles_20_wall as u8
                || tile_is_floor(curr_tile2 as c_int) != 0
            {
                if sj_row < 2 {
                    state.Kid().curr_row = sj_row + 1;
                    state.Kid().y = y_land_at(sj_row as usize + 2).wrapping_add(10) as u8;
                }
                do_knock(sj_room as c_int, sj_row as c_int);
            } else if tile_is_floor(curr_tile2 as c_int) == 0 {
                if sj_row == 2 {
                    let kid_room = state.Kid().room;
                    state.Kid().room = state.level().roomlinks[kid_room as usize - 1].up;
                }
                if state.Kid().room != 0 {
                    state.Kid().curr_row = sj_row + 1;
                    state.Kid().y = y_land_at(sj_row as usize + 2).wrapping_sub(10) as u8;
                    state.Kid().fall_x = 0;
                    state.Kid().fall_y = 0;
                    *state.super_jump_fall() = 1;
                    seqtbl_offset_kid_char(seqids_seq_19_fall as c_int);
                    play_seq();
                }
            }
        }
    }
}

/// Ticks all the per-frame countdowns: the shadow union, the guard's alertness
/// delay, the resurrection timer, feather fall, the mouse's entrance on the
/// mouse level, and the super-jump landing check.
///
/// The feather-fall branch has two implementations — the buggy original, which
/// counts *up* and ends the effect when the sound stops, and the fixed one
/// behind `fix_quicksave_during_feather`, which counts down so a quicksave
/// cannot leave the Kid floating forever.
#[no_mangle]
pub unsafe extern "C" fn timers() {
    timers_impl(&mut State);
}

/// Handles the mirror on the mirror level: reflection while running past it,
/// shadow creation on jumping through it.
// seg003:0798
unsafe fn check_mirror_impl(state: &mut State) {
    if *state.jumped_through_mirror() == -1 {
        jump_through_mirror_impl(state);
    } else if get_tile_at_char() == tiles_tiles_13_mirror as c_int {
        loadkid();
        load_frame();
        check_mirror_image_impl(state);
        if distance_mirror >= 0
            && (*custom).show_mirror_image != 0
            && state.Char().room == *state.drawn_room() as u8
        {
            load_frame_to_obj();
            reset_obj_clip();
            let curr_row = state.Char().curr_row;
            let clip_top = y_clip_at(curr_row as usize + 1) as u16;
            if clip_top < *state.obj_y() as u16 {
                *state.obj_clip_top() = clip_top as c_short;
                *state.obj_clip_left() = ((state.Char().curr_col as i32) << 5) as c_short + 9;
                add_objtable(4);
            }
        }
    }
}

/// Runs the mirror logic for the current frame.
///
/// If the Kid has just jumped through the mirror, spawns the shadow. Otherwise,
/// while he stands on the mirror tile, computes his mirrored pose and adds it to
/// the object table so the reflection is drawn — clipped to the part of him that
/// is actually level with the glass.
#[no_mangle]
pub unsafe extern "C" fn check_mirror() {
    check_mirror_impl(&mut State);
}

/// Turns the Kid's mirror image into the shadow character and splits his HP
/// bar in two.
// seg003:080A
unsafe fn jump_through_mirror_impl(state: &mut State) {
    loadkid();
    load_frame();
    check_mirror_image_impl(state);
    *state.jumped_through_mirror() = 0;
    state.Char().charid = charids_charid_1_shadow as u8;
    play_sound(soundids_sound_45_jump_through_mirror as c_int);
    saveshad();
    *state.guardhp_max() = *state.hitp_max();
    *state.guardhp_curr() = *state.hitp_max();
    *state.hitp_curr() = 1;
    draw_kid_hp(1, *state.hitp_max() as c_short);
    draw_guard_hp(*state.guardhp_curr() as c_short, *state.guardhp_max() as c_short);
}

/// Creates the shadow at the moment the Kid jumps through the mirror.
///
/// The reflection stops being a drawing and becomes a real character: it is
/// saved into the shadow's slot, given all of the Kid's HP as guard HP, and the
/// Kid himself is left on one unit — which is why the shadow chase on that level
/// starts with the player nearly dead.
#[no_mangle]
pub unsafe extern "C" fn jump_through_mirror() {
    jump_through_mirror_impl(&mut State);
}

/// Mirrors the current character's x-position and facing about the mirror line.
// seg003:085B
unsafe fn check_mirror_image_impl(state: &mut State) {
    let curr_col = state.Char().curr_col;
    let xpos: i16 =
        x_bump_at((curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i16 + 10;
    let mut dist = distance_to_edge_weight();
    if state.Char().direction >= directions_dir_0_right as i8 {
        dist = (!dist) + TILE_SIZEX as c_int;
    }
    distance_mirror = (dist - 2) as i8;
    state.Char().x = ((xpos << 1) - state.Char().x as i16) as u8;
    state.Char().direction = !state.Char().direction;
}

/// Reflects the current character about the mirror: flips his facing and
/// mirrors his x-position around the glass, which sits 10 pixels into the tile.
///
/// Also records how far past the glass he is in [`distance_mirror`], which the
/// caller uses to decide whether a reflection should be drawn at all.
#[no_mangle]
pub unsafe extern "C" fn check_mirror_image() {
    check_mirror_image_impl(&mut State);
}

/// Bounces the Kid off a guard he ran into instead of letting him walk through.
// seg003:08AA
unsafe fn bump_into_opponent_impl(state: &mut State) {
    if *state.can_guard_see_kid() >= 2
        && state.Char().sword == sword_status_sword_0_sheathed as u8
        && state.Opp().sword != sword_status_sword_0_sheathed as u8
        && state.Opp().action < 2
        && state.Char().direction != state.Opp().direction
    {
        let distance = char_opp_dist();
        if distance.abs() <= 15 {
            if (*fixes).fix_painless_fall_on_guard != 0 {
                if state.Char().fall_y >= 33 {
                    return;
                } else if state.Char().fall_y >= 22 {
                    take_hp(1);
                    play_sound(soundids_sound_16_medium_land as c_int);
                }
            }
            if (*fixes).fix_jumping_over_guard != 0 {
                if (state.Char().direction == directions_dir_0_right as i8 && state.Char().x > state.Opp().x)
                    || (state.Char().direction == directions_dir_FF_left as i8 && state.Char().x < state.Opp().x)
                {
                    let opp_x = state.Opp().x;
                    state.Char().x = opp_x;
                }
            }
            let curr_row = state.Char().curr_row;
            state.Char().y = y_land_at(curr_row as usize + 1) as u8;
            state.Char().fall_y = 0;
            seqtbl_offset_char(seqids_seq_47_bump as c_short);
            play_seq();
        }
    }
}

/// Makes an unarmed Kid bump off a guard standing within 15 pixels, rather than
/// walking through him.
///
/// Called from the Kid's frame handler, so "opponent" is always the guard. Two
/// optional fixes ride along: `fix_painless_fall_on_guard` charges the Kid for
/// landing on a guard from a height instead of cancelling the fall damage, and
/// `fix_jumping_over_guard` snaps him back to the guard's x-position so he
/// cannot end up on the far side.
#[no_mangle]
pub unsafe extern "C" fn bump_into_opponent() {
    bump_into_opponent_impl(&mut State);
}

/// Converts each room's stored guard tile into a pixel x-position.
// seg003:0913
unsafe fn pos_guards_impl(state: &mut State) {
    for room1 in 0..ROOMCOUNT as usize {
        let guard_tile = state.level().guards_tile[room1] as i16;
        if guard_tile < 30 {
            state.level().guards_x[room1] = x_bump_at(
                (guard_tile % 10) as usize + FIRST_ONSCREEN_COLUMN as usize,
            )
            .wrapping_add(TILE_SIZEX as u8);
            state.level().guards_seq_hi[room1] = 0;
        }
    }
}

/// Prepares every room's guard for play: turns the tile the guard was authored
/// on into a pixel x-position and clears the high byte of his animation
/// sequence pointer. Slots with a tile of 30 or more hold no guard and are left
/// untouched.
#[no_mangle]
pub unsafe extern "C" fn pos_guards() {
    pos_guards_impl(&mut State);
}

/// Decides whether the guard can see the Kid, and whether he will act on it.
///
/// Result in `can_guard_see_kid`: 0 = cannot see him, 1 = can see him but will
/// not come, 2 = can see him and will come.
// seg003:0959
unsafe fn check_can_guard_see_kid_impl(state: &mut State) {
    let kid_frame = state.Kid().frame;
    if state.Guard().charid == charids_charid_24_mouse as u8 {
        *state.can_guard_see_kid() = 0;
        return;
    }
    if (state.Guard().charid != charids_charid_1_shadow as u8 || *state.current_level() == 12)
        && kid_frame != 0
        && (kid_frame < frameids_frame_219_exit_stairs_3 as u8 || kid_frame >= 229)
        && state.Guard().direction != directions_dir_56_none as i8
        && state.Kid().alive < 0
        && state.Guard().alive < 0
        && state.Kid().room == state.Guard().room
        && state.Kid().curr_row == state.Guard().curr_row
    {
        *state.can_guard_see_kid() = 2;
        let kid_curr_col = state.Kid().curr_col;
        let mut left_pos: i16 =
            x_bump_at((kid_curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i16
                + TILE_MIDX as i16;
        if (*fixes).fix_doortop_disabling_guard != 0
            && (state.Kid().action == actions_actions_2_hang_climb as u8
                || state.Kid().action == actions_actions_6_hang_straight as u8)
        {
            left_pos += TILE_SIZEX as i16;
        }
        let guard_curr_col = state.Guard().curr_col;
        let mut right_pos: i16 =
            x_bump_at((guard_curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize)
                as i16
                + TILE_MIDX as i16;
        if left_pos > right_pos {
            core::mem::swap(&mut left_pos, &mut right_pos);
        }
        if get_tile_at_kid_impl(state, left_pos as c_int) == tiles_tiles_18_chomper as u8 {
            left_pos += TILE_SIZEX as i16;
        }
        let right_tile = get_tile_at_kid_impl(state, right_pos as c_int);
        if right_tile == tiles_tiles_4_gate as u8
            || ((*fixes).fix_doortop_disabling_guard != 0
                && (right_tile == tiles_tiles_7_doortop_with_floor as u8
                    || right_tile == tiles_tiles_12_doortop as u8))
        {
            right_pos -= TILE_SIZEX as i16;
        }
        // Walk the line of sight one tile at a time, from whichever of the two
        // is further left. An opaque tile ends the search outright; a tile that
        // can be seen through but not walked through downgrades to "won't come".
        for xpos in (left_pos..=right_pos).step_by(TILE_SIZEX as usize) {
            let tile = get_tile_at_kid_impl(state, xpos as c_int);
            if tile == tiles_tiles_20_wall as u8
                || curr_tile2 == tiles_tiles_7_doortop_with_floor as u8
                || curr_tile2 == tiles_tiles_12_doortop as u8
            {
                *state.can_guard_see_kid() = 0;
                return;
            }
            if curr_tile2 == tiles_tiles_11_loose as u8
                || curr_tile2 == tiles_tiles_18_chomper as u8
                || (curr_tile2 == tiles_tiles_4_gate as u8
                    && *curr_room_modif.add(curr_tilepos as usize) < 112)
                || tile_is_floor(curr_tile2 as c_int) == 0
            {
                *state.can_guard_see_kid() = 1;
            }
        }
    } else {
        *state.can_guard_see_kid() = 0;
    }
}

/// Works out whether the guard can see the Kid, storing the answer in
/// `can_guard_see_kid`: 0 = no, 1 = yes but he will stay put, 2 = yes and he
/// will come over.
///
/// The two must be alive, in the same room, on the same row, and the guard must
/// have a real facing. If so, the tiles between them are scanned: walls and
/// doortops block sight entirely, while loose floors, chompers, closed gates and
/// gaps in the floor are see-through but impassable, so the guard notices the
/// Kid without approaching. The mouse never "sees" anything — it has its own
/// behaviour.
#[no_mangle]
pub unsafe extern "C" fn check_can_guard_see_kid() {
    check_can_guard_see_kid_impl(&mut State);
}

/// Looks up the tile at pixel column `xpos` on the Kid's own row and room.
// seg003:0A99
unsafe fn get_tile_at_kid_impl(state: &mut State, xpos: c_int) -> byte {
    get_tile(state.Kid().room as c_int, get_tile_div_mod_m7(xpos), state.Kid().curr_row as c_int) as byte
}

/// Returns the tile id at pixel x-coordinate `xpos`, on the Kid's current row
/// and in the Kid's current room.
///
/// Like every `get_tile` call this also leaves `curr_tile2`, `curr_tilepos` and
/// the current room pointers set for the tile it found, which callers rely on.
#[no_mangle]
pub unsafe extern "C" fn get_tile_at_kid(xpos: c_int) -> byte {
    get_tile_at_kid_impl(&mut State, xpos)
}

/// Spawns the mouse that opens the level door on the mouse level.
// seg003:0ABA
unsafe fn do_mouse_impl(state: &mut State) {
    loadkid();
    state.Char().charid = (*custom).mouse_object;
    state.Char().x = (*custom).mouse_start_x;
    state.Char().curr_row = 0;
    let curr_row = state.Char().curr_row;
    state.Char().y = y_land_at(curr_row as usize + 1) as u8;
    state.Char().alive = -1;
    state.Char().direction = directions_dir_FF_left as i8;
    *state.guardhp_curr() = 1;
    seqtbl_offset_char(seqids_seq_105_mouse_forward as c_short);
    play_seq();
    saveshad();
}

/// Sends the mouse running in from the right of the room.
///
/// The mouse reuses the guard character slot: it is given the mouse's sprite id
/// and running sequence, one point of "guard" HP, and is saved into the shadow
/// slot like any other opponent. Triggered by [`timers`] once the level door has
/// been open for `mouse_delay` frames.
#[no_mangle]
pub unsafe extern "C" fn do_mouse() {
    do_mouse_impl(&mut State);
}

/// Flashes the screen if there is a pending flash or the Kid just lost HP.
// seg003:0AFC
unsafe fn flash_if_hurt_impl(state: &mut State) -> c_int {
    if *state.flash_time() != 0 {
        do_flash(*state.flash_color() as c_short);
        return 1;
    } else if *state.hitp_delta() < 0 {
        if is_joyst_mode != 0 && enable_controller_rumble != 0 {
            crate::platform::sdl::shared_input().rumble(1.0, 100);
        }
        do_flash(colorids_color_12_brightred as c_short);
        return 1;
    }
    0
}

/// Tints the whole screen for one frame when something dramatic happens, and
/// returns 1 if it did.
///
/// An explicitly requested flash (`flash_time`) uses `flash_color`; otherwise a
/// drop in the Kid's HP this frame flashes bright red and, on a controller,
/// fires a short rumble.
#[no_mangle]
pub unsafe extern "C" fn flash_if_hurt() -> c_int {
    flash_if_hurt_impl(&mut State)
}

/// Undoes the tint applied by [`flash_if_hurt`].
// seg003:0B1A
unsafe fn remove_flash_if_hurt_impl(state: &mut State) {
    if *state.flash_time() != 0 {
        *state.flash_time() -= 1;
    } else if *state.hitp_delta() >= 0 {
        return;
    }
    remove_flash();
}

/// Restores the normal palette after a flash, counting down `flash_time` so a
/// multi-frame flash keeps its colour until it expires.
#[no_mangle]
pub unsafe extern "C" fn remove_flash_if_hurt() {
    remove_flash_if_hurt_impl(&mut State);
}

#[cfg(test)]
#[allow(static_mut_refs)]
mod tests {
    use super::*;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // united_with_shadow skips 0 on its way down: when the decrement would land on 0
    // the code decrements once more to -1. This prevents 0 from lingering as a
    // "shadow united" state for an extra frame.
    #[test]
    fn timers_united_with_shadow_skips_zero() {
        setup();
        unsafe {
            is_feather_fall = 0;
            super_jump_timer = 0;
            leveldoor_open = 0;

            united_with_shadow = 1;
            timers();
            assert_eq!(united_with_shadow, -1, "1 -> 0 -> -1 (zero skipped)");

            united_with_shadow = 2;
            timers();
            assert_eq!(united_with_shadow, 1, "2 -> 1 (no skip when result != 0)");

            united_with_shadow = -3;
            timers();
            assert_eq!(united_with_shadow, -3, "negative: unchanged");

            united_with_shadow = 0;
            timers();
            assert_eq!(united_with_shadow, 0, "zero: unchanged (not touched)");
        }
    }

    // guard_notice_timer and resurrect_time each decrement by one per frame when
    // positive, and stop at zero (they are not skipped like united_with_shadow).
    #[test]
    fn timers_countdown_timers_decrement_and_stop_at_zero() {
        setup();
        unsafe {
            is_feather_fall = 0;
            super_jump_timer = 0;
            leveldoor_open = 0;
            united_with_shadow = 0;

            guard_notice_timer = 3;
            resurrect_time = 5;
            timers();
            assert_eq!(guard_notice_timer, 2);
            assert_eq!(resurrect_time, 4);

            guard_notice_timer = 1;
            resurrect_time = 1;
            timers();
            assert_eq!(guard_notice_timer, 0, "stops at 0");
            assert_eq!(resurrect_time, 0, "stops at 0");

            // Verify a second call at 0 leaves them at 0.
            timers();
            assert_eq!(guard_notice_timer, 0, "stays at 0");
            assert_eq!(resurrect_time, 0, "stays at 0");
        }
    }

    // pos_guards sets guards_x to x_bump[(tile_col + FIRST_ONSCREEN_COLUMN)] + TILE_SIZEX
    // and resets guards_seq_hi to 0 for any guard slot whose tile < 30.
    #[test]
    fn pos_guards_initializes_active_guard_slots() {
        setup();
        unsafe {
            level.guards_tile[0] = 7;   // tile_col = 7 % 10 = 7
            level.guards_seq_hi[0] = 0xFF;

            pos_guards();

            assert_eq!(level.guards_seq_hi[0], 0, "seq_hi cleared for active slot");
            let expected_x = x_bump_at(7 + FIRST_ONSCREEN_COLUMN as usize)
                .wrapping_add(TILE_SIZEX as u8);
            assert_eq!(
                level.guards_x[0], expected_x,
                "guards_x = x_bump[tile_col + FIRST_ONSCREEN_COLUMN] + TILE_SIZEX"
            );
        }
    }

    // Guard slots with tile >= 30 have no guard; pos_guards must leave them alone.
    #[test]
    fn pos_guards_skips_inactive_guard_slots() {
        setup();
        unsafe {
            level.guards_tile[2] = 30;
            level.guards_x[2] = 0xAB;
            level.guards_seq_hi[2] = 0xCD;

            pos_guards();

            assert_eq!(level.guards_x[2], 0xAB, "guards_x unchanged for inactive slot");
            assert_eq!(level.guards_seq_hi[2], 0xCD, "seq_hi unchanged for inactive slot");
        }
    }

    // When Guard is the mouse character, check_can_guard_see_kid returns immediately
    // with can_guard_see_kid = 0. The mouse handles visibility differently.
    #[test]
    fn check_can_guard_see_kid_mouse_guard_always_blind() {
        setup();
        unsafe {
            Guard.charid = charids_charid_24_mouse as u8;
            can_guard_see_kid = 2; // pre-load non-zero to confirm it gets cleared
            check_can_guard_see_kid();
            assert_eq!(can_guard_see_kid, 0);
        }
    }

    // A guard whose direction is dir_56_none is not placed in any room; the visibility
    // condition requires a real direction, so can_guard_see_kid must be 0.
    #[test]
    fn check_can_guard_see_kid_no_direction_means_blind() {
        setup();
        unsafe {
            Guard.charid = charids_charid_2_guard as u8;
            Guard.direction = directions_dir_56_none as i8;
            can_guard_see_kid = 2;
            check_can_guard_see_kid();
            assert_eq!(can_guard_see_kid, 0);
        }
    }
}
