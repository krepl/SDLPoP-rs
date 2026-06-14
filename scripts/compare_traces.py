#!/usr/bin/env python3
"""Compare two POPTRACE frame-state dumps to find the first divergence.

Usage:
    python3 scripts/compare_traces.py golden.trace compare.trace [--all]

Options:
    --all          Show every divergent frame, not just the first.
    --tick N       Start comparison from tick N.
    --ignore F     Ignore field named F (repeatable). Use for known-noisy
                   fields like random_seed that differ between runs.
"""

import sys
import struct
import argparse
from pathlib import Path


MAGIC = b"POPTRACE"
VERSION = 1


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


def compare(path_a, path_b, show_all=False, start_tick=0, ignore=()):
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
                    print(f"    A: {fmt_bytes(a, size)}")
                    print(f"    B: {fmt_bytes(b, size)}")
                if not show_all:
                    remaining_a = sum(1 for _ in iter(lambda: fa.read(4 + frame_size), b""))
                    print(f"\nStopping at first divergence. "
                          f"Remaining frames not checked.")
                    break

        if n_diverged == 0:
            print(f"OK — {n_frames} frames compared, no divergence found.")
        else:
            print(f"\nSummary: {n_diverged} divergent frame(s) out of {n_frames} compared.")


def main():
    parser = argparse.ArgumentParser(description="Compare two POPTRACE dumps.")
    parser.add_argument("a", help="golden trace (C build)")
    parser.add_argument("b", help="comparison trace (Rust build)")
    parser.add_argument("--all", action="store_true", help="show all divergent frames")
    parser.add_argument("--tick", type=int, default=0, metavar="N",
                        help="skip frames before tick N")
    parser.add_argument("--ignore", action="append", default=[], metavar="FIELD",
                        help="ignore this field (repeatable)")
    args = parser.parse_args()
    compare(args.a, args.b, show_all=args.all, start_tick=args.tick,
            ignore=set(args.ignore))


if __name__ == "__main__":
    main()
