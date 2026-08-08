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
# Each replay also gets a golden *pixel*-hash file under traces/pixels/ (mirrored
# path, .pixels extension): one FNV-1a hash per tick of the actual rendered
# surface (see rust/src/state_dump.rs / src/state_dump.c, dump_frame_pixels).
# The state trace above only proves game *state* matches; this proves the pixels
# drawn from that state match too -- catching rendering-only regressions the
# state trace is blind to (see docs: the climb-animation z-order investigation
# that motivated this). Regenerated/compared automatically alongside the state
# trace, same replay run, no extra cost.
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
  "doc/replays-testcases/run_right_and_die_lvl_1.p1r|traces/golden.trace"
  "doc/replays-testcases/Demo by Suave Prince level 11.p1r|traces/doc/Demo by Suave Prince level 11.trace"
  "doc/replays-testcases/Falling through floor (PR274).p1r|traces/doc/Falling through floor (PR274).trace"
  "doc/replays-testcases/Grab bug (PR288).p1r|traces/doc/Grab bug (PR288).trace"
  "doc/replays-testcases/Grab bug (PR289).p1r|traces/doc/Grab bug (PR289).trace"
  "doc/replays-testcases/Original level 12 xpos glitch.p1r|traces/doc/Original level 12 xpos glitch.trace"
  "doc/replays-testcases/Original level 2 falling into wall.p1r|traces/doc/Original level 2 falling into wall.trace"
  "doc/replays-testcases/Original level 5 shadow into wall.p1r|traces/doc/Original level 5 shadow into wall.trace"
  "doc/replays-testcases/SNES-PC-set level 11.p1r|traces/doc/SNES-PC-set level 11.trace"
  "doc/replays-testcases/trick_153.p1r|traces/doc/trick_153.trace"
  # lvlN_* replays, sorted by level number
  "doc/replays-testcases/lvl01_complete.p1r|traces/doc/lvl01_complete.trace"
  "doc/replays-testcases/lvl02_poison_complete.p1r|traces/doc/lvl02_poison_complete.trace"
  "doc/replays-testcases/lvl03_skeleton_complete.p1r|traces/doc/lvl03_skeleton_complete.trace"
  "doc/replays-testcases/lvl04_mirror_complete.p1r|traces/doc/lvl04_mirror_complete.trace"
  "doc/replays-testcases/lvl05_shadow_steal_complete.p1r|traces/doc/lvl05_shadow_steal_complete.trace"
  "doc/replays-testcases/lvl06_shadow_step_fatguard_complete.p1r|traces/doc/lvl06_shadow_step_fatguard_complete.trace"
  "doc/replays-testcases/lvl07_feather_complete.p1r|traces/doc/lvl07_feather_complete.trace"
  "doc/replays-testcases/lvl08_mouse_gate_complete.p1r|traces/doc/lvl08_mouse_gate_complete.trace"
  "doc/replays-testcases/lvl08_death_2.p1r|traces/doc/lvl08_death_2.trace"
  "doc/replays-testcases/lvl09_invert_complete.p1r|traces/doc/lvl09_invert_complete.trace"
  "doc/replays-testcases/lvl10_complete.p1r|traces/doc/lvl10_complete.trace"
  "doc/replays-testcases/lvl10_prince_disappears_bug.p1r|traces/doc/lvl10_prince_disappears_bug.trace"
  "doc/replays-testcases/lvl11_complete.p1r|traces/doc/lvl11_complete.trace"
  "doc/replays-testcases/lvl12_13_complete.p1r|traces/doc/lvl12_13_complete.trace"
  "doc/replays-testcases/lvl14_complete.p1r|traces/doc/lvl14_complete.trace"
  "doc/replays-testcases/time_limit_expiry_lvl3.p1r|traces/doc/time_limit_expiry_lvl3.trace"
  "doc/replays-testcases/long_fall_death.p1r|traces/doc/long_fall_death.trace"
  "doc/replays-testcases/impalement_death_lvl1.p1r|traces/doc/impalement_death_lvl1.trace"
  "doc/replays-testcases/running_impalement_lvl6.p1r|traces/doc/running_impalement_lvl6.trace"
  "doc/replays-testcases/chomper_death_lvl7.p1r|traces/doc/chomper_death_lvl7.trace"
)

# Golden trace path -> golden pixel-hash path: traces/... -> traces/pixels/...,
# .trace -> .pixels. e.g. traces/golden.trace -> traces/pixels/golden.pixels,
# traces/doc/foo.trace -> traces/pixels/doc/foo.pixels.
pixels_path_for() {
  local trace="$1"
  trace="${trace/traces\//traces/pixels/}"
  echo "${trace%.trace}.pixels"
}

mkdir -p "$ROOT/tmp" "$ROOT/traces/doc" "$ROOT/traces/pixels/doc"
# The game chdir()s to exe_dir on replay load; symlink data/replays there so it
# can find assets and so POPTRACE_OUT absolute paths resolve correctly.
mkdir -p "$ROOT/target/debug"
# Use -n (--no-dereference) so re-runs replace the existing symlink instead of
# descending into it and creating a stray self-link (e.g. replays/replays).
ln -sfn "$ROOT/data"        "$ROOT/target/debug/data"        2>/dev/null || true
ln -sfn "$ROOT/replays"     "$ROOT/target/debug/replays"     2>/dev/null || true
ln -sfn "$ROOT/SDLPoP.ini"  "$ROOT/target/debug/SDLPoP.ini"  2>/dev/null || true

run_one() {
  local replay="$ROOT/$1"
  local golden="$ROOT/$2"
  local name
  name=$(basename "$replay")

  if [ ! -f "$golden" ]; then
    echo "SKIP (no golden): $name"
    return 0
  fi
  # A missing replay makes 'prince validate' drop to the interactive title
  # screen and block forever waiting for input. Skip instead of hanging.
  if [ ! -f "$replay" ]; then
    echo "SKIP (no replay): $name"
    return 0
  fi

  # Per-replay filenames (not a shared "test.trace"/"test.pixels") -- run_one is called
  # concurrently by the parallel loop below, and two runs sharing one output path would
  # clobber each other's trace mid-write.
  local safe_name
  safe_name="$(echo "$name" | tr -c 'A-Za-z0-9._-' '_')"
  local test="$ROOT/tmp/${safe_name}.trace"
  local test_pixels="$ROOT/tmp/${safe_name}.pixels"
  local golden_pixels
  golden_pixels="$(pixels_path_for "$golden")"
  rm -f "$test" "$test_pixels"
  # SDL_AUDIODRIVER=dummy: the harness compares state traces, which audio never
  # affects, so we never want a real audio device here. Deliberately applied on
  # ALL platforms (not gated on WSL): whenever SDL can't reach a working audio
  # server it falls back to the ALSA backend, whose init blocks ~30s timing out
  # ("cannot find card '0'") before failing — which looks exactly like a hang.
  # This bites headless/CI runs and any reduced shell env that lacks the desktop
  # session's PulseAudio vars, even on a box whose interactive audio works fine.
  # The dummy driver sidesteps all of it. Do NOT wrap this in a WSL check.
  # timeout guards against any future replay that hangs (missing/corrupt input).
  # 60s is a hang backstop, not a perf budget: current replays run in <3s, so 60s
  # is fail-fast with zero false-fail risk. Do NOT go lower.
  #
  # When to revisit: only once the replay set spans a WIDE runtime range — e.g. a
  # 30-60s full-game replay alongside sub-second ones. At that point a single
  # constant can't be both tight-for-short and safe-for-long (a 35s run on a 3x-
  # slower CI box is ~90s, past any sane fixed value). Don't just bump the constant
  # to 120 — that re-loses fail-fast for the short replays. Instead scale the
  # timeout per-replay off the GOLDEN TRACE size (one fixed record per tick, so
  # bytes ∝ frames ∝ runtime). NOTE: scale off the trace, not the .p1r — the .p1r
  # is header-dominated and event-encoded (stores input changes), so its size
  # barely tracks runtime: run_right_and_die is 4125B/263 frames while
  # lvl01_complete is 7623B/3761 frames (2x the file, 14x the frames).
  timeout 60 env SDL_AUDIODRIVER=dummy POPTRACE_OUT="$test" POPPIXELS_OUT="$test_pixels" \
    "$BINARY" validate "$replay" >/dev/null 2>&1
  if [ ! -f "$test" ]; then
    echo "FAIL (no trace written): $name"
    return 1
  fi
  if ! "${COMPARE[@]}" "${IGNORE_FIELDS[@]}" "$golden" "$test"; then
    echo "FAIL: $name"
    return 1
  fi
  if [ -f "$golden_pixels" ]; then
    if [ ! -f "$test_pixels" ]; then
      echo "FAIL (no pixel trace written): $name"
      return 1
    fi
    if ! diff -q "$golden_pixels" "$test_pixels" >/dev/null; then
      local first_diff
      first_diff=$(diff "$golden_pixels" "$test_pixels" | head -1 || true)
      echo "FAIL (pixel mismatch): $name -- $first_diff"
      return 1
    fi
  fi
  echo "PASS: $name"
  return 0
}

regen_one() {
  local replay="$ROOT/$1"
  local golden="$ROOT/$2"
  local golden_pixels
  golden_pixels="$ROOT/$(pixels_path_for "$2")"
  mkdir -p "$(dirname "$golden")" "$(dirname "$golden_pixels")"
  echo "  Generating: $(basename "$golden")"
  SDL_AUDIODRIVER=dummy POPTRACE_OUT="$golden" POPPIXELS_OUT="$golden_pixels" \
    "$C_BINARY" validate "$replay" >/dev/null 2>&1
}

# Runs $1 (a function name, either run_one or regen_one) over every pair in PAIRS,
# concurrently, bounded to $JOBS at a time -- each replay run is an independent process
# (own binary invocation, own trace/pixel output files since run_one's naming fix above),
# so there's no correctness reason to run them one at a time, only a historical one. Each
# job's stdout/stderr is captured to a temp file so concurrent output doesn't interleave
# mid-line; results are printed back in PAIRS order once everything finishes, so output
# reads identically to the old sequential run. Exit statuses are collected the same way
# (via each job's own exit-code file), not via `wait`'s own status, since `set -e` would
# otherwise abort the whole script on the first background failure.
JOBS="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)"

run_parallel() {
  local fn="$1"
  local -a outs=() codes=()
  local i=0 running=0
  for pair in "${PAIRS[@]}"; do
    replay="${pair%%|*}"
    golden="${pair##*|}"
    local out="$ROOT/tmp/parallel_${i}.out"
    local code="$ROOT/tmp/parallel_${i}.code"
    outs+=("$out")
    codes+=("$code")
    (
      set +e
      "$fn" "$replay" "$golden" >"$out" 2>&1
      echo $? >"$code"
    ) &
    i=$((i + 1))
    running=$((running + 1))
    if [ "$running" -ge "$JOBS" ]; then
      wait -n || true
      running=$((running - 1))
    fi
  done
  wait

  local failures=0
  for idx in "${!outs[@]}"; do
    cat "${outs[$idx]}"
    if [ "$(cat "${codes[$idx]}")" != "0" ]; then
      failures=$((failures + 1))
    fi
    rm -f "${outs[$idx]}" "${codes[$idx]}"
  done
  return "$failures"
}

case "${1:-}" in
  --regen)
    echo "Regenerating all golden traces from C oracle ($C_BINARY), $JOBS at a time..."
    run_parallel regen_one || true
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
    # Smoke test first (fail fast): the replay comparisons below all run through
    # `prince validate`, which skips window creation and most of the live input
    # path -- see scripts/smoke_test.sh's header comment for why that's a real
    # coverage gap, not redundant with what follows.
    "$ROOT/scripts/smoke_test.sh" || exit 1
    echo ""

    # Same coverage-gap rationale as smoke_test.sh, one step further: proves
    # the live input path doesn't just avoid crashing, it actually drives the
    # Kid the way a real keyboard would. See gameplay_smoke_test.sh's header.
    "$ROOT/scripts/gameplay_smoke_test.sh" || exit 1
    echo ""

    # Opening the pause menu specifically -- none of the above ever does. See
    # menu_smoke_test.sh's header for why this exists (native was never actually
    # at risk; the wasm counterpart, scripts/wasm_menu_smoke_test.mjs, is the one
    # that caught the real bug, but needs Node/Playwright so it isn't run here --
    # see CLAUDE.md's wasm testing section).
    "$ROOT/scripts/menu_smoke_test.sh" || exit 1
    echo ""

    # Mouse-driven menu navigation actually working, not just "doesn't crash" -- see
    # menu_mouse_navigation_test.sh's header. Same Node/Playwright caveat as above for
    # the wasm counterpart (wasm_menu_mouse_navigation_test.mjs).
    "$ROOT/scripts/menu_mouse_navigation_test.sh" || exit 1
    echo ""

    echo "Comparing ${#PAIRS[@]} replays, $JOBS at a time..."
    failures=0
    run_parallel run_one || failures=$?
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
