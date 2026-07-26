#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_long, c_short, c_void};
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut};
use std::ffi::CStr;
use super::*;

// ============================================================================
// libc / stdlib declarations (fopen/fread/fclose come from lib.rs via super::*)
// ============================================================================
extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn memcpy(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> c_int;
    fn strlen(s: *const c_char) -> usize;
    fn strcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char;
    fn strcasecmp(a: *const c_char, b: *const c_char) -> c_int;
    fn strncasecmp(a: *const c_char, b: *const c_char, n: usize) -> c_int;
    fn strtol(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_long;
    fn strtoimax(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> i64;
    fn isspace(c: c_int) -> c_int;
    fn fgetc(stream: *mut FILE) -> c_int;
    fn feof(stream: *mut FILE) -> c_int;
    fn fileno(stream: *mut FILE) -> c_int;
    fn stat(path: *const c_char, buf: *mut stat_t) -> c_int;
    fn fstat(fd: c_int, buf: *mut stat_t) -> c_int;

    // process_rw_write / process_rw_read and never_is_16_list are defined in
    // src/sdl_rw_wrappers.c (still compiled as C). Reference never_is_16_list here.
    pub static mut never_is_16_list: names_list_type;
}

// glibc x86-64 struct stat (144 bytes). We only read st_mode and st_size.
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

const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
#[inline]
fn S_ISDIR(m: u32) -> bool {
    (m & S_IFMT) == S_IFDIR
}

// C string literal helper: produces a null-terminated *const c_char.
macro_rules! cs {
    ($s:expr) => {
        concat!($s, "\0").as_ptr() as *const c_char
    };
}

// Renders a C string the way printf's "%s" would, including glibc's "(null)" for a null
// pointer. Used by the `format!`-based replacements for the printf family.
unsafe fn cstr_arg(p: *const c_char) -> String {
    if p.is_null() {
        return String::from("(null)");
    }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}

/// Replaces `sscanf(str, "%d", out)` for the three call sites that pull a plain decimal
/// integer off the front of a C string. Mirrors `%d`: skip leading whitespace, accept an
/// optional sign, then require at least one decimal digit; trailing characters are ignored.
/// Out-of-range values behave like glibc, which parses with strtol and then truncates the
/// result to `int`. Returns the number of assignments (1 on success, 0 on a matching
/// failure) or -1 when the string ends before the conversion could start.
unsafe fn sscanf_leading_int(s: *const c_char, out: *mut c_int) -> c_int {
    let mut p = s;
    while matches!(*p as u8, 0x20 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D) {
        p = p.add(1);
    }
    if *p == 0 {
        return -1; // input failure: nothing left to convert
    }
    let negative = match *p as u8 {
        b'-' => {
            p = p.add(1);
            true
        }
        b'+' => {
            p = p.add(1);
            false
        }
        _ => false,
    };
    if !(*p as u8).is_ascii_digit() {
        return 0; // matching failure (a lone sign counts as one too)
    }
    let mut acc: i64 = 0;
    while (*p as u8).is_ascii_digit() {
        let digit = (*p as u8 - b'0') as i64;
        acc = acc.saturating_mul(10);
        acc = if negative {
            acc.saturating_sub(digit)
        } else {
            acc.saturating_add(digit)
        };
        p = p.add(1);
    }
    *out = acc as c_int; // glibc truncates the long result to int
    1
}

// snprintf_check (from common.h): bail out with quit(2) on truncation.
// The C original wraps snprintf; here the caller passes a Rust format string instead and
// the formatted text goes through write_c_str_truncating, which returns the same
// "length the full string would have had" that snprintf does.
macro_rules! snprintf_check {
    ($dst:expr, $size:expr, $($arg:tt)*) => {{
        let __len = crate::write_c_str_truncating($dst, $size, &format!($($arg)*));
        if __len < 0 || __len >= ($size) as c_int {
            crate::c_log_err("options: buffer truncation detected!\n");
            quit(2);
        }
    }};
}

// ============================================================================
// turn_fixes_and_enhancements_on_off / turn_custom_options_on_off
// ============================================================================
#[no_mangle]
pub unsafe extern "C" fn turn_fixes_and_enhancements_on_off(new_state: byte) {
    use_fixes_and_enhancements = new_state;
    fixes = if new_state != 0 {
        addr_of_mut!(fixes_saved)
    } else {
        addr_of_mut!(fixes_disabled_state)
    };
}

#[no_mangle]
pub unsafe extern "C" fn turn_custom_options_on_off(new_state: byte) {
    use_custom_options = new_state;
    custom = if new_state != 0 {
        addr_of_mut!(custom_saved)
    } else {
        addr_of_mut!(custom_defaults)
    };
}

// ============================================================================
// ini_load - .ini file parser adapted from https://gist.github.com/OrangeTide/947070
// ============================================================================
type ini_report_fn = unsafe extern "C" fn(*const c_char, *const c_char, *const c_char) -> c_int;

// ---------------------------------------------------------------------------
// Hand-written replacements for the four `fscanf` calls in ini_load.
//
// C's fscanf is variadic, so it can't be used from stable Rust on every target; the four
// format strings used here are fixed, so each is reimplemented directly against the stream
// with fgetc plus a one-character pushback (which is exactly what fscanf's internal ungetc
// does -- fscanf never looks ahead more than one character for these directives).
//
// The emulated rules, verified experimentally against glibc:
//   * whitespace in the format ("` `", "`\n`") = consume all whitespace, never fails.
//   * an ordinary character = read one char; on mismatch push it back and stop ("matching
//     failure"); at EOF stop ("input failure").
//   * `%<width>[^set]` = consume up to <width> chars not in `set`; the terminating char is
//     pushed back (as is the char just past the width limit). Assigns and NUL-terminates
//     only if at least one char matched; zero chars matched is a matching failure, and
//     zero chars matched *because of EOF* is an input failure.
//   * return value = number of assignments made, except that EOF (-1) is returned when an
//     input failure happens before the first conversion has completed. Suppressed
//     conversions (`%*[...]`) count as conversions for that rule but never as assignments.
// ---------------------------------------------------------------------------

/// Sentinel for "nothing pushed back" / EOF, matching C's EOF.
const INI_EOF: c_int = -1;

/// One-character pushback buffer standing in for the ungetc that fscanf performs.
struct IniStream {
    f: *mut FILE,
    pushback: c_int,
}

impl IniStream {
    unsafe fn getc(&mut self) -> c_int {
        if self.pushback != INI_EOF {
            let c = self.pushback;
            self.pushback = INI_EOF;
            c
        } else {
            fgetc(self.f)
        }
    }

    /// ungetc: pushing back EOF is a no-op, as in C.
    fn ungetc(&mut self, c: c_int) {
        if c != INI_EOF {
            self.pushback = c;
        }
    }

    /// True when the stream is exhausted: no pushback pending and the EOF flag is set.
    /// (C's ungetc clears the EOF indicator, hence the pushback check.)
    unsafe fn at_eof(&self) -> bool {
        self.pushback == INI_EOF && feof(self.f) != 0
    }

    /// A whitespace directive in the format string.
    unsafe fn skip_whitespace(&mut self) {
        loop {
            let c = self.getc();
            if c == INI_EOF {
                break;
            }
            // C locale isspace(): space, \t, \n, \v, \f, \r
            if !matches!(c, 0x20 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D) {
                self.ungetc(c);
                break;
            }
        }
    }
}

/// Outcome of one `%[...]` conversion.
const INI_SCANSET_INPUT_FAILURE: c_int = -1;
const INI_SCANSET_MATCH_FAILURE: c_int = 0;
const INI_SCANSET_OK: c_int = 1;

/// `%<width>[^<stop>]`. Writes into `dst` (NUL-terminated) only on success.
/// `dst` may be null, standing in for a suppressed conversion (`%*[^...]`).
unsafe fn ini_scanset(
    s: &mut IniStream,
    dst: *mut c_char,
    width: usize,
    stop: &[u8],
) -> c_int {
    let mut n: usize = 0;
    let mut hit_eof = false;
    while n < width {
        let c = s.getc();
        if c == INI_EOF {
            hit_eof = true;
            break;
        }
        if stop.contains(&(c as u8)) {
            s.ungetc(c);
            break;
        }
        if !dst.is_null() {
            *dst.add(n) = c as u8 as c_char;
        }
        n += 1;
    }
    if n == 0 {
        return if hit_eof {
            INI_SCANSET_INPUT_FAILURE
        } else {
            INI_SCANSET_MATCH_FAILURE
        };
    }
    if !dst.is_null() {
        *dst.add(n) = 0;
    }
    INI_SCANSET_OK
}

/// Replaces `fscanf(f, "[%127[^];\n]]\n", section)`.
unsafe fn ini_scan_section(s: &mut IniStream, section: *mut c_char) -> c_int {
    // literal '['
    let c = s.getc();
    if c == INI_EOF {
        return -1; // input failure before the first conversion
    }
    if c != b'[' as c_int {
        s.ungetc(c);
        return 0;
    }
    // %127[^];\n]
    match ini_scanset(s, section, 127, b"];\n") {
        INI_SCANSET_INPUT_FAILURE => return -1,
        INI_SCANSET_MATCH_FAILURE => return 0,
        _ => {}
    }
    // literal ']' -- a mismatch here still leaves the one assignment made above
    let c = s.getc();
    if c != b']' as c_int {
        s.ungetc(c);
        return 1;
    }
    // trailing '\n' whitespace directive
    s.skip_whitespace();
    1
}

/// Replaces `fscanf(f, " %63[^=;\n] = %255[^;\n]", name, value)`.
unsafe fn ini_scan_name_value(s: &mut IniStream, name: *mut c_char, value: *mut c_char) -> c_int {
    s.skip_whitespace();
    match ini_scanset(s, name, 63, b"=;\n") {
        INI_SCANSET_INPUT_FAILURE => return -1,
        INI_SCANSET_MATCH_FAILURE => return 0,
        _ => {}
    }
    s.skip_whitespace();
    let c = s.getc();
    if c != b'=' as c_int {
        s.ungetc(c);
        return 1;
    }
    s.skip_whitespace();
    // Both a matching failure and an input failure here leave the count at 1, because the
    // first conversion already completed.
    if ini_scanset(s, value, 255, b";\n") == INI_SCANSET_OK {
        2
    } else {
        1
    }
}

/// Replaces `fscanf(f, " ;%*[^\n]")`. Never assigns, so it returns 0 or EOF.
unsafe fn ini_scan_comment(s: &mut IniStream) -> c_int {
    s.skip_whitespace();
    let c = s.getc();
    if c == INI_EOF {
        return -1; // input failure before the (suppressed) conversion completed
    }
    if c != b';' as c_int {
        s.ungetc(c);
        return 0;
    }
    // %*[^\n] -- unbounded width, assignment suppressed
    if ini_scanset(s, core::ptr::null_mut(), usize::MAX, b"\n") == INI_SCANSET_INPUT_FAILURE {
        return -1;
    }
    0
}

unsafe fn ini_load(filename: *const c_char, report: ini_report_fn) -> c_int {
    let mut name = [0 as c_char; 64];
    let mut value = [0 as c_char; 256];
    let mut section = [0 as c_char; 128];
    section[0] = 0;
    let mut cnt: c_int;

    let f: *mut FILE = fopen(filename, cs!("r"));
    if f.is_null() {
        return -1;
    }

    let mut s = IniStream {
        f,
        pushback: INI_EOF,
    };

    while !s.at_eof() {
        if ini_scan_section(&mut s, section.as_mut_ptr()) == 1 {
            // section header
        } else {
            cnt = ini_scan_name_value(&mut s, name.as_mut_ptr(), value.as_mut_ptr());
            if cnt != 0 {
                if cnt == 1 {
                    value[0] = 0;
                }
                let np = name.as_mut_ptr();
                let mut s = np.wrapping_add(strlen(np)).wrapping_sub(1);
                while s > np && isspace(*s as c_int) != 0 {
                    *s = 0;
                    s = s.wrapping_sub(1);
                }
                let vp = value.as_mut_ptr();
                let mut s = vp.wrapping_add(strlen(vp)).wrapping_sub(1);
                while s > vp && isspace(*s as c_int) != 0 {
                    *s = 0;
                    s = s.wrapping_sub(1);
                }
                report(section.as_ptr(), name.as_ptr(), value.as_ptr());
            }
        }
        // C: `fscanf(f, " ;%*[^\n]") != 0 || fscanf(f, " \n") != 0`. The second format
        // contains only whitespace directives, so it always returns 0 -- but it is
        // short-circuited away (and its whitespace not consumed) when the first is nonzero.
        let short_read = if ini_scan_comment(&mut s) != 0 {
            true
        } else {
            s.skip_whitespace();
            false
        };
        if short_read {
            crate::c_log_err(&format!("short read from {}!?\n", cstr_arg(filename)));
            fclose(f);
            return -1;
        }
    }

    fclose(f);
    0
}

// ============================================================================
// Names lists and key/value lists (NAMES_LIST / KEY_VALUE_LIST macros)
// ============================================================================
const fn n20(s: &[u8]) -> [c_char; 20] {
    let mut a = [0 as c_char; 20];
    let mut i = 0;
    while i < s.len() {
        a[i] = s[i] as c_char;
        i += 1;
    }
    a
}

const fn kv(s: &[u8], v: c_int) -> key_value_type {
    key_value_type {
        key: n20(s),
        value: v,
    }
}

// NAMES_LIST(use_hardware_acceleration_names, {"false", "true", "default"});
static use_hardware_acceleration_names: [[c_char; 20]; 3] =
    [n20(b"false"), n20(b"true"), n20(b"default")];
static mut use_hardware_acceleration_names_list: names_list_type = names_list_type {
    type_: 0,
    __bindgen_anon_1: names_list_type__bindgen_ty_1 {
        names: names_list_type__bindgen_ty_1__bindgen_ty_1 {
            data: &use_hardware_acceleration_names as *const [[c_char; 20]; 3]
                as *const [[c_char; 20]; 0],
            count: 3,
        },
    },
};

// NAMES_LIST(level_type_names, {"dungeon", "palace"});
static level_type_names: [[c_char; 20]; 2] = [n20(b"dungeon"), n20(b"palace")];
static mut level_type_names_list: names_list_type = names_list_type {
    type_: 0,
    __bindgen_anon_1: names_list_type__bindgen_ty_1 {
        names: names_list_type__bindgen_ty_1__bindgen_ty_1 {
            data: &level_type_names as *const [[c_char; 20]; 2] as *const [[c_char; 20]; 0],
            count: 2,
        },
    },
};

// KEY_VALUE_LIST(guard_type_names, {{"none", -1}, {"guard", 0}, {"fat", 1}, {"skel", 2}, {"vizier", 3}, {"shadow", 4}});
static guard_type_names: [key_value_type; 6] = [
    kv(b"none", -1),
    kv(b"guard", 0),
    kv(b"fat", 1),
    kv(b"skel", 2),
    kv(b"vizier", 3),
    kv(b"shadow", 4),
];
static mut guard_type_names_list: names_list_type = names_list_type {
    type_: 1,
    __bindgen_anon_1: names_list_type__bindgen_ty_1 {
        kv_pairs: names_list_type__bindgen_ty_1__bindgen_ty_2 {
            data: &guard_type_names as *const [key_value_type; 6] as *mut key_value_type,
            count: 6,
        },
    },
};

// NAMES_LIST(tile_type_names, { ... 31 entries ... });
static tile_type_names: [[c_char; 20]; 31] = [
    n20(b"empty"),
    n20(b"floor"),
    n20(b"spike"),
    n20(b"pillar"),
    n20(b"gate"),
    n20(b"stuck"),
    n20(b"closer"),
    n20(b"doortop_with_floor"),
    n20(b"bigpillar_bottom"),
    n20(b"bigpillar_top"),
    n20(b"potion"),
    n20(b"loose"),
    n20(b"doortop"),
    n20(b"mirror"),
    n20(b"debris"),
    n20(b"opener"),
    n20(b"level_door_left"),
    n20(b"level_door_right"),
    n20(b"chomper"),
    n20(b"torch"),
    n20(b"wall"),
    n20(b"skeleton"),
    n20(b"sword"),
    n20(b"balcony_left"),
    n20(b"balcony_right"),
    n20(b"lattice_pillar"),
    n20(b"lattice_down"),
    n20(b"lattice_small"),
    n20(b"lattice_left"),
    n20(b"lattice_right"),
    n20(b"torch_with_debris"),
];
static mut tile_type_names_list: names_list_type = names_list_type {
    type_: 0,
    __bindgen_anon_1: names_list_type__bindgen_ty_1 {
        names: names_list_type__bindgen_ty_1__bindgen_ty_1 {
            data: &tile_type_names as *const [[c_char; 20]; 31] as *const [[c_char; 20]; 0],
            count: 31,
        },
    },
};

// NAMES_LIST(scaling_type_names, {"sharp", "fuzzy", "blurry"});
static scaling_type_names: [[c_char; 20]; 3] = [n20(b"sharp"), n20(b"fuzzy"), n20(b"blurry")];
static mut scaling_type_names_list: names_list_type = names_list_type {
    type_: 0,
    __bindgen_anon_1: names_list_type__bindgen_ty_1 {
        names: names_list_type__bindgen_ty_1__bindgen_ty_1 {
            data: &scaling_type_names as *const [[c_char; 20]; 3] as *const [[c_char; 20]; 0],
            count: 3,
        },
    },
};

// NAMES_LIST(row_names, {"top", "middle", "bottom"});
static row_names: [[c_char; 20]; 3] = [n20(b"top"), n20(b"middle"), n20(b"bottom")];
static mut row_names_list: names_list_type = names_list_type {
    type_: 0,
    __bindgen_anon_1: names_list_type__bindgen_ty_1 {
        names: names_list_type__bindgen_ty_1__bindgen_ty_1 {
            data: &row_names as *const [[c_char; 20]; 3] as *const [[c_char; 20]; 0],
            count: 3,
        },
    },
};

// KEY_VALUE_LIST(direction_names, {{"left", dir_FF_left}, {"right", dir_0_right}});
static direction_names: [key_value_type; 2] = [
    kv(b"left", directions_dir_FF_left as c_int),
    kv(b"right", directions_dir_0_right as c_int),
];
static mut direction_names_list: names_list_type = names_list_type {
    type_: 1,
    __bindgen_anon_1: names_list_type__bindgen_ty_1 {
        kv_pairs: names_list_type__bindgen_ty_1__bindgen_ty_2 {
            data: &direction_names as *const [key_value_type; 2] as *mut key_value_type,
            count: 2,
        },
    },
};

// NAMES_LIST(entry_pose_names, {"turning", "falling", "running"});
static entry_pose_names: [[c_char; 20]; 3] = [n20(b"turning"), n20(b"falling"), n20(b"running")];
static mut entry_pose_names_list: names_list_type = names_list_type {
    type_: 0,
    __bindgen_anon_1: names_list_type__bindgen_ty_1 {
        names: names_list_type__bindgen_ty_1__bindgen_ty_1 {
            data: &entry_pose_names as *const [[c_char; 20]; 3] as *const [[c_char; 20]; 0],
            count: 3,
        },
    },
};

// KEY_VALUE_LIST(never_is_16, {{"Never", 16}}); is defined in src/sdl_rw_wrappers.c.
// 16 is higher than any level, so some options can be disabled by setting it to this value.

const INI_NO_VALID_NAME: c_int = -9999;

// ============================================================================
// ini_get_named_value + ini_process_* helpers
// ============================================================================
unsafe fn ini_get_named_value(value: *const c_char, value_names: *mut names_list_type) -> c_int {
    if !value_names.is_null() {
        if (*value_names).type_ == 0
        /*names list*/
        {
            let base_ptr = (*value_names).__bindgen_anon_1.names.data as *const c_char;
            let count = (*value_names).__bindgen_anon_1.names.count as c_int;
            for i in 0..count {
                let name = base_ptr.add((i as usize) * MAX_OPTION_VALUE_NAME_LENGTH as usize);
                if strcasecmp(value, name) == 0 {
                    return i;
                }
            }
        } else if (*value_names).type_ == 1
        /*key/value list*/
        {
            let count = (*value_names).__bindgen_anon_1.kv_pairs.count as c_int;
            for i in 0..count {
                let kv_pair = (*value_names).__bindgen_anon_1.kv_pairs.data.add(i as usize);
                if strcasecmp(value, (*kv_pair).key.as_ptr()) == 0 {
                    return (*kv_pair).value;
                }
            }
        }
    }
    INI_NO_VALID_NAME // failure
}

unsafe fn ini_process_boolean(
    curr_name: *const c_char,
    value: *const c_char,
    option_name: *const c_char,
    target: *mut byte,
) -> c_int {
    if strcasecmp(curr_name, option_name) == 0 {
        if strcasecmp(value, cs!("true")) == 0 {
            *target = 1;
        } else if strcasecmp(value, cs!("false")) == 0 {
            *target = 0;
        }
        return 1;
    }
    0
}

macro_rules! ini_process_numeric_func {
    ($fn_name:ident, $data_type:ty) => {
        unsafe fn $fn_name(
            curr_name: *const c_char,
            value: *const c_char,
            option_name: *const c_char,
            target: *mut $data_type,
            value_names: *mut names_list_type,
        ) -> c_int {
            if strcasecmp(curr_name, option_name) == 0 {
                if strcasecmp(value, cs!("default")) != 0 {
                    let named_value = ini_get_named_value(value, value_names);
                    // target may point into a packed struct (custom_options_type),
                    // so write through it unaligned.
                    let v: $data_type = if named_value == INI_NO_VALID_NAME {
                        strtoimax(value, core::ptr::null_mut(), 0) as $data_type
                    } else {
                        named_value as $data_type
                    };
                    core::ptr::write_unaligned(target, v);
                }
                return 1;
            }
            0
        }
    };
}
ini_process_numeric_func!(ini_process_word, word);
ini_process_numeric_func!(ini_process_short, c_short);
ini_process_numeric_func!(ini_process_byte, byte);
ini_process_numeric_func!(ini_process_sbyte, sbyte);
ini_process_numeric_func!(ini_process_int, c_int);

// ============================================================================
// global_ini_callback
// ============================================================================
unsafe extern "C" fn global_ini_callback(
    section: *const c_char,
    name: *const c_char,
    value: *const c_char,
) -> c_int {
    macro_rules! check_ini_section {
        ($s:expr) => {
            strcasecmp(section, cs!($s)) == 0
        };
    }
    macro_rules! process_word {
        ($opt:expr, $tgt:expr, $vn:expr) => {
            if ini_process_word(name, value, cs!($opt), $tgt, $vn) != 0 {
                return 1;
            }
        };
    }
    macro_rules! process_short {
        ($opt:expr, $tgt:expr, $vn:expr) => {
            if ini_process_short(name, value, cs!($opt), $tgt, $vn) != 0 {
                return 1;
            }
        };
    }
    macro_rules! process_byte {
        ($opt:expr, $tgt:expr, $vn:expr) => {
            if ini_process_byte(name, value, cs!($opt), $tgt, $vn) != 0 {
                return 1;
            }
        };
    }
    macro_rules! process_sbyte {
        ($opt:expr, $tgt:expr, $vn:expr) => {
            if ini_process_sbyte(name, value, cs!($opt), $tgt, $vn) != 0 {
                return 1;
            }
        };
    }
    macro_rules! process_int {
        ($opt:expr, $tgt:expr, $vn:expr) => {
            if ini_process_int(name, value, cs!($opt), $tgt, $vn) != 0 {
                return 1;
            }
        };
    }
    macro_rules! process_boolean {
        ($opt:expr, $tgt:expr) => {
            if ini_process_boolean(name, value, cs!($opt), $tgt) != 0 {
                return 1;
            }
        };
    }
    let null_names: *mut names_list_type = core::ptr::null_mut();

    if check_ini_section!("General") {
        process_boolean!("enable_pause_menu", addr_of_mut!(enable_pause_menu));
        if strcasecmp(name, cs!("mods_folder")) == 0 {
            if *value != 0 && strcasecmp(value, cs!("default")) != 0 {
                let mut __lf = [0 as c_char; POP_MAX_PATH as usize];
                let lf = locate_file_(value, __lf.as_mut_ptr(), POP_MAX_PATH as c_int);
                snprintf_check!(addr_of_mut!(mods_folder) as *mut c_char, POP_MAX_PATH as usize, "{}", cstr_arg(lf));
            }
            return 1;
        }
        process_boolean!("enable_copyprot", addr_of_mut!(enable_copyprot));
        process_boolean!("enable_music", addr_of_mut!(enable_music));
        process_boolean!("enable_fade", addr_of_mut!(enable_fade));
        process_boolean!("enable_flash", addr_of_mut!(enable_flash));
        process_boolean!("enable_text", addr_of_mut!(enable_text));
        process_boolean!("enable_info_screen", addr_of_mut!(enable_info_screen));
        process_boolean!("start_fullscreen", addr_of_mut!(start_fullscreen));
        process_word!("pop_window_width", addr_of_mut!(pop_window_width), null_names);
        process_word!("pop_window_height", addr_of_mut!(pop_window_height), null_names);
        process_byte!(
            "use_hardware_acceleration",
            addr_of_mut!(use_hardware_acceleration),
            addr_of_mut!(use_hardware_acceleration_names_list)
        );
        process_boolean!("use_correct_aspect_ratio", addr_of_mut!(use_correct_aspect_ratio));
        process_boolean!("use_integer_scaling", addr_of_mut!(use_integer_scaling));
        process_byte!("scaling_type", addr_of_mut!(scaling_type), addr_of_mut!(scaling_type_names_list));
        process_boolean!("enable_controller_rumble", addr_of_mut!(enable_controller_rumble));
        process_boolean!("joystick_only_horizontal", addr_of_mut!(joystick_only_horizontal));
        process_int!("joystick_threshold", addr_of_mut!(joystick_threshold), null_names);

        if strcasecmp(name, cs!("levelset")) == 0 {
            if *value == 0
                || strcasecmp(value, cs!("original")) == 0
                || strcasecmp(value, cs!("default")) == 0
            {
                use_custom_levelset = 0;
            } else {
                use_custom_levelset = 1;
                strcpy(addr_of_mut!(levelset_name) as *mut c_char, value);
            }
            return 1;
        }

        process_boolean!("always_use_original_music", addr_of_mut!(always_use_original_music));
        process_boolean!("always_use_original_graphics", addr_of_mut!(always_use_original_graphics));

        if strcasecmp(name, cs!("gamecontrollerdb_file")) == 0 {
            if *value != 0 {
                let mut __lf = [0 as c_char; POP_MAX_PATH as usize];
                let lf = locate_file_(value, __lf.as_mut_ptr(), POP_MAX_PATH as c_int);
                snprintf_check!(addr_of_mut!(gamecontrollerdb_file) as *mut c_char, POP_MAX_PATH as usize, "{}", cstr_arg(lf));
            }
            return 1;
        }
    }

    if check_ini_section!("AdditionalFeatures") {
        process_boolean!("enable_quicksave", addr_of_mut!(enable_quicksave));
        process_boolean!("enable_quicksave_penalty", addr_of_mut!(enable_quicksave_penalty));

        process_boolean!("enable_replay", addr_of_mut!(enable_replay));

        if strcasecmp(name, cs!("replays_folder")) == 0 {
            if *value != 0 && strcasecmp(value, cs!("default")) != 0 {
                let mut __lf = [0 as c_char; POP_MAX_PATH as usize];
                let lf = locate_file_(value, __lf.as_mut_ptr(), POP_MAX_PATH as c_int);
                snprintf_check!(addr_of_mut!(replays_folder) as *mut c_char, POP_MAX_PATH as usize, "{}", cstr_arg(lf));
            }
            return 1;
        }
        process_boolean!("enable_lighting", addr_of_mut!(enable_lighting));
    }

    if check_ini_section!("Enhancements") {
        if strcasecmp(name, cs!("use_fixes_and_enhancements")) == 0 {
            if strcasecmp(value, cs!("true")) == 0 {
                use_fixes_and_enhancements = 1;
            } else if strcasecmp(value, cs!("false")) == 0 {
                use_fixes_and_enhancements = 0;
            } else if strcasecmp(value, cs!("prompt")) == 0 {
                use_fixes_and_enhancements = 2;
            }
            return 1;
        }
        process_boolean!("enable_crouch_after_climbing", addr_of_mut!(fixes_saved.enable_crouch_after_climbing));
        process_boolean!("enable_freeze_time_during_end_music", addr_of_mut!(fixes_saved.enable_freeze_time_during_end_music));
        process_boolean!("enable_remember_guard_hp", addr_of_mut!(fixes_saved.enable_remember_guard_hp));
        process_boolean!("fix_gate_sounds", addr_of_mut!(fixes_saved.fix_gate_sounds));
        process_boolean!("fix_two_coll_bug", addr_of_mut!(fixes_saved.fix_two_coll_bug));
        process_boolean!("fix_infinite_down_bug", addr_of_mut!(fixes_saved.fix_infinite_down_bug));
        process_boolean!("fix_gate_drawing_bug", addr_of_mut!(fixes_saved.fix_gate_drawing_bug));
        process_boolean!("fix_bigpillar_climb", addr_of_mut!(fixes_saved.fix_bigpillar_climb));
        process_boolean!("fix_jump_distance_at_edge", addr_of_mut!(fixes_saved.fix_jump_distance_at_edge));
        process_boolean!("fix_edge_distance_check_when_climbing", addr_of_mut!(fixes_saved.fix_edge_distance_check_when_climbing));
        process_boolean!("fix_painless_fall_on_guard", addr_of_mut!(fixes_saved.fix_painless_fall_on_guard));
        process_boolean!("fix_wall_bump_triggers_tile_below", addr_of_mut!(fixes_saved.fix_wall_bump_triggers_tile_below));
        process_boolean!("fix_stand_on_thin_air", addr_of_mut!(fixes_saved.fix_stand_on_thin_air));
        process_boolean!("fix_press_through_closed_gates", addr_of_mut!(fixes_saved.fix_press_through_closed_gates));
        process_boolean!("fix_grab_falling_speed", addr_of_mut!(fixes_saved.fix_grab_falling_speed));
        process_boolean!("fix_skeleton_chomper_blood", addr_of_mut!(fixes_saved.fix_skeleton_chomper_blood));
        process_boolean!("fix_move_after_drink", addr_of_mut!(fixes_saved.fix_move_after_drink));
        process_boolean!("fix_loose_left_of_potion", addr_of_mut!(fixes_saved.fix_loose_left_of_potion));
        process_boolean!("fix_guard_following_through_closed_gates", addr_of_mut!(fixes_saved.fix_guard_following_through_closed_gates));
        process_boolean!("fix_safe_landing_on_spikes", addr_of_mut!(fixes_saved.fix_safe_landing_on_spikes));
        process_boolean!("fix_glide_through_wall", addr_of_mut!(fixes_saved.fix_glide_through_wall));
        process_boolean!("fix_drop_through_tapestry", addr_of_mut!(fixes_saved.fix_drop_through_tapestry));
        process_boolean!("fix_land_against_gate_or_tapestry", addr_of_mut!(fixes_saved.fix_land_against_gate_or_tapestry));
        process_boolean!("fix_unintended_sword_strike", addr_of_mut!(fixes_saved.fix_unintended_sword_strike));
        process_boolean!("fix_retreat_without_leaving_room", addr_of_mut!(fixes_saved.fix_retreat_without_leaving_room));
        process_boolean!("fix_running_jump_through_tapestry", addr_of_mut!(fixes_saved.fix_running_jump_through_tapestry));
        process_boolean!("fix_push_guard_into_wall", addr_of_mut!(fixes_saved.fix_push_guard_into_wall));
        process_boolean!("fix_jump_through_wall_above_gate", addr_of_mut!(fixes_saved.fix_jump_through_wall_above_gate));
        process_boolean!("fix_chompers_not_starting", addr_of_mut!(fixes_saved.fix_chompers_not_starting));
        process_boolean!("fix_feather_interrupted_by_leveldoor", addr_of_mut!(fixes_saved.fix_feather_interrupted_by_leveldoor));
        process_boolean!("fix_offscreen_guards_disappearing", addr_of_mut!(fixes_saved.fix_offscreen_guards_disappearing));
        process_boolean!("fix_move_after_sheathe", addr_of_mut!(fixes_saved.fix_move_after_sheathe));
        process_boolean!("fix_hidden_floors_during_flashing", addr_of_mut!(fixes_saved.fix_hidden_floors_during_flashing));
        process_boolean!("fix_hang_on_teleport", addr_of_mut!(fixes_saved.fix_hang_on_teleport));
        process_boolean!("fix_exit_door", addr_of_mut!(fixes_saved.fix_exit_door));
        process_boolean!("fix_quicksave_during_feather", addr_of_mut!(fixes_saved.fix_quicksave_during_feather));
        process_boolean!("fix_caped_prince_sliding_through_gate", addr_of_mut!(fixes_saved.fix_caped_prince_sliding_through_gate));
        process_boolean!("fix_doortop_disabling_guard", addr_of_mut!(fixes_saved.fix_doortop_disabling_guard));
        process_boolean!("enable_super_high_jump", addr_of_mut!(fixes_saved.enable_super_high_jump));
        process_boolean!("fix_jumping_over_guard", addr_of_mut!(fixes_saved.fix_jumping_over_guard));
        process_boolean!("fix_drop_2_rooms_climbing_loose_tile", addr_of_mut!(fixes_saved.fix_drop_2_rooms_climbing_loose_tile));
        process_boolean!("fix_falling_through_floor_during_sword_strike", addr_of_mut!(fixes_saved.fix_falling_through_floor_during_sword_strike));
        process_boolean!("enable_jump_grab", addr_of_mut!(fixes_saved.enable_jump_grab));
        process_boolean!("fix_register_quick_input", addr_of_mut!(fixes_saved.fix_register_quick_input));
        process_boolean!("fix_turn_running_near_wall", addr_of_mut!(fixes_saved.fix_turn_running_near_wall));
        process_boolean!("fix_feather_fall_affects_guards", addr_of_mut!(fixes_saved.fix_feather_fall_affects_guards));
        process_boolean!("fix_one_hp_stops_blinking", addr_of_mut!(fixes_saved.fix_one_hp_stops_blinking));
        process_boolean!("fix_dead_floating_in_air", addr_of_mut!(fixes_saved.fix_dead_floating_in_air));
    }

    if check_ini_section!("CustomGameplay") {
        process_boolean!("use_custom_options", addr_of_mut!(use_custom_options));
        process_word!("start_minutes_left", addr_of_mut!(custom_saved.start_minutes_left), null_names);
        process_word!("start_ticks_left", addr_of_mut!(custom_saved.start_ticks_left), null_names);
        process_word!("start_hitp", addr_of_mut!(custom_saved.start_hitp), null_names);
        process_word!("max_hitp_allowed", addr_of_mut!(custom_saved.max_hitp_allowed), null_names);
        process_word!("saving_allowed_first_level", addr_of_mut!(custom_saved.saving_allowed_first_level), addr_of_mut!(never_is_16_list));
        process_word!("saving_allowed_last_level", addr_of_mut!(custom_saved.saving_allowed_last_level), addr_of_mut!(never_is_16_list));
        process_boolean!("start_upside_down", addr_of_mut!(custom_saved.start_upside_down));
        process_boolean!("start_in_blind_mode", addr_of_mut!(custom_saved.start_in_blind_mode));
        process_word!("copyprot_level", addr_of_mut!(custom_saved.copyprot_level), addr_of_mut!(never_is_16_list));
        process_byte!("drawn_tile_top_level_edge", addr_of_mut!(custom_saved.drawn_tile_top_level_edge), addr_of_mut!(tile_type_names_list));
        process_byte!("drawn_tile_left_level_edge", addr_of_mut!(custom_saved.drawn_tile_left_level_edge), addr_of_mut!(tile_type_names_list));
        process_byte!("level_edge_hit_tile", addr_of_mut!(custom_saved.level_edge_hit_tile), addr_of_mut!(tile_type_names_list));
        process_boolean!("allow_triggering_any_tile", addr_of_mut!(custom_saved.allow_triggering_any_tile));
        process_boolean!("enable_wda_in_palace", addr_of_mut!(custom_saved.enable_wda_in_palace));

        // Options that change the hard-coded color palette ('vga_color_0', ...)
        let prefix = cs!("vga_color_");
        let prefix_len: usize = 10; // sizeof("vga_color_")-1
        let mut ini_palette_color: c_int = -1;
        if strncasecmp(name, prefix, prefix_len) == 0
            && sscanf_leading_int(name.add(prefix_len), addr_of_mut!(ini_palette_color)) == 1
        {
            if !(ini_palette_color >= 0 && ini_palette_color <= 15) {
                return 0;
            }

            let mut rgb = [0u8; 3];
            if strcasecmp(value, cs!("default")) != 0 {
                // Parse an rgb string with three entries like "255, 255, 255"
                let mut start = value as *mut c_char;
                let mut end = value as *mut c_char;
                let mut i = 0;
                while i < 3 && *end != 0 {
                    rgb[i] = strtol(start, addr_of_mut!(end), 0) as u8;
                    while *end == b',' as c_char || *end == b' ' as c_char {
                        end = end.add(1);
                    }
                    start = end;
                    i += 1;
                }
            }
            let palette_color = addr_of_mut!(custom_saved.vga_palette[ini_palette_color as usize]);
            (*palette_color).r = rgb[0] / 4; // palette uses values 0..63, not 0..255
            (*palette_color).g = rgb[1] / 4;
            (*palette_color).b = rgb[2] / 4;
            return 1;
        }
        process_word!("first_level", addr_of_mut!(custom_saved.first_level), null_names);
        process_boolean!("skip_title", addr_of_mut!(custom_saved.skip_title));
        process_word!("shift_L_allowed_until_level", addr_of_mut!(custom_saved.shift_L_allowed_until_level), addr_of_mut!(never_is_16_list));
        process_word!("shift_L_reduced_minutes", addr_of_mut!(custom_saved.shift_L_reduced_minutes), null_names);
        process_word!("shift_L_reduced_ticks", addr_of_mut!(custom_saved.shift_L_reduced_ticks), null_names);
        process_word!("demo_hitp", addr_of_mut!(custom_saved.demo_hitp), null_names);
        process_word!("demo_end_room", addr_of_mut!(custom_saved.demo_end_room), null_names);
        process_word!("intro_music_level", addr_of_mut!(custom_saved.intro_music_level), addr_of_mut!(never_is_16_list));
        process_word!("have_sword_from_level", addr_of_mut!(custom_saved.have_sword_from_level), addr_of_mut!(never_is_16_list));
        process_word!("checkpoint_level", addr_of_mut!(custom_saved.checkpoint_level), addr_of_mut!(never_is_16_list));
        process_sbyte!("checkpoint_respawn_dir", addr_of_mut!(custom_saved.checkpoint_respawn_dir), addr_of_mut!(direction_names_list));
        process_byte!("checkpoint_respawn_room", addr_of_mut!(custom_saved.checkpoint_respawn_room), null_names);
        process_byte!("checkpoint_respawn_tilepos", addr_of_mut!(custom_saved.checkpoint_respawn_tilepos), null_names);
        process_byte!("checkpoint_clear_tile_room", addr_of_mut!(custom_saved.checkpoint_clear_tile_room), null_names);
        process_byte!("checkpoint_clear_tile_col", addr_of_mut!(custom_saved.checkpoint_clear_tile_col), null_names);
        process_byte!("checkpoint_clear_tile_row", addr_of_mut!(custom_saved.checkpoint_clear_tile_row), addr_of_mut!(row_names_list));
        process_word!("skeleton_level", addr_of_mut!(custom_saved.skeleton_level), addr_of_mut!(never_is_16_list));
        process_byte!("skeleton_room", addr_of_mut!(custom_saved.skeleton_room), null_names);
        process_byte!("skeleton_trigger_column_1", addr_of_mut!(custom_saved.skeleton_trigger_column_1), null_names);
        process_byte!("skeleton_trigger_column_2", addr_of_mut!(custom_saved.skeleton_trigger_column_2), null_names);
        process_byte!("skeleton_column", addr_of_mut!(custom_saved.skeleton_column), null_names);
        process_byte!("skeleton_row", addr_of_mut!(custom_saved.skeleton_row), addr_of_mut!(row_names_list));
        process_boolean!("skeleton_require_open_level_door", addr_of_mut!(custom_saved.skeleton_require_open_level_door));
        process_byte!("skeleton_skill", addr_of_mut!(custom_saved.skeleton_skill), null_names);
        process_byte!("skeleton_reappear_room", addr_of_mut!(custom_saved.skeleton_reappear_room), null_names);
        process_byte!("skeleton_reappear_x", addr_of_mut!(custom_saved.skeleton_reappear_x), null_names);
        process_byte!("skeleton_reappear_row", addr_of_mut!(custom_saved.skeleton_reappear_row), addr_of_mut!(row_names_list));
        process_byte!("skeleton_reappear_dir", addr_of_mut!(custom_saved.skeleton_reappear_dir), addr_of_mut!(direction_names_list));
        process_word!("mirror_level", addr_of_mut!(custom_saved.mirror_level), addr_of_mut!(never_is_16_list));
        process_byte!("mirror_room", addr_of_mut!(custom_saved.mirror_room), null_names);
        process_byte!("mirror_column", addr_of_mut!(custom_saved.mirror_column), null_names);
        process_byte!("mirror_row", addr_of_mut!(custom_saved.mirror_row), addr_of_mut!(row_names_list));
        process_byte!("mirror_tile", addr_of_mut!(custom_saved.mirror_tile), addr_of_mut!(tile_type_names_list));
        process_boolean!("show_mirror_image", addr_of_mut!(custom_saved.show_mirror_image));

        process_byte!("shadow_steal_level", addr_of_mut!(custom_saved.shadow_steal_level), addr_of_mut!(never_is_16_list));
        process_byte!("shadow_steal_room", addr_of_mut!(custom_saved.shadow_steal_room), null_names);
        process_byte!("shadow_step_level", addr_of_mut!(custom_saved.shadow_step_level), addr_of_mut!(never_is_16_list));
        process_byte!("shadow_step_room", addr_of_mut!(custom_saved.shadow_step_room), null_names);

        process_word!("falling_exit_level", addr_of_mut!(custom_saved.falling_exit_level), addr_of_mut!(never_is_16_list));
        process_byte!("falling_exit_room", addr_of_mut!(custom_saved.falling_exit_room), null_names);
        process_word!("falling_entry_level", addr_of_mut!(custom_saved.falling_entry_level), addr_of_mut!(never_is_16_list));
        process_byte!("falling_entry_room", addr_of_mut!(custom_saved.falling_entry_room), null_names);
        process_word!("mouse_level", addr_of_mut!(custom_saved.mouse_level), addr_of_mut!(never_is_16_list));
        process_byte!("mouse_room", addr_of_mut!(custom_saved.mouse_room), null_names);
        process_word!("mouse_delay", addr_of_mut!(custom_saved.mouse_delay), null_names);
        process_byte!("mouse_object", addr_of_mut!(custom_saved.mouse_object), null_names);
        process_byte!("mouse_start_x", addr_of_mut!(custom_saved.mouse_start_x), null_names);
        process_word!("loose_tiles_level", addr_of_mut!(custom_saved.loose_tiles_level), addr_of_mut!(never_is_16_list));
        process_byte!("loose_tiles_room_1", addr_of_mut!(custom_saved.loose_tiles_room_1), null_names);
        process_byte!("loose_tiles_room_2", addr_of_mut!(custom_saved.loose_tiles_room_2), null_names);
        process_byte!("loose_tiles_first_tile", addr_of_mut!(custom_saved.loose_tiles_first_tile), null_names);
        process_byte!("loose_tiles_last_tile", addr_of_mut!(custom_saved.loose_tiles_last_tile), null_names);
        process_word!("jaffar_victory_level", addr_of_mut!(custom_saved.jaffar_victory_level), addr_of_mut!(never_is_16_list));
        process_byte!("jaffar_victory_flash_time", addr_of_mut!(custom_saved.jaffar_victory_flash_time), null_names);
        process_word!("hide_level_number_from_level", addr_of_mut!(custom_saved.hide_level_number_from_level), addr_of_mut!(never_is_16_list));
        process_byte!("level_13_level_number", addr_of_mut!(custom_saved.level_13_level_number), null_names);
        process_word!("victory_stops_time_level", addr_of_mut!(custom_saved.victory_stops_time_level), addr_of_mut!(never_is_16_list));
        process_word!("win_level", addr_of_mut!(custom_saved.win_level), addr_of_mut!(never_is_16_list));
        process_byte!("win_room", addr_of_mut!(custom_saved.win_room), null_names);
        process_byte!("loose_floor_delay", addr_of_mut!(custom_saved.loose_floor_delay), null_names);
        process_byte!("base_speed", addr_of_mut!(custom_saved.base_speed), null_names);
        process_byte!("fight_speed", addr_of_mut!(custom_saved.fight_speed), null_names);
        process_byte!("chomper_speed", addr_of_mut!(custom_saved.chomper_speed), null_names);
        process_boolean!("no_mouse_in_ending", addr_of_mut!(custom_saved.no_mouse_in_ending));
    } // end of section [CustomGameplay]

    // [Level 1], etc.
    let mut ini_level: c_int = -1;
    if strncasecmp(section, cs!("Level "), 6) == 0
        && sscanf_leading_int(section.add(6), addr_of_mut!(ini_level)) == 1
    {
        if ini_level >= 0 && ini_level <= 15 {
            process_byte!("level_type", addr_of_mut!(custom_saved.tbl_level_type[ini_level as usize]), addr_of_mut!(level_type_names_list));
            process_word!("level_color", addr_of_mut!(custom_saved.tbl_level_color[ini_level as usize]), null_names);
            process_short!("guard_type", addr_of_mut!(custom_saved.tbl_guard_type[ini_level as usize]), addr_of_mut!(guard_type_names_list));
            process_byte!("guard_hp", addr_of_mut!(custom_saved.tbl_guard_hp[ini_level as usize]), null_names);

            let mut cutscene_index: byte = 0xFF;
            if ini_process_byte(name, value, cs!("cutscene"), addr_of_mut!(cutscene_index), null_names) == 1 {
                if (cutscene_index as usize) < 16 {
                    *addr_of_mut!(custom_saved.tbl_cutscenes_by_index[ini_level as usize]) = cutscene_index;
                }
                return 1;
            }

            process_byte!("entry_pose", addr_of_mut!(custom_saved.tbl_entry_pose[ini_level as usize]), addr_of_mut!(entry_pose_names_list));
            process_sbyte!("seamless_exit", addr_of_mut!(custom_saved.tbl_seamless_exit[ini_level as usize]), null_names);
        } else {
            crate::c_log(&format!("Warning: Invalid section [Level {}] in the INI!\n", ini_level));
        }
    }

    // [Skill 0], etc.
    let mut ini_skill: c_int = -1;
    if strncasecmp(section, cs!("Skill "), 6) == 0
        && sscanf_leading_int(section.add(6), addr_of_mut!(ini_skill)) == 1
    {
        if ini_skill >= 0 && ini_skill < NUM_GUARD_SKILLS as c_int {
            process_word!("strikeprob", addr_of_mut!(custom_saved.strikeprob[ini_skill as usize]), null_names);
            process_word!("restrikeprob", addr_of_mut!(custom_saved.restrikeprob[ini_skill as usize]), null_names);
            process_word!("blockprob", addr_of_mut!(custom_saved.blockprob[ini_skill as usize]), null_names);
            process_word!("impblockprob", addr_of_mut!(custom_saved.impblockprob[ini_skill as usize]), null_names);
            process_word!("advprob", addr_of_mut!(custom_saved.advprob[ini_skill as usize]), null_names);
            process_word!("refractimer", addr_of_mut!(custom_saved.refractimer[ini_skill as usize]), null_names);
            process_word!("extrastrength", addr_of_mut!(custom_saved.extrastrength[ini_skill as usize]), null_names);
        } else {
            crate::c_log(&format!("Warning: Invalid section [Skill {}] in the INI!\n", ini_skill));
        }
    }

    0
}

// Callback for a mod-specific INI configuration.
unsafe extern "C" fn mod_ini_callback(
    section: *const c_char,
    name: *const c_char,
    value: *const c_char,
) -> c_int {
    if strcasecmp(section, cs!("Enhancements")) == 0
        || strcasecmp(section, cs!("CustomGameplay")) == 0
        || strncasecmp(section, cs!("Level "), 6) == 0
        || strcasecmp(name, cs!("enable_copyprot")) == 0
        || strcasecmp(name, cs!("enable_quicksave")) == 0
        || strcasecmp(name, cs!("enable_quicksave_penalty")) == 0
    {
        global_ini_callback(section, name, value);
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn set_options_to_default() {
    enable_pause_menu = 1;
    enable_copyprot = 0;
    enable_music = 1;
    enable_fade = 1;
    enable_flash = 1;
    enable_text = 1;
    enable_info_screen = 1;
    start_fullscreen = 0;
    use_hardware_acceleration = 2;
    use_correct_aspect_ratio = 0;
    use_integer_scaling = 0;
    scaling_type = 0;
    enable_controller_rumble = 1;
    joystick_only_horizontal = 1;
    joystick_threshold = 8000;
    enable_quicksave = 1;
    enable_quicksave_penalty = 1;
    enable_replay = 1;
    enable_lighting = 0;
    // By default, all the fixes are used, unless otherwise specified.
    memset(addr_of_mut!(fixes_saved) as *mut c_void, 1, size_of::<fixes_options_type>());
    custom_saved = custom_defaults;
    turn_fixes_and_enhancements_on_off(0);
    turn_custom_options_on_off(0);
}

#[no_mangle]
pub unsafe extern "C" fn load_global_options() {
    set_options_to_default();
    let mut __lf = [0 as c_char; POP_MAX_PATH as usize];
    ini_load(
        locate_file_(cs!("SDLPoP.ini"), __lf.as_mut_ptr(), POP_MAX_PATH as c_int),
        global_ini_callback,
    ); // global configuration
    load_dos_exe_modifications(cs!(".")); // read PRINCE.EXE in the current working directory
}

#[no_mangle]
pub unsafe extern "C" fn check_mod_param() {
    // The 'mod' command line argument can override the levelset choice in SDLPoP.ini
    let mod_param = check_param(cs!("mod"));
    if !mod_param.is_null() {
        use_custom_levelset = 1;
        memset(addr_of_mut!(levelset_name) as *mut c_void, 0, POP_MAX_PATH as usize);
        snprintf_check!(addr_of_mut!(levelset_name) as *mut c_char, POP_MAX_PATH as usize, "{}", cstr_arg(mod_param));
    }
}

const DOS_10_PACKED: c_int = 0;
const DOS_10_UNPACKED: c_int = 1;
const DOS_13_PACKED: c_int = 2;
const DOS_13_UNPACKED: c_int = 3;
const DOS_14_PACKED: c_int = 4;
const DOS_14_UNPACKED: c_int = 5;

#[no_mangle]
pub unsafe extern "C" fn read_exe_bytes(
    dest: *mut c_void,
    nbytes: usize,
    exe_memory: *mut byte,
    exe_offset: c_int,
    exe_size: c_int,
) -> bool {
    if exe_offset < 0 {
        return false; // CusPop modification not available for the mod's EXE version.
    }
    if exe_offset < exe_size {
        memcpy(dest, exe_memory.add(exe_offset as usize) as *const c_void, nbytes);
    }
    true
}

#[no_mangle]
pub unsafe extern "C" fn identify_dos_exe_version(filesize: c_int) -> c_int {
    let mut dos_version: c_int = -1;
    match filesize {
        123335 => dos_version = DOS_10_PACKED,
        125115 => dos_version = DOS_13_PACKED,
        110855 => dos_version = DOS_14_PACKED,
        129504 => dos_version = DOS_10_UNPACKED,
        129472 => dos_version = DOS_13_UNPACKED,
        115008 => dos_version = DOS_14_UNPACKED,
        _ => {}
    }
    dos_version
}

// Most process!() calls in the DOS-version-detection block below don't check read_ok
// afterward (faithful to the C original's fallback chain, which only inspects the result
// for a handful of fields) -- each assignment through the macro looks like a dead store to
// Rust's dataflow analysis even though it's intentional.
#[allow(unused_assignments)]
#[no_mangle]
pub unsafe extern "C" fn load_dos_exe_modifications(folder_name: *const c_char) {
    let mut filename = [0 as c_char; POP_MAX_PATH as usize];
    snprintf_check!(filename.as_mut_ptr(), POP_MAX_PATH as usize, "{}/{}", cstr_arg(folder_name), "PRINCE.EXE");
    let mut fp: *mut FILE = fopen(filename.as_ptr(), cs!("rb"));

    let mut dos_version: c_int = -1;
    let mut info: stat_t = core::mem::zeroed();
    if !fp.is_null() && fstat(fileno(fp), addr_of_mut!(info)) == 0 && info.st_size > 0 {
        dos_version = identify_dos_exe_version(info.st_size as c_int);
    } else {
        // PRINCE.EXE not found, try to search for other .EXE files in the same folder.
        let directory_listing = create_directory_listing_and_find_first_file(folder_name, cs!("exe"));
        if !directory_listing.is_null() {
            loop {
                let current_filename = get_current_filename_from_directory_listing(directory_listing);
                snprintf_check!(filename.as_mut_ptr(), POP_MAX_PATH as usize, "{}/{}", cstr_arg(folder_name), cstr_arg(current_filename));
                fp = fopen(filename.as_ptr(), cs!("rb"));
                if !fp.is_null() && fstat(fileno(fp), addr_of_mut!(info)) == 0 && info.st_size > 0 {
                    dos_version = identify_dos_exe_version(info.st_size as c_int);
                    if dos_version >= 0 {
                        break; // We found a DOS executable with the right size!
                    }
                    fclose(fp);
                    fp = core::ptr::null_mut();
                }
                if !find_next_file(directory_listing) {
                    break;
                }
            }
            close_directory_listing(directory_listing);
        }
    }

    if dos_version >= 0 {
        turn_custom_options_on_off(1);
        let exe_memory = malloc(info.st_size as usize) as *mut byte;
        if fread(exe_memory as *mut c_void, info.st_size as usize, 1, fp) != 1 {
            crate::c_log_err(&format!("Could not read {}!?\n", cstr_arg(filename.as_ptr())));
            fclose(fp);
            return;
        }

        let mut temp_bytes = [0u8; 64];
        let mut temp_word: word = 0;
        let mut read_ok: bool = false;

        macro_rules! process {
            ($x:expr, $nbytes:expr, $offsets:expr) => {{
                let offsets: [c_int; 6] = $offsets;
                let offset = offsets[dos_version as usize];
                read_ok = read_exe_bytes($x as *mut c_void, $nbytes, exe_memory, offset, info.st_size as c_int);
            }};
        }

        // Offsets and comparisons are derived from princehack.xml
        process!(addr_of_mut!(custom_saved.start_minutes_left), 2, [0x04a23, 0x060d3, 0x04ea3, 0x055e3, 0x0495f, 0x05a8f]);
        process!(addr_of_mut!(custom_saved.start_ticks_left), 2, [0x04a29, 0x060d9, 0x04ea9, 0x055e9, 0x04965, 0x05a95]);
        process!(addr_of_mut!(custom_saved.start_hitp), 2, [0x04a2f, 0x060df, 0x04eaf, 0x055ef, 0x0496b, 0x05a9b]);
        process!(addr_of_mut!(custom_saved.first_level), 2, [0x00707, 0x01db7, 0x007db, 0x00f1b, 0x0079f, 0x018cf]);
        process!(addr_of_mut!(custom_saved.max_hitp_allowed), 2, [0x013f1, 0x02aa1, 0x015ac, 0x01cec, 0x014a3, 0x025d3]);
        process!(addr_of_mut!(custom_saved.saving_allowed_first_level), 1, [0x007c8, 0x01e78, 0x008b4, 0x00ff4, 0x00878, 0x019a8]);
        if read_ok {
            custom_saved.saving_allowed_first_level =
                custom_saved.saving_allowed_first_level.wrapping_add(1);
        }
        process!(addr_of_mut!(custom_saved.saving_allowed_last_level), 1, [0x007cf, 0x01e7f, 0x008bb, 0x00ffb, 0x0087f, 0x019af]);
        if read_ok {
            custom_saved.saving_allowed_last_level =
                custom_saved.saving_allowed_last_level.wrapping_sub(1);
        }
        if dos_version == DOS_10_PACKED || dos_version == DOS_10_UNPACKED {
            static COMPARISON: [u8; 22] = [
                0xa3, 0x92, 0x4e, 0xa3, 0x5c, 0x40, 0xa3, 0x8e, 0x4e, 0xa2, 0x2a, 0x3d, 0xa2, 0x29,
                0x3d, 0xa3, 0xee, 0x42, 0xa2, 0x2e, 0x3d, 0x98,
            ];
            process!(addr_of_mut!(temp_bytes), COMPARISON.len(), [0x04c9b, 0x0634b, -1, -1, -1, -1]);
            custom_saved.start_upside_down = (memcmp(
                addr_of!(temp_bytes) as *const c_void,
                COMPARISON.as_ptr() as *const c_void,
                COMPARISON.len(),
            ) != 0) as byte;
        }
        process!(addr_of_mut!(custom_saved.start_in_blind_mode), 1, [0x04e46, 0x064f6, 0x052ce, 0x05a0e, 0x04d8a, 0x05eba]);
        process!(addr_of_mut!(custom_saved.copyprot_level), 2, [0x1aaeb, 0x1c62e, 0x1b89b, 0x1c49e, 0x17c3d, 0x18e18]);
        process!(addr_of_mut!(custom_saved.drawn_tile_top_level_edge), 1, [0x0a1f0, 0x0b8a0, 0x0a69c, 0x0addc, 0x0a158, 0x0b288]);
        process!(addr_of_mut!(custom_saved.drawn_tile_left_level_edge), 1, [0x0a26b, 0x0b91b, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.level_edge_hit_tile), 1, [0x06f02, 0x085b2, -1, -1, -1, -1]);
        process!(addr_of_mut!(temp_bytes), 2, [0x9111, 0xA7C1, 0x95BE, 0x9CFE, 0x907A, 0xA1AA]); // allow triggering any tile
        if read_ok {
            custom_saved.allow_triggering_any_tile = ((temp_bytes[0] == 0x75 && temp_bytes[1] == 0x13)
                || (temp_bytes[0] == 0x90 && temp_bytes[1] == 0x90)) as byte; // used in Micro Palace
        }
        process!(addr_of_mut!(temp_bytes), 1, [0x0a7bb, 0x0be6b, 0x0ac67, 0x0b3a7, 0x0a723, 0x0b853]); // enable WDA in palace
        if read_ok {
            custom_saved.enable_wda_in_palace = (temp_bytes[0] != 116) as byte;
        }
        process!(addr_of_mut!(custom_saved.tbl_level_type), 16, [0x1acea, 0x1c842, 0x1b9ae, 0x1c5c6, 0x17d4c, 0x18f3c]);
        process!(addr_of_mut!(custom_saved.tbl_guard_hp), 16, [0x1b8a8, 0x1d46a, 0x1c6c5, 0x1d35c, 0x18a97, 0x19d06]);
        process!(addr_of_mut!(custom_saved.tbl_guard_type), 2 * 16, [-1, 0x1c964, -1, 0x1c702, -1, 0x1905e]);
        process!(addr_of_mut!(custom_saved.vga_palette), size_of::<rgb_type>() * 16, [0x1d141, 0x1f136, 0x1df5e, 0x1f02a, 0x1a335, 0x1b9de]);
        process!(addr_of_mut!(temp_word), 2, [0x003e2, 0x01a92, 0x0046b, 0x00bab, 0x00455, 0x01585]); // titles skipping
        if read_ok {
            custom_saved.skip_title = (temp_word != 63558) as byte;
        }
        process!(addr_of_mut!(custom_saved.shift_L_allowed_until_level), 1, [0x0085c, 0x01f0c, 0x00955, 0x01095, 0x00919, 0x01a49]);
        if read_ok {
            custom_saved.shift_L_allowed_until_level =
                custom_saved.shift_L_allowed_until_level.wrapping_add(1);
        }
        process!(addr_of_mut!(custom_saved.shift_L_reduced_minutes), 2, [0x008ad, 0x01f5d, 0x00991, 0x010d1, 0x00955, 0x01a85]);
        process!(addr_of_mut!(custom_saved.shift_L_reduced_ticks), 2, [0x008b3, 0x01f63, 0x00997, 0x010d7, 0x0095b, 0x01a8b]);
        // TODO: cutscenes
        // TODO: color variations
        process!(addr_of_mut!(custom_saved.demo_hitp), 1, [0x04c28, 0x062d8, 0x050b0, 0x057f0, 0x04b6c, 0x05c9c]);
        process!(addr_of_mut!(custom_saved.demo_end_room), 1, [0x00b40, 0x021f0, 0x00c25, 0x01365, 0x00be9, 0x01d19]);
        process!(addr_of_mut!(custom_saved.intro_music_level), 1, [0x04c37, 0x062e7, 0x050bf, 0x057ff, 0x04b7b, 0x05cab]);
        process!(addr_of_mut!(temp_bytes), 1, [0x04b29, 0x061d9, 0x04fa9, 0x056e9, 0x04a65, 0x05b95]); // where the kid will have the sword
        if read_ok {
            custom_saved.have_sword_from_level = if temp_bytes[0] == 0xEB { 16 /*never*/ } else { 2 };
        }
        process!(addr_of_mut!(custom_saved.checkpoint_level), 1, [0x04b9e, 0x0624e, 0x05026, 0x05766, 0x04ae2, 0x05c12]);
        process!(addr_of_mut!(custom_saved.checkpoint_respawn_dir), 1, [0x04bac, 0x0625c, 0x05034, 0x05774, 0x04af0, 0x05c20]);
        process!(addr_of_mut!(custom_saved.checkpoint_respawn_room), 1, [0x04bb1, 0x06261, 0x05039, 0x05779, 0x04af5, 0x05c25]);
        process!(addr_of_mut!(custom_saved.checkpoint_respawn_tilepos), 1, [0x04bb6, 0x06266, 0x0503e, 0x0577e, 0x04afa, 0x05c2a]);
        process!(addr_of_mut!(custom_saved.checkpoint_clear_tile_room), 1, [0x04bb8, 0x06268, 0x05040, 0x05780, 0x04afc, 0x05c2c]);
        process!(addr_of_mut!(custom_saved.checkpoint_clear_tile_col), 1, [0x04bbc, 0x0626c, 0x05044, 0x05784, 0x04b00, 0x05c30]);
        process!(addr_of_mut!(temp_word), 2, [0x04bbf, 0x0626f, 0x05047, 0x05787, 0x04b03, 0x05c33]); // row of the tile to clear
        if read_ok {
            if temp_word == 49195 {
                custom_saved.checkpoint_clear_tile_row = 0;
            } else if temp_word == 432 {
                custom_saved.checkpoint_clear_tile_row = 1;
            } else if temp_word == 688 {
                custom_saved.checkpoint_clear_tile_row = 2;
            }
        }
        process!(addr_of_mut!(custom_saved.skeleton_level), 1, [0x046a4, 0x05d54, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.skeleton_room), 1, [0x046b8, 0x05d68, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.skeleton_trigger_column_1), 1, [0x046cc, 0x05d7c, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.skeleton_trigger_column_2), 1, [0x046d3, 0x05d83, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.skeleton_column), 1, [0x046de, 0x05d8e, 0x04b5e, 0x0529e, 0x0461a, 0x0574a]);
        process!(addr_of_mut!(custom_saved.skeleton_row), 1, [0x046e2, 0x05d92, 0x04b62, 0x052a2, 0x0461e, 0x0574e]);
        process!(addr_of_mut!(temp_bytes), 1, [0x046c3, 0x05d73, -1, -1, -1, -1]);
        if read_ok {
            custom_saved.skeleton_require_open_level_door = (temp_bytes[0] != 0xEB) as byte;
        }
        process!(addr_of_mut!(custom_saved.skeleton_skill), 1, [0x0478f, 0x05e3f, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.skeleton_reappear_room), 1, [0x03b32, 0x051e2, 0x03fb2, 0x046f2, 0x03a6e, 0x04b9e]);
        process!(addr_of_mut!(custom_saved.skeleton_reappear_x), 1, [0x03b39, 0x051e9, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.skeleton_reappear_row), 1, [0x03b3e, 0x051ee, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.skeleton_reappear_dir), 1, [0x03b43, 0x051f3, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.mirror_level), 1, [0x08dc7, 0x0a477, 0x09274, 0x099b4, 0x08d30, 0x09e60]);
        process!(addr_of_mut!(custom_saved.mirror_room), 1, [0x08dcb, 0x0a47b, 0x09278, 0x099b8, 0x08d34, 0x09e64]);
        if read_ok {
            let mut opcode: byte = 0;
            process!(addr_of_mut!(opcode), 1, [0x08dcb + 2, 0x0a47b + 2, 0x09278 + 2, 0x099b8 + 2, 0x08d34 + 2, 0x09e64 + 2]);
            if opcode == 0x50 {
                // 0xA47A: B8 XX 00 50 50 where XX is room *and* column!
                custom_saved.mirror_column = custom_saved.mirror_room;
            } else if opcode == 0x6A {
                // 0xA47A: 68 RR 00 6A CC where RR is the room, CC is the column
                process!(addr_of_mut!(custom_saved.mirror_column), 1, [0x08dcb + 3, 0x0a47b + 3, 0x09278 + 3, 0x099b8 + 3, 0x08d34 + 3, 0x09e64 + 3]);
            }
        }
        process!(addr_of_mut!(temp_word), 2, [0x08dcf, 0x0a47f, 0x0927c, 0x099bc, 0x08d38, 0x09e68]); // mirror row
        if read_ok {
            if temp_word == 0xC02B {
                // 2B C0 = sub ax,ax
                custom_saved.mirror_row = 0;
            } else if temp_word == 0x01B0 {
                // B0 01 = mov al,1
                custom_saved.mirror_row = 1;
            } else if temp_word == 0x02B0 {
                // B0 02 = mov al,2
                custom_saved.mirror_row = 2;
            }
        }
        process!(addr_of_mut!(custom_saved.mirror_tile), 1, [0x08de3, 0x0a493, 0x09290, 0x099d0, 0x08d4c, 0x09e7c]);
        process!(addr_of_mut!(temp_bytes), 1, [0x051a2, 0x06852, 0x05636, 0x05d76, 0x050f2, 0x06222]);
        if read_ok {
            custom_saved.show_mirror_image = (temp_bytes[0] != 0xEB) as byte;
        }

        process!(addr_of_mut!(custom_saved.shadow_steal_level), 1, [-1, 0x5017, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.shadow_steal_room), 1, [-1, 0x5021, -1, -1, -1, -1]);

        process!(addr_of_mut!(custom_saved.shadow_step_level), 1, [-1, 0x4FE7, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.shadow_step_room), 1, [-1, 0x4FF1, -1, -1, -1, -1]);

        process!(addr_of_mut!(custom_saved.falling_exit_level), 1, [0x03eb2, 0x05562, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.falling_exit_room), 1, [0x03eb9, 0x05569, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.falling_entry_level), 1, [0x04cbd, 0x0636d, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.falling_entry_room), 1, [0x04cc4, 0x06374, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.mouse_level), 1, [0x05166, 0x06816, 0x055fa, 0x05d3a, 0x050b6, 0x061e6]);
        process!(addr_of_mut!(custom_saved.mouse_room), 1, [0x0516d, 0x0681d, 0x05601, 0x05d41, 0x050bd, 0x061ed]);
        process!(addr_of_mut!(custom_saved.mouse_delay), 2, [0x0517f, 0x0682f, 0x05613, 0x05d53, 0x050cf, 0x061ff]);
        process!(addr_of_mut!(custom_saved.mouse_object), 1, [0x054b3, 0x06b63, 0x05947, 0x06087, 0x05403, 0x06533]);
        process!(addr_of_mut!(custom_saved.mouse_start_x), 1, [0x054b8, 0x06b68, 0x0594c, 0x0608c, 0x05408, 0x06538]);
        {
            let mut level_: byte = 0;
            let mut room: byte = 0;
            process!(addr_of_mut!(level_), 1, [0x00b84, 0x02234, 0x00c6d, 0x013ad, 0x00c31, 0x01d61]); // seamless exit
            if read_ok {
                process!(addr_of_mut!(room), 1, [0x00b8b, 0x0223b, 0x00c74, 0x013b4, 0x00c38, 0x01d68]);
            }
            if read_ok && level_ < 16 {
                memset(addr_of_mut!(custom_saved.tbl_seamless_exit) as *mut c_void, -1, 16);
                *addr_of_mut!(custom_saved.tbl_seamless_exit[level_ as usize]) = room as sbyte;
            }
        }
        process!(addr_of_mut!(custom_saved.loose_tiles_level), 1, [0x0120d, 0x028bd, -1, -1, 0x01358, 0x02488]);
        process!(addr_of_mut!(custom_saved.loose_tiles_room_1), 1, [0x01214, 0x028c4, -1, -1, 0x0135f, 0x0248f]);
        process!(addr_of_mut!(custom_saved.loose_tiles_room_2), 1, [0x0121b, 0x028cb, -1, -1, 0x01366, 0x02496]);
        process!(addr_of_mut!(custom_saved.loose_tiles_first_tile), 1, [0x0122e, 0x028de, -1, -1, 0x01379, 0x024a9]);
        process!(addr_of_mut!(custom_saved.loose_tiles_last_tile), 1, [0x0124d, 0x028fd, -1, -1, 0x01398, 0x024c8]);
        process!(addr_of_mut!(custom_saved.jaffar_victory_level), 1, [0x084b3, 0x09b63, 0x08963, 0x090a3, 0x0841f, 0x0954f]);
        process!(addr_of_mut!(custom_saved.jaffar_victory_flash_time), 2, [0x084c0, 0x09b70, 0x08970, 0x090b0, 0x0842c, 0x0955c]);
        process!(addr_of_mut!(custom_saved.hide_level_number_from_level), 2, [0x0c3d9, 0x0da89, 0x0c8cd, 0x0d00d, 0x0c389, 0x0d4b9]);
        process!(addr_of_mut!(temp_bytes), 1, [0x0c3d9, 0x0da89, 0x0c8cd, 0x0d00d, 0x0c389, 0x0d4b9]);
        if read_ok {
            custom_saved.level_13_level_number = if temp_bytes[0] == 0xEB { 13 } else { 12 };
        }
        process!(addr_of_mut!(custom_saved.victory_stops_time_level), 1, [0x0c2e0, 0x0d990, -1, -1, -1, -1]);
        process!(addr_of_mut!(custom_saved.win_level), 1, [0x011dc, 0x0288c, 0x01397, 0x01ad7, 0x01327, 0x02457]);
        process!(addr_of_mut!(custom_saved.win_room), 1, [0x011e3, 0x02893, 0x0139e, 0x01ade, 0x0132e, 0x0245e]);
        process!(addr_of_mut!(custom_saved.loose_floor_delay), 1, [0x9536, 0xABE6, -1, -1, -1, -1]);

        // guard skills
        process!(addr_of_mut!(custom_saved.strikeprob), 2 * NUM_GUARD_SKILLS as usize, [-1, 0x1D3C2, -1, 0x1D2B4, -1, 0x19C5E]);
        process!(addr_of_mut!(custom_saved.restrikeprob), 2 * NUM_GUARD_SKILLS as usize, [-1, 0x1D3DA, -1, 0x1D2CC, -1, 0x19C76]);
        process!(addr_of_mut!(custom_saved.blockprob), 2 * NUM_GUARD_SKILLS as usize, [-1, 0x1D3F2, -1, 0x1D2E4, -1, 0x19C8E]);
        process!(addr_of_mut!(custom_saved.impblockprob), 2 * NUM_GUARD_SKILLS as usize, [-1, 0x1D40A, -1, 0x1D2FC, -1, 0x19CA6]);
        process!(addr_of_mut!(custom_saved.advprob), 2 * NUM_GUARD_SKILLS as usize, [-1, 0x1D422, -1, 0x1D314, -1, 0x19CBE]);
        process!(addr_of_mut!(custom_saved.refractimer), 2 * NUM_GUARD_SKILLS as usize, [-1, 0x1D43A, -1, 0x1D32C, -1, 0x19CD6]);
        process!(addr_of_mut!(custom_saved.extrastrength), 2 * NUM_GUARD_SKILLS as usize, [-1, 0x1D452, -1, 0x1D344, -1, 0x19CEE]);

        // shadow's starting positions
        process!(addr_of_mut!(custom_saved.init_shad_6), 8, [0x1B8B8, 0x1D47A, 0x1C6D5, 0x1D36C, 0x18AA7, 0x19D16]);
        process!(addr_of_mut!(custom_saved.init_shad_5), 8, [0x1B8C0, 0x1D482, 0x1C6DD, 0x1D374, 0x18AAF, 0x19D1E]);
        process!(addr_of_mut!(custom_saved.init_shad_12), 8, [-1, 0x1D48A, -1, 0x1D37C, -1, 0x19D26]); // packed: trailing zero bytes compressed
        // automatic moves
        process!(addr_of_mut!(custom_saved.shad_drink_move), 8 * 4, [-1, 0x1D492, -1, 0x1D384, -1, 0x19D2E]); // packed: leading zero bytes compressed
        process!(addr_of_mut!(custom_saved.demo_moves), 25 * 4, [0x1B8EE, 0x1D4B2, 0x1C70B, 0x1D3A4, 0x18ADD, 0x19D4E]);

        // speeds
        process!(addr_of_mut!(custom_saved.base_speed), 1, [0x4F01, 0x65B1, 0x5389, 0x5AC9, 0x4E45, 0x5F75]);
        process!(addr_of_mut!(custom_saved.fight_speed), 1, [0x4EF9, 0x65A9, 0x5381, 0x5AC1, 0x4E3D, 0x5F6D]);
        process!(addr_of_mut!(custom_saved.chomper_speed), 1, [0x8BBD, 0xA26D, 0x906D, 0x97AD, 0x8B29, 0x9C59]);

        // Skip the mouse in the ending scene. Used in Christmas of Persia.
        process!(addr_of_mut!(temp_bytes), 2, [0x2B8C, 0x423C, 0x2FE4, 0x3724, 0x2B28, 0x3C58]);
        custom_saved.no_mouse_in_ending = (temp_bytes[0] == 0xEB && temp_bytes[1] == 0x27) as byte;

        free(exe_memory as *mut c_void);
    }

    if !fp.is_null() {
        fclose(fp);
    }
}

#[no_mangle]
pub unsafe extern "C" fn load_mod_options() {
    // load mod-specific INI configuration
    if use_custom_levelset != 0 {
        // find the folder containing the mod's files
        let mut folder_name = [0 as c_char; POP_MAX_PATH as usize];
        snprintf_check!(folder_name.as_mut_ptr(), POP_MAX_PATH as usize, "{}/{}", cstr_arg(addr_of!(mods_folder) as *const c_char), cstr_arg(addr_of!(levelset_name) as *const c_char));
        let mut __lf = [0 as c_char; POP_MAX_PATH as usize];
        let located_folder_name = locate_file_(folder_name.as_ptr(), __lf.as_mut_ptr(), POP_MAX_PATH as c_int);
        let mut ok = false;
        let mut info: stat_t = core::mem::zeroed();
        if stat(located_folder_name, addr_of_mut!(info)) == 0 {
            if S_ISDIR(info.st_mode) {
                // It's a directory
                ok = true;
                snprintf_check!(addr_of_mut!(mod_data_path) as *mut c_char, POP_MAX_PATH as usize, "{}", cstr_arg(located_folder_name));
                // Try to load PRINCE.EXE (DOS)
                load_dos_exe_modifications(located_folder_name);
                // Try to load mod.ini
                let mut mod_ini_filename = [0 as c_char; POP_MAX_PATH as usize];
                snprintf_check!(mod_ini_filename.as_mut_ptr(), POP_MAX_PATH as usize, "{}/{}", cstr_arg(located_folder_name), "mod.ini");
                if file_exists(mod_ini_filename.as_ptr()) {
                    // Nearly all mods would want to use custom options, so always allow them.
                    use_custom_options = 1;
                    ini_load(mod_ini_filename.as_ptr(), mod_ini_callback);
                }
            } else {
                crate::c_log(&format!("Could not load mod '{}' - not a directory\n", cstr_arg(addr_of!(levelset_name) as *const c_char)));
            }
        } else {
            crate::c_log(&format!("Mod '{}' not found\n", cstr_arg(addr_of!(levelset_name) as *const c_char)));
            let mut message = [0 as c_char; 256];
            snprintf_check!(message.as_mut_ptr(), 256usize, "Cannot find the mod '{}' in the mods folder.", cstr_arg(addr_of!(levelset_name) as *const c_char));
            show_dialog(message.as_ptr());
            if replaying != 0 {
                show_dialog(cs!("If the replay file restarts the level or advances to the next level, a wrong level will be loaded."));
            }
        }
        if !ok {
            use_custom_levelset = 0;
            *addr_of_mut!(levelset_name[0]) = 0;
        }
    }
    turn_fixes_and_enhancements_on_off(use_fixes_and_enhancements);
    turn_custom_options_on_off(use_custom_options);
}

#[cfg(test)]
mod ini_scan_tests {
    use super::*;
    use std::io::Write;

    extern "C" {
        fn ftell(stream: *mut FILE) -> c_long;
    }

    fn temp_file(data: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("sdlpop_ini_scan_{:p}_{}.tmp", data.as_ptr(), data.len()));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(data.as_bytes()).unwrap();
        path
    }

    unsafe fn open(path: &std::path::Path) -> *mut FILE {
        let c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        fopen(c.as_ptr(), b"r\0".as_ptr() as *const c_char)
    }

    /// Mirrors the instrumented C ini_load loop so its output can be diffed against glibc.
    fn trace(label: &str, data: &str) -> String {
        let path = temp_file(data);
        let out = trace_path(&format!("{}: {}", label, data), &path);
        let _ = std::fs::remove_file(&path);
        out
    }

    fn trace_path(label: &str, path: &std::path::Path) -> String {
        let mut out = String::new();
        unsafe {
            let f = open(path);
            assert!(!f.is_null());
            let mut name = [0 as c_char; 64];
            let mut value = [0 as c_char; 256];
            let mut section = [0 as c_char; 128];
            let mut s = IniStream { f, pushback: INI_EOF };
            let pos = |s: &IniStream| -> c_long {
                ftell(s.f) - if s.pushback != INI_EOF { 1 } else { 0 }
            };
            let eofflag = |s: &IniStream| -> c_int {
                if s.pushback != INI_EOF { 0 } else { feof(s.f) }
            };
            out.push_str(&format!("=== {}\n", label));
            let mut iter = 0;
            while !s.at_eof() {
                iter += 1;
                if iter > 12 { out.push_str("  LOOPGUARD\n"); break; }
                let p0 = pos(&s);
                let r1 = ini_scan_section(&mut s, section.as_mut_ptr());
                out.push_str(&format!("  it{} sec_scan={} pos {}->{} sec='{}'\n",
                    iter, r1, p0, pos(&s), cstr_arg(section.as_ptr())));
                if r1 != 1 {
                    let p1 = pos(&s);
                    let cnt = ini_scan_name_value(&mut s, name.as_mut_ptr(), value.as_mut_ptr());
                    out.push_str(&format!("       nv_scan={} pos {}->{} name='{}' value='{}'\n",
                        cnt, p1, pos(&s), cstr_arg(name.as_ptr()), cstr_arg(value.as_ptr())));
                    if cnt != 0 {
                        if cnt == 1 { value[0] = 0; }
                        let np = name.as_mut_ptr();
                        let mut q = np.wrapping_add(strlen(np)).wrapping_sub(1);
                        while q > np && isspace(*q as c_int) != 0 { *q = 0; q = q.wrapping_sub(1); }
                        let vp = value.as_mut_ptr();
                        let mut q = vp.wrapping_add(strlen(vp)).wrapping_sub(1);
                        while q > vp && isspace(*q as c_int) != 0 { *q = 0; q = q.wrapping_sub(1); }
                        out.push_str(&format!("       REPORT [{}] '{}' = '{}'\n",
                            cstr_arg(section.as_ptr()), cstr_arg(name.as_ptr()), cstr_arg(value.as_ptr())));
                    }
                }
                let p2 = pos(&s);
                let a = ini_scan_comment(&mut s);
                let p3 = pos(&s);
                let b = if a != 0 { 0 } else { s.skip_whitespace(); 0 };
                out.push_str(&format!("       tail a={} b={} pos {}->{}->{} eof={}\n",
                    a, b, p2, p3, pos(&s), eofflag(&s)));
                if a != 0 || b != 0 { out.push_str("       SHORT READ -> return -1\n"); break; }
            }
            out.push_str(&format!("  loop exited, eof={}\n", eofflag(&s)));
            fclose(f);
        }
        out
    }

    fn trace_tail(data: &str) -> String {
        unsafe {
            let path = temp_file(data);
            let f = open(&path);
            let mut s = IniStream { f, pushback: INI_EOF };
            let pos = |s: &IniStream| -> c_long { ftell(s.f) - if s.pushback != INI_EOF { 1 } else { 0 } };
            let a = ini_scan_comment(&mut s);
            let p = pos(&s);
            let b = if a != 0 { 0 } else { s.skip_whitespace(); 0 };
            let eofflag = if s.pushback != INI_EOF { 0 } else { feof(s.f) };
            let out = format!("tail({:<12}) a={:2} b={:2} pos={}->{} eof={}\n", data, a, b, p, pos(&s), eofflag);
            fclose(f);
            let _ = std::fs::remove_file(&path);
            out
        }
    }

    fn trace_ival(text: &str) -> String {
        unsafe {
            let c = std::ffi::CString::new(text).unwrap();
            let mut v: c_int = 0xDEAD;
            let r = sscanf_leading_int(c.as_ptr(), &mut v);
            format!("sscanf({:<24}) r={} v={}\n", text, r, v)
        }
    }

    fn trace_sect(data: &str) -> String {
        unsafe {
            let path = temp_file(data);
            let f = open(&path);
            let mut s = IniStream { f, pushback: INI_EOF };
            let pos = |s: &IniStream| -> c_long { ftell(s.f) - if s.pushback != INI_EOF { 1 } else { 0 } };
            let mut section = [b'?' as c_char; 128];
            section[127] = 0;
            let r = ini_scan_section(&mut s, section.as_mut_ptr());
            let eofflag = if s.pushback != INI_EOF { 0 } else { feof(s.f) };
            let out = format!("sect r={} pos={} len={} eof={}\n", r, pos(&s), strlen(section.as_ptr()), eofflag);
            fclose(f);
            let _ = std::fs::remove_file(&path);
            out
        }
    }

    /// Golden output captured from glibc (see the C probe in the commit message): the
    /// hand-written scanners must return the same counts and leave the stream in the same
    /// position as the fscanf/sscanf calls they replace.
    #[test]
    fn micro_scans_match_glibc() {
        let mut all = String::new();
        for d in [";", ";\n", ";x", ";x\n", "", "\n", "   ", "k", "  ;c\n  "] {
            all.push_str(&trace_tail(d));
        }
        for d in ["3", "  -7abc", "+12", "", "abc", "99999999999999", "-99999999999999",
                  "2147483648", "0x10", "  \t 5", "5 "] {
            all.push_str(&trace_ival(d));
        }
        let mut big = vec![b'a'; 302];
        big[0] = b'['; big[300] = b']'; big[301] = b'\n';
        all.push_str(&trace_sect(std::str::from_utf8(&big).unwrap()));
        all.push_str(&trace_sect("["));
        all.push_str(&trace_sect("[]"));
        assert_eq!(all, "tail(;           ) a=-1 b= 0 pos=1->1 eof=1\ntail(;\n          ) a= 0 b= 0 pos=1->2 eof=1\ntail(;x          ) a= 0 b= 0 pos=2->2 eof=1\ntail(;x\n         ) a= 0 b= 0 pos=2->3 eof=1\ntail(            ) a=-1 b= 0 pos=0->0 eof=1\ntail(\n           ) a=-1 b= 0 pos=1->1 eof=1\ntail(            ) a=-1 b= 0 pos=3->3 eof=1\ntail(k           ) a= 0 b= 0 pos=0->0 eof=0\ntail(  ;c\n       ) a= 0 b= 0 pos=4->7 eof=1\nsscanf(3                       ) r=1 v=3\nsscanf(  -7abc                 ) r=1 v=-7\nsscanf(+12                     ) r=1 v=12\nsscanf(                        ) r=-1 v=57005\nsscanf(abc                     ) r=0 v=57005\nsscanf(99999999999999          ) r=1 v=276447231\nsscanf(-99999999999999         ) r=1 v=-276447231\nsscanf(2147483648              ) r=1 v=-2147483648\nsscanf(0x10                    ) r=1 v=0\nsscanf(  \t 5                   ) r=1 v=5\nsscanf(5                       ) r=1 v=5\nsect r=1 pos=128 len=127 eof=0\nsect r=-1 pos=1 len=127 eof=1\nsect r=0 pos=1 len=127 eof=0\n");
    }

    /// Golden output captured from glibc for the full ini_load parse loop.
    #[test]
    fn ini_load_loop_matches_glibc() {
        let cases: &[(&str, &str)] = &[
            ("simple", "[Section]\nkey = value\n"),
            ("no trailing nl", "[Section]\nkey = value"),
            ("comment", "; a comment\n[S]\nk = v ; trailing comment\n"),
            ("empty", ""),
            ("blank lines", "\n\n[S]\n\nk=v\n\n"),
            ("no value", "key =\n"),
            ("no equals", "keyonly\n"),
            ("bracket then bracket", "[]\nk=v\n"),
            ("unterminated section", "[Sec\nk=v\n"),
            ("semicolon in section", "[Se;c]\nk=v\n"),
            ("spaces", "   [ S ]  \n  k  =  v  \n"),
            ("crlf", "[S]\r\nk = v\r\n"),
        ];
        let mut all = String::new();
        for (l, d) in cases { all.push_str(&trace(l, d)); }
        assert_eq!(all, "=== simple: [Section]\nkey = value\n\n  it1 sec_scan=1 pos 0->10 sec='Section'\n       tail a=0 b=0 pos 10->10->10 eof=0\n  it2 sec_scan=0 pos 10->10 sec='Section'\n       nv_scan=2 pos 10->21 name='key ' value='value'\n       REPORT [Section] 'key' = 'value'\n       tail a=-1 b=0 pos 21->22->22 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== no trailing nl: [Section]\nkey = value\n  it1 sec_scan=1 pos 0->10 sec='Section'\n       tail a=0 b=0 pos 10->10->10 eof=0\n  it2 sec_scan=0 pos 10->10 sec='Section'\n       nv_scan=2 pos 10->21 name='key ' value='value'\n       REPORT [Section] 'key' = 'value'\n       tail a=-1 b=0 pos 21->21->21 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== comment: ; a comment\n[S]\nk = v ; trailing comment\n\n  it1 sec_scan=0 pos 0->0 sec=''\n       nv_scan=0 pos 0->0 name='' value=''\n       tail a=0 b=0 pos 0->11->12 eof=0\n  it2 sec_scan=1 pos 12->16 sec='S'\n       tail a=0 b=0 pos 16->16->16 eof=0\n  it3 sec_scan=0 pos 16->16 sec='S'\n       nv_scan=2 pos 16->22 name='k ' value='v '\n       REPORT [S] 'k' = 'v'\n       tail a=0 b=0 pos 22->40->41 eof=1\n  loop exited, eof=1\n=== empty: \n  it1 sec_scan=-1 pos 0->0 sec=''\n       nv_scan=-1 pos 0->0 name='' value=''\n       REPORT [] '' = ''\n       tail a=-1 b=0 pos 0->0->0 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== blank lines: \n\n[S]\n\nk=v\n\n\n  it1 sec_scan=0 pos 0->0 sec=''\n       nv_scan=1 pos 0->7 name='[S]' value=''\n       REPORT [] '[S]' = ''\n       tail a=0 b=0 pos 7->7->7 eof=0\n  it2 sec_scan=0 pos 7->7 sec=''\n       nv_scan=2 pos 7->10 name='k' value='v'\n       REPORT [] 'k' = 'v'\n       tail a=-1 b=0 pos 10->12->12 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== no value: key =\n\n  it1 sec_scan=0 pos 0->0 sec=''\n       nv_scan=1 pos 0->6 name='key ' value=''\n       REPORT [] 'key' = ''\n       tail a=-1 b=0 pos 6->6->6 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== no equals: keyonly\n\n  it1 sec_scan=0 pos 0->0 sec=''\n       nv_scan=1 pos 0->8 name='keyonly' value=''\n       REPORT [] 'keyonly' = ''\n       tail a=-1 b=0 pos 8->8->8 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== bracket then bracket: []\nk=v\n\n  it1 sec_scan=0 pos 0->1 sec=''\n       nv_scan=1 pos 1->3 name=']' value=''\n       REPORT [] ']' = ''\n       tail a=0 b=0 pos 3->3->3 eof=0\n  it2 sec_scan=0 pos 3->3 sec=''\n       nv_scan=2 pos 3->6 name='k' value='v'\n       REPORT [] 'k' = 'v'\n       tail a=-1 b=0 pos 6->7->7 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== unterminated section: [Sec\nk=v\n\n  it1 sec_scan=1 pos 0->4 sec='Sec'\n       tail a=0 b=0 pos 4->5->5 eof=0\n  it2 sec_scan=0 pos 5->5 sec='Sec'\n       nv_scan=2 pos 5->8 name='k' value='v'\n       REPORT [Sec] 'k' = 'v'\n       tail a=-1 b=0 pos 8->9->9 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== semicolon in section: [Se;c]\nk=v\n\n  it1 sec_scan=1 pos 0->3 sec='Se'\n       tail a=0 b=0 pos 3->6->7 eof=0\n  it2 sec_scan=0 pos 7->7 sec='Se'\n       nv_scan=2 pos 7->10 name='k' value='v'\n       REPORT [Se] 'k' = 'v'\n       tail a=-1 b=0 pos 10->11->11 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== spaces:    [ S ]  \n  k  =  v  \n\n  it1 sec_scan=0 pos 0->0 sec=''\n       nv_scan=1 pos 0->13 name='[ S ]  ' value=''\n       REPORT [] '[ S ]' = ''\n       tail a=0 b=0 pos 13->13->13 eof=0\n  it2 sec_scan=0 pos 13->13 sec=''\n       nv_scan=2 pos 13->22 name='k  ' value='v  '\n       REPORT [] 'k' = 'v'\n       tail a=-1 b=0 pos 22->23->23 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n=== crlf: [S]\r\nk = v\r\n\n  it1 sec_scan=1 pos 0->5 sec='S'\n       tail a=0 b=0 pos 5->5->5 eof=0\n  it2 sec_scan=0 pos 5->5 sec='S'\n       nv_scan=2 pos 5->11 name='k ' value='v\r'\n       REPORT [S] 'k' = 'v'\n       tail a=-1 b=0 pos 11->12->12 eof=1\n       SHORT READ -> return -1\n  loop exited, eof=1\n");
    }
}
