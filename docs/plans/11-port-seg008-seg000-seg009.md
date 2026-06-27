# Plan: Port seg008, seg000, seg009

## Current status

- seg001–seg007 all ported and harness-verified (263 frames, no divergence) on master.
- Remaining: seg008.c (2068 lines), seg000.c (2513 lines), seg009.c (4248 lines).
- Recommended order: **seg008 → seg000 → seg009**.

## Model selection

- **pop-porter agents: always use `model: "opus"`**. The trap categories in this codebase require holding many type constraints simultaneously; Sonnet gets stuck. The working seg001–seg007 ports were done with Opus.
- **Divergence debugging: switch main session to Opus + max effort** (`/model opus`, `/effort max`) when `--gen-test` + `--dump-tick` don't resolve the issue in one pass.
- **Orchestration and harness runs**: Sonnet + `/effort high` is fine.

## Verification gate between each file

After each file is ported and before starting the next:
```sh
cargo build 2>&1 | grep '^error'   # must be empty
scripts/run_harness.sh              # must pass (harness does NOT auto-rebuild; cargo build must run first)
```

Note: `scripts/run_harness.sh --build` combines both steps if preferred.

---

## Phase 1: seg008.c — Room renderer

### Setup checklist

1. Create `rust/src/seg008.rs` with module boilerplate (see CLAUDE.md).
2. Add SDL extern "C" block: `SDL_ConvertSurface`, `SDL_SetSurfacePalette`, `SDL_SetSurfaceBlendMode`, `SDL_SetColorKey`, `SDL_SetSurfaceAlphaMod`, `SDL_UpperBlit`, `SDL_FreeSurface`, `malloc`, `free`, `memset`.
3. Declare file-local statics: `drawn_row`, `draw_bottom_y`, `draw_main_y`, `drawn_col`, `tile_left`, `modifier_left`, `gate_top_y`, `gate_openness`, `gate_bottom_y`.
4. Declare function pointer: `type add_table_fn = unsafe extern "C" fn(c_short, c_int, i8, i8, c_int, c_int, u8) -> c_int; static mut ptr_add_table: add_table_fn = add_backtable;`

### Table transcription

Do NOT hand-transcribe `tile_table[31]`. Options:
- Copy from old branch: `git show worktree-agent-a99a6259c842dc9b8:rust/src/seg008.rs | grep -A 35 'static tile_table'`
- Or use a Python script to emit Rust struct literals from the C source.

Same for the small tables (`col_xh`, `doortop_fram_top`, `door_fram_top`, `blueline_fram*`, `spikes_fram_right`).

### goto patterns

`goto label_wall_continued` (line ~1221): forward jump in a wall-drawing `if` chain. Use labeled block:
```rust
'wall_block: {
    if some_condition {
        // wall-specific setup
        break 'wall_block;
    }
    // label_wall_continued:
    // common tail
}
```

`goto shadow` (line ~1584): conditional backward jump to call shadow overlay. Use a bool flag:
```rust
let draw_shadow = united_with_shadow != 0 && (united_with_shadow % 2) == 0;
// ... main path ...
if draw_shadow { draw_shadow_overlay(); }
```

### Porting batches

1. File-scope data + type alias declarations (no functions)
2. `redraw_room`, `load_room_links`, `draw_room` skeleton
3. `draw_tile`, `get_tile_to_draw`, `draw_objtable_item`
4. Wall drawing (`draw_wall`, `draw_main_wall`, etc.)
5. Gate/door rendering (`draw_gate_frame`, `draw_leveldoor`, etc.)
6. Object drawing (`draw_object_pic`, `draw_chtab_image`, `blit_frame`)
7. Remaining functions

After each batch: `cargo check`. After all: `scripts/run_harness.sh`.

### Integration

```sh
# Add to rust/src/lib.rs:
pub mod seg008;

# Remove from build.rs sources array only:
# "src/seg008.c",
# DO NOT remove from src/CMakeLists.txt — that controls the C oracle binary for --regen.
```

**Old branch reference:** `git show worktree-agent-a99a6259c842dc9b8:rust/src/seg008.rs` — use for structure, tables, and SDL extern block. **Warning:** that branch had a wrapping arithmetic bug fixed in commit `57942a1`. Audit every `u16`/`word` addition/subtraction and use `wrapping_add`/`wrapping_sub`.

---

## Phase 2: seg000.c — Main loop

### Key challenges

**setjmp/longjmp restart loop** (`start_game`, lines ~200–210):

First, check if `libc` is a Cargo dependency: `grep '^libc' Cargo.toml`.

If `libc` is available: `use libc::{setjmp, longjmp, jmp_buf};` and `static mut setjmp_buf: libc::jmp_buf = unsafe { std::mem::zeroed() };`

Otherwise, declare inline. `jmp_buf` is 200 bytes on x86-64 Linux (verify: `grep '_JBLEN\|jmp_buf' /usr/include/x86_64-linux-gnu/bits/setjmp.h`):

```rust
static mut first_start: u16 = 1;
static mut setjmp_buf: [u8; 200] = [0u8; 200];  // x86-64 Linux only

extern "C" {
    fn setjmp(env: *mut u8) -> c_int;
    fn longjmp(env: *mut u8, val: c_int) -> !;
}

pub unsafe extern "C" fn start_game() {
    if first_start != 0 {
        first_start = 0;
        setjmp(setjmp_buf.as_mut_ptr());
    } else {
        longjmp(setjmp_buf.as_mut_ptr(), -1);
    }
    // ...
}
```

**`process()` macro** in `quick_process` (lines ~256–310): first grep for the function pointer type: `grep 'process_func_type' target/debug/build/sdlpop-*/out/bindings.rs`. Then expand each `process(x)` call to `ok = ok && process_func.unwrap()(&mut x as *mut _ as *mut c_void, std::mem::size_of_val(&x))`. Or use a Rust macro:
```rust
macro_rules! process {
    ($func:expr, $ok:expr, $x:expr) => {
        $ok = $ok && ($func)(&mut $x as *mut _ as *mut std::ffi::c_void, std::mem::size_of_val(&$x));
    }
}
```

**`goto error` in quicksave** (lines ~2153, ~2188): forward jump to cleanup. Use labeled block with `break 'error_block` or restructure as early return with cleanup.

**SDL timer callback** — `SDL_AddTimer` takes `Option<unsafe extern "C" fn(u32, *mut c_void) -> u32>`. Declare callback with correct signature.

### Porting batches

1. File-local statics + SDL extern block
2. `pop_main`, `start_game`, `main_loop`
3. Input handling (`read_joyst_control`, `read_keyb_control`, `process_key`)
4. Sound functions (`enable_sounds`, `play_sound`, `load_sound`)
5. Sprite/image loading (`load_chtab`, `get_chtab_data`)
6. HP display (`draw_hp`, `flash_hp`)
7. `quick_process` + quicksave/quickload
8. Remaining (cutscene hooks, copy protection, etc.)

---

## Phase 3: seg009.c — Platform layer

### Key challenges

**File-local static functions** (not in proto.h / bindings.rs):
```rust
unsafe fn open_dat_from_root_or_data_dir(filename: *const c_char) -> *mut FILE { ... }
unsafe fn load_font_character_offsets(data: *mut rawfont_type) { ... }
// (do NOT export these as #[no_mangle])
```

**`hc_font_data`** — extract from C source with Python:
```python
# Find the array between 'byte hc_font_data[] = {' and '};'
# emit as Rust: static hc_font_data: &[u8] = &[...];
```

**Audio callback:**
```rust
pub unsafe extern "C" fn audio_callback(_userdata: *mut c_void, stream: *mut u8, len: c_int) {
    // ...
}
// Pass to SDL: desired.callback = Some(audio_callback);
```

**POSIX dir listing:**
```rust
extern "C" {
    fn opendir(name: *const c_char) -> *mut libc::DIR;
    fn readdir(dirp: *mut libc::DIR) -> *mut libc::dirent;
    fn closedir(dirp: *mut libc::DIR) -> c_int;
}
// Or: use libc::{opendir, readdir, closedir};  (if libc dep in Cargo.toml)
```

**Decompression routines** — pure pointer arithmetic, no special patterns. Port mechanically.

### Porting batches

1. File-local statics, helper declarations, SDL extern block
2. `hc_font_data` array + font loading functions
3. DAT file loading (`open_dat`, `find_dat_entry`, `load_from_dat`)
4. Path resolution (`get_data_path`, `get_replay_path`, directory listing)
5. SDL init (`init_sdl`, `init_video`, `init_audio`)
6. `audio_callback` + audio mix functions
7. Decompression (`decomp_chtab`, `decomp_dat`, palette loading)
8. Remaining (screenshots, lighting init, etc.)

### Harness note

seg009 provides SDL init. After porting, run the binary manually first:
```sh
./target/debug/prince validate replays/recorded_replay.p1r
```
Verify the trace file is written (check `POPTRACE_OUT`), then run the harness.

---

## Reference material

- Working seg008 port on old branch: `git show worktree-agent-a99a6259c842dc9b8:rust/src/seg008.rs`
- All patterns documented in CLAUDE.md under "Remaining files — per-file porting guide"
- Old branch for seg001–seg007 patterns: `worktree-agent-a99a6259c842dc9b8`
