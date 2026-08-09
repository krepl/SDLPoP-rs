#ifndef STATE_DUMP_H
#define STATE_DUMP_H

// Frame-state tracing for differential testing between C and Rust builds.
// Enable by setting env var: POPTRACE_OUT=/path/to/output.trace
// Compare two traces: scripts/compare_traces.py a.trace b.trace

void dump_frame_state(void);

// Per-tick FNV-1a hash of the final rendered surface, for pixel-level
// regression testing (catches rendering bugs the state-only trace above
// cannot). Enable by setting env var: POPPIXELS_OUT=/path/to/output.pixels
void dump_frame_pixels(void);

// The current tick_counter value (state_dump.c keeps it file-local) -- needed by
// seg009.c's scripted-input injection to key events off the same clock the trace/pixel
// dumps use. See rust/src/state_dump.rs's next_tick() for the Rust equivalent.
unsigned int state_dump_next_tick(void);

// dump_frame_pixels()'s counterpart for the pause menu specifically. The outer per-tick
// capture point above is only ever reached from play_level_2's per-tick loop -- while the
// menu is open, execution is inside draw_menu()'s own blocking inner loop, which never
// reaches that call site at all. Call from draw_menu() right after update_screen(). Uses its
// own monotonic frame counter (menu_frame_counter), independent of tick_counter (frozen
// solid the whole time the menu is open). Enable via POPMENUPIXELS_OUT=/path/to/output.
void dump_menu_frame_pixels(void);

#endif // STATE_DUMP_H
