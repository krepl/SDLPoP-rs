#ifndef STATE_DUMP_H
#define STATE_DUMP_H

// Frame-state tracing for differential testing between C and Rust builds.
// Enable by setting env var: POPTRACE_OUT=/path/to/output.trace
// Compare two traces: scripts/compare_traces.py a.trace b.trace

void dump_frame_state(void);

#endif // STATE_DUMP_H
