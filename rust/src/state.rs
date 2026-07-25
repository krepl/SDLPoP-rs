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
}
