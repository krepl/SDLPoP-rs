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

#endif // STATE_DUMP_H
