#!/usr/bin/env bash
# Gameplay smoke test: drives real headless gameplay via scripted keyboard
# input (POPTRACE_INPUT, see process_events() in seg009.rs) and asserts on
# the resulting Kid.x trajectory in the trace.
#
# Why this exists: smoke_test.sh only checks "didn't crash", and
# run_harness.sh's replay comparisons feed input from a canned .p1r file, not
# SDL's real event queue. This exercises the actual live input path
# end-to-end: injected SDL_KEYDOWN/KEYUP -> process_events() -> key_states ->
# control_kid() -> real Kid movement -- the same path a real keyboard uses,
# which validate mode never touches.
#
# Usage: scripts/gameplay_smoke_test.sh

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$ROOT/target/debug/prince"
INPUT_DIR="$ROOT/scripts/scripted_inputs"

mkdir -p "$ROOT/target/debug"
ln -sfn "$ROOT/data"        "$ROOT/target/debug/data"        2>/dev/null || true
ln -sfn "$ROOT/replays"     "$ROOT/target/debug/replays"     2>/dev/null || true
ln -sfn "$ROOT/SDLPoP.ini"  "$ROOT/target/debug/SDLPoP.ini"  2>/dev/null || true

if [ ! -x "$BINARY" ]; then
  echo "gameplay_smoke_test: $BINARY not found -- run cargo build first" >&2
  exit 1
fi

trace="$(mktemp)"
trap 'rm -f "$trace"' EXIT
fail=0

# Runs a scripted-input scenario against level 3 (a flat, obstacle-free
# starting stretch, verified by hand) and writes the trace to $trace.
run_scenario() {
  local script="$1" ticks="$2"
  rm -f "$trace"
  POPTRACE_INPUT="$INPUT_DIR/$script" POPTRACE_OUT="$trace" POPTRACE_TICKS="$ticks" \
    timeout --kill-after=2 20 "$BINARY" headless megahit 3 >/dev/null 2>&1
  if [ ! -s "$trace" ]; then
    echo "FAIL: $script produced no trace"
    fail=1
    return 1
  fi
  return 0
}

kid_x() {
  python3 "$ROOT/scripts/gameplay_checks.py" "$trace" "$@"
}

echo "== walk_right: holding Right should move the Kid =="
if run_scenario "walk_right.txt" 25; then
  read -r x0 x24 <<<"$(kid_x 0 24)"
  moved=$((x24 - x0))
  if [ "$moved" -ge 40 ]; then
    echo "PASS: Kid.x moved ${moved}px over 24 ticks ($x0 -> $x24)"
  else
    echo "FAIL: Kid.x only moved ${moved}px over 24 ticks ($x0 -> $x24), expected >= 40"
    fail=1
  fi
fi

echo "== walk_right_then_stop: releasing Right should stop the Kid =="
if run_scenario "walk_right_then_stop.txt" 40; then
  read -r x0 x15 x20 x39 <<<"$(kid_x 0 15 20 39)"
  moved=$((x15 - x0))
  settle=$((x39 - x20))
  settle_abs=${settle#-}
  if [ "$moved" -ge 20 ] && [ "$settle_abs" -le 3 ]; then
    echo "PASS: Kid moved ${moved}px while Right held ($x0 -> $x15), then stopped (drift ${settle}px after release)"
  else
    echo "FAIL: moved=${moved}px (want >= 20), post-release drift=${settle}px (want <= 3 abs)"
    fail=1
  fi
fi

exit $fail
