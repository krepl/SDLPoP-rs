#!/usr/bin/env bash
# Differential harness for the Prince of Persia Rust port.
#
# Usage:
#   ./scripts/run_harness.sh               # compare Rust binary against golden trace
#   ./scripts/run_harness.sh --regen       # regenerate golden trace from C oracle
#   ./scripts/run_harness.sh --compare A B # diff two arbitrary trace files
#
# The golden trace is committed at traces/golden.trace.
# It was generated from the all-C (cmake) build and is the reference oracle.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Rust binary (cargo build output)
BINARY="$ROOT/target/debug/prince"
# C oracle binary (cmake/ninja build output) — used only for --regen
C_BINARY="$ROOT/src/build/prince"
REPLAY="$ROOT/replays/run_right_and_die_lvl_1.p1r"
GOLDEN="$ROOT/traces/golden.trace"
TEST="$ROOT/tmp/test.trace"
COMPARE=(python3 "$ROOT/scripts/compare_traces.py")

mkdir -p "$ROOT/tmp"
# The game chdir()s to exe_dir on replay load; symlink data/replays there so it
# can find assets and so POPTRACE_OUT absolute paths resolve correctly.
mkdir -p "$ROOT/target/debug"
ln -sf "$ROOT/data"    "$ROOT/target/debug/data"    2>/dev/null || true
ln -sf "$ROOT/replays" "$ROOT/target/debug/replays" 2>/dev/null || true

case "${1:-}" in
  --regen)
    echo "Regenerating golden trace from C oracle ($C_BINARY)..."
    POPTRACE_OUT="$GOLDEN" "$C_BINARY" validate "$REPLAY"
    echo "Golden trace written to $GOLDEN"
    ;;
  --compare)
    "${COMPARE[@]}" "${2:?missing file A}" "${3:?missing file B}" "${@:4}"
    ;;
  "")
    if [ ! -f "$GOLDEN" ]; then
      echo "No golden trace found at $GOLDEN. Run with --regen first."
      exit 1
    fi
    rm -f "$TEST"
    echo "Running Rust binary..."
    POPTRACE_OUT="$TEST" "$BINARY" validate "$REPLAY"
    if [ ! -f "$TEST" ]; then
      echo "ERROR: trace file was not written — POPTRACE_OUT failed"
      exit 1
    fi
    echo "Comparing against golden..."
    "${COMPARE[@]}" "$GOLDEN" "$TEST"
    ;;
  *)
    echo "Unknown argument: $1"
    exit 1
    ;;
esac
