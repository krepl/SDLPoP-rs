#!/usr/bin/env python3
"""Compare two POPTRACE frame-state dumps to find the first divergence.

Usage:
    python3 scripts/compare_traces.py golden.trace compare.trace [--all]
    python3 scripts/compare_traces.py --dump-tick N golden.trace
    python3 scripts/compare_traces.py --gen-test N func_name golden.trace

Options:
    --all              Show every divergent frame, not just the first.
    --tick N           Start comparison from tick N.
    --ignore F         Ignore field named F (repeatable). Use for known-noisy
                       fields like random_seed that differ between runs.
    --dump-tick N      Print all field values at tick N (human-readable).
                       Struct fields (Kid, Guard, Char, Opp) are decoded.
    --gen-test N F     Generate a Rust #[test] stub seeded from state at
                       tick N-1 for a function named F.
    --dump-on-diverge  On first divergence at tick N: dump full state from
                       both traces at that tick and emit a gen-test stub
                       seeded from the golden trace. Useful for debugging.
"""

import sys
import struct
import argparse
from pathlib import Path


MAGIC = b"POPTRACE"
VERSION = 1

# char_type layout from src/types.h (16 bytes total):
#   byte frame;       [0]
#   byte x;           [1]
#   byte y;           [2]
#   sbyte direction;  [3]
#   sbyte curr_col;   [4]
#   sbyte curr_row;   [5]
#   byte action;      [6]
#   sbyte fall_x;     [7]
#   sbyte fall_y;     [8]
#   byte room;        [9]
#   byte repeat;      [10]
#   byte charid;      [11]
#   byte sword;       [12]
#   sbyte alive;      [13]
#   word curr_seq;    [14-15]
CHAR_TYPE_FIELDS = [
    ("frame",     0,  "u8"),
    ("x",         1,  "u8"),
    ("y",         2,  "u8"),
    ("direction", 3,  "i8"),
    ("curr_col",  4,  "i8"),
    ("curr_row",  5,  "i8"),
    ("action",    6,  "u8"),
    ("fall_x",    7,  "i8"),
    ("fall_y",    8,  "i8"),
    ("room",      9,  "u8"),
    ("repeat",    10, "u8"),
    ("charid",    11, "u8"),
    ("sword",     12, "u8"),
    ("alive",     13, "i8"),
    ("curr_seq",  14, "u16"),  # little-endian word at bytes [14:16]
]

# Trace fields that contain a char_type blob (16 bytes).
CHAR_BLOB_FIELDS = {"Kid", "Guard", "Char", "Opp"}


def read_header(f):
    magic = f.read(8)
    if magic != MAGIC:
        raise ValueError(f"bad magic: {magic!r}")
    (version,) = struct.unpack("<I", f.read(4))
    if version != VERSION:
        raise ValueError(f"unsupported version: {version}")
    (field_count,) = struct.unpack("<I", f.read(4))
    (frame_size,)  = struct.unpack("<I", f.read(4))
    fields = []
    for _ in range(field_count):
        name = f.read(64).rstrip(b"\x00").decode()
        (offset, size) = struct.unpack("<II", f.read(8))
        fields.append((name, offset, size))
    return fields, frame_size


def read_frame(f, frame_size):
    tick_bytes = f.read(4)
    if not tick_bytes:
        return None, None
    (tick,) = struct.unpack("<I", tick_bytes)
    blob = f.read(frame_size)
    if len(blob) < frame_size:
        return None, None
    return tick, blob


def diff_blobs(blob_a, blob_b, fields, ignore=()):
    diffs = []
    for name, offset, size in fields:
        if name in ignore:
            continue
        a = blob_a[offset:offset + size]
        b = blob_b[offset:offset + size]
        if a != b:
            diffs.append((name, offset, size, a, b))
    return diffs


def fmt_bytes(b, size):
    if size == 1:
        return f"0x{b[0]:02x} ({b[0]})"
    if size == 2:
        (v,) = struct.unpack("<H", b)
        (vs,) = struct.unpack("<h", b)
        return f"0x{v:04x} (u={v}, s={vs})"
    if size == 4:
        (v,) = struct.unpack("<I", b)
        (vs,) = struct.unpack("<i", b)
        return f"0x{v:08x} (u={v}, s={vs})"
    # larger: show first 16 bytes as hex
    preview = b[:16].hex()
    if len(b) > 16:
        preview += "..."
    return f"[{len(b)} bytes] {preview}"


def decode_char_type(blob16):
    """Return list of (subfield_name, value, type_str) for a 16-byte char_type blob."""
    result = []
    for fname, offset, ftype in CHAR_TYPE_FIELDS:
        if ftype == "u8":
            v = blob16[offset]
        elif ftype == "i8":
            v = struct.unpack("<b", bytes([blob16[offset]]))[0]
        elif ftype == "u16":
            v = struct.unpack("<H", blob16[offset:offset+2])[0]
        else:
            v = blob16[offset]
        result.append((fname, v, ftype))
    return result


def dump_tick(path, target_tick):
    """Print all field values at the given tick in human-readable form."""
    with open(path, "rb") as f:
        fields, frame_size = read_header(f)
        while True:
            tick, blob = read_frame(f, frame_size)
            if tick is None:
                print(f"Tick {target_tick} not found in trace.")
                return
            if tick < target_tick:
                continue
            if tick > target_tick:
                print(f"Tick {target_tick} not found (trace jumps to {tick}).")
                return
            print(f"=== tick {tick} ===")
            for name, offset, size in fields:
                raw = blob[offset:offset + size]
                if name in CHAR_BLOB_FIELDS and size == 16:
                    print(f"  {name}:")
                    for subfname, v, ftype in decode_char_type(raw):
                        print(f"    .{subfname} = {v}  ({ftype})")
                else:
                    if size == 1:
                        v = raw[0]
                        sv = struct.unpack("<b", raw)[0]
                        print(f"  {name} = {v}  (u8={v}, i8={sv})")
                    elif size == 2:
                        (v,)  = struct.unpack("<H", raw)
                        (sv,) = struct.unpack("<h", raw)
                        print(f"  {name} = {v}  (u16={v}, i16={sv})")
                    elif size == 4:
                        (v,)  = struct.unpack("<I", raw)
                        (sv,) = struct.unpack("<i", raw)
                        print(f"  {name} = {v}  (u32={v}, i32={sv})")
                    else:
                        print(f"  {name} = {fmt_bytes(raw, size)}")
            return


def _gen_test_from_blob(blob, fields, target_tick, func_name):
    """Print a Rust #[test] stub from a pre-loaded blob (state at tick target_tick-1)."""
    input_tick = max(0, target_tick - 1)
    lines = []
    lines.append(f"#[test]")
    lines.append(f"fn {func_name}() {{")
    lines.append(f"    // State seeded from golden trace tick {input_tick} (= input to tick {target_tick}).")
    lines.append(f"    // NOTE: level.fg / level.bg tiles are NOT in the trace — set manually.")
    lines.append(f"    unsafe {{")
    lines.append(f"        set_options_to_default();")

    for name, offset, size in fields:
        raw = blob[offset:offset + size]
        if name in CHAR_BLOB_FIELDS and size == 16:
            for subfname, v, ftype in decode_char_type(raw):
                lines.append(f"        {name}.{subfname} = {v};")
        else:
            if size == 1:
                v = raw[0]
                sv = struct.unpack("<b", raw)[0]
                lines.append(f"        {name} = {v};  // u8={v} i8={sv}")
            elif size == 2:
                (v,)  = struct.unpack("<H", raw)
                (sv,) = struct.unpack("<h", raw)
                lines.append(f"        {name} = {v};  // u16={v} i16={sv}")
            elif size == 4:
                (v,)  = struct.unpack("<I", raw)
                (sv,) = struct.unpack("<i", raw)
                lines.append(f"        {name} = {v};  // u32={v} i32={sv}")
            else:
                lines.append(f"        // {name}: {fmt_bytes(raw, size)}  (set manually)")

    lines.append(f"")
    lines.append(f"        // TODO: set level.fg / level.bg for any tiles the function reads.")
    lines.append(f"        // TODO: call the diverging function.")
    lines.append(f"        // TODO: assert expected post-call state (read from --dump-on-diverge output above).")
    lines.append(f"")
    lines.append(f"        set_options_to_default();")
    lines.append(f"    }}")
    lines.append(f"}}")
    print("\n".join(lines))


def gen_test(path, target_tick, func_name):
    """Emit a Rust #[test] stub with state from target_tick-1 pre-filled."""
    input_tick = max(0, target_tick - 1)
    with open(path, "rb") as f:
        fields, frame_size = read_header(f)
        frames = {}
        while True:
            tick, blob = read_frame(f, frame_size)
            if tick is None:
                break
            if tick == input_tick:
                frames[input_tick] = blob
            if tick == target_tick:
                frames[target_tick] = blob
            if len(frames) == 2:
                break

    if input_tick not in frames:
        print(f"// ERROR: tick {input_tick} not found in trace")
        return

    _gen_test_from_blob(frames[input_tick], fields, target_tick, f"{func_name}_tick_{target_tick}")


def dump_blob(label, blob, fields):
    """Print all field values from a blob in human-readable form."""
    print(f"  [{label}]")
    for name, offset, size in fields:
        raw = blob[offset:offset + size]
        if name in CHAR_BLOB_FIELDS and size == 16:
            print(f"    {name}:")
            for subfname, v, ftype in decode_char_type(raw):
                print(f"      .{subfname} = {v}  ({ftype})")
        else:
            if size == 1:
                v = raw[0]
                sv = struct.unpack("<b", raw)[0]
                print(f"    {name} = {v}  (u8={v}, i8={sv})")
            elif size == 2:
                (v,)  = struct.unpack("<H", raw)
                (sv,) = struct.unpack("<h", raw)
                print(f"    {name} = {v}  (u16={v}, i16={sv})")
            elif size == 4:
                (v,)  = struct.unpack("<I", raw)
                (sv,) = struct.unpack("<i", raw)
                print(f"    {name} = {v}  (u32={v}, i32={sv})")
            else:
                print(f"    {name} = {fmt_bytes(raw, size)}")


def compare(path_a, path_b, show_all=False, start_tick=0, ignore=(),
            dump_on_diverge=False):
    with open(path_a, "rb") as fa, open(path_b, "rb") as fb:
        fields_a, frame_size_a = read_header(fa)
        fields_b, frame_size_b = read_header(fb)

        if [f[0] for f in fields_a] != [f[0] for f in fields_b]:
            print("ERROR: field tables differ between traces")
            print("  A fields:", [f[0] for f in fields_a])
            print("  B fields:", [f[0] for f in fields_b])
            sys.exit(1)
        if frame_size_a != frame_size_b:
            print(f"ERROR: frame sizes differ: {frame_size_a} vs {frame_size_b}")
            sys.exit(1)

        if ignore:
            print(f"Ignoring fields: {', '.join(ignore)}\n")

        frame_size = frame_size_a
        fields = fields_a
        n_frames = 0
        n_diverged = 0

        while True:
            tick_a, blob_a = read_frame(fa, frame_size)
            tick_b, blob_b = read_frame(fb, frame_size)

            if tick_a is None or tick_b is None:
                break

            n_frames += 1
            if tick_a < start_tick:
                continue

            diffs = diff_blobs(blob_a, blob_b, fields, ignore)
            if diffs:
                n_diverged += 1
                print(f"\n--- tick {tick_a} ({len(diffs)} field(s) differ) ---")
                for name, offset, size, a, b in diffs:
                    print(f"  {name}")
                    print(f"    A (golden): {fmt_bytes(a, size)}")
                    print(f"    B (test):   {fmt_bytes(b, size)}")
                if dump_on_diverge:
                    print(f"\n--- full state at tick {tick_a} ---")
                    dump_blob("golden", blob_a, fields)
                    dump_blob("test",   blob_b, fields)
                    diverged_fields = "_".join(name for name, *_ in diffs[:2])
                    stub_name = f"investigate_{diverged_fields}_tick_{tick_a}"
                    print(f"\n--- gen-test stub (fill tiles, add assertions) ---")
                    _gen_test_from_blob(blob_a, fields, tick_a, stub_name)
                if not show_all:
                    print(f"\nStopping at first divergence.")
                    break

        if n_diverged == 0:
            print(f"OK — {n_frames} frames compared, no divergence found.")
        else:
            print(f"\nSummary: {n_diverged} divergent frame(s) out of {n_frames} compared.")


def main():
    parser = argparse.ArgumentParser(description="Compare two POPTRACE dumps.")
    parser.add_argument("--dump-tick", type=int, metavar="N",
                        help="print all field values at tick N (human-readable)")
    parser.add_argument("--gen-test", type=str, nargs=2, metavar=("N", "FUNC"),
                        help="emit a Rust test stub seeded from state at tick N-1 for function FUNC")
    parser.add_argument("a", nargs="?",
                        help="golden trace (C build); sole trace for --dump-tick / --gen-test")
    parser.add_argument("b", nargs="?", help="comparison trace (Rust build)")
    parser.add_argument("--all", action="store_true", help="show all divergent frames")
    parser.add_argument("--tick", type=int, default=0, metavar="N",
                        help="skip frames before tick N")
    parser.add_argument("--ignore", action="append", default=[], metavar="FIELD",
                        help="ignore this field (repeatable)")
    parser.add_argument("--dump-on-diverge", action="store_true",
                        help="on first divergence: dump full state from both traces + emit gen-test stub")
    args = parser.parse_args()

    if args.dump_tick is not None:
        if not args.a:
            parser.error("--dump-tick requires a trace file argument")
        dump_tick(args.a, args.dump_tick)
    elif args.gen_test is not None:
        tick_n, func_name = int(args.gen_test[0]), args.gen_test[1]
        if not args.a:
            parser.error("--gen-test requires a trace file argument")
        gen_test(args.a, tick_n, func_name)
    else:
        if not args.a or not args.b:
            parser.error("comparison mode requires two trace file arguments")
        compare(args.a, args.b, show_all=args.all, start_tick=args.tick,
                ignore=set(args.ignore), dump_on_diverge=args.dump_on_diverge)


if __name__ == "__main__":
    main()
