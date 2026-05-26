# Plan: Port seg003, seg002, seg001 to Rust

## Context

seg004–seg007 are now ported, covering collision detection, character movement,
the tile/frame system, and all animated tiles (trobs, mobs, doors, loose floors).
The remaining C files are:

| File | Lines | Functions | Notes |
|------|-------|-----------|-------|
| seg003.c | 795 | 29 | Level loop, room redraw — sits on all ported segs |
| seg002.c | 1,237 | 67 | Guard/shadow AI |
| seg001.c | 866 | 68 | Cutscene playback and animation |
| seg008.c | 2,068 | 112 | Room rendering engine |
| seg000.c | 2,513 | 111 | Main loop, initialization |
| seg009.c | 4,248 | 225 | Platform layer (SDL, file I/O) |

This plan covers the three smallest/most tractable files: **seg003 → seg002 → seg001**,
in that order. seg008, seg000, seg009 are deferred — they are larger, more SDL-heavy,
or the entry point (best left until nearly everything else is ported).

---

## Phase order and rationale

### 1. seg003 (795 lines, 29 functions) — first

- Smallest remaining file by far
- Is the per-frame game loop and room redraw orchestrator: it calls into every
  already-ported module (seg004 collision, seg005 movement, seg006 tile, seg007 trobs)
- Getting it in Rust validates the entire ported stack end-to-end at runtime
- No `goto` observed; straightforward control flow

### 2. seg002 (1,237 lines, 67 functions) — second

- Guard/shadow AI: self-contained logic, calls seg005 and seg006
- Moderate size; no unusual patterns expected beyond standard type-scan precautions

### 3. seg001 (866 lines, 68 functions) — third

- Cutscene and animation sequencing
- Calls seg005 (`seqtbl_offset_char`) and seg006 (`load_frame_to_obj`, `play_seq`)
- Shorter than seg002 in lines but similar function count; order is flexible

---

## Pre-porting checklist (apply before each file)

1. **Type-scan all globals** the file touches:
   ```sh
   grep 'pub static mut VARNAME' target/debug/build/sdlpop-*/out/bindings.rs
   ```
2. **Check every C function called** for `c_short` params (bindgen reflects exact
   C prototypes):
   ```sh
   grep -A3 'pub fn fn_name' target/debug/build/sdlpop-*/out/bindings.rs
   ```
3. Watch for:
   - C `!` (logical NOT) vs Rust `!` (bitwise NOT) — use `== 0` in Rust
   - `word` (u16) globals that look like they should be signed
   - `c_short` params on functions you'd expect to take `c_int`
   - `curr_room_tiles`/`curr_room_modif` are `*mut byte` — index via pointer arithmetic

---

## Porting workflow (same as previous phases)

For each file:

1. Write module boilerplate + any file-local statics
2. Port in batches of ~10 functions, `cargo check` after each batch
3. `cargo test` when all functions compile
4. Remove the C file from `src/Makefile` (OBJ line), `src/CMakeLists.txt`
   (SOURCE_FILES block), and `build.rs` (sources array + rerun-if-changed line)
5. Add `pub mod segXXX;` to `rust/src/lib.rs`
6. Add tests for pure/near-pure functions (state machines, table lookups, math)
7. For any runtime bug found: add a regression test before moving on

---

## Build file changes (per file)

Each ported file requires removing it from three places:

- `build.rs` — remove from `sources` array; add `rerun-if-changed` for the new `.rs`
- `src/Makefile` — remove `.o` from `OBJ =` line
- `src/CMakeLists.txt` — remove `.c` from `SOURCE_FILES` block

---

## Verification

```sh
cargo check          # after each batch
cargo test           # after all functions in a file compile
cargo run            # smoke-test the game after each file is removed from C
```
