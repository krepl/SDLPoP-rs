#!/usr/bin/env bash
# Regression test: mouse-driven pause-menu navigation (Phase D item 3/4,
# docs/plans/13-platform-architecture-unification.md) must actually work and produce a
# sane sequence of menu-frame captures.
#
# Unlike menu_smoke_test.sh (which deliberately hangs open forever -- see its header),
# scripts/scripted_inputs/menu_mouse_navigation.txt navigates all the way to a real QUIT
# GAME + confirm, so this expects a genuine clean exit (exit 0), not a timeout. See that
# script's header for exactly what it does and why the coordinates/tick numbers are what
# they are.
#
# Also asserts POPMENUPIXELS_OUT actually captured something: this is the mechanism that
# would catch a real menu-rendering regression (see project_wasm_menu_alpha_blend_bug
# memory for the bug it already caught once, during this feature's own development).
#
# Usage: scripts/menu_mouse_navigation_test.sh

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$ROOT/target/debug/prince"
SCRIPT="$ROOT/scripts/scripted_inputs/menu_mouse_navigation.txt"

mkdir -p "$ROOT/target/debug"
ln -sfn "$ROOT/data"        "$ROOT/target/debug/data"        2>/dev/null || true
ln -sfn "$ROOT/replays"     "$ROOT/target/debug/replays"     2>/dev/null || true
ln -sfn "$ROOT/SDLPoP.ini"  "$ROOT/target/debug/SDLPoP.ini"  2>/dev/null || true

if [ ! -x "$BINARY" ]; then
  echo "menu_mouse_navigation_test: $BINARY not found -- run cargo build first" >&2
  exit 1
fi

log="$(mktemp)"
pixels="$(mktemp)"
trap 'rm -f "$log" "$pixels"' EXIT

echo "== menu_mouse_navigation: mouse-driven menu nav must exit cleanly and capture frames =="
timeout --kill-after=2 8 env POPTRACE_INPUT="$SCRIPT" POPMENUPIXELS_OUT="$pixels" "$BINARY" headless megahit 3 >"$log" 2>&1
code=$?

if [ "$code" -ne 0 ]; then
  echo "FAIL: expected a clean exit (0), got exit $code"
  echo "--- output ---"
  cat "$log"
  exit 1
fi

lines="$(wc -l < "$pixels" | tr -d ' ')"
if [ "$lines" -lt 3 ]; then
  echo "FAIL: expected at least 3 captured menu frames, got $lines"
  cat "$pixels"
  exit 1
fi

echo "PASS: exited cleanly, captured $lines menu frames"
exit 0
