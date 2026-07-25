use std::env;
use std::path::PathBuf;

fn main() {
    // Only re-run this script when C sources or headers change, not on every Rust edit.
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=rust/src/seqtbl.rs");
    println!("cargo:rerun-if-changed=rust/src/seg004.rs");
    println!("cargo:rerun-if-changed=rust/src/seg005.rs");
    println!("cargo:rerun-if-changed=rust/src/seg006.rs");
    println!("cargo:rerun-if-changed=rust/src/seg007.rs");
    println!("cargo:rerun-if-changed=rust/src/seg003.rs");
    println!("cargo:rerun-if-changed=rust/src/seg002.rs");
    println!("cargo:rerun-if-changed=rust/src/seg001.rs");
    println!("cargo:rerun-if-changed=rust/src/seg008.rs");
    println!("cargo:rerun-if-changed=rust/src/seg000.rs");
    println!("cargo:rerun-if-changed=rust/src/seg009.rs");
    println!("cargo:rerun-if-changed=rust/src/sdl_rw_wrappers.rs");
    println!("cargo:rerun-if-changed=rust/src/lighting.rs");
    println!("cargo:rerun-if-changed=rust/src/state_dump.rs");
    println!("cargo:rerun-if-changed=rust/src/options.rs");
    println!("cargo:rerun-if-changed=rust/src/screenshot.rs");
    println!("cargo:rerun-if-changed=rust/src/replay.rs");
    println!("cargo:rerun-if-changed=rust/src/opl3.rs");
    println!("cargo:rerun-if-changed=rust/src/midi.rs");

    // Probe SDL2 (auto-emits cargo:rustc-link-* directives)
    let sdl2 = pkg_config::Config::new()
        .probe("sdl2")
        .expect("sdl2 not found via pkg-config; install libsdl2-dev");
    let sdl2_image = pkg_config::Config::new()
        .probe("SDL2_image")
        .expect("SDL2_image not found via pkg-config; install libsdl2-image-dev");

    let include_paths: Vec<PathBuf> = sdl2
        .include_paths
        .iter()
        .chain(sdl2_image.include_paths.iter())
        .cloned()
        .collect();

    // Compile all C sources except main.c (Rust provides main)
    // Ported to Rust: seg004
    // data.c ported to Rust (rust/src/globals.rs) -- see docs/plans Step B
    let sources = [
        // seg000.c ported to Rust
        // seg008.c ported to Rust
        // seg009.c ported to Rust
        // seqtbl.c ported to Rust
        // options.c ported to Rust
        // replay.c ported to Rust
        // sdl_rw_wrappers.c ported to Rust
        // lighting.c ported to Rust
        // screenshot.c ported to Rust
        // menu.c ported to Rust
        // midi.c ported to Rust
        // opl3.c ported to Rust
        "src/stb_vorbis.c",
        // state_dump.c ported to Rust
    ];

    let mut build = cc::Build::new();
    build
        .std("c99")
        .define("_GNU_SOURCE", "1")
        .flag("-O2")
        .flag("-w");

    for path in &include_paths {
        build.include(path);
    }
    for source in &sources {
        build.file(source);
    }
    build.compile("sdlpop");

    println!("cargo:rustc-link-lib=m");

    // Generate bindings from common.h
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    // data.c's globals (declared in data.h, formerly defined in data.c via the
    // `#define BODY` trick) are now defined directly in Rust (rust/src/globals.rs, see
    // docs/plans Step B). Block bindgen from emitting `extern "C" { pub static mut FOO: T; }`
    // for these -- otherwise they'd collide with globals.rs's real Rust definitions of the
    // same names at crate-root scope (duplicate-definition compile error). This list is the
    // exact set of top-level `extern` declarations in src/data.h.
    const DATA_C_GLOBALS: &str = r"^(Char|Guard|Kid|Opp|above_row_coll_flags|above_row_coll_room|always_use_original_graphics|always_use_original_music|backtable|below_row_coll_flags|below_row_coll_room|can_guard_see_kid|char_bottom_row|char_col_left|char_col_right|char_height|char_top_row|char_top_y|char_width_half|char_x_left|char_x_left_coll|char_x_right|char_x_right_coll|cheats_enabled|checkpoint|chtab_addrs|chtab_flip_clip|chtab_shift|chtab_title40|chtab_title50|collision_row|control_backward|control_down|control_forward|control_shift|control_shift2|control_up|control_x|control_y|copyprot_dialog|copyprot_idx|copyprot_letter|copyprot_plac|copyprot_room|copyprot_tile|cplevel_entr|ctrl1_backward|ctrl1_down|ctrl1_forward|ctrl1_shift2|ctrl1_up|cur_frame|curmob|curr_guard_color|curr_modifier|curr_objs|curr_room|curr_room_modif|curr_room_tiles|curr_row_coll_flags|curr_row_coll_room|curr_tick|curr_tile|curr_tile2|curr_tilepos|current_level|current_sound|current_target_surface|custom|custom_defaults|custom_saved|debug_cheats_enabled|demo_index|demo_mode|demo_time|dialog_rect_1|dialog_rect_2|dialog_settings|different_room|dir_behind|dir_front|dont_reset_time|doorlink1_ad|doorlink2_ad|draw_mode|draw_xh|drawn_room|drects|drects_count|droppedout|edge_type|enable_controller_rumble|enable_copyprot|enable_fade|enable_flash|enable_info_screen|enable_lighting|enable_music|enable_pause_menu|enable_quicksave|enable_quicksave_penalty|enable_replay|enable_text|escape_key_suppressed|exit_room_timer|fade_palette_buffer|fall_frame|fixes|fixes_disabled_state|fixes_saved|flash_color|flash_time|foretable|full_image|g_argc|g_argv|g_deprecation_number|gamecontrollerdb_file|global_blink_state|grab_timer|graphics_mode|guard_notice_timer|guard_palettes|guard_refrac|guard_skill|guardhp_curr|guardhp_delta|guardhp_max|have_keyboard_or_controller_input|have_mouse_input|have_sword|hc_font|hc_small_font|hitp_beg_lev|hitp_curr|hitp_delta|hitp_max|hof_count|holding_sword|infrontx|is_blind_mode|is_cutscene|is_ending_sequence|is_feather_fall|is_feather_timer_displayed|is_global_fading|is_guard_notice|is_joyst_mode|is_keyboard_mode|is_menu_shown|is_overlay_displayed|is_paused|is_renderer_targettexture_supported|is_restart_level|is_screaming|is_show_time|is_sound_on|is_timer_displayed|is_validate_mode|joy_axis|joy_axis_max|joy_button_states|joy_left_stick_states|joy_right_stick_states|joystick_only_horizontal|joystick_threshold|jumped_through_mirror|justblocked|keep_last_seed|key_action|key_down|key_enter|key_esc|key_jump_left|key_jump_right|key_left|key_right|key_states|key_up|kid_sword_strike|knock|last_any_key_scancode|last_key_scancode|last_loose_sound|leftroom_|level|leveldoor_open|leveldoor_right|leveldoor_ybottom|levelset_name|lighting_mask|loaded_room|menu_control_scroll_y|merged_surface|midtable|milliseconds_per_counter|mobs|mobs_count|mod_data_path|mods_folder|mouse_button_clicked_right|mouse_clicked|mouse_moved|mouse_x|mouse_y|n_curr_objs|need_drects|need_full_redraw|need_level1_music|need_quick_load|need_quick_save|need_quotes|need_replay_cycle|need_start_replay|next_level|next_room|next_sound|num_replay_ticks|obj_chtab|obj_clip_bottom|obj_clip_left|obj_clip_right|obj_clip_top|obj_direction|obj_id|obj_tilepos|obj_x|obj_xh|obj_xl|obj_y|objtable|offguard|offscreen_surface|onscreen_surface_|overlay_surface|palace_wall_colors|peels_count|peels_table|perf_counters_per_tick|perf_frequency|pickup_obj_type|play_demo_level|pop_window_height|pop_window_width|preserved_seed|pressed_enter|prev_char_col_left|prev_char_col_right|prev_char_top_row|prev_coll_flags|prev_coll_room|prev_collision_row|random_seed|recording|rect_bottom_text|rect_top|redraw_frames2|redraw_frames_above|redraw_frames_anim|redraw_frames_floor_overlay|redraw_frames_fore|redraw_frames_full|redraw_height|rem_min|rem_tick|renderer_|replay_seek_target|replaying|replays_folder|resurrect_time|room_A|room_AL|room_AR|room_B|room_BL|room_BR|room_L|room_R|roomleave_result|row_below_left_|saved_random_seed|scaling_type|screen_rect|sdl_controller_|sdl_haptic|sdl_joystick_|seamless|seed_was_init|shadow_initialized|skip_mod_data_files|skip_normal_data_files|skipping_replay|sound_flags|sound_interruptible|sound_mode|sound_names|sound_pointers|special_move|start_fullscreen|start_level|super_jump_col|super_jump_fall|super_jump_room|super_jump_row|super_jump_timer|table_counts|target_texture|tbl_cutscenes|tbl_line|text_time_remaining|text_time_total|textstate|texture_blurry|texture_fuzzy|texture_sharp|through_tile|tile_col|tile_object_redraw|tile_row|torch_colors|trob|trobs|trobs_count|united_with_shadow|upside_down|use_correct_aspect_ratio|use_custom_levelset|use_custom_options|use_fixes_and_enhancements|use_hardware_acceleration|use_integer_scaling|using_sdl_joystick_interface|window_|wipe_frames|wipe_heights|wipetable|x_bump|y_clip|y_land)$";

    let mut builder = bindgen::Builder::default()
        .header("src/common.h")
        .clang_arg("-std=c99")
        .clang_arg("-D_GNU_SOURCE=1")
        .allowlist_file(r".*src/.*")
        .blocklist_var(DATA_C_GLOBALS)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    for path in &include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    builder
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
