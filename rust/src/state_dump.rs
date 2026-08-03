#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

// Not part of the Step D State-facade migration: this module's entire purpose is to
// reflectively dump every named global (gameplay, rendering, config alike) to a trace
// file for the differential harness. It is diagnostic instrumentation, not gameplay
// logic, and (like SDL/audio/replay infra) is out of `State`'s scope by design.

use std::os::raw::{c_char, c_int, c_void};
use super::*;
use crate::platform::Renderer;

extern "C" {
    fn fflush(stream: *mut FILE) -> c_int;
    fn atoi(s: *const c_char) -> c_int;
}

// X(field_name, ptr, size) — all mutable game state worth tracing.
// The order here MUST match the C FIELDS macro exactly: the harness compares
// trace files byte-for-byte against the C golden trace.
macro_rules! for_each_field {
    ($op:ident) => {
        $op!(curr_room);
        $op!(current_level);
        $op!(drawn_room);
        $op!(loaded_room);
        $op!(draw_xh);
        $op!(room_L);
        $op!(room_R);
        $op!(room_A);
        $op!(room_B);
        $op!(room_BR);
        $op!(room_BL);
        $op!(room_AR);
        $op!(room_AL);
        $op!(Kid);
        $op!(Guard);
        $op!(Char);
        $op!(Opp);
        $op!(hitp_curr);
        $op!(hitp_max);
        $op!(hitp_delta);
        $op!(hitp_beg_lev);
        $op!(guardhp_curr);
        $op!(guardhp_max);
        $op!(guardhp_delta);
        $op!(flash_color);
        $op!(flash_time);
        $op!(rem_min);
        $op!(rem_tick);
        $op!(grab_timer);
        $op!(exit_room_timer);
        $op!(guard_notice_timer);
        $op!(guard_refrac);
        $op!(have_sword);
        $op!(holding_sword);
        $op!(checkpoint);
        $op!(leveldoor_open);
        $op!(leveldoor_right);
        $op!(leveldoor_ybottom);
        $op!(united_with_shadow);
        $op!(shadow_initialized);
        $op!(is_feather_fall);
        $op!(is_screaming);
        $op!(kid_sword_strike);
        $op!(need_full_redraw);
        $op!(guard_skill);
        $op!(can_guard_see_kid);
        $op!(offguard);
        $op!(droppedout);
        $op!(justblocked);
        $op!(knock);
        $op!(seamless);
        $op!(different_room);
        $op!(is_blind_mode);
        $op!(is_paused);
        $op!(next_level);
        $op!(is_restart_level);
        $op!(random_seed);
        $op!(curr_tile);
        $op!(curr_modifier);
        $op!(curr_tilepos);
        $op!(tile_col);
        $op!(tile_row);
        $op!(edge_type);
        $op!(char_col_right);
        $op!(char_col_left);
        $op!(char_top_row);
        $op!(prev_char_top_row);
        $op!(char_bottom_row);
        $op!(prev_char_col_right);
        $op!(prev_char_col_left);
        $op!(char_x_left);
        $op!(char_x_right);
        $op!(char_x_left_coll);
        $op!(char_x_right_coll);
        $op!(char_top_y);
        $op!(char_width_half);
        $op!(char_height);
        $op!(redraw_height);
        $op!(fall_frame);
        $op!(through_tile);
        $op!(infrontx);
        $op!(collision_row);
        $op!(prev_collision_row);
        $op!(obj_xh);
        $op!(obj_xl);
        $op!(obj_y);
        $op!(obj_chtab);
        $op!(obj_id);
        $op!(obj_tilepos);
        $op!(obj_x);
        $op!(obj_direction);
        $op!(obj_clip_left);
        $op!(obj_clip_top);
        $op!(obj_clip_right);
        $op!(obj_clip_bottom);
        $op!(prev_coll_room);
        $op!(curr_row_coll_room);
        $op!(below_row_coll_room);
        $op!(above_row_coll_room);
        $op!(curr_row_coll_flags);
        $op!(above_row_coll_flags);
        $op!(below_row_coll_flags);
        $op!(prev_coll_flags);
        $op!(table_counts);
        $op!(foretable);
        $op!(backtable);
        $op!(midtable);
        $op!(drects_count);
        $op!(need_drects);
        $op!(drects);
        $op!(mobs_count);
        $op!(mobs);
        $op!(trobs_count);
        $op!(trob);
        $op!(trobs);
        $op!(n_curr_objs);
        $op!(objtable);
        $op!(curr_objs);
        $op!(curmob);
        $op!(redraw_frames_anim);
        $op!(redraw_frames2);
        $op!(redraw_frames_full);
        $op!(redraw_frames_fore);
        $op!(redraw_frames_floor_overlay);
        $op!(tile_object_redraw);
        $op!(redraw_frames_above);
        $op!(wipe_frames);
        $op!(wipe_heights);
        $op!(level);
        $op!(leftroom_);
        $op!(row_below_left_);
        $op!(palace_wall_colors);
        $op!(curr_guard_color);
    };
}

#[repr(C)]
#[derive(Copy, Clone)]
struct field_desc_t {
    name: [c_char; 64],
    offset: u32,
    size: u32,
}

static mut trace_fp: *mut FILE = core::ptr::null_mut();
static mut initialized: c_int = 0;
static mut tick_counter: u32 = 0;

// Exposes the tick about to be simulated (the value dump_frame_state will next
// write) so seg009.rs's scripted-input injection can schedule key events against
// the same clock the trace uses, instead of maintaining a second, drifting one.
pub(crate) unsafe fn next_tick() -> u32 {
    tick_counter
}
static mut frame_size: u32 = 0;
static mut num_fields: u32 = 0;
static mut field_table: [field_desc_t; 256] = [field_desc_t {
    name: [0; 64],
    offset: 0,
    size: 0,
}; 256];

// max_ticks is a function-static in C (inside dump_frame_state); module-scope here.
static mut max_ticks: i32 = -1;

unsafe fn set_field_name(idx: usize, name: &str) {
    // strncpy(field_table[idx].name, name, 63); field_table[idx].name[63] = '\0';
    let dst = &mut field_table[idx].name;
    let bytes = name.as_bytes();
    let n = if bytes.len() < 63 { bytes.len() } else { 63 };
    let mut i = 0;
    while i < n {
        dst[i] = bytes[i] as c_char;
        i += 1;
    }
    while i < 64 {
        dst[i] = 0;
        i += 1;
    }
}

unsafe fn build_field_table() {
    let mut offset: u32 = 0;
    macro_rules! reg {
        ($fname:ident) => {{
            set_field_name(num_fields as usize, stringify!($fname));
            field_table[num_fields as usize].offset = offset;
            let sz = core::mem::size_of_val(&$fname) as u32;
            field_table[num_fields as usize].size = sz;
            num_fields += 1;
            offset += sz;
        }};
    }
    for_each_field!(reg);
    frame_size = offset;
}

unsafe fn write_header() {
    let magic: [u8; 8] = *b"POPTRACE";
    let version: u32 = 1;
    fwrite(magic.as_ptr() as *const c_void, 1, 8, trace_fp);
    fwrite(
        &version as *const u32 as *const c_void,
        core::mem::size_of::<u32>(),
        1,
        trace_fp,
    );
    fwrite(
        &num_fields as *const u32 as *const c_void,
        core::mem::size_of::<u32>(),
        1,
        trace_fp,
    );
    fwrite(
        &frame_size as *const u32 as *const c_void,
        core::mem::size_of::<u32>(),
        1,
        trace_fp,
    );
    let mut i: u32 = 0;
    while i < num_fields {
        fwrite(
            field_table[i as usize].name.as_ptr() as *const c_void,
            64,
            1,
            trace_fp,
        );
        fwrite(
            &field_table[i as usize].offset as *const u32 as *const c_void,
            core::mem::size_of::<u32>(),
            1,
            trace_fp,
        );
        fwrite(
            &field_table[i as usize].size as *const u32 as *const c_void,
            core::mem::size_of::<u32>(),
            1,
            trace_fp,
        );
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn dump_frame_state() {
    if initialized == 0 {
        initialized = 1;
        let path = getenv(b"POPTRACE_OUT\0".as_ptr() as *const c_char);
        if path.is_null() {
            return;
        }
        trace_fp = fopen(path, b"wb\0".as_ptr() as *const c_char);
        if trace_fp.is_null() {
            eprintln!(
                "state_dump: could not open {}",
                std::ffi::CStr::from_ptr(path).to_string_lossy()
            );
            return;
        }
        build_field_table();
        write_header();
    }
    if trace_fp.is_null() {
        return;
    }

    // Auto-exit after POPTRACE_TICKS ticks if set
    if max_ticks < 0 {
        let mt = getenv(b"POPTRACE_TICKS\0".as_ptr() as *const c_char);
        max_ticks = if !mt.is_null() { atoi(mt) } else { 0 };
    }
    if max_ticks > 0 && tick_counter as i32 >= max_ticks {
        fflush(trace_fp);
        fclose(trace_fp);
        std::process::exit(0);
    }

    fwrite(
        &tick_counter as *const u32 as *const c_void,
        core::mem::size_of::<u32>(),
        1,
        trace_fp,
    );
    tick_counter = tick_counter.wrapping_add(1);

    macro_rules! dump {
        ($fname:ident) => {{
            let sz = core::mem::size_of_val(&$fname);
            fwrite(
                core::ptr::addr_of!($fname) as *const c_void,
                sz,
                1,
                trace_fp,
            );
        }};
    }
    for_each_field!(dump);

    fflush(trace_fp);
}

static mut pixels_fp: *mut FILE = core::ptr::null_mut();
static mut pixels_initialized: c_int = 0;

/// Appends one `"<tick> <hash>\n"` line to `POPPIXELS_OUT`, hashing the raw bytes
/// of [`get_final_surface`] with FNV-1a. A cheap per-tick fingerprint of what was
/// actually drawn, used by the harness to catch rendering regressions the
/// state-only [`dump_frame_state`] trace can't see. The C oracle has a
/// byte-identical implementation (`state_dump.c`) reading the same surface
/// fields, so golden hash files compare directly against this output.
#[no_mangle]
pub unsafe extern "C" fn dump_frame_pixels() {
    if pixels_initialized == 0 {
        pixels_initialized = 1;
        let path = getenv(b"POPPIXELS_OUT\0".as_ptr() as *const c_char);
        if path.is_null() {
            return;
        }
        pixels_fp = fopen(path, b"wb\0".as_ptr() as *const c_char);
        if pixels_fp.is_null() {
            eprintln!(
                "state_dump: could not open {}",
                std::ffi::CStr::from_ptr(path).to_string_lossy()
            );
            return;
        }
    }
    if pixels_fp.is_null() {
        return;
    }

    let surf = get_final_surface();
    let renderer = crate::platform::sdl::shared_renderer();
    let (w, h) = renderer.surface_size(surf);
    let pitch = renderer.surface_pitch(surf);
    let bpp = renderer.surface_format_info(surf).bytes_per_pixel as usize;
    let pixels = renderer.surface_pixels(surf) as *const u8;
    let row_len = w as usize * bpp;

    // FNV-1a 64-bit, computed row by row so `pitch` padding bytes (if any)
    // never enter the hash.
    let mut hash: u64 = 0xcbf29ce484222325;
    for row in 0..h as isize {
        let row_ptr = pixels.offset(row * pitch as isize);
        let row_bytes = core::slice::from_raw_parts(row_ptr, row_len);
        for &b in row_bytes {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }

    // dump_frame_state (called earlier this same tick, in play_level_2) already
    // incremented tick_counter past the tick just simulated -- subtract 1 so
    // this line's tick number matches the trace record for the same frame.
    let line = format!("{} {:016x}\n", tick_counter.wrapping_sub(1), hash);
    fwrite(line.as_ptr() as *const c_void, 1, line.len(), pixels_fp);
    fflush(pixels_fp);
}

#[cfg(test)]
mod tests {
    use super::*;

    // The trace harness compares trace files byte-for-byte against the all-C
    // golden trace. The header (field count, names, offsets, sizes, frame_size)
    // must therefore match exactly. These values are read from
    // traces/golden.trace (header parsed with the format documented at the top
    // of state_dump.c).
    #[test]
    fn field_table_matches_golden_header() {
        unsafe {
            num_fields = 0;
            build_field_table();
            assert_eq!(num_fields, 133, "field count must match golden");
            assert_eq!(frame_size, 10270, "frame_size must match golden");

            // Spot-check a few (name, offset, size) descriptors against golden.
            let check = |idx: usize, name: &str, off: u32, sz: u32| {
                let n = &field_table[idx].name;
                let got: String = n
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();
                assert_eq!(got, name, "name at idx {idx}");
                assert_eq!(field_table[idx].offset, off, "offset of {name}");
                assert_eq!(field_table[idx].size, sz, "size of {name}");
            };
            check(0, "curr_room", 0, 2);
            check(1, "current_level", 2, 2);
            check(4, "draw_xh", 8, 2);
            check(128, "level", 7805, 2305);
            check(129, "leftroom_", 10110, 6);
            check(130, "row_below_left_", 10116, 20);
            check(131, "palace_wall_colors", 10136, 132);
            check(132, "curr_guard_color", 10268, 2);
        }
    }
}
