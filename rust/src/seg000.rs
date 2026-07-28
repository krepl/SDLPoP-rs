//! Program entry point, game start-up, input polling, sound/sprite loading and the HP display.
//!
//! Ported from `seg000.c`. This is the outermost layer of the game: everything here either runs
//! once during start-up, or runs once per gameplay tick around the level loop that lives in
//! `seg003`.
//!
//! # The three nested loops
//!
//! The game has no single "main loop" function. Control is arranged as three nested cycles:
//!
//! 1. **The game cycle.** [`pop_main`] initialises everything and calls [`init_game_main`], which
//!    ends in [`start_game`]. `start_game` shows the title sequence and enters a level; the level
//!    loop in `seg003` eventually calls back into `start_game` to restart from the title. That
//!    back-edge is implemented with `setjmp`/`longjmp` — see [`start_game`].
//! 2. **The frame cycle.** Per gameplay tick the level loop calls [`play_frame`] (advance the
//!    simulation) then [`draw_game_frame`] (render it and age the bottom-line message).
//! 3. **The input cycle.** [`do_paused`] samples the keyboard/gamepad into the `control_*`
//!    globals, dispatches one keypress through [`process_key`], and — if the game got paused —
//!    spins in a nested read-key loop until the player unpauses.
//!
//! # Control input is a set of globals, not a return value
//!
//! [`read_keyb_control`] and [`read_joyst_control`] both write the same four globals: `control_x`,
//! `control_y`, `control_shift` and (indirectly) `next_room`. They are *accumulating* — the caller
//! ([`do_paused`]) clears them to `CONTROL_RELEASED` first, and each reader only ever sets them.
//! That is why `read_joyst_control` has no `else` branches: a gamepad reading of "neutral" must
//! leave a value another source already set alone.
//!
//! `key_states` entries carry two bits: `KEYSTATE_HELD` (the key is down right now) and
//! `KEYSTATE_HELD_NEW` (it went down since the last tick). The `fix_register_quick_input` fix
//! makes the readers accept either bit, so a keypress shorter than one tick still registers;
//! without it only `KEYSTATE_HELD` counts. `do_paused` clears every `KEYSTATE_HELD_NEW` bit at the
//! end of the tick, which is what makes it mean "new".
//!
//! # Sound priorities
//!
//! At most one sound plays at a time. [`play_sound`] does not play anything — it nominates
//! `next_sound`, keeping whichever candidate has the *numerically lowest* priority in
//! `sound_prio_table` (lower number = more important). [`draw_game_frame`] then calls
//! [`play_next_sound`], which only actually starts it if nothing is playing, or if the currently
//! playing sound is marked interruptible *and* outranked. [`fix_sound_priorities`] retunes three
//! table entries to match PoP 1.3.
//!
//! # Copy protection
//!
//! On the potions level the game asks for a word from the printed manual. [`start_game`] picks 14
//! distinct manual entries (distinct also in their first letter, since the answer is given by
//! drinking the potion whose letter matches) into `cplevel_entr`, and marks one slot
//! `copyprot_plac` as the one that will be asked; [`show_copyprot`] renders the question from the
//! `COPYPROT_WORD`/`LINE`/`PAGE` tables.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_long, c_short, c_void};
use core::ptr::{addr_of, addr_of_mut, null_mut};
use super::*;
use crate::platform::{InputSource, Renderer};

// ---------------------------------------------------------------------------
// SDL / libc externs (not in bindings.rs)
// ---------------------------------------------------------------------------
// setjmp/longjmp are native-only (see start_game's doc comment below for the wasm32
// alternative -- a panic-based retry loop, since wasm32 has no non-local jump primitive).
#[cfg(not(target_arch = "wasm32"))]
extern "C" {
    fn setjmp(env: *mut u8) -> c_int;
    fn longjmp(env: *mut u8, val: c_int) -> !;
}

extern "C" {
    fn mkdir(path: *const c_char, mode: u32) -> c_int;
    fn strcmp(a: *const c_char, b: *const c_char) -> c_int;
    fn strrchr(s: *const c_char, c: c_int) -> *mut c_char;
    fn strcasecmp(a: *const c_char, b: *const c_char) -> c_int;
    fn atoi(s: *const c_char) -> c_int;

    // declared `extern int audio_speed;` in seg000.c (USE_FAST_FORWARD)
    static mut audio_speed: c_int;
}

struct SDL_version {
    major: u8,
    minor: u8,
    patch: u8,
}

const SEEK_CUR: c_int = 1;

// SDL scancodes (not emitted by bindgen)
const SDL_SCANCODE_A: c_int = 4;
const SDL_SCANCODE_B: c_int = 5;
const SDL_SCANCODE_C: c_int = 6;
const SDL_SCANCODE_F: c_int = 9;
const SDL_SCANCODE_G: c_int = 10;
const SDL_SCANCODE_H: c_int = 11;
const SDL_SCANCODE_I: c_int = 12;
const SDL_SCANCODE_J: c_int = 13;
const SDL_SCANCODE_K: c_int = 14;
const SDL_SCANCODE_L: c_int = 15;
const SDL_SCANCODE_N: c_int = 17;
const SDL_SCANCODE_R: c_int = 21;
const SDL_SCANCODE_S: c_int = 22;
const SDL_SCANCODE_T: c_int = 23;
const SDL_SCANCODE_U: c_int = 24;
const SDL_SCANCODE_V: c_int = 25;
const SDL_SCANCODE_W: c_int = 26;
const SDL_SCANCODE_RETURN: c_int = 40;
const SDL_SCANCODE_ESCAPE: c_int = 41;
const SDL_SCANCODE_BACKSPACE: c_int = 42;
const SDL_SCANCODE_TAB: c_int = 43;
const SDL_SCANCODE_SPACE: c_int = 44;
const SDL_SCANCODE_LEFTBRACKET: c_int = 47;
const SDL_SCANCODE_RIGHTBRACKET: c_int = 48;
const SDL_SCANCODE_F6: c_int = 63;
const SDL_SCANCODE_F9: c_int = 66;
const SDL_SCANCODE_HOME: c_int = 74;
const SDL_SCANCODE_PAGEUP: c_int = 75;
const SDL_SCANCODE_RIGHT: c_int = 79;
const SDL_SCANCODE_LEFT: c_int = 80;
const SDL_SCANCODE_DOWN: c_int = 81;
const SDL_SCANCODE_UP: c_int = 82;
const SDL_SCANCODE_KP_MINUS: c_int = 86;
const SDL_SCANCODE_KP_PLUS: c_int = 87;
const SDL_SCANCODE_KP_2: c_int = 90;
const SDL_SCANCODE_KP_4: c_int = 92;
const SDL_SCANCODE_KP_5: c_int = 93;
const SDL_SCANCODE_KP_6: c_int = 94;
const SDL_SCANCODE_KP_7: c_int = 95;
const SDL_SCANCODE_KP_8: c_int = 96;
const SDL_SCANCODE_KP_9: c_int = 97;
const SDL_SCANCODE_CLEAR: c_int = 156;
const SDL_SCANCODE_LSHIFT: c_int = 225;
const SDL_SCANCODE_RSHIFT: c_int = 229;
const SDL_NUM_SCANCODES: usize = 512;

const SDL_CONTROLLER_AXIS_LEFTX: usize = 0;
const SDL_CONTROLLER_AXIS_LEFTY: usize = 1;
const SDL_CONTROLLER_AXIS_RIGHTX: usize = 2;
const SDL_CONTROLLER_AXIS_RIGHTY: usize = 3;
const SDL_CONTROLLER_AXIS_TRIGGERLEFT: usize = 4;
const SDL_CONTROLLER_AXIS_TRIGGERRIGHT: usize = 5;

const WITH_CTRL: c_int = key_modifiers_WITH_CTRL as c_int;
const WITH_SHIFT: c_int = key_modifiers_WITH_SHIFT as c_int;
const KEYSTATE_HELD_I: c_int = KEYSTATE_HELD as c_int;
const KEYSTATE_HELD_NEW_I: c_int = KEYSTATE_HELD_NEW as c_int;

// Modified-key codes, pre-combined so they can be used as `match` patterns.
//
// These MUST be named constants: writing `SDL_SCANCODE_TAB | WITH_CTRL` directly in a pattern
// position would silently parse as an *or-pattern* ("TAB or WITH_CTRL"), not as the bitwise-or
// key code the C `switch` labels mean.
const KEY_SHIFT_ESCAPE: c_int = SDL_SCANCODE_ESCAPE | WITH_SHIFT;
const KEY_SHIFT_F6: c_int = SDL_SCANCODE_F6 | WITH_SHIFT;
const KEY_SHIFT_F9: c_int = SDL_SCANCODE_F9 | WITH_SHIFT;
const KEY_SHIFT_B: c_int = SDL_SCANCODE_B | WITH_SHIFT;
const KEY_SHIFT_C: c_int = SDL_SCANCODE_C | WITH_SHIFT;
const KEY_SHIFT_I: c_int = SDL_SCANCODE_I | WITH_SHIFT;
const KEY_SHIFT_L: c_int = SDL_SCANCODE_L | WITH_SHIFT;
const KEY_SHIFT_S: c_int = SDL_SCANCODE_S | WITH_SHIFT;
const KEY_SHIFT_T: c_int = SDL_SCANCODE_T | WITH_SHIFT;
const KEY_SHIFT_W: c_int = SDL_SCANCODE_W | WITH_SHIFT;
const KEY_CTRL_A: c_int = SDL_SCANCODE_A | WITH_CTRL;
const KEY_CTRL_B: c_int = SDL_SCANCODE_B | WITH_CTRL;
const KEY_CTRL_C: c_int = SDL_SCANCODE_C | WITH_CTRL;
const KEY_CTRL_G: c_int = SDL_SCANCODE_G | WITH_CTRL;
const KEY_CTRL_J: c_int = SDL_SCANCODE_J | WITH_CTRL;
const KEY_CTRL_K: c_int = SDL_SCANCODE_K | WITH_CTRL;
const KEY_CTRL_L: c_int = SDL_SCANCODE_L | WITH_CTRL;
const KEY_CTRL_R: c_int = SDL_SCANCODE_R | WITH_CTRL;
const KEY_CTRL_S: c_int = SDL_SCANCODE_S | WITH_CTRL;
const KEY_CTRL_V: c_int = SDL_SCANCODE_V | WITH_CTRL;
const KEY_CTRL_TAB: c_int = SDL_SCANCODE_TAB | WITH_CTRL;
const KEY_CTRL_SHIFT_TAB: c_int = SDL_SCANCODE_TAB | WITH_CTRL | WITH_SHIFT;

const M_PI: f64 = std::f64::consts::PI;
const DEGREES_TO_RADIANS: f64 = M_PI / 180.0;

// ---------------------------------------------------------------------------
// File-local globals (defined in seg000.c, not exported via headers)
// ---------------------------------------------------------------------------
// data:461E
static mut dathandle: *mut dat_type = null_mut();
// data:4C08
static mut need_redraw_because_flipped: word = 0;
static mut level_var_palettes: *mut byte = null_mut();

// data:02C2
static mut first_start: word = 1;
// data:4C38
#[cfg(not(target_arch = "wasm32"))]
static mut setjmp_buf: [u8; 200] = [0u8; 200];

static mut last_transition_counter: u64 = 0;
// data:42C4
static mut which_quote: word = 0;

// ---------------------------------------------------------------------------
// String / table constants
// ---------------------------------------------------------------------------
const TBL_ENVIR_GR: [&str; 6] = ["", "C", "C", "E", "E", "V"];
const TBL_ENVIR_KI: [&str; 2] = ["DUNGEON", "PALACE"];
static TBL_GUARD_DAT: [&[u8]; 5] = [
    b"GUARD.DAT\0",
    b"FAT.DAT\0",
    b"SKEL.DAT\0",
    b"VIZIER.DAT\0",
    b"SHADOW.DAT\0",
];

static OPTGRAF_MIN: [byte; 8] = [0x01, 0x1E, 0x4B, 0x4E, 0x56, 0x65, 0x7F, 0x0A];
static OPTGRAF_MAX: [byte; 8] = [0x09, 0x1F, 0x4D, 0x53, 0x5B, 0x7B, 0x8F, 0x0D];

// data:017A
static COPYPROT_WORD: [word; 40] = [
    9, 1, 6, 4, 5, 3, 6, 3, 4, 4, 3, 2, 12, 5, 13, 1, 9, 2, 2, 4, 9, 4, 11, 8, 5, 4, 1, 6, 2, 4, 6,
    8, 4, 2, 7, 11, 5, 4, 1, 2,
];
// data:012A
static COPYPROT_LINE: [word; 40] = [
    2, 1, 5, 4, 3, 5, 1, 3, 7, 2, 2, 4, 6, 6, 2, 6, 3, 1, 2, 3, 2, 2, 3, 10, 5, 6, 5, 6, 3, 5, 7,
    2, 2, 4, 5, 7, 2, 6, 5, 5,
];
// data:00DA
static COPYPROT_PAGE: [word; 40] = [
    5, 3, 7, 3, 3, 4, 1, 5, 12, 5, 11, 10, 1, 2, 8, 8, 2, 4, 6, 1, 4, 7, 3, 2, 1, 7, 10, 1, 4, 3,
    4, 1, 4, 1, 8, 1, 1, 10, 3, 3,
];

// data:042E   {top, left, bottom, right}
static rect_titles: rect_type = rect_type { top: 106, left: 24, bottom: 195, right: 296 };
static splash_text_1_rect: rect_type = rect_type { top: 0, left: 0, bottom: 50, right: 320 };
static splash_text_2_rect: rect_type = rect_type { top: 50, left: 0, bottom: 200, right: 320 };

// ---------------------------------------------------------------------------
// Quicksave
// ---------------------------------------------------------------------------
static mut quick_fp: *mut FILE = null_mut();
// "V1.16b4 " + NUL  (COUNT == 9)
static quick_version: [c_char; 9] = [
    b'V' as c_char,
    b'1' as c_char,
    b'.' as c_char,
    b'1' as c_char,
    b'6' as c_char,
    b'b' as c_char,
    b'4' as c_char,
    b' ' as c_char,
    0,
];
static mut quick_control: [c_char; 9] = [
    b'.' as c_char,
    b'.' as c_char,
    b'.' as c_char,
    b'.' as c_char,
    b'.' as c_char,
    b'.' as c_char,
    b'.' as c_char,
    b'.' as c_char,
    0,
];

// ---------------------------------------------------------------------------
// Sound priority tables (seg000:128C-ish)
// ---------------------------------------------------------------------------
static mut sound_prio_table: [byte; 58] = [
    0x14, 0x1E, 0x23, 0x66, 0x32, 0x37, 0x30, 0x30, 0x4B, 0x50, 0x0A, 0x12, 0x0C, 0x0B, 0x69, 0x6E,
    0x73, 0x78, 0x7D, 0x82, 0x91, 0x96, 0x9B, 0xA0, 0x01, 0x01, 0x01, 0x01, 0x01, 0x13, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x01, 0x01, 0x01, 0x01, 0x87, 0x8C, 0x0F, 0x10,
    0x19, 0x16, 0x01, 0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00,
];
static sound_pcspeaker_exists: [byte; 58] = [
    1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0,
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
/// Look up a NUL-terminated command-line parameter; null if it was not passed.
#[inline]
unsafe fn cp(s: &[u8]) -> *const c_char {
    check_param(s.as_ptr() as *const c_char)
}

/// Borrow a C string as `&str`, mapping null and invalid UTF-8 to `""`.
unsafe fn cstr<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    std::ffi::CStr::from_ptr(p).to_str().unwrap_or("")
}

/// `snprintf` into a fixed C buffer: copy `s`, truncate to fit, always NUL-terminate.
unsafe fn cbuf_set(buf: &mut [c_char], s: &str) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(buf.len().saturating_sub(1));
    for i in 0..n {
        buf[i] = bytes[i] as c_char;
    }
    buf[n] = 0;
}

/// `snprintf_check`: like [`cbuf_set`], but a would-be truncation is fatal (`quit(2)`).
unsafe fn snprintf_check_ptr(dst: *mut c_char, size: usize, s: &str) {
    let b = s.as_bytes();
    if b.len() >= size {
        quit(2);
    }
    for i in 0..b.len() {
        *dst.add(i) = b[i] as c_char;
    }
    *dst.add(b.len()) = 0;
}

/// `copyprot_letter` is an extern incomplete array; bindgen emits `[c_char; 0]`.
unsafe fn copyprot_letter_at(i: usize) -> c_char {
    *addr_of!(copyprot_letter).cast::<c_char>().add(i)
}

/// Index into `letts_used` for a manual entry: `copyprot_letter[entry] - 'A'`.
unsafe fn copyprot_letter_index(entry: word) -> usize {
    (copyprot_letter_at(entry as usize) as i32 - b'A' as i32) as usize
}

/// `sound_interruptible` is an extern incomplete array; bindgen emits `[byte; 0]`.
unsafe fn sound_interruptible_at(idx: usize) -> byte {
    *addr_of!(sound_interruptible).cast::<byte>().add(idx)
}
unsafe fn sound_interruptible_set(idx: usize, val: byte) {
    *addr_of_mut!(sound_interruptible).cast::<byte>().add(idx) = val;
}

/// `chtab_type::images` is a flexible array member; bindgen emits `[*mut image_type; 0]`.
unsafe fn chtab_image(chtab: *mut chtab_type, idx: usize) -> *mut image_type {
    addr_of!((*chtab).images).cast::<*mut image_type>().add(idx).read()
}
unsafe fn chtab_image_set(chtab: *mut chtab_type, idx: usize, img: *mut image_type) {
    *addr_of_mut!((*chtab).images).cast::<*mut image_type>().add(idx) = img;
}

// ---------------------------------------------------------------------------
// seg000:0000
// ---------------------------------------------------------------------------
/// Program entry point: parse the command line, bring up every subsystem, hand off to the game.
///
/// The ordering here is load-bearing. Video and the copy-protection dialog come up *before*
/// [`load_mod_options`], because a broken mod must be reportable through an on-screen error
/// dialog. `PRINCE.DAT` is opened after that for the same reason.
///
/// Never returns in the normal case: [`init_game_main`] ends in [`start_game`], which is the
/// target of the `longjmp` restart loop.
#[no_mangle]
pub unsafe extern "C" fn pop_main() {
    if !cp(b"--version\0").is_null() || !cp(b"-v\0").is_null() {
        print!("SDLPoP v{}\n", cstr(SDLPOP_VERSION.as_ptr() as *const c_char));
        std::process::exit(0);
    }

    if !cp(b"--help\0").is_null() || !cp(b"-h\0").is_null() || !cp(b"-?\0").is_null() {
        print!("See README.md\n");
        std::process::exit(0);
    }

    let temp = cp(b"seed=\0");
    if !temp.is_null() {
        random_seed = atoi(temp.add(5)) as dword;
        seed_was_init = 1;
    }

    // FIX_SOUND_PRIORITIES
    fix_sound_priorities();

    load_global_options();
    check_mod_param();
    // USE_MENU
    load_ingame_settings();
    if !cp(b"mute\0").is_null() {
        is_sound_on = 0;
    }
    turn_sound_on_off(((is_sound_on != 0) as byte) * 15);

    // USE_REPLAY
    if g_argc > 1 {
        let filename = *g_argv.add(1);
        let e = strrchr(filename, b'.' as c_int);
        if !e.is_null() && strcasecmp(e, b".P1R\0".as_ptr() as *const c_char) == 0 {
            start_with_replay_file(filename);
        }
    }
    let temp = cp(b"validate\0");
    if !temp.is_null() {
        is_validate_mode = 1;
        start_with_replay_file(temp);
    }

    // Headless testing support -- not in the original C. Forces SDL's dummy video/audio
    // drivers so the game runs its *real* startup path (window creation, event pump, audio
    // init) with no real display or sound device needed, unlike `validate` mode, which
    // skips window creation and most of the input path entirely (see the SdlPlatform
    // event-pump startup panic this gap let through, fixed in commit 4a11238). Must be set
    // before parse_grmode()/set_gr_mode() below, which is where SDL_Init actually runs.
    if !cp(b"headless\0").is_null() {
        std::env::set_var("SDL_VIDEODRIVER", "dummy");
        std::env::set_var("SDL_AUDIODRIVER", "dummy");
    }

    parse_grmode();
    current_target_surface = rect_sthg(onscreen_surface_, addr_of!(screen_rect));
    set_hc_pal();
    init_copyprot_dialog();

    load_mod_options();

    // CusPop option
    is_blind_mode = (*custom).start_in_blind_mode as word;
    need_drects = 1;

    apply_seqtbl_patches();

    let mut sprintf_temp = [0i8; 100];

    init_timer(BASE_FPS as c_int);
    parse_cmdline_sound();

    show_loading();
    set_joy_mode();
    cheats_enabled = (!cp(b"megahit\0").is_null()) as word;
    // USE_DEBUG_CHEATS
    debug_cheats_enabled = (!cp(b"debug\0").is_null()) as byte;
    if debug_cheats_enabled != 0 {
        cheats_enabled = 1;
    }
    draw_mode = (!cp(b"draw\0").is_null() && cheats_enabled != 0) as word;
    demo_mode = (!cp(b"demo\0").is_null()) as word;

    // USE_REPLAY
    init_record_replay();

    dathandle = open_dat(b"PRINCE.DAT\0".as_ptr() as *const c_char, b'G' as c_int);

    // A bare level number on the command line picks the starting level. Scanned high-to-low so
    // that "15" wins over the "1" and "5" it contains.
    if cheats_enabled != 0 || recording != 0 {
        for level_number in (0..=15).rev() {
            cbuf_set(&mut sprintf_temp, &format!("{}", level_number));
            if !check_param(sprintf_temp.as_ptr()).is_null() {
                start_level = level_number as c_short;
                break;
            }
        }
    }

    play_demo_level = (!cp(b"playdemo\0").is_null()) as c_int;

    // USE_SCREENSHOT
    init_screenshot();

    // USE_MENU
    init_menu();

    init_game_main();
}

// seg000:024F
/// Load the assets that outlive every level, then start the game.
///
/// These are the sprites and palettes that are never freed between levels: the sword and the
/// flame/potion sheets out of `PRINCE.DAT`, the guard colour palettes, and the per-level colour
/// variation palettes (`level_var_palettes`, a PoP 1.3 addition that [`load_lev_spr`] indexes).
/// Everything level-specific is loaded and freed by `load_lev_spr` instead.
#[no_mangle]
pub unsafe extern "C" fn init_game_main() {
    doorlink1_ad = addr_of_mut!(level.doorlinks1) as *mut byte;
    doorlink2_ad = addr_of_mut!(level.doorlinks2) as *mut byte;
    prandom(1);
    if graphics_mode == grmodes_gmMcgaVga as byte {
        // Guard palettes
        guard_palettes =
            load_from_opendats_alloc(10, b"bin\0".as_ptr() as *const c_char, null_mut(), null_mut())
                as *mut byte;
        set_pal(12, 0x38, 0x00, 0x0C);
        set_pal(6, 0x30, 0x26, 0x14);
        level_var_palettes =
            load_from_opendats_alloc(20, b"bin\0".as_ptr() as *const c_char, null_mut(), null_mut())
                as *mut byte;
    }
    chtab_addrs[chtabs_id_chtab_0_sword as usize] = load_sprites_from_file(700, 1 << 2, 1);
    chtab_addrs[chtabs_id_chtab_1_flameswordpotion as usize] = load_sprites_from_file(150, 1 << 3, 1);
    close_dat(dathandle);
    // USE_LIGHTING
    init_lighting();
    load_all_sounds();

    hof_read();
    show_splash();
    start_game();
}

// seg000:0358
/// Restart the game from scratch: re-roll the copy-protection question and enter the title or a
/// level.
///
/// # The `setjmp`/`longjmp` restart loop
///
/// `start_game` is called from all over the codebase to restart — `process_key`, `play_frame`,
/// `draw_game_frame`, `play_level`, `control_kid`, `end_sequence`, `expired`. Every one of those
/// call sites is buried deep inside the level loop's call stack, and none of them unwinds first.
/// Calling `start_game` recursively from each would grow the stack without bound, which is what
/// the original comment "Prevent filling of stack" refers to.
///
/// The fix, inherited verbatim from the DOS original, is a `setjmp` landing pad. The *first*
/// call marks this spot and falls through. Every later call tears down the screen and sounds and
/// `longjmp`s back to that mark, discarding the whole intervening stack, and re-enters
/// `start_game` from the top with `first_start` already zero — so the second call from the
/// restored frame takes the `if` branch and proceeds normally.
///
/// **Do not restructure the native path.** There is no safe Rust equivalent to a real
/// non-local jump: it lands in the middle of a call stack that is never unwound, so no
/// combination of loops, closures or `?` reproduces it without changing which frames get
/// discarded. The native implementation below is untouched from the original port.
///
/// wasm32 has no `setjmp`/`longjmp` at all (see `wasm_libc.rs`), so it uses a different but
/// behaviorally equivalent mechanism: Rust's own unwinding. The *outer* call (`first_start !=
/// 0`) wraps [`start_game_body`] in a `catch_unwind` retry loop instead of calling `setjmp`;
/// every *inner* call (one of the ~12 restart call sites, arbitrarily deep in the stack) tears
/// down the screen/sounds exactly like native does, then panics with a [`RestartGameSignal`]
/// marker instead of calling `longjmp` -- caught by the outer loop, which reruns the body,
/// exactly mirroring "resume execution right after the landing pad." No other restart call
/// site needs to change: they already just call `start_game()` again, which is what drives
/// both branches on either target.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub unsafe extern "C" fn start_game() {
    // Prevent filling of stack.
    if first_start != 0 {
        first_start = 0;
        setjmp(setjmp_buf.as_mut_ptr());
    } else {
        draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
        show_quotes();
        clear_screen_and_sounds();
        longjmp(setjmp_buf.as_mut_ptr(), -1);
    }
    start_game_body();
}

/// Marker type panicked with by wasm32's `start_game` when a nested call wants to restart --
/// caught by the outer call's `catch_unwind`, never allowed to escape it. Carries no data;
/// its identity (via `downcast_ref`) is the whole signal.
#[cfg(target_arch = "wasm32")]
pub(crate) struct RestartGameSignal;

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn start_game() {
    if first_start != 0 {
        first_start = 0;
        loop {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| start_game_body())) {
                Ok(()) => return,
                Err(payload) => {
                    if payload.downcast_ref::<RestartGameSignal>().is_some() {
                        continue;
                    }
                    // A real panic/bug, not a restart request -- propagate it.
                    std::panic::resume_unwind(payload);
                }
            }
        }
    } else {
        draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
        show_quotes();
        clear_screen_and_sounds();
        std::panic::panic_any(RestartGameSignal);
    }
}

/// The actual "start (or restart) the game" logic -- identical on every target. Landed on
/// (native: via the `setjmp` fallthrough; wasm32: via the `catch_unwind` loop) once per
/// restart, so `entry_used`/`letts_used` are declared fresh here rather than by the caller.
unsafe fn start_game_body() {
    // USE_COPYPROT
    let mut entry_used = [0u16; 40];
    let mut letts_used = [0u8; 26];

    release_title_images();
    free_optsnd_chtab();

    // USE_COPYPROT
    entry_used.fill(0);
    letts_used.fill(0);

    // Deal 14 manual entries into the potion slots, rejecting any entry already dealt and any
    // whose first letter is already in play -- the player answers by drinking the potion whose
    // letter matches, so two entries sharing a letter would make the question ambiguous.
    copyprot_plac = prandom(13);
    for pos in 0u16..14 {
        let which_entry = loop {
            let candidate = prandom(39);
            // The slot at copyprot_plac is the one that will actually be asked. C assigns
            // copyprot_idx on every draw including rejected ones; only the accepted draw survives.
            if pos == copyprot_plac {
                copyprot_idx = candidate;
            }
            if entry_used[candidate as usize] == 0 && letts_used[copyprot_letter_index(candidate)] == 0
            {
                break candidate;
            }
        };
        cplevel_entr[pos as usize] = which_entry;
        entry_used[which_entry as usize] = 1;
        letts_used[copyprot_letter_index(which_entry)] = 1;
    }

    // CusPop option: skip the title sequence (the level loads instantly).
    if (*custom).skip_title != 0 {
        let level_number = if start_level >= 0 {
            start_level as c_int
        } else {
            (*custom).first_level as c_int
        };
        init_game(level_number);
        return;
    }

    if start_level < 0 {
        show_title();
    } else {
        init_game(start_level as c_int);
    }
}

// ---------------------------------------------------------------------------
// USE_QUICKSAVE
// ---------------------------------------------------------------------------
/// One field of a quicksave: either written or read, depending which callback is passed to
/// [`quick_process`]. Returns non-zero on success.
type ProcessFn = unsafe extern "C" fn(*mut c_void, usize) -> c_int;

/// [`ProcessFn`] that writes a field to the open quicksave file.
unsafe extern "C" fn process_save(data: *mut c_void, data_size: usize) -> c_int {
    (fwrite(data, data_size, 1, quick_fp) == 1) as c_int
}

/// [`ProcessFn`] that reads a field back from the open quicksave file.
unsafe extern "C" fn process_load(data: *mut c_void, data_size: usize) -> c_int {
    (fread(data, data_size, 1, quick_fp) == 1) as c_int
}

/// Walk every global that belongs in a quicksave, in a fixed order, through `process_func`.
///
/// Save and load share this one function so the two can never disagree about the field order or
/// sizes; the file format *is* this list. Returns non-zero if every field succeeded — a failure
/// short-circuits the rest, exactly like C's `ok = ok && ...`.
///
/// Changing the order or the set of fields is a save-format break, and is why `quick_version`
/// exists: [`quick_load`] refuses a file whose version header does not match.
#[no_mangle]
pub unsafe extern "C" fn quick_process(process_func: ProcessFn) -> c_int {
    let mut ok: c_int = 1;
    macro_rules! process {
        ($x:expr) => {
            if ok != 0 {
                ok = process_func(
                    addr_of_mut!($x) as *mut c_void,
                    core::mem::size_of_val(&$x),
                );
            }
        };
    }
    // level
    // USE_DEBUG_CHEATS: don't load the level if Shift is held while pressing F9. Skipping the
    // bytes rather than not writing them keeps the file offset in step for every later field.
    if debug_cheats_enabled != 0
        && (key_states[SDL_SCANCODE_LSHIFT as usize] as c_int & KEYSTATE_HELD_I != 0
            || key_states[SDL_SCANCODE_RSHIFT as usize] as c_int & KEYSTATE_HELD_I != 0)
    {
        fseek(quick_fp, core::mem::size_of::<level_type>() as c_long, SEEK_CUR);
    } else {
        process!(level);
    }
    process!(checkpoint);
    process!(upside_down);
    process!(drawn_room);
    process!(current_level);
    process!(next_level);
    process!(mobs_count);
    process!(mobs);
    process!(trobs_count);
    process!(trobs);
    process!(leveldoor_open);
    // kid
    process!(Kid);
    process!(hitp_curr);
    process!(hitp_max);
    process!(hitp_beg_lev);
    process!(grab_timer);
    process!(holding_sword);
    process!(united_with_shadow);
    process!(have_sword);
    process!(kid_sword_strike);
    process!(pickup_obj_type);
    process!(offguard);
    // guard
    process!(Guard);
    process!(Char);
    process!(Opp);
    process!(guardhp_curr);
    process!(guardhp_max);
    process!(demo_index);
    process!(demo_time);
    process!(curr_guard_color);
    process!(guard_notice_timer);
    process!(guard_skill);
    process!(shadow_initialized);
    process!(guard_refrac);
    process!(justblocked);
    process!(droppedout);
    // collision
    process!(curr_row_coll_room);
    process!(curr_row_coll_flags);
    process!(below_row_coll_room);
    process!(below_row_coll_flags);
    process!(above_row_coll_room);
    process!(above_row_coll_flags);
    process!(prev_collision_row);
    // flash
    process!(flash_color);
    process!(flash_time);
    // sounds
    process!(need_level1_music);
    process!(is_screaming);
    process!(is_feather_fall);
    process!(last_loose_sound);
    // random
    process!(random_seed);
    // remaining time
    process!(rem_min);
    process!(rem_tick);
    // saved controls
    process!(control_x);
    process!(control_y);
    process!(control_shift);
    process!(control_forward);
    process!(control_backward);
    process!(control_up);
    process!(control_down);
    process!(control_shift2);
    process!(ctrl1_forward);
    process!(ctrl1_backward);
    process!(ctrl1_up);
    process!(ctrl1_down);
    process!(ctrl1_shift2);
    // USE_REPLAY
    process!(curr_tick);
    // USE_COLORED_TORCHES
    process!(torch_colors);
    // USE_SUPER_HIGH_JUMP
    process!(super_jump_fall);
    process!(super_jump_timer);
    process!(super_jump_room);
    process!(super_jump_col);
    process!(super_jump_row);
    process!(is_guard_notice);
    process!(can_guard_see_kid);
    ok
}

const QUICK_FILE: &[u8] = b"QUICKSAVE.SAV\0";

/// Resolve where `QUICKSAVE.SAV` lives for the current levelset. See [`get_writable_file_path`].
unsafe fn get_quick_path(custom_path_buffer: *mut c_char, max_len: usize) -> *const c_char {
    get_writable_file_path(custom_path_buffer, max_len, QUICK_FILE.as_ptr() as *const c_char)
}

/// Write a quicksave: the version header, then every field in [`quick_process`] order.
///
/// Returns non-zero on success.
#[no_mangle]
pub unsafe extern "C" fn quick_save() -> c_int {
    let mut ok: c_int = 0;
    let mut custom_quick_path = [0i8; POP_MAX_PATH as usize];
    let path = get_quick_path(custom_quick_path.as_mut_ptr(), custom_quick_path.len());
    quick_fp = fopen(path, b"wb\0".as_ptr() as *const c_char);
    if !quick_fp.is_null() {
        process_save(quick_version.as_ptr() as *mut c_void, quick_version.len());
        ok = quick_process(process_save);
        fclose(quick_fp);
        quick_fp = null_mut();
    } else {
        perror(b"quick_save: fopen\0".as_ptr() as *const c_char);
        print!("Tried to open for writing: {}\n", cstr(path));
    }
    ok
}

/// Rebuild everything a quicksave does not store: level sprites, room links, and the screen.
///
/// A quicksave holds simulation state only, not the loaded graphics. Reloading them means calling
/// [`load_lev_spr`], which clobbers `curr_guard_color` and `next_level` as a side effect — hence
/// the save/restore around it.
#[no_mangle]
pub unsafe extern "C" fn restore_room_after_quick_load() {
    let saved_guard_color = curr_guard_color as c_int;
    let saved_next_level = next_level as c_int;
    reset_level_unused_fields(false);
    load_lev_spr(current_level as c_int);
    curr_guard_color = saved_guard_color as word;
    next_level = saved_next_level as word;

    // Feather fall can only be restored if the fix that turns it into a timer is enabled;
    // otherwise the saved flag has no duration attached and would last forever.
    if (*fixes).fix_quicksave_during_feather == 0 && is_feather_fall > 0 {
        is_feather_fall = 0;
        stop_sounds();
    }

    // Show the room the prince is in, even if the player had panned the view away (H/J/U/N).
    different_room = 1;
    next_room = Kid.room as word;
    drawn_room = Kid.room as word;
    load_room_links();
    draw_game_frame(); // for falling

    // Force both HP bars to redraw.
    hitp_delta = 1;
    guardhp_delta = 1;
    // Don't draw guard HP if a previously viewed room had a guard but this one doesn't.
    // Same clearing as clear_char().
    if Guard.room as word != drawn_room {
        Guard.direction = directions_dir_56_none as sbyte;
        guardhp_curr = 0;
    }

    draw_hp();
    loadkid_and_opp();
    // Get rid of the "press button" message if the Kid was dead before the quickload.
    text_time_total = 0;
    text_time_remaining = 0;
    exit_room_timer = 0;
}

/// Read a quicksave back and restore the game to it. Returns non-zero on success.
///
/// A file whose version header does not match `quick_version` is rejected before anything is
/// touched. Otherwise this is destructive: it blanks the screen, overwrites every field in
/// [`quick_process`], and rebuilds the room via [`restore_room_after_quick_load`].
#[no_mangle]
pub unsafe extern "C" fn quick_load() -> c_int {
    let mut ok: c_int = 0;
    let mut custom_quick_path = [0i8; POP_MAX_PATH as usize];
    let path = get_quick_path(custom_quick_path.as_mut_ptr(), custom_quick_path.len());
    quick_fp = fopen(path, b"rb\0".as_ptr() as *const c_char);
    if !quick_fp.is_null() {
        // Check the quicksave version is compatible.
        process_load(quick_control.as_mut_ptr() as *mut c_void, quick_control.len());
        if strcmp(quick_control.as_ptr(), quick_version.as_ptr()) != 0 {
            fclose(quick_fp);
            quick_fp = null_mut();
            return 0;
        }

        stop_sounds();
        draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
        update_screen();
        delay_ticks(5); // briefly display a black screen as a visual cue

        let old_rem_min = rem_min;
        let old_rem_tick = rem_tick;

        ok = quick_process(process_load);
        fclose(quick_fp);
        quick_fp = null_mut();

        restore_room_after_quick_load();
        update_screen();

        // USE_QUICKLOAD_PENALTY: subtract one minute from the remaining time, unless time has
        // already stopped for the victory sequence.
        if enable_quicksave_penalty != 0
            && (current_level < (*custom).victory_stops_time_level
                || (current_level == (*custom).victory_stops_time_level && leveldoor_open < 2))
        {
            let ticks_elapsed = 720 * (rem_min as c_int - old_rem_min as c_int)
                + (rem_tick as c_int - old_rem_tick as c_int);
            if ticks_elapsed > 0 && ticks_elapsed < 720 {
                // Under a minute has passed since the save: don't restore the clock at all,
                // which already costs the player that time.
                rem_min = old_rem_min;
                rem_tick = old_rem_tick;
            } else {
                // Crop to exactly "5 minutes" if this quickload crosses the threshold.
                if rem_min == 6 {
                    rem_tick = 719;
                }
                // Be lenient below 5 minutes; below 0 the clock runs 'forward', so charge there too.
                if rem_min > 5 || rem_min < 0 {
                    rem_min -= 1;
                }
            }
        }
    } else {
        perror(b"quick_load: fopen\0".as_ptr() as *const c_char);
        print!("Tried to open for reading: {}\n", cstr(path));
    }
    ok
}

/// Service a pending F6/F9 request, once per tick, and report the result on the bottom line.
///
/// The keypress only sets `need_quick_save`/`need_quick_load`; the actual file I/O happens here,
/// at a point in the tick where the game state is coherent.
#[no_mangle]
pub unsafe extern "C" fn check_quick_op() {
    if enable_quicksave == 0 {
        return;
    }
    if need_quick_save != 0 {
        if (is_feather_fall == 0 || (*fixes).fix_quicksave_during_feather != 0) && quick_save() != 0 {
            display_text_bottom(b"QUICKSAVE\0".as_ptr() as *const c_char);
        } else {
            display_text_bottom(b"NO QUICKSAVE\0".as_ptr() as *const c_char);
        }
        need_quick_save = 0;
        text_time_total = 24;
        text_time_remaining = 24;
    }
    if need_quick_load != 0 {
        if quick_load() != 0 {
            display_text_bottom(b"QUICKLOAD\0".as_ptr() as *const c_char);
        } else {
            display_text_bottom(b"NO QUICKLOAD\0".as_ptr() as *const c_char);
        }
        need_quick_load = 0;
        text_time_total = 24;
        text_time_remaining = 24;
    }
}

/// One-shot timer callback that re-arms Shift a moment after Shift+L cleared it.
///
/// Shift+L skips to the next level. `process_key` zeroes both Shift key states so the cutscene
/// that follows does not immediately see Shift held (which would skip it too). This callback
/// fires 250 ms later and restores the bits if the player really is still holding Shift — so
/// holding it deliberately *does* skip the cutscene.
unsafe fn temp_shift_release_callback() {
    let input = crate::platform::sdl::shared_input();
    if input.key_state(SDL_SCANCODE_LSHIFT) {
        key_states[SDL_SCANCODE_LSHIFT as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as byte;
    }
    if input.key_state(SDL_SCANCODE_RSHIFT) {
        key_states[SDL_SCANCODE_RSHIFT as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as byte;
    }
}

// seg000:04CD
/// Take at most one keypress from the queue and act on it. Returns non-zero if a key was handled.
///
/// Three dispatch stages, in order:
///
/// 1. **Title screen** (`start_level < 0`). Any key starts the game; a few keys pick *how*
///    (Tab replays, Ctrl+Tab records, Ctrl+L loads a saved game). This branch normally does not
///    return — it ends in [`start_game`], which `longjmp`s away.
/// 2. **Always-available keys**: pause, restart level, save, sound/input mode, quicksave.
/// 3. **Cheat keys**, only when `cheats_enabled` (the `megahit`/`debug` command-line options).
///
/// The `answer_text` local threads a bottom-line message out of whichever arm produced one; it is
/// displayed once at the end so no arm has to repeat the display-and-set-timer sequence.
#[no_mangle]
pub unsafe extern "C" fn process_key() -> c_int {
    let mut sprintf_temp = [0i8; 80];
    let mut answer_text: Option<*const c_char> = None;
    let mut key = key_test_quit();

    // USE_MENU
    if is_paused != 0 && is_menu_shown != 0 {
        key = key_test_paused_menu(key);
        if key == 0 {
            return 0;
        }
    }

    // remap
    if key == key_enter {
        key = SDL_SCANCODE_RETURN;
    } else if key == key_esc {
        key = SDL_SCANCODE_ESCAPE;
    }

    if start_level < 0 {
        if key != 0 || control_shift != 0 {
            // USE_QUICKSAVE
            if key == SDL_SCANCODE_F9 {
                need_quick_load = 1;
            }
            // USE_REPLAY
            if key == SDL_SCANCODE_TAB || need_start_replay != 0 {
                start_replay();
            } else if key == KEY_CTRL_TAB {
                start_level = (*custom).first_level as c_short;
                start_recording();
            } else if key == KEY_CTRL_L {
                if load_game() == 0 {
                    return 0;
                }
            } else {
                start_level = (*custom).first_level as c_short;
            }
            draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
            // USE_FADE
            if is_global_fading != 0 {
                if let Some(f) = (*fade_palette_buffer).proc_restore_free {
                    f(fade_palette_buffer);
                }
                is_global_fading = 0;
            }
            start_game();
        }
    }
    // If the Kid died, Enter or Shift will restart the level.
    if rem_min != 0 && Kid.alive > 6 && (control_shift != 0 || key == SDL_SCANCODE_RETURN) {
        key = SDL_SCANCODE_A | WITH_CTRL;
    }
    // USE_REPLAY
    if recording != 0 {
        key_press_while_recording(&mut key);
    } else if replaying != 0 {
        key_press_while_replaying(&mut key);
    }
    if key == 0 {
        return 0;
    }
    if is_keyboard_mode != 0 {
        clear_kbd_buf();
    }

    // ----- always-available keys -----
    match key {
        // Esc also pauses while grabbing, hence the Shift variant.
        SDL_SCANCODE_ESCAPE | KEY_SHIFT_ESCAPE => {
            is_paused = 1;
            // USE_MENU
            if enable_pause_menu != 0 && is_cutscene == 0 && !is_ending_sequence {
                is_menu_shown = 1;
            }
        }
        // USE_MENU: Backspace opens the menu even when the pause menu is disabled.
        SDL_SCANCODE_BACKSPACE => {
            if is_cutscene == 0 && !is_ending_sequence {
                is_paused = 1;
                is_menu_shown = 1;
            }
        }
        SDL_SCANCODE_SPACE => is_show_time = 1,
        KEY_CTRL_A => {
            if current_level != 15 {
                stop_sounds();
                is_restart_level = 1;
            }
        }
        // CusPop: first and last level on which saving is allowed.
        KEY_CTRL_G => {
            if current_level >= (*custom).saving_allowed_first_level
                && current_level <= (*custom).saving_allowed_last_level
            {
                save_game();
            }
        }
        KEY_CTRL_J => {
            answer_text = Some(
                if (sound_flags & soundflags_sfDigi as byte) != 0
                    && sound_mode == sound_modes_smTandy as byte
                {
                    b"JOYSTICK UNAVAILABLE\0".as_ptr() as *const c_char
                } else if set_joy_mode() != 0 {
                    b"JOYSTICK MODE\0".as_ptr() as *const c_char
                } else {
                    b"JOYSTICK NOT FOUND\0".as_ptr() as *const c_char
                },
            );
        }
        KEY_CTRL_K => {
            answer_text = Some(b"KEYBOARD MODE\0".as_ptr() as *const c_char);
            is_joyst_mode = 0;
            is_keyboard_mode = 1;
        }
        KEY_CTRL_R => {
            start_level = -1;
            // USE_MENU
            if is_menu_shown != 0 {
                menu_was_closed(); // Do necessary cleanup.
            }
            start_game();
        }
        KEY_CTRL_S => {
            turn_sound_on_off(((is_sound_on == 0) as byte) * 15);
            answer_text = Some(if is_sound_on != 0 {
                b"SOUND ON\0".as_ptr() as *const c_char
            } else {
                b"SOUND OFF\0".as_ptr() as *const c_char
            });
        }
        KEY_CTRL_V => {
            cbuf_set(
                &mut sprintf_temp,
                &format!("SDLPoP v{}\n", cstr(SDLPOP_VERSION.as_ptr() as *const c_char)),
            );
            answer_text = Some(sprintf_temp.as_ptr());
        }
        KEY_CTRL_C => {
            let verc = SDL_version { major: 2, minor: 30, patch: 0 };
            let (vmajor, vminor, vpatch) =
                crate::platform::sdl::shared_renderer().linked_sdl_version();
            let verl = SDL_version { major: vmajor, minor: vminor, patch: vpatch };
            cbuf_set(
                &mut sprintf_temp,
                &format!(
                    "SDL COMP v{}.{}.{} LINK v{}.{}.{}",
                    verc.major, verc.minor, verc.patch, verl.major, verl.minor, verl.patch
                ),
            );
            answer_text = Some(sprintf_temp.as_ptr());
        }
        // Shift+L: skip to the next level.
        KEY_SHIFT_L => {
            if current_level < (*custom).shift_L_allowed_until_level || cheats_enabled != 0 {
                // Clear Shift so the cutscene doesn't see it. If Shift is still held when the
                // timer fires, temp_shift_release_callback puts it back and the cutscene is
                // skipped after all.
                let delay: u32 = 250;
                key_states[SDL_SCANCODE_LSHIFT as usize] = 0;
                key_states[SDL_SCANCODE_RSHIFT as usize] = 0;
                let ok = crate::platform::sdl::shared_input()
                    .add_one_shot_timer(delay, Box::new(|| unsafe { temp_shift_release_callback() }));
                if !ok {
                    sdlperror(b"process_key: SDL_AddTimer\0".as_ptr() as *const c_char);
                    quit(1);
                }
                if current_level == 14 {
                    next_level = 1;
                } else if current_level == 15 && cheats_enabled != 0 {
                    // USE_COPYPROT: leaving the potions level returns to wherever it was entered
                    // from, and disarms the check so it isn't asked again.
                    if enable_copyprot != 0 {
                        next_level = (*custom).copyprot_level;
                        (*custom).copyprot_level = -1i32 as word;
                    }
                } else {
                    next_level = current_level.wrapping_add(1);
                    // Without cheats, skipping costs you the clock. Both operands are promoted to
                    // int in C: rem_min is short and shift_L_reduced_minutes is word, so the
                    // comparison must not be narrowed back to short.
                    if cheats_enabled == 0
                        && (rem_min as c_int) > (*custom).shift_L_reduced_minutes as c_int
                    {
                        rem_min = (*custom).shift_L_reduced_minutes as c_short;
                        rem_tick = (*custom).shift_L_reduced_ticks;
                    }
                }
                stop_sounds();
            }
        }
        // USE_QUICKSAVE. Only quicksave while the Kid is alive (alive < 0 means "not dying").
        SDL_SCANCODE_F6 | KEY_SHIFT_F6 => {
            if Kid.alive < 0 {
                need_quick_save = 1;
            }
        }
        SDL_SCANCODE_F9 | KEY_SHIFT_F9 => need_quick_load = 1,
        // USE_REPLAY
        KEY_CTRL_TAB | KEY_CTRL_SHIFT_TAB => {
            if recording != 0 {
                stop_recording();
            } else {
                start_recording();
            }
        }
        _ => {}
    }

    // ----- cheat keys -----
    if cheats_enabled != 0 {
        match key {
            // Report the room links of the drawn room, and of the rooms diagonally adjacent.
            SDL_SCANCODE_C => {
                cbuf_set(
                    &mut sprintf_temp,
                    &format!("S{} L{} R{} A{} B{}", drawn_room, room_L, room_R, room_A, room_B),
                );
                answer_text = Some(sprintf_temp.as_ptr());
            }
            KEY_SHIFT_C => {
                cbuf_set(
                    &mut sprintf_temp,
                    &format!("AL{} AR{} BL{} BR{}", room_AL, room_AR, room_BL, room_BR),
                );
                answer_text = Some(sprintf_temp.as_ptr());
            }
            // '-' subtracts a minute. ALLOW_INFINITE_TIME: a negative rem_min means the clock
            // runs 'forward', so there the same key moves it the other way.
            SDL_SCANCODE_KP_MINUS => {
                if rem_min > 1 {
                    rem_min -= 1;
                } else if rem_min < -1 {
                    rem_min += 1;
                } else if rem_min == -1 {
                    rem_tick = 720; // resets the timer to 00:00:00
                }
                text_time_total = 0;
                text_time_remaining = 0;
                is_show_time = 1;
            }
            // '+' adds a minute, with the same sign convention.
            SDL_SCANCODE_KP_PLUS => {
                if rem_min < 0 {
                    if rem_min > i16::MIN {
                        rem_min -= 1;
                    }
                } else {
                    rem_min += 1;
                }
                text_time_total = 0;
                text_time_remaining = 0;
                is_show_time = 1;
            }
            // Revive the Kid.
            SDL_SCANCODE_R => {
                if Kid.alive > 0 {
                    resurrect_time = 20;
                    Kid.alive = -1;
                    erase_bottom_text(1);
                }
            }
            // Kill the guard. Skeletons are immune -- they cannot be killed by damage.
            SDL_SCANCODE_K => {
                if Guard.charid != charids_charid_4_skeleton as byte {
                    // guardhp_curr is word; C negates it after the usual int promotion.
                    guardhp_delta = -(guardhp_curr as c_int) as c_short;
                    Guard.alive = 0;
                }
            }
            KEY_SHIFT_I => toggle_upside(),
            KEY_SHIFT_W => feather_fall(),
            // H/J/U/N pan the view to an adjacent room; Ctrl+B pans back to the Kid. The guard HP
            // bar is blanked first because the guard belongs to the room being left.
            SDL_SCANCODE_H => {
                draw_guard_hp(0, 10);
                next_room = room_L;
            }
            SDL_SCANCODE_J => {
                draw_guard_hp(0, 10);
                next_room = room_R;
            }
            SDL_SCANCODE_U => {
                draw_guard_hp(0, 10);
                next_room = room_A;
            }
            SDL_SCANCODE_N => {
                draw_guard_hp(0, 10);
                next_room = room_B;
            }
            KEY_CTRL_B => {
                draw_guard_hp(0, 10);
                next_room = Kid.room as word;
            }
            KEY_SHIFT_B => {
                is_blind_mode = (is_blind_mode == 0) as word;
                if is_blind_mode != 0 {
                    draw_rect(addr_of!(rect_top), colorids_color_0_black as c_int);
                } else {
                    need_full_redraw = 1;
                }
            }
            // Small potion: one hit point back.
            KEY_SHIFT_S => {
                if hitp_curr != hitp_max {
                    play_sound(soundids_sound_33_small_potion as c_int);
                    hitp_delta = 1;
                    flash_color = 4; // red
                    flash_time = 2;
                }
            }
            // Big potion: one more maximum hit point.
            KEY_SHIFT_T => {
                play_sound(soundids_sound_30_big_potion as c_int);
                flash_color = 4; // red
                flash_time = 4;
                add_life();
            }
            // USE_DEBUG_CHEATS
            SDL_SCANCODE_T => is_timer_displayed = 1 - is_timer_displayed,
            SDL_SCANCODE_F => {
                // The feather timer only exists when feather fall is timer-based.
                is_feather_timer_displayed = if (*fixes).fix_quicksave_during_feather != 0 {
                    1 - is_feather_timer_displayed
                } else {
                    0
                };
            }
            _ => {}
        }
    }

    if let Some(text) = answer_text {
        display_text_bottom(text);
        text_time_total = 24;
        text_time_remaining = 24;
    }
    1
}

// seg000:08EB
/// Advance the simulation by one gameplay tick.
///
/// Order matters throughout: mobs and animated tiles move before the characters that stand on
/// them, the Kid moves before the guard so the guard reacts to where the Kid *is*, and damage is
/// applied ([`do_delta_hp`]) only after every source of it has been evaluated.
///
/// Returns early if [`play_kid_frame`] reports that the level is restarting, since the rest of the
/// tick would be operating on state that is about to be thrown away.
///
/// The tail handles the three levels that end by walking off the edge of a room rather than
/// through a level door.
#[no_mangle]
pub unsafe extern "C" fn play_frame() {
    // Keep the feather-fall music going while more than a second of it remains.
    if (*fixes).fix_quicksave_during_feather != 0 && is_feather_fall >= 10 && check_sound_playing() == 0
    {
        play_sound(soundids_sound_39_low_weight as c_int);
    }
    do_mobs();
    process_trobs();
    check_skel();
    check_can_guard_see_kid();
    if play_kid_frame() != 0 {
        return;
    }
    play_guard_frame();
    if resurrect_time == 0 {
        check_sword_hurting();
        check_sword_hurt();
    }
    check_sword_vs_sword();
    do_delta_hp();
    exit_room();
    check_the_end();
    check_guard_fallout();
    if current_level == 0 {
        // Special event: the demo level ends by running out of its last room.
        if Kid.room as word == (*custom).demo_end_room as word {
            draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
            start_level = -1;
            need_quotes = 1;
            start_game();
        }
    } else if current_level == (*custom).falling_exit_level {
        // Special event: level 6 ends by falling off the bottom of the level.
        if roomleave_result == -2 {
            Kid.y = -1i32 as byte;
            stop_sounds();
            next_level = next_level.wrapping_add(1);
        }
    } else if (*custom).tbl_seamless_exit[current_level as usize] >= 0 {
        // Special event: level 12 ends by running into a specific room, with no level-door
        // cutscene -- hence "seamless".
        if Kid.room as c_short == (*custom).tbl_seamless_exit[current_level as usize] as c_short {
            next_level = next_level.wrapping_add(1);
            // Sounds must be stopped: play_level_2() only checks next_level when nothing is playing.
            stop_sounds();
            seamless = 1;
        }
    }
    show_time();
    // Running out of time doesn't count on the Jaffar/princess levels.
    if current_level < 13 && rem_min == 0 {
        expired();
    }
}

// seg000:09B6
/// Render one gameplay tick and age the bottom-line message.
///
/// Four mutually exclusive drawing strategies, cheapest last:
/// a full redraw, a room change (which re-rolls the palace wall pattern for the new room), a
/// redraw forced by the upside-down cheat toggling, or the normal case — redraw only the
/// rectangles that moving objects dirtied, tracked in `drects`.
///
/// Bottom-line messages are identified by their *total* duration, not by their text: 36 ticks
/// means "died on the demo/potions level" and 288 means "press button to continue", both of which
/// restart the game when they expire, while 1188 (the copy-protection question) never expires at
/// all.
#[no_mangle]
pub unsafe extern "C" fn draw_game_frame() {
    if need_full_redraw != 0 {
        redraw_screen(0);
        need_full_redraw = 0;
    } else if different_room != 0 {
        drawn_room = next_room;
        if (*custom).tbl_level_type[current_level as usize] != 0 {
            gen_palace_wall_colors();
        }
        redraw_screen(1);
    } else if need_redraw_because_flipped != 0 {
        need_redraw_because_flipped = 0;
        redraw_screen(0);
    } else {
        core::ptr::write_bytes(addr_of_mut!(table_counts) as *mut u8, 0, core::mem::size_of_val(&table_counts));
        draw_moving();
        draw_tables();
        if is_blind_mode != 0 {
            draw_rect(addr_of!(rect_top), colorids_color_0_black as c_int);
        }
        // Flip, blit the dirty rectangles, flip back. drects_count is a global, so the drain has
        // to write through it rather than being taken by value.
        if upside_down != 0 {
            flip_screen(offscreen_surface);
        }
        while drects_count != 0 {
            drects_count -= 1;
            copy_screen_rect(addr_of!(drects[drects_count as usize]));
        }
        if upside_down != 0 {
            flip_screen(offscreen_surface);
        }
        drects_count = 0;
    }

    play_next_sound();
    // Note: texts are identified by their total time!
    if text_time_remaining == 1 {
        if text_time_total == 36 || text_time_total == 288 {
            start_level = -1;
            need_quotes = 1;
            // USE_REPLAY
            if recording != 0 {
                stop_recording();
            }
            if replaying != 0 {
                end_replay();
            }
            start_game();
        } else {
            erase_bottom_text(1);
        }
    } else if text_time_remaining != 0 && text_time_total != 1188 {
        text_time_remaining -= 1;
        // Over the last 72 ticks, blink "Press Button to Continue" on a 12-tick cycle: visible
        // for the 3 ticks after it is (re)drawn, blank for the other 9.
        if text_time_total == 288 && text_time_remaining < 72 {
            let blink_frame = text_time_remaining % 12;
            if blink_frame > 3 {
                erase_bottom_text(0);
            } else if blink_frame == 3 {
                display_text_bottom(b"Press Button to Continue\0".as_ptr() as *const c_char);
                play_sound_from_buffer(sound_pointers[soundids_sound_38_blink as usize]);
            }
        }
    }
}

// seg000:0B12
/// Register every self-animating tile in the newly drawn room as a trob (see `seg007`).
///
/// Called on entering a room. Potions bubble, torches flicker and floor swords glint; each needs
/// a trob worklist entry so `process_trobs` will keep animating it.
#[no_mangle]
pub unsafe extern "C" fn anim_tile_modif() {
    for tilepos in 0u16..30 {
        // get_curr_tile masks with 0x1F, so the value always fits a tile id.
        match get_curr_tile(tilepos as c_short) as tiles {
            tiles_tiles_10_potion => start_anim_potion(drawn_room as c_short, tilepos as c_short),
            tiles_tiles_19_torch | tiles_tiles_30_torch_with_debris => {
                start_anim_torch(drawn_room as c_short, tilepos as c_short)
            }
            tiles_tiles_22_sword => start_anim_sword(drawn_room as c_short, tilepos as c_short),
            _ => {}
        }
    }

    // Animate torches in the rightmost column of the room to the left as well: that column is
    // visible along the left edge of the current room.
    for row in 0..=2 {
        match get_tile(room_L as c_int, 9, row) as tiles {
            tiles_tiles_19_torch | tiles_tiles_30_torch_with_debris => {
                start_anim_torch(room_L as c_short, (row * 10 + 9) as c_short)
            }
            _ => {}
        }
    }
}

// seg000:0B72
/// Load sound resources `first..=last` from the main sound archives.
///
/// The three `.DAT` archives (PC speaker, digital, MIDI) must all be open while `load_sound` runs,
/// because which one a given id comes from depends on `sound_flags`. Sounds are never freed, so
/// an id that is already loaded is skipped.
#[no_mangle]
pub unsafe extern "C" fn load_sounds(first: c_int, last: c_int) {
    let mut digi1_dat: *mut dat_type = null_mut();
    let mut digi3_dat: *mut dat_type = null_mut();
    let mut midi_dat: *mut dat_type = null_mut();
    let ibm_dat = open_dat(b"IBM_SND1.DAT\0".as_ptr() as *const c_char, 0);
    if (sound_flags & soundflags_sfDigi as byte) != 0 {
        digi1_dat = open_dat(b"DIGISND1.DAT\0".as_ptr() as *const c_char, 0);
        digi3_dat = open_dat(b"DIGISND3.DAT\0".as_ptr() as *const c_char, 0);
    }
    if (sound_flags & soundflags_sfMidi as byte) != 0 {
        midi_dat = open_dat(b"MIDISND1.DAT\0".as_ptr() as *const c_char, 0);
    }

    load_sound_names();

    for current in (first as c_short)..=(last as c_short) {
        // We don't free sounds, so load only once.
        if sound_pointers[current as usize].is_null() {
            sound_pointers[current as usize] = load_sound(current as c_int);
        }
    }
    if !midi_dat.is_null() {
        close_dat(midi_dat);
    }
    if !digi1_dat.is_null() {
        close_dat(digi1_dat);
    }
    if !digi3_dat.is_null() {
        close_dat(digi3_dat);
    }
    close_dat(ibm_dat);
}

// seg000:0C5E
/// Load sound resources `first..=last` from the *optional* sound archives.
///
/// Same shape as [`load_sounds`], but reads the `...SND2.DAT` set: the level-specific effects
/// (skeleton, mirror, chomper, spikes) and the cutscene music, which are loaded on demand rather
/// than at start-up.
#[no_mangle]
pub unsafe extern "C" fn load_opt_sounds(first: c_int, last: c_int) {
    let mut digi_dat: *mut dat_type = null_mut();
    let mut midi_dat: *mut dat_type = null_mut();
    let ibm_dat = open_dat(b"IBM_SND2.DAT\0".as_ptr() as *const c_char, 0);
    if (sound_flags & soundflags_sfDigi as byte) != 0 {
        digi_dat = open_dat(b"DIGISND2.DAT\0".as_ptr() as *const c_char, 0);
    }
    if (sound_flags & soundflags_sfMidi as byte) != 0 {
        midi_dat = open_dat(b"MIDISND2.DAT\0".as_ptr() as *const c_char, 0);
    }
    for current in (first as c_short)..=(last as c_short) {
        // We don't free sounds, so load only once.
        if sound_pointers[current as usize].is_null() {
            sound_pointers[current as usize] = load_sound(current as c_int);
        }
    }
    if !midi_dat.is_null() {
        close_dat(midi_dat);
    }
    if !digi_dat.is_null() {
        close_dat(digi_dat);
    }
    close_dat(ibm_dat);
}

// seg000:0D20
/// Swap in every graphic and sound that depends on which level is being played.
///
/// The environment sheets are named by graphics mode and level type, e.g. `VDUNGEON.DAT`. Guard
/// sprites come from a per-level table, with type 0 (the ordinary guard) additionally overlaid
/// from `GUARD1`/`GUARD2.DAT` so dungeon and palace guards look different. Level colour variation
/// (a PoP 1.3 feature) then re-tints the environment and wall palettes in place.
///
/// Note this clobbers `curr_guard_color` and `next_level` as a side effect — see
/// [`restore_room_after_quick_load`], which has to save them around this call.
#[no_mangle]
pub unsafe extern "C" fn load_lev_spr(level_no: c_int) {
    let mut dh: *mut dat_type = null_mut();
    let mut filename = [0i8; 20];
    current_level = level_no as word;
    next_level = level_no as word;
    draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
    free_optsnd_chtab();
    cbuf_set(
        &mut filename,
        &format!(
            "{}{}.DAT",
            TBL_ENVIR_GR[graphics_mode as usize],
            TBL_ENVIR_KI[(*custom).tbl_level_type[current_level as usize] as usize]
        ),
    );
    load_chtab_from_file(
        chtabs_id_chtab_6_environment as c_int,
        200,
        filename.as_ptr(),
        1 << 5,
    );
    load_more_opt_graf(filename.as_ptr());
    let guardtype = (*custom).tbl_guard_type[current_level as usize];
    if guardtype != -1 {
        if guardtype == 0 {
            let name: *const c_char = if (*custom).tbl_level_type[current_level as usize] != 0 {
                b"GUARD1.DAT\0".as_ptr() as *const c_char
            } else {
                b"GUARD2.DAT\0".as_ptr() as *const c_char
            };
            dh = open_dat(name, b'G' as c_int);
        }
        load_chtab_from_file(
            chtabs_id_chtab_5_guard as c_int,
            750,
            TBL_GUARD_DAT[guardtype as usize].as_ptr() as *const c_char,
            1 << 8,
        );
        if !dh.is_null() {
            close_dat(dh);
        }
    }
    curr_guard_color = 0;
    load_chtab_from_file(
        chtabs_id_chtab_7_environmentwall as c_int,
        360,
        filename.as_ptr(),
        1 << 6,
    );

    // Level colors (1.3)
    if graphics_mode == grmodes_gmMcgaVga as byte && !level_var_palettes.is_null() {
        let level_color = (*custom).tbl_level_color[current_level as usize];
        if level_color != 0 {
            let env_pal = level_var_palettes.add(0x30 * (level_color as usize - 1));
            let wall_pal = env_pal.add(0x30 * (*custom).tbl_level_type[current_level as usize] as usize);
            set_pal_arr(0x50, 0x10, env_pal as *const rgb_type);
            set_pal_arr(0x60, 0x10, wall_pal as *const rgb_type);
            set_chtab_palette(chtab_addrs[chtabs_id_chtab_6_environment as usize], env_pal, 0x10);
            set_chtab_palette(chtab_addrs[chtabs_id_chtab_7_environmentwall as usize], wall_pal, 0x10);
        }
    }

    load_opt_sounds(44, 44); // skel alive
    load_opt_sounds(45, 45); // mirror
    load_opt_sounds(46, 47); // something chopped, chomper
    load_opt_sounds(48, 49); // something spiked, spikes
}

// seg000:0E6C
/// Read the current level's room/tile data out of `LEVELS.DAT` into the `level` global.
#[no_mangle]
pub unsafe extern "C" fn load_level() {
    let dh = open_dat(b"LEVELS.DAT\0".as_ptr() as *const c_char, 0);
    load_from_opendats_to_area(
        current_level as c_int + 2000,
        addr_of_mut!(level) as *mut c_void,
        core::mem::size_of::<level_type>() as c_int,
        b"bin\0".as_ptr() as *const c_char,
    );
    close_dat(dh);

    alter_mods_allrm();
    reset_level_unused_fields(true);
}

/// Zero the parts of the level format that the game does not use, so they can be repurposed later.
///
/// The original level files carry leftover fields and unused bits; normalising them means a
/// future format extension can rely on them being zero. `loading_clean_level` is false when
/// restoring a savestate, where the spare bits of `guards_color` legitimately carry remembered
/// guard HP and must survive.
#[no_mangle]
pub unsafe extern "C" fn reset_level_unused_fields(loading_clean_level: bool) {
    core::ptr::write_bytes(addr_of_mut!(level.roomxs) as *mut u8, 0, core::mem::size_of_val(&level.roomxs));
    core::ptr::write_bytes(addr_of_mut!(level.roomys) as *mut u8, 0, core::mem::size_of_val(&level.roomys));
    core::ptr::write_bytes(addr_of_mut!(level.fill_1) as *mut u8, 0, core::mem::size_of_val(&level.fill_1));
    core::ptr::write_bytes(addr_of_mut!(level.fill_2) as *mut u8, 0, core::mem::size_of_val(&level.fill_2));
    core::ptr::write_bytes(addr_of_mut!(level.fill_3) as *mut u8, 0, core::mem::size_of_val(&level.fill_3));

    // level.used_rooms is 25 on some levels. Limit it to the actual number of rooms.
    if level.used_rooms as u32 > ROOMCOUNT {
        level.used_rooms = ROOMCOUNT as byte;
    }

    let used_rooms = level.used_rooms as usize;
    for skill in &mut level.guards_skill[..used_rooms] {
        *skill &= 0x0F; // 4 bits in use
    }

    // In savestates the other 4 bits hold remembered guard hp -- don't clear those.
    if loading_clean_level {
        for color in &mut level.guards_color[..used_rooms] {
            *color &= 0x0F; // 4 bits in use
        }
    }
}

// seg000:0EA8
/// Run one tick of the Kid's simulation. Returns 1 if the level is restarting, 0 otherwise.
///
/// The `Char.room != 0` guard skips all the physics when the Kid is not in a real room — that is
/// how a Kid who has fallen out of the level, or is mid-cutscene, is left alone.
#[no_mangle]
pub unsafe extern "C" fn play_kid_frame() -> c_int {
    loadkid_and_opp();
    load_fram_det_col();
    check_killed_shadow();
    play_kid();
    if upside_down != 0 && Char.alive >= 0 {
        upside_down = 0;
        need_redraw_because_flipped = 1;
    }
    if is_restart_level != 0 {
        return 1;
    }
    if Char.room != 0 {
        play_seq();
        fall_accel();
        fall_speed();
        load_frame_to_obj();
        load_fram_det_col();
        set_char_collision();
        bump_into_opponent();
        check_collisions();
        check_bumped();
        check_gate_push();
        check_action();
        check_press();
        check_spike_below();
        if resurrect_time == 0 {
            check_spiked();
            check_chomped_kid();
        }
        check_knock();
    }
    savekid();
    0
}

// seg000:0F48
/// Run one tick of the guard's simulation.
///
/// Cheaper than [`play_kid_frame`] by two nested guards: a guard whose direction is `dir_56_none`
/// does not exist at all, and one outside the drawn room, or off the visible horizontal span
/// (x in 44..211), gets its animation advanced but skips collision and physics entirely.
#[no_mangle]
pub unsafe extern "C" fn play_guard_frame() {
    if Guard.direction != directions_dir_56_none as sbyte {
        loadshad_and_opp();
        load_fram_det_col();
        check_killed_shadow();
        play_guard();
        if Char.room as word == drawn_room {
            play_seq();
            if Char.x >= 44 && Char.x < 211 {
                fall_accel();
                fall_speed();
                load_frame_to_obj();
                load_fram_det_col();
                set_char_collision();
                check_guard_bumped();
                check_action();
                check_press();
                check_spike_below();
                check_spiked();
                check_chomped_guard();
            }
        }
        saveshad();
    }
}

// seg000:0FBD
/// Handle a change of drawn room: re-link neighbours, re-arm the room's animations, and detect
/// the room that ends the game.
///
/// Despite the name this runs every tick; "the end" is the one branch that fires when the drawn
/// room is the winning room of the winning level.
#[no_mangle]
pub unsafe extern "C" fn check_the_end() {
    if next_room != 0 && next_room != drawn_room {
        drawn_room = next_room;
        load_room_links();
        if current_level == (*custom).win_level && drawn_room == (*custom).win_room as word {
            // USE_REPLAY
            if recording != 0 {
                stop_recording();
            }
            if replaying != 0 {
                end_replay();
            }
            // Special event: end of game.
            end_sequence();
        }
        different_room = 1;
        loadkid();
        anim_tile_modif();
        start_chompers();
        check_fall_flo();
        check_shadow();
    }
}

// seg000:1009
/// Special event on level 13: entering either of two rooms rains the ceiling down on you.
///
/// A run of tiles in the room *above* is turned loose with a randomised negative delay, so they
/// start falling in a ragged cascade rather than all at once. `curr_tilepos` is a global that
/// `make_loose_fall` reads, so the loop has to write through it.
#[no_mangle]
pub unsafe extern "C" fn check_fall_flo() {
    if current_level == (*custom).loose_tiles_level
        && (drawn_room == (*custom).loose_tiles_room_1 as word
            || drawn_room == (*custom).loose_tiles_room_2 as word)
    {
        curr_room = room_A as c_short;
        get_room_address(curr_room as c_int);
        curr_tilepos = (*custom).loose_tiles_first_tile;
        while curr_tilepos <= (*custom).loose_tiles_last_tile {
            make_loose_fall((-((prandom(0xFF) & 0x0F) as i32)) as byte);
            curr_tilepos += 1;
        }
    }
}

/// Reduce one analogue stick to a `[x, y]` pair of -1/0/+1, with a dead zone and two tweaks.
///
/// Writes only the axes it has an opinion about: a stick that reads neutral horizontally will
/// *not* clear `axis_state[0]` if the Kid is mid running-jump (which makes chaining running jumps
/// easier), and will not clear `axis_state[1]` while standing up from a crouch and pushing down
/// (which makes crouch-hopping possible).
unsafe fn get_joystick_state(raw_x: c_int, raw_y: c_int, axis_state: *mut c_int) {
    // Deliberate overflow, matching C. Some gamepads report raw_x = raw_y = -32768 in the
    // top-left corner; that squares and sums to 2147483648, which does not fit a signed 32-bit
    // int and wraps negative -- making the dead-zone test true and the stick look centred.
    // Comparing both sides as unsigned is the fix, and requires the wrap to happen first.
    let dist_squared = raw_x.wrapping_mul(raw_x).wrapping_add(raw_y.wrapping_mul(raw_y));
    let threshold_squared = joystick_threshold.wrapping_mul(joystick_threshold);
    if (dist_squared as u32) < (threshold_squared as u32) {
        *axis_state.add(0) = 0;
        *axis_state.add(1) = 0;
    } else {
        // 0 = right, > 0 = downward, < 0 = upward.
        let angle = (raw_y as f64).atan2(raw_x as f64);

        if angle.abs() < (60.0 * DEGREES_TO_RADIANS) {
            *axis_state.add(0) = 1; // 120 degree range facing right
        } else if angle.abs() > (120.0 * DEGREES_TO_RADIANS) {
            *axis_state.add(0) = -1; // 120 degree range facing left
        } else if !(angle < 0.0 && Kid.action == actions_actions_1_run_jump as byte) {
            // Horizontally neutral, so release -- unless the Kid is mid running-jump and the
            // stick is pushed upward, where releasing would stop the run.
            *axis_state.add(0) = 0;
        }

        if angle < (-30.0 * DEGREES_TO_RADIANS) && angle > (-150.0 * DEGREES_TO_RADIANS) {
            *axis_state.add(1) = -1; // 120 degree range facing up
        } else if angle > (35.0 * DEGREES_TO_RADIANS) && angle < (145.0 * DEGREES_TO_RADIANS) {
            // Down is slightly less sensitive than up, so a thumb slipping down doesn't crouch.
            *axis_state.add(1) = 1; // 110 degree range facing down
        } else if !((frameids_frame_108_fall_land_2 as byte
            ..=frameids_frame_112_stand_up_from_crouch_3 as byte)
            .contains(&Kid.frame)
            && angle > 0.0)
        {
            // Vertically neutral, so release -- unless the Kid is standing up from a crouch and
            // the stick is pushed downward, which is how a crouch-hop is held.
            *axis_state.add(1) = 0;
        }
    }
}

/// [`get_joystick_state`] for `joystick_only_horizontal`: a simple threshold on X, Y forced to 0.
///
/// With this on, up/down must come from the D-pad or the Y/A buttons instead.
unsafe fn get_joystick_state_hor_only(raw_x: c_int, axis_state: *mut c_int) {
    if raw_x > joystick_threshold {
        *axis_state.add(0) = 1;
    } else if raw_x < -joystick_threshold {
        *axis_state.add(0) = -1;
    } else {
        *axis_state.add(0) = 0;
    }
    *axis_state.add(1) = 0;
}

// seg000:1051
/// Fold the gamepad into the `control_*` globals.
///
/// Either stick, and the D-pad, all feed the same controls. Notice there are no `else` branches:
/// this only ever *sets* a control, never releases one, because [`do_paused`] has already cleared
/// them and a neutral stick must not undo what the D-pad said.
///
/// `fix_register_quick_input` swaps in the per-tick maxima (`joy_axis_max`) and accepts
/// `KEYSTATE_HELD_NEW`, so a flick shorter than one tick still registers.
#[no_mangle]
pub unsafe extern "C" fn read_joyst_control() {
    let key_state: c_int;
    let joy_axis_ptr: *mut c_int;
    if (*fixes).fix_register_quick_input != 0 {
        key_state = KEYSTATE_HELD_I | KEYSTATE_HELD_NEW_I;
        joy_axis_ptr = joy_axis_max.as_mut_ptr();
    } else {
        key_state = KEYSTATE_HELD_I;
        joy_axis_ptr = joy_axis.as_mut_ptr();
    }

    if joystick_only_horizontal != 0 {
        get_joystick_state_hor_only(
            *joy_axis_ptr.add(SDL_CONTROLLER_AXIS_LEFTX),
            joy_left_stick_states.as_mut_ptr(),
        );
        get_joystick_state_hor_only(
            *joy_axis_ptr.add(SDL_CONTROLLER_AXIS_RIGHTX),
            joy_right_stick_states.as_mut_ptr(),
        );
    } else {
        get_joystick_state(
            *joy_axis_ptr.add(SDL_CONTROLLER_AXIS_LEFTX),
            *joy_axis_ptr.add(SDL_CONTROLLER_AXIS_LEFTY),
            joy_left_stick_states.as_mut_ptr(),
        );
        get_joystick_state(
            *joy_axis_ptr.add(SDL_CONTROLLER_AXIS_RIGHTX),
            *joy_axis_ptr.add(SDL_CONTROLLER_AXIS_RIGHTY),
            joy_right_stick_states.as_mut_ptr(),
        );
    }

    if joy_left_stick_states[0] == -1
        || joy_right_stick_states[0] == -1
        || joy_button_states[JOYINPUT_DPAD_LEFT as usize] & key_state != 0
    {
        control_x = CONTROL_HELD_LEFT as sbyte;
    }

    if joy_left_stick_states[0] == 1
        || joy_right_stick_states[0] == 1
        || joy_button_states[JOYINPUT_DPAD_RIGHT as usize] & key_state != 0
    {
        control_x = CONTROL_HELD_RIGHT as sbyte;
    }

    if joy_left_stick_states[1] == -1
        || joy_right_stick_states[1] == -1
        || joy_button_states[JOYINPUT_DPAD_UP as usize] & key_state != 0
        || joy_button_states[JOYINPUT_Y as usize] & key_state != 0
    {
        control_y = CONTROL_HELD_UP as sbyte;
    }

    if joy_left_stick_states[1] == 1
        || joy_right_stick_states[1] == 1
        || joy_button_states[JOYINPUT_DPAD_DOWN as usize] & key_state != 0
        || joy_button_states[JOYINPUT_A as usize] & key_state != 0
    {
        control_y = CONTROL_HELD_DOWN as sbyte;
    }

    if joy_button_states[JOYINPUT_X as usize] & key_state != 0
        || *joy_axis_ptr.add(SDL_CONTROLLER_AXIS_TRIGGERLEFT) > 8000
        || *joy_axis_ptr.add(SDL_CONTROLLER_AXIS_TRIGGERRIGHT) > 8000
    {
        control_shift = CONTROL_HELD as sbyte;
    }
}

// seg000:10EA
/// Draw the Kid's HP bar: empty vials from `curr_hp` to `max_hp`, then full ones up to `curr_hp`.
///
/// Callers exploit the two independent ranges: `draw_kid_hp(1, 0)` draws *only* the full vial in
/// slot 0 and `draw_kid_hp(0, 1)` draws only the empty one, which is how [`draw_hp`] blinks the
/// last hit point.
#[no_mangle]
pub unsafe extern "C" fn draw_kid_hp(curr_hp: c_short, max_hp: c_short) {
    for drawn_hp_index in curr_hp..max_hp {
        // empty HP
        method_6_blit_img_to_scr(
            get_image(chtabs_id_chtab_2_kid as c_short, 217),
            drawn_hp_index as c_int * 7,
            194,
            blitters_blitters_0_no_transp as c_int,
        );
    }
    for drawn_hp_index in 0..curr_hp {
        // full HP
        method_6_blit_img_to_scr(
            get_image(chtabs_id_chtab_2_kid as c_short, 216),
            drawn_hp_index as c_int * 7,
            194,
            blitters_blitters_0_no_transp as c_int,
        );
    }
}

// seg000:1159
/// Draw the guard's HP bar, mirrored to the right-hand side of the status line.
///
/// Unlike the Kid's bar this uses a single sprite twice: blitted black for an empty slot, opaque
/// for a full one. Skeletons and mice never show HP (they cannot be killed by damage), and the
/// shadow only shows it on level 12, where he actually fights back.
#[no_mangle]
pub unsafe extern "C" fn draw_guard_hp(curr_hp: c_short, max_hp: c_short) {
    if chtab_addrs[chtabs_id_chtab_5_guard as usize].is_null() {
        return;
    }
    let guard_charid = Guard.charid as c_short;
    if guard_charid != charids_charid_4_skeleton as c_short
        && guard_charid != charids_charid_24_mouse as c_short
        && (guard_charid != charids_charid_1_shadow as c_short || current_level == 12)
    {
        let chtab = chtab_addrs[chtabs_id_chtab_5_guard as usize];
        for drawn_hp_index in curr_hp..max_hp {
            method_6_blit_img_to_scr(
                chtab_image(chtab, 0),
                314 - drawn_hp_index as c_int * 7,
                194,
                blitters_blitters_9_black as c_int,
            );
        }
        for drawn_hp_index in 0..curr_hp {
            method_6_blit_img_to_scr(
                chtab_image(chtab, 0),
                314 - drawn_hp_index as c_int * 7,
                194,
                blitters_blitters_0_no_transp as c_int,
            );
        }
    }
}

// seg000:11EC
/// Big-potion effect: raise maximum HP by one, capped at the CusPop `max_hitp_allowed`, and heal
/// to full.
#[no_mangle]
pub unsafe extern "C" fn add_life() {
    let mut hpmax = hitp_max as c_short;
    hpmax += 1;
    if hpmax as c_int > (*custom).max_hitp_allowed as c_int {
        hpmax = (*custom).max_hitp_allowed as c_short;
    }
    hitp_max = hpmax as word;
    set_health_life();
}

// seg000:1200
/// Queue a heal to full: [`do_delta_hp`] applies `hitp_delta` at the end of the tick.
#[no_mangle]
pub unsafe extern "C" fn set_health_life() {
    hitp_delta = (hitp_max as c_int - hitp_curr as c_int) as c_short;
}

// seg000:120B
/// Redraw either HP bar if it changed, and blink the last remaining hit point.
///
/// The blink phase normally comes from the low bit of the game clock, which means it stops
/// advancing once the clock does — `fix_one_hp_stops_blinking` sources it from a free-running
/// counter instead. Level 15 (the potions level) is exempt: HP there is a puzzle, not a threat.
#[no_mangle]
pub unsafe extern "C" fn draw_hp() {
    if hitp_delta != 0 {
        draw_kid_hp(hitp_curr as c_short, hitp_max as c_short);
    }

    // FIX_ONE_HP_STOPS_BLINKING
    let blink_state: bool = if (*fixes).fix_one_hp_stops_blinking != 0 {
        global_blink_state
    } else {
        (rem_tick & 1) != 0
    };

    if hitp_curr == 1 && current_level != 15 {
        // blinking hitpoint
        if blink_state {
            draw_kid_hp(1, 0);
        } else {
            draw_kid_hp(0, 1);
        }
    }
    if guardhp_delta != 0 {
        draw_guard_hp(guardhp_curr as c_short, guardhp_max as c_short);
    }
    if guardhp_curr == 1 {
        if blink_state {
            draw_guard_hp(1, 0);
        } else {
            draw_guard_hp(0, 1);
        }
    }
}

// seg000:127B
/// Apply this tick's accumulated HP changes to both fighters, clamped to `[0, max]`.
///
/// Every source of damage or healing writes `hitp_delta`/`guardhp_delta` rather than the HP
/// itself, so all of them land at one point in the tick. Level 12 is the exception the first
/// branch encodes: the shadow *is* the Kid, so hurting him hurts you too.
#[no_mangle]
pub unsafe extern "C" fn do_delta_hp() {
    // Level 12: if the shadow is hurt, the Kid is also hurt.
    if Opp.charid == charids_charid_1_shadow as byte && current_level == 12 && guardhp_delta != 0 {
        hitp_delta = guardhp_delta;
    }
    hitp_curr =
        ((hitp_curr as c_int + hitp_delta as c_int).max(0)).min(hitp_max as c_int) as word;
    guardhp_curr =
        ((guardhp_curr as c_int + guardhp_delta as c_int).max(0)).min(guardhp_max as c_int) as word;
}

/// FIX_SOUND_PRIORITIES: retune three table entries to match PoP 1.3.
///
/// Without this, running into spikes never played the "spiked" sound (the looping spikes sound
/// outranked it and was not interruptible), and with 1.3's sound set the "guard hurt" sound was
/// swallowed by a parry immediately preceding the hit.
#[no_mangle]
pub unsafe extern "C" fn fix_sound_priorities() {
    sound_interruptible_set(soundids_sound_49_spikes as usize, 1);
    sound_prio_table[soundids_sound_48_spiked as usize] = 0x15; // moved above spikes
    sound_prio_table[soundids_sound_10_sword_vs_sword as usize] = 0x0D; // below hit_user/hit_guard
}

// seg000:12C5
/// Nominate a sound to play at the end of this tick, if it outranks whatever is already nominated.
///
/// Nothing is played here. Lower `sound_prio_table` values win. A sound with no loaded buffer is
/// dropped, as is a PC-speaker-only rendition of a sound that has no PC speaker version.
#[no_mangle]
pub unsafe extern "C" fn play_sound(sound_id: c_int) {
    if next_sound < 0
        || sound_prio_table[sound_id as usize] <= sound_prio_table[next_sound as usize]
    {
        if sound_pointers[sound_id as usize].is_null() {
            return;
        }
        if sound_pcspeaker_exists[sound_id as usize] != 0
            || (*sound_pointers[sound_id as usize]).type_ != sound_type_sound_speaker as byte
        {
            next_sound = sound_id as c_short;
        }
    }
}

// seg000:1304
/// Start the sound [`play_sound`] nominated this tick, then clear the nomination.
///
/// It is only actually started if nothing is playing, or if what is playing is marked
/// interruptible *and* is outranked. `next_sound` is cleared unconditionally, so a nomination
/// that loses is simply dropped rather than queued.
#[no_mangle]
pub unsafe extern "C" fn play_next_sound() {
    if next_sound >= 0 {
        if check_sound_playing() == 0
            || (sound_interruptible_at(current_sound as usize) != 0
                && sound_prio_table[next_sound as usize] <= sound_prio_table[current_sound as usize])
        {
            current_sound = next_sound as word;
            play_sound_from_buffer(sound_pointers[current_sound as usize]);
        }
    }
    next_sound = -1;
}

// seg000:1353
/// Play the clash sound if either fighter is on the parry frame (167).
#[no_mangle]
pub unsafe extern "C" fn check_sword_vs_sword() {
    if Kid.frame == 167 || Guard.frame == 167 {
        play_sound(soundids_sound_10_sword_vs_sword as c_int);
    }
}

// seg000:136A
/// Load one sprite table from a `.DAT` file, unless that slot is already populated.
#[no_mangle]
pub unsafe extern "C" fn load_chtab_from_file(
    chtab_id: c_int,
    resource: c_int,
    filename: *const c_char,
    palette_bits: c_int,
) {
    if !chtab_addrs[chtab_id as usize].is_null() {
        return;
    }
    let dh = open_dat(filename, b'G' as c_int);
    chtab_addrs[chtab_id as usize] = load_sprites_from_file(resource, palette_bits, 1);
    close_dat(dh);
}

// seg000:13BA
/// Free sprite tables `first..10`. Slots below `first` are the ones that outlive the level.
#[no_mangle]
pub unsafe extern "C" fn free_all_chtabs_from(first: c_int) {
    free_peels();
    for chtab_id in (first as word)..10 {
        if !chtab_addrs[chtab_id as usize].is_null() {
            free_chtab(chtab_addrs[chtab_id as usize]);
            chtab_addrs[chtab_id as usize] = null_mut();
        }
    }
}

// seg009:12EF
/// Overwrite a contiguous run of an existing sprite table with higher-quality replacements.
///
/// An image that fails to load leaves the original in place, so a data file that only supplies
/// some of the range still works.
unsafe fn load_one_optgraf(
    chtab_ptr: *mut chtab_type,
    pal_ptr: *mut dat_pal_type,
    base_id: c_int,
    min_index: c_int,
    max_index: c_int,
) {
    for index in (min_index as c_short)..=(max_index as c_short) {
        let image = load_image(base_id + index as c_int + 1, pal_ptr);
        if !image.is_null() {
            chtab_image_set(chtab_ptr, index as usize, image);
        }
    }
}

// seg000:13FC
/// Patch the eight optional-graphics ranges into the environment sprite table.
///
/// The `.DAT` file is opened lazily on the first iteration and reused for the rest — a leftover
/// of the C shape, where the loop body was conditional per range.
#[no_mangle]
pub unsafe extern "C" fn load_more_opt_graf(filename: *const c_char) {
    let mut area: dat_shpl_type = core::mem::zeroed();
    let mut dh: *mut dat_type = null_mut();
    for graf_index in 0..8 {
        if dh.is_null() {
            dh = open_dat(filename, b'G' as c_int);
            load_from_opendats_to_area(
                200,
                addr_of_mut!(area) as *mut c_void,
                core::mem::size_of::<dat_shpl_type>() as c_int,
                b"pal\0".as_ptr() as *const c_char,
            );
            area.palette.row_bits = 0x20;
        }
        load_one_optgraf(
            chtab_addrs[chtabs_id_chtab_6_environment as usize],
            addr_of_mut!(area.palette),
            1200,
            OPTGRAF_MIN[graf_index] as c_int - 1,
            OPTGRAF_MAX[graf_index] as c_int - 1,
        );
    }
    if !dh.is_null() {
        close_dat(dh);
    }
}

// seg000:148D
/// Poll input for one tick, dispatch one keypress, and service the pause state.
///
/// Returns non-zero if any key or the Shift/action button was pressed — cutscene code uses that
/// as "the player wants to skip this".
///
/// The four `control_*` globals are cleared to `CONTROL_RELEASED` first, then whichever reader is
/// active *sets* the ones it has an opinion about; see the module docs. Pausing without the menu
/// enabled spins in a nested read-key loop right here, which is why a paused game costs no CPU
/// but also runs no simulation.
///
/// The three loops at the end close out the tick: every `KEYSTATE_HELD_NEW` bit is cleared (which
/// is what makes it mean "since the last tick"), and the per-tick axis maxima are reset to the
/// current axis values so the next tick starts measuring afresh.
#[no_mangle]
pub unsafe extern "C" fn do_paused() -> c_int {
    // USE_REPLAY
    if replaying != 0 && skipping_replay != 0 {
        return 0;
    }

    let key: word;
    next_room = 0;
    control_shift = CONTROL_RELEASED as sbyte;
    control_y = CONTROL_RELEASED as sbyte;
    control_x = CONTROL_RELEASED as sbyte;
    if is_joyst_mode != 0 {
        read_joyst_control();
    } else {
        read_keyb_control();
    }
    key = process_key() as word;
    // Fix being able to pause the game during the ending sequence.
    if is_ending_sequence && is_paused != 0 {
        is_paused = 0;
    }
    if is_paused != 0 {
        // Feather fall gets interrupted by pause.
        if (*fixes).fix_quicksave_during_feather != 0 && is_feather_fall > 0 && check_sound_playing() != 0
        {
            stop_sounds();
        }
        display_text_bottom(b"GAME PAUSED\0".as_ptr() as *const c_char);
        // USE_MENU
        if enable_pause_menu != 0 || is_menu_shown != 0 {
            draw_menu();
            menu_was_closed();
        } else {
            is_paused = 0;
            // Busy-wait for the next keypress; the simulation is not advancing meanwhile.
            loop {
                idle();
                delay_ticks(1);
                if process_key() != 0 {
                    break;
                }
            }
        }
        erase_bottom_text(1);
    }

    // Input for this gameplay tick has been consumed: age every "new" flag out and rebase the
    // per-tick axis maxima on the current axis positions.
    for state in key_states.iter_mut() {
        *state &= !(KEYSTATE_HELD_NEW as byte);
    }
    for state in joy_button_states[..JOYINPUT_NUM as usize].iter_mut() {
        *state &= !KEYSTATE_HELD_NEW_I;
    }
    joy_axis_max[..JOY_AXIS_NUM as usize].copy_from_slice(&joy_axis[..JOY_AXIS_NUM as usize]);

    (key != 0 || control_shift != 0) as c_int
}

// seg000:1500
/// Fold the keyboard into the `control_*` globals.
///
/// Each direction accepts several keys at once: the arrows, the numeric keypad (including the
/// diagonals, which set both axes), and the user's remapped `key_*` bindings. Unlike
/// [`read_joyst_control`], `control_shift` *is* released here when nothing is held, because the
/// keyboard is authoritative about the action button.
#[no_mangle]
pub unsafe extern "C" fn read_keyb_control() {
    let key_state: c_int = if (*fixes).fix_register_quick_input != 0 {
        KEYSTATE_HELD_I | KEYSTATE_HELD_NEW_I
    } else {
        KEYSTATE_HELD_I
    };

    // True if the given scancode counts as pressed under the current key_state mask. Also used
    // for the remappable key_* bindings, which are themselves scancodes.
    let held = |scancode: c_int| (key_states[scancode as usize] as c_int) & key_state != 0;

    if held(SDL_SCANCODE_UP)
        || held(SDL_SCANCODE_HOME)
        || held(SDL_SCANCODE_PAGEUP)
        || held(SDL_SCANCODE_KP_8)
        || held(SDL_SCANCODE_KP_7)
        || held(SDL_SCANCODE_KP_9)
        || held(key_up)
        || held(key_jump_left)
        || held(key_jump_right)
    {
        control_y = CONTROL_HELD_UP as sbyte;
    } else if held(SDL_SCANCODE_CLEAR)
        || held(SDL_SCANCODE_DOWN)
        || held(SDL_SCANCODE_KP_5)
        || held(SDL_SCANCODE_KP_2)
        || held(key_down)
    {
        control_y = CONTROL_HELD_DOWN as sbyte;
    }
    if held(SDL_SCANCODE_LEFT)
        || held(SDL_SCANCODE_HOME)
        || held(SDL_SCANCODE_KP_4)
        || held(SDL_SCANCODE_KP_7)
        || held(key_left)
        || held(key_jump_left)
    {
        control_x = CONTROL_HELD_LEFT as sbyte;
    } else if held(SDL_SCANCODE_RIGHT)
        || held(SDL_SCANCODE_PAGEUP)
        || held(SDL_SCANCODE_KP_6)
        || held(SDL_SCANCODE_KP_9)
        || held(key_right)
        || held(key_jump_right)
    {
        control_x = CONTROL_HELD_RIGHT as sbyte;
    }

    control_shift = if held(SDL_SCANCODE_LSHIFT) || held(SDL_SCANCODE_RSHIFT) || held(key_action) {
        CONTROL_HELD as sbyte
    } else {
        CONTROL_RELEASED as sbyte
    };

    // USE_DEBUG_CHEATS: '[' and ']' nudge the character one pixel sideways.
    if cheats_enabled != 0 && debug_cheats_enabled != 0 {
        if held(SDL_SCANCODE_RIGHTBRACKET) {
            Char.x = Char.x.wrapping_add(1);
        } else if held(SDL_SCANCODE_LEFTBRACKET) {
            Char.x = Char.x.wrapping_sub(1);
        }
    }
}

/// A `showmessage()`-alike that can also report modifier keys, for the key-rebinding dialog.
///
/// Blocks until any key is pressed and returns its scancode. `last_any_key_scancode` is cleared
/// on entry so the Enter that opened the dialog is not read as the answer, and again inside the
/// loop so the keypress does not leak back into the menu.
unsafe fn showmessage_any_key(text: *const c_char, _arg_4: c_int, _arg_0: *mut c_void) -> c_int {
    let mut key: word;
    let mut rect: rect_type = core::mem::zeroed();
    method_1_blit_rect(
        offscreen_surface,
        onscreen_surface_,
        addr_of!((*copyprot_dialog).peel_rect),
        addr_of!((*copyprot_dialog).peel_rect),
        0,
    );
    draw_dialog_frame(copyprot_dialog);
    shrink2_rect(&mut rect, addr_of!((*copyprot_dialog).text_rect), 2, 1);
    show_text_with_color(&rect, 0, 0, text, colorids_color_15_brightwhite as c_int);
    clear_kbd_buf();
    last_any_key_scancode = 0;
    loop {
        idle();
        clear_kbd_buf();
        key = last_any_key_scancode as word;
        last_any_key_scancode = 0;
        if key != 0 {
            break;
        }
    }
    need_full_redraw = 1;
    key as c_int
}

/// Prompt for and store one key binding. Esc leaves the existing binding alone.
#[no_mangle]
pub unsafe extern "C" fn redefine_key(name: *const c_char, key: *mut c_int) {
    let mut message = [0i8; 256];
    cbuf_set(
        &mut message,
        &format!(
            "Redefining keys:\nPress key for \"{}\".\nOr press Esc to cancel.",
            cstr(name)
        ),
    );

    // Use the regular big font for the dialog instead of the small menu font.
    let saved_font = textstate.ptr_font;
    textstate.ptr_font = addr_of_mut!(hc_font);

    let new_key = showmessage_any_key(message.as_ptr(), 1, key_test_quit as *mut c_void);

    textstate.ptr_font = saved_font;

    if new_key == SDL_SCANCODE_ESCAPE {
        return;
    }
    *key = new_key;
}

/// Walk the player through rebinding all seven movement/action keys.
#[no_mangle]
pub unsafe extern "C" fn redefine_keys() {
    redefine_key(b"left\0".as_ptr() as *const c_char, addr_of_mut!(key_left));
    redefine_key(b"right\0".as_ptr() as *const c_char, addr_of_mut!(key_right));
    redefine_key(b"up\0".as_ptr() as *const c_char, addr_of_mut!(key_up));
    redefine_key(b"down\0".as_ptr() as *const c_char, addr_of_mut!(key_down));
    redefine_key(b"jump left\0".as_ptr() as *const c_char, addr_of_mut!(key_jump_left));
    redefine_key(b"jump right\0".as_ptr() as *const c_char, addr_of_mut!(key_jump_right));
    redefine_key(b"action\0".as_ptr() as *const c_char, addr_of_mut!(key_action));
}

// seg000:156D
/// Blit one dirty rectangle from the offscreen buffer to the screen, flipping it if the
/// upside-down cheat is active.
///
/// The rectangle is mirrored about the gameplay area's midline rather than the whole screen, so
/// the status line at the bottom stays put.
// target_rect_ptr aliases target_rect's memory (taken before the fields below are filled
// in), so those writes ARE observed later through the pointer -- Rust's liveness lint
// doesn't track that aliasing and flags them as dead stores.
#[allow(unused_assignments)]
#[no_mangle]
pub unsafe extern "C" fn copy_screen_rect(source_rect_ptr: *const rect_type) {
    let target_rect_ptr: *const rect_type;
    let mut target_rect: rect_type = core::mem::zeroed();
    if upside_down != 0 {
        target_rect_ptr = &target_rect;
        target_rect = *source_rect_ptr;
        target_rect.top = SCREEN_GAMEPLAY_HEIGHT as c_short - (*source_rect_ptr).bottom;
        target_rect.bottom = SCREEN_GAMEPLAY_HEIGHT as c_short - (*source_rect_ptr).top;
    } else {
        target_rect_ptr = source_rect_ptr;
    }
    method_1_blit_rect(onscreen_surface_, offscreen_surface, target_rect_ptr, target_rect_ptr, 0);
    // USE_LIGHTING
    update_lighting(target_rect_ptr);
}

// seg000:15E9
/// Toggle the invert-screen cheat.
///
/// `upside_down` is a `word` and C flips it with `~`, not `!` — so it cycles 0 / 0xFFFF, and Rust's
/// `!` on an integer is the same bitwise operation. Every test of it is against zero, so the
/// exact non-zero value never matters.
#[no_mangle]
pub unsafe extern "C" fn toggle_upside() {
    upside_down = !upside_down;
    need_redraw_because_flipped = 1;
}

// seg000:15F8
/// Start feather fall: the Kid drifts down and takes no fall damage.
///
/// Without `fix_quicksave_during_feather` this is a bare flag with no duration, cleared by the
/// code that consumes it; with the fix it becomes a tick countdown, which is what makes it
/// survive a quicksave.
#[no_mangle]
pub unsafe extern "C" fn feather_fall() {
    if (*fixes).fix_quicksave_during_feather != 0 {
        is_feather_fall =
            (FEATHER_FALL_LENGTH * get_ticks_per_sec(timerids_timer_1 as c_int)) as word;
    } else {
        is_feather_fall = 1;
    }
    flash_color = 2; // green
    flash_time = 3;
    stop_sounds();
    play_sound(soundids_sound_39_low_weight as c_int);
}

// seg000:1618
/// Select the graphics mode. A stub: the port only supports MCGA/VGA.
#[no_mangle]
pub unsafe extern "C" fn parse_grmode() -> c_int {
    set_gr_mode(grmodes_gmMcgaVga as byte);
    grmodes_gmMcgaVga as c_int
}

// seg000:172C
/// Generate the palace level's mottled brick pattern for the drawn room.
///
/// Three brick rows of four sub-rows of eleven columns. Odd and even sub-rows draw from different
/// four-colour ranges, and no brick may repeat the colour immediately to its left — hence the
/// inner reroll loop.
///
/// The RNG is temporarily reseeded from the room number and restored afterwards, so the pattern
/// is stable per room across visits without perturbing the gameplay RNG stream.
#[no_mangle]
pub unsafe extern "C" fn gen_palace_wall_colors() {
    let old_randseed = random_seed;
    random_seed = drawn_room as dword;
    prandom(1); // discard
    for row in 0i16..3 {
        for subrow in 0i16..4 {
            // 0x61..0x64 in sub-rows 1 and 3; 0x66..0x69 in sub-rows 0 and 2.
            let color_base: word = if subrow % 2 != 0 { 0x61 } else { 0x66 };
            let mut prev_color: word = 0xFFFF; // C's `word prev_color = -1`: matches no colour
            for column in 0i16..=10 {
                let color = loop {
                    let candidate = color_base.wrapping_add(prandom(3));
                    if candidate != prev_color {
                        break candidate;
                    }
                };
                palace_wall_colors[(44 * row + 11 * subrow + column) as usize] = color as byte;
                prev_color = color;
            }
        }
    }
    random_seed = old_randseed;
}

/// Blit the title-text band back from the offscreen buffer, erasing whatever text was over it.
///
/// The title sequence works by drawing successive captions into this one rectangle; each step
/// restores the clean background first.
unsafe fn restore_titles_rect() {
    method_1_blit_rect(
        onscreen_surface_,
        offscreen_surface,
        addr_of!(rect_titles),
        addr_of!(rect_titles),
        blitters_blitters_0_no_transp as c_int,
    );
}

/// Idle until nothing is playing, keeping input responsive so the sequence stays skippable.
unsafe fn wait_for_sounds_to_finish() {
    while check_sound_playing() != 0 {
        idle();
        do_paused();
        delay_ticks(1);
    }
}

// seg000:17E6
/// Play the whole title sequence, then start the demo level.
///
/// A fixed script: the Broderbund/Mechner credits over the main title art, the "In the absence of
/// the Sultan" story frame, the cutscene of the princess (`load_intro`), the credits, and the
/// hall of fame if anyone has finished the game. Each caption waits on `timer_0` for a fixed
/// number of ticks, and every wait routes through [`do_paused`], so any keypress escapes.
///
/// Ends in `init_game(0)` — level 0 is the attract-mode demo.
#[no_mangle]
pub unsafe extern "C" fn show_title() {
    load_opt_sounds(
        soundids_sound_50_story_2_princess as c_int,
        soundids_sound_55_story_1_absence as c_int,
    );
    dont_reset_time = 0;
    if !offscreen_surface.is_null() {
        free_surface(offscreen_surface);
    }
    offscreen_surface = make_offscreen_buffer(addr_of!(screen_rect));
    load_title_images(1);
    current_target_surface = offscreen_surface;
    idle();
    do_paused();

    draw_full_image(full_image_id_TITLE_MAIN);
    fade_in_2(offscreen_surface, 0x1000);
    method_1_blit_rect(
        onscreen_surface_,
        offscreen_surface,
        addr_of!(screen_rect),
        addr_of!(screen_rect),
        blitters_blitters_0_no_transp as c_int,
    );
    current_sound = soundids_sound_54_intro_music as word;
    play_sound_from_buffer(sound_pointers[soundids_sound_54_intro_music as usize]);
    start_timer(timerids_timer_0 as c_int, 0x82);
    draw_full_image(full_image_id_TITLE_PRESENTS);
    do_wait(timerids_timer_0 as c_int);

    start_timer(timerids_timer_0 as c_int, 0xCD);
    restore_titles_rect();
    draw_full_image(full_image_id_TITLE_MAIN);
    do_wait(timerids_timer_0 as c_int);

    start_timer(timerids_timer_0 as c_int, 0x41);
    restore_titles_rect();
    draw_full_image(full_image_id_TITLE_MAIN);
    draw_full_image(full_image_id_TITLE_GAME);
    do_wait(timerids_timer_0 as c_int);

    start_timer(timerids_timer_0 as c_int, 0x10E);
    restore_titles_rect();
    draw_full_image(full_image_id_TITLE_MAIN);
    do_wait(timerids_timer_0 as c_int);

    start_timer(timerids_timer_0 as c_int, 0xEB);
    restore_titles_rect();
    draw_full_image(full_image_id_TITLE_MAIN);
    draw_full_image(full_image_id_TITLE_POP);
    draw_full_image(full_image_id_TITLE_MECHNER);
    do_wait(timerids_timer_0 as c_int);

    restore_titles_rect();
    draw_full_image(full_image_id_STORY_FRAME);
    draw_full_image(full_image_id_STORY_ABSENCE);
    current_target_surface = onscreen_surface_;
    wait_for_sounds_to_finish();
    play_sound_from_buffer(sound_pointers[soundids_sound_55_story_1_absence as usize]);
    transition_ltr();
    pop_wait(timerids_timer_0 as c_int, 0x258);
    fade_out_2(0x800);
    release_title_images();

    load_intro(0, Some(pv_scene), 0);

    load_title_images(1);
    current_target_surface = offscreen_surface;
    draw_full_image(full_image_id_STORY_FRAME);
    draw_full_image(full_image_id_STORY_MARRY);
    fade_in_2(offscreen_surface, 0x800);
    draw_full_image(full_image_id_TITLE_MAIN);
    draw_full_image(full_image_id_TITLE_POP);
    draw_full_image(full_image_id_TITLE_MECHNER);
    wait_for_sounds_to_finish();
    transition_ltr();
    pop_wait(timerids_timer_0 as c_int, 0x78);
    draw_full_image(full_image_id_STORY_FRAME);
    draw_full_image(full_image_id_STORY_CREDITS);
    transition_ltr();
    pop_wait(timerids_timer_0 as c_int, 0x168);
    if hof_count != 0 {
        draw_full_image(full_image_id_STORY_FRAME);
        draw_full_image(full_image_id_HOF_POP);
        show_hof();
        transition_ltr();
        pop_wait(timerids_timer_0 as c_int, 0xF0);
    }
    current_target_surface = onscreen_surface_;
    wait_for_sounds_to_finish();
    fade_out_2(0x1800);
    free_surface(offscreen_surface);
    offscreen_surface = null_mut();
    release_title_images();
    init_game(0);
}

// seg000:1BB3
/// Wipe the offscreen buffer onto the screen in a left-to-right sweep, two pixels at a time.
///
/// The pace is a fixed 120 wipe-frames per second, estimated from the transition speed on an
/// Apple IIe (<https://www.youtube.com/watch?v=7m7j2VuWhQ0>), so it looks the same regardless of
/// CPU speed. The inner loop busy-waits for the next frame boundary; if a slow machine falls
/// behind, `overshoot` lets the next few columns blit without a screen refresh to catch up.
#[no_mangle]
pub unsafe extern "C" fn transition_ltr() {
    let mut rect = rect_type { top: 0, bottom: 200, left: 0, right: 2 };
    let mut transition_fps = 120;
    // USE_FAST_FORWARD
    transition_fps *= audio_speed;
    let counters_per_frame = perf_frequency / transition_fps as u64;
    last_transition_counter = crate::platform::sdl::shared_renderer().performance_counter();
    let mut overshoot = 0;
    for _position in (0..320).step_by(2) {
        method_1_blit_rect(onscreen_surface_, offscreen_surface, &rect, &rect, 0);
        rect.left += 2;
        rect.right += 2;
        if overshoot > 0 && overshoot < 10 {
            overshoot -= 1;
            continue; // let the animation catch up before refreshing the screen again
        }
        idle();
        do_paused();
        // Delay until the next wipe frame is due, so this isn't instantaneous on a fast CPU.
        loop {
            let current_counter = crate::platform::sdl::shared_renderer().performance_counter();
            let frametimes_elapsed = ((current_counter / counters_per_frame)
                - (last_transition_counter / counters_per_frame)) as c_int;
            if frametimes_elapsed > 0 {
                overshoot = frametimes_elapsed - 1;
                last_transition_counter = current_counter;
                break; // proceed to the next frame
            } else {
                crate::platform::sdl::shared_renderer().delay(1);
            }
        }
    }
}

// seg000:1C0F
/// Free the two title-screen sprite tables. Idempotent.
#[no_mangle]
pub unsafe extern "C" fn release_title_images() {
    if !chtab_title50.is_null() {
        free_chtab(chtab_title50);
        chtab_title50 = null_mut();
    }
    if !chtab_title40.is_null() {
        free_chtab(chtab_title40);
        chtab_title40 = null_mut();
    }
}

// seg000:1C3A
/// Draw one of the title/cutscene full-screen images, at the position and with the blitter its
/// `full_image` table entry specifies.
///
/// `blitters_white` is not a blitter but a request to resolve the current text colour and then
/// blit mono — in C that is a `switch` case falling through into `default`, which is why the
/// white arm and the final `else` arm below do the same thing.
#[no_mangle]
pub unsafe extern "C" fn draw_full_image(id: full_image_id) {
    let idx = id as usize;

    if id >= full_image_id_MAX_FULL_IMAGES {
        return;
    }
    if (*full_image[idx].chtab).is_null() {
        return;
    }
    let chtab = *full_image[idx].chtab;
    let decoded_image = chtab_image(chtab, full_image[idx].id as usize);
    let mut blit = full_image[idx].blitter as c_int;
    let xpos = full_image[idx].xpos;
    let ypos = full_image[idx].ypos;

    if blit == blitters_blitters_white as c_int {
        // Falls through to the mono blit below in C.
        blit = get_text_color(15, colorids_color_15_brightwhite as c_int, 0x800);
        method_3_blit_mono(decoded_image, xpos, ypos, blitters_blitters_0_no_transp as c_int, blit as byte);
    } else if blit == blitters_blitters_10h_transp as c_int {
        // CGA/Hercules would build a separate 1-bit mask here and free it afterwards; in VGA the
        // image is its own mask. parse_grmode() only ever selects VGA, so that arm is dead.
        let mask = decoded_image;
        draw_image_transp(decoded_image, mask, xpos, ypos);
    } else if blit == blitters_blitters_0_no_transp as c_int {
        method_6_blit_img_to_scr(decoded_image, xpos, ypos, blit);
    } else {
        method_3_blit_mono(decoded_image, xpos, ypos, blitters_blitters_0_no_transp as c_int, blit as byte);
    }
}

// seg000:1D2C
/// Load the Kid's sprite sheet from `KID.DAT`.
#[no_mangle]
pub unsafe extern "C" fn load_kid_sprite() {
    load_chtab_from_file(
        chtabs_id_chtab_2_kid as c_int,
        400,
        b"KID.DAT\0".as_ptr() as *const c_char,
        1 << 7,
    );
}

const SAVE_FILE: &[u8] = b"PRINCE.SAV\0";

/// Resolve where `PRINCE.SAV` lives for the current levelset. See [`get_writable_file_path`].
unsafe fn get_save_path(custom_path_buffer: *mut c_char, max_len: usize) -> *const c_char {
    get_writable_file_path(custom_path_buffer, max_len, SAVE_FILE.as_ptr() as *const c_char)
}

// seg000:1D45
/// Write the long-term save (Ctrl+G): four 16-bit fields, nothing else.
///
/// This is the original game's save, quite unlike a quicksave — it records only where you are and
/// how you were doing, and resuming replays the level from its start. A partial write is deleted
/// rather than left behind. The labelled block replaces C's `goto error`.
#[no_mangle]
pub unsafe extern "C" fn save_game() {
    let mut success: word = 0;
    let mut custom_save_path = [0i8; POP_MAX_PATH as usize];
    let save_path = get_save_path(custom_save_path.as_mut_ptr(), custom_save_path.len());

    let handle = fopen(save_path, b"wb\0".as_ptr() as *const c_char);
    if !handle.is_null() {
        'err: {
            if fwrite(addr_of!(rem_min) as *const c_void, 1, 2, handle) != 2 {
                break 'err;
            }
            if fwrite(addr_of!(rem_tick) as *const c_void, 1, 2, handle) != 2 {
                break 'err;
            }
            if fwrite(addr_of!(current_level) as *const c_void, 1, 2, handle) != 2 {
                break 'err;
            }
            if fwrite(addr_of!(hitp_beg_lev) as *const c_void, 1, 2, handle) != 2 {
                break 'err;
            }
            success = 1;
        }
        if success == 0 {
            print!("save_game: fwrite: Can not write to: {}\n", cstr(save_path));
        }
        fclose(handle);
        if success == 0 {
            remove(save_path);
        }
    } else {
        perror(b"save_game: fopen\0".as_ptr() as *const c_char);
        print!("Tried to open for writing: {}\n", cstr(save_path));
    }

    if success != 0 {
        display_text_bottom(b"GAME SAVED\0".as_ptr() as *const c_char);
    } else {
        display_text_bottom(b"UNABLE TO SAVE GAME\0".as_ptr() as *const c_char);
    }
    text_time_remaining = 24;
}

// seg000:1E38
/// Read the long-term save back (Ctrl+L). Returns 1 on success, 0 otherwise.
///
/// Note the third field lands in `start_level`, not `current_level`: loading sets up a *restart*
/// at the saved level rather than restoring a position within it. `dont_reset_time` then keeps
/// the restored clock from being reset when that level initialises.
#[no_mangle]
pub unsafe extern "C" fn load_game() -> c_short {
    let mut success: word = 0;
    let mut custom_save_path = [0i8; POP_MAX_PATH as usize];
    let save_path = get_save_path(custom_save_path.as_mut_ptr(), custom_save_path.len());

    let handle = fopen(save_path, b"rb\0".as_ptr() as *const c_char);
    if !handle.is_null() {
        'err: {
            if fread(addr_of_mut!(rem_min) as *mut c_void, 1, 2, handle) != 2 {
                break 'err;
            }
            if fread(addr_of_mut!(rem_tick) as *mut c_void, 1, 2, handle) != 2 {
                break 'err;
            }
            if fread(addr_of_mut!(start_level) as *mut c_void, 1, 2, handle) != 2 {
                break 'err;
            }
            if fread(addr_of_mut!(hitp_beg_lev) as *mut c_void, 1, 2, handle) != 2 {
                break 'err;
            }
            // USE_COPYPROT
            if enable_copyprot != 0 && (*custom).copyprot_level > 0 {
                (*custom).copyprot_level = start_level as word;
            }
            success = 1;
            dont_reset_time = 1;
        }
        if success == 0 {
            print!("load_game: fread: Can not read from: {}\n", cstr(save_path));
        }
        fclose(handle);
    } else {
        perror(b"load_game: fopen\0".as_ptr() as *const c_char);
        print!("Tried to open for reading: {}\n", cstr(save_path));
    }
    success as c_short
}

// seg000:1F02
/// Tear the game down to a blank screen before a restart: stop sounds, drop level sprites, and
/// mark no level as loaded.
///
/// Sprite tables 0 and 1 (sword, flame/potion) are kept — they are loaded once by
/// [`init_game_main`] and are never reloaded. Sounds are deliberately not freed; modern machines
/// have the memory, and reloading them is slow.
#[no_mangle]
pub unsafe extern "C" fn clear_screen_and_sounds() {
    stop_sounds();
    current_target_surface = rect_sthg(onscreen_surface_, addr_of!(screen_rect));

    is_cutscene = 0;
    is_ending_sequence = false;
    peels_count = 0;
    for index in 2..10 {
        if !chtab_addrs[index].is_null() {
            free_chtab(chtab_addrs[index]);
            chtab_addrs[index] = null_mut();
        }
    }
    current_level = -1i32 as word;
}

// seg000:1F7B
/// Pick the sound hardware: `stdsnd` on the command line keeps PC speaker, otherwise digital
/// samples plus MIDI music.
#[no_mangle]
pub unsafe extern "C" fn parse_cmdline_sound() {
    if !cp(b"stdsnd\0").is_null() {
        // Use PC Speaker sounds and music.
    } else {
        // Use digi (wave) sounds and MIDI music.
        sound_flags |= soundflags_sfDigi as byte;
        sound_flags |= soundflags_sfMidi as byte;
        sound_mode = sound_modes_smSblast as byte;
    }
}

// seg000:226D
/// Free the optional (level-specific and cutscene) sounds. A stub: sounds are never freed.
#[no_mangle]
pub unsafe extern "C" fn free_optional_sounds() {
    // stub
}

/// Free every loaded sound buffer. Only used at shutdown.
#[no_mangle]
pub unsafe extern "C" fn free_all_sounds() {
    for slot in sound_pointers.iter_mut() {
        free_sound(*slot);
        *slot = null_mut();
    }
}

/// Load every sound, preferring a mod's own audio and falling back to SDLPoP's.
///
/// The two-pass form exists so a mod need only ship the sounds it actually replaces: the first
/// pass reads only mod files, the second only the base game's, and since neither pass overwrites
/// an already-loaded id the result is mod-first with base-game fallback.
#[no_mangle]
pub unsafe extern "C" fn load_all_sounds() {
    if use_custom_levelset == 0 || always_use_original_music != 0 {
        load_sounds(0, 43);
        load_opt_sounds(43, 56);
    } else {
        // First load any sounds included in the mod folder...
        skip_normal_data_files = true;
        load_sounds(0, 43);
        load_opt_sounds(43, 56);
        skip_normal_data_files = false;

        // ... then load any missing sounds from SDLPoP's own resources.
        skip_mod_data_files = true;
        load_sounds(0, 43);
        load_opt_sounds(43, 56);
        skip_mod_data_files = false;
    }
}

// seg000:22BB
/// Drop everything level-specific: the optional sounds and sprite tables 3 and up.
#[no_mangle]
pub unsafe extern "C" fn free_optsnd_chtab() {
    free_optional_sounds();
    free_all_chtabs_from(chtabs_id_chtab_3_princessinstory as c_int);
}

// seg000:22C8
/// Load the title-screen sprite tables and tint the text-frame background.
///
/// `bgcolor` picks between dark blue (`#100060`, used for the title and story frames) and dark
/// red (`#800000`). Both the 6-bit VGA palette entry and the SDL surface palette have to be set:
/// the former drives the game's own rendering, the latter the blit.
#[no_mangle]
pub unsafe extern "C" fn load_title_images(bgcolor: c_int) {
    let dh = open_dat(b"TITLE.DAT\0".as_ptr() as *const c_char, b'G' as c_int);
    chtab_title40 = load_sprites_from_file(40, 1 << 11, 1);
    chtab_title50 = load_sprites_from_file(50, 1 << 12, 1);
    close_dat(dh);
    if graphics_mode == grmodes_gmMcgaVga as byte {
        // background of text frame
        let color;
        if bgcolor != 0 {
            // RGB(4,0,18h) = #100060 = dark blue
            set_pal((find_first_pal_row(1 << 11) << 4) + 14, 0x04, 0x00, 0x18);
            color = SDL_Color { r: 0x10, g: 0x00, b: 0x60, a: 0xFF };
        } else {
            // RGB(20h,0,0) = #800000 = dark red
            set_pal((find_first_pal_row(1 << 11) << 4) + 14, 0x20, 0x00, 0x00);
            color = SDL_Color { r: 0x80, g: 0x00, b: 0x00, a: 0xFF };
        }
        if !chtab_title40.is_null() {
            let img = chtab_image(chtab_title40, 0);
            crate::platform::sdl::shared_renderer().set_palette(img, &color, 14, 1);
        }
    }
}

// seg000:23F4
/// Ask the copy-protection question, either as a dialog (`where_ == 0`) or on the bottom line.
///
/// Only ever shown on level 15, the potions level. The bottom-line form uses a total time of 1188
/// ticks, which [`draw_game_frame`] treats as "never expires".
#[no_mangle]
pub unsafe extern "C" fn show_copyprot(where_: c_int) {
    // USE_COPYPROT
    if current_level != 15 {
        return;
    }
    if where_ != 0 {
        if text_time_remaining != 0 || is_cutscene != 0 {
            return;
        }
        text_time_total = 1188;
        text_time_remaining = 1188;
        is_show_time = 0;
        let mut buf = [0i8; 140];
        cbuf_set(
            &mut buf,
            &format!(
                "WORD {} LINE {} PAGE {}",
                COPYPROT_WORD[copyprot_idx as usize],
                COPYPROT_LINE[copyprot_idx as usize],
                COPYPROT_PAGE[copyprot_idx as usize]
            ),
        );
        display_text_bottom(buf.as_ptr());
    } else {
        let mut buf = [0i8; 140];
        cbuf_set(
            &mut buf,
            &format!(
                "Drink potion matching the first letter of Word {} on Line {}\nof Page {} of the manual.",
                COPYPROT_WORD[copyprot_idx as usize],
                COPYPROT_LINE[copyprot_idx as usize],
                COPYPROT_PAGE[copyprot_idx as usize]
            ),
        );
        show_dialog(buf.as_ptr());
    }
}

// seg000:2489
/// Put "Loading. . . ." on screen while start-up proceeds.
#[no_mangle]
pub unsafe extern "C" fn show_loading() {
    show_text(
        addr_of!(screen_rect),
        halign_center,
        valign_middle,
        b"Loading. . . .\0".as_ptr() as *const c_char,
    );
    update_screen();
}

const TBL_QUOTE_0: &[u8] = b"\"(****/****) Incredibly realistic. . . The adventurer character actually looks human as he runs, jumps, climbs, and hangs from ledges.\"\n\n                                  Computer Entertainer\n\n\n\n\n\"A tremendous achievement. . . Mechner has crafted the smoothest animation ever seen in a game of this type.\n\n\"PRINCE OF PERSIA is the STAR WARS of its field.\"\n\n                                  Computer Gaming World\0";
const TBL_QUOTE_1: &[u8] = b"\"An unmitigated delight. . . comes as close to (perfection) as any arcade game has come in a long, long time. . . what makes this game so wonderful (am I gushing?) is that the little onscreen character does not move like a little onscreen character -- he moves like a person.\"\n\n                                      Nibble\0";

// seg000:249D
/// In `demo` mode, show one of the two magazine review quotes between attract-mode loops,
/// alternating each time.
#[no_mangle]
pub unsafe extern "C" fn show_quotes() {
    if demo_mode != 0 && need_quotes != 0 {
        draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
        let quote = if which_quote == 0 { TBL_QUOTE_0 } else { TBL_QUOTE_1 };
        show_text(addr_of!(screen_rect), halign_left, valign_middle, quote.as_ptr() as *const c_char);
        which_quote = (which_quote == 0) as word;
        start_timer(timerids_timer_0 as c_int, 0x384);
    }
    need_quotes = 0;
}

const SPLASH_TEXT_1: &[u8] = b"SDLPoP 1.24 RC\0";
const SPLASH_TEXT_2: &[u8] = b"In-game, Esc opens a settings/quicksave menu.\n\nTo record replays, press Ctrl+Tab in-game.\nTo view replays, press Tab on the title screen.\n\nEdit SDLPoP.ini to customize SDLPoP.\nMods also work with SDLPoP.\n\nFor more information, read README.md.\nQuestions? Visit https://forum.princed.org\n\nPress any key to continue...\0";

/// Show SDLPoP's own "press any key" info screen before the title sequence.
///
/// Not part of the original game. Skipped entirely when a starting level was given on the command
/// line. Gamepad buttons are folded into a synthetic Shift press so the screen can be dismissed
/// without a keyboard, and a Ctrl-modified key (or F9/Tab) is forwarded through
/// `last_key_scancode` so Ctrl+L, quickload or replay can be triggered straight from here.
#[no_mangle]
pub unsafe extern "C" fn show_splash() {
    if enable_info_screen == 0 || start_level >= 0 {
        return;
    }
    current_target_surface = onscreen_surface_;
    draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
    show_text_with_color(
        addr_of!(splash_text_1_rect),
        halign_center,
        valign_middle,
        SPLASH_TEXT_1.as_ptr() as *const c_char,
        colorids_color_15_brightwhite as c_int,
    );
    show_text_with_color(
        addr_of!(splash_text_2_rect),
        halign_center,
        valign_top,
        SPLASH_TEXT_2.as_ptr() as *const c_char,
        colorids_color_7_lightgray as c_int,
    );

    // USE_TEXT
    let mut key;
    loop {
        idle();
        key = key_test_quit();

        let joy_input = joy_button_states[..JOYINPUT_NUM as usize]
            .iter()
            .any(|state| state & KEYSTATE_HELD_I != 0);
        if joy_input {
            joy_button_states[..JOYINPUT_NUM as usize].fill(0);
            // Close the splash screen using the gamepad.
            key_states[SDL_SCANCODE_LSHIFT as usize] |= KEYSTATE_HELD as byte;
        }

        delay_ticks(1);

        if key != 0
            || (key_states[SDL_SCANCODE_LSHIFT as usize] as c_int & KEYSTATE_HELD_I != 0
                || key_states[SDL_SCANCODE_RSHIFT as usize] as c_int & KEYSTATE_HELD_I != 0)
        {
            break;
        }
    }

    if (key & WITH_CTRL) != 0
        || (enable_quicksave != 0 && key == SDL_SCANCODE_F9)
        || (enable_replay != 0 && key == SDL_SCANCODE_TAB)
    {
        last_key_scancode = key;
    }
    // Don't immediately start the game if Shift was what dismissed this screen.
    key_states[SDL_SCANCODE_LSHIFT as usize] &= !(KEYSTATE_HELD as byte);
    key_states[SDL_SCANCODE_RSHIFT as usize] &= !(KEYSTATE_HELD as byte);
}

/// Resolve where a save/config file should be written, creating directories as needed.
///
/// Preference order: `$SDLPOP_SAVE_PATH`, then `$HOME/.SDLPoP`. When a custom levelset is loaded,
/// a per-levelset subdirectory is used so mods do not share saves. If neither environment
/// variable is set, the bare `file_name` is returned (writing next to the executable), or the
/// mod's own data directory when playing a mod.
///
/// Truncation is fatal — `snprintf_check` calls `quit(2)`.
#[no_mangle]
pub unsafe extern "C" fn get_writable_file_path(
    custom_path_buffer: *mut c_char,
    max_len: usize,
    file_name: *const c_char,
) -> *const c_char {
    let mut save_path = [0i8; POP_MAX_PATH as usize];
    let custom_save_path = getenv(b"SDLPOP_SAVE_PATH\0".as_ptr() as *const c_char);
    let home_path = getenv(b"HOME\0".as_ptr() as *const c_char);
    if !custom_save_path.is_null() && *custom_save_path != 0 {
        snprintf_check_ptr(save_path.as_mut_ptr(), max_len, cstr(custom_save_path));
    } else if !home_path.is_null() && *home_path != 0 {
        snprintf_check_ptr(
            save_path.as_mut_ptr(),
            max_len,
            &format!("{}/.{}", cstr(home_path), cstr(POP_DIR_NAME.as_ptr() as *const c_char)),
        );
    }

    if save_path[0] != 0 {
        mkdir(save_path.as_ptr(), 0o700);
        if use_custom_levelset != 0 {
            snprintf_check_ptr(
                custom_path_buffer,
                max_len,
                &format!("{}/{}", cstr(save_path.as_ptr()), cstr(levelset_name.as_ptr())),
            );
            mkdir(custom_path_buffer, 0o700);
            snprintf_check_ptr(
                custom_path_buffer,
                max_len,
                &format!(
                    "{}/{}/{}",
                    cstr(save_path.as_ptr()),
                    cstr(levelset_name.as_ptr()),
                    cstr(file_name)
                ),
            );
        } else {
            snprintf_check_ptr(
                custom_path_buffer,
                max_len,
                &format!("{}/{}", cstr(save_path.as_ptr()), cstr(file_name)),
            );
        }
        return custom_path_buffer;
    }

    if use_custom_levelset == 0 {
        return file_name;
    }
    snprintf_check_ptr(
        custom_path_buffer,
        max_len,
        &format!("{}/{}", cstr(mod_data_path.as_ptr()), cstr(file_name)),
    );
    custom_path_buffer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ScratchDir, ENV_LOCK};

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // Not reachable via the replay/trace harness: quicksave/quickload (F6/F9) aren't
    // recordable (enum replay_special_moves only encodes MOVE_RESTART_LEVEL and
    // MOVE_EFFECT_END, seg000.c has no special_move for save/load keys). quick_save()
    // has no SDL calls, so it's safe to call directly; quick_load() is NOT safe to call
    // directly here (it calls stop_sounds/draw_rect/update_screen/delay_ticks before
    // restore_room_after_quick_load(), which needs a real video subsystem). Instead this
    // test drives the read side directly via quick_process(process_load), the same
    // function quick_load() itself calls after its version check passes.
    #[test]
    fn quicksave_writes_and_reads_back_state() {
        let _guard = ENV_LOCK.lock().unwrap();
        let scratch = ScratchDir::new("quicksave");
        setup();
        unsafe {
            std::env::set_var("SDLPOP_SAVE_PATH", &scratch.0);

            current_level = 7;
            hitp_curr = 2;
            Kid.x = 111;
            assert_eq!(quick_save(), 1, "quick_save should succeed");

            // Corrupt in-memory state so the read-back actually proves something.
            current_level = 0;
            hitp_curr = 0;
            Kid.x = 0;

            let mut path_buf = [0i8; POP_MAX_PATH as usize];
            let path = get_quick_path(path_buf.as_mut_ptr(), path_buf.len());
            quick_fp = fopen(path, b"rb\0".as_ptr() as *const c_char);
            assert!(!quick_fp.is_null(), "quicksave file should exist");

            process_load(quick_control.as_mut_ptr() as *mut c_void, quick_control.len());
            assert_eq!(
                strcmp(quick_control.as_ptr(), quick_version.as_ptr()),
                0,
                "version header should round-trip"
            );
            assert_eq!(quick_process(process_load), 1, "quick_process(process_load) should succeed");
            fclose(quick_fp);
            quick_fp = null_mut();

            assert_eq!(current_level, 7);
            assert_eq!(hitp_curr, 2);
            assert_eq!(Kid.x, 111);

            std::env::remove_var("SDLPOP_SAVE_PATH");
        }
    }

    // Regression test for the version-mismatch rejection branch in quick_load()
    // (`if strcmp(quick_control, quick_version) != 0 { ...; return 0; }`), exercised
    // without touching the SDL-drawing tail of the real quick_load() function.
    #[test]
    fn quicksave_version_mismatch_is_detected() {
        let _guard = ENV_LOCK.lock().unwrap();
        let scratch = ScratchDir::new("quicksave-version-mismatch");
        setup();
        unsafe {
            std::env::set_var("SDLPOP_SAVE_PATH", &scratch.0);

            let mut path_buf = [0i8; POP_MAX_PATH as usize];
            let path = get_quick_path(path_buf.as_mut_ptr(), path_buf.len());
            let handle = fopen(path, b"wb\0".as_ptr() as *const c_char);
            assert!(!handle.is_null());
            let garbage: [c_char; 9] = [b'X' as c_char; 9];
            fwrite(garbage.as_ptr() as *const c_void, 1, garbage.len(), handle);
            fclose(handle);

            quick_fp = fopen(path, b"rb\0".as_ptr() as *const c_char);
            assert!(!quick_fp.is_null());
            process_load(quick_control.as_mut_ptr() as *mut c_void, quick_control.len());
            assert_ne!(
                strcmp(quick_control.as_ptr(), quick_version.as_ptr()),
                0,
                "garbage header must not match the real version string"
            );
            fclose(quick_fp);
            quick_fp = null_mut();

            std::env::remove_var("SDLPOP_SAVE_PATH");
        }
    }

    // Cross-compatibility check: does the Rust port correctly read a QUICKSAVE.SAV
    // actually written by the C oracle's quick_save()? The two tests above only prove
    // Rust's own read/write agree with each other, not that Rust matches the real file
    // format C produces. This fixture is committed at
    // doc/test-fixtures/quicksave_c_oracle.sav, generated by scripts/gen_quicksave_fixture.sh
    // (src/test_quicksave_fixture.c calls the real quick_save() directly -- not part of the
    // pinned oracle build in src/CMakeLists.txt/src/Makefile, just a throwaway executable
    // linking the same sources). Fixture was generated with current_level=7, hitp_curr=2,
    // Kid.x=111 (see test_quicksave_fixture.c).
    #[test]
    fn quicksave_reads_fixture_written_by_c_oracle() {
        let _guard = ENV_LOCK.lock().unwrap();
        let scratch = ScratchDir::new("quicksave-c-oracle");
        setup();
        unsafe {
            let fixture = concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/doc/test-fixtures/quicksave_c_oracle.sav"
            );
            std::fs::copy(fixture, scratch.0.join("QUICKSAVE.SAV"))
                .expect("copy committed C-oracle fixture into scratch dir");
            std::env::set_var("SDLPOP_SAVE_PATH", &scratch.0);

            current_level = 0;
            hitp_curr = 0;
            Kid.x = 0;

            let mut path_buf = [0i8; POP_MAX_PATH as usize];
            let path = get_quick_path(path_buf.as_mut_ptr(), path_buf.len());
            quick_fp = fopen(path, b"rb\0".as_ptr() as *const c_char);
            assert!(!quick_fp.is_null(), "fixture file should exist");

            process_load(quick_control.as_mut_ptr() as *mut c_void, quick_control.len());
            assert_eq!(
                strcmp(quick_control.as_ptr(), quick_version.as_ptr()),
                0,
                "fixture's version header should match what Rust expects"
            );
            assert_eq!(quick_process(process_load), 1, "quick_process(process_load) should succeed");
            fclose(quick_fp);
            quick_fp = null_mut();

            assert_eq!(current_level, 7, "current_level from the C-written fixture");
            assert_eq!(hitp_curr, 2, "hitp_curr from the C-written fixture");
            assert_eq!(Kid.x, 111, "Kid.x from the C-written fixture");

            std::env::remove_var("SDLPOP_SAVE_PATH");
        }
    }

    // Not reachable via replay either (same special_move limitation as quicksave), and
    // save_game() itself isn't safe to call directly (it ends with display_text_bottom(),
    // which calls draw_rect/show_text). load_game() has no SDL calls at all, so this tests
    // its read path directly against a hand-constructed fixture file: 4 sequential u16
    // fields (rem_min, rem_tick, start_level, hitp_beg_lev), matching save_game()'s write
    // order exactly (seg000.c:2146-2157). This is the side most vulnerable to a
    // word-order/size mistake in a future refactor.
    #[test]
    fn load_game_reads_fixture_file_in_expected_field_order() {
        let _guard = ENV_LOCK.lock().unwrap();
        let scratch = ScratchDir::new("long-term-save");
        setup();
        unsafe {
            std::env::set_var("SDLPOP_SAVE_PATH", &scratch.0);

            let mut path_buf = [0i8; POP_MAX_PATH as usize];
            let path = get_save_path(path_buf.as_mut_ptr(), path_buf.len());

            let mut fixture = Vec::new();
            fixture.extend_from_slice(&5i16.to_le_bytes());   // rem_min
            fixture.extend_from_slice(&300u16.to_le_bytes()); // rem_tick
            fixture.extend_from_slice(&9i16.to_le_bytes());   // start_level
            fixture.extend_from_slice(&3u16.to_le_bytes());   // hitp_beg_lev
            std::fs::write(cstr(path), &fixture).expect("write fixture save file");

            rem_min = 0;
            rem_tick = 0;
            start_level = 0;
            hitp_beg_lev = 0;

            assert_eq!(load_game(), 1, "load_game should succeed");
            assert_eq!(rem_min, 5);
            assert_eq!(rem_tick, 300);
            assert_eq!(start_level, 9);
            assert_eq!(hitp_beg_lev, 3);

            std::env::remove_var("SDLPOP_SAVE_PATH");
        }
    }
}
