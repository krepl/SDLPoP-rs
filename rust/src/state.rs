//! All game simulation state for the Prince of Persia Rust port.
//!
//! Fields are added here first (pre-defined for all subsystems), then
//! implemented subsystem-by-subsystem. Never remove a field once added.
//!
//! SDL handles, audio, options/config, and replay infrastructure are NOT
//! here — those stay as C globals. Only simulation state that affects
//! gameplay determinism belongs in this struct.

use super::*;

#[derive(Clone)]
pub struct State {
    // -------------------------------------------------------------------------
    // Characters (seg002, seg005)
    // -------------------------------------------------------------------------
    pub kid:   char_type,   // C: Kid
    pub guard: char_type,   // C: Guard
    pub char_: char_type,   // C: Char  (current character being processed)
    pub opp:   char_type,   // C: Opp   (current opponent)

    // -------------------------------------------------------------------------
    // Level / room layout (seg003, seg006)
    // -------------------------------------------------------------------------
    pub level:         level_type,
    pub current_level: u16,
    pub loaded_room:   u16,
    pub drawn_room:    u16,
    pub curr_room:     i16,
    pub next_room:     u16,
    pub next_level:    u16,
    // Adjacent room indices
    pub room_l:  u16,  // C: room_L
    pub room_r:  u16,  // C: room_R
    pub room_a:  u16,  // C: room_A
    pub room_b:  u16,  // C: room_B
    pub room_al: u16,  // C: room_AL
    pub room_ar: u16,  // C: room_AR
    pub room_bl: u16,  // C: room_BL
    pub room_br: u16,  // C: room_BR

    // -------------------------------------------------------------------------
    // Tile state (seg006, seg008)
    // -------------------------------------------------------------------------
    pub curr_tile:     u8,
    pub curr_modifier: u8,
    pub curr_tilepos:  u8,
    pub curr_tile2:    u8,
    pub tile_col:      i16,
    pub tile_row:      i16,
    pub draw_xh:       u16,
    pub through_tile:  u8,
    pub edge_type:     u8,
    pub leftroom_:        [tile_and_mod; 3],
    pub row_below_left_:  [tile_and_mod; 10],

    // -------------------------------------------------------------------------
    // HP / combat (seg002, seg005)
    // -------------------------------------------------------------------------
    pub hitp_curr:      u16,
    pub hitp_max:       u16,
    pub hitp_delta:     i16,
    pub hitp_beg_lev:   u16,
    pub guardhp_curr:   u16,
    pub guardhp_max:    u16,
    pub guardhp_delta:  i16,
    pub flash_color:    u16,
    pub flash_time:     u16,
    pub have_sword:     u16,
    pub holding_sword:  u16,
    pub kid_sword_strike: u16,
    pub knock:          i16,
    pub justblocked:    u16,
    pub united_with_shadow: i16,

    // -------------------------------------------------------------------------
    // Timers (seg003)
    // -------------------------------------------------------------------------
    pub rem_min:            i16,
    pub rem_tick:           u16,
    pub text_time_remaining: u16,
    pub text_time_total:    u16,
    pub is_show_time:       u16,
    pub resurrect_time:     u16,
    pub checkpoint:         u16,
    pub exit_room_timer:    u16,
    pub guard_notice_timer: i16,
    pub grab_timer:         u16,
    pub demo_time:          i16,
    pub need_level1_music:  u16,

    // -------------------------------------------------------------------------
    // Guard AI state (seg002)
    // -------------------------------------------------------------------------
    pub can_guard_see_kid: i16,
    pub is_guard_notice:   u16,
    pub guard_skill:       u16,
    pub guard_refrac:      u16,
    pub offguard:          u16,
    pub curr_guard_color:  u16,

    // -------------------------------------------------------------------------
    // Mobs / trobs — animated tiles (seg007)
    // -------------------------------------------------------------------------
    pub mobs:       [mob_type; 14],
    pub trobs:      [trob_type; 30],  // TROBS_MAX = 30
    pub mobs_count: i16,
    pub trobs_count: i16,
    pub curmob: mob_type,
    pub trob:   trob_type,

    // -------------------------------------------------------------------------
    // Collision / position (seg004)
    // -------------------------------------------------------------------------
    pub char_col_right:      i16,
    pub char_col_left:       i16,
    pub char_top_row:        i16,
    pub char_bottom_row:     i16,
    pub prev_char_top_row:   i16,
    pub prev_char_col_right: i16,
    pub prev_char_col_left:  i16,
    pub collision_row:       i8,
    pub prev_collision_row:  i8,
    pub prev_coll_room:           [i8; 10],
    pub curr_row_coll_room:       [i8; 10],
    pub below_row_coll_room:      [i8; 10],
    pub above_row_coll_room:      [i8; 10],
    pub curr_row_coll_flags:      [u8; 10],
    pub above_row_coll_flags:     [u8; 10],
    pub below_row_coll_flags:     [u8; 10],
    pub prev_coll_flags:          [u8; 10],
    pub char_width_half: u16,
    pub char_height:     u16,
    pub char_x_left:      i16,
    pub char_x_left_coll: i16,
    pub char_x_right_coll: i16,
    pub char_x_right:     i16,
    pub char_top_y:       i16,
    pub fall_frame:       u8,
    pub infrontx:         i8,

    // -------------------------------------------------------------------------
    // Input controls
    // -------------------------------------------------------------------------
    pub control_x:        i8,
    pub control_y:        i8,
    pub control_shift:    i8,
    pub control_shift2:   i8,
    pub control_forward:  i8,
    pub control_backward: i8,
    pub control_up:       i8,
    pub control_down:     i8,
    pub ctrl1_forward:    i8,
    pub ctrl1_backward:   i8,
    pub ctrl1_up:         i8,
    pub ctrl1_down:       i8,

    // -------------------------------------------------------------------------
    // RNG
    // -------------------------------------------------------------------------
    pub random_seed:    u32,
    pub seed_was_init:  u16,

    // -------------------------------------------------------------------------
    // Drawing / render tables (seg008)
    // -------------------------------------------------------------------------
    pub foretable:   [back_table_type; 200],
    pub backtable:   [back_table_type; 200],
    pub midtable:    [midtable_type; 50],
    pub wipetable:   [wipetable_type; 300],
    pub table_counts: [i16; 5],
    pub drects:       [rect_type; 30],
    pub drects_count: i16,
    pub peels_count:  i16,
    pub wipe_frames:               [u8; 30],
    pub wipe_heights:              [i8; 30],
    pub redraw_frames_anim:        [u8; 30],
    pub redraw_frames2:            [u8; 30],
    pub redraw_frames_floor_overlay: [u8; 30],
    pub redraw_frames_full:        [u8; 30],
    pub redraw_frames_fore:        [u8; 30],
    pub tile_object_redraw:        [u8; 30],
    pub redraw_frames_above:       [u8; 10],
    pub need_full_redraw: u16,
    pub n_curr_objs:      i16,
    pub objtable:  [objtable_type; 50],
    pub curr_objs: [i16; 50],
    pub seamless:    u16,
    pub need_drects: u16,
    pub is_blind_mode: u16,
    pub redraw_height: i16,

    // -------------------------------------------------------------------------
    // Current object being drawn (seg008)
    // -------------------------------------------------------------------------
    pub obj_direction: i8,
    pub obj_clip_left:   i16,
    pub obj_clip_top:    i16,
    pub obj_clip_right:  i16,
    pub obj_clip_bottom: i16,
    pub obj_xh:     u8,
    pub obj_xl:     u8,
    pub obj_y:      u8,
    pub obj_chtab:  u8,
    pub obj_id:     u8,
    pub obj_tilepos: u8,
    pub obj_x:      i16,
    pub cur_frame:  frame_type,

    // -------------------------------------------------------------------------
    // Miscellaneous game flags
    // -------------------------------------------------------------------------
    pub upside_down:       u16,
    pub dont_reset_time:   u16,
    pub draw_mode:         u16,
    pub is_paused:         u16,
    pub is_restart_level:  u16,
    pub is_cutscene:       u16,
    pub is_ending_sequence: bool,
    pub is_feather_fall:   u16,
    pub is_screaming:      u16,
    pub droppedout:        u16,
    pub roomleave_result:  i16,
    pub different_room:    u16,
    pub shadow_initialized: u16,
    pub jumped_through_mirror: i16,
    pub pickup_obj_type:   i16,
    pub last_loose_sound:  u16,
    pub leveldoor_right:   u16,
    pub leveldoor_ybottom: u16,
    pub leveldoor_open:    u16,
    pub current_sound:     u16,
    pub need_quotes:       u16,
    pub demo_index:        u16,
    pub is_guard_notice_:  u16,  // distinct alias; C uses is_guard_notice in two roles

    // -------------------------------------------------------------------------
    // Super-high-jump state (USE_SUPER_HIGH_JUMP, always on in Rust port)
    // -------------------------------------------------------------------------
    pub super_jump_timer: u8,
    pub super_jump_fall:  u8,
    pub super_jump_room:  u8,
    pub super_jump_col:   i8,
    pub super_jump_row:   i8,
}

impl Default for State {
    fn default() -> Self {
        // SAFETY: all fields are plain data; zero-init is a valid starting point.
        // Subsystem init functions (ported from C) are responsible for setting
        // correct initial values before use.
        unsafe { std::mem::zeroed() }
    }
}
