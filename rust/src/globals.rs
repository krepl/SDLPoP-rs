// Translated from src/data.h / src/data.c (the `#define BODY` trick) as plain Rust
// statics. Mechanical 1:1 translation -- same names, same types, same
// unsafe { FOO = ...; ... FOO ... } access pattern every existing call site already
// uses. No redesign here; see docs/plans (Step D) for the eventual State-struct
// consolidation. `data.c` no longer exists -- these are the sole definitions.
//
// Spliced directly into lib.rs's crate-root scope via `include!` (like bindings.rs),
// so no `use super::*;` needed -- everything is already in scope at this point, and
// lib.rs's own `#![allow(non_upper_case_globals)]` already covers this file.

#[no_mangle]
pub static mut Char: char_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut Guard: char_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut Kid: char_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut Opp: char_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut above_row_coll_flags: [byte; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut above_row_coll_room: [sbyte; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut always_use_original_graphics: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut always_use_original_music: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut backtable: [back_table_type; 200usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut below_row_coll_flags: [byte; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut below_row_coll_room: [sbyte; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut can_guard_see_kid: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_bottom_row: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_col_left: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_col_right: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_height: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_top_row: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_top_y: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_width_half: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_x_left: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_x_left_coll: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_x_right: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut char_x_right_coll: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut cheats_enabled: word = 0;
#[no_mangle]
pub static mut checkpoint: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut chtab_addrs: [*mut chtab_type; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static chtab_flip_clip: [byte; 10usize] = [1,0,1,1,1,1,0,0,0,0];
#[no_mangle]
pub static chtab_shift: [byte; 10usize] = [0,1,0,0,0,0,1,1,1,0];
#[no_mangle]
pub static mut chtab_title40: *mut chtab_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut chtab_title50: *mut chtab_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut collision_row: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut control_backward: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut control_down: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut control_forward: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut control_shift: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut control_shift2: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut control_up: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut control_x: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut control_y: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut copyprot_dialog: *mut dialog_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut copyprot_idx: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static copyprot_letter: [::std::os::raw::c_char; 40usize] = [b'A' as ::std::os::raw::c_char,b'A' as ::std::os::raw::c_char,b'B' as ::std::os::raw::c_char,b'B' as ::std::os::raw::c_char,b'C' as ::std::os::raw::c_char,b'C' as ::std::os::raw::c_char,b'D' as ::std::os::raw::c_char,b'D' as ::std::os::raw::c_char,b'E' as ::std::os::raw::c_char,b'F' as ::std::os::raw::c_char,b'F' as ::std::os::raw::c_char,b'G' as ::std::os::raw::c_char,b'H' as ::std::os::raw::c_char,b'H' as ::std::os::raw::c_char,b'I' as ::std::os::raw::c_char,b'I' as ::std::os::raw::c_char,b'J' as ::std::os::raw::c_char,b'J' as ::std::os::raw::c_char,b'K' as ::std::os::raw::c_char,b'L' as ::std::os::raw::c_char,b'L' as ::std::os::raw::c_char,b'M' as ::std::os::raw::c_char,b'M' as ::std::os::raw::c_char,b'N' as ::std::os::raw::c_char,b'O' as ::std::os::raw::c_char,b'O' as ::std::os::raw::c_char,b'P' as ::std::os::raw::c_char,b'P' as ::std::os::raw::c_char,b'R' as ::std::os::raw::c_char,b'R' as ::std::os::raw::c_char,b'S' as ::std::os::raw::c_char,b'S' as ::std::os::raw::c_char,b'T' as ::std::os::raw::c_char,b'T' as ::std::os::raw::c_char,b'U' as ::std::os::raw::c_char,b'U' as ::std::os::raw::c_char,b'V' as ::std::os::raw::c_char,b'Y' as ::std::os::raw::c_char,b'W' as ::std::os::raw::c_char,b'Y' as ::std::os::raw::c_char];
#[no_mangle]
pub static mut copyprot_plac: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut copyprot_room: [word; 14usize] = [3,  3,  3,  3,  3,  3,  4,  4,  4,  4,  4,  4,  4,  4];
#[no_mangle]
pub static copyprot_tile: [word; 14usize] = [1,  5,  7,  9, 11, 21,  1,  3,  7, 11, 17, 21, 25, 27];
#[no_mangle]
pub static mut cplevel_entr: [word; 14usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut ctrl1_backward: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut ctrl1_down: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut ctrl1_forward: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut ctrl1_shift2: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut ctrl1_up: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut cur_frame: frame_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curmob: mob_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_guard_color: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_modifier: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_objs: [::std::os::raw::c_short; 50usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_room: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_room_modif: *mut byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_room_tiles: *mut byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_row_coll_flags: [byte; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_row_coll_room: [sbyte; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_tick: dword = 0;
#[no_mangle]
pub static mut curr_tile: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_tile2: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut curr_tilepos: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut current_level: word = u16::MAX; // C: INIT(= -1), wraps to u16::MAX
#[no_mangle]
pub static mut current_sound: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut custom_saved: custom_options_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut debug_cheats_enabled: byte = 0;
#[no_mangle]
pub static mut demo_index: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut demo_mode: word = 0;
#[no_mangle]
pub static mut demo_time: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut different_room: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static dir_behind: [sbyte; 2usize] = [1, -1];
#[no_mangle]
pub static dir_front: [sbyte; 2usize] = [-1, 1];
#[no_mangle]
pub static mut dont_reset_time: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut doorlink1_ad: *mut byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut doorlink2_ad: *mut byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut draw_mode: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut draw_xh: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut drawn_room: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut drects: [rect_type; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut drects_count: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut droppedout: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut edge_type: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut enable_controller_rumble: byte = 0;
#[no_mangle]
pub static mut enable_copyprot: byte = 0;
#[no_mangle]
pub static mut enable_fade: byte = 1;
#[no_mangle]
pub static mut enable_flash: byte = 1;
#[no_mangle]
pub static mut enable_info_screen: byte = 1;
#[no_mangle]
pub static mut enable_lighting: byte = 0;
#[no_mangle]
pub static mut enable_music: byte = 1;
#[no_mangle]
pub static mut enable_pause_menu: byte = 1;
#[no_mangle]
pub static mut enable_quicksave: byte = 1;
#[no_mangle]
pub static mut enable_quicksave_penalty: byte = 1;
#[no_mangle]
pub static mut enable_replay: byte = 1;
#[no_mangle]
pub static mut enable_text: byte = 1;
#[no_mangle]
pub static mut escape_key_suppressed: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut exit_room_timer: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut fade_palette_buffer: *mut palette_fade_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut fall_frame: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut fixes_disabled_state: fixes_options_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut fixes_saved: fixes_options_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut flash_color: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut flash_time: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut foretable: [back_table_type; 200usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut g_argc: ::std::os::raw::c_int = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut g_argv: *mut *mut ::std::os::raw::c_char = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut g_deprecation_number: ::std::os::raw::c_int = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut global_blink_state: bool = false;
#[no_mangle]
pub static mut grab_timer: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut graphics_mode: byte = 0;
#[no_mangle]
pub static mut guard_notice_timer: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut guard_palettes: *mut byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut guard_refrac: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut guard_skill: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut guardhp_curr: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut guardhp_delta: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut guardhp_max: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut have_keyboard_or_controller_input: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut have_mouse_input: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut have_sword: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut hitp_beg_lev: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut hitp_curr: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut hitp_delta: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut hitp_max: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut hof_count: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut holding_sword: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut infrontx: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_blind_mode: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_cutscene: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_ending_sequence: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_feather_fall: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_feather_timer_displayed: byte = 0;
#[no_mangle]
pub static mut is_global_fading: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_guard_notice: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_joyst_mode: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_keyboard_mode: word = 0;
#[no_mangle]
pub static mut is_menu_shown: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_overlay_displayed: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_paused: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_renderer_targettexture_supported: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_restart_level: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_screaming: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_show_time: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut is_sound_on: byte = 0x0F;
#[no_mangle]
pub static mut is_timer_displayed: byte = 0;
#[no_mangle]
pub static mut is_validate_mode: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut joy_axis: [::std::os::raw::c_int; 6usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut joy_axis_max: [::std::os::raw::c_int; 6usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut joy_button_states: [::std::os::raw::c_int; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut joy_left_stick_states: [::std::os::raw::c_int; 2usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut joy_right_stick_states: [::std::os::raw::c_int; 2usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut joystick_only_horizontal: byte = 0;
#[no_mangle]
pub static mut joystick_threshold: ::std::os::raw::c_int = 8000;
#[no_mangle]
pub static mut jumped_through_mirror: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut justblocked: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut keep_last_seed: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut key_action: ::std::os::raw::c_int = 229;
#[no_mangle]
pub static mut key_down: ::std::os::raw::c_int = 81;
#[no_mangle]
pub static mut key_enter: ::std::os::raw::c_int = 40;
#[no_mangle]
pub static mut key_esc: ::std::os::raw::c_int = 41;
#[no_mangle]
pub static mut key_jump_left: ::std::os::raw::c_int = 74;
#[no_mangle]
pub static mut key_jump_right: ::std::os::raw::c_int = 75;
#[no_mangle]
pub static mut key_left: ::std::os::raw::c_int = 80;
#[no_mangle]
pub static mut key_right: ::std::os::raw::c_int = 79;
#[no_mangle]
pub static mut key_states: [byte; 512usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut key_up: ::std::os::raw::c_int = 82;
#[no_mangle]
pub static mut kid_sword_strike: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut knock: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut last_any_key_scancode: ::std::os::raw::c_int = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut last_key_scancode: ::std::os::raw::c_int = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut last_loose_sound: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut leftroom_: [tile_and_mod; 3usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut level: level_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut leveldoor_open: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut leveldoor_right: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut leveldoor_ybottom: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut levelset_name: [::std::os::raw::c_char; 256usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut lighting_mask: *mut image_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut loaded_room: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut menu_control_scroll_y: ::std::os::raw::c_int = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut merged_surface: *mut SDL_Surface = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut midtable: [midtable_type; 50usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut milliseconds_per_counter: f32 = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut mobs: [mob_type; 14usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut mobs_count: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut mod_data_path: [::std::os::raw::c_char; 256usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut mouse_button_clicked_right: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut mouse_clicked: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut mouse_moved: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut mouse_x: ::std::os::raw::c_int = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut mouse_y: ::std::os::raw::c_int = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut n_curr_objs: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut need_drects: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut need_full_redraw: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut need_level1_music: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut need_quick_load: ::std::os::raw::c_int = 0;
#[no_mangle]
pub static mut need_quick_save: ::std::os::raw::c_int = 0;
#[no_mangle]
pub static mut need_quotes: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut need_replay_cycle: byte = 0;
#[no_mangle]
pub static mut need_start_replay: byte = 0;
#[no_mangle]
pub static mut next_level: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut next_room: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut next_sound: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut num_replay_ticks: dword = 0;
#[no_mangle]
pub static mut obj_chtab: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_clip_bottom: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_clip_left: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_clip_right: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_clip_top: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_direction: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_id: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_tilepos: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_x: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_xh: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_xl: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut obj_y: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut objtable: [objtable_type; 50usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut offguard: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut offscreen_surface: *mut surface_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut onscreen_surface_: *mut SDL_Surface = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut overlay_surface: *mut SDL_Surface = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut palace_wall_colors: [byte; 132usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut peels_count: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut peels_table: [*mut peel_type; 50usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut perf_counters_per_tick: Uint64 = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut perf_frequency: Uint64 = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut pickup_obj_type: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut play_demo_level: ::std::os::raw::c_int = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut pop_window_height: word = 400;
#[no_mangle]
pub static mut pop_window_width: word = 640;
#[no_mangle]
pub static mut preserved_seed: dword = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut pressed_enter: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut prev_char_col_left: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut prev_char_col_right: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut prev_char_top_row: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut prev_coll_flags: [byte; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut prev_coll_room: [sbyte; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut prev_collision_row: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut random_seed: dword = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut recording: byte = 0;
#[no_mangle]
pub static mut redraw_frames2: [byte; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut redraw_frames_above: [byte; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut redraw_frames_anim: [byte; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut redraw_frames_floor_overlay: [byte; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut redraw_frames_fore: [byte; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut redraw_frames_full: [byte; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut redraw_height: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut rem_min: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut rem_tick: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut renderer_: *mut SDL_Renderer = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut replay_seek_target: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut replaying: byte = 0;
#[no_mangle]
pub static mut resurrect_time: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut room_A: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut room_AL: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut room_AR: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut room_B: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut room_BL: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut room_BR: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut room_L: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut room_R: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut roomleave_result: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut row_below_left_: [tile_and_mod; 10usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut saved_random_seed: dword = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut scaling_type: byte = 0;
#[no_mangle]
pub static mut sdl_haptic: *mut SDL_Haptic = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut sdl_joystick_: *mut SDL_Joystick = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut seamless: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut seed_was_init: word = 0;
#[no_mangle]
pub static mut shadow_initialized: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut skip_mod_data_files: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut skip_normal_data_files: bool = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut skipping_replay: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut sound_flags: byte = 0;
#[no_mangle]
pub static mut sound_interruptible: [byte; 58usize] = [
	0, 
	1, 
	1, 
	1, 
	1, 
	1, 
	0, 
	1, 
	1, 
	1, 
	1, 
	1, 
	1, 
	1, 
	0, 
	0, 
	1, 
	1, 
	0, 
	1, 
	1, 
	1, 
	1, 
	1, 
	0, 
	0, 
	0, 
	0, 
	0, 
	1, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	1, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0, 
	0
];
#[no_mangle]
pub static mut sound_mode: byte = 0;
#[no_mangle]
pub static mut sound_names: *mut *mut ::std::os::raw::c_char = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut sound_pointers: [*mut sound_buffer_type; 58usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut special_move: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut start_fullscreen: byte = 0;
#[no_mangle]
pub static mut start_level: ::std::os::raw::c_short = -1;
#[no_mangle]
pub static mut super_jump_col: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut super_jump_fall: byte = 0;
#[no_mangle]
pub static mut super_jump_room: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut super_jump_row: sbyte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut super_jump_timer: byte = 0;
#[no_mangle]
pub static mut table_counts: [::std::os::raw::c_short; 5usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut target_texture: *mut SDL_Texture = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static tbl_line: [word; 3usize] = [0, 10, 20];
#[no_mangle]
pub static mut text_time_remaining: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut text_time_total: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut texture_blurry: *mut SDL_Texture = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut texture_fuzzy: *mut SDL_Texture = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut texture_sharp: *mut SDL_Texture = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut through_tile: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut tile_col: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut tile_object_redraw: [byte; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut tile_row: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut torch_colors: [[byte; 30usize]; 25usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut trob: trob_type = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut trobs: [trob_type; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut trobs_count: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut united_with_shadow: ::std::os::raw::c_short = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut upside_down: word = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut use_correct_aspect_ratio: byte = 0;
#[no_mangle]
pub static mut use_custom_levelset: byte = 0;
#[no_mangle]
pub static mut use_custom_options: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut use_fixes_and_enhancements: byte = 0;
#[no_mangle]
pub static mut use_hardware_acceleration: byte = 2;
#[no_mangle]
pub static mut use_integer_scaling: byte = 0;
#[no_mangle]
pub static mut using_sdl_joystick_interface: byte = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut window_: *mut SDL_Window = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut wipe_frames: [byte; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut wipe_heights: [sbyte; 30usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
pub static mut wipetable: [wipetable_type; 300usize] = unsafe { core::mem::zeroed() };
#[no_mangle]
// C: INIT(= {-12, 2, ...}); -12 wraps to 244 as an unsigned byte.
pub static x_bump: [byte; 20usize] = [244, 2, 16, 30, 44, 58, 72, 86, 100, 114, 128, 142, 156, 170, 184, 198, 212, 226, 240, 254];
#[no_mangle]
pub static y_clip: [::std::os::raw::c_short; 5usize] = [-60, 3, 66, 129, 192];
#[no_mangle]
pub static y_land: [::std::os::raw::c_short; 5usize] = [-8, 55, 118, 181, 244];

// ---------------------------------------------------------------------------
// Hand-translated: struct/pointer/function-pointer initializers that don't
// fit the mechanical scalar/array translation above.
// ---------------------------------------------------------------------------

#[no_mangle]
pub static mut screen_rect: rect_type = rect_type { top: 0, left: 0, bottom: 200, right: 320 };
#[no_mangle]
pub static mut rect_top: rect_type = rect_type { top: 0, left: 0, bottom: 192, right: 320 };
#[no_mangle]
pub static mut rect_bottom_text: rect_type = rect_type { top: 193, left: 70, bottom: 202, right: 250 };
#[no_mangle]
pub static mut dialog_rect_1: rect_type = rect_type { top: 60, left: 56, bottom: 124, right: 264 };
#[no_mangle]
pub static mut dialog_rect_2: rect_type = rect_type { top: 61, left: 56, bottom: 120, right: 264 };

#[no_mangle]
pub static mut dialog_settings: dialog_settings_type = dialog_settings_type {
    method_1: Some(add_dialog_rect),
    method_2_frame: Some(dialog_method_2_frame),
    top_border: 4,
    left_border: 4,
    bottom_border: 4,
    right_border: 4,
    shadow_bottom: 3,
    shadow_right: 4,
    outer_border: 1,
};

#[no_mangle]
pub static mut hc_font: font_type = font_type {
    first_char: 0x01,
    last_char: 0xFF,
    height_above_baseline: 7,
    height_below_baseline: 2,
    space_between_lines: 1,
    space_between_chars: 1,
    chtab: std::ptr::null_mut(),
};
#[no_mangle]
pub static mut hc_small_font: font_type = font_type {
    first_char: 32,
    last_char: 126,
    height_above_baseline: 5,
    height_below_baseline: 2,
    space_between_lines: 1,
    space_between_chars: 1,
    chtab: std::ptr::null_mut(),
};
#[no_mangle]
pub static mut textstate: textstate_type = textstate_type {
    current_x: 0,
    current_y: 0,
    textblit: 0,
    textcolor: 15,
    ptr_font: core::ptr::addr_of_mut!(hc_font),
};

#[no_mangle]
pub static mut current_target_surface: *mut surface_type = std::ptr::null_mut();
#[no_mangle]
pub static mut sdl_controller_: *mut SDL_GameController = std::ptr::null_mut();

#[no_mangle]
pub static mut custom: *mut custom_options_type = core::ptr::addr_of_mut!(custom_defaults);
#[no_mangle]
pub static mut fixes: *mut fixes_options_type = core::ptr::addr_of_mut!(fixes_disabled_state);

// Fixed-size char buffers holding a string default: C's `= "text"` zero-fills the
// remainder of the array (string literal + implicit trailing NUL + padding).
#[no_mangle]
pub static mut gamecontrollerdb_file: [::std::os::raw::c_char; 256usize] = [0; 256];
#[no_mangle]
pub static mut mods_folder: [::std::os::raw::c_char; 256usize] = {
    let mut a = [0 as ::std::os::raw::c_char; 256];
    let s = b"mods";
    let mut i = 0;
    while i < s.len() { a[i] = s[i] as ::std::os::raw::c_char; i += 1; }
    a
};
#[no_mangle]
pub static mut replays_folder: [::std::os::raw::c_char; 256usize] = {
    let mut a = [0 as ::std::os::raw::c_char; 256];
    let s = b"replays";
    let mut i = 0;
    while i < s.len() { a[i] = s[i] as ::std::os::raw::c_char; i += 1; }
    a
};

#[no_mangle]
pub static mut tbl_cutscenes: [cutscene_ptr_type; 16usize] = [
    None,
    None,
    Some(cutscene_2_6),
    None,
    Some(cutscene_4),
    None,
    Some(cutscene_2_6),
    None,
    Some(cutscene_8),
    Some(cutscene_9),
    None,
    None,
    Some(cutscene_12),
    None,
    None,
    None,
];

#[no_mangle]
pub static mut full_image: [full_image_type; 11usize] = [
    // TITLE_MAIN
    full_image_type { id: 0, chtab: core::ptr::addr_of_mut!(chtab_title50), blitter: blitters_blitters_0_no_transp, xpos: 0, ypos: 0 },
    // TITLE_PRESENTS
    full_image_type { id: 1, chtab: core::ptr::addr_of_mut!(chtab_title50), blitter: blitters_blitters_0_no_transp, xpos: 96, ypos: 106 },
    // TITLE_GAME
    full_image_type { id: 2, chtab: core::ptr::addr_of_mut!(chtab_title50), blitter: blitters_blitters_0_no_transp, xpos: 96, ypos: 122 },
    // TITLE_POP
    full_image_type { id: 3, chtab: core::ptr::addr_of_mut!(chtab_title50), blitter: blitters_blitters_10h_transp, xpos: 24, ypos: 107 },
    // TITLE_MECHNER
    full_image_type { id: 4, chtab: core::ptr::addr_of_mut!(chtab_title50), blitter: blitters_blitters_0_no_transp, xpos: 48, ypos: 184 },
    // HOF_POP
    full_image_type { id: 3, chtab: core::ptr::addr_of_mut!(chtab_title50), blitter: blitters_blitters_10h_transp, xpos: 24, ypos: 24 },
    // STORY_FRAME
    full_image_type { id: 0, chtab: core::ptr::addr_of_mut!(chtab_title40), blitter: blitters_blitters_0_no_transp, xpos: 0, ypos: 0 },
    // STORY_ABSENCE
    full_image_type { id: 1, chtab: core::ptr::addr_of_mut!(chtab_title40), blitter: blitters_blitters_white, xpos: 24, ypos: 25 },
    // STORY_MARRY
    full_image_type { id: 2, chtab: core::ptr::addr_of_mut!(chtab_title40), blitter: blitters_blitters_white, xpos: 24, ypos: 25 },
    // STORY_HAIL
    full_image_type { id: 3, chtab: core::ptr::addr_of_mut!(chtab_title40), blitter: blitters_blitters_white, xpos: 24, ypos: 25 },
    // STORY_CREDITS
    full_image_type { id: 4, chtab: core::ptr::addr_of_mut!(chtab_title40), blitter: blitters_blitters_white, xpos: 24, ypos: 26 },
];

#[no_mangle]
pub static mut custom_defaults: custom_options_type = custom_options_type {
    start_minutes_left: 60,
    start_ticks_left: 719,
    start_hitp: 3,
    max_hitp_allowed: 10,
    saving_allowed_first_level: 3,
    saving_allowed_last_level: 13,
    start_upside_down: 0,
    start_in_blind_mode: 0,
    copyprot_level: 2,
    drawn_tile_top_level_edge: tiles_tiles_1_floor as byte,
    drawn_tile_left_level_edge: tiles_tiles_20_wall as byte,
    level_edge_hit_tile: tiles_tiles_20_wall as byte,
    allow_triggering_any_tile: 0,
    enable_wda_in_palace: 0,
    vga_palette: [
        rgb_type { r: 0x00, g: 0x00, b: 0x00 },
        rgb_type { r: 0x00, g: 0x00, b: 0x2A },
        rgb_type { r: 0x00, g: 0x2A, b: 0x00 },
        rgb_type { r: 0x00, g: 0x2A, b: 0x2A },
        rgb_type { r: 0x2A, g: 0x00, b: 0x00 },
        rgb_type { r: 0x2A, g: 0x00, b: 0x2A },
        rgb_type { r: 0x2A, g: 0x15, b: 0x00 },
        rgb_type { r: 0x2A, g: 0x2A, b: 0x2A },
        rgb_type { r: 0x15, g: 0x15, b: 0x15 },
        rgb_type { r: 0x15, g: 0x15, b: 0x3F },
        rgb_type { r: 0x15, g: 0x3F, b: 0x15 },
        rgb_type { r: 0x15, g: 0x3F, b: 0x3F },
        rgb_type { r: 0x3F, g: 0x15, b: 0x15 },
        rgb_type { r: 0x3F, g: 0x15, b: 0x3F },
        rgb_type { r: 0x3F, g: 0x3F, b: 0x15 },
        rgb_type { r: 0x3F, g: 0x3F, b: 0x3F },
    ],
    first_level: 1,
    skip_title: 0,
    shift_L_allowed_until_level: 4,
    shift_L_reduced_minutes: 15,
    shift_L_reduced_ticks: 719,
    demo_hitp: 4,
    demo_end_room: 24,
    intro_music_level: 1,
    have_sword_from_level: 2,
    checkpoint_level: 3,
    checkpoint_respawn_dir: directions_dir_FF_left as sbyte,
    checkpoint_respawn_room: 2,
    checkpoint_respawn_tilepos: 6,
    checkpoint_clear_tile_room: 7,
    checkpoint_clear_tile_col: 4,
    checkpoint_clear_tile_row: 0,
    skeleton_level: 3,
    skeleton_room: 1,
    skeleton_trigger_column_1: 2,
    skeleton_trigger_column_2: 3,
    skeleton_column: 5,
    skeleton_row: 1,
    skeleton_require_open_level_door: 1,
    skeleton_skill: 2,
    skeleton_reappear_room: 3,
    skeleton_reappear_x: 133,
    skeleton_reappear_row: 1,
    skeleton_reappear_dir: directions_dir_0_right as byte,
    mirror_level: 4,
    mirror_room: 4,
    mirror_column: 4,
    mirror_row: 0,
    mirror_tile: tiles_tiles_13_mirror as byte,
    show_mirror_image: 1,
    shadow_steal_level: 5,
    shadow_steal_room: 24,
    shadow_step_level: 6,
    shadow_step_room: 1,
    falling_exit_level: 6,
    falling_exit_room: 1,
    falling_entry_level: 7,
    falling_entry_room: 17,
    mouse_level: 8,
    mouse_room: 16,
    mouse_delay: 150,
    mouse_object: 24,
    mouse_start_x: 200,
    loose_tiles_level: 13,
    loose_tiles_room_1: 23,
    loose_tiles_room_2: 16,
    loose_tiles_first_tile: 22,
    loose_tiles_last_tile: 27,
    jaffar_victory_level: 13,
    jaffar_victory_flash_time: 18,
    hide_level_number_from_level: 14,
    level_13_level_number: 12,
    victory_stops_time_level: 13,
    win_level: 14,
    win_room: 5,
    loose_floor_delay: 11,
    tbl_level_type: [0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 0, 0, 1, 0],
    tbl_level_color: [0, 0, 0, 1, 0, 0, 0, 1, 2, 2, 0, 0, 3, 3, 4, 0],
    tbl_guard_type: [0, 0, 0, 2, 0, 0, 1, 0, 0, 0, 0, 0, 4, 3, -1, -1],
    tbl_guard_hp: [4, 3, 3, 3, 3, 4, 5, 4, 4, 5, 5, 5, 4, 6, 0, 0],
    tbl_cutscenes_by_index: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    tbl_entry_pose: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0],
    tbl_seamless_exit: [-1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 23, -1, -1, -1],
    strikeprob: [61, 100, 61, 61, 61, 40, 100, 220, 0, 48, 32, 48],
    restrikeprob: [0, 0, 0, 5, 5, 175, 16, 8, 0, 255, 255, 150],
    blockprob: [0, 150, 150, 200, 200, 255, 200, 250, 0, 255, 255, 255],
    impblockprob: [0, 61, 61, 100, 100, 145, 100, 250, 0, 145, 255, 175],
    advprob: [255, 200, 200, 200, 255, 255, 200, 0, 0, 255, 100, 100],
    refractimer: [16, 16, 16, 16, 8, 8, 8, 8, 0, 8, 0, 0],
    extrastrength: [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
    init_shad_6: [0x0F, 0x51, 0x76, 0, 0, 1, 0, 0],
    init_shad_5: [0x0F, 0x37, 0x37, 0, 0xFF, 0, 0, 0],
    init_shad_12: [0x0F, 0x51, 0xE8, 0, 0, 0, 0, 0],
    demo_moves: [
        auto_move_type { time: 0x00, move_: 0 },
        auto_move_type { time: 0x01, move_: 1 },
        auto_move_type { time: 0x0D, move_: 0 },
        auto_move_type { time: 0x1E, move_: 1 },
        auto_move_type { time: 0x25, move_: 5 },
        auto_move_type { time: 0x2F, move_: 0 },
        auto_move_type { time: 0x30, move_: 1 },
        auto_move_type { time: 0x41, move_: 0 },
        auto_move_type { time: 0x49, move_: 2 },
        auto_move_type { time: 0x4B, move_: 0 },
        auto_move_type { time: 0x63, move_: 2 },
        auto_move_type { time: 0x64, move_: 0 },
        auto_move_type { time: 0x73, move_: 5 },
        auto_move_type { time: 0x80, move_: 6 },
        auto_move_type { time: 0x88, move_: 3 },
        auto_move_type { time: 0x9D, move_: 7 },
        auto_move_type { time: 0x9E, move_: 0 },
        auto_move_type { time: 0x9F, move_: 1 },
        auto_move_type { time: 0xAB, move_: 4 },
        auto_move_type { time: 0xB1, move_: 0 },
        auto_move_type { time: 0xB2, move_: 1 },
        auto_move_type { time: 0xBC, move_: 0 },
        auto_move_type { time: 0xC1, move_: 1 },
        auto_move_type { time: 0xCD, move_: 0 },
        auto_move_type { time: 0xE9, move_: -1 },
    ],
    shad_drink_move: [
        auto_move_type { time: 0x00, move_: 0 },
        auto_move_type { time: 0x01, move_: 1 },
        auto_move_type { time: 0x0E, move_: 0 },
        auto_move_type { time: 0x12, move_: 6 },
        auto_move_type { time: 0x1D, move_: 7 },
        auto_move_type { time: 0x2D, move_: 2 },
        auto_move_type { time: 0x31, move_: 1 },
        auto_move_type { time: 0xFF, move_: -2 },
    ],
    base_speed: 5,
    fight_speed: 6,
    chomper_speed: 15,
    no_mouse_in_ending: 0,
};
