#!/usr/bin/env bash
# Differential harness for the Prince of Persia Rust port.
#
# Usage:
#   ./scripts/run_harness.sh               # compare current binary against golden trace
#   ./scripts/run_harness.sh --regen       # regenerate the golden trace
#   ./scripts/run_harness.sh --compare A B # diff two arbitrary trace files
#
# The golden trace is committed at tmp/golden.trace.
# It was generated from the all-C build and is the reference for all future comparisons.

set -euo pipefail

BINARY="./prince"
REPLAY="replays/run_right_and_die_lvl_1.p1r"
GOLDEN="traces/golden.trace"
TEST="tmp/test.trace"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
COMPARE=(python3 "$SCRIPT_DIR/compare_traces.py")

mkdir -p tmp

case "${1:-}" in
  --regen)
    echo "Regenerating golden trace..."
    POPTRACE_OUT="$GOLDEN" "$BINARY" validate "$REPLAY"
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
    echo "Running binary..."
    POPTRACE_OUT="$TEST" "$BINARY" validate "$REPLAY"
    echo "Comparing against golden..."
    "${COMPARE[@]}" "$GOLDEN" "$TEST"
    ;;
  *)
    echo "Unknown argument: $1"
    exit 1
    ;;
esac
