// Replay recording and playback (.P1R files) — ported from replay.c.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_short, c_void};
use core::ptr::{addr_of, addr_of_mut, null_mut};
use core::mem::size_of;
use super::*;

// ============================================================================
// libc / SDL declarations (those not already provided by lib.rs via super::*)
// fopen/fread/fwrite/fclose/fseek/perror come from lib.rs.
// ============================================================================
extern "C" {
    fn fgetc(stream: *mut FILE) -> c_int;
    fn fputc(c: c_int, stream: *mut FILE) -> c_int;
    fn fputs(s: *const c_char, stream: *mut FILE) -> c_int;
    fn rewind(stream: *mut FILE);
    fn snprintf(buf: *mut c_char, size: usize, fmt: *const c_char, ...) -> c_int;
    fn printf(fmt: *const c_char, ...) -> c_int;
    fn fprintf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    fn putchar(c: c_int) -> c_int;
    fn time(t: *mut i64) -> i64;
    fn difftime(time1: i64, time0: i64) -> f64;
    fn malloc(size: usize) -> *mut c_void;
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    fn memcpy(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    fn strncmp(a: *const c_char, b: *const c_char, n: usize) -> c_int;
    fn strncpy(dst: *mut c_char, src: *const c_char, n: usize) -> *mut c_char;
    fn strnlen(s: *const c_char, maxlen: usize) -> usize;
    fn strlen(s: *const c_char) -> usize;
    fn qsort(
        base: *mut c_void,
        nmemb: usize,
        size: usize,
        compar: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> c_int>,
    );
    fn exit(code: c_int) -> !;
    fn chdir(path: *const c_char) -> c_int;
    fn mkdir(path: *const c_char, mode: u32) -> c_int;
    fn stat(path: *const c_char, buf: *mut stat_t) -> c_int;
    static mut stderr: *mut FILE;

    // SDL (not emitted by bindgen, which only processes src/)
    fn SDL_RWFromMem(mem: *mut c_void, size: c_int) -> *mut SDL_RWops;
    fn SDL_RWtell(context: *mut SDL_RWops) -> i64;
    fn SDL_RWclose(context: *mut SDL_RWops) -> c_int;
    fn SDL_ShowSimpleMessageBox(
        flags: u32,
        title: *const c_char,
        message: *const c_char,
        window: *mut SDL_Window,
    ) -> c_int;
}

const SEEK_SET: c_int = 0;
const SEEK_CUR: c_int = 1;
const SDL_MESSAGEBOX_ERROR: u32 = 0x00000010;

// glibc x86-64 struct stat (144 bytes). We only read st_ctim.
#[repr(C)]
struct stat_t {
    st_dev: u64,
    st_ino: u64,
    st_nlink: u64,
    st_mode: u32,
    st_uid: u32,
    st_gid: u32,
    __pad0: u32,
    st_rdev: u64,
    st_size: i64,
    st_blksize: i64,
    st_blocks: i64,
    st_atim: [u64; 2],
    st_mtim: [u64; 2],
    st_ctim: [u64; 2],
    __glibc_reserved: [i64; 3],
}

// SDL scancodes (not emitted by bindgen)
const SDL_SCANCODE_A: c_int = 4;
const SDL_SCANCODE_B: c_int = 5;
const SDL_SCANCODE_C: c_int = 6;
const SDL_SCANCODE_F: c_int = 9;
const SDL_SCANCODE_I: c_int = 12;
const SDL_SCANCODE_R: c_int = 21;
const SDL_SCANCODE_S: c_int = 22;
const SDL_SCANCODE_T: c_int = 23;
const SDL_SCANCODE_V: c_int = 25;
const SDL_SCANCODE_ESCAPE: c_int = 41;
const SDL_SCANCODE_BACKSPACE: c_int = 42;
const SDL_SCANCODE_TAB: c_int = 43;
const SDL_SCANCODE_SPACE: c_int = 44;

const WITH_CTRL: c_int = key_modifiers_WITH_CTRL as c_int;
const WITH_SHIFT: c_int = key_modifiers_WITH_SHIFT as c_int;

// C string literal helper.
macro_rules! cs {
    ($s:expr) => {
        concat!($s, "\0").as_ptr() as *const c_char
    };
}

// snprintf_check (from common.h): bail out with quit(2) on truncation.
macro_rules! snprintf_check {
    ($dst:expr, $size:expr, $($arg:tt)*) => {{
        let __len = snprintf($dst, $size, $($arg)*);
        if __len < 0 || __len >= ($size) as c_int {
            fprintf(stderr, cs!("%s: buffer truncation detected!\n"), cs!("replay"));
            quit(2);
        }
    }};
}

#[inline]
fn ptr_size_of<T>(_p: *const T) -> usize {
    size_of::<T>()
}

// ============================================================================
// Constants (from #defines in replay.c)
// ============================================================================
const REPLAY_FORMAT_CURR_VERSION: c_int = 102;
const REPLAY_FORMAT_MIN_VERSION: byte = 101;
const REPLAY_FORMAT_DEPRECATION_NUMBER: c_int = 2;

const MAX_REPLAY_DURATION: usize = 345600; // 8 hours: 720 * 60 * 8 ticks
const MAX_SAVESTATE_SIZE: usize = 4096;

const REPLAY_HEADER_ERROR_MESSAGE_MAX: usize = 512;

// ============================================================================
// File-local globals (defined in replay.c, not exported via headers)
// ============================================================================
static replay_magic_number: [c_char; 3] = [b'P' as c_char, b'1' as c_char, b'R' as c_char];
static replay_format_class: word = 0; // unique number associated with this SDLPoP fork

static mut moves: [byte; MAX_REPLAY_DURATION] = [0; MAX_REPLAY_DURATION];

static mut replay_levelset_name: [c_char; POP_MAX_PATH as usize] = [0; POP_MAX_PATH as usize];
static mut stored_levelset_name: [c_char; POP_MAX_PATH as usize] = [0; POP_MAX_PATH as usize];

static mut replay_fp: *mut FILE = null_mut();
static mut replay_file_open: byte = 0;
static mut current_replay_number: c_int = 0;
static mut next_replay_number: c_int = 0;

static mut savestate_buffer: *mut byte = null_mut();
static mut savestate_offset: dword = 0;
static mut savestate_size: dword = 0;

// "SDLPoP v" SDLPOP_VERSION
unsafe fn implementation_name() -> *const c_char {
    static mut BUF: [c_char; 64] = [0; 64];
    static mut INIT: bool = false;
    if !INIT {
        snprintf(
            BUF.as_mut_ptr(),
            64,
            cs!("SDLPoP v%s"),
            SDLPOP_VERSION.as_ptr() as *const c_char,
        );
        INIT = true;
    }
    BUF.as_ptr()
}

// fixes_options_replay (file-local). Stored as raw bytes to avoid needing a
// const initializer for the bindgen struct; accessed via a typed pointer.
const FIXES_SZ: usize = size_of::<fixes_options_type>();
static mut fixes_options_replay_storage: [u8; FIXES_SZ] = [0u8; FIXES_SZ];
#[inline]
unsafe fn fixes_options_replay() -> *mut fixes_options_type {
    addr_of_mut!(fixes_options_replay_storage) as *mut fixes_options_type
}

// ============================================================================
// header / info structs (file-local typedefs in replay.c)
// ============================================================================
#[repr(C)]
struct replay_header_type {
    uses_custom_levelset: byte,
    levelset_name: [c_char; POP_MAX_PATH as usize],
    implementation_name: [c_char; POP_MAX_PATH as usize],
}

#[repr(C)]
struct replay_info_type {
    filename: [c_char; POP_MAX_PATH as usize],
    creation_time: i64, // time_t
    header: replay_header_type,
}

// ============================================================================
// fread_check macro (matches the C #define): on short read, optionally set the
// error message and return 0 from the enclosing function.
// ============================================================================
macro_rules! fread_check {
    ($dst:expr, $size:expr, $elements:expr, $fp:expr, $err:expr, $name:literal) => {{
        let __count: usize = fread($dst as *mut c_void, $size as usize, $elements as usize, $fp);
        if __count != ($elements as usize) {
            if !($err).is_null() {
                snprintf_check!(
                    $err,
                    REPLAY_HEADER_ERROR_MESSAGE_MAX,
                    cs!(concat!($name, " missing -- not a valid replay file!"))
                );
            }
            return 0; // incompatible file
        }
    }};
}

// seg: read_replay_header
unsafe fn read_replay_header(
    header: *mut replay_header_type,
    fp: *mut FILE,
    error_message: *mut c_char,
) -> c_int {
    // Explicitly go to the beginning, because the current filepos might be nonzero.
    fseek(fp, 0, SEEK_SET);
    // read the magic number
    let mut magic = [0 as c_char; 3];
    fread_check!(magic.as_mut_ptr(), 3, 1, fp, error_message, "magic");
    if strncmp(magic.as_ptr(), replay_magic_number.as_ptr(), 3) != 0 {
        if !error_message.is_null() {
            snprintf_check!(
                error_message,
                REPLAY_HEADER_ERROR_MESSAGE_MAX,
                cs!("not a valid replay file!")
            );
        }
        return 0; // incompatible, magic number not correct!
    }
    // read the unique number associated with this SDLPoP implementation / fork (for normal SDLPoP: 0)
    let mut class_: word = 0;
    fread_check!(
        addr_of_mut!(class_),
        size_of::<word>(),
        1,
        fp,
        error_message,
        "&class"
    );
    // read the format version number
    let version_number: byte = fgetc(fp) as byte;
    // read the format deprecation number
    let deprecation_number: byte = fgetc(fp) as byte;

    // creation time (seconds since 1970) is embedded in the format, but not used in SDLPoP right now
    fseek(fp, size_of::<i64>() as _, SEEK_CUR);

    // read the levelset_name
    let mut len_read: byte = fgetc(fp) as byte;
    (*header).uses_custom_levelset = (len_read != 0) as byte;
    fread_check!(
        addr_of_mut!((*header).levelset_name) as *mut c_char,
        size_of::<c_char>(),
        len_read,
        fp,
        error_message,
        "header->levelset_name"
    );
    (*header).levelset_name[len_read as usize] = 0;

    // read the implementation_name
    len_read = fgetc(fp) as byte;
    fread_check!(
        addr_of_mut!((*header).implementation_name) as *mut c_char,
        size_of::<c_char>(),
        len_read,
        fp,
        error_message,
        "header->implementation_name"
    );
    (*header).implementation_name[len_read as usize] = 0;

    if class_ != replay_format_class {
        // incompatible, replay format is associated with a different implementation of SDLPoP
        if !error_message.is_null() {
            snprintf_check!(
                error_message,
                REPLAY_HEADER_ERROR_MESSAGE_MAX,
                cs!("replay created with \"%s\"...\nIncompatible replay class identifier! (expected %d, found %d)"),
                addr_of!((*header).implementation_name) as *const c_char,
                replay_format_class as c_int,
                class_ as c_int
            );
        }
        return 0;
    }

    if version_number < REPLAY_FORMAT_MIN_VERSION {
        // incompatible, replay format is too old
        if !error_message.is_null() {
            snprintf_check!(
                error_message,
                REPLAY_HEADER_ERROR_MESSAGE_MAX,
                cs!("replay created with \"%s\"...\nReplay format version too old! (minimum %d, found %d)"),
                addr_of!((*header).implementation_name) as *const c_char,
                REPLAY_FORMAT_MIN_VERSION as c_int,
                version_number as c_int
            );
        }
        return 0;
    }

    if deprecation_number as c_int > REPLAY_FORMAT_DEPRECATION_NUMBER {
        // incompatible, replay format is too new
        if !error_message.is_null() {
            snprintf_check!(
                error_message,
                REPLAY_HEADER_ERROR_MESSAGE_MAX,
                cs!("replay created with \"%s\"...\nReplay deprecation number too new! (max %d, found %d)"),
                addr_of!((*header).implementation_name) as *const c_char,
                REPLAY_FORMAT_DEPRECATION_NUMBER,
                deprecation_number as c_int
            );
        }
        return 0;
    }

    g_deprecation_number = deprecation_number as c_int;

    if is_validate_mode != 0 {
        static mut is_replay_info_printed: byte = 0;
        if is_replay_info_printed == 0 {
            printf(
                cs!("\nReplay created with %s.\n"),
                addr_of!((*header).implementation_name) as *const c_char,
            );
            printf(
                cs!("Format: class identifier %d, version number %d, deprecation number %d.\n"),
                class_ as c_int,
                version_number as c_int,
                deprecation_number as c_int,
            );
            if (*header).levelset_name[0] == 0 {
                printf(cs!("Levelset: original Prince of Persia.\n"));
            } else {
                printf(
                    cs!("Levelset: %s.\n"),
                    addr_of!((*header).levelset_name) as *const c_char,
                );
            }
            putchar(b'\n' as c_int);
            is_replay_info_printed = 1; // do this only once
        }
    }

    1
}

static mut num_replay_files: c_int = 0; // number of listed replays
static mut max_replay_files: c_int = 128; // initially, may grow if there are > 128 replay files found
static mut replay_list: *mut replay_info_type = null_mut();

// Compare function -- for qsort() in list_replay_files() below
unsafe extern "C" fn compare_replay_creation_time(a: *const c_void, b: *const c_void) -> c_int {
    difftime(
        (*(b as *const replay_info_type)).creation_time,
        (*(a as *const replay_info_type)).creation_time,
    ) as c_int
}

unsafe fn list_replay_files() {
    if replay_list.is_null() {
        // need to allocate enough memory to store info about all replay files in the directory
        replay_list =
            malloc(max_replay_files as usize * size_of::<replay_info_type>()) as *mut replay_info_type;
    }

    num_replay_files = 0;

    let directory_listing = create_directory_listing_and_find_first_file(
        addr_of!(replays_folder) as *const c_char,
        cs!("p1r"),
    );
    if directory_listing.is_null() {
        return;
    }

    loop {
        num_replay_files += 1;
        if num_replay_files > max_replay_files {
            // too many files, expand the memory available for replay_list
            max_replay_files += 128;
            let new_replay_list = realloc(
                replay_list as *mut c_void,
                max_replay_files as usize * size_of::<replay_info_type>(),
            );
            if new_replay_list.is_null() {
                printf(cs!("list_replay_files: realloc failed!"));
                quit(1);
            }
            replay_list = new_replay_list as *mut replay_info_type;
        }
        let replay_info = replay_list.add((num_replay_files - 1) as usize); // current replay file
        memset(replay_info as *mut c_void, 0, size_of::<replay_info_type>());
        // store the filename of the replay
        snprintf_check!(
            addr_of_mut!((*replay_info).filename) as *mut c_char,
            POP_MAX_PATH as usize,
            cs!("%s/%s"),
            addr_of!(replays_folder) as *const c_char,
            get_current_filename_from_directory_listing(directory_listing)
        );

        // get the creation time
        let mut st: stat_t = core::mem::zeroed();
        if stat(addr_of!((*replay_info).filename) as *const c_char, addr_of_mut!(st)) == 0 {
            (*replay_info).creation_time = st.st_ctim[0] as i64;
        }
        // read and store the levelset name associated with the replay
        let fp = fopen(addr_of!((*replay_info).filename) as *const c_char, cs!("rb"));
        let mut ok = 0;
        if !fp.is_null() {
            ok = read_replay_header(addr_of_mut!((*replay_info).header), fp, null_mut());
            fclose(fp);
        }
        if ok == 0 {
            num_replay_files -= 1; // scrap the file if it is not compatible
        }

        if !find_next_file(directory_listing) {
            break;
        }
    }

    close_directory_listing(directory_listing);

    if num_replay_files > 1 {
        // sort listed replays by their creation date
        qsort(
            replay_list as *mut c_void,
            num_replay_files as usize,
            size_of::<replay_info_type>(),
            Some(compare_replay_creation_time),
        );
    }
}

unsafe fn open_replay_file(filename: *const c_char) -> byte {
    printf(cs!("Opening replay file: %s\n"), filename);
    if replay_file_open != 0 {
        fclose(replay_fp);
    }
    replay_fp = fopen(filename, cs!("rb"));
    if !replay_fp.is_null() {
        replay_file_open = 1;
        1
    } else {
        replay_file_open = 0;
        0
    }
}

unsafe fn change_working_dir_to_sdlpop_root() {
    let exe_path = *g_argv.add(0);
    // strip away everything after the last slash or backslash in the path
    let mut len = strlen(exe_path) as c_int;
    while len > 0 {
        if *exe_path.add(len as usize) == b'\\' as c_char
            || *exe_path.add(len as usize) == b'/' as c_char
        {
            break;
        }
        len -= 1;
    }
    if len > 0 {
        let mut exe_dir = [0 as c_char; POP_MAX_PATH as usize];
        strncpy(exe_dir.as_mut_ptr(), exe_path, len as usize);
        exe_dir[len as usize] = 0;

        let result = chdir(exe_dir.as_ptr());
        if result != 0 {
            perror(cs!("Can't change into SDLPoP directory"));
        }
    }
}

// Called in pop_main(); check whether a replay file is being opened directly.
#[no_mangle]
pub unsafe extern "C" fn start_with_replay_file(filename: *const c_char) {
    if open_replay_file(filename) != 0 {
        change_working_dir_to_sdlpop_root();
        current_replay_number = -1; // don't cycle when pressing Tab
        // We should read the header in advance so we know the levelset name
        let mut header: replay_header_type = core::mem::zeroed();
        let mut header_error_message = [0 as c_char; REPLAY_HEADER_ERROR_MESSAGE_MAX];
        let ok =
            read_replay_header(addr_of_mut!(header), replay_fp, header_error_message.as_mut_ptr());
        if ok == 0 {
            let mut error_message = [0 as c_char; REPLAY_HEADER_ERROR_MESSAGE_MAX];
            snprintf_check!(
                error_message.as_mut_ptr(),
                REPLAY_HEADER_ERROR_MESSAGE_MAX,
                cs!("Error opening replay file: %s\n"),
                header_error_message.as_ptr()
            );
            fprintf(stderr, cs!("%s"), error_message.as_ptr());
            fclose(replay_fp);
            replay_fp = null_mut();
            replay_file_open = 0;

            if is_validate_mode != 0 {
                // Validating replays is cmd-line only, so, no sense continuing from here.
                exit(0);
            }

            SDL_ShowSimpleMessageBox(
                SDL_MESSAGEBOX_ERROR,
                cs!("SDLPoP"),
                error_message.as_ptr(),
                null_mut(),
            );
            return;
        }
        if header.uses_custom_levelset != 0 {
            // use the replay's levelset
            strncpy(
                addr_of_mut!(replay_levelset_name) as *mut c_char,
                header.levelset_name.as_ptr(),
                size_of::<[c_char; POP_MAX_PATH as usize]>(),
            );
        }
        rewind(replay_fp); // replay file is still open and will be read in load_replay() later
        need_start_replay = 1; // will later call start_replay(), from init_record_replay()
    }
}

// ============================================================================
// options I/O sections
// ============================================================================
type rw_process_fn = unsafe extern "C" fn(*mut SDL_RWops, *mut c_void, usize) -> c_int;
type section_fn = unsafe extern "C" fn(*mut SDL_RWops, rw_process_fn);

// #define process(x) if (!process_func(rw, &(x), sizeof(x))) return
macro_rules! process {
    ($rw:expr, $pf:expr, $x:expr) => {{
        if $pf(
            $rw,
            addr_of_mut!($x) as *mut c_void,
            ptr_size_of(addr_of!($x)),
        ) == 0
        {
            return;
        }
    }};
}

unsafe extern "C" fn options_process_features(rw: *mut SDL_RWops, process_func: rw_process_fn) {
    process!(rw, process_func, enable_copyprot);
    process!(rw, process_func, enable_quicksave);
    process!(rw, process_func, enable_quicksave_penalty);
}

unsafe extern "C" fn options_process_enhancements(rw: *mut SDL_RWops, process_func: rw_process_fn) {
    process!(rw, process_func, use_fixes_and_enhancements);
    process!(rw, process_func, (*fixes_options_replay()).enable_crouch_after_climbing);
    process!(rw, process_func, (*fixes_options_replay()).enable_freeze_time_during_end_music);
    process!(rw, process_func, (*fixes_options_replay()).enable_remember_guard_hp);
    process!(rw, process_func, (*fixes_options_replay()).enable_super_high_jump);
    process!(rw, process_func, (*fixes_options_replay()).enable_jump_grab);
}

unsafe extern "C" fn options_process_fixes(rw: *mut SDL_RWops, process_func: rw_process_fn) {
    process!(rw, process_func, (*fixes_options_replay()).fix_gate_sounds);
    process!(rw, process_func, (*fixes_options_replay()).fix_two_coll_bug);
    process!(rw, process_func, (*fixes_options_replay()).fix_infinite_down_bug);
    process!(rw, process_func, (*fixes_options_replay()).fix_gate_drawing_bug);
    process!(rw, process_func, (*fixes_options_replay()).fix_bigpillar_climb);
    process!(rw, process_func, (*fixes_options_replay()).fix_jump_distance_at_edge);
    process!(rw, process_func, (*fixes_options_replay()).fix_edge_distance_check_when_climbing);
    process!(rw, process_func, (*fixes_options_replay()).fix_painless_fall_on_guard);
    process!(rw, process_func, (*fixes_options_replay()).fix_wall_bump_triggers_tile_below);
    process!(rw, process_func, (*fixes_options_replay()).fix_stand_on_thin_air);
    process!(rw, process_func, (*fixes_options_replay()).fix_press_through_closed_gates);
    process!(rw, process_func, (*fixes_options_replay()).fix_grab_falling_speed);
    process!(rw, process_func, (*fixes_options_replay()).fix_skeleton_chomper_blood);
    process!(rw, process_func, (*fixes_options_replay()).fix_move_after_drink);
    process!(rw, process_func, (*fixes_options_replay()).fix_loose_left_of_potion);
    process!(rw, process_func, (*fixes_options_replay()).fix_guard_following_through_closed_gates);
    process!(rw, process_func, (*fixes_options_replay()).fix_safe_landing_on_spikes);
    process!(rw, process_func, (*fixes_options_replay()).fix_glide_through_wall);
    process!(rw, process_func, (*fixes_options_replay()).fix_drop_through_tapestry);
    process!(rw, process_func, (*fixes_options_replay()).fix_land_against_gate_or_tapestry);
    process!(rw, process_func, (*fixes_options_replay()).fix_unintended_sword_strike);
    process!(rw, process_func, (*fixes_options_replay()).fix_retreat_without_leaving_room);
    process!(rw, process_func, (*fixes_options_replay()).fix_running_jump_through_tapestry);
    process!(rw, process_func, (*fixes_options_replay()).fix_push_guard_into_wall);
    process!(rw, process_func, (*fixes_options_replay()).fix_jump_through_wall_above_gate);
    process!(rw, process_func, (*fixes_options_replay()).fix_chompers_not_starting);
    process!(rw, process_func, (*fixes_options_replay()).fix_feather_interrupted_by_leveldoor);
    process!(rw, process_func, (*fixes_options_replay()).fix_offscreen_guards_disappearing);
    process!(rw, process_func, (*fixes_options_replay()).fix_move_after_sheathe);
    process!(rw, process_func, (*fixes_options_replay()).fix_hidden_floors_during_flashing);
    process!(rw, process_func, (*fixes_options_replay()).fix_hang_on_teleport);
    process!(rw, process_func, (*fixes_options_replay()).fix_exit_door);
    process!(rw, process_func, (*fixes_options_replay()).fix_quicksave_during_feather);
    process!(rw, process_func, (*fixes_options_replay()).fix_caped_prince_sliding_through_gate);
    process!(rw, process_func, (*fixes_options_replay()).fix_doortop_disabling_guard);
    process!(rw, process_func, (*fixes_options_replay()).fix_jumping_over_guard);
    process!(rw, process_func, (*fixes_options_replay()).fix_drop_2_rooms_climbing_loose_tile);
    process!(rw, process_func, (*fixes_options_replay()).fix_falling_through_floor_during_sword_strike);
}

unsafe extern "C" fn options_process_custom_general(rw: *mut SDL_RWops, process_func: rw_process_fn) {
    process!(rw, process_func, (*custom).start_minutes_left);
    process!(rw, process_func, (*custom).start_ticks_left);
    process!(rw, process_func, (*custom).start_hitp);
    process!(rw, process_func, (*custom).max_hitp_allowed);
    process!(rw, process_func, (*custom).saving_allowed_first_level);
    process!(rw, process_func, (*custom).saving_allowed_last_level);
    process!(rw, process_func, (*custom).start_upside_down);
    process!(rw, process_func, (*custom).start_in_blind_mode);
    process!(rw, process_func, (*custom).copyprot_level);
    process!(rw, process_func, (*custom).drawn_tile_top_level_edge);
    process!(rw, process_func, (*custom).drawn_tile_left_level_edge);
    process!(rw, process_func, (*custom).level_edge_hit_tile);
    process!(rw, process_func, (*custom).allow_triggering_any_tile);
    process!(rw, process_func, (*custom).enable_wda_in_palace);
    process!(rw, process_func, (*custom).vga_palette);
    process!(rw, process_func, (*custom).first_level);
    process!(rw, process_func, (*custom).skip_title);
    process!(rw, process_func, (*custom).shift_L_allowed_until_level);
    process!(rw, process_func, (*custom).shift_L_reduced_minutes);
    process!(rw, process_func, (*custom).shift_L_reduced_ticks);
    process!(rw, process_func, (*custom).demo_hitp);
    process!(rw, process_func, (*custom).demo_end_room);
    process!(rw, process_func, (*custom).intro_music_level);
    process!(rw, process_func, (*custom).checkpoint_level);
    process!(rw, process_func, (*custom).checkpoint_respawn_dir);
    process!(rw, process_func, (*custom).checkpoint_respawn_room);
    process!(rw, process_func, (*custom).checkpoint_respawn_tilepos);
    process!(rw, process_func, (*custom).checkpoint_clear_tile_room);
    process!(rw, process_func, (*custom).checkpoint_clear_tile_col);
    process!(rw, process_func, (*custom).checkpoint_clear_tile_row);
    process!(rw, process_func, (*custom).skeleton_level);
    process!(rw, process_func, (*custom).skeleton_room);
    process!(rw, process_func, (*custom).skeleton_trigger_column_1);
    process!(rw, process_func, (*custom).skeleton_trigger_column_2);
    process!(rw, process_func, (*custom).skeleton_column);
    process!(rw, process_func, (*custom).skeleton_row);
    process!(rw, process_func, (*custom).skeleton_require_open_level_door);
    process!(rw, process_func, (*custom).skeleton_skill);
    process!(rw, process_func, (*custom).skeleton_reappear_room);
    process!(rw, process_func, (*custom).skeleton_reappear_x);
    process!(rw, process_func, (*custom).skeleton_reappear_row);
    process!(rw, process_func, (*custom).skeleton_reappear_dir);
    process!(rw, process_func, (*custom).mirror_level);
    process!(rw, process_func, (*custom).mirror_room);
    process!(rw, process_func, (*custom).mirror_column);
    process!(rw, process_func, (*custom).mirror_row);
    process!(rw, process_func, (*custom).mirror_tile);
    process!(rw, process_func, (*custom).show_mirror_image);
    process!(rw, process_func, (*custom).falling_exit_level);
    process!(rw, process_func, (*custom).falling_exit_room);
    process!(rw, process_func, (*custom).falling_entry_level);
    process!(rw, process_func, (*custom).falling_entry_room);
    process!(rw, process_func, (*custom).mouse_level);
    process!(rw, process_func, (*custom).mouse_room);
    process!(rw, process_func, (*custom).mouse_delay);
    process!(rw, process_func, (*custom).mouse_object);
    process!(rw, process_func, (*custom).mouse_start_x);
    process!(rw, process_func, (*custom).loose_tiles_level);
    process!(rw, process_func, (*custom).loose_tiles_room_1);
    process!(rw, process_func, (*custom).loose_tiles_room_2);
    process!(rw, process_func, (*custom).loose_tiles_first_tile);
    process!(rw, process_func, (*custom).loose_tiles_last_tile);
    process!(rw, process_func, (*custom).jaffar_victory_level);
    process!(rw, process_func, (*custom).jaffar_victory_flash_time);
    process!(rw, process_func, (*custom).hide_level_number_from_level);
    process!(rw, process_func, (*custom).level_13_level_number);
    process!(rw, process_func, (*custom).victory_stops_time_level);
    process!(rw, process_func, (*custom).win_level);
    process!(rw, process_func, (*custom).win_room);
    process!(rw, process_func, (*custom).loose_floor_delay);
    process!(rw, process_func, (*custom).shadow_steal_level);
    process!(rw, process_func, (*custom).shadow_steal_room);
    process!(rw, process_func, (*custom).shadow_step_level);
    process!(rw, process_func, (*custom).shadow_step_room);
}

unsafe extern "C" fn options_process_custom_per_level(rw: *mut SDL_RWops, process_func: rw_process_fn) {
    process!(rw, process_func, (*custom).tbl_level_type);
    process!(rw, process_func, (*custom).tbl_level_color);
    process!(rw, process_func, (*custom).tbl_guard_type);
    process!(rw, process_func, (*custom).tbl_guard_hp);
    process!(rw, process_func, (*custom).tbl_cutscenes_by_index);
    process!(rw, process_func, (*custom).tbl_entry_pose);
    process!(rw, process_func, (*custom).tbl_seamless_exit);
}

// struct for keeping track of both the normal and the replay options
#[repr(C)]
struct replay_options_section_type {
    data_size: dword,
    replay_data: [byte; POP_MAX_OPTIONS_SIZE as usize],
    stored_data: [byte; POP_MAX_OPTIONS_SIZE as usize],
    section_func: section_fn,
}

static mut replay_options_sections: [replay_options_section_type; 5] = [
    replay_options_section_type {
        data_size: 0,
        replay_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        stored_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        section_func: options_process_features,
    },
    replay_options_section_type {
        data_size: 0,
        replay_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        stored_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        section_func: options_process_enhancements,
    },
    replay_options_section_type {
        data_size: 0,
        replay_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        stored_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        section_func: options_process_fixes,
    },
    replay_options_section_type {
        data_size: 0,
        replay_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        stored_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        section_func: options_process_custom_general,
    },
    replay_options_section_type {
        data_size: 0,
        replay_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        stored_data: [0; POP_MAX_OPTIONS_SIZE as usize],
        section_func: options_process_custom_per_level,
    },
];

const REPLAY_OPTIONS_SECTIONS_COUNT: usize = 5;

// output the current options to a memory buffer
unsafe fn save_options_to_buffer(
    options_buffer: *mut c_void,
    max_size: usize,
    process_section_func: section_fn,
) -> usize {
    let rw = SDL_RWFromMem(options_buffer, max_size as c_int);
    process_section_func(rw, process_rw_write as rw_process_fn);
    let mut section_size: i64 = SDL_RWtell(rw);
    if section_size < 0 {
        section_size = 0;
    }
    SDL_RWclose(rw);
    section_size as usize
}

// restore the options from a memory buffer
unsafe fn load_options_from_buffer(
    options_buffer: *mut c_void,
    options_size: usize,
    process_section_func: section_fn,
) {
    let rw = SDL_RWFromMem(options_buffer, options_size as c_int);
    process_section_func(rw, process_rw_read as rw_process_fn);
    SDL_RWclose(rw);
}

#[no_mangle]
pub unsafe extern "C" fn init_record_replay() {
    if enable_replay == 0 {
        return;
    }
    if !check_param(cs!("record")).is_null() {
        start_recording();
    } else if need_start_replay != 0 || !check_param(cs!("replay")).is_null() {
        start_replay();
    }
}

#[no_mangle]
pub unsafe extern "C" fn replay_restore_level() {
    // Need to restore the savestate at the right time (just before the first room of the level is drawn).
    if curr_tick == 0 {
        restore_savestate_from_buffer();
    }
}

unsafe extern "C" fn process_to_buffer(data: *mut c_void, data_size: usize) -> c_int {
    if savestate_offset as usize + data_size > MAX_SAVESTATE_SIZE {
        printf(cs!("Saving savestate to memory failed: buffer is overflowing!\n"));
        return 0;
    }
    memcpy(
        savestate_buffer.add(savestate_offset as usize) as *mut c_void,
        data,
        data_size,
    );
    savestate_offset += data_size as dword;
    1
}

unsafe extern "C" fn process_load_from_buffer(data: *mut c_void, data_size: usize) -> c_int {
    // Prevent torches from being randomly colored when an older replay is loaded.
    if savestate_offset >= savestate_size {
        return 0;
    }
    memcpy(
        data,
        savestate_buffer.add(savestate_offset as usize) as *const c_void,
        data_size,
    );
    savestate_offset += data_size as dword;
    1
}

type ProcessFn = unsafe extern "C" fn(*mut c_void, usize) -> c_int;
extern "C" {
    fn quick_process(process_func: ProcessFn) -> c_int;
}

unsafe fn savestate_to_buffer() -> c_int {
    let mut ok = 0;
    if savestate_buffer.is_null() {
        savestate_buffer = malloc(MAX_SAVESTATE_SIZE) as *mut byte;
    }
    if !savestate_buffer.is_null() {
        savestate_offset = 0;
        savestate_size = 0;
        ok = quick_process(process_to_buffer);
        savestate_size = savestate_offset;
    }
    ok
}

unsafe fn reload_resources() {
    // the replay's levelset might use different sounds, so we need to free and reload sounds
    free_all_sounds();
    load_all_sounds();
    free_all_chtabs_from(chtabs_id_chtab_0_sword as c_int);
    // chtabs 0-2 are usually not freed; reload them manually.
    let dat = open_dat(cs!("PRINCE.DAT"), b'G' as c_int);
    // PRINCE.DAT: sword
    chtab_addrs[chtabs_id_chtab_0_sword as usize] = load_sprites_from_file(700, 1 << 2, 1);
    // PRINCE.DAT: flame, sword on floor, potion
    chtab_addrs[chtabs_id_chtab_1_flameswordpotion as usize] = load_sprites_from_file(150, 1 << 3, 1);
    close_dat(dat);
    load_kid_sprite(); // reloads chtab 2
}

#[no_mangle]
pub unsafe extern "C" fn restore_savestate_from_buffer() -> c_int {
    let mut ok = 0;
    savestate_offset = 0;
    // This condition should be checked in process_load_from_buffer() instead of here.
    while savestate_offset < savestate_size {
        ok = quick_process(process_load_from_buffer);
    }
    reload_resources();
    restore_room_after_quick_load();
    ok
}

#[no_mangle]
pub unsafe extern "C" fn start_recording() {
    curr_tick = 0;
    recording = 1; // further set-up is done in add_replay_move, on the first gameplay tick
}

#[no_mangle]
pub unsafe extern "C" fn add_replay_move() {
    if curr_tick == 0 {
        prandom(1); // make sure random_seed is initialized
        saved_random_seed = random_seed;
        seed_was_init = 1;
        savestate_to_buffer(); // create a savestate in memory
        display_text_bottom(cs!("RECORDING"));
        text_time_total = 24;
        text_time_remaining = 24;
    }

    let mut curr_move: byte = 0;
    // curr_move.x = control_x;
    curr_move |= (control_x as byte) & 0x03;
    // curr_move.y = control_y;
    curr_move |= ((control_y as byte) & 0x03) << 2;
    if control_shift != 0 {
        curr_move |= 1 << 4;
    }

    if special_move != 0 {
        curr_move |= (special_move & 0x07) << 5;
        special_move = 0;
    }

    moves[curr_tick as usize] = curr_move;

    curr_tick += 1;

    if curr_tick >= MAX_REPLAY_DURATION as dword {
        // max replay length exceeded
        stop_recording();
    }
}

#[no_mangle]
pub unsafe extern "C" fn stop_recording() {
    recording = 0;
    if save_recorded_replay_dialog() != 0 {
        display_text_bottom(cs!("REPLAY SAVED"));
    } else {
        display_text_bottom(cs!("REPLAY CANCELED"));
    }
    text_time_total = 24;
    text_time_remaining = 24;
}

unsafe fn apply_replay_options() {
    // store the current options, so they can be restored later
    for i in 0..REPLAY_OPTIONS_SECTIONS_COUNT {
        save_options_to_buffer(
            addr_of_mut!(replay_options_sections[i].stored_data) as *mut c_void,
            POP_MAX_OPTIONS_SIZE as usize,
            replay_options_sections[i].section_func,
        );
    }

    // apply the options from the memory buffer
    for i in 0..REPLAY_OPTIONS_SECTIONS_COUNT {
        load_options_from_buffer(
            addr_of_mut!(replay_options_sections[i].replay_data) as *mut c_void,
            replay_options_sections[i].data_size as usize,
            replay_options_sections[i].section_func,
        );
    }

    // fixes_saved = fixes_options_replay;
    memcpy(
        addr_of_mut!(fixes_saved) as *mut c_void,
        fixes_options_replay() as *const c_void,
        FIXES_SZ,
    );
    turn_fixes_and_enhancements_on_off(use_fixes_and_enhancements);
    enable_replay = 1; // just to be safe...

    memcpy(
        addr_of_mut!(stored_levelset_name) as *mut c_void,
        addr_of!(levelset_name) as *const c_void,
        POP_MAX_PATH as usize,
    );
    memcpy(
        addr_of_mut!(levelset_name) as *mut c_void,
        addr_of!(replay_levelset_name) as *const c_void,
        POP_MAX_PATH as usize,
    );
    use_custom_levelset = if levelset_name[0] == 0 { 0 } else { 1 };

    load_mod_options(); // Load resources from the correct places if there is a mod name in the replay file.
    reload_resources();
}

unsafe fn restore_normal_options() {
    // apply the stored options
    for i in 0..REPLAY_OPTIONS_SECTIONS_COUNT {
        load_options_from_buffer(
            addr_of_mut!(replay_options_sections[i].stored_data) as *mut c_void,
            POP_MAX_OPTIONS_SIZE as usize,
            replay_options_sections[i].section_func,
        );
    }

    start_level = -1; // may have been set to a different value by the replay

    memcpy(
        addr_of_mut!(levelset_name) as *mut c_void,
        addr_of!(stored_levelset_name) as *const c_void,
        POP_MAX_PATH as usize,
    );
    use_custom_levelset = if levelset_name[0] == 0 { 0 } else { 1 };
}

unsafe fn print_remaining_time() {
    if rem_min > 0 {
        printf(
            cs!("Remaining time: %d min, %d sec, %d ticks. "),
            rem_min as c_int - 1,
            rem_tick as c_int / 12,
            rem_tick as c_int % 12,
        );
    } else {
        printf(
            cs!("Elapsed time:   %d min, %d sec, %d ticks. "),
            -(rem_min as c_int + 1),
            (719 - rem_tick as c_int) / 12,
            (719 - rem_tick as c_int) % 12,
        );
    }
    printf(cs!("(rem_min=%d, rem_tick=%d)\n"), rem_min as c_int, rem_tick as c_int);
}

#[no_mangle]
pub unsafe extern "C" fn start_replay() {
    stop_sounds(); // Don't crash if the intro music is interrupted by Tab in PC Speaker mode.
    if enable_replay == 0 {
        return;
    }
    need_start_replay = 0;
    if is_validate_mode == 0 {
        list_replay_files();
    }
    if load_replay() == 0 {
        return;
    }
    // Set replaying before applying options, so the latter can display an appropriate error message.
    replaying = 1;
    apply_replay_options();
    curr_tick = 0;
}

#[no_mangle]
pub unsafe extern "C" fn end_replay() {
    if is_validate_mode == 0 {
        replaying = 0;
        skipping_replay = 0;
        restore_normal_options();
        start_game();
    } else {
        printf(
            cs!("\nReplay ended in level %d, room %d.\n"),
            current_level as c_int,
            drawn_room as c_int,
        );

        if Kid.alive < 0 {
            printf(cs!("Kid is alive.\n"));
        } else if text_time_total == 288 && text_time_remaining <= 1 {
            printf(cs!("Kid is dead. (Did not press button to continue.)\n"));
        } else {
            printf(cs!("Kid is dead.\n"));
        }

        print_remaining_time();

        let minute_ticks = curr_tick % 720;
        printf(
            cs!("Play duration:  %d min, %d sec, %d ticks. (curr_tick=%d)\n\n"),
            (curr_tick / 720) as c_int,
            (minute_ticks / 12) as c_int,
            (minute_ticks % 12) as c_int,
            curr_tick as c_int,
        );

        if num_replay_ticks != curr_tick {
            printf(
                cs!("WARNING: Play duration does not match replay length. (%d ticks)\n"),
                num_replay_ticks as c_int,
            );
        } else {
            printf(
                cs!("Play duration matches replay length. (%d ticks)\n"),
                num_replay_ticks as c_int,
            );
        }
        exit(0);
    }
}

#[inline]
unsafe fn sext2(v: byte) -> sbyte {
    (((v & 0x03) << 6) as i8) >> 6
}

#[no_mangle]
pub unsafe extern "C" fn do_replay_move() {
    if curr_tick == 0 {
        random_seed = saved_random_seed;
        seed_was_init = 1;

        if is_validate_mode != 0 {
            printf(
                cs!("Replay started in level %d, room %d.\n"),
                current_level as c_int,
                drawn_room as c_int,
            );
            print_remaining_time();
            skipping_replay = 1;
            replay_seek_target = replay_seek_targets_replay_seek_2_end as byte;
        }
    }
    if curr_tick == num_replay_ticks {
        // replay is finished
        end_replay();
        return;
    }
    if current_level == next_level {
        let bits = moves[curr_tick as usize];

        control_x = sext2(bits & 0x03);
        control_y = sext2((bits >> 2) & 0x03);

        // Ignore Shift if the kid is dead: restart moves are hard-coded as a 'special move'.
        if rem_min != 0 && Kid.alive > 6 {
            control_shift = CONTROL_RELEASED as sbyte;
        } else {
            control_shift = if ((bits >> 4) & 0x01) != 0 {
                CONTROL_HELD as sbyte
            } else {
                CONTROL_RELEASED as sbyte
            };
        }

        let special = (bits >> 5) & 0x07;
        if special == replay_special_moves_MOVE_RESTART_LEVEL as byte {
            // restart level
            stop_sounds();
            is_restart_level = 1;
        } else if special == replay_special_moves_MOVE_EFFECT_END as byte {
            stop_sounds();
            if need_level1_music == 2 {
                need_level1_music = 0;
            }
            is_feather_fall = 0;
        }

        curr_tick += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn save_recorded_replay_dialog() -> c_int {
    // prompt for replay filename
    let mut rect: rect_type = core::mem::zeroed();
    let bgcolor: c_int = colorids_color_8_darkgray as c_int;
    let color: c_int = colorids_color_15_brightwhite as c_int;
    current_target_surface = onscreen_surface_;
    method_1_blit_rect(
        offscreen_surface,
        onscreen_surface_,
        addr_of!((*copyprot_dialog).peel_rect),
        addr_of!((*copyprot_dialog).peel_rect),
        0,
    );
    draw_dialog_frame(copyprot_dialog);
    shrink2_rect(addr_of_mut!(rect), addr_of!((*copyprot_dialog).text_rect), 2, 1);
    show_text_with_color(
        addr_of!(rect),
        halign_center,
        valign_middle,
        cs!("Save replay\nenter the filename...\n\n"),
        colorids_color_15_brightwhite as c_int,
    );
    clear_kbd_buf();

    let mut text_rect: rect_type = core::mem::zeroed();
    let input_rect = rect_type {
        top: 104,
        left: 64,
        bottom: 118,
        right: 256,
    };
    offset4_rect_add(addr_of_mut!(text_rect), addr_of!(input_rect), -2, 0, 2, 0);
    draw_rect(addr_of!(text_rect), bgcolor);
    current_target_surface = onscreen_surface_;
    need_full_redraw = 1; // lazy: redraw the whole screen

    let mut input_filename = [0 as c_char; POP_MAX_PATH as usize];
    let mut input_length: c_int;
    loop {
        input_length = input_str(
            addr_of!(input_rect),
            input_filename.as_mut_ptr(),
            64,
            cs!(""),
            0,
            0,
            color,
            bgcolor,
        );
        if input_length != 0 {
            break;
        }
    } // filename must be at least 1 character

    if input_length < 0 {
        return 0; // Escape was pressed -> discard the replay
    }

    let mut full_filename = [0 as c_char; POP_MAX_PATH as usize];
    snprintf_check!(
        full_filename.as_mut_ptr(),
        POP_MAX_PATH as usize,
        cs!("%s/%s.p1r"),
        addr_of!(replays_folder) as *const c_char,
        input_filename.as_ptr()
    );

    // create the "replays" folder if it does not exist already
    mkdir(addr_of!(replays_folder) as *const c_char, 0o700);

    // NOTE: We currently overwrite the replay file if it exists already.

    save_recorded_replay(full_filename.as_ptr())
}

#[no_mangle]
pub unsafe extern "C" fn save_recorded_replay(full_filename: *const c_char) -> c_int {
    replay_fp = fopen(full_filename, cs!("wb"));
    if !replay_fp.is_null() {
        fwrite(replay_magic_number.as_ptr() as *const c_void, 3, 1, replay_fp); // magic number "P1R"
        fwrite(addr_of!(replay_format_class) as *const c_void, size_of::<word>(), 1, replay_fp);
        fputc(REPLAY_FORMAT_CURR_VERSION, replay_fp);
        fputc(REPLAY_FORMAT_DEPRECATION_NUMBER, replay_fp);
        let seconds: i64 = time(null_mut());
        fwrite(addr_of!(seconds) as *const c_void, size_of::<i64>(), 1, replay_fp);
        // levelset_name
        fputc(
            strnlen(addr_of!(levelset_name) as *const c_char, 255) as c_int,
            replay_fp,
        ); // length of the levelset name (is zero for original levels)
        fputs(addr_of!(levelset_name) as *const c_char, replay_fp);
        // implementation name
        let impl_name = implementation_name();
        fputc(strnlen(impl_name, 255) as c_int, replay_fp);
        fputs(impl_name, replay_fp);
        // embed a savestate into the replay
        fwrite(addr_of!(savestate_size) as *const c_void, size_of::<dword>(), 1, replay_fp);
        fwrite(savestate_buffer as *const c_void, savestate_size as usize, 1, replay_fp);

        // Save the current options (not the defaults) into the replay!
        // fixes_options_replay = fixes_saved;
        memcpy(
            fixes_options_replay() as *mut c_void,
            addr_of!(fixes_saved) as *const c_void,
            FIXES_SZ,
        );

        // save the options, organized per section
        let mut temp_options = [0u8; POP_MAX_OPTIONS_SIZE as usize];
        for i in 0..REPLAY_OPTIONS_SECTIONS_COUNT {
            let section_size: dword = save_options_to_buffer(
                temp_options.as_mut_ptr() as *mut c_void,
                temp_options.len(),
                replay_options_sections[i].section_func,
            ) as dword;
            fwrite(addr_of!(section_size) as *const c_void, size_of::<dword>(), 1, replay_fp);
            fwrite(temp_options.as_ptr() as *const c_void, section_size as usize, 1, replay_fp);
        }

        // save the rest of the replay data
        fwrite(addr_of!(start_level) as *const c_void, size_of::<c_short>(), 1, replay_fp);
        fwrite(addr_of!(saved_random_seed) as *const c_void, size_of::<dword>(), 1, replay_fp);
        num_replay_ticks = curr_tick;
        fwrite(addr_of!(num_replay_ticks) as *const c_void, size_of::<dword>(), 1, replay_fp);
        fwrite(addr_of!(moves) as *const c_void, num_replay_ticks as usize, 1, replay_fp);
        fclose(replay_fp);
        replay_fp = null_mut();
    }

    1
}

unsafe fn open_next_replay_file() -> byte {
    if next_replay_number > num_replay_files - 1 {
        return 0; // reached the last replay file, return to title screen
    }
    current_replay_number = next_replay_number;
    next_replay_number += 1; // cycle
    open_replay_file(
        addr_of!((*replay_list.add(current_replay_number as usize)).filename) as *const c_char,
    );
    if replay_file_open != 0 {
        return 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn replay_cycle() {
    need_replay_cycle = 0;
    skipping_replay = 0;
    stop_sounds();
    if current_replay_number == -1 /* opened .P1R file directly, so cycling is disabled */
        || open_next_replay_file() == 0
        || load_replay() == 0
    {
        // there is no replay to be cycled to after the current one --> restart the game
        replaying = 0;
        restore_normal_options();
        start_game();
        return;
    }
    apply_replay_options();
    restore_savestate_from_buffer();
    curr_tick = 0; // Do this after restoring the savestate, in case the savestate contained a non-zero curr_tick.
    show_level();
}

#[no_mangle]
pub unsafe extern "C" fn load_replay() -> c_int {
    if replay_file_open == 0 {
        next_replay_number = 0;
        if open_next_replay_file() == 0 {
            return 0;
        }
    }
    if savestate_buffer.is_null() {
        savestate_buffer = malloc(MAX_SAVESTATE_SIZE) as *mut byte;
    }
    if !replay_fp.is_null() && !savestate_buffer.is_null() {
        let mut header: replay_header_type = core::mem::zeroed();
        let mut error_message = [0 as c_char; REPLAY_HEADER_ERROR_MESSAGE_MAX];
        let err = error_message.as_mut_ptr();
        let ok = read_replay_header(addr_of_mut!(header), replay_fp, err);
        if ok == 0 {
            printf(cs!("Error loading replay: %s!\n"), error_message.as_ptr());
            fclose(replay_fp);
            replay_fp = null_mut();
            replay_file_open = 0;
            return 0;
        }

        memcpy(
            addr_of_mut!(replay_levelset_name) as *mut c_void,
            header.levelset_name.as_ptr() as *const c_void,
            size_of::<[c_char; POP_MAX_PATH as usize]>(),
        );

        // load the savestate
        fread_check!(addr_of_mut!(savestate_size), size_of::<dword>(), 1, replay_fp, err, "&savestate_size");
        fread_check!(savestate_buffer, savestate_size, 1, replay_fp, err, "savestate_buffer");

        // load the replay options, organized per section
        for i in 0..REPLAY_OPTIONS_SECTIONS_COUNT {
            let mut section_size: dword = 0;
            fread_check!(addr_of_mut!(section_size), size_of::<dword>(), 1, replay_fp, err, "&section_size");
            fread_check!(
                addr_of_mut!(replay_options_sections[i].replay_data) as *mut byte,
                section_size,
                1,
                replay_fp,
                err,
                "replay_options_sections[i].replay_data"
            );
            replay_options_sections[i].data_size = section_size;
        }

        // load the rest of the replay data
        fread_check!(addr_of_mut!(start_level), size_of::<c_short>(), 1, replay_fp, err, "&start_level");
        fread_check!(addr_of_mut!(saved_random_seed), size_of::<dword>(), 1, replay_fp, err, "&saved_random_seed");
        fread_check!(addr_of_mut!(num_replay_ticks), size_of::<dword>(), 1, replay_fp, err, "&num_replay_ticks");
        fread_check!(addr_of_mut!(moves) as *mut byte, num_replay_ticks, 1, replay_fp, err, "moves");
        fclose(replay_fp);
        replay_fp = null_mut();
        replay_file_open = 0;
        return 1; // success
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn key_press_while_recording(key_ptr: *mut c_int) {
    let key = *key_ptr;
    if key == (SDL_SCANCODE_A | WITH_CTRL) {
        special_move = replay_special_moves_MOVE_RESTART_LEVEL as byte;
    } else if key == (SDL_SCANCODE_R | WITH_CTRL) {
        save_recorded_replay_dialog();
        recording = 0;
    }
}

#[no_mangle]
pub unsafe extern "C" fn key_press_while_replaying(key_ptr: *mut c_int) {
    let key = *key_ptr;
    const ESCAPE_SHIFT: c_int = SDL_SCANCODE_ESCAPE | WITH_SHIFT;
    const S_CTRL: c_int = SDL_SCANCODE_S | WITH_CTRL;
    const V_CTRL: c_int = SDL_SCANCODE_V | WITH_CTRL;
    const C_CTRL: c_int = SDL_SCANCODE_C | WITH_CTRL;
    const C_SHIFT: c_int = SDL_SCANCODE_C | WITH_SHIFT;
    const I_SHIFT: c_int = SDL_SCANCODE_I | WITH_SHIFT;
    const B_SHIFT: c_int = SDL_SCANCODE_B | WITH_SHIFT;
    const R_CTRL: c_int = SDL_SCANCODE_R | WITH_CTRL;
    const F_SHIFT: c_int = SDL_SCANCODE_F | WITH_SHIFT;
    match key {
        0 => {} // 'no key pressed'
        // ...but these are allowable actions:
        SDL_SCANCODE_ESCAPE      // pause
        | ESCAPE_SHIFT
        | SDL_SCANCODE_BACKSPACE // menu
        | SDL_SCANCODE_SPACE     // time
        | S_CTRL                 // sound toggle
        | V_CTRL                 // version
        | C_CTRL                 // SDL version
        | SDL_SCANCODE_C         // room numbers
        | C_SHIFT
        | I_SHIFT                // invert
        | B_SHIFT                // blind
        | SDL_SCANCODE_T => {}   // debug time
        R_CTRL => {
            // restart game
            replaying = 0;
            restore_normal_options();
        }
        SDL_SCANCODE_TAB => {
            need_replay_cycle = 1;
            restore_normal_options();
        }
        SDL_SCANCODE_F => {
            // skip forward to next room
            skipping_replay = 1;
            replay_seek_target = replay_seek_targets_replay_seek_0_next_room as byte;
        }
        F_SHIFT => {
            // skip forward to start of next level
            skipping_replay = 1;
            replay_seek_target = replay_seek_targets_replay_seek_1_next_level as byte;
        }
        _ => {
            // cannot manually do most stuff during a replay, so cancel the pressed key...
            *key_ptr = 1; // don't set to zero (we would be unable to unpause a replay)
        }
    }
}
