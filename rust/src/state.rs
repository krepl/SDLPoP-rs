//! Facade over legacy global game state (Step D de-globalization).
//!
//! `State` owns no storage of its own yet. Each accessor method below borrows
//! an existing `static mut` global directly, so there is exactly one physical
//! location for a given field for as long as both migrated code (which reaches
//! it through `&mut State`) and unmigrated code (which still names the global
//! directly) need access to it. This is Fowler's "parallel change" /
//! expand-contract technique applied to global state:
//!
//! - **Expand**: add an accessor here that borrows the existing global. Old
//!   and new code paths now coexist, reading/writing the same memory — there
//!   is never a second copy to drift out of sync.
//! - **Migrate**: convert call sites, one function (or module) at a time, to
//!   take `&mut State` and go through the accessor instead of the bare name.
//! - **Contract**: once nothing references a field's global by name anymore,
//!   move that field into real ownership here and delete the `static mut`.
//!   This is a small, independent, easily-verified step per field — it does
//!   not need to happen for every field at once.
//!
//! SDL handles, audio, options/config, and replay infrastructure are
//! deliberately not modeled here — those stay as C-side globals accessed
//! directly (or via the `Platform` trait). Only simulation state that affects
//! gameplay determinism belongs in `State`.

#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::c_short;
use super::*;

pub struct State;

impl State {
    // -- seg004 collision-detection state ------------------------------------
    pub unsafe fn Char(&mut self) -> &mut char_type { &mut Char }
    pub unsafe fn level(&mut self) -> &mut level_type { &mut level }
    pub unsafe fn curr_room(&mut self) -> &mut c_short { &mut curr_room }
    pub unsafe fn curr_room_modif(&mut self) -> &mut *mut byte { &mut curr_room_modif }
    pub unsafe fn drawn_room(&mut self) -> &mut word { &mut drawn_room }
    pub unsafe fn room_L(&mut self) -> &mut word { &mut room_L }
    pub unsafe fn room_R(&mut self) -> &mut word { &mut room_R }
    pub unsafe fn room_BL(&mut self) -> &mut word { &mut room_BL }
    pub unsafe fn room_BR(&mut self) -> &mut word { &mut room_BR }

    pub unsafe fn tile_col(&mut self) -> &mut c_short { &mut tile_col }
    pub unsafe fn tile_row(&mut self) -> &mut c_short { &mut tile_row }
    pub unsafe fn curr_tile2(&mut self) -> &mut byte { &mut curr_tile2 }
    pub unsafe fn curr_tilepos(&mut self) -> &mut byte { &mut curr_tilepos }
    pub unsafe fn edge_type(&mut self) -> &mut byte { &mut edge_type }
    pub unsafe fn infrontx(&mut self) -> &mut sbyte { &mut infrontx }

    pub unsafe fn char_height(&mut self) -> &mut word { &mut char_height }
    pub unsafe fn char_x_left_coll(&mut self) -> &mut c_short { &mut char_x_left_coll }
    pub unsafe fn char_x_right_coll(&mut self) -> &mut c_short { &mut char_x_right_coll }
    pub unsafe fn control_shift(&mut self) -> &mut sbyte { &mut control_shift }
    pub unsafe fn is_guard_notice(&mut self) -> &mut word { &mut is_guard_notice }
    pub unsafe fn jumped_through_mirror(&mut self) -> &mut c_short { &mut jumped_through_mirror }

    pub unsafe fn collision_row(&mut self) -> &mut sbyte { &mut collision_row }
    pub unsafe fn prev_collision_row(&mut self) -> &mut sbyte { &mut prev_collision_row }

    pub unsafe fn curr_row_coll_room(&mut self) -> &mut [sbyte; 10] { &mut curr_row_coll_room }
    pub unsafe fn curr_row_coll_flags(&mut self) -> &mut [byte; 10] { &mut curr_row_coll_flags }
    pub unsafe fn above_row_coll_room(&mut self) -> &mut [sbyte; 10] { &mut above_row_coll_room }
    pub unsafe fn above_row_coll_flags(&mut self) -> &mut [byte; 10] { &mut above_row_coll_flags }
    pub unsafe fn below_row_coll_room(&mut self) -> &mut [sbyte; 10] { &mut below_row_coll_room }
    pub unsafe fn below_row_coll_flags(&mut self) -> &mut [byte; 10] { &mut below_row_coll_flags }
    pub unsafe fn prev_coll_room(&mut self) -> &mut [sbyte; 10] { &mut prev_coll_room }
    pub unsafe fn prev_coll_flags(&mut self) -> &mut [byte; 10] { &mut prev_coll_flags }

    // -- seg003 level loop / room-switch state -------------------------------
    pub unsafe fn Kid(&mut self) -> &mut char_type { &mut Kid }
    pub unsafe fn Guard(&mut self) -> &mut char_type { &mut Guard }
    pub unsafe fn Opp(&mut self) -> &mut char_type { &mut Opp }

    pub unsafe fn can_guard_see_kid(&mut self) -> &mut c_short { &mut can_guard_see_kid }
    pub unsafe fn char_bottom_row(&mut self) -> &mut c_short { &mut char_bottom_row }
    pub unsafe fn char_col_left(&mut self) -> &mut c_short { &mut char_col_left }
    pub unsafe fn char_col_right(&mut self) -> &mut c_short { &mut char_col_right }
    pub unsafe fn char_top_row(&mut self) -> &mut c_short { &mut char_top_row }
    pub unsafe fn checkpoint(&mut self) -> &mut word { &mut checkpoint }
    pub unsafe fn current_level(&mut self) -> &mut word { &mut current_level }
    pub unsafe fn demo_index(&mut self) -> &mut word { &mut demo_index }
    pub unsafe fn demo_mode(&mut self) -> &mut word { &mut demo_mode }
    pub unsafe fn demo_time(&mut self) -> &mut c_short { &mut demo_time }
    pub unsafe fn different_room(&mut self) -> &mut word { &mut different_room }
    pub unsafe fn droppedout(&mut self) -> &mut word { &mut droppedout }
    pub unsafe fn exit_room_timer(&mut self) -> &mut word { &mut exit_room_timer }
    pub unsafe fn flash_color(&mut self) -> &mut word { &mut flash_color }
    pub unsafe fn flash_time(&mut self) -> &mut word { &mut flash_time }
    pub unsafe fn grab_timer(&mut self) -> &mut word { &mut grab_timer }
    pub unsafe fn guard_notice_timer(&mut self) -> &mut c_short { &mut guard_notice_timer }
    pub unsafe fn guardhp_curr(&mut self) -> &mut word { &mut guardhp_curr }
    pub unsafe fn guardhp_delta(&mut self) -> &mut c_short { &mut guardhp_delta }
    pub unsafe fn guardhp_max(&mut self) -> &mut word { &mut guardhp_max }
    pub unsafe fn have_sword(&mut self) -> &mut word { &mut have_sword }
    pub unsafe fn hitp_beg_lev(&mut self) -> &mut word { &mut hitp_beg_lev }
    pub unsafe fn hitp_curr(&mut self) -> &mut word { &mut hitp_curr }
    pub unsafe fn hitp_delta(&mut self) -> &mut c_short { &mut hitp_delta }
    pub unsafe fn hitp_max(&mut self) -> &mut word { &mut hitp_max }
    pub unsafe fn holding_sword(&mut self) -> &mut word { &mut holding_sword }
    pub unsafe fn is_blind_mode(&mut self) -> &mut word { &mut is_blind_mode }
    pub unsafe fn is_feather_fall(&mut self) -> &mut word { &mut is_feather_fall }
    pub unsafe fn is_restart_level(&mut self) -> &mut word { &mut is_restart_level }
    pub unsafe fn is_screaming(&mut self) -> &mut word { &mut is_screaming }
    pub unsafe fn is_show_time(&mut self) -> &mut word { &mut is_show_time }
    pub unsafe fn keep_last_seed(&mut self) -> &mut sbyte { &mut keep_last_seed }
    pub unsafe fn knock(&mut self) -> &mut c_short { &mut knock }
    pub unsafe fn leveldoor_open(&mut self) -> &mut word { &mut leveldoor_open }
    pub unsafe fn mobs_count(&mut self) -> &mut c_short { &mut mobs_count }
    pub unsafe fn need_drects(&mut self) -> &mut word { &mut need_drects }
    pub unsafe fn need_level1_music(&mut self) -> &mut word { &mut need_level1_music }
    pub unsafe fn need_quotes(&mut self) -> &mut word { &mut need_quotes }
    pub unsafe fn next_level(&mut self) -> &mut word { &mut next_level }
    pub unsafe fn next_room(&mut self) -> &mut word { &mut next_room }
    pub unsafe fn next_sound(&mut self) -> &mut c_short { &mut next_sound }
    pub unsafe fn obj_clip_left(&mut self) -> &mut c_short { &mut obj_clip_left }
    pub unsafe fn obj_clip_top(&mut self) -> &mut c_short { &mut obj_clip_top }
    pub unsafe fn obj_y(&mut self) -> &mut byte { &mut obj_y }
    pub unsafe fn offguard(&mut self) -> &mut word { &mut offguard }
    pub unsafe fn preserved_seed(&mut self) -> &mut dword { &mut preserved_seed }
    pub unsafe fn prev_char_col_left(&mut self) -> &mut c_short { &mut prev_char_col_left }
    pub unsafe fn prev_char_col_right(&mut self) -> &mut c_short { &mut prev_char_col_right }
    pub unsafe fn prev_char_top_row(&mut self) -> &mut c_short { &mut prev_char_top_row }
    pub unsafe fn random_seed(&mut self) -> &mut dword { &mut random_seed }
    pub unsafe fn rem_min(&mut self) -> &mut c_short { &mut rem_min }
    pub unsafe fn rem_tick(&mut self) -> &mut word { &mut rem_tick }
    pub unsafe fn resurrect_time(&mut self) -> &mut word { &mut resurrect_time }
    pub unsafe fn seamless(&mut self) -> &mut word { &mut seamless }
    pub unsafe fn super_jump_col(&mut self) -> &mut sbyte { &mut super_jump_col }
    pub unsafe fn super_jump_fall(&mut self) -> &mut byte { &mut super_jump_fall }
    pub unsafe fn super_jump_room(&mut self) -> &mut byte { &mut super_jump_room }
    pub unsafe fn super_jump_row(&mut self) -> &mut sbyte { &mut super_jump_row }
    pub unsafe fn super_jump_timer(&mut self) -> &mut byte { &mut super_jump_timer }
    pub unsafe fn table_counts(&mut self) -> &mut [c_short; 5] { &mut table_counts }
    pub unsafe fn text_time_remaining(&mut self) -> &mut word { &mut text_time_remaining }
    pub unsafe fn text_time_total(&mut self) -> &mut word { &mut text_time_total }
    pub unsafe fn trobs_count(&mut self) -> &mut c_short { &mut trobs_count }
    pub unsafe fn united_with_shadow(&mut self) -> &mut c_short { &mut united_with_shadow }
    pub unsafe fn upside_down(&mut self) -> &mut word { &mut upside_down }

    // -- seg005 character movement / input decode state ----------------------
    pub unsafe fn control_backward(&mut self) -> &mut sbyte { &mut control_backward }
    pub unsafe fn control_down(&mut self) -> &mut sbyte { &mut control_down }
    pub unsafe fn control_forward(&mut self) -> &mut sbyte { &mut control_forward }
    pub unsafe fn control_shift2(&mut self) -> &mut sbyte { &mut control_shift2 }
    pub unsafe fn control_up(&mut self) -> &mut sbyte { &mut control_up }
    pub unsafe fn control_x(&mut self) -> &mut sbyte { &mut control_x }
    pub unsafe fn control_y(&mut self) -> &mut sbyte { &mut control_y }
    pub unsafe fn ctrl1_shift2(&mut self) -> &mut sbyte { &mut ctrl1_shift2 }
    pub unsafe fn curr_modifier(&mut self) -> &mut byte { &mut curr_modifier }
    pub unsafe fn guard_refrac(&mut self) -> &mut word { &mut guard_refrac }
    pub unsafe fn kid_sword_strike(&mut self) -> &mut word { &mut kid_sword_strike }
    pub unsafe fn pickup_obj_type(&mut self) -> &mut c_short { &mut pickup_obj_type }
    pub unsafe fn through_tile(&mut self) -> &mut byte { &mut through_tile }

    // -- seg002 guard/shadow AI state -----------------------------------------
    pub unsafe fn char_x_left(&mut self) -> &mut c_short { &mut char_x_left }
    pub unsafe fn char_x_right(&mut self) -> &mut c_short { &mut char_x_right }
    pub unsafe fn curr_guard_color(&mut self) -> &mut word { &mut curr_guard_color }
    pub unsafe fn guard_skill(&mut self) -> &mut word { &mut guard_skill }
    pub unsafe fn justblocked(&mut self) -> &mut word { &mut justblocked }
    pub unsafe fn redraw_height(&mut self) -> &mut c_short { &mut redraw_height }
    pub unsafe fn roomleave_result(&mut self) -> &mut c_short { &mut roomleave_result }
    pub unsafe fn shadow_initialized(&mut self) -> &mut word { &mut shadow_initialized }

    // -- seg007 animated tiles / mob physics ----------------------------------
    pub unsafe fn trob(&mut self) -> &mut trob_type { &mut trob }
    pub unsafe fn trobs(&mut self) -> &mut [trob_type; 30] { &mut trobs }
    pub unsafe fn curmob(&mut self) -> &mut mob_type { &mut curmob }
    pub unsafe fn mobs(&mut self) -> &mut [mob_type; 14] { &mut mobs }
    pub unsafe fn curr_tile(&mut self) -> &mut byte { &mut curr_tile }
    pub unsafe fn g_deprecation_number(&mut self) -> &mut c_int { &mut g_deprecation_number }
    pub unsafe fn last_loose_sound(&mut self) -> &mut word { &mut last_loose_sound }
    pub unsafe fn obj_tilepos(&mut self) -> &mut byte { &mut obj_tilepos }
    pub unsafe fn objtable(&mut self) -> &mut [objtable_type; 50] { &mut objtable }
    pub unsafe fn redraw_frames2(&mut self) -> &mut [byte; 30] { &mut redraw_frames2 }
    pub unsafe fn redraw_frames_above(&mut self) -> &mut [byte; 10] { &mut redraw_frames_above }
    pub unsafe fn redraw_frames_anim(&mut self) -> &mut [byte; 30] { &mut redraw_frames_anim }
    pub unsafe fn redraw_frames_floor_overlay(&mut self) -> &mut [byte; 30] { &mut redraw_frames_floor_overlay }
    pub unsafe fn redraw_frames_fore(&mut self) -> &mut [byte; 30] { &mut redraw_frames_fore }
    pub unsafe fn redraw_frames_full(&mut self) -> &mut [byte; 30] { &mut redraw_frames_full }
    pub unsafe fn room_A(&mut self) -> &mut word { &mut room_A }
    pub unsafe fn room_AL(&mut self) -> &mut word { &mut room_AL }
    pub unsafe fn room_B(&mut self) -> &mut word { &mut room_B }
    pub unsafe fn wipe_frames(&mut self) -> &mut [byte; 30] { &mut wipe_frames }
    pub unsafe fn wipe_heights(&mut self) -> &mut [sbyte; 30] { &mut wipe_heights }
    pub unsafe fn doorlink1_ad(&mut self) -> &mut *mut byte { &mut doorlink1_ad }
    pub unsafe fn doorlink2_ad(&mut self) -> &mut *mut byte { &mut doorlink2_ad }
}
