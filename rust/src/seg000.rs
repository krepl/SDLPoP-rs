// Main loop, game initialization, input, sound/sprite loading, HP display — ported from seg000.c.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_long, c_short, c_void};
use core::ptr::{addr_of, addr_of_mut, null, null_mut};
use super::*;

// ---------------------------------------------------------------------------
// SDL / libc externs (not in bindings.rs)
// ---------------------------------------------------------------------------
enum FILE {}

extern "C" {
    fn setjmp(env: *mut u8) -> c_int;
    fn longjmp(env: *mut u8, val: c_int) -> !;

    fn SDL_GetKeyboardState(numkeys: *mut c_int) -> *const u8;
    fn SDL_AddTimer(
        interval: u32,
        callback: Option<unsafe extern "C" fn(u32, *mut c_void) -> u32>,
        param: *mut c_void,
    ) -> c_int;
    fn SDL_Delay(ms: u32);
    fn SDL_GetPerformanceCounter() -> u64;
    fn SDL_GetVersion(ver: *mut SDL_version);
    fn SDL_SetPaletteColors(
        palette: *mut SDL_Palette,
        colors: *const SDL_Color,
        firstcolor: c_int,
        ncolors: c_int,
    ) -> c_int;

    fn fopen(path: *const c_char, mode: *const c_char) -> *mut FILE;
    fn fread(ptr: *mut c_void, size: usize, count: usize, stream: *mut FILE) -> usize;
    fn fwrite(ptr: *const c_void, size: usize, count: usize, stream: *mut FILE) -> usize;
    fn fclose(stream: *mut FILE) -> c_int;
    fn fseek(stream: *mut FILE, offset: c_long, whence: c_int) -> c_int;
    fn remove(path: *const c_char) -> c_int;
    fn perror(s: *const c_char);
    fn getenv(name: *const c_char) -> *mut c_char;
    fn mkdir(path: *const c_char, mode: u32) -> c_int;
    fn strcmp(a: *const c_char, b: *const c_char) -> c_int;
    fn strrchr(s: *const c_char, c: c_int) -> *mut c_char;
    fn strcasecmp(a: *const c_char, b: *const c_char) -> c_int;
    fn atoi(s: *const c_char) -> c_int;

    // declared `extern int audio_speed;` in seg000.c (USE_FAST_FORWARD)
    static mut audio_speed: c_int;
}

#[repr(C)]
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
#[inline]
unsafe fn cp(s: &[u8]) -> *const c_char {
    check_param(s.as_ptr() as *const c_char)
}

unsafe fn cstr<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    std::ffi::CStr::from_ptr(p).to_str().unwrap_or("")
}

// Copy a Rust string into a C buffer (truncating like snprintf, no quit).
unsafe fn cbuf_set(buf: &mut [c_char], s: &str) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(buf.len().saturating_sub(1));
    for i in 0..n {
        buf[i] = bytes[i] as c_char;
    }
    buf[n] = 0;
}

// snprintf_check: quit(2) on truncation.
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

unsafe fn copyprot_letter_at(i: usize) -> c_char {
    *addr_of!(copyprot_letter).cast::<c_char>().add(i)
}

// sound_interruptible is an extern const incomplete array; bindgen emits [byte; 0].
unsafe fn sound_interruptible_at(idx: usize) -> byte {
    *addr_of!(sound_interruptible).cast::<byte>().add(idx)
}
unsafe fn sound_interruptible_set(idx: usize, val: byte) {
    *addr_of_mut!(sound_interruptible).cast::<byte>().add(idx) = val;
}

unsafe fn chtab_image(chtab: *mut chtab_type, idx: usize) -> *mut image_type {
    addr_of!((*chtab).images).cast::<*mut image_type>().add(idx).read()
}
unsafe fn chtab_image_set(chtab: *mut chtab_type, idx: usize, img: *mut image_type) {
    *addr_of_mut!((*chtab).images).cast::<*mut image_type>().add(idx) = img;
}

// ---------------------------------------------------------------------------
// seg000:0000
// ---------------------------------------------------------------------------
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

    if cheats_enabled != 0 || recording != 0 {
        let mut i = 15;
        while i >= 0 {
            cbuf_set(&mut sprintf_temp, &format!("{}", i));
            if !check_param(sprintf_temp.as_ptr()).is_null() {
                start_level = i as c_short;
                break;
            }
            i -= 1;
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
#[no_mangle]
pub unsafe extern "C" fn start_game() {
    // USE_COPYPROT
    let mut which_entry: word;
    let mut entry_used = [0u16; 40];
    let mut letts_used = [0u8; 26];

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
    release_title_images();
    free_optsnd_chtab();
    // USE_COPYPROT
    copyprot_plac = prandom(13);
    for v in entry_used.iter_mut() {
        *v = 0;
    }
    for v in letts_used.iter_mut() {
        *v = 0;
    }
    for pos in 0u16..14 {
        loop {
            if pos == copyprot_plac {
                which_entry = prandom(39);
                copyprot_idx = which_entry;
            } else {
                which_entry = prandom(39);
            }
            let lett = (copyprot_letter_at(which_entry as usize) as i32 - b'A' as i32) as usize;
            if !(entry_used[which_entry as usize] != 0 || letts_used[lett] != 0) {
                break;
            }
        }
        cplevel_entr[pos as usize] = which_entry;
        entry_used[which_entry as usize] = 1;
        let lett = (copyprot_letter_at(which_entry as usize) as i32 - b'A' as i32) as usize;
        letts_used[lett] = 1;
    }

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
type ProcessFn = unsafe extern "C" fn(*mut c_void, usize) -> c_int;

unsafe extern "C" fn process_save(data: *mut c_void, data_size: usize) -> c_int {
    (fwrite(data, data_size, 1, quick_fp) == 1) as c_int
}

unsafe extern "C" fn process_load(data: *mut c_void, data_size: usize) -> c_int {
    (fread(data, data_size, 1, quick_fp) == 1) as c_int
}

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
    // USE_DEBUG_CHEATS: don't load the level if Shift is held while pressing F9.
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

unsafe fn get_quick_path(custom_path_buffer: *mut c_char, max_len: usize) -> *const c_char {
    get_writable_file_path(custom_path_buffer, max_len, QUICK_FILE.as_ptr() as *const c_char)
}

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

#[no_mangle]
pub unsafe extern "C" fn restore_room_after_quick_load() {
    let temp1 = curr_guard_color as c_int;
    let temp2 = next_level as c_int;
    reset_level_unused_fields(false);
    load_lev_spr(current_level as c_int);
    curr_guard_color = temp1 as word;
    next_level = temp2 as word;

    if (*fixes).fix_quicksave_during_feather == 0 && is_feather_fall > 0 {
        is_feather_fall = 0;
        stop_sounds();
    }

    different_room = 1;
    next_room = Kid.room as word;
    drawn_room = Kid.room as word;
    load_room_links();
    draw_game_frame();

    hitp_delta = 1;
    guardhp_delta = 1;
    if Guard.room as word != drawn_room {
        Guard.direction = directions_dir_56_none as sbyte;
        guardhp_curr = 0;
    }

    draw_hp();
    loadkid_and_opp();
    text_time_total = 0;
    text_time_remaining = 0;
    exit_room_timer = 0;
}

#[no_mangle]
pub unsafe extern "C" fn quick_load() -> c_int {
    let mut ok: c_int = 0;
    let mut custom_quick_path = [0i8; POP_MAX_PATH as usize];
    let path = get_quick_path(custom_quick_path.as_mut_ptr(), custom_quick_path.len());
    quick_fp = fopen(path, b"rb\0".as_ptr() as *const c_char);
    if !quick_fp.is_null() {
        process_load(quick_control.as_mut_ptr() as *mut c_void, quick_control.len());
        if strcmp(quick_control.as_ptr(), quick_version.as_ptr()) != 0 {
            fclose(quick_fp);
            quick_fp = null_mut();
            return 0;
        }

        stop_sounds();
        draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
        update_screen();
        delay_ticks(5);

        let old_rem_min = rem_min;
        let old_rem_tick = rem_tick;

        ok = quick_process(process_load);
        fclose(quick_fp);
        quick_fp = null_mut();

        restore_room_after_quick_load();
        update_screen();

        // USE_QUICKLOAD_PENALTY
        if enable_quicksave_penalty != 0
            && (current_level < (*custom).victory_stops_time_level
                || (current_level == (*custom).victory_stops_time_level && leveldoor_open < 2))
        {
            let ticks_elapsed = 720 * (rem_min as c_int - old_rem_min as c_int)
                + (rem_tick as c_int - old_rem_tick as c_int);
            if ticks_elapsed > 0 && ticks_elapsed < 720 {
                rem_min = old_rem_min;
                rem_tick = old_rem_tick;
            } else {
                if rem_min == 6 {
                    rem_tick = 719;
                }
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

unsafe extern "C" fn temp_shift_release_callback(_interval: u32, _param: *mut c_void) -> u32 {
    let state = SDL_GetKeyboardState(null_mut());
    if *state.add(SDL_SCANCODE_LSHIFT as usize) != 0 {
        key_states[SDL_SCANCODE_LSHIFT as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as byte;
    }
    if *state.add(SDL_SCANCODE_RSHIFT as usize) != 0 {
        key_states[SDL_SCANCODE_RSHIFT as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as byte;
    }
    0
}

// seg000:04CD
#[no_mangle]
pub unsafe extern "C" fn process_key() -> c_int {
    let mut sprintf_temp = [0i8; 80];
    let mut answer_text: *const c_char = null();
    let mut need_show_text: word = 0;
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
            } else if key == (SDL_SCANCODE_TAB | WITH_CTRL) {
                start_level = (*custom).first_level as c_short;
                start_recording();
            } else if key == (SDL_SCANCODE_L | WITH_CTRL) {
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

    // ----- main switch -----
    if key == SDL_SCANCODE_ESCAPE || key == (SDL_SCANCODE_ESCAPE | WITH_SHIFT) {
        is_paused = 1;
        // USE_MENU
        if enable_pause_menu != 0 && is_cutscene == 0 && !is_ending_sequence {
            is_menu_shown = 1;
        }
    } else if key == SDL_SCANCODE_BACKSPACE {
        // USE_MENU
        if is_cutscene == 0 && !is_ending_sequence {
            is_paused = 1;
            is_menu_shown = 1;
        }
    } else if key == SDL_SCANCODE_SPACE {
        is_show_time = 1;
    } else if key == (SDL_SCANCODE_A | WITH_CTRL) {
        if current_level != 15 {
            stop_sounds();
            is_restart_level = 1;
        }
    } else if key == (SDL_SCANCODE_G | WITH_CTRL) {
        if current_level >= (*custom).saving_allowed_first_level
            && current_level <= (*custom).saving_allowed_last_level
        {
            save_game();
        }
    } else if key == (SDL_SCANCODE_J | WITH_CTRL) {
        if (sound_flags & soundflags_sfDigi as byte) != 0 && sound_mode == sound_modes_smTandy as byte
        {
            answer_text = b"JOYSTICK UNAVAILABLE\0".as_ptr() as *const c_char;
        } else if set_joy_mode() != 0 {
            answer_text = b"JOYSTICK MODE\0".as_ptr() as *const c_char;
        } else {
            answer_text = b"JOYSTICK NOT FOUND\0".as_ptr() as *const c_char;
        }
        need_show_text = 1;
    } else if key == (SDL_SCANCODE_K | WITH_CTRL) {
        answer_text = b"KEYBOARD MODE\0".as_ptr() as *const c_char;
        is_joyst_mode = 0;
        is_keyboard_mode = 1;
        need_show_text = 1;
    } else if key == (SDL_SCANCODE_R | WITH_CTRL) {
        start_level = -1;
        // USE_MENU
        if is_menu_shown != 0 {
            menu_was_closed();
        }
        start_game();
    } else if key == (SDL_SCANCODE_S | WITH_CTRL) {
        turn_sound_on_off(((is_sound_on == 0) as byte) * 15);
        answer_text = b"SOUND OFF\0".as_ptr() as *const c_char;
        if is_sound_on != 0 {
            answer_text = b"SOUND ON\0".as_ptr() as *const c_char;
        }
        need_show_text = 1;
    } else if key == (SDL_SCANCODE_V | WITH_CTRL) {
        cbuf_set(
            &mut sprintf_temp,
            &format!("SDLPoP v{}\n", cstr(SDLPOP_VERSION.as_ptr() as *const c_char)),
        );
        answer_text = sprintf_temp.as_ptr();
        need_show_text = 1;
    } else if key == (SDL_SCANCODE_C | WITH_CTRL) {
        let verc = SDL_version { major: 2, minor: 30, patch: 0 };
        let mut verl = SDL_version { major: 0, minor: 0, patch: 0 };
        SDL_GetVersion(&mut verl);
        cbuf_set(
            &mut sprintf_temp,
            &format!(
                "SDL COMP v{}.{}.{} LINK v{}.{}.{}",
                verc.major, verc.minor, verc.patch, verl.major, verl.minor, verl.patch
            ),
        );
        answer_text = sprintf_temp.as_ptr();
        need_show_text = 1;
    } else if key == (SDL_SCANCODE_L | WITH_SHIFT) {
        if current_level < (*custom).shift_L_allowed_until_level || cheats_enabled != 0 {
            let delay: u32 = 250;
            key_states[SDL_SCANCODE_LSHIFT as usize] = 0;
            key_states[SDL_SCANCODE_RSHIFT as usize] = 0;
            let timer = SDL_AddTimer(delay, Some(temp_shift_release_callback), null_mut());
            if timer == 0 {
                sdlperror(b"process_key: SDL_AddTimer\0".as_ptr() as *const c_char);
                quit(1);
            }
            if current_level == 14 {
                next_level = 1;
            } else if current_level == 15 && cheats_enabled != 0 {
                // USE_COPYPROT
                if enable_copyprot != 0 {
                    next_level = (*custom).copyprot_level;
                    (*custom).copyprot_level = -1i32 as word;
                }
            } else {
                next_level = current_level.wrapping_add(1);
                if cheats_enabled == 0 && rem_min > (*custom).shift_L_reduced_minutes as c_short {
                    rem_min = (*custom).shift_L_reduced_minutes as c_short;
                    rem_tick = (*custom).shift_L_reduced_ticks;
                }
            }
            stop_sounds();
        }
    } else if key == SDL_SCANCODE_F6 || key == (SDL_SCANCODE_F6 | WITH_SHIFT) {
        // USE_QUICKSAVE
        if Kid.alive < 0 {
            need_quick_save = 1;
        }
    } else if key == SDL_SCANCODE_F9 || key == (SDL_SCANCODE_F9 | WITH_SHIFT) {
        // USE_QUICKSAVE
        need_quick_load = 1;
    } else if key == (SDL_SCANCODE_TAB | WITH_CTRL) || key == (SDL_SCANCODE_TAB | WITH_CTRL | WITH_SHIFT)
    {
        // USE_REPLAY
        if recording != 0 {
            stop_recording();
        } else {
            start_recording();
        }
    }

    if cheats_enabled != 0 {
        if key == SDL_SCANCODE_C {
            cbuf_set(
                &mut sprintf_temp,
                &format!(
                    "S{} L{} R{} A{} B{}",
                    drawn_room, room_L, room_R, room_A, room_B
                ),
            );
            answer_text = sprintf_temp.as_ptr();
            need_show_text = 1;
        } else if key == (SDL_SCANCODE_C | WITH_SHIFT) {
            cbuf_set(
                &mut sprintf_temp,
                &format!("AL{} AR{} BL{} BR{}", room_AL, room_AR, room_BL, room_BR),
            );
            answer_text = sprintf_temp.as_ptr();
            need_show_text = 1;
        } else if key == SDL_SCANCODE_KP_MINUS {
            if rem_min > 1 {
                rem_min -= 1;
            }
            // ALLOW_INFINITE_TIME
            else if rem_min < -1 {
                rem_min += 1;
            } else if rem_min == -1 {
                rem_tick = 720;
            }
            text_time_total = 0;
            text_time_remaining = 0;
            is_show_time = 1;
        } else if key == SDL_SCANCODE_KP_PLUS {
            // ALLOW_INFINITE_TIME
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
        } else if key == SDL_SCANCODE_R {
            if Kid.alive > 0 {
                resurrect_time = 20;
                Kid.alive = -1;
                erase_bottom_text(1);
            }
        } else if key == SDL_SCANCODE_K {
            if Guard.charid != charids_charid_4_skeleton as byte {
                guardhp_delta = -(guardhp_curr as c_short);
                Guard.alive = 0;
            }
        } else if key == (SDL_SCANCODE_I | WITH_SHIFT) {
            toggle_upside();
        } else if key == (SDL_SCANCODE_W | WITH_SHIFT) {
            feather_fall();
        } else if key == SDL_SCANCODE_H {
            draw_guard_hp(0, 10);
            next_room = room_L;
        } else if key == SDL_SCANCODE_J {
            draw_guard_hp(0, 10);
            next_room = room_R;
        } else if key == SDL_SCANCODE_U {
            draw_guard_hp(0, 10);
            next_room = room_A;
        } else if key == SDL_SCANCODE_N {
            draw_guard_hp(0, 10);
            next_room = room_B;
        } else if key == (SDL_SCANCODE_B | WITH_CTRL) {
            draw_guard_hp(0, 10);
            next_room = Kid.room as word;
        } else if key == (SDL_SCANCODE_B | WITH_SHIFT) {
            is_blind_mode = (is_blind_mode == 0) as word;
            if is_blind_mode != 0 {
                draw_rect(addr_of!(rect_top), colorids_color_0_black as c_int);
            } else {
                need_full_redraw = 1;
            }
        } else if key == (SDL_SCANCODE_S | WITH_SHIFT) {
            if hitp_curr != hitp_max {
                play_sound(soundids_sound_33_small_potion as c_int);
                hitp_delta = 1;
                flash_color = 4;
                flash_time = 2;
            }
        } else if key == (SDL_SCANCODE_T | WITH_SHIFT) {
            play_sound(soundids_sound_30_big_potion as c_int);
            flash_color = 4;
            flash_time = 4;
            add_life();
        } else if key == SDL_SCANCODE_T {
            // USE_DEBUG_CHEATS
            is_timer_displayed = 1 - is_timer_displayed;
        } else if key == SDL_SCANCODE_F {
            // USE_DEBUG_CHEATS
            if (*fixes).fix_quicksave_during_feather != 0 {
                is_feather_timer_displayed = 1 - is_feather_timer_displayed;
            } else {
                is_feather_timer_displayed = 0;
            }
        }
    }

    if need_show_text != 0 {
        display_text_bottom(answer_text);
        text_time_total = 24;
        text_time_remaining = 24;
    }
    1
}

// seg000:08EB
#[no_mangle]
pub unsafe extern "C" fn play_frame() {
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
        if Kid.room as word == (*custom).demo_end_room as word {
            draw_rect(addr_of!(screen_rect), colorids_color_0_black as c_int);
            start_level = -1;
            need_quotes = 1;
            start_game();
        }
    } else if current_level == (*custom).falling_exit_level {
        if roomleave_result == -2 {
            Kid.y = -1i32 as byte;
            stop_sounds();
            next_level = next_level.wrapping_add(1);
        }
    } else if (*custom).tbl_seamless_exit[current_level as usize] >= 0 {
        if Kid.room as c_short == (*custom).tbl_seamless_exit[current_level as usize] as c_short {
            next_level = next_level.wrapping_add(1);
            stop_sounds();
            seamless = 1;
        }
    }
    show_time();
    if current_level < 13 && rem_min == 0 {
        expired();
    }
}

// seg000:09B6
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
#[no_mangle]
pub unsafe extern "C" fn anim_tile_modif() {
    for tilepos in 0u16..30 {
        let t = get_curr_tile(tilepos as c_short);
        if t == tiles_tiles_10_potion as c_short {
            start_anim_potion(drawn_room as c_short, tilepos as c_short);
        } else if t == tiles_tiles_19_torch as c_short || t == tiles_tiles_30_torch_with_debris as c_short
        {
            start_anim_torch(drawn_room as c_short, tilepos as c_short);
        } else if t == tiles_tiles_22_sword as c_short {
            start_anim_sword(drawn_room as c_short, tilepos as c_short);
        }
    }

    for row in 0..=2 {
        let t = get_tile(room_L as c_int, 9, row);
        if t == tiles_tiles_19_torch as c_int || t == tiles_tiles_30_torch_with_debris as c_int {
            start_anim_torch(room_L as c_short, (row * 10 + 9) as c_short);
        }
    }
}

// seg000:0B72
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

    let mut current = first as c_short;
    while current <= last as c_short {
        if !sound_pointers[current as usize].is_null() {
            current += 1;
            continue;
        }
        sound_pointers[current as usize] = load_sound(current as c_int);
        current += 1;
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
    let mut current = first as c_short;
    while current <= last as c_short {
        if !sound_pointers[current as usize].is_null() {
            current += 1;
            continue;
        }
        sound_pointers[current as usize] = load_sound(current as c_int);
        current += 1;
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

#[no_mangle]
pub unsafe extern "C" fn reset_level_unused_fields(loading_clean_level: bool) {
    core::ptr::write_bytes(addr_of_mut!(level.roomxs) as *mut u8, 0, core::mem::size_of_val(&level.roomxs));
    core::ptr::write_bytes(addr_of_mut!(level.roomys) as *mut u8, 0, core::mem::size_of_val(&level.roomys));
    core::ptr::write_bytes(addr_of_mut!(level.fill_1) as *mut u8, 0, core::mem::size_of_val(&level.fill_1));
    core::ptr::write_bytes(addr_of_mut!(level.fill_2) as *mut u8, 0, core::mem::size_of_val(&level.fill_2));
    core::ptr::write_bytes(addr_of_mut!(level.fill_3) as *mut u8, 0, core::mem::size_of_val(&level.fill_3));

    if level.used_rooms as u32 > ROOMCOUNT {
        level.used_rooms = ROOMCOUNT as byte;
    }

    for i in 0..level.used_rooms as usize {
        level.guards_skill[i] &= 0x0F;
    }

    if loading_clean_level {
        for i in 0..level.used_rooms as usize {
            level.guards_color[i] &= 0x0F;
        }
    }
}

// seg000:0EA8
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

unsafe fn get_joystick_state(raw_x: c_int, raw_y: c_int, axis_state: *mut c_int) {
    // deliberate overflow to match C (cast both sides to unsigned)
    let dist_squared = raw_x.wrapping_mul(raw_x).wrapping_add(raw_y.wrapping_mul(raw_y));
    if (dist_squared as u32) < ((joystick_threshold * joystick_threshold) as u32) {
        *axis_state.add(0) = 0;
        *axis_state.add(1) = 0;
    } else {
        let angle = (raw_y as f64).atan2(raw_x as f64);

        if angle.abs() < (60.0 * DEGREES_TO_RADIANS) {
            *axis_state.add(0) = 1;
        } else if angle.abs() > (120.0 * DEGREES_TO_RADIANS) {
            *axis_state.add(0) = -1;
        } else if !(angle < 0.0 && Kid.action == actions_actions_1_run_jump as byte) {
            *axis_state.add(0) = 0;
        }

        if angle < (-30.0 * DEGREES_TO_RADIANS) && angle > (-150.0 * DEGREES_TO_RADIANS) {
            *axis_state.add(1) = -1;
        } else if angle > (35.0 * DEGREES_TO_RADIANS) && angle < (145.0 * DEGREES_TO_RADIANS) {
            *axis_state.add(1) = 1;
        } else if !((Kid.frame >= frameids_frame_108_fall_land_2 as byte
            && Kid.frame <= frameids_frame_112_stand_up_from_crouch_3 as byte)
            && angle > 0.0)
        {
            *axis_state.add(1) = 0;
        }
    }
}

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
#[no_mangle]
pub unsafe extern "C" fn draw_kid_hp(curr_hp: c_short, max_hp: c_short) {
    let mut drawn_hp_index = curr_hp;
    while drawn_hp_index < max_hp {
        method_6_blit_img_to_scr(
            get_image(chtabs_id_chtab_2_kid as c_short, 217),
            drawn_hp_index as c_int * 7,
            194,
            blitters_blitters_0_no_transp as c_int,
        );
        drawn_hp_index += 1;
    }
    let mut drawn_hp_index = 0;
    while drawn_hp_index < curr_hp {
        method_6_blit_img_to_scr(
            get_image(chtabs_id_chtab_2_kid as c_short, 216),
            drawn_hp_index as c_int * 7,
            194,
            blitters_blitters_0_no_transp as c_int,
        );
        drawn_hp_index += 1;
    }
}

// seg000:1159
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
        let mut drawn_hp_index = curr_hp;
        while drawn_hp_index < max_hp {
            method_6_blit_img_to_scr(
                chtab_image(chtab, 0),
                314 - drawn_hp_index as c_int * 7,
                194,
                blitters_blitters_9_black as c_int,
            );
            drawn_hp_index += 1;
        }
        let mut drawn_hp_index = 0;
        while drawn_hp_index < curr_hp {
            method_6_blit_img_to_scr(
                chtab_image(chtab, 0),
                314 - drawn_hp_index as c_int * 7,
                194,
                blitters_blitters_0_no_transp as c_int,
            );
            drawn_hp_index += 1;
        }
    }
}

// seg000:11EC
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
#[no_mangle]
pub unsafe extern "C" fn set_health_life() {
    hitp_delta = (hitp_max as c_int - hitp_curr as c_int) as c_short;
}

// seg000:120B
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
#[no_mangle]
pub unsafe extern "C" fn do_delta_hp() {
    if Opp.charid == charids_charid_1_shadow as byte && current_level == 12 && guardhp_delta != 0 {
        hitp_delta = guardhp_delta;
    }
    hitp_curr =
        ((hitp_curr as c_int + hitp_delta as c_int).max(0)).min(hitp_max as c_int) as word;
    guardhp_curr =
        ((guardhp_curr as c_int + guardhp_delta as c_int).max(0)).min(guardhp_max as c_int) as word;
}

#[no_mangle]
pub unsafe extern "C" fn fix_sound_priorities() {
    sound_interruptible_set(soundids_sound_49_spikes as usize, 1);
    sound_prio_table[soundids_sound_48_spiked as usize] = 0x15;
    sound_prio_table[soundids_sound_10_sword_vs_sword as usize] = 0x0D;
}

// seg000:12C5
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
#[no_mangle]
pub unsafe extern "C" fn check_sword_vs_sword() {
    if Kid.frame == 167 || Guard.frame == 167 {
        play_sound(soundids_sound_10_sword_vs_sword as c_int);
    }
}

// seg000:136A
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
unsafe fn load_one_optgraf(
    chtab_ptr: *mut chtab_type,
    pal_ptr: *mut dat_pal_type,
    base_id: c_int,
    min_index: c_int,
    max_index: c_int,
) {
    let mut index = min_index as c_short;
    while index <= max_index as c_short {
        let image = load_image(base_id + index as c_int + 1, pal_ptr);
        if !image.is_null() {
            chtab_image_set(chtab_ptr, index as usize, image);
        }
        index += 1;
    }
}

// seg000:13FC
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
    if is_ending_sequence && is_paused != 0 {
        is_paused = 0;
    }
    if is_paused != 0 {
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

    for i in 0..SDL_NUM_SCANCODES {
        key_states[i] &= !(KEYSTATE_HELD_NEW as byte);
    }
    for i in 0..(JOYINPUT_NUM as usize) {
        joy_button_states[i] &= !KEYSTATE_HELD_NEW_I;
    }
    for i in 0..(JOY_AXIS_NUM as usize) {
        joy_axis_max[i] = joy_axis[i];
    }

    (key != 0 || control_shift != 0) as c_int
}

// seg000:1500
#[no_mangle]
pub unsafe extern "C" fn read_keyb_control() {
    let key_state: c_int;
    if (*fixes).fix_register_quick_input != 0 {
        key_state = KEYSTATE_HELD_I | KEYSTATE_HELD_NEW_I;
    } else {
        key_state = KEYSTATE_HELD_I;
    }

    let ks = |sc: c_int| (key_states[sc as usize] as c_int) & key_state;
    let ksk = |k: c_int| (key_states[k as usize] as c_int) & key_state;

    if ks(SDL_SCANCODE_UP) != 0
        || ks(SDL_SCANCODE_HOME) != 0
        || ks(SDL_SCANCODE_PAGEUP) != 0
        || ks(SDL_SCANCODE_KP_8) != 0
        || ks(SDL_SCANCODE_KP_7) != 0
        || ks(SDL_SCANCODE_KP_9) != 0
        || ksk(key_up) != 0
        || ksk(key_jump_left) != 0
        || ksk(key_jump_right) != 0
    {
        control_y = CONTROL_HELD_UP as sbyte;
    } else if ks(SDL_SCANCODE_CLEAR) != 0
        || ks(SDL_SCANCODE_DOWN) != 0
        || ks(SDL_SCANCODE_KP_5) != 0
        || ks(SDL_SCANCODE_KP_2) != 0
        || ksk(key_down) != 0
    {
        control_y = CONTROL_HELD_DOWN as sbyte;
    }
    if ks(SDL_SCANCODE_LEFT) != 0
        || ks(SDL_SCANCODE_HOME) != 0
        || ks(SDL_SCANCODE_KP_4) != 0
        || ks(SDL_SCANCODE_KP_7) != 0
        || ksk(key_left) != 0
        || ksk(key_jump_left) != 0
    {
        control_x = CONTROL_HELD_LEFT as sbyte;
    } else if ks(SDL_SCANCODE_RIGHT) != 0
        || ks(SDL_SCANCODE_PAGEUP) != 0
        || ks(SDL_SCANCODE_KP_6) != 0
        || ks(SDL_SCANCODE_KP_9) != 0
        || ksk(key_right) != 0
        || ksk(key_jump_right) != 0
    {
        control_x = CONTROL_HELD_RIGHT as sbyte;
    }

    if ks(SDL_SCANCODE_LSHIFT) != 0 || ks(SDL_SCANCODE_RSHIFT) != 0 || ksk(key_action) != 0 {
        control_shift = CONTROL_HELD as sbyte;
    } else {
        control_shift = CONTROL_RELEASED as sbyte;
    }

    // USE_DEBUG_CHEATS
    if cheats_enabled != 0 && debug_cheats_enabled != 0 {
        if ks(SDL_SCANCODE_RIGHTBRACKET) != 0 {
            Char.x = Char.x.wrapping_add(1);
        } else if ks(SDL_SCANCODE_LEFTBRACKET) != 0 {
            Char.x = Char.x.wrapping_sub(1);
        }
    }
}

// We need a version of showmessage() which can detect modifier keys as well.
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

    let saved_font = textstate.ptr_font;
    textstate.ptr_font = addr_of_mut!(hc_font);

    let new_key = showmessage_any_key(message.as_ptr(), 1, key_test_quit as *mut c_void);

    textstate.ptr_font = saved_font;

    if new_key == SDL_SCANCODE_ESCAPE {
        return;
    }
    *key = new_key;
}

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
#[no_mangle]
pub unsafe extern "C" fn toggle_upside() {
    upside_down = !upside_down;
    need_redraw_because_flipped = 1;
}

// seg000:15F8
#[no_mangle]
pub unsafe extern "C" fn feather_fall() {
    if (*fixes).fix_quicksave_during_feather != 0 {
        is_feather_fall =
            (FEATHER_FALL_LENGTH * get_ticks_per_sec(timerids_timer_1 as c_int)) as word;
    } else {
        is_feather_fall = 1;
    }
    flash_color = 2;
    flash_time = 3;
    stop_sounds();
    play_sound(soundids_sound_39_low_weight as c_int);
}

// seg000:1618
#[no_mangle]
pub unsafe extern "C" fn parse_grmode() -> c_int {
    set_gr_mode(grmodes_gmMcgaVga as byte);
    grmodes_gmMcgaVga as c_int
}

// seg000:172C
#[no_mangle]
pub unsafe extern "C" fn gen_palace_wall_colors() {
    let old_randseed = random_seed;
    random_seed = drawn_room as dword;
    prandom(1);
    for row in 0i16..3 {
        for subrow in 0i16..4 {
            let color_base: word = if subrow % 2 != 0 { 0x61 } else { 0x66 };
            let mut prev_color: word = 0xFFFF;
            for column in 0i16..=10 {
                let mut color: word;
                loop {
                    color = color_base.wrapping_add(prandom(3));
                    if color != prev_color {
                        break;
                    }
                }
                palace_wall_colors[(44 * row + 11 * subrow + column) as usize] = color as byte;
                prev_color = color;
            }
        }
    }
    random_seed = old_randseed;
}

// seg000:17E6
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
    method_1_blit_rect(
        onscreen_surface_,
        offscreen_surface,
        addr_of!(rect_titles),
        addr_of!(rect_titles),
        blitters_blitters_0_no_transp as c_int,
    );
    draw_full_image(full_image_id_TITLE_MAIN);
    do_wait(timerids_timer_0 as c_int);

    start_timer(timerids_timer_0 as c_int, 0x41);
    method_1_blit_rect(
        onscreen_surface_,
        offscreen_surface,
        addr_of!(rect_titles),
        addr_of!(rect_titles),
        blitters_blitters_0_no_transp as c_int,
    );
    draw_full_image(full_image_id_TITLE_MAIN);
    draw_full_image(full_image_id_TITLE_GAME);
    do_wait(timerids_timer_0 as c_int);

    start_timer(timerids_timer_0 as c_int, 0x10E);
    method_1_blit_rect(
        onscreen_surface_,
        offscreen_surface,
        addr_of!(rect_titles),
        addr_of!(rect_titles),
        blitters_blitters_0_no_transp as c_int,
    );
    draw_full_image(full_image_id_TITLE_MAIN);
    do_wait(timerids_timer_0 as c_int);

    start_timer(timerids_timer_0 as c_int, 0xEB);
    method_1_blit_rect(
        onscreen_surface_,
        offscreen_surface,
        addr_of!(rect_titles),
        addr_of!(rect_titles),
        blitters_blitters_0_no_transp as c_int,
    );
    draw_full_image(full_image_id_TITLE_MAIN);
    draw_full_image(full_image_id_TITLE_POP);
    draw_full_image(full_image_id_TITLE_MECHNER);
    do_wait(timerids_timer_0 as c_int);

    method_1_blit_rect(
        onscreen_surface_,
        offscreen_surface,
        addr_of!(rect_titles),
        addr_of!(rect_titles),
        blitters_blitters_0_no_transp as c_int,
    );
    draw_full_image(full_image_id_STORY_FRAME);
    draw_full_image(full_image_id_STORY_ABSENCE);
    current_target_surface = onscreen_surface_;
    while check_sound_playing() != 0 {
        idle();
        do_paused();
        delay_ticks(1);
    }
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
    while check_sound_playing() != 0 {
        idle();
        do_paused();
        delay_ticks(1);
    }
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
    while check_sound_playing() != 0 {
        idle();
        do_paused();
        delay_ticks(1);
    }
    fade_out_2(0x1800);
    free_surface(offscreen_surface);
    offscreen_surface = null_mut();
    release_title_images();
    init_game(0);
}

// seg000:1BB3
#[no_mangle]
pub unsafe extern "C" fn transition_ltr() {
    let mut rect = rect_type { top: 0, bottom: 200, left: 0, right: 2 };
    let mut transition_fps = 120;
    // USE_FAST_FORWARD
    transition_fps *= audio_speed;
    let counters_per_frame = perf_frequency / transition_fps as u64;
    last_transition_counter = SDL_GetPerformanceCounter();
    let mut overshoot = 0;
    let mut position = 0i16;
    while position < 320 {
        method_1_blit_rect(onscreen_surface_, offscreen_surface, &rect, &rect, 0);
        rect.left += 2;
        rect.right += 2;
        if overshoot > 0 && overshoot < 10 {
            overshoot -= 1;
            position += 2;
            continue;
        }
        idle();
        do_paused();
        loop {
            let current_counter = SDL_GetPerformanceCounter();
            let frametimes_elapsed = ((current_counter / counters_per_frame)
                - (last_transition_counter / counters_per_frame)) as c_int;
            if frametimes_elapsed > 0 {
                overshoot = frametimes_elapsed - 1;
                last_transition_counter = current_counter;
                break;
            } else {
                SDL_Delay(1);
            }
        }
        position += 2;
    }
}

// seg000:1C0F
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
#[no_mangle]
pub unsafe extern "C" fn draw_full_image(id: full_image_id) {
    let idx = id as usize;
    let mut mask: *mut image_type = null_mut();

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
        blit = get_text_color(15, colorids_color_15_brightwhite as c_int, 0x800);
        method_3_blit_mono(decoded_image, xpos, ypos, blitters_blitters_0_no_transp as c_int, blit as byte);
    } else if blit == blitters_blitters_10h_transp as c_int {
        if graphics_mode == grmodes_gmCga as byte || graphics_mode == grmodes_gmHgaHerc as byte {
            // ...
        } else {
            mask = decoded_image;
        }
        draw_image_transp(decoded_image, mask, xpos, ypos);
        if graphics_mode == grmodes_gmCga as byte || graphics_mode == grmodes_gmHgaHerc as byte {
            // free(mask) — not applicable in VGA
        }
    } else if blit == blitters_blitters_0_no_transp as c_int {
        method_6_blit_img_to_scr(decoded_image, xpos, ypos, blit);
    } else {
        method_3_blit_mono(decoded_image, xpos, ypos, blitters_blitters_0_no_transp as c_int, blit as byte);
    }
}

// seg000:1D2C
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

unsafe fn get_save_path(custom_path_buffer: *mut c_char, max_len: usize) -> *const c_char {
    get_writable_file_path(custom_path_buffer, max_len, SAVE_FILE.as_ptr() as *const c_char)
}

// seg000:1D45
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
#[no_mangle]
pub unsafe extern "C" fn parse_cmdline_sound() {
    if !cp(b"stdsnd\0").is_null() {
        // Use PC Speaker sounds and music.
    } else {
        sound_flags |= soundflags_sfDigi as byte;
        sound_flags |= soundflags_sfMidi as byte;
        sound_mode = sound_modes_smSblast as byte;
    }
}

// seg000:226D
#[no_mangle]
pub unsafe extern "C" fn free_optional_sounds() {
    // stub
}

#[no_mangle]
pub unsafe extern "C" fn free_all_sounds() {
    for i in 0..58 {
        free_sound(sound_pointers[i]);
        sound_pointers[i] = null_mut();
    }
}

#[no_mangle]
pub unsafe extern "C" fn load_all_sounds() {
    if use_custom_levelset == 0 || always_use_original_music != 0 {
        load_sounds(0, 43);
        load_opt_sounds(43, 56);
    } else {
        {
            skip_normal_data_files = true;
            load_sounds(0, 43);
            load_opt_sounds(43, 56);
            skip_normal_data_files = false;
        }
        skip_mod_data_files = true;
        load_sounds(0, 43);
        load_opt_sounds(43, 56);
        skip_mod_data_files = false;
    }
}

// seg000:22BB
#[no_mangle]
pub unsafe extern "C" fn free_optsnd_chtab() {
    free_optional_sounds();
    free_all_chtabs_from(chtabs_id_chtab_3_princessinstory as c_int);
}

// seg000:22C8
#[no_mangle]
pub unsafe extern "C" fn load_title_images(bgcolor: c_int) {
    let dh = open_dat(b"TITLE.DAT\0".as_ptr() as *const c_char, b'G' as c_int);
    chtab_title40 = load_sprites_from_file(40, 1 << 11, 1);
    chtab_title50 = load_sprites_from_file(50, 1 << 12, 1);
    close_dat(dh);
    if graphics_mode == grmodes_gmMcgaVga as byte {
        let mut color = SDL_Color { r: 0, g: 0, b: 0, a: 0 };
        if bgcolor != 0 {
            set_pal((find_first_pal_row(1 << 11) << 4) + 14, 0x04, 0x00, 0x18);
            color.r = 0x10;
            color.g = 0x00;
            color.b = 0x60;
            color.a = 0xFF;
        } else {
            set_pal((find_first_pal_row(1 << 11) << 4) + 14, 0x20, 0x00, 0x00);
            color.r = 0x80;
            color.g = 0x00;
            color.b = 0x00;
            color.a = 0xFF;
        }
        if !chtab_title40.is_null() {
            let img = chtab_image(chtab_title40, 0);
            SDL_SetPaletteColors((*(*img).format).palette, &color, 14, 1);
        }
    }
}

// seg000:23F4
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

        let mut joy_input = false;
        for i in 0..(JOYINPUT_NUM as usize) {
            if joy_button_states[i] & KEYSTATE_HELD_I != 0 {
                joy_input = true;
                break;
            }
        }
        if joy_input {
            for i in 0..(JOYINPUT_NUM as usize) {
                joy_button_states[i] = 0;
            }
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
    key_states[SDL_SCANCODE_LSHIFT as usize] &= !(KEYSTATE_HELD as byte);
    key_states[SDL_SCANCODE_RSHIFT as usize] &= !(KEYSTATE_HELD as byte);
}

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
