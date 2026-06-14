#include "common.h"
#include "state_dump.h"
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

// Trace file format:
//   Header:
//     magic[8]        "POPTRACE"
//     version u32     = 1
//     field_count u32
//     frame_size u32  (bytes per frame blob)
//     field_table[field_count]:
//       name[64]  char
//       offset u32
//       size u32
//   Frames (repeated):
//     tick u32
//     blob[frame_size]
//
// Controlled at runtime by env var POPTRACE_OUT=<path>.

// X(field_name, ptr, size) — all mutable game state worth tracing.
// scalars use &name; arrays/structs decay naturally (name = &name[0]).
#define FIELDS(X) \
    X(curr_room,               &curr_room,               sizeof(curr_room)) \
    X(current_level,           &current_level,           sizeof(current_level)) \
    X(drawn_room,              &drawn_room,              sizeof(drawn_room)) \
    X(loaded_room,             &loaded_room,             sizeof(loaded_room)) \
    X(draw_xh,                 &draw_xh,                 sizeof(draw_xh)) \
    X(room_L,                  &room_L,                  sizeof(room_L)) \
    X(room_R,                  &room_R,                  sizeof(room_R)) \
    X(room_A,                  &room_A,                  sizeof(room_A)) \
    X(room_B,                  &room_B,                  sizeof(room_B)) \
    X(room_BR,                 &room_BR,                 sizeof(room_BR)) \
    X(room_BL,                 &room_BL,                 sizeof(room_BL)) \
    X(room_AR,                 &room_AR,                 sizeof(room_AR)) \
    X(room_AL,                 &room_AL,                 sizeof(room_AL)) \
    X(Kid,                     &Kid,                     sizeof(Kid)) \
    X(Guard,                   &Guard,                   sizeof(Guard)) \
    X(Char,                    &Char,                    sizeof(Char)) \
    X(Opp,                     &Opp,                     sizeof(Opp)) \
    X(hitp_curr,               &hitp_curr,               sizeof(hitp_curr)) \
    X(hitp_max,                &hitp_max,                sizeof(hitp_max)) \
    X(hitp_delta,              &hitp_delta,              sizeof(hitp_delta)) \
    X(hitp_beg_lev,            &hitp_beg_lev,            sizeof(hitp_beg_lev)) \
    X(guardhp_curr,            &guardhp_curr,            sizeof(guardhp_curr)) \
    X(guardhp_max,             &guardhp_max,             sizeof(guardhp_max)) \
    X(guardhp_delta,           &guardhp_delta,           sizeof(guardhp_delta)) \
    X(flash_color,             &flash_color,             sizeof(flash_color)) \
    X(flash_time,              &flash_time,              sizeof(flash_time)) \
    X(rem_min,                 &rem_min,                 sizeof(rem_min)) \
    X(rem_tick,                &rem_tick,                sizeof(rem_tick)) \
    X(grab_timer,              &grab_timer,              sizeof(grab_timer)) \
    X(exit_room_timer,         &exit_room_timer,         sizeof(exit_room_timer)) \
    X(guard_notice_timer,      &guard_notice_timer,      sizeof(guard_notice_timer)) \
    X(guard_refrac,            &guard_refrac,            sizeof(guard_refrac)) \
    X(have_sword,              &have_sword,              sizeof(have_sword)) \
    X(holding_sword,           &holding_sword,           sizeof(holding_sword)) \
    X(checkpoint,              &checkpoint,              sizeof(checkpoint)) \
    X(leveldoor_open,          &leveldoor_open,          sizeof(leveldoor_open)) \
    X(leveldoor_right,         &leveldoor_right,         sizeof(leveldoor_right)) \
    X(leveldoor_ybottom,       &leveldoor_ybottom,       sizeof(leveldoor_ybottom)) \
    X(united_with_shadow,      &united_with_shadow,      sizeof(united_with_shadow)) \
    X(shadow_initialized,      &shadow_initialized,      sizeof(shadow_initialized)) \
    X(is_feather_fall,         &is_feather_fall,         sizeof(is_feather_fall)) \
    X(is_screaming,            &is_screaming,            sizeof(is_screaming)) \
    X(kid_sword_strike,        &kid_sword_strike,        sizeof(kid_sword_strike)) \
    X(need_full_redraw,        &need_full_redraw,        sizeof(need_full_redraw)) \
    X(guard_skill,             &guard_skill,             sizeof(guard_skill)) \
    X(can_guard_see_kid,       &can_guard_see_kid,       sizeof(can_guard_see_kid)) \
    X(offguard,                &offguard,                sizeof(offguard)) \
    X(droppedout,              &droppedout,              sizeof(droppedout)) \
    X(justblocked,             &justblocked,             sizeof(justblocked)) \
    X(knock,                   &knock,                   sizeof(knock)) \
    X(seamless,                &seamless,                sizeof(seamless)) \
    X(different_room,          &different_room,          sizeof(different_room)) \
    X(is_blind_mode,           &is_blind_mode,           sizeof(is_blind_mode)) \
    X(is_paused,               &is_paused,               sizeof(is_paused)) \
    X(next_level,              &next_level,              sizeof(next_level)) \
    X(is_restart_level,        &is_restart_level,        sizeof(is_restart_level)) \
    X(random_seed,             &random_seed,             sizeof(random_seed)) \
    X(curr_tile,               &curr_tile,               sizeof(curr_tile)) \
    X(curr_modifier,           &curr_modifier,           sizeof(curr_modifier)) \
    X(curr_tilepos,            &curr_tilepos,            sizeof(curr_tilepos)) \
    X(tile_col,                &tile_col,                sizeof(tile_col)) \
    X(tile_row,                &tile_row,                sizeof(tile_row)) \
    X(edge_type,               &edge_type,               sizeof(edge_type)) \
    X(char_col_right,          &char_col_right,          sizeof(char_col_right)) \
    X(char_col_left,           &char_col_left,           sizeof(char_col_left)) \
    X(char_top_row,            &char_top_row,            sizeof(char_top_row)) \
    X(prev_char_top_row,       &prev_char_top_row,       sizeof(prev_char_top_row)) \
    X(char_bottom_row,         &char_bottom_row,         sizeof(char_bottom_row)) \
    X(prev_char_col_right,     &prev_char_col_right,     sizeof(prev_char_col_right)) \
    X(prev_char_col_left,      &prev_char_col_left,      sizeof(prev_char_col_left)) \
    X(char_x_left,             &char_x_left,             sizeof(char_x_left)) \
    X(char_x_right,            &char_x_right,            sizeof(char_x_right)) \
    X(char_x_left_coll,        &char_x_left_coll,        sizeof(char_x_left_coll)) \
    X(char_x_right_coll,       &char_x_right_coll,       sizeof(char_x_right_coll)) \
    X(char_top_y,              &char_top_y,              sizeof(char_top_y)) \
    X(char_width_half,         &char_width_half,         sizeof(char_width_half)) \
    X(char_height,             &char_height,             sizeof(char_height)) \
    X(redraw_height,           &redraw_height,           sizeof(redraw_height)) \
    X(fall_frame,              &fall_frame,              sizeof(fall_frame)) \
    X(through_tile,            &through_tile,            sizeof(through_tile)) \
    X(infrontx,                &infrontx,                sizeof(infrontx)) \
    X(collision_row,           &collision_row,           sizeof(collision_row)) \
    X(prev_collision_row,      &prev_collision_row,      sizeof(prev_collision_row)) \
    X(obj_xh,                  &obj_xh,                  sizeof(obj_xh)) \
    X(obj_xl,                  &obj_xl,                  sizeof(obj_xl)) \
    X(obj_y,                   &obj_y,                   sizeof(obj_y)) \
    X(obj_chtab,               &obj_chtab,               sizeof(obj_chtab)) \
    X(obj_id,                  &obj_id,                  sizeof(obj_id)) \
    X(obj_tilepos,             &obj_tilepos,             sizeof(obj_tilepos)) \
    X(obj_x,                   &obj_x,                   sizeof(obj_x)) \
    X(obj_direction,           &obj_direction,           sizeof(obj_direction)) \
    X(obj_clip_left,           &obj_clip_left,           sizeof(obj_clip_left)) \
    X(obj_clip_top,            &obj_clip_top,            sizeof(obj_clip_top)) \
    X(obj_clip_right,          &obj_clip_right,          sizeof(obj_clip_right)) \
    X(obj_clip_bottom,         &obj_clip_bottom,         sizeof(obj_clip_bottom)) \
    X(prev_coll_room,          prev_coll_room,           sizeof(prev_coll_room)) \
    X(curr_row_coll_room,      curr_row_coll_room,       sizeof(curr_row_coll_room)) \
    X(below_row_coll_room,     below_row_coll_room,      sizeof(below_row_coll_room)) \
    X(above_row_coll_room,     above_row_coll_room,      sizeof(above_row_coll_room)) \
    X(curr_row_coll_flags,     curr_row_coll_flags,      sizeof(curr_row_coll_flags)) \
    X(above_row_coll_flags,    above_row_coll_flags,     sizeof(above_row_coll_flags)) \
    X(below_row_coll_flags,    below_row_coll_flags,     sizeof(below_row_coll_flags)) \
    X(prev_coll_flags,         prev_coll_flags,          sizeof(prev_coll_flags)) \
    X(table_counts,            table_counts,             sizeof(table_counts)) \
    X(foretable,               foretable,                sizeof(foretable)) \
    X(backtable,               backtable,                sizeof(backtable)) \
    X(midtable,                midtable,                 sizeof(midtable)) \
    X(drects_count,            &drects_count,            sizeof(drects_count)) \
    X(need_drects,             &need_drects,             sizeof(need_drects)) \
    X(drects,                  drects,                   sizeof(drects)) \
    X(mobs_count,              &mobs_count,              sizeof(mobs_count)) \
    X(mobs,                    mobs,                     sizeof(mobs)) \
    X(trobs_count,             &trobs_count,             sizeof(trobs_count)) \
    X(trob,                    &trob,                    sizeof(trob)) \
    X(trobs,                   trobs,                    sizeof(trobs)) \
    X(n_curr_objs,             &n_curr_objs,             sizeof(n_curr_objs)) \
    X(objtable,                objtable,                 sizeof(objtable)) \
    X(curr_objs,               curr_objs,                sizeof(curr_objs)) \
    X(curmob,                  &curmob,                  sizeof(curmob)) \
    X(redraw_frames_anim,      redraw_frames_anim,       sizeof(redraw_frames_anim)) \
    X(redraw_frames2,          redraw_frames2,           sizeof(redraw_frames2)) \
    X(redraw_frames_full,      redraw_frames_full,       sizeof(redraw_frames_full)) \
    X(redraw_frames_fore,      redraw_frames_fore,       sizeof(redraw_frames_fore)) \
    X(redraw_frames_floor_overlay, redraw_frames_floor_overlay, sizeof(redraw_frames_floor_overlay)) \
    X(tile_object_redraw,      tile_object_redraw,       sizeof(tile_object_redraw)) \
    X(redraw_frames_above,     redraw_frames_above,      sizeof(redraw_frames_above)) \
    X(wipe_frames,             wipe_frames,              sizeof(wipe_frames)) \
    X(wipe_heights,            wipe_heights,             sizeof(wipe_heights)) \
    X(level,                   &level,                   sizeof(level)) \
    X(leftroom_,               leftroom_,                sizeof(leftroom_)) \
    X(row_below_left_,         row_below_left_,          sizeof(row_below_left_)) \
    X(palace_wall_colors,      palace_wall_colors,       sizeof(palace_wall_colors)) \
    X(curr_guard_color,        &curr_guard_color,        sizeof(curr_guard_color))

typedef struct {
    char name[64];
    uint32_t offset;
    uint32_t size;
} field_desc_t;

static FILE*        trace_fp     = NULL;
static int          initialized  = 0;
static uint32_t     tick_counter = 0;
static uint32_t     frame_size   = 0;
static uint32_t     num_fields   = 0;
static field_desc_t field_table[256];

static void build_field_table(void) {
    uint32_t offset = 0;
#define REG(fname, ptr, sz) \
    do { \
        strncpy(field_table[num_fields].name, #fname, 63); \
        field_table[num_fields].name[63] = '\0'; \
        field_table[num_fields].offset = offset; \
        field_table[num_fields].size   = (uint32_t)(sz); \
        num_fields++; \
        offset += (uint32_t)(sz); \
    } while(0);
    FIELDS(REG)
#undef REG
    frame_size = offset;
}

static void write_header(void) {
    const char magic[8] = "POPTRACE";
    uint32_t version = 1;
    fwrite(magic,         1,          8,          trace_fp);
    fwrite(&version,      sizeof(version), 1,     trace_fp);
    fwrite(&num_fields,   sizeof(num_fields), 1,  trace_fp);
    fwrite(&frame_size,   sizeof(frame_size), 1,  trace_fp);
    for (uint32_t i = 0; i < num_fields; i++) {
        fwrite(field_table[i].name,   64,                         1, trace_fp);
        fwrite(&field_table[i].offset, sizeof(uint32_t),          1, trace_fp);
        fwrite(&field_table[i].size,   sizeof(uint32_t),          1, trace_fp);
    }
}

void dump_frame_state(void) {
    if (!initialized) {
        initialized = 1;
        const char* path = getenv("POPTRACE_OUT");
        if (!path) return;
        trace_fp = fopen(path, "wb");
        if (!trace_fp) {
            fprintf(stderr, "state_dump: could not open %s\n", path);
            return;
        }
        build_field_table();
        write_header();
    }
    if (!trace_fp) return;

    // Auto-exit after POPTRACE_TICKS ticks if set
    static int32_t max_ticks = -1;
    if (max_ticks < 0) {
        const char* mt = getenv("POPTRACE_TICKS");
        max_ticks = mt ? atoi(mt) : 0;
    }
    if (max_ticks > 0 && (int32_t)tick_counter >= max_ticks) {
        fflush(trace_fp);
        fclose(trace_fp);
        exit(0);
    }

    fwrite(&tick_counter, sizeof(tick_counter), 1, trace_fp);
    tick_counter++;

#define DUMP(fname, ptr, sz) fwrite((ptr), (sz), 1, trace_fp);
    FIELDS(DUMP)
#undef DUMP

    fflush(trace_fp);
}
