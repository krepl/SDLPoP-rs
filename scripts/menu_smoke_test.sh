#!/usr/bin/env bash
# Regression test: opening the pause menu must not crash.
#
# Nothing else in the harness ever opens the pause menu -- run_harness.sh's replays
# never press Escape, and gameplay_smoke_test.sh only drives movement. This is what
# would have caught the wasm build's Esc-menu crash (WasmRenderer::get_window_flags
# was an unimplemented!() stub, hit for the first time by a real user pressing Esc --
# see the project_wasm_esc_menu_crash memory / commit d20c68e). This script only
# covers the *native* build; see scripts/wasm_menu_smoke_test.mjs for the wasm one,
# which is the side that actually caught the real bug (native's SDL already
# implements everything the menu needs, so this native run was never at risk --
# it exists for symmetry and because scripted-input itself is worth smoke-testing).
#
# do_paused()'s own inner loop (draw_menu) blocks the outer per-tick loop for the
# whole time the menu is open, polling for real input every iteration -- with no
# further scripted input queued (see scripts/scripted_inputs/open_menu.txt's header
# comment for why a scripted "close" isn't feasible), it just hangs peacefully
# forever, the same way it would with a real keyboard and no keypress. So the pass
# condition here is inverted from a normal test: running to the timeout WITHOUT
# crashing is success; exiting early (a crash) or exiting cleanly (unexpected -- got
# closed somehow) are both failures.
#
# Usage: scripts/menu_smoke_test.sh

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$ROOT/target/debug/prince"
SCRIPT="$ROOT/scripts/scripted_inputs/open_menu.txt"

mkdir -p "$ROOT/target/debug"
ln -sfn "$ROOT/data"        "$ROOT/target/debug/data"        2>/dev/null || true
ln -sfn "$ROOT/replays"     "$ROOT/target/debug/replays"     2>/dev/null || true
ln -sfn "$ROOT/SDLPoP.ini"  "$ROOT/target/debug/SDLPoP.ini"  2>/dev/null || true

if [ ! -x "$BINARY" ]; then
  echo "menu_smoke_test: $BINARY not found -- run cargo build first" >&2
  exit 1
fi

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

echo "== open_menu: pressing Escape must not crash =="
timeout --kill-after=2 6 env POPTRACE_INPUT="$SCRIPT" "$BINARY" headless megahit 3 >"$log" 2>&1
code=$?

if [ "$code" -eq 124 ] || [ "$code" -eq 137 ]; then
  echo "PASS: menu opened and stayed open for the timeout window without crashing"
  exit 0
else
  echo "FAIL: expected the run to hang open (timeout exit 124), got exit $code"
  echo "--- output ---"
  cat "$log"
  exit 1
fi
