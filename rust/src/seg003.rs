// Level loop, room redraw, initialization — ported from seg003.c.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;

extern "C" { fn dump_frame_state(); }

// File-local global (declared in seg003.c, not exported via data.c).
static mut distance_mirror: i8 = 0;

// SDL functions not captured by our bindings (which allowlist only src/).
extern "C" {
    fn SDL_Delay(ms: u32);
    fn SDL_HapticRumblePlay(haptic: *mut SDL_Haptic, strength: f32, length: u32) -> c_int;
    fn SDL_GameControllerRumble(
        gamecontroller: *mut SDL_GameController,
        low_frequency_rumble: u16,
        high_frequency_rumble: u16,
        duration_ms: u32,
    ) -> c_int;
    fn SDL_JoystickRumble(
        joystick: *mut SDL_Joystick,
        low_frequency_rumble: u16,
        high_frequency_rumble: u16,
        duration_ms: u32,
    ) -> c_int;
}

unsafe fn y_clip_at(idx: usize) -> i16 {
    *core::ptr::addr_of!(y_clip).cast::<i16>().add(idx)
}

unsafe fn copyprot_room_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_room).cast::<u16>().add(idx)
}

unsafe fn copyprot_tile_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_tile).cast::<u16>().add(idx)
}

// seg003:0000
#[no_mangle]
pub unsafe extern "C" fn init_game(lev: c_int) {
    if !offscreen_surface.is_null() {
        free_surface(offscreen_surface);
        offscreen_surface = core::ptr::null_mut();
    }
    offscreen_surface = make_offscreen_buffer(&rect_top);
    load_kid_sprite();
    text_time_remaining = 0;
    text_time_total = 0;
    is_show_time = 0;
    checkpoint = 0;
    upside_down = 0;
    resurrect_time = 0;
    if dont_reset_time == 0 {
        rem_min = (*custom).start_minutes_left as c_short;
        rem_tick = (*custom).start_ticks_left;
        hitp_beg_lev = (*custom).start_hitp;
    }
    need_level1_music = (lev as u16 == (*custom).intro_music_level) as u16;
    play_level(lev);
}

// seg003:005C
#[no_mangle]
pub unsafe extern "C" fn play_level(mut level_number: c_int) {
    if enable_copyprot != 0 && level_number as u16 == (*custom).copyprot_level {
        level_number = 15;
    }
    loop {
        if demo_mode != 0 && level_number > 2 {
            start_level = -1;
            need_quotes = 1;
            start_game();
        }
        if level_number != current_level as c_int {
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
        if level_number != current_level as c_int {
            load_lev_spr(level_number);
        }
        load_level();
        pos_guards();
        clear_coll_rooms();
        clear_saved_ctrl();
        drawn_room = 0;
        mobs_count = 0;
        trobs_count = 0;
        next_sound = -1;
        holding_sword = 0;
        grab_timer = 0;
        can_guard_see_kid = 0;
        united_with_shadow = 0;
        flash_time = 0;
        leveldoor_open = 0;
        demo_index = 0;
        demo_time = 0;
        guardhp_curr = 0;
        hitp_delta = 0;
        Guard.charid = charids_charid_2_guard as u8;
        Guard.direction = directions_dir_56_none as i8;
        do_startpos();
        have_sword = (level_number == 0
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
        draw_level_first();
        show_copyprot(0);
        level_number = play_level_2();
        if enable_copyprot != 0
            && level_number as u16 == (*custom).copyprot_level
            && demo_mode == 0
        {
            level_number = 15;
        } else if level_number == 16 {
            level_number = (*custom).copyprot_level as c_int;
            (*custom).copyprot_level = u16::MAX;
        }
        free_peels();
    }
}

// seg003:01A3
#[no_mangle]
pub unsafe extern "C" fn do_startpos() {
    let mut x: u16;
    if current_level == (*custom).checkpoint_level && checkpoint != 0 {
        level.start_dir = (*custom).checkpoint_respawn_dir;
        level.start_room = (*custom).checkpoint_respawn_room;
        level.start_pos = (*custom).checkpoint_respawn_tilepos;
        get_tile(
            (*custom).checkpoint_clear_tile_room as c_int,
            (*custom).checkpoint_clear_tile_col as c_int,
            (*custom).checkpoint_clear_tile_row as c_int,
        );
        *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_0_empty as u8;
    }
    next_room = level.start_room as u16;
    Char.room = level.start_room;
    x = level.start_pos as u16;
    Char.curr_col = (x % SCREEN_TILECOUNTX as u16) as i8;
    Char.curr_row = (x / SCREEN_TILECOUNTX as u16) as i8;
    Char.x = x_bump_at((Char.curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize)
        .wrapping_add(TILE_SIZEX as u8);
    Char.direction = !level.start_dir;
    if seamless == 0 {
        x = if current_level != 0 {
            hitp_beg_lev
        } else {
            (*custom).demo_hitp
        };
        hitp_max = x;
        hitp_curr = x;
    }
    if (*custom).tbl_entry_pose[current_level as usize] == 1 {
        get_tile(5, 2, 0);
        trigger_button(0, 0, -1);
        seqtbl_offset_char(seqids_seq_7_fall as c_short);
    } else if (*custom).tbl_entry_pose[current_level as usize] == 2 {
        seqtbl_offset_char(seqids_seq_84_run as c_short);
    } else {
        seqtbl_offset_char(seqids_seq_5_turn as c_short);
    }
    set_start_pos();
}

// seg003:028A
#[no_mangle]
pub unsafe extern "C" fn set_start_pos() {
    Char.y = y_land_at(Char.curr_row as usize + 1) as u8;
    Char.alive = -1;
    Char.charid = charids_charid_0_kid as u8;
    is_screaming = 0;
    knock = 0;
    upside_down = (*custom).start_upside_down as u16;
    is_feather_fall = 0;
    Char.fall_y = 0;
    Char.fall_x = 0;
    offguard = 0;
    Char.sword = sword_status_sword_0_sheathed as u8;
    droppedout = 0;
    play_seq();
    if current_level == (*custom).falling_entry_level
        && Char.room == (*custom).falling_entry_room
    {
        goto_other_room(3);
    }
    savekid();
}

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

// seg003:0326
#[no_mangle]
pub unsafe extern "C" fn draw_level_first() {
    next_room = Kid.room as u16;
    check_the_end();
    if (*custom).tbl_level_type[current_level as usize] != 0 {
        gen_palace_wall_colors();
    }
    draw_rect(&screen_rect, colorids_color_0_black as c_int);
    show_level();
    redraw_screen(0);
    draw_kid_hp(hitp_curr as c_short, hitp_max as c_short);
    check_quick_op();
    auto_screenshot();
    start_timer(timerids_timer_1 as c_int, 5);
    do_simple_wait(1);
}

// seg003:037B
#[no_mangle]
pub unsafe extern "C" fn redraw_screen(drawing_different_room: c_int) {
    if drawing_different_room != 0 {
        draw_rect(&rect_top, colorids_color_0_black as c_int);
        update_screen();
        SDL_Delay(100);
    }
    different_room = 0;
    if is_blind_mode != 0 {
        draw_rect(&rect_top, colorids_color_0_black as c_int);
    } else {
        if curr_guard_color != 0 {
            set_chtab_palette(
                chtab_addrs[chtabs_id_chtab_5_guard as usize],
                guard_palettes.add(0x30 * curr_guard_color as usize - 0x30),
                0x10,
            );
        }
        need_drects = 0;
        redraw_room();
        redraw_lighting();
        if is_keyboard_mode != 0 {
            clear_kbd_buf();
        }
        is_blind_mode = 1;
        draw_tables();
        if is_keyboard_mode != 0 {
            clear_kbd_buf();
        }
        if current_level == 15 {
            current_target_surface = offscreen_surface;
            for i in 0i16..14 {
                if copyprot_room_at(i as usize) == drawn_room {
                    let ct = copyprot_tile_at(i as usize);
                    set_curr_pos(
                        ((ct % 10) << 5) as c_int + 24,
                        (ct / 10 * 63 + 38) as c_int,
                    );
                    let letter_idx = cplevel_entr[i as usize] as usize;
                    let letter =
                        *core::ptr::addr_of!(copyprot_letter).cast::<u8>().add(letter_idx);
                    draw_text_character(letter);
                }
            }
            current_target_surface = onscreen_surface_;
        }
        is_blind_mode = 0;
        table_counts.fill(0);
        draw_moving();
        draw_tables();
        if is_keyboard_mode != 0 {
            clear_kbd_buf();
        }
        need_drects = 1;
        if upside_down != 0 {
            flip_screen(offscreen_surface);
        }
        copy_screen_rect(&rect_top);
        if upside_down != 0 {
            flip_screen(offscreen_surface);
        }
        if is_keyboard_mode != 0 {
            clear_kbd_buf();
        }
    }
    exit_room_timer = 2;
}

// seg003:04F8
#[no_mangle]
pub unsafe extern "C" fn play_level_2() -> c_int {
    reset_timer(timerids_timer_1 as c_int);
    loop {
        check_quick_op();
        if need_replay_cycle != 0 {
            replay_cycle();
        }
        if Kid.sword == sword_status_sword_2_drawn as u8 {
            set_timer_length(timerids_timer_1 as c_int, (*custom).fight_speed as c_int);
        } else {
            set_timer_length(timerids_timer_1 as c_int, (*custom).base_speed as c_int);
        }
        guardhp_delta = 0;
        hitp_delta = 0;
        timers();
        play_frame();
        dump_frame_state();
        if keep_last_seed == 1 {
            preserved_seed = random_seed;
            keep_last_seed = -1;
        }
        if is_restart_level != 0 {
            is_restart_level = 0;
            return current_level as c_int;
        } else if next_level == current_level || check_sound_playing() != 0 {
            draw_game_frame();
            flash_if_hurt();
            remove_flash_if_hurt();
            do_simple_wait(timerids_timer_1 as c_int);
        } else {
            stop_sounds();
            hitp_beg_lev = hitp_max;
            checkpoint = 0;
            if keep_last_seed == -1 {
                random_seed = preserved_seed;
                keep_last_seed = 0;
            }
            return next_level as c_int;
        }
    }
}

// seg003:0576
#[no_mangle]
pub unsafe extern "C" fn redraw_at_char() {
    let x_top_row: c_short;
    let x_col_left: c_short;
    let x_col_right: c_short;
    if Char.sword >= sword_status_sword_2_drawn as u8 {
        if Char.direction >= directions_dir_0_right as i8 {
            char_col_right += 1;
            if char_col_right > 9 {
                char_col_right = 9;
            }
        } else {
            char_col_left -= 1;
            if char_col_left < 0 {
                char_col_left = 0;
            }
        }
    }
    if Char.charid == charids_charid_0_kid as u8 {
        x_top_row = char_top_row.min(prev_char_top_row);
        x_col_right = char_col_right.max(prev_char_col_right);
        x_col_left = char_col_left.min(prev_char_col_left);
    } else {
        x_top_row = char_top_row;
        x_col_right = char_col_right;
        x_col_left = char_col_left;
    }
    for trow in x_top_row..=char_bottom_row {
        for tcol in x_col_left..=x_col_right {
            set_redraw_fore(get_tilepos(tcol as c_int, trow as c_int) as c_short, 1);
        }
    }
    if Char.charid == charids_charid_0_kid as u8 {
        prev_char_top_row = char_top_row;
        prev_char_col_right = char_col_right;
        prev_char_col_left = char_col_left;
    }
}

// seg003:0645
#[no_mangle]
pub unsafe extern "C" fn redraw_at_char2() {
    let char_action = Char.action;
    let char_frame = Char.frame;
    let redraw_func: unsafe extern "C" fn(c_short, byte);
    if char_frame < frameids_frame_78_jumphang as u8
        || char_frame >= frameids_frame_80_jumphang as u8
    {
        if char_frame >= frameids_frame_137_climbing_3 as u8
            && char_frame < frameids_frame_145_climbing_11 as u8
        {
            redraw_func = set_redraw_floor_overlay;
        } else if char_action != actions_actions_2_hang_climb as u8
            && char_action != actions_actions_3_in_midair as u8
            && char_action != actions_actions_4_in_freefall as u8
            && char_action != actions_actions_6_hang_straight as u8
            && (char_action != actions_actions_5_bumped as u8
                || char_frame < frameids_frame_102_start_fall_1 as u8
                || char_frame > frameids_frame_106_fall as u8)
        {
            return;
        } else {
            redraw_func = set_redraw2;
        }
    } else {
        redraw_func = set_redraw2;
    }
    tile_col = char_col_right;
    while tile_col >= char_col_left {
        if char_action != 2 {
            redraw_func(
                get_tilepos(tile_col as c_int, char_bottom_row as c_int) as c_short,
                1,
            );
        }
        if char_top_row != char_bottom_row {
            redraw_func(
                get_tilepos(tile_col as c_int, char_top_row as c_int) as c_short,
                1,
            );
        }
        tile_col -= 1;
    }
}

// seg003:0706
#[no_mangle]
pub unsafe extern "C" fn check_knock() {
    if knock != 0 {
        do_knock(Char.room as c_int, Char.curr_row as c_int - (knock > 0) as c_int);
        knock = 0;
    }
}

// seg003:0735
#[no_mangle]
pub unsafe extern "C" fn timers() {
    if united_with_shadow > 0 {
        united_with_shadow -= 1;
        if united_with_shadow == 0 {
            united_with_shadow -= 1;
        }
    }
    if guard_notice_timer > 0 {
        guard_notice_timer -= 1;
    }
    if resurrect_time > 0 {
        resurrect_time -= 1;
    }
    if (*fixes).fix_quicksave_during_feather != 0 {
        if is_feather_fall > 0 {
            is_feather_fall -= 1;
            if is_feather_fall == 0 {
                if check_sound_playing() != 0 {
                    stop_sounds();
                }
                if recording != 0 {
                    special_move = replay_special_moves_MOVE_EFFECT_END as u8;
                }
            }
        }
    } else {
        if is_feather_fall != 0 {
            is_feather_fall += 1;
        }
        if is_feather_fall != 0
            && (check_sound_playing() == 0 || is_feather_fall > 225)
        {
            if recording != 0 {
                special_move = replay_special_moves_MOVE_EFFECT_END as u8;
            }
            if replaying == 0 {
                is_feather_fall = 0;
            }
        }
    }
    if current_level == (*custom).mouse_level
        && Char.room == (*custom).mouse_room
        && leveldoor_open != 0
    {
        leveldoor_open += 1;
        if leveldoor_open == (*custom).mouse_delay {
            do_mouse();
        }
    }
    if (*fixes).enable_super_high_jump != 0 && super_jump_timer > 0 {
        super_jump_timer -= 1;
        if super_jump_timer == 0 && Kid.frame == frameids_frame_79_jumphang as u8 {
            if get_tile(
                super_jump_room as c_int,
                super_jump_col as c_int,
                super_jump_row as c_int,
            ) == tiles_tiles_11_loose as c_int
                && *curr_room_tiles.add(curr_tilepos as usize) & 0x20 == 0
            {
                make_loose_fall(1);
                do_knock(super_jump_room as c_int, super_jump_row as c_int);
            } else if curr_tile2 == tiles_tiles_20_wall as u8
                || tile_is_floor(curr_tile2 as c_int) != 0
            {
                if super_jump_row < 2 {
                    Kid.curr_row = super_jump_row + 1;
                    Kid.y = y_land_at(super_jump_row as usize + 2).wrapping_add(10) as u8;
                }
                do_knock(super_jump_room as c_int, super_jump_row as c_int);
            } else if tile_is_floor(curr_tile2 as c_int) == 0 {
                if super_jump_row == 2 {
                    Kid.room = level.roomlinks[Kid.room as usize - 1].up;
                }
                if Kid.room != 0 {
                    Kid.curr_row = super_jump_row + 1;
                    Kid.y = y_land_at(super_jump_row as usize + 2).wrapping_sub(10) as u8;
                    Kid.fall_x = 0;
                    Kid.fall_y = 0;
                    super_jump_fall = 1;
                    seqtbl_offset_kid_char(seqids_seq_19_fall as c_int);
                    play_seq();
                }
            }
        }
    }
}

// seg003:0798
#[no_mangle]
pub unsafe extern "C" fn check_mirror() {
    if jumped_through_mirror == -1 {
        jump_through_mirror();
    } else if get_tile_at_char() == tiles_tiles_13_mirror as c_int {
        loadkid();
        load_frame();
        check_mirror_image();
        if distance_mirror >= 0
            && (*custom).show_mirror_image != 0
            && Char.room == drawn_room as u8
        {
            load_frame_to_obj();
            reset_obj_clip();
            let clip_top = y_clip_at(Char.curr_row as usize + 1) as u16;
            if clip_top < obj_y as u16 {
                obj_clip_top = clip_top as c_short;
                obj_clip_left = ((Char.curr_col as i32) << 5) as c_short + 9;
                add_objtable(4);
            }
        }
    }
}

// seg003:080A
#[no_mangle]
pub unsafe extern "C" fn jump_through_mirror() {
    loadkid();
    load_frame();
    check_mirror_image();
    jumped_through_mirror = 0;
    Char.charid = charids_charid_1_shadow as u8;
    play_sound(soundids_sound_45_jump_through_mirror as c_int);
    saveshad();
    guardhp_max = hitp_max;
    guardhp_curr = hitp_max;
    hitp_curr = 1;
    draw_kid_hp(1, hitp_max as c_short);
    draw_guard_hp(guardhp_curr as c_short, guardhp_max as c_short);
}

// seg003:085B
#[no_mangle]
pub unsafe extern "C" fn check_mirror_image() {
    let xpos: i16 =
        x_bump_at((Char.curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i16 + 10;
    let mut dist = distance_to_edge_weight();
    if Char.direction >= directions_dir_0_right as i8 {
        dist = (!dist) + TILE_SIZEX as c_int;
    }
    distance_mirror = (dist - 2) as i8;
    Char.x = ((xpos << 1) - Char.x as i16) as u8;
    Char.direction = !Char.direction;
}

// seg003:08AA
#[no_mangle]
pub unsafe extern "C" fn bump_into_opponent() {
    if can_guard_see_kid >= 2
        && Char.sword == sword_status_sword_0_sheathed as u8
        && Opp.sword != sword_status_sword_0_sheathed as u8
        && Opp.action < 2
        && Char.direction != Opp.direction
    {
        let distance = char_opp_dist();
        if distance.abs() <= 15 {
            if (*fixes).fix_painless_fall_on_guard != 0 {
                if Char.fall_y >= 33 {
                    return;
                } else if Char.fall_y >= 22 {
                    take_hp(1);
                    play_sound(soundids_sound_16_medium_land as c_int);
                }
            }
            if (*fixes).fix_jumping_over_guard != 0 {
                if (Char.direction == directions_dir_0_right as i8 && Char.x > Opp.x)
                    || (Char.direction == directions_dir_FF_left as i8 && Char.x < Opp.x)
                {
                    Char.x = Opp.x;
                }
            }
            Char.y = y_land_at(Char.curr_row as usize + 1) as u8;
            Char.fall_y = 0;
            seqtbl_offset_char(seqids_seq_47_bump as c_short);
            play_seq();
        }
    }
}

// seg003:0913
#[no_mangle]
pub unsafe extern "C" fn pos_guards() {
    for room1 in 0..ROOMCOUNT as usize {
        let guard_tile = level.guards_tile[room1] as i16;
        if guard_tile < 30 {
            level.guards_x[room1] = x_bump_at(
                (guard_tile % 10) as usize + FIRST_ONSCREEN_COLUMN as usize,
            )
            .wrapping_add(TILE_SIZEX as u8);
            level.guards_seq_hi[room1] = 0;
        }
    }
}

// seg003:0959
#[no_mangle]
pub unsafe extern "C" fn check_can_guard_see_kid() {
    let kid_frame = Kid.frame;
    if Guard.charid == charids_charid_24_mouse as u8 {
        can_guard_see_kid = 0;
        return;
    }
    if (Guard.charid != charids_charid_1_shadow as u8 || current_level == 12)
        && kid_frame != 0
        && (kid_frame < frameids_frame_219_exit_stairs_3 as u8 || kid_frame >= 229)
        && Guard.direction != directions_dir_56_none as i8
        && Kid.alive < 0
        && Guard.alive < 0
        && Kid.room == Guard.room
        && Kid.curr_row == Guard.curr_row
    {
        can_guard_see_kid = 2;
        let mut left_pos: i16 =
            x_bump_at((Kid.curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize) as i16
                + TILE_MIDX as i16;
        if (*fixes).fix_doortop_disabling_guard != 0
            && (Kid.action == actions_actions_2_hang_climb as u8
                || Kid.action == actions_actions_6_hang_straight as u8)
        {
            left_pos += TILE_SIZEX as i16;
        }
        let mut right_pos: i16 =
            x_bump_at((Guard.curr_col as i32 + FIRST_ONSCREEN_COLUMN as i32) as usize)
                as i16
                + TILE_MIDX as i16;
        if left_pos > right_pos {
            core::mem::swap(&mut left_pos, &mut right_pos);
        }
        if get_tile_at_kid(left_pos as c_int) == tiles_tiles_18_chomper as u8 {
            left_pos += TILE_SIZEX as i16;
        }
        let right_tile = get_tile_at_kid(right_pos as c_int);
        if right_tile == tiles_tiles_4_gate as u8
            || ((*fixes).fix_doortop_disabling_guard != 0
                && (right_tile == tiles_tiles_7_doortop_with_floor as u8
                    || right_tile == tiles_tiles_12_doortop as u8))
        {
            right_pos -= TILE_SIZEX as i16;
        }
        if right_pos >= left_pos {
            while left_pos <= right_pos {
                let t = get_tile_at_kid(left_pos as c_int);
                if t == tiles_tiles_20_wall as u8
                    || curr_tile2 == tiles_tiles_7_doortop_with_floor as u8
                    || curr_tile2 == tiles_tiles_12_doortop as u8
                {
                    can_guard_see_kid = 0;
                    return;
                }
                if curr_tile2 == tiles_tiles_11_loose as u8
                    || curr_tile2 == tiles_tiles_18_chomper as u8
                    || (curr_tile2 == tiles_tiles_4_gate as u8
                        && *curr_room_modif.add(curr_tilepos as usize) < 112)
                    || tile_is_floor(curr_tile2 as c_int) == 0
                {
                    can_guard_see_kid = 1;
                }
                left_pos += TILE_SIZEX as i16;
            }
        }
    } else {
        can_guard_see_kid = 0;
    }
}

// seg003:0A99
#[no_mangle]
pub unsafe extern "C" fn get_tile_at_kid(xpos: c_int) -> byte {
    get_tile(Kid.room as c_int, get_tile_div_mod_m7(xpos), Kid.curr_row as c_int) as byte
}

// seg003:0ABA
#[no_mangle]
pub unsafe extern "C" fn do_mouse() {
    loadkid();
    Char.charid = (*custom).mouse_object;
    Char.x = (*custom).mouse_start_x;
    Char.curr_row = 0;
    Char.y = y_land_at(Char.curr_row as usize + 1) as u8;
    Char.alive = -1;
    Char.direction = directions_dir_FF_left as i8;
    guardhp_curr = 1;
    seqtbl_offset_char(seqids_seq_105_mouse_forward as c_short);
    play_seq();
    saveshad();
}

// seg003:0AFC
#[no_mangle]
pub unsafe extern "C" fn flash_if_hurt() -> c_int {
    if flash_time != 0 {
        do_flash(flash_color as c_short);
        return 1;
    } else if hitp_delta < 0 {
        if is_joyst_mode != 0 && enable_controller_rumble != 0 {
            if !sdl_haptic.is_null() {
                SDL_HapticRumblePlay(sdl_haptic, 1.0, 100);
            } else if !sdl_controller_.is_null() {
                SDL_GameControllerRumble(sdl_controller_, 0xFFFF, 0xFFFF, 100);
            } else {
                SDL_JoystickRumble(sdl_joystick_, 0xFFFF, 0xFFFF, 100);
            }
        }
        do_flash(colorids_color_12_brightred as c_short);
        return 1;
    }
    0
}

// seg003:0B1A
#[no_mangle]
pub unsafe extern "C" fn remove_flash_if_hurt() {
    if flash_time != 0 {
        flash_time -= 1;
    } else if hitp_delta >= 0 {
        return;
    }
    remove_flash();
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
