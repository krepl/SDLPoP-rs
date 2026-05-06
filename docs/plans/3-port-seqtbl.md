# Plan 3: Port seqtbl.c to Rust

**Goal:** Replace `src/seqtbl.c` with a Rust implementation in `rust/src/seqtbl.rs`.
All public symbols are re-exported with `#[no_mangle]` so the rest of the C codebase
calls them transparently. The game must run identically after the swap; all existing
tests must pass, and new tests verify the byte-level correctness of the ported data.

---

## Background

`seqtbl.c` (1228 lines) defines the animation bytecode interpreter's data — every
character animation in the game (run, jump, fall, fight, climb, …) is a stream of
bytes in `seqtbl`. The file has four conceptual parts:

1. **Instruction encoding macros** (`act`, `jmp`, `dx`, `dy`, `snd`, `set_fall`) that
   expand to byte sequences using `SEQ_*` opcode constants from `types.h`.
2. **~90 label `#define`s** that name offsets into `seqtbl` (e.g. `running`,
   `startrun`, `stand`, …). Each is computed as a cumulative sum from
   `SEQTBL_BASE = 0x196E`.
3. **`seqtbl[]`** — the animation bytecode, written in human-readable form using the
   macros and labels above. The base table (without teleports) is 2310 bytes and is
   identical to the original DOS executable; an optional 42-byte `USE_TELEPORTS`
   extension is appended at the end.
4. **`seqtbl_offsets[]`** — a 115-entry (116 with `USE_TELEPORTS`) lookup table of
   `u16` absolute addresses, mapping sequence enum values to positions in the table.
5. **`apply_seqtbl_patches()`** — mutates one byte of `seqtbl` at runtime based on a
   fix flag.
6. **`check_seqtable_matches_original()`** — debug validator comparing `seqtbl`
   against the embedded DOS reference bytes; compiled only when
   `CHECK_SEQTABLE_MATCHES_ORIGINAL` is defined (off by default).

The file has **zero SDL calls** and **zero C function calls** (beyond the `fixes`
global access in `apply_seqtbl_patches`). It is the cleanest possible port target.

---

## Exported symbols

| Symbol | C type | Mutable | Notes |
|---|---|---|---|
| `seqtbl` | `byte[]` | yes | `apply_seqtbl_patches` writes index `bumpfall+1` |
| `seqtbl_offsets` | `const word[]` | no | read-only lookup used by `seg005.c` |
| `apply_seqtbl_patches` | `void ()` | — | reads `fixes` C global |
| `check_seqtable_matches_original` | `void ()` | — | debug only; may be a no-op |

`seg006.c` accesses `seqtbl` via the macro `SEQTBL_0 = (seqtbl - SEQTBL_BASE)`,
which treats it as a pointer to an array conceptually indexed from address `0x196E`.
This works transparently when `seqtbl` is a `#[no_mangle] static mut [u8; N]` —
the C side receives a `byte*` pointing to element 0, which is correct.

---

## Data strategy

The `original_seqtbl` bytes already in `seqtbl.c` are the authoritative ground truth
(extracted from the DOS `.exe`). Use them verbatim for the base 2310-byte portion of
`seqtbl`. Append the teleport extension (42 bytes) as inline byte literals, with
comments referencing the C source to make the correspondence clear.

Define all label offsets as `pub const` values rather than deriving them from a
cumulative-sum chain at build time. The values are already computed and stable; the
constants serve as documentation anchors and power the `seqtbl_offsets` array.
`seqtbl_offsets` is then just an array literal referencing those constants.

Do **not** attempt to reconstruct `seqtbl` from the instruction macros in a Rust
`const` context — Rust has no stable const array concatenation, and the gain in
readability is not worth a proc-macro or build-script detour.

---

## Repository changes

```
SDLPoP/
  src/
    seqtbl.c           ← excluded from cc build (file stays on disk for reference)
  rust/
    src/
      lib.rs           ← add `pub mod seqtbl;`
      seqtbl.rs        ← new: full Rust port
  build.rs             ← remove "src/seqtbl.c" from C source list; add rerun-if-changed
  docs/plans/
    3-port-seqtbl.md   ← this file
```

No new C wrapper file is needed — there are no SDL types to stub out.

---

## Step-by-step checklist

### Step 1 — Instruction and opcode constants

Define the `SEQ_*` byte values from `types.h` as Rust constants. These are used only
to construct the teleport extension; the base 2310 bytes are copied verbatim.

```rust
const SEQ_DX:             u8 = 0xFB;
const SEQ_DY:             u8 = 0xFA;
const SEQ_FLIP:           u8 = 0xFE;
const SEQ_JMP_IF_FEATHER: u8 = 0xF7;
const SEQ_JMP:            u8 = 0xFF;
const SEQ_ACTION:         u8 = 0xF9;
const SEQ_SET_FALL:       u8 = 0xF8;
const SEQ_SOUND:          u8 = 0xF2;
const SEQ_END_LEVEL:      u8 = 0xF1;
const SEQ_GET_ITEM:       u8 = 0xF3;
```

### Step 2 — Label offset constants

Translate every C `#define` label to a `pub const` with type `u16`. Preserve the
cumulative comment from the C source so the offset chain is auditable.

```rust
pub const SEQTBL_BASE: u16 = 0x196E;

pub const RUNNING:          u16 = SEQTBL_BASE;            // offset 0     0x196E
pub const STARTRUN:         u16 = RUNNING        +  5;    // offset 5     0x1973
pub const RUNSTT1:          u16 = STARTRUN       +  2;    // offset 7     0x1975
// ... (all ~90 labels from seqtbl.c lines 45–202)

#[cfg(feature = "teleports")]
pub const TELEPORT:         u16 = MCLIMB_LOOP    +  4;   // offset 2307  0x2075
#[cfg(feature = "teleports")]
pub const TELEPORT_LOOP:    u16 = TELEPORT        + 38;   // offset 2345  0x209B
```

> **Note on USE_TELEPORTS:** The C build uses a `#ifdef`. Map this to a Cargo feature
> named `teleports`, enabled by default in `Cargo.toml`. This keeps the conditional
> compilation consistent with the C side. All `#[cfg(feature = "teleports")]` blocks
> gate on this feature.

### Step 3 — The `seqtbl` byte array

Declare the static. The base 2310 bytes come from `original_seqtbl` in `seqtbl.c`,
copied verbatim. The `USE_TELEPORTS` extension (42 bytes) is appended in a
`#[cfg(feature = "teleports")]` block with inline comments derived from the C source.

Because `apply_seqtbl_patches()` writes to `seqtbl` at runtime, it must be
`static mut`. Wrap the definition in a single `unsafe` block, with a comment
explaining the invariant: only `apply_seqtbl_patches` ever writes, and it is called
exactly once during initialization.

```rust
// Base: 2310 bytes, byte-for-byte match with original DOS seqtbl.
// Teleport extension (42 bytes) is appended when the "teleports" feature is enabled.
#[no_mangle]
pub static mut seqtbl: [u8; SEQTBL_LEN] = [
    // === Base (original DOS bytes) ===
    0xF9, 0x01, 0xFF, 0x81, 0x19, /* ... 2305 more bytes ... */

    // === USE_TELEPORTS extension ===
    // teleport: act(actions_5_bumped)
    SEQ_ACTION, 5,
    // dx(-5), dy(-1), snd(SND_FOOTSTEP), frame_217
    SEQ_DX, (-5i8) as u8, SEQ_DY, (-1i8) as u8, SEQ_SOUND, 1, 0xD9,
    // ... remaining 35 bytes of teleport + teleport_loop ...
];
```

`SEQTBL_LEN` is a const computed from whether `teleports` feature is enabled:

```rust
#[cfg(not(feature = "teleports"))]
const SEQTBL_LEN: usize = 2310;
#[cfg(feature = "teleports")]
const SEQTBL_LEN: usize = 2352;
```

### Step 4 — The `seqtbl_offsets` array

This is a direct translation of the C `seqtbl_offsets[]` array (lines 205–238),
substituting `#define` names with the Rust `const` labels from Step 2.

```rust
#[no_mangle]
pub static seqtbl_offsets: [u16; SEQTBL_OFFSETS_LEN] = [
    0x0000,  STARTRUN,   STAND,      STANDJUMP,
    RUNJUMP, TURN,       RUNTURN,    STEPFALL,
    // ... (115 entries matching seqtbl_offsets[] in seqtbl.c) ...
    #[cfg(feature = "teleports")]
    TELEPORT,
];
```

`SEQTBL_OFFSETS_LEN`: 115 without teleports, 116 with.

### Step 5 — `apply_seqtbl_patches`

This function reads the `fixes` pointer from the C global and conditionally overwrites
one byte in `seqtbl`:

```rust
#[no_mangle]
pub unsafe extern "C" fn apply_seqtbl_patches() {
    // FIX_WALL_BUMP_TRIGGERS_TILE_BELOW: change bumpfall+1 from actions_5_bumped
    // to actions_3_in_midair so a wall bump doesn't trigger the tile below.
    if (*fixes).fix_wall_bump_triggers_tile_below != 0 {
        seqtbl[(BUMPFALL + 1 - SEQTBL_BASE) as usize] = 3; // actions_3_in_midair
    }
}
```

`fixes`, `BUMPFALL` (as a label const), and the action values are all available from
the C bindings or from the consts in this file. Use `(*fixes).fix_wall_bump_triggers_tile_below`
via the FFI binding.

### Step 6 — `check_seqtable_matches_original`

Port this as a no-op stub. The function is only called when
`CHECK_SEQTABLE_MATCHES_ORIGINAL` is defined in `config.h`, which is commented out by
default. A stub is sufficient to satisfy the linker; the validation logic is covered
by the Rust tests in Step 10.

```rust
#[no_mangle]
pub unsafe extern "C" fn check_seqtable_matches_original() {
    // Validation is covered by Rust tests; this stub satisfies the C caller.
}
```

If future work needs the full validator, it can call `check_seqtbl()` from the test
suite (Step 10).

### Step 7 — Wire up `lib.rs`

- [ ] Add `pub mod seqtbl;` to `rust/src/lib.rs`.

### Step 8 — Update `build.rs`

- [ ] In the C source file list, remove `"src/seqtbl.c"`.
- [ ] Add `rerun-if-changed` for `rust/src/seqtbl.rs`.

### Step 9 — Verify

- [ ] `cargo build` compiles without errors or link-time duplicate-symbol warnings.
- [ ] `cargo run` launches the game identically to before.
- [ ] `cargo test` passes all existing tests.

### Step 10 — New tests

Tests live in `rust/src/seqtbl.rs` under `#[cfg(test)]`. They call into the exported
statics directly (no C setup needed — `seqtbl` is pure Rust data).

#### `seqtbl_length_is_correct`

Verify `SEQTBL_LEN` matches the expected value for the current feature set.

| Feature | Expected `SEQTBL_LEN` |
|---|---|
| teleports off | 2310 |
| teleports on | 2352 |

#### `seqtbl_base_matches_original_dos_bytes`

The first 2310 bytes of `seqtbl` must equal `original_seqtbl` from the C file.
Embed the same hex bytes as a `const` slice in the test and assert equality.
Spot-check a few named positions:

| Index (from base 0) | Expected byte | Rationale |
|---|---|---|
| 0 | `0xF9` | `SEQ_ACTION` — first byte of `running` sequence |
| 1 | `0x01` | `actions_1_run_jump` |
| 2 | `0xFF` | `SEQ_JMP` |
| 3 | `0x81` | lo-byte of `runcyc1` address (0x1981) |
| 4 | `0x19` | hi-byte of `runcyc1` address |

#### `seqtbl_offsets_match_original_dos_values`

Compare `seqtbl_offsets[0..115]` against the known-good values from
`original_seqtbl_offsets[]` in `seqtbl.c`. A selection of spot-checks:

| Index | Expected value | Name |
|---|---|---|
| 0 | `0x0000` | (sentinel zero) |
| 1 | `0x1973` | `startrun` |
| 2 | `0x19A0` | `stand` |
| 86 | `0x196E` | `running` (index 86 in the C table) |
| 114 | `0x2257` | `mraise` (last base entry) |

#### `label_constants_are_self_consistent`

The labels are a cumulative chain. Spot-check that each label equals its predecessor
plus the documented stride. Failures here indicate a transcription error.

| Assertion | Value |
|---|---|
| `STARTRUN == RUNNING + 5` | `0x1973` |
| `RUNSTT1 == STARTRUN + 2` | `0x1975` |
| `STAND == RUNCYC7 + 11` | `0x19A0` |
| `CLIMBSTAIRS_LOOP == CLIMBSTAIRS + 81` | `0x212E` |

#### `apply_seqtbl_patches_leaves_seqtbl_unchanged_when_fix_disabled`

Call `set_options_to_default()` (which sets `use_fixes_and_enhancements = 0`, pointing
`fixes` at `fixes_disabled_state` where all fix fields are 0). Then call
`apply_seqtbl_patches()`. Assert that `seqtbl[(BUMPFALL + 1 - SEQTBL_BASE) as usize]`
is still `5` (`actions_5_bumped`) — the default value from the array literal.

#### `apply_seqtbl_patches_writes_correct_byte_when_fix_enabled`

Enable the fix: call `turn_fixes_and_enhancements_on_off(1)`, then set
`(*fixes_saved).fix_wall_bump_triggers_tile_below = 1` directly via FFI. Call
`apply_seqtbl_patches()`. Assert that `seqtbl[(BUMPFALL + 1 - SEQTBL_BASE) as usize]`
is now `3` (`actions_3_in_midair`).

---

## Key pitfalls

1. **`seqtbl` must be `static mut`.** `apply_seqtbl_patches` writes to it, so a
   shared `static` is not enough. Access the array in tests via `unsafe` blocks and
   note that test isolation matters — tests that call `apply_seqtbl_patches` must
   restore the patched byte afterward, or run in a single-threaded context.

2. **`seqtbl_offsets` entry 0 is `0x0000`, not `RUNNING`.** This is a sentinel;
   `seg005.c` never calls `do_fall(0)`. Do not substitute `RUNNING` here — keep
   `0x0000` to match the C array and the DOS original exactly.

3. **`jmp` addresses in the teleport extension embed absolute addresses** (little-endian
   `u16`). Compute `TELEPORT_LOOP` first, then encode it as
   `[SEQ_JMP, (TELEPORT_LOOP & 0xFF) as u8, (TELEPORT_LOOP >> 8) as u8]`. A
   miscounted byte offset here will silently corrupt the animation loop.

4. **`seqtbl` label offsets are absolute DOS addresses, not array indices.** When
   `apply_seqtbl_patches` writes `seqtbl[(BUMPFALL + 1 - SEQTBL_BASE) as usize]`,
   the subtraction converts the absolute address to a zero-based array index. Do
   not forget the `- SEQTBL_BASE` in any direct array access.

5. **`seqtbl_offsets` uses `u16` (`word` in C).** Confirm the FFI binding in
   `bindings.rs` agrees. If bindgen emits `u16` for `word`, no cast is needed;
   if it emits `c_ushort` import that type instead.

---

## Out of scope

- Porting the human-readable macro form of `seqtbl[]` using Rust proc-macros or
  const array concatenation. The byte-literal approach is sufficient and simpler.
- Porting `seg005.c` or `seg006.c` (the seqtbl consumers).
- Windows or macOS build support.
