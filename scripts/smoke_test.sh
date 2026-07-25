#!/usr/bin/env bash
# Smoke test: launches the actual game binary via its normal interactive
# startup path (NOT `prince validate`) and confirms it runs for a few seconds
# without panicking or crashing.
#
# Why this exists: scripts/run_harness.sh's `validate` replay mode skips
# window creation ("run without a window if validating a replay") and most of
# the live input-polling path, so bugs that only trigger during the real
# interactive startup are invisible to it. Case in point: commit 4a11238 fixed
# a startup panic ("no event pump (SdlPlatform not constructed yet)") in
# key_state(), called every frame from the live input path -- 30/30 harness
# replays passed the entire time that bug existed, because validate mode
# never reached the code path that panicked.
#
# Usage:
#   scripts/smoke_test.sh [duration_seconds]   # default 5s

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$ROOT/target/debug/prince"
DURATION="${1:-5}"

mkdir -p "$ROOT/target/debug"
ln -sfn "$ROOT/data"        "$ROOT/target/debug/data"        2>/dev/null || true
ln -sfn "$ROOT/replays"     "$ROOT/target/debug/replays"     2>/dev/null || true
ln -sfn "$ROOT/SDLPoP.ini"  "$ROOT/target/debug/SDLPoP.ini"  2>/dev/null || true

if [ ! -x "$BINARY" ]; then
  echo "smoke_test: $BINARY not found -- run cargo build first" >&2
  exit 1
fi

out="$(mktemp)"
trap 'rm -f "$out"' EXIT

# `headless` (seg000.rs's pop_main, not part of the original C) sets SDL's dummy
# video/audio drivers from inside the binary itself -- no real display needed, but
# still exercises real window/renderer/event-pump creation (unlike validate mode,
# which skips the window entirely). Self-contained: doesn't depend on whoever
# launches this script remembering to set SDL_VIDEODRIVER/SDL_AUDIODRIVER first.
timeout --kill-after=2 "$DURATION" "$BINARY" headless >"$out" 2>&1
status=$?

# timeout's exit codes for "still running when killed" (124 on SIGTERM, 137 if
# --kill-after's SIGKILL was needed) mean the game was alive and looping --
# that's the expected/successful outcome for an interactive program with no
# input. Anything else is a real, unexpected exit; in particular 134 is
# SIGABRT, which is what a Rust panic-in-a-non-unwinding-context produces.
if [ "$status" -eq 124 ] || [ "$status" -eq 137 ] || [ "$status" -eq 0 ]; then
  if grep -qi 'panicked\|segmentation fault\|SIGABRT\|SIGSEGV' "$out"; then
    echo "FAIL: $BINARY printed a panic/crash within ${DURATION}s:"
    cat "$out"
    exit 1
  fi
  echo "PASS: $BINARY ran for ${DURATION}s without crashing"
  exit 0
else
  echo "FAIL: $BINARY exited with status $status within ${DURATION}s:"
  cat "$out"
  exit 1
fi
