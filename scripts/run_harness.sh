#!/usr/bin/env bash
# Differential harness for the Prince of Persia Rust port.
#
# Usage:
#   ./scripts/run_harness.sh               # compare Rust binary against all golden traces
#   ./scripts/run_harness.sh --regen       # regenerate all golden traces from C oracle
#   ./scripts/run_harness.sh --compare A B # diff two arbitrary trace files
#   ./scripts/run_harness.sh --one REPLAY GOLDEN  # run one replay/trace pair
#
# Golden traces are committed under traces/.
# They were generated from the all-C (cmake) build and are the reference oracle.
#

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Rust binary (cargo build output)
BINARY="$ROOT/target/debug/prince"
# C oracle binary (cmake/ninja build output) — used only for --regen
C_BINARY="$ROOT/prince"

COMPARE=(python3 "$ROOT/scripts/compare_traces.py")
IGNORE_FIELDS=()

# Registered replay/golden-trace pairs: "replay_path|golden_trace_path"
PAIRS=(
  "replays/run_right_and_die_lvl_1.p1r|traces/golden.trace"
  "doc/replays-testcases/Demo by Suave Prince level 11.p1r|traces/doc/Demo by Suave Prince level 11.trace"
  "doc/replays-testcases/Falling through floor (PR274).p1r|traces/doc/Falling through floor (PR274).trace"
  "doc/replays-testcases/Grab bug (PR288).p1r|traces/doc/Grab bug (PR288).trace"
  "doc/replays-testcases/Grab bug (PR289).p1r|traces/doc/Grab bug (PR289).trace"
  "doc/replays-testcases/Original level 12 xpos glitch.p1r|traces/doc/Original level 12 xpos glitch.trace"
  "doc/replays-testcases/Original level 2 falling into wall.p1r|traces/doc/Original level 2 falling into wall.trace"
  "doc/replays-testcases/Original level 5 shadow into wall.p1r|traces/doc/Original level 5 shadow into wall.trace"
  "doc/replays-testcases/SNES-PC-set level 11.p1r|traces/doc/SNES-PC-set level 11.trace"
  "doc/replays-testcases/trick_153.p1r|traces/doc/trick_153.trace"
)

mkdir -p "$ROOT/tmp" "$ROOT/traces/doc"
# The game chdir()s to exe_dir on replay load; symlink data/replays there so it
# can find assets and so POPTRACE_OUT absolute paths resolve correctly.
mkdir -p "$ROOT/target/debug"
ln -sf "$ROOT/data"        "$ROOT/target/debug/data"        2>/dev/null || true
ln -sf "$ROOT/replays"     "$ROOT/target/debug/replays"     2>/dev/null || true
ln -sf "$ROOT/SDLPoP.ini"  "$ROOT/target/debug/SDLPoP.ini"  2>/dev/null || true

run_one() {
  local replay="$ROOT/$1"
  local golden="$ROOT/$2"
  local name
  name=$(basename "$replay")

  if [ ! -f "$golden" ]; then
    echo "SKIP (no golden): $name"
    return 0
  fi

  local test="$ROOT/tmp/test.trace"
  rm -f "$test"
  POPTRACE_OUT="$test" "$BINARY" validate "$replay" >/dev/null 2>&1
  if [ ! -f "$test" ]; then
    echo "FAIL (no trace written): $name"
    return 1
  fi
  if "${COMPARE[@]}" "${IGNORE_FIELDS[@]}" "$golden" "$test"; then
    echo "PASS: $name"
    return 0
  else
    echo "FAIL: $name"
    return 1
  fi
}

regen_one() {
  local replay="$ROOT/$1"
  local golden="$ROOT/$2"
  mkdir -p "$(dirname "$golden")"
  echo "  Generating: $(basename "$golden")"
  POPTRACE_OUT="$golden" "$C_BINARY" validate "$replay" >/dev/null 2>&1
}

case "${1:-}" in
  --regen)
    echo "Regenerating all golden traces from C oracle ($C_BINARY)..."
    for pair in "${PAIRS[@]}"; do
      replay="${pair%%|*}"
      golden="${pair##*|}"
      regen_one "$replay" "$golden"
    done
    echo "Done."
    ;;
  --compare)
    "${COMPARE[@]}" "${IGNORE_FIELDS[@]}" "${2:?missing file A}" "${3:?missing file B}" "${@:4}"
    ;;
  --one)
    run_one "${2:?missing replay}" "${3:?missing golden}"
    ;;
  --build)
    echo "Building Rust binary..."
    cargo build --manifest-path "$ROOT/Cargo.toml" 2>&1
    ;;
  "")
    failures=0
    for pair in "${PAIRS[@]}"; do
      replay="${pair%%|*}"
      golden="${pair##*|}"
      run_one "$replay" "$golden" || failures=$((failures + 1))
    done
    echo ""
    if [ "$failures" -eq 0 ]; then
      echo "All ${#PAIRS[@]} replays passed."
    else
      echo "$failures of ${#PAIRS[@]} replays FAILED."
      exit 1
    fi
    ;;
  *)
    echo "Unknown argument: $1"
    exit 1
    ;;
esac
