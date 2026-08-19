# Deferred work register

Everything consciously parked, with *why*, so none of it survives only as tribal knowledge or
in a task list. Reviewed and re-prioritised whenever; nothing here is blocking.

Last updated 2026-08-19.

## Waiting on Seth

| Item | Detail |
|---|---|
| **Native bug-hunt pass** | `docs/native-bug-hunt-checklist.md`. ~20 min. The only untested surface left: the native build's live window/keyboard/timing. Automation structurally cannot cover "feels wrong". |
| **Design call: none outstanding** | The fullscreen/Esc conflict was the one open design question and it's resolved (`b0802fc`). |

## Deferred by decision

| Item | Why parked | Cost to resume |
|---|---|---|
| **#48 Controls discoverability** | Your call: nice-to-have, low priority. A partial down payment shipped in `b0802fc` (in-page controls reference replacing the dev-harness copy). The real thing is an in-game help overlay. | Small |
| **#37 Residual 1px mismatch + bit-exact alpha blend** | Your explicit lowest priority. Cosmetic; no functional impact. Details in `project_wasm_1px_residual_bug` memory. | Medium, fiddly |
| **#35 Safe reference accessors on wasm Renderer** | Genuinely blocked: Step D de-globalisation hasn't reached `platform/sdl.rs`/`wasm.rs`, which still use a raw `static mut SHARED_RENDERER`. Doing it before then means doing it twice. | Blocked |
| **#39 Abstract SDL_Color/Palette/PixelFormat** | Follow-up to the `SDL_Rect` work (`8943e5a`). Riskier than Rect was: Color is coupled to Palette's bindgen-fixed layout and PixelFormat is genuinely opaque. No consumer needs it yet. | Medium-large |
| **Mobile / touch controls** | Classified as a new feature, not a quality fix, so it lost to the quality pass. Needs viewport meta, responsive canvas, on-screen D-pad. | Large |
| **Plan 12 Phase 4 — CI fuzzing** | Was blocked on having no CI at all; that's fixed (`.github/workflows/ci.yml`). Now blocked on a corpus, which the game-beater is meant to produce. | Medium, after game-beater |
| **Plan 12 Phase 5 — audio port verification** | Still genuinely open. `opl3.rs`/`midi.rs` have no tests and no sample-checksum trace field. Worth doing if the native pass reports anything odd about sound. | Small-medium |
| **Plan 13 deferrals** | `advance_one_frame()`/setjmp removal (~20 call sites), `FileSystem` trait migration, `winit`/`pixels` presentation swap. All explicitly "deferred, not scheduled" in the plan; none blocking. | Large |
| **WebKit/Safari browser testing** | Playwright's WebKit build needs ~15 system libraries this machine lacks; installing needs sudo, so I didn't do it unprompted. Safari has no Keyboard Lock, so it takes the same P/Backspace pause path as Firefox — reasoned, not verified. | Small, needs sudo |

## Known-and-accepted behaviours (do NOT "fix")

Confirmed byte-identical to the C oracle. Changing any of these would violate the port's
no-behaviour-changes directive. Recorded because each one *looks* like a bug.

- **Kid isn't drawn after a quickload** until the player nudges him. C does this too.
- **Level 1 ignores scripted input entirely** (Kid starts crouched, frame 109); levels 2–5
  respond. C does this too.
- **`Ctrl+G` does nothing on levels 1–2** — saving is levels 3–13 by design.
- **0-byte `PRINCE.SAV`/`PRINCE.HOF` in OPFS** — placeholders, not failed writes.

## Open technical debt worth knowing

- **Sub-12ms keypresses can be dropped.** The wasm shared-input transport is level-sampled;
  a press starting and ending between two polls produces no event. Harmless for human taps
  (50–150ms). The fix (latch presses, clear on consume) changes a transport contract shared
  with the menu smoke test, so it isn't a drive-by.
- **Menu-resident scenarios can't be live-diffed.** `draw_menu`'s inner loop blocks the tick
  loop so no trace is written. `menu-smoke-test` covers those separately by capturing frames.
- **`cargo xtask verify` never rebuilds the C oracle.** CI does (separate job). Any change to
  `c/*.h` prototypes or `c/` build config needs a from-scratch cmake build to be sure.
