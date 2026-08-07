# Blog notes: hunting two wasm-only rendering bugs

Draft notes for a future write-up. Not polished prose — just the narrative, the key
technical beats, and pointers to commits/screenshots/memory so writing the real post later
doesn't require re-deriving any of this. Covers the arc from "user notices a glitch while
playing" through "built a whole native-vs-wasm pixel-diffing toolchain" to "found and fixed
two real, unrelated rendering bugs."

## The setup

SDLPoP (Prince of Persia) had already been fully ported from C to Rust, and was running in
a browser via a Web Worker running the same, unmodified `pop_main()` game loop compiled to
wasm32. The differential test harness that had guarded the whole port so far only compared
game *state* (positions, frame numbers, HP, timers, ...) between the Rust build and a C
oracle build — never anything about what actually got drawn on screen. That gap turns out to
matter a lot once you're pushing pixels through a hand-rolled `WasmRenderer` instead of real
SDL2.

## The report

User played through level 1 in the browser and reported: climbing up and to the right onto a
ledge, there's a moment where the Kid renders *on top of* the ledge instead of behind it —
should be hidden by the floor lip he's pulling himself over. Small, easy to miss, but jarring
once you see it.

## First pass: it's not the port

Spent a long stretch reading `seg008.rs`'s draw-order code against the original C
byte-for-byte — draw order, tile-bucketing, overlay scheduling, all of it matched exactly. A
live hypothesis (same-tile draw-order inversion at a specific frame transition) got refuted by
actually instrumenting the running game and checking real tile-bucket values across 228 real
climb-frame instances in a full playthrough. Zero coincidences. Dead end, but a *productive*
dead end — it ruled out the shared game logic entirely.

## The real breakthrough: pixel-diffing against the C oracle

Rather than keep reading code, built a way to *see* the actual rendered output: a temporary
screenshot-dump hook (`get_final_surface()` → PNG) added to both the Rust build and a
freshly-rebuilt C oracle, run against the same replay, same ticks. Result: **zero pixel
differences** across 141 rightward-climb frames. The Rust port's rendering matched the C
reference exactly, everywhere. That fully cleared the port of suspicion — if the bug was
real, it had to be a wasm-only rendering issue, not anything shared.

*(Screenshot: `2026-08-03-wasm-climb-zorder-bug/`)*

## Building a permanent pixel-hash regression harness

Turned the throwaway screenshot-dump spike into real infrastructure: `dump_frame_pixels`
(an FNV-1a hash of the rendered frame, once per tick) mirrored in both the Rust and C builds,
wired into the existing differential harness so every one of the 30 golden replays now gets a
committed golden `.pixels` file alongside its state trace. Free regression coverage for
rendering bugs going forward, not just this one investigation. All 30 replays: zero pixel
divergence between Rust-native and the C oracle.

## Teaching the wasm build to run real replays

The wasm build only ever took *live* keyboard input — there was no way to feed it one of the
existing `.p1r` replay files and get deterministic, comparable output. Built that from
scratch: `run_game_with_args` (pass real argv, so JS can say `["prince", "validate",
"<path>"]` the same way the native harness does), a fake `getenv`/`setenv` so the existing
`POPTRACE_OUT`/`POPPIXELS_OUT` diagnostics work unchanged, and a `read_vfs_file` bridge to
pull the results back out to JS. A `fflush` bug had to be fixed along the way too — it was a
no-op stub, so nothing ever actually reached the virtual filesystem (a validated replay ends
by calling C's `exit()`, which on wasm throws a catchable JS error rather than ever calling
`fclose`, so the "write on close" path never ran).

A Node + Playwright driver (`scripts/wasm_pixel_harness.mjs`) then runs one replay through a
headless Chromium tab and writes the resulting trace/pixel-hash dumps back to disk for
comparison against the same goldens the native harness already uses.

## Bug #1: the clip-rect bug

Running `lvl01_complete.p1r` through the wasm build for the first time: state trace matched
byte-for-byte, but the pixel trace diverged at the start of *every single* climb-up
animation. Captured the raw pixels at one such tick and diffed against native: a small stray
fragment of skin-tone and gold pixels floating above the ledge in the wasm frame — pixels the
native frame simply didn't have.

Root cause: the game clips a character sprite to hide the part that shouldn't be visible yet
(the classic "reaching arm not yet fully over the ledge" pose) by setting a clip rectangle on
the destination surface before the blit. `WasmRenderer`'s blit function stored clip rects
when asked, but never actually *checked* one when drawing — it only clipped against the
surface's raw bounds. So on wasm, every clipped sprite drew fully unclipped, bleeding through
onto whatever it should have been hidden behind. An 11-line fix. All 7 climb-start
divergences in the replay vanished.

## Bug #2: the RGB24 channel-swap bug

While spot-checking a second replay for confidence, `lvl04_mirror_complete.p1r` (level 4, the
mirror level) turned up something much bigger: pixel divergence from *tick 0* through nearly
the entire 4989-tick replay. A raw capture showed the level's floor/wall area — a mottled tan
brick pattern natively — rendering as a solid blue checkerboard in wasm instead. Same
spatial pattern, completely wrong colors.

This took more digging. First ruled out the game's own in-memory color palette (dumped it
from both builds, byte-identical). The actual cause: years earlier, someone had discovered
some real SDL builds get the byte order wrong on 24-bit color surfaces, and added a
self-test — create a throwaway probe surface, fill it pure red, check whether the red
channel landed where expected, and if not, permanently swap red and blue on every future fill
to compensate. That probe surface is created by asking for "sensible default channel
layout," which real SDL knows how to provide. The wasm renderer's surface-creation code
didn't — it just stored the literal "no channels at all" values it was given, which made the
self-test structurally unable to ever pass. So the fix-for-a-bug-that-doesn't-exist kicked in
on every single wasm run, silently swapping red and blue on every flat-color fill (level
wall/floor colors, the damage-flash effect, screen fades — anything that wasn't a straight
sprite copy).

Fixed by making wasm's surface creation pick the same sensible defaults real SDL would.
`lvl04_mirror_complete.p1r`: 4502 diverging ticks → zero.

*(Screenshots: `2026-08-04-wasm-rgb24-mask-bug/`)*

## Closing the loop: a full-suite sweep

Ran all 30 golden replays through the wasm build (previously only 4 had been spot-checked).
Turned up one more thing — not a rendering bug, a gap in the test driver itself: one replay's
in-game timer runs out mid-level, triggering an ordinary restart, which the headless driver
didn't know how to handle (it only expected "replay finished cleanly"). Fixed with the same
retry-on-restart loop the live interactive build already had.

Final tally: 29 of 30 replays match the native/C-oracle golden with *zero* pixel divergence.
The one holdout has narrowed to a single isolated pixel — cosmetic, low priority, logged for
later.

## Threads for later

- Two single-pixel color mismatches (one in `lvl01_complete.p1r`, one in
  `lvl12_13_complete.p1r`) remain unexplained — tiny, easy to miss, deliberately deprioritized.
- The wasm pixel-comparison toolchain built here (`scripts/wasm_pixel_harness.mjs`,
  `dump_frame_raw` for single-frame captures) is reusable for whatever comes up next.
- Possible narrative hook for the post: "the test suite said everything was fine the whole
  time" — a state-only differential harness can prove your *simulation* is right while your
  *renderer* silently lies to every player, and nobody would know without actually looking at
  pixels.

## References

- Commits: `10645f2` (wasm replay support), `8820729` (clip-rect fix), `13bbfd7` (RGB24 mask
  fix), `45ee1f3` (headless driver restart handling), `8f0dd9e`/`47fca59` (pixel-hash
  harness).
- Memory: `project_climb_zorder_bug`, `project_wasm_rgb24_bug_check_mask_bug`,
  `project_wasm_1px_residual_bug`.
- `CLAUDE.md`'s "Headless replay support (wasm)" section for the toolchain's usage.
