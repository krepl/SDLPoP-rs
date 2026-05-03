# Plan 2: Port options.c to Rust

**Goal:** Replace `src/options.c` with a Rust implementation. All public symbols are
re-exported with `#[no_mangle] extern "C"` so the rest of the C codebase calls them
transparently. The game must run identically after the swap; all existing tests must
continue to pass, and new tests are added to cover the ported logic.

---

## Scope

`options.c` has 838 lines. Nearly everything is portable — no SDL in the core logic.
Two small functions at the bottom (`process_rw_write`, `process_rw_read`) take
`SDL_RWops*`, which is not in our filtered bindings. They are 2 lines each and stay in
C (moved to a new stub file `src/sdl_rw_stubs.c`). Everything else moves to Rust.

Functions / groups to port:

| Function(s) | Notes |
|---|---|
| `turn_fixes_and_enhancements_on_off`, `turn_custom_options_on_off` | Pointer swaps into C globals; accessed via FFI |
| `ini_load` | stdio-based parser; rewritten using `BufReader` |
| name/kv lists (`use_hardware_acceleration_names`, etc.) | Static data; become Rust `const` arrays |
| `ini_get_named_value`, `ini_process_boolean`, `ini_process_*` | String matching helpers |
| `global_ini_callback`, `mod_ini_callback` | Main INI dispatch; complex but mechanical |
| `set_options_to_default` | Sets C globals via FFI |
| `load_global_options`, `check_mod_param` | Call other C functions via FFI |
| `identify_dos_exe_version`, `read_exe_bytes` | Pure computation; trivially portable |
| `load_dos_exe_modifications` | File I/O + lots of hardcoded EXE offsets |
| `load_mod_options` | Calls `locate_file`, `show_dialog` via FFI |

Functions staying in C (moved to `src/sdl_rw_stubs.c`):

| Function | Reason |
|---|---|
| `process_rw_write` | Takes `SDL_RWops*`, not in bindings |
| `process_rw_read` | Same |

---

## Repository changes

```
SDLPoP/
  src/
    options.c          ← deleted from cc build (file stays on disk for reference; remove later)
    sdl_rw_stubs.c     ← new: process_rw_write, process_rw_read
  rust/
    src/
      lib.rs           ← add `pub mod options;`
      options.rs       ← new: the full Rust port
  build.rs             ← swap options.c for sdl_rw_stubs.c in the file list
  docs/plans/
    2-port-options.md  ← this file
```

---

## Step-by-step checklist

### Step 1 — Create `src/sdl_rw_stubs.c`

- [ ] Create `src/sdl_rw_stubs.c` containing only `process_rw_write` and `process_rw_read`
      (copied verbatim from `options.c`), with `#include "common.h"` at the top.

### Step 2 — Update `build.rs`

- [ ] In the C source file list, replace `"src/options.c"` with `"src/sdl_rw_stubs.c"`.
- [ ] Add `rerun-if-changed` entries for `src/sdl_rw_stubs.c` and `rust/src/options.rs`.

### Step 3 — Create `rust/src/options.rs`

This is the main work. The functions below must be exported with
`#[no_mangle] pub unsafe extern "C"` so the existing C callers find them in the linked
binary. All C globals (`fixes`, `custom`, `fixes_saved`, etc.) are accessed via the
FFI bindings in `lib.rs`.

#### 3a — Name and key/value lists

The C macros `NAMES_LIST` and `KEY_VALUE_LIST` declare static arrays and a
`names_list_type` struct that points into them. In Rust, represent these as:

```rust
const LEVEL_TYPE_NAMES: &[&[u8]] = &[b"dungeon\0", b"palace\0"];
// etc.
```

Matching in `ini_get_named_value` then iterates these slices with `strcasecmp` via
FFI, or with Rust's own case-insensitive comparison after converting from `*const c_char`.

Because the C `ini_get_named_value` callback signature passes a `*mut names_list_type`,
the simplest approach is to keep the C-compatible struct layout and build the
`names_list_type` values as Rust statics with `unsafe` initializers, exactly mirroring
the C layout. This avoids any ABI mismatch.

Alternatively, inline the lookup logic into `global_ini_callback` directly (skipping
the intermediate struct) — this is simpler and avoids the packed-struct hazard.

**Recommended:** inline the lookups. The `names_list_type` C struct is an internal
implementation detail used only within `options.c`.

#### 3b — `ini_load`

The C version uses `fscanf` with format strings. Replace it with idiomatic Rust using
`std::fs::read_to_string` + `str::lines()` + standard string combinators:

```rust
fn ini_load(path: &Path, mut report: impl FnMut(&str, &str, &str)) -> i32 {
    let Ok(content) = std::fs::read_to_string(path) else { return -1; };
    let mut section = "";

    for line in content.lines() {
        let line = match line.split_once(';') {
            Some((before, _)) => before,  // strip inline comment
            None => line,
        }.trim();

        if line.is_empty() { continue; }

        if let Some(inner) = line.strip_prefix('[') {
            section = inner.split_once(']').map(|(s, _)| s.trim()).unwrap_or("");
            continue;
        }

        let (name, value) = match line.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => (line, ""),  // name with no '='; matches C's cnt==1 case
        };
        report(section, name, value);
    }
    0
}
```

`split_once`, `strip_prefix`, `trim`, and `lines` are all in `std` — no crates needed.
`read_to_string` reads the whole file before parsing, which is fine for config files
and simpler to reason about than the C stream-position-based `fscanf` loop.

The callback signature changes from C's
`int (*report)(const char*, const char*, const char*)` to a Rust closure
`FnMut(&str, &str, &str)`. `ini_load` itself is not called from other C
modules directly (it's only called from `load_global_options` and `load_mod_options`,
both ported here), so it does not need a C ABI export.

#### 3c — `global_ini_callback` and `mod_ini_callback`

These are the largest functions. The C version is a single function full of
`ini_process_*` macro expansions. In Rust:

- Write a `process_boolean(name: &str, value: &str, option_name: &str, target: *mut u8) -> bool`
  helper that does a case-insensitive name check and writes 0/1.
- Write `process_word`, `process_byte`, `process_sbyte`, `process_short`, `process_int`
  analogues.
- The `global_ini_callback` body becomes a series of `if` / `else if` branches
  grouped by section, calling those helpers.
- `vga_color_N` parsing (the RGB loop on lines 320–344 of `options.c`) ports
  straightforwardly to Rust iterators.
- `#ifdef USE_MENU`, `#ifdef USE_REPLAY`, `#ifdef USE_LIGHTING` blocks: check
  whether the relevant globals exist in bindings. They will if those features were
  compiled into the C library. Use `#[cfg(feature = "...")]` or simply always
  include the code — the globals are present unconditionally in a standard build.

These functions do NOT need C ABI exports; they are closures or internal Rust
functions called only by `load_global_options` / `load_mod_options`.

#### 3d — `set_options_to_default`

```rust
#[no_mangle]
pub unsafe extern "C" fn set_options_to_default() {
    enable_music = 1;
    enable_fade  = 1;
    // ... etc., mirroring the C function exactly
    memset(
        &raw mut fixes_saved as *mut _,
        1,
        std::mem::size_of_val(&fixes_saved),
    );
    custom_saved = custom_defaults;
    turn_fixes_and_enhancements_on_off(0);
    turn_custom_options_on_off(0);
}
```

All globals (`enable_music`, `fixes_saved`, `custom_saved`, `custom_defaults`) are
`extern` symbols from the C library, already in bindings.

#### 3e — `turn_fixes_and_enhancements_on_off` / `turn_custom_options_on_off`

```rust
#[no_mangle]
pub unsafe extern "C" fn turn_fixes_and_enhancements_on_off(new_state: u8) {
    use_fixes_and_enhancements = new_state;
    fixes = if new_state != 0 { &raw mut fixes_saved } else { &raw mut fixes_disabled_state };
}
```

#### 3f — `identify_dos_exe_version` / `read_exe_bytes`

Pure computation; translate directly.

```rust
#[no_mangle]
pub unsafe extern "C" fn identify_dos_exe_version(filesize: c_int) -> c_int {
    match filesize {
        123335 => 0, // dos_10_packed
        125115 => 2, // dos_13_packed
        // ...
        _ => -1,
    }
}
```

#### 3g — `load_dos_exe_modifications`

This is the longest function — mostly a table of hardcoded EXE offsets followed by
`process(dest, size, {offsets})` macro expansions. In Rust, translate each `process`
call to a `read_exe_bytes` call with the same offset table as a `[i32; 6]` array.
The logic is mechanical but verbose; translate it literally rather than trying to
abstract it.

Calls `create_directory_listing_and_find_first_file`, `get_current_filename_from_directory_listing`,
`find_next_file`, `close_directory_listing` — all available in bindings via seg009.c.

#### 3h — `check_mod_param` / `load_global_options` / `load_mod_options`

These call `check_param`, `locate_file`, `show_dialog`, `file_exists`, `stat` — all
available via FFI or the standard library. Translate directly.

### Step 4 — Wire up `lib.rs`

- [ ] Add `pub mod options;` to `rust/src/lib.rs`.

### Step 5 — Verify

- [ ] `cargo build` compiles without errors.
- [ ] `cargo run` launches the game identically to `make && ./prince`.
- [ ] `cargo test` passes all existing tests.
- [ ] `make -C src` still works (no C files were removed, only excluded from the Rust build).

### Step 6 — Add new tests

Tests live in `rust/src/options.rs` under `#[cfg(test)]`. Each test calls
`set_options_to_default()` in a `setup()` fixture to bring globals to a known
baseline before asserting.

#### `identify_dos_exe_version`

| Input (filesize) | Expected output | Rationale |
|---|---|---|
| `123335` | `0` (dos_10_packed) | known size |
| `129504` | `1` (dos_10_unpacked) | known size |
| `125115` | `2` (dos_13_packed) | known size |
| `129472` | `3` (dos_13_unpacked) | known size |
| `110855` | `4` (dos_14_packed) | known size |
| `115008` | `5` (dos_14_unpacked) | known size |
| `123334` | `-1` | one byte under a known size |
| `123336` | `-1` | one byte over a known size |
| `0` | `-1` | empty file |
| `i32::MAX` | `-1` | large unknown size |

#### `ini_load` — parser behaviour

Each row is a self-contained call with a synthetic in-memory INI string written to a
temp file. The "calls" column counts how many times the callback fires; "section /
name / value" are what the callback receives.

| Input | Calls | section | name | value | Rationale |
|---|---|---|---|---|---|
| _(empty file)_ | 0 | — | — | — | nothing to parse |
| `; just a comment` | 0 | — | — | — | comment-only line |
| `[MySection]` | 0 | — | — | — | section with no keys |
| `[MySection]\nkey = val` | 1 | `"MySection"` | `"key"` | `"val"` | basic happy path |
| `[A]\nk=v\n[B]\nk=w` | 2 | `"A"` then `"B"` | `"k"` both | `"v"` then `"w"` | section tracking across multiple sections |
| `[Sec] ; comment\nk=v` | 1 | `"Sec"` | `"k"` | `"v"` | inline comment on section line stripped |
| `k = v ; comment` | 1 | `""` | `"k"` | `"v"` | inline comment on value line stripped |
| `  key  =  value  ` | 1 | `""` | `"key"` | `"value"` | whitespace trimmed from name and value |
| `key =` | 1 | `""` | `"key"` | `""` | empty value |
| `key` | 1 | `""` | `"key"` | `""` | no `=` sign; matches C `cnt==1` case |
| `key = v1\nkey = v2` | 2 | `""` both | `"key"` both | `"v1"` then `"v2"` | repeated key fires callback twice |

#### `global_ini_callback` — boolean options

Test by calling `set_options_to_default()` then calling the Rust `global_ini_callback`
directly (it is an internal function, not exported), then reading the global.

| section | name | value | Expected global | Expected value | Rationale |
|---|---|---|---|---|---|
| `General` | `enable_music` | `"false"` | `enable_music` | `0` | basic false |
| `General` | `enable_music` | `"true"` | `enable_music` | `1` | basic true |
| `General` | `enable_music` | `"False"` | `enable_music` | `0` | case-insensitive |
| `General` | `enable_music` | `"TRUE"` | `enable_music` | `1` | case-insensitive |
| `General` | `enable_music` | `"yes"` | `enable_music` | `1` (unchanged) | invalid value; no write |
| `General` | `enable_fade` | `"false"` | `enable_fade` | `0` | different bool field |
| `General` | `start_fullscreen` | `"true"` | `start_fullscreen` | `1` | default is 0; verify it changes |

#### `global_ini_callback` — `use_fixes_and_enhancements` special case

This key accepts three values instead of the normal true/false two.

| section | name | value | Expected | Rationale |
|---|---|---|---|---|
| `Enhancements` | `use_fixes_and_enhancements` | `"true"` | `1` | normal enable |
| `Enhancements` | `use_fixes_and_enhancements` | `"false"` | `0` | normal disable |
| `Enhancements` | `use_fixes_and_enhancements` | `"prompt"` | `2` | third state unique to this key |
| `Enhancements` | `use_fixes_and_enhancements` | `"PROMPT"` | `2` | case-insensitive |

#### `global_ini_callback` — numeric and named-value options

| section | name | value | Expected global | Expected value | Rationale |
|---|---|---|---|---|---|
| `General` | `pop_window_width` | `"800"` | `pop_window_width` | `800` | decimal integer |
| `General` | `pop_window_width` | `"0x320"` | `pop_window_width` | `800` | hex integer |
| `General` | `scaling_type` | `"sharp"` | `scaling_type` | `0` | named value lookup |
| `General` | `scaling_type` | `"fuzzy"` | `scaling_type` | `1` | named value lookup |
| `General` | `scaling_type` | `"blurry"` | `scaling_type` | `2` | named value lookup |
| `General` | `scaling_type` | `"SHARP"` | `scaling_type` | `0` | case-insensitive named value |
| `General` | `scaling_type` | `"default"` | `scaling_type` | `0` (unchanged) | "default" → no write |
| `CustomGameplay` | `start_minutes_left` | `"60"` | `custom_saved.start_minutes_left` | `60` | custom struct field |

#### `global_ini_callback` — `vga_color_N` RGB parsing

| name | value | Expected `r` | Expected `g` | Expected `b` | Rationale |
|---|---|---|---|---|---|
| `vga_color_0` | `"255, 255, 255"` | `63` | `63` | `63` | max values divided by 4 |
| `vga_color_0` | `"0, 0, 0"` | `0` | `0` | `0` | zero values |
| `vga_color_0` | `"4, 8, 12"` | `1` | `2` | `3` | division rounds down |
| `vga_color_0` | `"default"` | _(unchanged)_ | _(unchanged)_ | _(unchanged)_ | "default" → no write |
| `vga_color_15` | `"255, 0, 0"` | `63` | `0` | `0` | last valid index |
| `vga_color_16` | `"255, 0, 0"` | _(unchanged)_ | _(unchanged)_ | _(unchanged)_ | index out of range 0–15; ignored |

#### `global_ini_callback` — `[Level N]` section

| section | name | value | Expected | Rationale |
|---|---|---|---|---|
| `Level 0` | `level_type` | `"dungeon"` | `tbl_level_type[0] == 0` | named value, first index |
| `Level 0` | `level_type` | `"palace"` | `tbl_level_type[0] == 1` | named value |
| `Level 0` | `level_type` | `"PALACE"` | `tbl_level_type[0] == 1` | case-insensitive |
| `Level 15` | `guard_hp` | `"3"` | `tbl_guard_hp[15] == 3` | last valid index |
| `Level 16` | `guard_hp` | `"3"` | _(unchanged)_ | out of range; ignored |
| `Level -1` | `guard_hp` | `"3"` | _(unchanged)_ | negative; ignored |

#### `global_ini_callback` — `[Skill N]` section

| section | name | value | Expected | Rationale |
|---|---|---|---|---|
| `Skill 0` | `strikeprob` | `"255"` | `custom_saved.strikeprob[0] == 255` | first skill slot |
| `Skill 0` | `blockprob` | `"128"` | `custom_saved.blockprob[0] == 128` | different field same slot |
| `Skill 7` | `strikeprob` | `"10"` | _(unchanged)_ | `NUM_GUARD_SKILLS` is 7; index 7 is out of range |

#### `turn_fixes_and_enhancements_on_off`

| Call | Expected `use_fixes_and_enhancements` | Expected `fixes` pointer | Rationale |
|---|---|---|---|
| `(0)` | `0` | `== &fixes_disabled_state` | off state |
| `(1)` | `1` | `== &fixes_saved` | on state |
| `(1)` then `(0)` | `0` | `== &fixes_disabled_state` | toggle returns to off |

#### `set_options_to_default`

| What is checked | Expected | Rationale |
|---|---|---|
| `enable_music` | `1` | already tested; keep |
| `enable_fade` | `1` | already tested; keep |
| `enable_flash` | `1` | already tested; keep |
| `enable_text` | `1` | already tested; keep |
| `start_fullscreen` | `0` | already tested; keep |
| `enable_lighting` | `0` | already tested; keep |
| `use_fixes_and_enhancements` | `0` | fixes off by default |
| `fixes == &fixes_disabled_state` | true | pointer set correctly |
| every byte of `fixes_saved` | `1` | memset initializes all fix fields to true |
| called twice → same result | identical | idempotent |

---

## Key pitfalls

1. **`fixes` and `custom` are pointers into C globals.** The pointer assignments in
   `turn_fixes_and_enhancements_on_off` must use `&raw mut` (not `&mut`) to avoid
   creating a Rust reference to a C-owned object.

2. **`memset(&fixes_saved, 1, ...)`.** The C code memsets the entire `fixes_saved`
   struct to 1 to default all bool fields to true. Replicate this exactly —
   don't use a struct literal with every field set to 1, because new fields added
   upstream would silently default to 0.

3. **`ini_load` edge cases.** The Rust parser must handle: inline comments on both
   section and value lines, names with no `=` (empty value), leading/trailing
   whitespace on names and values, and repeated keys in the same section. The test
   table in Step 6 covers all of these explicitly.

4. **`#ifdef` guards.** The C file has `#ifdef USE_MENU`, `#ifdef USE_REPLAY`,
   `#ifdef USE_LIGHTING` blocks. A standard SDLPoP build has all three enabled.
   Include all three unconditionally in the Rust port — if a future build disables
   them, the globals will be absent and the linker will catch it.

5. **`SDL_RWops` functions stay in C.** Do not attempt to port `process_rw_write` /
   `process_rw_read` — `SDL_RWops` is not in the filtered bindings and adding it
   would pull in hundreds of SDL types.

---

## Out of scope

- Making the INI parsing logic idiomatic Rust (error types, `Result`, etc.).
- Porting any other C module.
- Windows or macOS build support.
