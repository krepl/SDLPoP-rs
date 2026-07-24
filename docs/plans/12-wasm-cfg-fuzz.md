# Plan: cfg Feature Gates, WASM Target, Fuzzing, and Game-Beating AI

## Context

The Rust port currently compiles with all features always-on, mirroring the C build's
default configuration. Three future goals require more structure:

1. **cfg feature gates** — mirror the C `#ifdef` flags as Cargo features so subsystems
   can be stripped for headless/WASM/fuzz builds
2. **WASM/browser target** — run the game in a browser without native SDL2
3. **Automated game-beating** — use the headless build + tree search to find a sequence
   of inputs that completes the game; fuzzing as a side-effect

These are sequentially dependent: gates first, then WASM and game-AI in parallel (both
need the headless build). None of this changes the C oracle or golden traces.

---

## Complete ifdef inventory

### Subsystem feature flags (→ Cargo features)

These control whole subsystems in config.h. All currently on; the Rust port includes all
branches. In parentheses: C-level dependencies.

| C flag | Cargo feature | Notes |
|--------|--------------|-------|
| `USE_FADE` | `fade` | screen fade effects |
| `USE_FLASH` | `flash` | screen flash effects |
| `USE_TEXT` | `text` | in-game text rendering |
| `USE_ALPHA` | `alpha` | alpha transparency (currently off in C config) |
| `USE_QUICKSAVE` | `quicksave` | save/load game state to disk |
| `USE_QUICKLOAD_PENALTY` | `quickload_penalty` | penalty on quickload (needs `quicksave`) |
| `USE_REPLAY` | `replay` | replay recording/playback (needs `quicksave`) |
| `USE_COPYPROT` | `copyprot` | copy protection puzzle level |
| `USE_MENU` | `menu` | in-game pause menu (needs `text`) |
| `USE_LIGHTING` | `lighting` | torch lighting overlay |
| `USE_SCREENSHOT` | `screenshot` | screenshot capture |
| `USE_SUPER_HIGH_JUMP` | `super_high_jump` | extended jump height |
| `USE_JUMP_GRAB` | `jump_grab` | grab ledge while jumping |
| `USE_TELEPORTS` | `teleports` | teleport tile type |
| `USE_FAKE_TILES` | `fake_tiles` | fake/invisible tile type |
| `USE_AUTO_INPUT_MODE` | `auto_input_mode` | auto-detect joystick vs keyboard |
| `USE_COLORED_TORCHES` | `colored_torches` | colored torch flame sprites |
| `USE_FAST_FORWARD` | `fast_forward` | in-game fast-forward |
| `USE_DARK_TRANSITION` | `dark_transition` | dark room transition effect |
| `USE_DEBUG_CHEATS` | `debug_cheats` | Ctrl+A/D/G/J/K/L/R/T/W cheat keys |
| `USE_COMPAT_TIMER` | `compat_timer` | alternate timer for old hardware |
| *(no ifdef)* `midi.c` / `opl3.c` | `audio` | MIDI+OPL3 subsystem; always-on in C, no flag |

**Dependency tree** (Cargo `requires` via feature deps):
```
replay         → quicksave
quickload_penalty → quicksave
menu           → text
audio          → (no deps; just controls midi.c/opl3.c compilation)
```

### Bug-fix flags (always-on — no Cargo features needed)

The `FIX_*`, `ALLOW_*`, `FREEZE_*`, `REMEMBER_*` flags are runtime-controlled via the
`fixes` pointer (set in `options.rs`). They gate struct field declarations and the
checks that read them, but enabling/disabling at runtime requires no recompile — just
set `use_fixes_and_enhancements = 0` in `SDLPoP.ini`. Compile-time gating would trade
a runtime toggle for a recompile requirement, with no benefit for WASM or headless builds
(none of these flags pull in platform code).

**Decision: keep all `FIX_*` always-on in Rust, exactly as in the current C default.**

The one exception worth considering later: if the speedrun/tournament community asks for
a guaranteed-vanilla binary, add a single `vanilla` Cargo feature that sets
`USE_FIXES_AND_ENHANCEMENTS = false` at compile time, eliminating the ~40 runtime checks
entirely. That's one gate, not ~40. Defer until there's an actual request.

Complete list for reference (all compiled-in, all have `fixes_options_type` fields):

`FIX_ABOVE_GATE`, `FIX_BIGPILLAR_CLIMB`, `FIX_BIGPILLAR_JUMP_UP`, `FIX_BLACK_RECT`,
`FIX_CAPED_PRINCE_SLIDING_THROUGH_GATE`, `FIX_CHOMPERS_NOT_STARTING`, `FIX_COLL_FLAGS`,
`FIX_CORNER_GRAB`, `FIX_DEAD_FLOATING_IN_AIR`, `FIX_DISAPPEARING_GUARD_A`,
`FIX_DISAPPEARING_GUARD_B`, `FIX_DOORTOP_DISABLING_GUARD`,
`FIX_DROP_2_ROOMS_CLIMBING_LOOSE_TILE`, `FIX_DROP_THROUGH_TAPESTRY`,
`FIX_EDGE_DISTANCE_CHECK_WHEN_CLIMBING`, `FIX_ENTERING_GLITCHED_ROOMS`,
`FIX_FALLING_THROUGH_FLOOR_DURING_SWORD_STRIKE`, `FIX_FEATHER_FALL_AFFECTS_GUARDS`,
`FIX_FEATHER_INTERRUPTED_BY_LEVELDOOR`, `FIX_GATE_DRAWING_BUG`, `FIX_GATE_SOUNDS`,
`FIX_GLIDE_THROUGH_WALL`, `FIX_GRAB_FALLING_SPEED`,
`FIX_GUARD_FOLLOWING_THROUGH_CLOSED_GATES`, `FIX_HANG_ON_TELEPORT`,
`FIX_HIDDEN_FLOORS_DURING_FLASHING`, `FIX_INFINITE_DOWN_BUG`,
`FIX_JUMP_DISTANCE_AT_EDGE`, `FIX_JUMPING_OVER_GUARD`,
`FIX_JUMP_THROUGH_WALL_ABOVE_GATE`, `FIX_LAND_AGAINST_GATE_OR_TAPESTRY`,
`FIX_LEVEL_14_RESTARTING`, `FIX_LOOSE_LEFT_OF_POTION`, `FIX_LOOSE_NEXT_TO_POTION`,
`FIX_MOVE_AFTER_DRINK`, `FIX_MOVE_AFTER_SHEATHE`, `FIX_OFFSCREEN_GUARDS_DISAPPEARING`,
`FIX_ONE_HP_STOPS_BLINKING`, `FIX_PAINLESS_FALL_ON_GUARD`,
`FIX_PRESS_THROUGH_CLOSED_GATES`, `FIX_PUSH_GUARD_INTO_WALL`,
`FIX_RETREAT_WITHOUT_LEAVING_ROOM`, `FIX_RUNNING_JUMP_THROUGH_TAPESTRY`,
`FIX_SAFE_LANDING_ON_SPIKES`, `FIX_SKELETON_CHOMPER_BLOOD`, `FIX_SOUND_PRIORITIES`,
`FIX_SPRITE_XPOS`, `FIX_STAND_ON_THIN_AIR`, `FIX_TURN_RUN_NEAR_WALL`,
`FIX_TWO_COLL_BUG`, `FIX_UNINTENDED_SWORD_STRIKE`, `FIX_WALL_BUMP_TRIGGERS_TILE_BELOW`,
`ALLOW_CROUCH_AFTER_CLIMBING`, `ALLOW_INFINITE_TIME`, `FREEZE_TIME_DURING_END_MUSIC`,
`REMEMBER_GUARD_HP`

### Platform / target flags (→ `#[cfg(target_...)]`, not Cargo features)

| C flag | Rust equivalent |
|--------|----------------|
| `__EMSCRIPTEN__` | `#[cfg(target_arch = "wasm32")]` |
| `_WIN32`, `__MINGW32__` | `#[cfg(target_os = "windows")]` |
| `__PSP__` | `#[cfg(target_os = "psp")]` — skip for now |
| `__NEWLIB__` | `#[cfg(target_env = "newlib")]` — skip for now |
| `O_BINARY` | not needed in Rust (open in binary mode by default) |

### Internal / implementation flags (no Cargo feature needed)

These gate debug assertions, include guards, or configuration of vendored libraries.
Leave them as `const` booleans or remove the `#ifdef` entirely:

`CHECK_SEQTABLE_MATCHES_ORIGINAL` — compile-time debug assertion; port as
`#[cfg(debug_assertions)]` or a unit test.  
`CHECK_TIMING` — timing debug prints; gate with `#[cfg(debug_assertions)]`.  
`FAST_FORWARD_MUTE`, `FAST_FORWARD_RESAMPLE_SOUND` — fast-forward audio behavior
variants; always include the better variant.  
`STB_VORBIS_*` — internal stb_vorbis configuration; stays in the C file.  
`BODY`, `COMMON_H`, `DATA_H`, `TYPES_H`, `CONFIG_H`, `STATE_DUMP_H`, `OPL_OPL3_H`
— C include guards; irrelevant in Rust.

---

## Phase 1 — Cargo feature gates

### Cargo.toml

```toml
[features]
default = [
    "fade", "flash", "text", "quicksave", "quickload_penalty",
    "replay", "copyprot", "menu", "lighting", "screenshot",
    "super_high_jump", "jump_grab", "teleports", "fake_tiles",
    "auto_input_mode", "colored_torches", "fast_forward",
    "dark_transition", "debug_cheats", "audio",
]

# Subsystem features
fade              = []
flash             = []
text              = []
alpha             = []       # off by default, as in C
quicksave         = []
quickload_penalty = ["quicksave"]
replay            = ["quicksave"]
copyprot          = []
menu              = ["text"]
lighting          = []
screenshot        = []
super_high_jump   = []
jump_grab         = []
teleports         = []
fake_tiles        = []
auto_input_mode   = []
colored_torches   = []
fast_forward      = []
dark_transition   = []
debug_cheats      = []
compat_timer      = []
audio             = []   # midi + opl3 subsystem

# Platform backends (mutually exclusive)
sdl      = []   # native SDL2 (default for native targets)
wasm     = []   # Web APIs via wasm-bindgen

# Convenience bundles
headless = []   # no rendering, no audio, no menu — for fuzzing and search
```

### Annotation approach

In each Rust module, wrap blocks with `#[cfg(feature = "...")]`. The `lib.rs` module
declarations:

```rust
#[cfg(feature = "menu")]
pub mod menu;
#[cfg(feature = "audio")]
pub mod midi;
#[cfg(feature = "audio")]
pub mod opl3;
#[cfg(feature = "lighting")]
pub mod lighting;
#[cfg(feature = "screenshot")]
pub mod screenshot;
#[cfg(feature = "replay")]
pub mod replay;
```

Within function bodies, use inline cfg:

```rust
// C: #ifdef USE_LIGHTING
#[cfg(feature = "lighting")]
redraw_lighting();
// C: #endif
```

### Verification targets after annotation

```sh
# Default (all features) — must still pass the harness
cargo build && scripts/run_harness.sh

# Headless: no SDL, no audio, no rendering
cargo build --no-default-features --features "quicksave,replay,text,headless"

# Minimal: just core gameplay
cargo build --no-default-features
```

---

## Phase 2 — Platform abstraction layer

### The split

seg009.c contains almost all SDL platform code. Split into:

| Module | Content | Gate |
|--------|---------|------|
| `seg009_core.rs` | DAT loading, path resolution, INI config, decompression | always |
| `seg009_sdl.rs` | SDL init/teardown, audio device, window, event loop | `#[cfg(feature = "sdl")]` |
| `seg009_wasm.rs` | web-sys Canvas, Web Audio, fetch for assets | `#[cfg(feature = "wasm")]` |
| `seg009_headless.rs` | stub implementations (no-op audio, memory filesystem) | `#[cfg(feature = "headless")]` |

Same split for seg000 (game loop scheduling) and seg008 (rendering).

### Rendering in WASM

The SDLPoP renderer writes pixels to an SDL surface. For WASM, the cheapest shim:
write to a `[u8; 320 * 192 * 4]` RGBA framebuffer, then call a JS function to paint it
onto a `<canvas>` element. No WebGL needed for a first pass.

```rust
#[cfg(feature = "wasm")]
#[wasm_bindgen]
extern "C" {
    fn paint_frame(pixels: &[u8]);  // JS side: ctx.putImageData(...)
}
```

### Audio in WASM

`opl3.rs` is pure arithmetic — runs in WASM with no changes. Wire the SDL audio
callback to a Web Audio `ScriptProcessorNode`:

```js
const processor = audioCtx.createScriptProcessor(2048, 0, 1);
processor.onaudioprocess = (e) => {
    wasm.fill_audio_buffer(e.outputBuffer.getChannelData(0));
};
```

### Asset loading

Game reads DAT files from `data/`. In WASM, fetch at startup:

```js
const dat = await fetch('data/PRINCE.DAT').then(r => r.arrayBuffer());
wasm.load_dat_from_memory('PRINCE.DAT', new Uint8Array(dat));
```

### Quicksave in WASM

Map `USE_QUICKSAVE` file I/O to `localStorage` (small) or `IndexedDB` (large). A WASM
platform impl of the `write_file` / `read_file` functions that the quicksave code calls.

### Cargo.toml additions for WASM

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = [
    "Window", "Document", "HtmlCanvasElement", "CanvasRenderingContext2d",
    "ImageData", "AudioContext", "AudioBuffer", "AudioBufferSourceNode",
    "ScriptProcessorNode", "Performance", "Request", "Response",
    "Storage",   # localStorage for quicksave
] }
js-sys = "0.3"
console_error_panic_hook = "0.1"

[build-dependencies]
wasm-pack = { version = "0.12", optional = true }
```

### Build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
wasm-pack build --target web --features "wasm,audio,lighting,screenshot,menu,replay,quicksave"
# Output: pkg/sdlpop_bg.wasm + pkg/sdlpop.js
```

---

## Phase 3 — Automated game-beating

### Why this is feasible

The game is small and deterministic:
- ~13 playable levels, each fits in a single room graph (≤24 rooms)
- 12.5 frames/sec original; inputs are 1 byte/frame (8 buttons, most combos unused)
- Headless Rust simulation: conservatively 500,000+ ticks/sec on modern hardware
  (no SDL, no rendering, no audio)
- Full game is ~60 min real-time = ~45,000 frames. With no rendering overhead,
  that's < 0.1 seconds of wall-clock simulation time per full-game rollout
- State is fully observable: `(level, room, Kid.x, Kid.y, Kid.frame, Kid.action, hitp_curr, random_seed, guard states)`

### Input space

Per frame, the active input combinations are roughly:
- Nothing (0)
- Left, Right, Up, Down (4)
- Shift+Left, Shift+Right (cautious step)
- Ctrl (fight/action)
- Shift+Ctrl combinations
- ~10 distinct meaningful inputs per frame

### Approach: Beam search with state deduplication

The simplest algorithm that should work for most levels:

```rust
struct GameState {
    snapshot: Vec<u8>,        // full game state bytes (quicksave format)
    progress: u32,            // heuristic score
    history: Vec<u8>,         // input sequence that reached this state
}

fn progress(state: &GameState) -> u32 {
    let level = read_level(&state.snapshot);
    let room  = read_room(&state.snapshot);
    let x     = read_kid_x(&state.snapshot);
    // Higher is better: completing a level >> being further right
    (level as u32) * 1_000_000
        + (room as u32) * 10_000   // rightward rooms score higher
        + (x as u32)
        - (death_penalty(&state.snapshot)) * 100_000
}

fn beam_search(initial: GameState, beam_width: usize, frames_per_step: usize) -> Vec<u8> {
    let mut beam = vec![initial];
    let mut visited: HashSet<StateHash> = HashSet::new();

    loop {
        let mut candidates = Vec::new();
        for state in &beam {
            for input in MEANINGFUL_INPUTS {
                let next = simulate(&state.snapshot, input, frames_per_step);
                let hash = state_hash(&next);
                if visited.insert(hash) {
                    candidates.push(next);
                }
            }
        }
        candidates.sort_by_key(|s| std::cmp::Reverse(s.progress));
        candidates.dedup_by(|a, b| state_hash(a) == state_hash(b));
        beam = candidates.into_iter().take(beam_width).collect();

        if beam.iter().any(|s| s.progress > WIN_THRESHOLD) {
            return beam[0].history.clone();
        }
    }
}
```

### State hashing

Exact hash: `(level, curr_room, Kid.x, Kid.y, Kid.frame, Kid.action, hitp_curr, guard.room, guard.x, guard.y, guard.hp)`. This fits in ~20 bytes; use FxHash or xxHash for speed. The `random_seed` does NOT need to be in the hash for deduplication — two states at the same position with different seeds are for practical purposes equivalent.

### Integration with the game engine

The headless build needs two operations:
1. **Quicksave to memory**: serialize full game state to `Vec<u8>` without writing to disk
2. **Quickload from memory**: restore state from `Vec<u8>` and advance N frames with given input

Both map directly to `USE_QUICKSAVE` functions. In headless mode, redirect the file path to an in-memory buffer.

### Fuzz testing as a side-effect

The beam search generates thousands of diverse game states and input sequences. Feed these as a corpus to `cargo-fuzz`:

```sh
cargo fuzz init
cargo fuzz add fuzz_game_inputs
# Seed with beam search outputs:
mkdir -p fuzz/corpus/fuzz_game_inputs
./target/release/beam_search --dump-corpus fuzz/corpus/fuzz_game_inputs/
cargo fuzz run fuzz_game_inputs
```

The fuzzer will then mutate these valid-looking input sequences and explore edge cases.

### Validation

Once a winning input sequence is found:
1. Save it as a `.P1R` replay file (it's just a sequence of frame inputs)
2. Run it through both the C oracle and the Rust build
3. Compare traces — if they match, the game-beater is provably correct

```sh
# Save as replay
./target/release/solver --output solutions/game_complete.p1r

# Validate against oracle
./prince validate solutions/game_complete.p1r  # C oracle
POPTRACE_OUT=tmp/rust.trace ./target/debug/prince validate solutions/game_complete.p1r
python3 scripts/compare_traces.py traces/golden_solution.trace tmp/rust.trace
```

### Potential enhancements

- **MCTS** instead of beam search: UCB1 rollouts explore more of the tree but require
  a value function. The progress heuristic above works as a rollout reward signal.
- **Level-by-level decomposition**: solve each level independently (harder given
  carry-over state like HP and time), then chain solutions.
- **Speedrun optimization**: after finding any winning sequence, apply local search to
  minimize frame count — swap individual frames, delete redundant inputs.
- **Adversarial**: run the solver against modified level files (mods) to auto-generate
  speedrun solutions for custom levels.

---

## Phase 4 — CI fuzzing (independent of game-beating)

### Targets

| Fuzz target | Purpose |
|-------------|---------|
| `fuzz_game_inputs` | Random inputs for N ticks; catch panics, OOB, infinite loops |
| `fuzz_tile_access` | `get_tile` / `get_modifier` with random room/col/row |
| `fuzz_seqtbl` | Execute random sequence indices; catch OOB in sequence table |
| `fuzz_char_state` | Random `Char` fields + `do_char`; catch state machine violations |
| `fuzz_ini_parser` | Random `.ini` content through `load_options`; catch parse panics |

### CI integration

```yaml
# .github/workflows/fuzz.yml
jobs:
  fuzz:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - run: rustup install nightly
      - run: cargo install cargo-fuzz
      - run: |
          for target in fuzz_game_inputs fuzz_tile_access fuzz_seqtbl; do
            cargo +nightly fuzz run $target -- -max_total_time=60
          done
```

### Corpus seeding

```sh
# Existing replays are valid seed inputs
for f in doc/replays-testcases/*.p1r; do
    cp "$f" fuzz/corpus/fuzz_game_inputs/
done
# Add beam-search solutions when available
```

---

## Phase 5 — Audio port verification (deferred, low priority)

`opl3.rs` and `midi.rs` were ported but are **not exercised by the trace harness** — the
state trace captures game state, which audio never affects. This is a real coverage gap,
but a deliberately low-priority one: **audio is a pure sink** (game state → audio, never the
reverse), so a synthesis divergence cannot corrupt gameplay — it can only make the game
sound subtly wrong, which is audible. Finish gameplay coverage (Phase 1.5) first.

**Do NOT verify via a fake/virtual audio device.** Routing through a virtual ALSA/PulseAudio
sink reintroduces device-driven callback timing, which is nondeterministic tick-to-tick —
traces would differ between runs for reasons unrelated to correctness. Instead, drive the
synth deterministically off the game tick. The Nuked OPL3 core is **integer/fixed-point**,
so C and Rust output should match sample-for-sample (no float tolerance needed):

- **Preferred: synth-core unit tests.** Feed a fixed sequence of OPL3 register writes,
  render N samples, assert the C and Rust output arrays are byte-identical. No SDL, no
  timing, isolates the emulator.
- **Alternative: per-tick sample checksum in the trace.** Render a fixed sample count per
  game tick and dump a hash as an extra trace field — reuses `compare_traces.py`; a
  divergence surfaces at the exact tick.

---

## Order of work

1. **Phase 1** (cfg gates) ✅ — annotation pass; harness passes on all replays
2. **Phase 1.5** (replay coverage) — in progress; lvl1 completion done (see below)
3. **Phase 3** (game-beating) and **Phase 2** (WASM) — independent after Phase 1 produces
   a headless build; work in parallel
4. **Phase 4** (CI fuzzing) — start as soon as Phase 1 is done; doesn't need WASM
5. **Phase 5** (audio port verification) — deferred, low priority; after Phase 1.5

### Phase 1.5 — current scope

**Done:** `lvl01_complete.p1r` — a level 1 playthrough covering sword pickup, two guard
kills, potion (used *and* wasted-at-full-HP), spikes (walk-through + hang-above), and
loose floors. Committed with its golden trace; all 30 harness replays pass.

Also recorded `lvl04_mirror_complete.p1r`: full level 4 playthrough, jumped through the mirror at
the end (mirror image encounter, HP dropped to 1). Committed with its golden trace, no
divergence (4990 frames). Note: level 4's mechanic is the **mirror**, not a skeleton
guard or feather-fall potion — those were mislabeled in an earlier pass of this checklist
(skeleton guard is actually level 3: `skeleton_level = 3` default in `data.h`).

Also recorded `lvl03_skeleton_complete.p1r`: level 3 playthrough, pushed the skeleton guard into a
pit. **Found and fixed a real port bug**: `draw_mob` (`seg007.rs`) panicked with "attempt
to negate with overflow" — the C source computes `ABS((sbyte)ypos)`, which promotes the
`sbyte` to `int` before negating (so `-128` safely becomes `128`), but the Rust port did
`(ypos as i8).abs()`, which panics on `i8::MIN` since the negated result doesn't fit back
in `i8`. Fixed by widening to `i32` first: `(ypos as i8 as i32).abs()`. Added a regression
test (`draw_mob_room_b_abs_does_not_panic_on_i8_min`). Harness now passes with no
divergence (2327 frames); all 13 replays green.

Also recorded `lvl07_feather_complete.p1r`: level 7 playthrough, drank the feather-fall potion and
fell from a height. Confirmed via trace scan: `is_feather_fall` goes nonzero at tick 2702
(peak value 224), so the slow-descent state is genuinely exercised. Committed with its
golden trace, no divergence (3011 frames); all 14 replays green.

Also recorded `lvl02_poison_complete.p1r`: full level 2 playthrough, drank a poison potion.
Static analysis of the raw level files (`data/LEVELS/res20NN.bin`) to pre-identify the
poison potion's level turned out unreliable (the on-disk modifier bytes didn't cleanly
match the `>> 3` decode in `seg005.c:654` for most levels), so this was confirmed
after-the-fact from the trace instead: 4 separate `hitp_curr` drops of exactly 1, each
coinciding with `Kid.frame == frame_205_drink` and `Guard.guard_notice_timer == 0` /
`holding_sword == 0` (i.e. not combat damage). Committed with its golden trace, no
divergence (3441 frames); all 15 replays green.

Also recorded `lvl05_shadow_steal_complete.p1r`: full level 5 playthrough. Level 5 has its
own distinct shadow mechanic (`shadow_steal_level = 5` in `data.h`) — where the shadow
steals a potion in room 24 (`shadow_steal_room = 24`). Confirmed via trace:
`Guard.charid == 1` (shadow) while `curr_room == 24`. Committed with its golden trace, no
divergence; all 16 replays green.

Also recorded `lvl06_shadow_step_fatguard_complete.p1r`: full level 6 playthrough. Another
correction: level 6 is **not** shadow unification (that was wrong, see below) — it's a
"shadow step" presentation event (`shadow_step_level = 6`, `shadow_step_room = 1` in
`data.h`; shadow appears, sets `leveldoor_open = 0x4D`, no union) plus a **Fat** guard
fight (`tbl_guard_type[6] = 1` = Fat in `data.h`). Confirmed via trace:
`leveldoor_open == 0x4D` fires at tick 854, and `guardhp_max == 5` during the fight
(higher than the normal 3 HP). The actual shadow *reunification* (`united_with_shadow`
set to 42) only happens on **level 12** (`seg002.c:1218`, `check_shadow()`), which needs
its own separate replay. Committed with its golden trace, no divergence; all 17 replays
green. Also sorted `scripts/run_harness.sh`'s `lvlN_*` entries by level number for
readability.

Also recorded `lvl08_mouse_gate_complete.p1r`: full level 8 playthrough. Confirms the level
8 mouse event (`mouse_level=8`, `mouse_room=16`) actually crosses a pressure-plate tile
and opens a gate — correcting an earlier claim in this checklist that `do_mouse()` has no
gate tie-in. That claim was too narrow: `do_mouse()` itself has no special gate logic, but
the mouse's scripted movement path can cross an ordinary button/pressure-plate tile, which
triggers the *generic* button-trigger system any character can activate — no mouse-specific
code needed. Confirmed via trace: `Char.charid == 24` (mouse) appears at tick 566.
Committed with its golden trace, no divergence; all 19 replays green.

**Found and FIXED: a real Rust vs C divergence in control-input handling, surfaced as a
sword-combat sequence divergence.** While processing a death replay (`lvl08_death_2.p1r`),
also found and fixed a **harness bug**: `scripts/compare_traces.py`'s `compare()` never
called `sys.exit(1)` on divergence, so `run_harness.sh`'s exit-code check always saw
success — the harness could never actually fail on a real divergence, it would just print a
diff and report PASS. Fixed by adding `sys.exit(1)` in the `n_diverged > 0` branch. With
that fix, `lvl08_death_2` caught a real divergence: `Kid.curr_seq` diverged at tick 2469
during an active sword fight (Kid vs Guard, room 12) — golden `curr_seq = 6616`, Rust
`6698`, i.e. the Kid entered `fastadvance` while golden stayed in the `ready` stance.

The seed/parse-alignment hypothesis was **wrong** — `random_seed` matched at every tick.
Root cause was traced (by temporarily adding the `control_*`/`ctrl1_*` globals to the trace
and re-diffing) to a much earlier untraced divergence: `control_up` first differed at tick
76, during a climb-up (`control_hanging` → `can_climb_up`). The bug: `can_climb_up` (and
`draw_sword`) in `rust/src/seg005.rs` ported C's **chained assignment**
`control_up = control_shift2 = release_arrows();` — which calls `release_arrows()` **once** —
as **two separate calls**. Since `release_arrows()` side-effect-zeroes
`control_forward/backward/up/down`, the second call silently reset `control_up` (resp.
`control_forward`) back to `0` (RELEASED) instead of the intended `1` (IGNORE). That
corrupted the up-key latch state, which only manifested visibly ~2400 ticks later in
combat. Fixed both sites to a single `release_arrows()` call with a follow-up copy
(`control_shift2 = release_arrows(); control_up = control_shift2;`). `lvl08_death_2.p1r`/
`.trace` are now committed and registered in `PAIRS`; harness fully green at 19/19.

Also recorded `lvl09_invert_complete.p1r`: full level 9 playthrough, drank the invert
potion (screen-flip effect, `toggle_upside()`/`case 4` in `seg000.c:1885`). `upside_down`
isn't a traced field, so confirmed indirectly instead: a full crouch-drink-standup sequence
at ticks 3709–3713 (room 10) with `hitp_curr` unchanged and `is_feather_fall` staying 0,
ruling out health/life/poison/feather — the only potion type left that fits is invert.
Committed with its golden trace, no divergence (6326 frames); all 20 replays green.

Also recorded `lvl10_prince_disappears_bug.p1r`: a user-found glitch on level 10 where the
Kid sprite disappears in a specific scenario. Purpose is regression coverage for the bug
itself (a faithful port must reproduce original-game glitches, not fix them — see "No
behavior changes" in the porting prime directives). Confirmed: Rust matches the C oracle
exactly, no divergence (404 frames) — the glitch reproduces identically in the Rust port.
Committed with its golden trace.

Also recorded `lvl10_complete.p1r`: a full, non-glitched level 10 playthrough (general
mechanics coverage for the level, separate from the disappearing-Kid bug replay above).
Committed with its golden trace, no divergence (2981 frames); all 22 replays green.

Also recorded `lvl11_complete.p1r`: a full level 11 playthrough. Committed with its golden
trace, no divergence (2491 frames); all 23 replays green.

Also recorded `lvl12_13_complete.p1r` (recorded/named `lvl12_complete.p1r` originally, but
the recording actually ran through level 12, into level 13, and briefly into level 14 —
renamed to reflect the real coverage). Level 12 portion closes out the shadow
*unification* checklist item — confirmed via trace: `united_with_shadow` jumps to exactly
`42` at tick 1543 (matching `check_shadow()`, `seg002.c:1218`), then decrements each
subsequent tick, exactly as the C source does. The level 13 portion (ticks 1712–2632) DOES
include the vizier fight, closing that checklist item too — confirmed via trace:
`guardhp_curr` starts at 6 (tick 2106) and is whittled to 0 by tick 2436 (the kill).
(Correction to an earlier pass of this note: checking `Guard.charid == charid_6_vizier`
was the wrong signal — that constant is only set in the post-game cutscene, `seg001.c:278`;
during real gameplay the vizier is just the ordinary `Guard` struct, since `tbl_guard_type`
only allows one guard config per level, so any guard fought in level 13 is the vizier by
construction.) The level 14 entry (ticks 2632–2652) is just the door transition, not real
level 14 coverage — see `lvl14_complete.p1r` below for that. Committed with its golden
trace, no divergence (2653 frames); all 24 replays green.

Also recorded `lvl14_complete.p1r`: a full level 14 playthrough (the upside-down/invert
potions level). Committed with its golden trace, no divergence (216 frames); all 25
replays green.

Also recorded `time_limit_expiry_lvl3.p1r`: recorded via a temporary `SDLPoP.ini` tweak
(`start_minutes_left = 1` instead of the default `60`, reverted afterward) so the timer
runs out in ~1 real minute instead of 60. Confirms a genuine, slightly surprising original-
game quirk: timing out doesn't restart the *current* level, it kicks you back to
`custom->first_level` (level 1) — traced `current_level` goes `3 → 0` at tick 718, the
exact tick `rem_min`/`rem_tick` bottom out. (`expired()`, `seg001.c:644`, sets
`start_level = -1`, and `seg000.c:229` resolves that to `custom->first_level`, not the
level you were just on — a different code path than the ordinary retry-same-level death,
which uses a separate `is_restart_level` flag.) A first recording attempt was discarded:
a stray keypress near the end triggered a level restart before the timer actually reached
zero, so it never captured the real timeout — not committed. Committed with its golden
trace, no divergence (776 frames); all 26 replays green.

Also recorded `long_fall_death.p1r`: a level 7 death from falling more than 2 stories
("hard land" / splat). Confirmed via trace: `Char.action == 4` (in-freefall) with
`fall_y == 33` (max fall speed) sustained across several ticks right before death at tick
15, where `Kid.frame` jumps to `frame_185_dead` and `hitp_curr` is wiped from 3 to 0 in one
hit (`hitp_delta == -3`) — a fatal instant-kill, distinct from the gradual damage of
combat/poison. Committed with its golden trace, no divergence (59 frames); all 27 replays
green.

Also recorded `impalement_death_lvl1.p1r`: a fatal spike impalement on level 1. Confirmed
via trace: `Kid.frame` hits `177` (`frame_177_spiked`) at tick 279 with `hitp_curr` dropping
straight to 0, followed by the expected level restart at tick 339. Committed with its
golden trace, no divergence (343 frames); all 28 replays green.

Also recorded `running_impalement_lvl6.p1r` (fatal spike impalement while running, level 6)
and `chomper_death_lvl7.p1r` (fatal chomper contact, level 7). Confirmed via trace:
`running_impalement_lvl6` hits `Kid.frame == 177` at tick 85 (`hitp_curr` to 0); note
`action == 1` (`run_jump`) at the death tick matches `impalement_death_lvl1`, so these may
exercise the same code path mechanically despite being a different level/route — harmless
extra coverage either way. `chomper_death_lvl7` hits `Kid.frame == 178`
(`frame_178_chomped`) at tick 329 (`hitp_curr` to 0). Both committed with golden traces, no
divergence (166 and 439 frames respectively); all 30 replays green.

Also recovered/committed `run_right_and_die_lvl_1.p1r` — the replay that generates the
primary `traces/golden.trace`. It had lived only in the gitignored `replays/` dir and was
never committed (i.e. lost); it's now tracked under `doc/replays-testcases/`.

**Harness hardening done alongside** (see `scripts/run_harness.sh`): `SDL_AUDIODRIVER=dummy`
on all invocations (SDL's ALSA fallback blocks ~30s when it can't reach an audio server —
a headless/CI/reduced-env hang, *not* a port bug); skip-missing-replay guard; `timeout 60`
backstop; `ln -sfn` to stop stray self-links.

**Naming convention:** replays that reach the level's exit door get a `_complete` suffix
(e.g. `lvl04_mirror_complete.p1r`); ones that stop early once the target mechanic is shown
don't. `lvl04_mirror`, `lvl03_skeleton`, and `lvl07_feather` were renamed to add the suffix
after confirming with the recorder that each was a full clear.

### Coverage checklist — pick up here

Confirmed covered by `lvl01_complete`:
- [x] Sword pickup
- [x] Guard combat / kill (x2)
- [x] Potion — small red, both used and drunk-at-full-HP
- [x] Spikes — walked through, and hung above
- [x] Loose floor tiles (walked over one, one fell on Kid from the ceiling)
- [x] Level exit door → level 2 transition (confirmed: player exited through it)

Confirmed covered by `lvl04_mirror_complete`:
- [x] Mirror / mirror-image encounter (jumped through, HP dropped to 1)

Confirmed covered by `lvl03_skeleton_complete`:
- [x] Skeleton guard (immune to sword, pit-pushed) — also caught a real `draw_mob` panic bug

Confirmed covered by `lvl07_feather_complete`:
- [x] Feather fall potion — drunk, confirmed via `is_feather_fall` going nonzero in trace

Confirmed covered by `lvl02_poison_complete`:
- [x] Poison potion — drunk, confirmed via `hitp_curr` drop coinciding with the drink frame

Confirmed covered by `lvl05_shadow_steal_complete`:
- [x] Shadow steal encounter (room 24) — confirmed via `Guard.charid == 1` while in that room

Confirmed covered by `lvl06_shadow_step_fatguard_complete`:
- [x] Shadow step presentation event — confirmed via `leveldoor_open == 0x4D`
- [x] Fat guard fight (5 HP vs normal 3) — confirmed via `guardhp_max == 5`

Confirmed covered by `lvl08_mouse_gate_complete`:
- [x] Mouse event crossing a button tile, opening a gate — confirmed via `Char.charid == 24`

Confirmed covered by `lvl09_invert_complete`:
- [x] Invert (upside-down) potion — drunk, confirmed indirectly (no hitp/feather change)

Confirmed covered by `lvl10_prince_disappears_bug`:
- [x] Regression coverage for a user-found sprite-disappearing glitch — Rust matches C
      oracle exactly (glitch reproduces identically, as required for a faithful port)

Confirmed covered by `lvl10_complete`:
- [x] Full level 10 playthrough — general mechanics coverage, no divergence

Confirmed covered by `lvl11_complete`:
- [x] Full level 11 playthrough — general mechanics coverage, no divergence

Confirmed covered by `lvl12_13_complete`:
- [x] Shadow unification — confirmed via `united_with_shadow` jumping to 42 at tick 1543
- [x] Vizier fight — confirmed via `guardhp_curr` dropping from 6 to 0 (tick 2106–2436)

Confirmed covered by `lvl14_complete`:
- [x] Full level 14 playthrough (upside-down/invert potions level) — no divergence
- [x] Princess/win ending sequence (per recorder's account, not independently trace-verified —
      `dump_frame_state()` only runs inside `play_level_2()`'s core gameplay loop,
      `seg003.c:374`; the ending cutscene runs through a wholly separate path in
      `seg001.c` that never calls it, so trace recording naturally stops the moment
      gameplay hands off to the cutscene and there is no traced signal to check either way)

Confirmed covered by `time_limit_expiry_lvl3`:
- [x] Time-limit expiry — confirmed via `current_level` dropping from 3 to 0 at the exact
      tick `rem_min`/`rem_tick` bottom out; confirms timing out kicks you back to level 1
      (`custom->first_level`), not a retry of the current level

Confirmed covered by `long_fall_death`:
- [x] Long-fall death — confirmed via sustained `fall_y == 33` (max speed) before death,
      `Kid.frame == frame_185_dead`, `hitp_curr` wiped from 3 to 0 in one hit

Confirmed covered by `impalement_death_lvl1` and `running_impalement_lvl6`:
- [x] Fatal spike impalement — confirmed via `Kid.frame == 177` (`frame_177_spiked`) with
      `hitp_curr` dropping straight to 0 (two separate levels/routes)

Confirmed covered by `chomper_death_lvl7`:
- [x] Chomper death — confirmed via `Kid.frame == 178` (`frame_178_chomped`) with
      `hitp_curr` dropping straight to 0

**Gate + button / Chomper** — the original "plausibly on lvl1" assumption was wrong:
raw tile scan of `data/LEVELS/res20NN.bin` shows level 1 has **zero** gate (tile 4) or
chomper (tile 18) tiles at all. Found instead: gates on levels 5/6/7/10, chompers on
levels 3/7/8/10. Likely already covered by existing full-level replays without a
dedicated recording — `lvl07_feather_complete` visits room 3 (gate) and room 23
(chomper); `lvl08_mouse_gate_complete` visits room 23 (chomper); `lvl05_shadow_steal_complete`
visits rooms 7/8/13/24 (gates). This is room-visit evidence only (walking through a room
doesn't guarantee touching the specific tile), not independently confirmed — accepted as
likely-covered rather than digging further.

**Balcony ledge** — raw tile scan finds **zero** balcony tiles (ID 23/24) across all 14
standard campaign levels. It's a real tile type (used in `seg005.c`/`seg008.c` for
gate-related collision), just apparently unused in the stock level set. Dropped from the
checklist — not testable without a custom level.

Not yet recorded — next replays to make, roughly in priority order:
- [x] Fix the sword-combat `curr_seq` divergence found via `lvl08_death_2.p1r` (see above) —
      root cause was the split-chain `release_arrows()` bug in `can_climb_up`/`draw_sword`;
      replay now registered in `PAIRS`
- [ ] Quicksave/quickload integration test (F6/F9 — not a replay; separate script:
      save → kill → relaunch with `--load` → compare state)
- [ ] Long-term save (Ctrl+G, `PRINCE.SAV`) — low priority
- [ ] Hall of fame write on game completion — low priority

---

## What is NOT changing

- C oracle (`src/Makefile`, `src/CMakeLists.txt`) — unchanged forever
- Harness (`scripts/run_harness.sh`) — continues running native SDL build vs golden traces
- `data.c` and `stb_vorbis.c` — stay compiled from C in the native build
- Game logic fidelity — WASM build must produce identical traces to native build
  in headless/replay mode
