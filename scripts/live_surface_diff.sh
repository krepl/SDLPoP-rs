#!/usr/bin/env bash
# Differential test for the *live* surface: runs one scripted-input scenario against both the
# C oracle and the Rust build and diffs the resulting state trace and pixel hashes.
#
# Why this exists: run_harness.sh proves the two builds agree on recorded *replays*, which
# exercise the simulation but never touch menus, pause, save/load or the live input path.
# This closes that gap using the same oracle, so live-surface behavior is held to the same
# standard as gameplay.
#
# Two things are load-bearing and were both learned the hard way (see
# docs/ux-audit-2026-08-16.md):
#
#   * `seed=<n>` is mandatory. Interactive runs otherwise seed the RNG from the clock and
#     two runs of the *same* build diverge at frame 1. Without this the whole comparison is
#     noise that looks exactly like a real divergence.
#   * The scenario must start inside a level (`headless megahit <level>`). The scripted-input
#     clock is state_dump::next_tick(), which only advances inside play_level_2's loop -- on
#     the title screen it is frozen at 0, so nothing scheduled past tick 0 ever fires and the
#     game hangs on the splash. Scripted input cannot drive the title/intro screens.
#
# Known limitation: scenarios that sit in the pause menu do not work here. draw_menu's inner
# loop blocks the outer per-tick loop, so dump_frame_state never runs, no trace is written,
# and POPTRACE_TICKS can never fire its auto-exit either -- the run just hits the timeout.
# Menu rendering is already covered differentially by cargo xtask menu-smoke-test /
# wasm-menu-smoke-test, which capture menu frames directly instead of relying on the tick
# loop. Use those for menu work and this for everything else.
#
# Usage: live_surface_diff.sh <scripted-input-file> [level] [ticks] [seed]

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="${1:?usage: live_surface_diff.sh <scripted-input-file> [level] [ticks] [seed]}"
LEVEL="${2:-3}"
TICKS="${3:-100}"
SEED="${4:-12345}"

C_BIN="$ROOT/prince"
RUST_BIN="$ROOT/target/debug/prince"

if [ ! -x "$C_BIN" ]; then
  echo "FAIL: C oracle not built at $C_BIN" >&2
  echo "  build it with:  mkdir -p c/build && cd c/build && cmake -G Ninja .. && ninja" >&2
  exit 1
fi
if [ ! -x "$RUST_BIN" ]; then
  echo "FAIL: Rust binary not built at $RUST_BIN (run: cargo build)" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# SDL's ALSA fallback blocks ~30s with no audio server; same workaround run_harness.sh uses.
export SDL_AUDIODRIVER=dummy

run_one() {
  local bin="$1" tag="$2" workdir="$3"
  ( cd "$workdir" && \
    POPTRACE_INPUT="$SCRIPT" \
    POPTRACE_OUT="$tmp/$tag.trace" \
    POPPIXELS_OUT="$tmp/$tag.pixels" \
    POPTRACE_TICKS="$TICKS" \
    timeout --kill-after=2 60 "$bin" "seed=$SEED" headless megahit "$LEVEL" >/dev/null 2>&1 )
  if [ ! -s "$tmp/$tag.trace" ]; then
    echo "FAIL: $tag produced no trace (did the scenario hang before reaching a level?)" >&2
    return 1
  fi
  return 0
}

# Each build runs from its own directory so neither picks up the other's relative assets.
run_one "$C_BIN" c "$ROOT" || exit 1
run_one "$RUST_BIN" rust "$ROOT/target/debug" || exit 1

status=0

if diff -q "$tmp/c.pixels" "$tmp/rust.pixels" >/dev/null; then
  echo "PASS (pixels): $(basename "$SCRIPT") -- rendering identical to the C oracle"
else
  echo "FAIL (pixel mismatch): $(basename "$SCRIPT")"
  diff "$tmp/c.pixels" "$tmp/rust.pixels" | head -10
  status=1
fi

if python3 "$ROOT/scripts/compare_traces.py" "$tmp/c.trace" "$tmp/rust.trace" >/dev/null 2>&1; then
  echo "PASS (state):  $(basename "$SCRIPT") -- $TICKS ticks, no divergence"
else
  echo "FAIL (state divergence): $(basename "$SCRIPT")"
  python3 "$ROOT/scripts/compare_traces.py" "$tmp/c.trace" "$tmp/rust.trace" 2>&1 | tail -20
  status=1
fi

exit $status
