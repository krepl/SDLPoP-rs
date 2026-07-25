#!/usr/bin/env bash
# Generates doc/test-fixtures/quicksave_c_oracle.sav: a real QUICKSAVE.SAV written by the
# actual C quick_save() implementation (src/seg000.c), for the Rust port's cross-
# compatibility test (does Rust correctly read a save file the original C code wrote?).
#
# Does NOT touch src/CMakeLists.txt or src/Makefile (those stay pinned to build the full
# `prince` oracle binary). Compiles src/test_quicksave_fixture.c directly against the same
# source files, swapping in a minimal main() instead of main.c's pop_main() entry point.
# quick_save() has no SDL calls, so no SDL_Init/game-startup sequence is needed here.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/src"
OUT_DIR="$ROOT/doc/test-fixtures"
SAVE_DIR="$(mktemp -d)"
trap 'rm -rf "$SAVE_DIR"' EXIT

mkdir -p "$OUT_DIR"

BIN="$(mktemp)"
cc -std=c99 -I"$SRC" -o "$BIN" \
  "$SRC/test_quicksave_fixture.c" \
  "$SRC/data.c" \
  "$SRC/seg000.c" \
  "$SRC/seg001.c" \
  "$SRC/seg002.c" \
  "$SRC/seg003.c" \
  "$SRC/seg004.c" \
  "$SRC/seg005.c" \
  "$SRC/seg006.c" \
  "$SRC/seg007.c" \
  "$SRC/seg008.c" \
  "$SRC/seg009.c" \
  "$SRC/seqtbl.c" \
  "$SRC/options.c" \
  "$SRC/sdl_rw_wrappers.c" \
  "$SRC/state_dump.c" \
  "$SRC/midi.c" "$SRC/opl3.c" \
  "$SRC/replay.c" \
  "$SRC/lighting.c" \
  "$SRC/screenshot.c" \
  "$SRC/menu.c" \
  "$SRC/stb_vorbis.c" \
  $(pkg-config --cflags --libs sdl2 SDL2_image) -lm

SDLPOP_SAVE_PATH="$SAVE_DIR" "$BIN"
cp "$SAVE_DIR/QUICKSAVE.SAV" "$OUT_DIR/quicksave_c_oracle.sav"
echo "Wrote $OUT_DIR/quicksave_c_oracle.sav"
