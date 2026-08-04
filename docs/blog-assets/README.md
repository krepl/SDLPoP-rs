# Blog assets

Screenshots and other artifacts worth keeping around for a future write-up about this
project's Rust port and wasm/browser work. Not part of the game or its build — this
directory exists purely so interesting investigation moments (bugs found, before/after
comparisons, etc.) survive past the session that produced them, since scratch files under
`/tmp` don't.

One subdirectory per investigation/milestone, named `YYYY-MM-DD-short-slug/`. Each
subdirectory should have enough in its filenames (or a short `NOTES.md` if it needs more
context) to be usable later without re-reading the original conversation.

## Index

- `2026-08-03-wasm-climb-zorder-bug/` — native vs. wasm rendering diff at the exact tick a
  rightward climb-up animation starts. `wasm-tick890.png` has a stray sprite-fragment
  artifact (a few skin-tone/gold pixels floating above the ledge) that
  `native-tick890.png` does not; the `-zoom.png` variants are a cropped, nearest-neighbor
  8x zoom of the differing region. Found via the pixel-hash regression harness
  (`traces/pixels/`) plus the wasm headless replay driver (`scripts/wasm_pixel_harness.mjs`,
  `web/headless.mjs`) — see `CLAUDE.md`'s "Headless replay support (wasm)" section for how
  these were generated, and commit `10645f2` for the infrastructure that made it possible.

- `2026-08-04-wasm-rgb24-mask-bug/` — level 4 (the mirror level, a "palace"-type level) at
  tick 0: `native-tick0.png` shows the correct tan mottled-brick floor/wall, `wasm-tick0-
  before-fix.png` shows the same scene with that whole area rendered as a blue checkerboard
  instead — same pattern, wrong color, because `WasmRenderer::create_surface` was breaking a
  real-SDL-quirk workaround (`RGB24_bug_check`) and unconditionally swapping the R/B channels
  on 24bpp fills. Fixed in commit `13bbfd7`; see `project_wasm_rgb24_bug_check_mask_bug`
  memory for the full root-cause writeup.
