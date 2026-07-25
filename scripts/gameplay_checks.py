#!/usr/bin/env python3
"""Print Kid.x at specific ticks from a POPTRACE trace file.

Used by gameplay_smoke_test.sh to verify a scripted-input scenario actually
moved (or stopped) the Kid, not just that the binary ran without crashing.
Reuses compare_traces.py's trace-parsing helpers rather than re-implementing
the binary format.

Usage: python3 scripts/gameplay_checks.py TRACE_FILE TICK [TICK ...]
Prints the requested Kid.x values, space-separated, in the order given.
Missing ticks (trace ended early) print as "?".
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import compare_traces as ct


def kid_x_by_tick(path, wanted_ticks):
    wanted = set(wanted_ticks)
    values = {}
    with open(path, "rb") as f:
        fields, frame_size = ct.read_header(f)
        offset, size = next((o, s) for n, o, s in fields if n == "Kid")
        while True:
            tick, blob = ct.read_frame(f, frame_size)
            if tick is None:
                break
            if tick in wanted:
                subfields = ct.decode_char_type(blob[offset:offset + size])
                x_value = next(v for name, v, _ in subfields if name == "x")
                values[tick] = x_value
    return values


if __name__ == "__main__":
    trace_path = sys.argv[1]
    ticks = [int(t) for t in sys.argv[2:]]
    values = kid_x_by_tick(trace_path, ticks)
    print(" ".join(str(values.get(t, "?")) for t in ticks))
