use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

use super::*;

// ── string / path helpers ────────────────────────────────────────────────────

unsafe fn locate(filename: &CStr) -> CString {
    let mut buf = [0i8; 256];
    let result = locate_file_(filename.as_ptr(), buf.as_mut_ptr(), 256);
    CStr::from_ptr(result).to_owned()
}

unsafe fn copy_cstr_to_array<const N: usize>(dst: *mut [c_char; N], src: &CStr) {
    let bytes = src.to_bytes_with_nul();
    let n = bytes.len().min(N);
    std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, dst as *mut c_char, n);
    (dst as *mut c_char).add(N - 1).write(0);
}

// ── numeric parsing ──────────────────────────────────────────────────────────

fn parse_numeric(value: &str) -> Option<i64> {
    if value.eq_ignore_ascii_case("default") {
        return None;
    }
    let v = value.trim();
    let (neg, v) = if let Some(rest) = v.strip_prefix('-') {
        (true, rest)
    } else {
        (false, v)
    };
    let n: i64 = if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()?
    } else {
        v.parse().ok()?
    };
    Some(if neg { -n } else { n })
}

fn parse_bool(value: &str) -> Option<u8> {
    if value.eq_ignore_ascii_case("true") {
        return Some(1);
    }
    if value.eq_ignore_ascii_case("false") {
        return Some(0);
    }
    None
}

// ── named-value lookup tables ────────────────────────────────────────────────

fn lookup_scaling_type(v: &str) -> Option<i64> {
    match () {
        _ if v.eq_ignore_ascii_case("sharp") => Some(0),
        _ if v.eq_ignore_ascii_case("fuzzy") => Some(1),
        _ if v.eq_ignore_ascii_case("blurry") => Some(2),
        _ => None,
    }
}

fn lookup_hardware_accel(v: &str) -> Option<i64> {
    match () {
        _ if v.eq_ignore_ascii_case("false") => Some(0),
        _ if v.eq_ignore_ascii_case("true") => Some(1),
        _ if v.eq_ignore_ascii_case("default") => Some(2),
        _ => None,
    }
}

fn lookup_level_type(v: &str) -> Option<i64> {
    match () {
        _ if v.eq_ignore_ascii_case("dungeon") => Some(0),
        _ if v.eq_ignore_ascii_case("palace") => Some(1),
        _ => None,
    }
}

fn lookup_guard_type(v: &str) -> Option<i64> {
    match () {
        _ if v.eq_ignore_ascii_case("none") => Some(-1),
        _ if v.eq_ignore_ascii_case("guard") => Some(0),
        _ if v.eq_ignore_ascii_case("fat") => Some(1),
        _ if v.eq_ignore_ascii_case("skel") => Some(2),
        _ if v.eq_ignore_ascii_case("vizier") => Some(3),
        _ if v.eq_ignore_ascii_case("shadow") => Some(4),
        _ => None,
    }
}

fn lookup_tile_type(v: &str) -> Option<i64> {
    const NAMES: &[&str] = &[
        "empty",
        "floor",
        "spike",
        "pillar",
        "gate",
        "stuck",
        "closer",
        "doortop_with_floor",
        "bigpillar_bottom",
        "bigpillar_top",
        "potion",
        "loose",
        "doortop",
        "mirror",
        "debris",
        "opener",
        "level_door_left",
        "level_door_right",
        "chomper",
        "torch",
        "wall",
        "skeleton",
        "sword",
        "balcony_left",
        "balcony_right",
        "lattice_pillar",
        "lattice_down",
        "lattice_small",
        "lattice_left",
        "lattice_right",
        "torch_with_debris",
    ];
    NAMES
        .iter()
        .position(|&n| v.eq_ignore_ascii_case(n))
        .map(|i| i as i64)
}

fn lookup_direction(v: &str) -> Option<i64> {
    match () {
        _ if v.eq_ignore_ascii_case("left") => Some(-1), // dir_FF_left
        _ if v.eq_ignore_ascii_case("right") => Some(0), // dir_0_right
        _ => None,
    }
}

fn lookup_entry_pose(v: &str) -> Option<i64> {
    match () {
        _ if v.eq_ignore_ascii_case("turning") => Some(0),
        _ if v.eq_ignore_ascii_case("falling") => Some(1),
        _ if v.eq_ignore_ascii_case("running") => Some(2),
        _ => None,
    }
}

fn lookup_row(v: &str) -> Option<i64> {
    match () {
        _ if v.eq_ignore_ascii_case("top") => Some(0),
        _ if v.eq_ignore_ascii_case("middle") => Some(1),
        _ if v.eq_ignore_ascii_case("bottom") => Some(2),
        _ => None,
    }
}

// 16 = "Never" sentinel
fn lookup_never(v: &str) -> Option<i64> {
    if v.eq_ignore_ascii_case("never") {
        Some(16)
    } else {
        None
    }
}

// ── INI parser ───────────────────────────────────────────────────────────────

fn ini_parse_str(content: &str, mut report: impl FnMut(&str, &str, &str)) {
    let mut section = "";

    for line in content.lines() {
        let line = match line.split_once(';') {
            Some((before, _)) => before,
            None => line,
        }
        .trim();

        if line.is_empty() {
            continue;
        }

        if let Some(inner) = line.strip_prefix('[') {
            section = inner
                .split_once(']')
                .map(|(s, _)| s.trim())
                .unwrap_or("");
            continue;
        }

        let (name, value) = match line.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => (line, ""),
        };
        report(section, name, value);
    }
}

fn ini_load(path: &Path, report: impl FnMut(&str, &str, &str)) -> i32 {
    let Ok(content) = std::fs::read_to_string(path) else {
        return -1;
    };
    ini_parse_str(&content, report);
    0
}

// ── callback helpers ─────────────────────────────────────────────────────────

/// Returns true if name matches and the option was handled (whether or not the
/// value was valid). Mirrors the C macros' early-return semantics.
unsafe fn proc_bool(name: &str, value: &str, opt: &str, target: *mut u8) -> bool {
    if !name.eq_ignore_ascii_case(opt) {
        return false;
    }
    if let Some(v) = parse_bool(value) {
        *target = v;
    }
    true
}

unsafe fn proc_u8(
    name: &str,
    value: &str,
    opt: &str,
    named: Option<i64>,
    target: *mut u8,
) -> bool {
    if !name.eq_ignore_ascii_case(opt) {
        return false;
    }
    let n = named.or_else(|| parse_numeric(value));
    if let Some(n) = n {
        *target = n as u8;
    }
    true
}

unsafe fn proc_i8(
    name: &str,
    value: &str,
    opt: &str,
    named: Option<i64>,
    target: *mut i8,
) -> bool {
    if !name.eq_ignore_ascii_case(opt) {
        return false;
    }
    let n = named.or_else(|| parse_numeric(value));
    if let Some(n) = n {
        *target = n as i8;
    }
    true
}

unsafe fn proc_u16(
    name: &str,
    value: &str,
    opt: &str,
    named: Option<i64>,
    target: *mut u16,
) -> bool {
    if !name.eq_ignore_ascii_case(opt) {
        return false;
    }
    let n = named.or_else(|| parse_numeric(value));
    if let Some(n) = n {
        std::ptr::write_unaligned(target, n as u16);
    }
    true
}

unsafe fn proc_i16(
    name: &str,
    value: &str,
    opt: &str,
    named: Option<i64>,
    target: *mut i16,
) -> bool {
    if !name.eq_ignore_ascii_case(opt) {
        return false;
    }
    let n = named.or_else(|| parse_numeric(value));
    if let Some(n) = n {
        std::ptr::write_unaligned(target, n as i16);
    }
    true
}

unsafe fn proc_i32(
    name: &str,
    value: &str,
    opt: &str,
    named: Option<i64>,
    target: *mut i32,
) -> bool {
    if !name.eq_ignore_ascii_case(opt) {
        return false;
    }
    let n = named.or_else(|| parse_numeric(value));
    if let Some(n) = n {
        std::ptr::write_unaligned(target, n as i32);
    }
    true
}

// ── INI callbacks ────────────────────────────────────────────────────────────

unsafe fn global_ini_callback(section: &str, name: &str, value: &str) {
    if section.eq_ignore_ascii_case("General") {
        if proc_bool(name, value, "enable_pause_menu", &raw mut enable_pause_menu) {
            return;
        }
        if name.eq_ignore_ascii_case("mods_folder") {
            if !value.is_empty() && !value.eq_ignore_ascii_case("default") {
                let located = locate(&CString::new(value).unwrap());
                copy_cstr_to_array(&raw mut mods_folder, &located);
            }
            return;
        }
        if proc_bool(name, value, "enable_copyprot", &raw mut enable_copyprot) {
            return;
        }
        if proc_bool(name, value, "enable_music", &raw mut enable_music) {
            return;
        }
        if proc_bool(name, value, "enable_fade", &raw mut enable_fade) {
            return;
        }
        if proc_bool(name, value, "enable_flash", &raw mut enable_flash) {
            return;
        }
        if proc_bool(name, value, "enable_text", &raw mut enable_text) {
            return;
        }
        if proc_bool(name, value, "enable_info_screen", &raw mut enable_info_screen) {
            return;
        }
        if proc_bool(name, value, "start_fullscreen", &raw mut start_fullscreen) {
            return;
        }
        if proc_u16(name, value, "pop_window_width", None, &raw mut pop_window_width) {
            return;
        }
        if proc_u16(name, value, "pop_window_height", None, &raw mut pop_window_height) {
            return;
        }
        if proc_u8(
            name,
            value,
            "use_hardware_acceleration",
            lookup_hardware_accel(value),
            &raw mut use_hardware_acceleration,
        ) {
            return;
        }
        if proc_bool(
            name,
            value,
            "use_correct_aspect_ratio",
            &raw mut use_correct_aspect_ratio,
        ) {
            return;
        }
        if proc_bool(name, value, "use_integer_scaling", &raw mut use_integer_scaling) {
            return;
        }
        if proc_u8(
            name,
            value,
            "scaling_type",
            lookup_scaling_type(value),
            &raw mut scaling_type,
        ) {
            return;
        }
        if proc_bool(
            name,
            value,
            "enable_controller_rumble",
            &raw mut enable_controller_rumble,
        ) {
            return;
        }
        if proc_bool(
            name,
            value,
            "joystick_only_horizontal",
            &raw mut joystick_only_horizontal,
        ) {
            return;
        }
        if proc_i32(name, value, "joystick_threshold", None, &raw mut joystick_threshold) {
            return;
        }
        if name.eq_ignore_ascii_case("levelset") {
            if value.is_empty()
                || value.eq_ignore_ascii_case("original")
                || value.eq_ignore_ascii_case("default")
            {
                use_custom_levelset = 0;
            } else {
                use_custom_levelset = 1;
                let src = CString::new(value).unwrap();
                copy_cstr_to_array(&raw mut levelset_name, &src);
            }
            return;
        }
        if proc_bool(
            name,
            value,
            "always_use_original_music",
            &raw mut always_use_original_music,
        ) {
            return;
        }
        if proc_bool(
            name,
            value,
            "always_use_original_graphics",
            &raw mut always_use_original_graphics,
        ) {
            return;
        }
        if name.eq_ignore_ascii_case("gamecontrollerdb_file") {
            if !value.is_empty() {
                let located = locate(&CString::new(value).unwrap());
                copy_cstr_to_array(&raw mut gamecontrollerdb_file, &located);
            }
            return;
        }
    }

    if section.eq_ignore_ascii_case("AdditionalFeatures") {
        if proc_bool(name, value, "enable_quicksave", &raw mut enable_quicksave) {
            return;
        }
        if proc_bool(
            name,
            value,
            "enable_quicksave_penalty",
            &raw mut enable_quicksave_penalty,
        ) {
            return;
        }
        if proc_bool(name, value, "enable_replay", &raw mut enable_replay) {
            return;
        }
        if name.eq_ignore_ascii_case("replays_folder") {
            if !value.is_empty() && !value.eq_ignore_ascii_case("default") {
                let located = locate(&CString::new(value).unwrap());
                copy_cstr_to_array(&raw mut replays_folder, &located);
            }
            return;
        }
        if proc_bool(name, value, "enable_lighting", &raw mut enable_lighting) {
            return;
        }
    }

    if section.eq_ignore_ascii_case("Enhancements") {
        if name.eq_ignore_ascii_case("use_fixes_and_enhancements") {
            if value.eq_ignore_ascii_case("true") {
                use_fixes_and_enhancements = 1;
            } else if value.eq_ignore_ascii_case("false") {
                use_fixes_and_enhancements = 0;
            } else if value.eq_ignore_ascii_case("prompt") {
                use_fixes_and_enhancements = 2;
            }
            return;
        }
        let f = &raw mut fixes_saved;
        if proc_bool(name, value, "enable_crouch_after_climbing",          &raw mut (*f).enable_crouch_after_climbing)          { return; }
        if proc_bool(name, value, "enable_freeze_time_during_end_music",   &raw mut (*f).enable_freeze_time_during_end_music)   { return; }
        if proc_bool(name, value, "enable_remember_guard_hp",              &raw mut (*f).enable_remember_guard_hp)              { return; }
        if proc_bool(name, value, "fix_gate_sounds",                       &raw mut (*f).fix_gate_sounds)                       { return; }
        if proc_bool(name, value, "fix_two_coll_bug",                      &raw mut (*f).fix_two_coll_bug)                      { return; }
        if proc_bool(name, value, "fix_infinite_down_bug",                 &raw mut (*f).fix_infinite_down_bug)                 { return; }
        if proc_bool(name, value, "fix_gate_drawing_bug",                  &raw mut (*f).fix_gate_drawing_bug)                  { return; }
        if proc_bool(name, value, "fix_bigpillar_climb",                   &raw mut (*f).fix_bigpillar_climb)                   { return; }
        if proc_bool(name, value, "fix_jump_distance_at_edge",             &raw mut (*f).fix_jump_distance_at_edge)             { return; }
        if proc_bool(name, value, "fix_edge_distance_check_when_climbing", &raw mut (*f).fix_edge_distance_check_when_climbing) { return; }
        if proc_bool(name, value, "fix_painless_fall_on_guard",            &raw mut (*f).fix_painless_fall_on_guard)            { return; }
        if proc_bool(name, value, "fix_wall_bump_triggers_tile_below",     &raw mut (*f).fix_wall_bump_triggers_tile_below)     { return; }
        if proc_bool(name, value, "fix_stand_on_thin_air",                 &raw mut (*f).fix_stand_on_thin_air)                 { return; }
        if proc_bool(name, value, "fix_press_through_closed_gates",        &raw mut (*f).fix_press_through_closed_gates)        { return; }
        if proc_bool(name, value, "fix_grab_falling_speed",                &raw mut (*f).fix_grab_falling_speed)                { return; }
        if proc_bool(name, value, "fix_skeleton_chomper_blood",            &raw mut (*f).fix_skeleton_chomper_blood)            { return; }
        if proc_bool(name, value, "fix_move_after_drink",                  &raw mut (*f).fix_move_after_drink)                  { return; }
        if proc_bool(name, value, "fix_loose_left_of_potion",              &raw mut (*f).fix_loose_left_of_potion)              { return; }
        if proc_bool(name, value, "fix_guard_following_through_closed_gates", &raw mut (*f).fix_guard_following_through_closed_gates) { return; }
        if proc_bool(name, value, "fix_safe_landing_on_spikes",            &raw mut (*f).fix_safe_landing_on_spikes)            { return; }
        if proc_bool(name, value, "fix_glide_through_wall",                &raw mut (*f).fix_glide_through_wall)                { return; }
        if proc_bool(name, value, "fix_drop_through_tapestry",             &raw mut (*f).fix_drop_through_tapestry)             { return; }
        if proc_bool(name, value, "fix_land_against_gate_or_tapestry",     &raw mut (*f).fix_land_against_gate_or_tapestry)     { return; }
        if proc_bool(name, value, "fix_unintended_sword_strike",           &raw mut (*f).fix_unintended_sword_strike)           { return; }
        if proc_bool(name, value, "fix_retreat_without_leaving_room",      &raw mut (*f).fix_retreat_without_leaving_room)      { return; }
        if proc_bool(name, value, "fix_running_jump_through_tapestry",     &raw mut (*f).fix_running_jump_through_tapestry)     { return; }
        if proc_bool(name, value, "fix_push_guard_into_wall",              &raw mut (*f).fix_push_guard_into_wall)              { return; }
        if proc_bool(name, value, "fix_jump_through_wall_above_gate",      &raw mut (*f).fix_jump_through_wall_above_gate)      { return; }
        if proc_bool(name, value, "fix_chompers_not_starting",             &raw mut (*f).fix_chompers_not_starting)             { return; }
        if proc_bool(name, value, "fix_feather_interrupted_by_leveldoor",  &raw mut (*f).fix_feather_interrupted_by_leveldoor)  { return; }
        if proc_bool(name, value, "fix_offscreen_guards_disappearing",     &raw mut (*f).fix_offscreen_guards_disappearing)     { return; }
        if proc_bool(name, value, "fix_move_after_sheathe",                &raw mut (*f).fix_move_after_sheathe)                { return; }
        if proc_bool(name, value, "fix_hidden_floors_during_flashing",     &raw mut (*f).fix_hidden_floors_during_flashing)     { return; }
        if proc_bool(name, value, "fix_hang_on_teleport",                  &raw mut (*f).fix_hang_on_teleport)                  { return; }
        if proc_bool(name, value, "fix_exit_door",                         &raw mut (*f).fix_exit_door)                         { return; }
        if proc_bool(name, value, "fix_quicksave_during_feather",          &raw mut (*f).fix_quicksave_during_feather)          { return; }
        if proc_bool(name, value, "fix_caped_prince_sliding_through_gate", &raw mut (*f).fix_caped_prince_sliding_through_gate) { return; }
        if proc_bool(name, value, "fix_doortop_disabling_guard",           &raw mut (*f).fix_doortop_disabling_guard)           { return; }
        if proc_bool(name, value, "enable_super_high_jump",                &raw mut (*f).enable_super_high_jump)                { return; }
        if proc_bool(name, value, "fix_jumping_over_guard",                &raw mut (*f).fix_jumping_over_guard)                { return; }
        if proc_bool(name, value, "fix_drop_2_rooms_climbing_loose_tile",  &raw mut (*f).fix_drop_2_rooms_climbing_loose_tile)  { return; }
        if proc_bool(name, value, "fix_falling_through_floor_during_sword_strike", &raw mut (*f).fix_falling_through_floor_during_sword_strike) { return; }
        if proc_bool(name, value, "enable_jump_grab",                      &raw mut (*f).enable_jump_grab)                      { return; }
        if proc_bool(name, value, "fix_register_quick_input",              &raw mut (*f).fix_register_quick_input)              { return; }
        if proc_bool(name, value, "fix_turn_running_near_wall",            &raw mut (*f).fix_turn_running_near_wall)            { return; }
        if proc_bool(name, value, "fix_feather_fall_affects_guards",       &raw mut (*f).fix_feather_fall_affects_guards)       { return; }
        if proc_bool(name, value, "fix_one_hp_stops_blinking",             &raw mut (*f).fix_one_hp_stops_blinking)             { return; }
        if proc_bool(name, value, "fix_dead_floating_in_air",              &raw mut (*f).fix_dead_floating_in_air)              { return; }
    }

    if section.eq_ignore_ascii_case("CustomGameplay") {
        let c = &raw mut custom_saved;
        if proc_bool(name, value, "use_custom_options", &raw mut use_custom_options) {
            return;
        }
        if proc_u16(name, value, "start_minutes_left", None,                     &raw mut (*c).start_minutes_left)         { return; }
        if proc_u16(name, value, "start_ticks_left",   None,                     &raw mut (*c).start_ticks_left)           { return; }
        if proc_u16(name, value, "start_hitp",         None,                     &raw mut (*c).start_hitp)                 { return; }
        if proc_u16(name, value, "max_hitp_allowed",   None,                     &raw mut (*c).max_hitp_allowed)           { return; }
        if proc_u16(name, value, "saving_allowed_first_level", lookup_never(value), &raw mut (*c).saving_allowed_first_level) { return; }
        if proc_u16(name, value, "saving_allowed_last_level",  lookup_never(value), &raw mut (*c).saving_allowed_last_level)  { return; }
        if proc_bool(name, value, "start_upside_down",   &raw mut (*c).start_upside_down)   { return; }
        if proc_bool(name, value, "start_in_blind_mode", &raw mut (*c).start_in_blind_mode) { return; }
        if proc_u16(name, value, "copyprot_level",       lookup_never(value), &raw mut (*c).copyprot_level) { return; }
        if proc_u8(name, value, "drawn_tile_top_level_edge",  lookup_tile_type(value), &raw mut (*c).drawn_tile_top_level_edge)  { return; }
        if proc_u8(name, value, "drawn_tile_left_level_edge", lookup_tile_type(value), &raw mut (*c).drawn_tile_left_level_edge) { return; }
        if proc_u8(name, value, "level_edge_hit_tile",        lookup_tile_type(value), &raw mut (*c).level_edge_hit_tile)        { return; }
        if proc_bool(name, value, "allow_triggering_any_tile", &raw mut (*c).allow_triggering_any_tile) { return; }
        if proc_bool(name, value, "enable_wda_in_palace",      &raw mut (*c).enable_wda_in_palace)      { return; }

        // vga_color_N: parse "vga_color_N = R, G, B"
        let prefix = "vga_color_";
        if name.len() > prefix.len() && name[..prefix.len()].eq_ignore_ascii_case(prefix) {
            if let Ok(idx) = name[prefix.len()..].parse::<usize>() {
                if idx <= 15 {
                    if !value.eq_ignore_ascii_case("default") {
                        let mut rgb = [0u8; 3];
                        for (i, part) in value.splitn(4, |c| c == ',' || c == ' ').filter(|s| !s.is_empty()).take(3).enumerate() {
                            rgb[i] = part.trim().parse::<u8>().unwrap_or(0);
                        }
                        (*c).vga_palette[idx].r = rgb[0] / 4;
                        (*c).vga_palette[idx].g = rgb[1] / 4;
                        (*c).vga_palette[idx].b = rgb[2] / 4;
                    }
                }
            }
            return;
        }

        if proc_u16(name, value, "first_level",       None,                &raw mut (*c).first_level)       { return; }
        if proc_bool(name, value, "skip_title",        &raw mut (*c).skip_title)                            { return; }
        if proc_u16(name, value, "shift_L_allowed_until_level", lookup_never(value), &raw mut (*c).shift_L_allowed_until_level) { return; }
        if proc_u16(name, value, "shift_L_reduced_minutes",     None, &raw mut (*c).shift_L_reduced_minutes) { return; }
        if proc_u16(name, value, "shift_L_reduced_ticks",       None, &raw mut (*c).shift_L_reduced_ticks)   { return; }
        if proc_u16(name, value, "demo_hitp",          None,                &raw mut (*c).demo_hitp)         { return; }
        if proc_u16(name, value, "demo_end_room",      None,                &raw mut (*c).demo_end_room)     { return; }
        if proc_u16(name, value, "intro_music_level",  lookup_never(value), &raw mut (*c).intro_music_level) { return; }
        if proc_u16(name, value, "have_sword_from_level", lookup_never(value), &raw mut (*c).have_sword_from_level) { return; }
        if proc_u16(name, value, "checkpoint_level",   lookup_never(value), &raw mut (*c).checkpoint_level) { return; }
        if proc_i8(name, value, "checkpoint_respawn_dir",      lookup_direction(value), &raw mut (*c).checkpoint_respawn_dir)      { return; }
        if proc_u8(name, value, "checkpoint_respawn_room",     None,                    &raw mut (*c).checkpoint_respawn_room)     { return; }
        if proc_u8(name, value, "checkpoint_respawn_tilepos",  None,                    &raw mut (*c).checkpoint_respawn_tilepos)  { return; }
        if proc_u8(name, value, "checkpoint_clear_tile_room",  None,                    &raw mut (*c).checkpoint_clear_tile_room)  { return; }
        if proc_u8(name, value, "checkpoint_clear_tile_col",   None,                    &raw mut (*c).checkpoint_clear_tile_col)   { return; }
        if proc_u8(name, value, "checkpoint_clear_tile_row",   lookup_row(value),       &raw mut (*c).checkpoint_clear_tile_row)   { return; }
        if proc_u16(name, value, "skeleton_level",      lookup_never(value), &raw mut (*c).skeleton_level)   { return; }
        if proc_u8(name, value, "skeleton_room",                 None,             &raw mut (*c).skeleton_room)                 { return; }
        if proc_u8(name, value, "skeleton_trigger_column_1",     None,             &raw mut (*c).skeleton_trigger_column_1)     { return; }
        if proc_u8(name, value, "skeleton_trigger_column_2",     None,             &raw mut (*c).skeleton_trigger_column_2)     { return; }
        if proc_u8(name, value, "skeleton_column",               None,             &raw mut (*c).skeleton_column)               { return; }
        if proc_u8(name, value, "skeleton_row",                  lookup_row(value), &raw mut (*c).skeleton_row)                 { return; }
        if proc_bool(name, value, "skeleton_require_open_level_door", &raw mut (*c).skeleton_require_open_level_door)           { return; }
        if proc_u8(name, value, "skeleton_skill",                None,             &raw mut (*c).skeleton_skill)                { return; }
        if proc_u8(name, value, "skeleton_reappear_room",        None,             &raw mut (*c).skeleton_reappear_room)        { return; }
        if proc_u8(name, value, "skeleton_reappear_x",           None,             &raw mut (*c).skeleton_reappear_x)           { return; }
        if proc_u8(name, value, "skeleton_reappear_row",         lookup_row(value), &raw mut (*c).skeleton_reappear_row)        { return; }
        if proc_u8(name, value, "skeleton_reappear_dir",         lookup_direction(value).map(|n| n as i64), &raw mut (*c).skeleton_reappear_dir) { return; }
        if proc_u16(name, value, "mirror_level",        lookup_never(value), &raw mut (*c).mirror_level)     { return; }
        if proc_u8(name, value, "mirror_room",           None,              &raw mut (*c).mirror_room)        { return; }
        if proc_u8(name, value, "mirror_column",         None,              &raw mut (*c).mirror_column)      { return; }
        if proc_u8(name, value, "mirror_row",            lookup_row(value), &raw mut (*c).mirror_row)         { return; }
        if proc_u8(name, value, "mirror_tile",           lookup_tile_type(value), &raw mut (*c).mirror_tile)  { return; }
        if proc_bool(name, value, "show_mirror_image",   &raw mut (*c).show_mirror_image)                     { return; }
        if proc_u8(name, value, "shadow_steal_level",    lookup_never(value).map(|n| n as i64), &raw mut (*c).shadow_steal_level) { return; }
        if proc_u8(name, value, "shadow_steal_room",     None,              &raw mut (*c).shadow_steal_room)  { return; }
        if proc_u8(name, value, "shadow_step_level",     lookup_never(value).map(|n| n as i64), &raw mut (*c).shadow_step_level) { return; }
        if proc_u8(name, value, "shadow_step_room",      None,              &raw mut (*c).shadow_step_room)   { return; }
        if proc_u16(name, value, "falling_exit_level",   lookup_never(value), &raw mut (*c).falling_exit_level)  { return; }
        if proc_u8(name, value, "falling_exit_room",     None,              &raw mut (*c).falling_exit_room)   { return; }
        if proc_u16(name, value, "falling_entry_level",  lookup_never(value), &raw mut (*c).falling_entry_level) { return; }
        if proc_u8(name, value, "falling_entry_room",    None,              &raw mut (*c).falling_entry_room)  { return; }
        if proc_u16(name, value, "mouse_level",          lookup_never(value), &raw mut (*c).mouse_level)       { return; }
        if proc_u8(name, value, "mouse_room",            None,              &raw mut (*c).mouse_room)           { return; }
        if proc_u16(name, value, "mouse_delay",          None,              &raw mut (*c).mouse_delay)          { return; }
        if proc_u8(name, value, "mouse_object",          None,              &raw mut (*c).mouse_object)         { return; }
        if proc_u8(name, value, "mouse_start_x",         None,              &raw mut (*c).mouse_start_x)        { return; }
        if proc_u16(name, value, "loose_tiles_level",    lookup_never(value), &raw mut (*c).loose_tiles_level)  { return; }
        if proc_u8(name, value, "loose_tiles_room_1",    None,              &raw mut (*c).loose_tiles_room_1)   { return; }
        if proc_u8(name, value, "loose_tiles_room_2",    None,              &raw mut (*c).loose_tiles_room_2)   { return; }
        if proc_u8(name, value, "loose_tiles_first_tile", None,             &raw mut (*c).loose_tiles_first_tile) { return; }
        if proc_u8(name, value, "loose_tiles_last_tile",  None,             &raw mut (*c).loose_tiles_last_tile)  { return; }
        if proc_u16(name, value, "jaffar_victory_level",  lookup_never(value), &raw mut (*c).jaffar_victory_level)  { return; }
        if proc_u8(name, value, "jaffar_victory_flash_time", None,          &raw mut (*c).jaffar_victory_flash_time) { return; }
        if proc_u16(name, value, "hide_level_number_from_level", lookup_never(value), &raw mut (*c).hide_level_number_from_level) { return; }
        if proc_u8(name, value, "level_13_level_number", None,              &raw mut (*c).level_13_level_number)  { return; }
        if proc_u16(name, value, "victory_stops_time_level", lookup_never(value), &raw mut (*c).victory_stops_time_level) { return; }
        if proc_u16(name, value, "win_level",            lookup_never(value), &raw mut (*c).win_level)           { return; }
        if proc_u8(name, value, "win_room",              None,              &raw mut (*c).win_room)               { return; }
        if proc_u8(name, value, "loose_floor_delay",     None,              &raw mut (*c).loose_floor_delay)      { return; }
        if proc_u8(name, value, "base_speed",            None,              &raw mut (*c).base_speed)             { return; }
        if proc_u8(name, value, "fight_speed",           None,              &raw mut (*c).fight_speed)            { return; }
        if proc_u8(name, value, "chomper_speed",         None,              &raw mut (*c).chomper_speed)          { return; }
        if proc_bool(name, value, "no_mouse_in_ending",  &raw mut (*c).no_mouse_in_ending)                       { return; }
    }

    // [Level N]
    {
        let sec_lower = section.to_ascii_lowercase();
        if let Some(rest) = sec_lower.strip_prefix("level ") {
            if let Ok(lvl) = rest.trim().parse::<usize>() {
                if lvl <= 15 {
                    let c = &raw mut custom_saved;
                    if proc_u8(name, value, "level_type",  lookup_level_type(value),  &raw mut (*c).tbl_level_type[lvl])  { return; }
                    if proc_u16(name, value, "level_color", None,                      &raw mut (*c).tbl_level_color[lvl]) { return; }
                    if proc_i16(name, value, "guard_type",  lookup_guard_type(value),  &raw mut (*c).tbl_guard_type[lvl])  { return; }
                    if proc_u8(name, value, "guard_hp",     None,                      &raw mut (*c).tbl_guard_hp[lvl])    { return; }
                    if name.eq_ignore_ascii_case("cutscene") {
                        let mut idx: u8 = 0xFF;
                        if proc_u8(name, value, "cutscene", None, &raw mut idx) {
                            if (idx as usize) < (*c).tbl_cutscenes_by_index.len() {
                                (*c).tbl_cutscenes_by_index[lvl] = idx;
                            }
                        }
                        return;
                    }
                    if proc_u8(name, value, "entry_pose",   lookup_entry_pose(value),  &raw mut (*c).tbl_entry_pose[lvl])  { return; }
                    if proc_i8(name, value, "seamless_exit", None,                     &raw mut (*c).tbl_seamless_exit[lvl]) { return; }
                } else {
                    eprintln!("Warning: Invalid section [Level {}] in the INI!", lvl);
                }
            }
        }
    }

    // [Skill N]
    {
        let sec_lower = section.to_ascii_lowercase();
        if let Some(rest) = sec_lower.strip_prefix("skill ") {
            if let Ok(sk) = rest.trim().parse::<usize>() {
                const NUM_GUARD_SKILLS: usize = 12;
                if sk < NUM_GUARD_SKILLS {
                    let c = &raw mut custom_saved;
                    if proc_u16(name, value, "strikeprob",    None, &raw mut (*c).strikeprob[sk])    { return; }
                    if proc_u16(name, value, "restrikeprob",  None, &raw mut (*c).restrikeprob[sk])  { return; }
                    if proc_u16(name, value, "blockprob",     None, &raw mut (*c).blockprob[sk])     { return; }
                    if proc_u16(name, value, "impblockprob",  None, &raw mut (*c).impblockprob[sk])  { return; }
                    if proc_u16(name, value, "advprob",       None, &raw mut (*c).advprob[sk])       { return; }
                    if proc_u16(name, value, "refractimer",   None, &raw mut (*c).refractimer[sk])   { return; }
                    if proc_u16(name, value, "extrastrength",  None, &raw mut (*c).extrastrength[sk]) { return; }
                } else {
                    eprintln!("Warning: Invalid section [Skill {}] in the INI!", sk);
                }
            }
        }
    }
}

unsafe fn mod_ini_callback(section: &str, name: &str, value: &str) {
    let sec = section.to_ascii_lowercase();
    if sec == "enhancements"
        || sec == "customgameplay"
        || sec.starts_with("level ")
        || name.eq_ignore_ascii_case("enable_copyprot")
        || name.eq_ignore_ascii_case("enable_quicksave")
        || name.eq_ignore_ascii_case("enable_quicksave_penalty")
    {
        global_ini_callback(section, name, value);
    }
}

// ── DOS EXE loader ───────────────────────────────────────────────────────────

unsafe fn read_exe_bytes(
    dest: *mut u8,
    nbytes: usize,
    exe_memory: *const u8,
    exe_offset: i32,
    exe_size: i32,
) -> bool {
    if exe_offset < 0 {
        return false;
    }
    if exe_offset < exe_size {
        std::ptr::copy_nonoverlapping(exe_memory.add(exe_offset as usize), dest, nbytes);
    }
    true
}

fn identify_dos_exe_version(filesize: usize) -> i32 {
    match filesize {
        123335 => 0, // dos_10_packed
        129504 => 1, // dos_10_unpacked
        125115 => 2, // dos_13_packed
        129472 => 3, // dos_13_unpacked
        110855 => 4, // dos_14_packed
        115008 => 5, // dos_14_unpacked
        _ => -1,
    }
}

#[allow(unused_assignments)]
unsafe fn load_dos_exe_modifications_inner(folder_name: &str) {
    // Try PRINCE.EXE first, then fall back to any .exe with a matching size.
    let prince_path = format!("{}/PRINCE.EXE", folder_name);
    let (exe_bytes, dos_version) = 'find: {
        if let Ok(bytes) = std::fs::read(&prince_path) {
            let ver = identify_dos_exe_version(bytes.len());
            if ver >= 0 {
                break 'find (bytes, ver);
            }
        }
        // Search for other .exe files.
        let folder_c = CString::new(folder_name).unwrap();
        let ext_c = c"exe";
        let listing =
            create_directory_listing_and_find_first_file(folder_c.as_ptr(), ext_c.as_ptr());
        if listing.is_null() {
            return;
        }
        let mut found: Option<(Vec<u8>, i32)> = None;
        loop {
            let fname_ptr = get_current_filename_from_directory_listing(listing);
            if !fname_ptr.is_null() {
                let fname = CStr::from_ptr(fname_ptr).to_string_lossy();
                let path = format!("{}/{}", folder_name, fname);
                if let Ok(bytes) = std::fs::read(&path) {
                    let ver = identify_dos_exe_version(bytes.len());
                    if ver >= 0 {
                        found = Some((bytes, ver));
                        break;
                    }
                }
            }
            if !find_next_file(listing) {
                break;
            }
        }
        close_directory_listing(listing);
        match found {
            Some(pair) => break 'find pair,
            None => return,
        }
    };

    turn_custom_options_on_off(1);

    let exe_memory = exe_bytes.as_ptr();
    let exe_size = exe_bytes.len() as i32;
    let v = dos_version as usize;
    let mut read_ok: bool;

    macro_rules! process {
        ($dest:expr, $nbytes:expr, $offsets:expr) => {{
            let offsets: [i32; 6] = $offsets;
            read_ok = read_exe_bytes($dest as *mut u8, $nbytes, exe_memory, offsets[v], exe_size);
        }};
    }

    let c = &raw mut custom_saved;
    let mut temp_bytes = [0u8; 64];
    let mut temp_word: u16 = 0;

    process!(&raw mut (*c).start_minutes_left, 2, [0x04a23, 0x060d3, 0x04ea3, 0x055e3, 0x0495f, 0x05a8f]);
    process!(&raw mut (*c).start_ticks_left,   2, [0x04a29, 0x060d9, 0x04ea9, 0x055e9, 0x04965, 0x05a95]);
    process!(&raw mut (*c).start_hitp,         2, [0x04a2f, 0x060df, 0x04eaf, 0x055ef, 0x0496b, 0x05a9b]);
    process!(&raw mut (*c).first_level,        2, [0x00707, 0x01db7, 0x007db, 0x00f1b, 0x0079f, 0x018cf]);
    process!(&raw mut (*c).max_hitp_allowed,   2, [0x013f1, 0x02aa1, 0x015ac, 0x01cec, 0x014a3, 0x025d3]);
    process!(&raw mut (*c).saving_allowed_first_level, 1, [0x007c8, 0x01e78, 0x008b4, 0x00ff4, 0x00878, 0x019a8]);
    if read_ok { (*c).saving_allowed_first_level += 1; }
    process!(&raw mut (*c).saving_allowed_last_level, 1, [0x007cf, 0x01e7f, 0x008bb, 0x00ffb, 0x0087f, 0x019af]);
    if read_ok { (*c).saving_allowed_last_level -= 1; }
    if v == 0 || v == 1 {
        // dos_10_packed or dos_10_unpacked
        const CMP: &[u8] = &[
            0xa3, 0x92, 0x4e, 0xa3, 0x5c, 0x40, 0xa3, 0x8e, 0x4e, 0xa2, 0x2a,
            0x3d, 0xa2, 0x29, 0x3d, 0xa3, 0xee, 0x42, 0xa2, 0x2e, 0x3d, 0x98,
        ];
        process!(temp_bytes.as_mut_ptr(), CMP.len(), [0x04c9b, 0x0634b, -1, -1, -1, -1]);
        (*c).start_upside_down = (&temp_bytes[..CMP.len()] != CMP) as u8;
    }
    process!(&raw mut (*c).start_in_blind_mode, 1, [0x04e46, 0x064f6, 0x052ce, 0x05a0e, 0x04d8a, 0x05eba]);
    process!(&raw mut (*c).copyprot_level,       2, [0x1aaeb, 0x1c62e, 0x1b89b, 0x1c49e, 0x17c3d, 0x18e18]);
    process!(&raw mut (*c).drawn_tile_top_level_edge,  1, [0x0a1f0, 0x0b8a0, 0x0a69c, 0x0addc, 0x0a158, 0x0b288]);
    process!(&raw mut (*c).drawn_tile_left_level_edge, 1, [0x0a26b, 0x0b91b, -1, -1, -1, -1]);
    process!(&raw mut (*c).level_edge_hit_tile,        1, [0x06f02, 0x085b2, -1, -1, -1, -1]);
    process!(temp_bytes.as_mut_ptr(), 2, [0x9111, 0xA7C1, 0x95BE, 0x9CFE, 0x907A, 0xA1AA]);
    if read_ok {
        (*c).allow_triggering_any_tile =
            ((temp_bytes[0] == 0x75 && temp_bytes[1] == 0x13)
                || (temp_bytes[0] == 0x90 && temp_bytes[1] == 0x90)) as u8;
    }
    process!(temp_bytes.as_mut_ptr(), 1, [0x0a7bb, 0x0be6b, 0x0ac67, 0x0b3a7, 0x0a723, 0x0b853]);
    if read_ok { (*c).enable_wda_in_palace = (temp_bytes[0] != 116) as u8; }
    process!(&raw mut (*c).tbl_level_type, 16,         [0x1acea, 0x1c842, 0x1b9ae, 0x1c5c6, 0x17d4c, 0x18f3c]);
    process!(&raw mut (*c).tbl_guard_hp,   16,         [0x1b8a8, 0x1d46a, 0x1c6c5, 0x1d35c, 0x18a97, 0x19d06]);
    process!(&raw mut (*c).tbl_guard_type, 2 * 16,     [-1, 0x1c964, -1, 0x1c702, -1, 0x1905e]);
    process!(&raw mut (*c).vga_palette,    3 * 16,     [0x1d141, 0x1f136, 0x1df5e, 0x1f02a, 0x1a335, 0x1b9de]);
    process!(&raw mut temp_word, 2, [0x003e2, 0x01a92, 0x0046b, 0x00bab, 0x00455, 0x01585]);
    if read_ok { (*c).skip_title = (temp_word != 63558) as u8; }
    process!(&raw mut (*c).shift_L_allowed_until_level, 1, [0x0085c, 0x01f0c, 0x00955, 0x01095, 0x00919, 0x01a49]);
    if read_ok { (*c).shift_L_allowed_until_level += 1; }
    process!(&raw mut (*c).shift_L_reduced_minutes, 2, [0x008ad, 0x01f5d, 0x00991, 0x010d1, 0x00955, 0x01a85]);
    process!(&raw mut (*c).shift_L_reduced_ticks,   2, [0x008b3, 0x01f63, 0x00997, 0x010d7, 0x0095b, 0x01a8b]);
    process!(&raw mut (*c).demo_hitp,     1, [0x04c28, 0x062d8, 0x050b0, 0x057f0, 0x04b6c, 0x05c9c]);
    process!(&raw mut (*c).demo_end_room, 1, [0x00b40, 0x021f0, 0x00c25, 0x01365, 0x00be9, 0x01d19]);
    process!(&raw mut (*c).intro_music_level, 1, [0x04c37, 0x062e7, 0x050bf, 0x057ff, 0x04b7b, 0x05cab]);
    process!(temp_bytes.as_mut_ptr(), 1, [0x04b29, 0x061d9, 0x04fa9, 0x056e9, 0x04a65, 0x05b95]);
    if read_ok { (*c).have_sword_from_level = if temp_bytes[0] == 0xEB { 16 } else { 2 }; }
    process!(&raw mut (*c).checkpoint_level,          1, [0x04b9e, 0x0624e, 0x05026, 0x05766, 0x04ae2, 0x05c12]);
    process!(&raw mut (*c).checkpoint_respawn_dir,    1, [0x04bac, 0x0625c, 0x05034, 0x05774, 0x04af0, 0x05c20]);
    process!(&raw mut (*c).checkpoint_respawn_room,   1, [0x04bb1, 0x06261, 0x05039, 0x05779, 0x04af5, 0x05c25]);
    process!(&raw mut (*c).checkpoint_respawn_tilepos,1, [0x04bb6, 0x06266, 0x0503e, 0x0577e, 0x04afa, 0x05c2a]);
    process!(&raw mut (*c).checkpoint_clear_tile_room,1, [0x04bb8, 0x06268, 0x05040, 0x05780, 0x04afc, 0x05c2c]);
    process!(&raw mut (*c).checkpoint_clear_tile_col, 1, [0x04bbc, 0x0626c, 0x05044, 0x05784, 0x04b00, 0x05c30]);
    process!(&raw mut temp_word, 2, [0x04bbf, 0x0626f, 0x05047, 0x05787, 0x04b03, 0x05c33]);
    if read_ok {
        (*c).checkpoint_clear_tile_row = match temp_word {
            49195 => 0,
            432   => 1,
            688   => 2,
            _     => (*c).checkpoint_clear_tile_row,
        };
    }
    process!(&raw mut (*c).skeleton_level,            1, [0x046a4, 0x05d54, -1, -1, -1, -1]);
    process!(&raw mut (*c).skeleton_room,             1, [0x046b8, 0x05d68, -1, -1, -1, -1]);
    process!(&raw mut (*c).skeleton_trigger_column_1, 1, [0x046cc, 0x05d7c, -1, -1, -1, -1]);
    process!(&raw mut (*c).skeleton_trigger_column_2, 1, [0x046d3, 0x05d83, -1, -1, -1, -1]);
    process!(&raw mut (*c).skeleton_column,           1, [0x046de, 0x05d8e, 0x04b5e, 0x052a2, 0x0461a, 0x0574a]);
    process!(&raw mut (*c).skeleton_row,              1, [0x046e2, 0x05d92, 0x04b62, 0x052a2, 0x0461e, 0x0574e]);
    process!(temp_bytes.as_mut_ptr(), 1, [0x046c3, 0x05d73, -1, -1, -1, -1]);
    if read_ok { (*c).skeleton_require_open_level_door = (temp_bytes[0] != 0xEB) as u8; }
    process!(&raw mut (*c).skeleton_skill,           1, [0x0478f, 0x05e3f, -1, -1, -1, -1]);
    process!(&raw mut (*c).skeleton_reappear_room,   1, [0x03b32, 0x051e2, 0x03fb2, 0x046f2, 0x03a6e, 0x04b9e]);
    process!(&raw mut (*c).skeleton_reappear_x,      1, [0x03b39, 0x051e9, -1, -1, -1, -1]);
    process!(&raw mut (*c).skeleton_reappear_row,    1, [0x03b3e, 0x051ee, -1, -1, -1, -1]);
    process!(&raw mut (*c).skeleton_reappear_dir,    1, [0x03b43, 0x051f3, -1, -1, -1, -1]);
    process!(&raw mut (*c).mirror_level,  1, [0x08dc7, 0x0a477, 0x09274, 0x099b4, 0x08d30, 0x09e60]);
    process!(&raw mut (*c).mirror_room,   1, [0x08dcb, 0x0a47b, 0x09278, 0x099b8, 0x08d34, 0x09e64]);
    if read_ok {
        let mut opcode: u8 = 0;
        process!(&raw mut opcode, 1, [0x08dcd, 0x0a47d, 0x0927a, 0x099ba, 0x08d36, 0x09e66]);
        if opcode == 0x50 {
            (*c).mirror_column = (*c).mirror_room;
        } else if opcode == 0x6A {
            process!(&raw mut (*c).mirror_column, 1, [0x08dce, 0x0a47e, 0x0927b, 0x099bb, 0x08d37, 0x09e67]);
        }
    }
    process!(&raw mut temp_word, 2, [0x08dcf, 0x0a47f, 0x0927c, 0x099bc, 0x08d38, 0x09e68]);
    if read_ok {
        (*c).mirror_row = match temp_word {
            0xC02B => 0,
            0x01B0 => 1,
            0x02B0 => 2,
            _      => (*c).mirror_row,
        };
    }
    process!(&raw mut (*c).mirror_tile, 1, [0x08de3, 0x0a493, 0x09290, 0x099d0, 0x08d4c, 0x09e7c]);
    process!(temp_bytes.as_mut_ptr(), 1, [0x051a2, 0x06852, 0x05636, 0x05d76, 0x050f2, 0x06222]);
    if read_ok { (*c).show_mirror_image = (temp_bytes[0] != 0xEB) as u8; }
    process!(&raw mut (*c).shadow_steal_level, 1, [-1, 0x5017, -1, -1, -1, -1]);
    process!(&raw mut (*c).shadow_steal_room,  1, [-1, 0x5021, -1, -1, -1, -1]);
    process!(&raw mut (*c).shadow_step_level,  1, [-1, 0x4FE7, -1, -1, -1, -1]);
    process!(&raw mut (*c).shadow_step_room,   1, [-1, 0x4FF1, -1, -1, -1, -1]);
    process!(&raw mut (*c).falling_exit_level, 1, [0x03eb2, 0x05562, -1, -1, -1, -1]);
    process!(&raw mut (*c).falling_exit_room,  1, [0x03eb9, 0x05569, -1, -1, -1, -1]);
    process!(&raw mut (*c).falling_entry_level,1, [0x04cbd, 0x0636d, -1, -1, -1, -1]);
    process!(&raw mut (*c).falling_entry_room, 1, [0x04cc4, 0x06374, -1, -1, -1, -1]);
    process!(&raw mut (*c).mouse_level,   1, [0x05166, 0x06816, 0x055fa, 0x05d3a, 0x050b6, 0x061e6]);
    process!(&raw mut (*c).mouse_room,    1, [0x0516d, 0x0681d, 0x05601, 0x05d41, 0x050bd, 0x061ed]);
    process!(&raw mut (*c).mouse_delay,   2, [0x0517f, 0x0682f, 0x05613, 0x05d53, 0x050cf, 0x061ff]);
    process!(&raw mut (*c).mouse_object,  1, [0x054b3, 0x06b63, 0x05947, 0x06087, 0x05403, 0x06533]);
    process!(&raw mut (*c).mouse_start_x, 1, [0x054b8, 0x06b68, 0x0594c, 0x0608c, 0x05408, 0x06538]);
    {
        let mut exit_level: u8 = 0;
        let mut exit_room:  u8 = 0;
        process!(&raw mut exit_level, 1, [0x00b84, 0x02234, 0x00c6d, 0x013ad, 0x00c31, 0x01d61]);
        if read_ok {
            process!(&raw mut exit_room, 1, [0x00b8b, 0x0223b, 0x00c74, 0x013b4, 0x00c38, 0x01d68]);
        }
        if read_ok && (exit_level as usize) < 16 {
            std::ptr::write_bytes((*c).tbl_seamless_exit.as_mut_ptr(), 0xFF, 16);
            (*c).tbl_seamless_exit[exit_level as usize] = exit_room as i8;
        }
    }
    process!(&raw mut (*c).loose_tiles_level,      1, [0x0120d, 0x028bd, -1, -1, 0x01358, 0x02488]);
    process!(&raw mut (*c).loose_tiles_room_1,     1, [0x01214, 0x028c4, -1, -1, 0x0135f, 0x0248f]);
    process!(&raw mut (*c).loose_tiles_room_2,     1, [0x0121b, 0x028cb, -1, -1, 0x01366, 0x02496]);
    process!(&raw mut (*c).loose_tiles_first_tile, 1, [0x0122e, 0x028de, -1, -1, 0x01379, 0x024a9]);
    process!(&raw mut (*c).loose_tiles_last_tile,  1, [0x0124d, 0x028fd, -1, -1, 0x01398, 0x024c8]);
    process!(&raw mut (*c).jaffar_victory_level,      1, [0x084b3, 0x09b63, 0x08963, 0x090a3, 0x0841f, 0x0954f]);
    process!(&raw mut (*c).jaffar_victory_flash_time, 2, [0x084c0, 0x09b70, 0x08970, 0x090b0, 0x0842c, 0x0955c]);
    process!(&raw mut (*c).hide_level_number_from_level, 2, [0x0c3d9, 0x0da89, 0x0c8cd, 0x0d00d, 0x0c389, 0x0d4b9]);
    process!(temp_bytes.as_mut_ptr(), 1, [0x0c3d9, 0x0da89, 0x0c8cd, 0x0d00d, 0x0c389, 0x0d4b9]);
    if read_ok { (*c).level_13_level_number = if temp_bytes[0] == 0xEB { 13 } else { 12 }; }
    process!(&raw mut (*c).victory_stops_time_level, 1, [0x0c2e0, 0x0d990, -1, -1, -1, -1]);
    process!(&raw mut (*c).win_level, 1, [0x011dc, 0x0288c, 0x01397, 0x01ad7, 0x01327, 0x02457]);
    process!(&raw mut (*c).win_room,  1, [0x011e3, 0x02893, 0x0139e, 0x01ade, 0x0132e, 0x0245e]);
    process!(&raw mut (*c).loose_floor_delay, 1, [0x9536, 0xABE6, -1, -1, -1, -1]);

    process!(&raw mut (*c).strikeprob,    2 * 12, [-1, 0x1D3C2, -1, 0x1D2B4, -1, 0x19C5E]);
    process!(&raw mut (*c).restrikeprob,  2 * 12, [-1, 0x1D3DA, -1, 0x1D2CC, -1, 0x19C76]);
    process!(&raw mut (*c).blockprob,     2 * 12, [-1, 0x1D3F2, -1, 0x1D2E4, -1, 0x19C8E]);
    process!(&raw mut (*c).impblockprob,  2 * 12, [-1, 0x1D40A, -1, 0x1D2FC, -1, 0x19CA6]);
    process!(&raw mut (*c).advprob,       2 * 12, [-1, 0x1D422, -1, 0x1D314, -1, 0x19CBE]);
    process!(&raw mut (*c).refractimer,   2 * 12, [-1, 0x1D43A, -1, 0x1D32C, -1, 0x19CD6]);
    process!(&raw mut (*c).extrastrength, 2 * 12, [-1, 0x1D452, -1, 0x1D344, -1, 0x19CEE]);

    process!(&raw mut (*c).init_shad_6,    8, [0x1B8B8, 0x1D47A, 0x1C6D5, 0x1D36C, 0x18AA7, 0x19D16]);
    process!(&raw mut (*c).init_shad_5,    8, [0x1B8C0, 0x1D482, 0x1C6DD, 0x1D374, 0x18AAF, 0x19D1E]);
    process!(&raw mut (*c).init_shad_12,   8, [-1, 0x1D48A, -1, 0x1D37C, -1, 0x19D26]);
    process!(&raw mut (*c).shad_drink_move, 8 * 4, [-1, 0x1D492, -1, 0x1D384, -1, 0x19D2E]);
    process!(&raw mut (*c).demo_moves,     25 * 4, [0x1B8EE, 0x1D4B2, 0x1C70B, 0x1D3A4, 0x18ADD, 0x19D4E]);

    process!(&raw mut (*c).base_speed,    1, [0x4F01, 0x65B1, 0x5389, 0x5AC9, 0x4E45, 0x5F75]);
    process!(&raw mut (*c).fight_speed,   1, [0x4EF9, 0x65A9, 0x5381, 0x5AC1, 0x4E3D, 0x5F6D]);
    process!(&raw mut (*c).chomper_speed, 1, [0x8BBD, 0xA26D, 0x906D, 0x97AD, 0x8B29, 0x9C59]);

    process!(temp_bytes.as_mut_ptr(), 2, [0x2B8C, 0x423C, 0x2FE4, 0x3724, 0x2B28, 0x3C58]);
    (*c).no_mouse_in_ending = (temp_bytes[0] == 0xEB && temp_bytes[1] == 0x27) as u8;
}

// ── exported functions ───────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn turn_fixes_and_enhancements_on_off(new_state: u8) {
    use_fixes_and_enhancements = new_state;
    fixes = if new_state != 0 {
        &raw mut fixes_saved
    } else {
        &raw mut fixes_disabled_state
    };
}

#[no_mangle]
pub unsafe extern "C" fn turn_custom_options_on_off(new_state: u8) {
    use_custom_options = new_state;
    custom = if new_state != 0 {
        &raw mut custom_saved
    } else {
        &raw mut custom_defaults
    };
}

#[no_mangle]
pub unsafe extern "C" fn set_options_to_default() {
    enable_pause_menu = 1;
    enable_copyprot = 0;
    enable_music = 1;
    enable_fade = 1;
    enable_flash = 1;
    enable_text = 1;
    enable_info_screen = 1;
    start_fullscreen = 0;
    use_hardware_acceleration = 2;
    use_correct_aspect_ratio = 0;
    use_integer_scaling = 0;
    scaling_type = 0;
    enable_controller_rumble = 1;
    joystick_only_horizontal = 1;
    joystick_threshold = 8000;
    enable_quicksave = 1;
    enable_quicksave_penalty = 1;
    enable_replay = 1;
    enable_lighting = 0;
    // All fixes default to true; memset to 1 covers any future fields automatically.
    std::ptr::write_bytes(&raw mut fixes_saved, 1, 1);
    // Copy the default custom options struct.
    std::ptr::copy_nonoverlapping(&raw const custom_defaults, &raw mut custom_saved, 1);
    turn_fixes_and_enhancements_on_off(0);
    turn_custom_options_on_off(0);
}

#[no_mangle]
pub unsafe extern "C" fn load_dos_exe_modifications(folder_name: *const c_char) {
    let folder = CStr::from_ptr(folder_name).to_string_lossy();
    load_dos_exe_modifications_inner(&folder);
}

#[no_mangle]
pub unsafe extern "C" fn check_mod_param() {
    let param = check_param(c"mod".as_ptr());
    if !param.is_null() {
        use_custom_levelset = 1;
        *(&raw mut levelset_name) = std::mem::zeroed();
        let src = CStr::from_ptr(param);
        copy_cstr_to_array(&raw mut levelset_name, src);
    }
}

#[no_mangle]
pub unsafe extern "C" fn load_global_options() {
    set_options_to_default();
    let ini_path = locate(c"SDLPoP.ini");
    ini_load(
        Path::new(ini_path.to_str().unwrap_or("")),
        |sec, name, val| global_ini_callback(sec, name, val),
    );
    load_dos_exe_modifications(c".".as_ptr());
}

#[no_mangle]
pub unsafe extern "C" fn load_mod_options() {
    if use_custom_levelset == 0 {
        turn_fixes_and_enhancements_on_off(use_fixes_and_enhancements);
        turn_custom_options_on_off(use_custom_options);
        return;
    }

    let levelset = CStr::from_ptr((&raw const levelset_name) as *const c_char)
        .to_string_lossy()
        .into_owned();
    let mods = CStr::from_ptr((&raw const mods_folder) as *const c_char)
        .to_string_lossy()
        .into_owned();
    let folder = format!("{}/{}", mods, levelset);
    let folder_c = CString::new(folder.clone()).unwrap();
    let located_c = locate(&folder_c);
    let located = located_c.to_string_lossy();

    let meta = std::fs::metadata(located.as_ref());
    let is_dir = meta.map(|m| m.is_dir()).unwrap_or(false);

    if is_dir {
        copy_cstr_to_array(&raw mut mod_data_path, &located_c);
        load_dos_exe_modifications(located_c.as_ptr());

        let mod_ini = format!("{}/mod.ini", located);
        let mod_ini_c = CString::new(mod_ini.clone()).unwrap();
        if file_exists(mod_ini_c.as_ptr()) {
            use_custom_options = 1;
            ini_load(
                Path::new(&mod_ini),
                |sec, name, val| mod_ini_callback(sec, name, val),
            );
        }
    } else {
        if std::fs::metadata(located.as_ref()).is_err() {
            eprintln!("Mod '{}' not found", levelset);
            let msg = CString::new(format!(
                "Cannot find the mod '{}' in the mods folder.",
                levelset
            ))
            .unwrap();
            show_dialog(msg.as_ptr());
            if replaying != 0 {
                show_dialog(
                    c"If the replay file restarts the level or advances to the next level, a wrong level will be loaded.".as_ptr(),
                );
            }
        } else {
            eprintln!("Could not load mod '{}' - not a directory", levelset);
        }
        use_custom_levelset = 0;
        *(&raw mut levelset_name) = std::mem::zeroed();
    }

    turn_fixes_and_enhancements_on_off(use_fixes_and_enhancements);
    turn_custom_options_on_off(use_custom_options);
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(static_mut_refs)]
mod tests {
    use super::*;
    
    unsafe fn setup() {
        set_options_to_default();
    }

    // ── identify_dos_exe_version ─────────────────────────────────────────────

    #[test]
    fn exe_version_all_known_sizes() {
        assert_eq!(identify_dos_exe_version(123335), 0); // dos_10_packed
        assert_eq!(identify_dos_exe_version(129504), 1); // dos_10_unpacked
        assert_eq!(identify_dos_exe_version(125115), 2); // dos_13_packed
        assert_eq!(identify_dos_exe_version(129472), 3); // dos_13_unpacked
        assert_eq!(identify_dos_exe_version(110855), 4); // dos_14_packed
        assert_eq!(identify_dos_exe_version(115008), 5); // dos_14_unpacked
    }

    #[test]
    fn exe_version_unknown_sizes() {
        assert_eq!(identify_dos_exe_version(0),          -1);
        assert_eq!(identify_dos_exe_version(123334),     -1); // one byte under dos_10_packed
        assert_eq!(identify_dos_exe_version(123336),     -1); // one byte over
        assert_eq!(identify_dos_exe_version(usize::MAX), -1);
    }

    // ── ini_load parser ──────────────────────────────────────────────────────

    fn parse_ini(content: &str) -> Vec<(String, String, String)> {
        let mut calls = Vec::new();
        ini_parse_str(content, |sec, name, val| {
            calls.push((sec.to_owned(), name.to_owned(), val.to_owned()));
        });
        calls
    }

    #[test]
    fn ini_empty_file() {
        assert!(parse_ini("").is_empty());
    }

    #[test]
    fn ini_comment_only() {
        assert!(parse_ini("; just a comment\n").is_empty());
    }

    #[test]
    fn ini_section_no_keys() {
        assert!(parse_ini("[MySection]\n").is_empty());
    }

    #[test]
    fn ini_basic_key_value() {
        let calls = parse_ini("[MySection]\nkey = val\n");
        assert_eq!(calls, [("MySection".into(), "key".into(), "val".into())]);
    }

    #[test]
    fn ini_multiple_sections() {
        let calls = parse_ini("[A]\nk=v\n[B]\nk=w\n");
        assert_eq!(calls[0], ("A".into(), "k".into(), "v".into()));
        assert_eq!(calls[1], ("B".into(), "k".into(), "w".into()));
    }

    #[test]
    fn ini_inline_comment_on_section() {
        let calls = parse_ini("[Sec] ; comment\nk=v\n");
        assert_eq!(calls[0].0, "Sec");
    }

    #[test]
    fn ini_inline_comment_on_value() {
        let calls = parse_ini("k = v ; comment\n");
        assert_eq!(calls[0], ("".into(), "k".into(), "v".into()));
    }

    #[test]
    fn ini_whitespace_trimmed() {
        let calls = parse_ini("  key  =  value  \n");
        assert_eq!(calls[0], ("".into(), "key".into(), "value".into()));
    }

    #[test]
    fn ini_empty_value() {
        let calls = parse_ini("key =\n");
        assert_eq!(calls[0], ("".into(), "key".into(), "".into()));
    }

    #[test]
    fn ini_no_equals() {
        let calls = parse_ini("key\n");
        assert_eq!(calls[0], ("".into(), "key".into(), "".into()));
    }

    #[test]
    fn ini_repeated_key() {
        let calls = parse_ini("key = v1\nkey = v2\n");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].2, "v1");
        assert_eq!(calls[1].2, "v2");
    }

    // ── global_ini_callback — boolean options ────────────────────────────────

    #[test]
    fn callback_bool_false() {
        unsafe {
            setup();
            global_ini_callback("General", "enable_music", "false");
            assert_eq!(enable_music, 0);
        }
    }

    #[test]
    fn callback_bool_true() {
        unsafe {
            setup();
            enable_music = 0;
            global_ini_callback("General", "enable_music", "true");
            assert_eq!(enable_music, 1);
        }
    }

    #[test]
    fn callback_bool_case_insensitive() {
        unsafe {
            setup();
            global_ini_callback("General", "enable_music", "False");
            assert_eq!(enable_music, 0);
            global_ini_callback("General", "enable_music", "TRUE");
            assert_eq!(enable_music, 1);
        }
    }

    #[test]
    fn callback_bool_invalid_value_no_write() {
        unsafe {
            setup(); // enable_music = 1
            global_ini_callback("General", "enable_music", "yes");
            assert_eq!(enable_music, 1); // unchanged
        }
    }

    #[test]
    fn callback_start_fullscreen_changes_from_default() {
        unsafe {
            setup(); // start_fullscreen = 0
            global_ini_callback("General", "start_fullscreen", "true");
            assert_eq!(start_fullscreen, 1);
        }
    }

    // ── global_ini_callback — use_fixes_and_enhancements special case ────────

    #[test]
    fn callback_fixes_true() {
        unsafe {
            setup();
            global_ini_callback("Enhancements", "use_fixes_and_enhancements", "true");
            assert_eq!(use_fixes_and_enhancements, 1);
        }
    }

    #[test]
    fn callback_fixes_false() {
        unsafe {
            setup();
            global_ini_callback("Enhancements", "use_fixes_and_enhancements", "false");
            assert_eq!(use_fixes_and_enhancements, 0);
        }
    }

    #[test]
    fn callback_fixes_prompt() {
        unsafe {
            setup();
            global_ini_callback("Enhancements", "use_fixes_and_enhancements", "prompt");
            assert_eq!(use_fixes_and_enhancements, 2);
        }
    }

    #[test]
    fn callback_fixes_prompt_case_insensitive() {
        unsafe {
            setup();
            global_ini_callback("Enhancements", "use_fixes_and_enhancements", "PROMPT");
            assert_eq!(use_fixes_and_enhancements, 2);
        }
    }

    // ── global_ini_callback — numeric and named-value options ────────────────

    #[test]
    fn callback_numeric_decimal() {
        unsafe {
            setup();
            global_ini_callback("General", "pop_window_width", "800");
            assert_eq!(pop_window_width, 800);
        }
    }

    #[test]
    fn callback_numeric_hex() {
        unsafe {
            setup();
            global_ini_callback("General", "pop_window_width", "0x320");
            assert_eq!(pop_window_width, 800);
        }
    }

    #[test]
    fn callback_named_scaling_type() {
        unsafe {
            setup();
            global_ini_callback("General", "scaling_type", "sharp");
            assert_eq!(scaling_type, 0);
            global_ini_callback("General", "scaling_type", "fuzzy");
            assert_eq!(scaling_type, 1);
            global_ini_callback("General", "scaling_type", "blurry");
            assert_eq!(scaling_type, 2);
        }
    }

    #[test]
    fn callback_named_scaling_type_case_insensitive() {
        unsafe {
            setup();
            global_ini_callback("General", "scaling_type", "SHARP");
            assert_eq!(scaling_type, 0);
        }
    }

    #[test]
    fn callback_default_value_no_write() {
        unsafe {
            setup();
            let before = scaling_type;
            global_ini_callback("General", "scaling_type", "default");
            assert_eq!(scaling_type, before);
        }
    }

    #[test]
    fn callback_custom_start_minutes() {
        unsafe {
            setup();
            global_ini_callback("CustomGameplay", "start_minutes_left", "60");
            let v = std::ptr::read_unaligned(&raw const custom_saved.start_minutes_left);
            assert_eq!(v, 60);
        }
    }

    // ── global_ini_callback — vga_color_N RGB parsing ────────────────────────

    #[test]
    fn callback_vga_color_max() {
        unsafe {
            setup();
            global_ini_callback("CustomGameplay", "vga_color_0", "255, 255, 255");
            assert_eq!(custom_saved.vga_palette[0].r, 63);
            assert_eq!(custom_saved.vga_palette[0].g, 63);
            assert_eq!(custom_saved.vga_palette[0].b, 63);
        }
    }

    #[test]
    fn callback_vga_color_zero() {
        unsafe {
            setup();
            global_ini_callback("CustomGameplay", "vga_color_0", "0, 0, 0");
            assert_eq!(custom_saved.vga_palette[0].r, 0);
            assert_eq!(custom_saved.vga_palette[0].g, 0);
            assert_eq!(custom_saved.vga_palette[0].b, 0);
        }
    }

    #[test]
    fn callback_vga_color_division() {
        unsafe {
            setup();
            global_ini_callback("CustomGameplay", "vga_color_0", "4, 8, 12");
            assert_eq!(custom_saved.vga_palette[0].r, 1);
            assert_eq!(custom_saved.vga_palette[0].g, 2);
            assert_eq!(custom_saved.vga_palette[0].b, 3);
        }
    }

    #[test]
    fn callback_vga_color_default_no_write() {
        unsafe {
            setup();
            let before = custom_saved.vga_palette[0];
            global_ini_callback("CustomGameplay", "vga_color_0", "default");
            assert_eq!(custom_saved.vga_palette[0].r, before.r);
            assert_eq!(custom_saved.vga_palette[0].g, before.g);
            assert_eq!(custom_saved.vga_palette[0].b, before.b);
        }
    }

    #[test]
    fn callback_vga_color_last_valid_index() {
        unsafe {
            setup();
            global_ini_callback("CustomGameplay", "vga_color_15", "255, 0, 0");
            assert_eq!(custom_saved.vga_palette[15].r, 63);
        }
    }

    #[test]
    fn callback_vga_color_out_of_range_ignored() {
        unsafe {
            setup();
            let before = custom_saved.vga_palette[0];
            global_ini_callback("CustomGameplay", "vga_color_16", "255, 0, 0");
            assert_eq!(custom_saved.vga_palette[0].r, before.r); // unchanged
        }
    }

    // ── global_ini_callback — [Level N] section ──────────────────────────────

    #[test]
    fn callback_level_type() {
        unsafe {
            setup();
            global_ini_callback("Level 0", "level_type", "dungeon");
            assert_eq!(custom_saved.tbl_level_type[0], 0);
            global_ini_callback("Level 0", "level_type", "palace");
            assert_eq!(custom_saved.tbl_level_type[0], 1);
        }
    }

    #[test]
    fn callback_level_type_case_insensitive() {
        unsafe {
            setup();
            global_ini_callback("Level 0", "level_type", "PALACE");
            assert_eq!(custom_saved.tbl_level_type[0], 1);
        }
    }

    #[test]
    fn callback_level_last_index() {
        unsafe {
            setup();
            global_ini_callback("Level 15", "guard_hp", "3");
            assert_eq!(custom_saved.tbl_guard_hp[15], 3);
        }
    }

    #[test]
    fn callback_level_out_of_range_ignored() {
        unsafe {
            setup();
            let before = custom_saved.tbl_guard_hp[0];
            global_ini_callback("Level 16", "guard_hp", "3");
            assert_eq!(custom_saved.tbl_guard_hp[0], before);
        }
    }

    // ── global_ini_callback — [Skill N] section ──────────────────────────────

    #[test]
    fn callback_skill_strikeprob() {
        unsafe {
            setup();
            global_ini_callback("Skill 0", "strikeprob", "255");
            let v = std::ptr::read_unaligned(&raw const custom_saved.strikeprob[0]);
            assert_eq!(v, 255);
        }
    }

    #[test]
    fn callback_skill_multiple_fields() {
        unsafe {
            setup();
            global_ini_callback("Skill 0", "blockprob", "128");
            let v = std::ptr::read_unaligned(&raw const custom_saved.blockprob[0]);
            assert_eq!(v, 128);
        }
    }

    #[test]
    fn callback_skill_out_of_range_ignored() {
        unsafe {
            setup();
            let before = std::ptr::read_unaligned(&raw const custom_saved.strikeprob[0]);
            global_ini_callback("Skill 12", "strikeprob", "10"); // NUM_GUARD_SKILLS = 12
            let after = std::ptr::read_unaligned(&raw const custom_saved.strikeprob[0]);
            assert_eq!(after, before);
        }
    }

    // ── turn_fixes_and_enhancements_on_off ───────────────────────────────────

    #[test]
    fn turn_fixes_off() {
        unsafe {
            setup();
            turn_fixes_and_enhancements_on_off(0);
            assert_eq!(use_fixes_and_enhancements, 0);
            assert_eq!(fixes, &raw mut fixes_disabled_state);
        }
    }

    #[test]
    fn turn_fixes_on() {
        unsafe {
            setup();
            turn_fixes_and_enhancements_on_off(1);
            assert_eq!(use_fixes_and_enhancements, 1);
            assert_eq!(fixes, &raw mut fixes_saved);
        }
    }

    #[test]
    fn turn_fixes_toggle() {
        unsafe {
            setup();
            turn_fixes_and_enhancements_on_off(1);
            turn_fixes_and_enhancements_on_off(0);
            assert_eq!(fixes, &raw mut fixes_disabled_state);
        }
    }

    // ── set_options_to_default ───────────────────────────────────────────────

    #[test]
    fn defaults_known_values() {
        unsafe {
            setup();
            assert_eq!(enable_music,             1);
            assert_eq!(enable_fade,              1);
            assert_eq!(enable_flash,             1);
            assert_eq!(enable_text,              1);
            assert_eq!(start_fullscreen,         0);
            assert_eq!(enable_lighting,          0);
            assert_eq!(use_fixes_and_enhancements, 0);
        }
    }

    #[test]
    fn defaults_fixes_pointer_is_disabled_state() {
        unsafe {
            setup();
            assert_eq!(fixes, &raw mut fixes_disabled_state);
        }
    }

    #[test]
    fn defaults_fixes_saved_all_true() {
        unsafe {
            setup();
            let bytes = std::slice::from_raw_parts(
                &raw const fixes_saved as *const u8,
                std::mem::size_of::<fixes_options_type>(),
            );
            assert!(bytes.iter().all(|&b| b == 1), "not all fix fields defaulted to 1");
        }
    }

    #[test]
    fn defaults_idempotent() {
        unsafe {
            setup();
            let music_1 = enable_music;
            let fullscreen_1 = start_fullscreen;
            setup();
            assert_eq!(enable_music,     music_1);
            assert_eq!(start_fullscreen, fullscreen_1);
        }
    }
}
