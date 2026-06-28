// In-game pause menu — ported from menu.c (USE_MENU).
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]
#![allow(unused_assignments)]

use std::os::raw::{c_char, c_int, c_short, c_void};
use core::ptr::{addr_of, addr_of_mut, null, null_mut};
use super::*;

// ============================================================================
// SDL / libc externs not present in bindings.rs
// ============================================================================
extern "C" {
    fn SDL_GetMouseState(x: *mut c_int, y: *mut c_int) -> u32;
    fn SDL_SetWindowFullscreen(window: *mut SDL_Window, flags: u32) -> c_int;
    fn SDL_RenderGetScale(renderer: *mut SDL_Renderer, scaleX: *mut f32, scaleY: *mut f32);
    fn SDL_RenderGetLogicalSize(renderer: *mut SDL_Renderer, w: *mut c_int, h: *mut c_int);
    fn SDL_RenderGetViewport(renderer: *mut SDL_Renderer, rect: *mut SDL_Rect);
    fn SDL_RenderSetIntegerScale(renderer: *mut SDL_Renderer, enable: c_int) -> c_int;
    fn SDL_GetScancodeName(scancode: u32) -> *const c_char;
    fn SDL_UpperBlit(src: *mut SDL_Surface, srcrect: *const SDL_Rect, dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int;
    fn SDL_MapRGBA(fmt: *const SDL_PixelFormat, r: u8, g: u8, b: u8, a: u8) -> u32;
    fn SDL_FillRect(dst: *mut SDL_Surface, rect: *const SDL_Rect, color: u32) -> c_int;
    fn SDL_GetWindowFlags(window: *mut SDL_Window) -> u32;
    fn SDL_ShowCursor(toggle: c_int) -> c_int;
    fn SDL_GetPerformanceFrequency() -> u64;
    fn SDL_GetPerformanceCounter() -> u64;
    fn SDL_Delay(ms: u32);
    fn SDL_SetColorKey(surface: *mut SDL_Surface, flag: c_int, key: u32) -> c_int;
    fn SDL_RWFromFile(file: *const c_char, mode: *const c_char) -> *mut SDL_RWops;
    fn SDL_RWwrite(ctx: *mut SDL_RWops, ptr: *const c_void, size: usize, n: usize) -> usize;
    fn SDL_RWread(ctx: *mut SDL_RWops, ptr: *mut c_void, size: usize, maxnum: usize) -> usize;
    fn SDL_RWclose(ctx: *mut SDL_RWops) -> c_int;

    fn snprintf(s: *mut c_char, n: usize, format: *const c_char, ...) -> c_int;
    fn strlen(s: *const c_char) -> usize;
    fn strncpy(dst: *mut c_char, src: *const c_char, n: usize) -> *mut c_char;
    fn strnlen(s: *const c_char, maxlen: usize) -> usize;
    fn strncmp(a: *const c_char, b: *const c_char, n: usize) -> c_int;
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn ftell(stream: *mut FILE) -> std::os::raw::c_long;
    fn fprintf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    static mut stderr: *mut FILE;

    // glibc stat (matches seg009.rs declaration).
    fn stat(path: *const c_char, buf: *mut stat_t) -> c_int;

    // never_is_16_list lives in sdl_rw_wrappers.rs
    pub static mut never_is_16_list: names_list_type;
}

// SDL_BlitSurface is a macro in SDL2 that expands to SDL_UpperBlit.
#[inline]
unsafe fn SDL_BlitSurface(src: *mut SDL_Surface, srcrect: *const SDL_Rect,
                           dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int {
    SDL_UpperBlit(src, srcrect, dst, dstrect)
}

// glibc x86-64 struct stat (144 bytes). We only read st_mtim.
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

macro_rules! cs {
    ($s:literal) => {
        concat!($s, "\0").as_ptr() as *const c_char
    };
}

// SDL scancodes (not emitted by bindgen)
const SDL_SCANCODE_A: c_int = 4;
const SDL_SCANCODE_Q: c_int = 20;
const SDL_SCANCODE_R: c_int = 21;
const SDL_SCANCODE_RETURN: c_int = 40;
const SDL_SCANCODE_ESCAPE: c_int = 41;
const SDL_SCANCODE_BACKSPACE: c_int = 42;
const SDL_SCANCODE_SPACE: c_int = 44;
const SDL_SCANCODE_F6: c_int = 63;
const SDL_SCANCODE_F9: c_int = 66;
const SDL_SCANCODE_HOME: c_int = 74;
const SDL_SCANCODE_PAGEUP: c_int = 75;
const SDL_SCANCODE_END: c_int = 77;
const SDL_SCANCODE_PAGEDOWN: c_int = 78;
const SDL_SCANCODE_RIGHT: c_int = 79;
const SDL_SCANCODE_LEFT: c_int = 80;
const SDL_SCANCODE_DOWN: c_int = 81;
const SDL_SCANCODE_UP: c_int = 82;

const WITH_CTRL: c_int = key_modifiers_WITH_CTRL as c_int;
const WITH_SHIFT: c_int = key_modifiers_WITH_SHIFT as c_int;
const KEYSTATE_HELD_I: c_int = KEYSTATE_HELD as c_int;

const SDL_TRUE: c_int = 1;
const SDL_FALSE: c_int = 0;
const SDL_ENABLE: c_int = 1;
const SDL_DISABLE: c_int = 0;
const SDL_WINDOW_FULLSCREEN_DESKTOP: u32 = 0x1001;

const SDL_CONTROLLER_AXIS_LEFTX: usize = 0;
const SDL_CONTROLLER_AXIS_LEFTY: usize = 1;

const SEEK_END: c_int = 2;
const SEEK_SET: c_int = 0;
const POP_MAX_PATH: usize = 256;

// const helper: build a fixed-size NUL-terminated char array from a byte string.
const fn cstr<const N: usize>(src: &[u8]) -> [c_char; N] {
    let mut a = [0 as c_char; N];
    let mut i = 0;
    while i < src.len() && i + 1 < N {
        a[i] = src[i] as c_char;
        i += 1;
    }
    a
}

// ============================================================================
// Hardcoded small font + arrowhead bitmaps (decoded from macros in menu.c).
// ============================================================================
macro_rules! mbit {
    (_) => { 0u8 };
    (1) => { 1u8 };
}
// BINARY_4(b7,b6,b5,b4) = (b4<<4)|(b5<<5)|(b6<<6)|(b7<<7)
macro_rules! b4 {
    ($a:tt,$b:tt,$c:tt,$d:tt) => {
        (mbit!($d) << 4) | (mbit!($c) << 5) | (mbit!($b) << 6) | (mbit!($a) << 7)
    };
}
// BINARY_8(b7,b6,b5,b4,b3,b2,b1,b0) = b0|b1<<1|...|b7<<7
macro_rules! b8 {
    ($a:tt,$b:tt,$c:tt,$d:tt,$e:tt,$f:tt,$g:tt,$h:tt) => {
        mbit!($h) | (mbit!($g) << 1) | (mbit!($f) << 2) | (mbit!($e) << 3)
            | (mbit!($d) << 4) | (mbit!($c) << 5) | (mbit!($b) << 6) | (mbit!($a) << 7)
    };
}

#[no_mangle]
pub static mut hc_small_font_data: [u8; 1242] = [
    // header: 32, 126, WORD(5), WORD(2), WORD(1), WORD(1)
    32, 126, 5,0, 2,0, 1,0, 1,0,
    // offsets (95 words, filled at runtime)
    0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,
    // space
    1,0,3,0,1,0, b4!(_,_,_,_),
    // !
    5,0,1,0,1,0, b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(_,_,_,_),b4!(1,_,_,_),
    // "
    5,0,3,0,1,0, b4!(1,_,1,_),b4!(1,_,1,_),b4!(_,_,_,_),b4!(_,_,_,_),b4!(_,_,_,_),
    // #
    5,0,5,0,1,0, b8!(_,1,_,1,_,_,_,_),b8!(1,1,1,1,1,_,_,_),b8!(_,1,_,1,_,_,_,_),b8!(1,1,1,1,1,_,_,_),b8!(_,1,_,1,_,_,_,_),
    // $
    6,0,3,0,1,0, b4!(_,1,_,_),b4!(1,1,1,_),b4!(1,1,_,_),b4!(_,1,1,_),b4!(1,1,1,_),b4!(_,1,_,_),
    // %
    5,0,6,0,1,0, b8!(_,_,_,_,_,_,_,_),b8!(1,1,_,_,1,_,_,_),b8!(1,1,_,1,_,_,_,_),b8!(_,_,1,_,1,1,_,_),b8!(_,1,_,_,1,1,_,_),
    // &
    5,0,5,0,1,0, b8!(_,1,1,_,_,_,_,_),b8!(_,1,1,_,_,_,_,_),b8!(1,1,1,_,1,_,_,_),b8!(1,_,_,1,_,_,_,_),b8!(_,1,1,_,1,_,_,_),
    // '
    2,0,1,0,1,0, b4!(1,_,_,_),b4!(1,_,_,_),
    // (
    5,0,3,0,1,0, b4!(_,1,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(_,1,_,_),
    // )
    5,0,3,0,1,0, b4!(_,1,_,_),b4!(_,_,1,_),b4!(_,_,1,_),b4!(_,_,1,_),b4!(_,1,_,_),
    // *
    4,0,5,0,1,0, b8!(_,_,_,_,_,_,_,_),b8!(1,_,1,_,1,_,_,_),b8!(_,1,1,1,_,_,_,_),b8!(1,_,1,_,1,_,_,_),
    // +
    4,0,3,0,1,0, b4!(_,_,_,_),b4!(_,1,_,_),b4!(1,1,1,_),b4!(_,1,_,_),
    // ,
    6,0,2,0,1,0, b4!(_,_,_,_),b4!(_,_,_,_),b4!(_,_,_,_),b4!(_,_,_,_),b4!(_,1,_,_),b4!(1,_,_,_),
    // -
    3,0,3,0,1,0, b4!(_,_,_,_),b4!(_,_,_,_),b4!(1,1,1,_),
    // .
    5,0,1,0,1,0, b4!(_,_,_,_),b4!(_,_,_,_),b4!(_,_,_,_),b4!(_,_,_,_),b4!(1,_,_,_),
    // /
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(_,_,_,1),b4!(_,_,1,_),b4!(_,1,_,_),b4!(1,_,_,_),
    // 0
    5,0,4,0,1,0, b4!(_,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,_),
    // 1
    5,0,2,0,1,0, b4!(1,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),
    // 2
    5,0,4,0,1,0, b4!(1,1,1,_),b4!(_,_,_,1),b4!(_,1,1,_),b4!(1,_,_,_),b4!(1,1,1,1),
    // 3
    5,0,4,0,1,0, b4!(1,1,1,_),b4!(_,_,_,1),b4!(_,1,1,_),b4!(_,_,_,1),b4!(1,1,1,_),
    // 4
    5,0,4,0,1,0, b4!(_,_,1,_),b4!(_,1,1,_),b4!(1,_,1,_),b4!(1,1,1,1),b4!(_,_,1,_),
    // 5
    5,0,4,0,1,0, b4!(1,1,1,1),b4!(1,_,_,_),b4!(1,1,1,_),b4!(_,_,_,1),b4!(1,1,1,_),
    // 6
    5,0,4,0,1,0, b4!(_,1,1,_),b4!(1,_,_,_),b4!(1,1,1,_),b4!(1,_,_,1),b4!(_,1,1,_),
    // 7
    5,0,4,0,1,0, b4!(1,1,1,1),b4!(_,_,_,1),b4!(_,_,1,_),b4!(_,1,_,_),b4!(_,1,_,_),
    // 8
    5,0,4,0,1,0, b4!(_,1,1,_),b4!(1,_,_,1),b4!(_,1,1,_),b4!(1,_,_,1),b4!(_,1,1,_),
    // 9
    5,0,4,0,1,0, b4!(_,1,1,_),b4!(1,_,_,1),b4!(_,1,1,1),b4!(_,_,_,1),b4!(_,1,1,_),
    // :
    5,0,1,0,1,0, b4!(_,_,_,_),b4!(1,_,_,_),b4!(_,_,_,_),b4!(_,_,_,_),b4!(1,_,_,_),
    // ;
    6,0,2,0,1,0, b4!(_,_,_,_),b4!(_,1,_,_),b4!(_,_,_,_),b4!(_,_,_,_),b4!(_,1,_,_),b4!(1,_,_,_),
    // <
    5,0,3,0,1,0, b4!(_,_,1,_),b4!(_,1,_,_),b4!(1,_,_,_),b4!(_,1,_,_),b4!(_,_,1,_),
    // =
    4,0,3,0,1,0, b4!(_,_,_,_),b4!(1,1,1,_),b4!(_,_,_,_),b4!(1,1,1,_),
    // >
    5,0,4,0,1,0, b4!(1,_,_,_),b4!(_,1,_,_),b4!(_,_,1,_),b4!(_,1,_,_),b4!(1,_,_,_),
    // ?
    5,0,4,0,1,0, b4!(_,1,1,_),b4!(1,_,_,1),b4!(_,_,1,_),b4!(_,_,_,_),b4!(_,_,1,_),
    // @
    6,0,4,0,1,0, b4!(_,1,1,_),b4!(1,_,_,1),b4!(1,_,1,1),b4!(1,_,1,1),b4!(1,_,_,_),b4!(_,1,1,_),
    // A
    5,0,4,0,1,0, b4!(_,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,1,1,1),b4!(1,_,_,1),
    // B
    5,0,4,0,1,0, b4!(1,1,1,_),b4!(1,_,_,1),b4!(1,1,1,_),b4!(1,_,_,1),b4!(1,1,1,_),
    // C
    5,0,4,0,1,0, b4!(_,1,1,1),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(_,1,1,1),
    // D
    5,0,4,0,1,0, b4!(1,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,1,1,_),
    // E
    5,0,4,0,1,0, b4!(1,1,1,1),b4!(1,_,_,_),b4!(1,1,1,1),b4!(1,_,_,_),b4!(1,1,1,1),
    // F
    5,0,4,0,1,0, b4!(1,1,1,1),b4!(1,_,_,_),b4!(1,1,1,1),b4!(1,_,_,_),b4!(1,_,_,_),
    // G
    5,0,4,0,1,0, b4!(_,1,1,1),b4!(1,_,_,_),b4!(1,_,1,1),b4!(1,_,_,1),b4!(_,1,1,1),
    // H
    5,0,4,0,1,0, b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,1,1,1),b4!(1,_,_,1),b4!(1,_,_,1),
    // I
    5,0,3,0,1,0, b4!(1,1,1,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(1,1,1,_),
    // J
    5,0,4,0,1,0, b4!(_,_,1,1),b4!(_,_,_,1),b4!(_,_,_,1),b4!(1,_,_,1),b4!(_,1,1,_),
    // K
    5,0,4,0,1,0, b4!(1,_,_,1),b4!(1,_,1,_),b4!(1,1,_,_),b4!(1,_,1,_),b4!(1,_,_,1),
    // L
    5,0,4,0,1,0, b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,1,1,1),
    // M
    5,0,5,0,1,0, b8!(1,_,_,_,1,_,_,_),b8!(1,1,_,1,1,_,_,_),b8!(1,_,1,_,1,_,_,_),b8!(1,_,_,_,1,_,_,_),b8!(1,_,_,_,1,_,_,_),
    // N
    5,0,4,0,1,0, b4!(1,_,_,1),b4!(1,1,_,1),b4!(1,_,1,1),b4!(1,_,_,1),b4!(1,_,_,1),
    // O
    5,0,4,0,1,0, b4!(_,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,_),
    // P
    5,0,4,0,1,0, b4!(1,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,1,1,_),b4!(1,_,_,_),
    // Q
    6,0,4,0,1,0, b4!(_,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,_),b4!(_,_,_,1),
    // R
    5,0,4,0,1,0, b4!(1,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,1,1,_),b4!(1,_,_,1),
    // S
    5,0,4,0,1,0, b4!(_,1,1,1),b4!(1,_,_,_),b4!(_,1,1,_),b4!(_,_,_,1),b4!(1,1,1,_),
    // T
    5,0,3,0,1,0, b4!(1,1,1,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),
    // U
    5,0,4,0,1,0, b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,_),
    // V
    5,0,4,0,1,0, b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,1,_),b4!(1,_,1,_),b4!(_,1,_,_),
    // W
    5,0,5,0,1,0, b8!(1,_,_,_,1,_,_,_),b8!(1,_,1,_,1,_,_,_),b8!(1,_,1,_,1,_,_,_),b8!(1,_,1,_,1,_,_,_),b8!(_,1,_,1,_,_,_,_),
    // X
    5,0,4,0,1,0, b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),
    // Y
    5,0,4,0,1,0, b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,1),b4!(_,_,_,1),b4!(_,1,1,_),
    // Z
    5,0,3,0,1,0, b4!(1,1,1,_),b4!(_,_,1,_),b4!(_,1,_,_),b4!(1,_,_,_),b4!(1,1,1,_),
    // [
    5,0,2,0,1,0, b4!(1,1,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,1,_,_),
    // backslash
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(1,_,_,_),b4!(_,1,_,_),b4!(_,_,1,_),b4!(_,_,_,1),
    // ]
    5,0,4,0,1,0, b4!(1,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(1,1,_,_),
    // ^
    2,0,3,0,1,0, b4!(_,1,_,_),b4!(1,_,1,_),
    // _
    5,0,3,0,1,0, b4!(_,_,_,_),b4!(_,_,_,_),b4!(_,_,_,_),b4!(_,_,_,_),b4!(1,1,1,_),
    // `
    2,0,2,0,1,0, b4!(1,_,_,_),b4!(_,1,_,_),
    // a
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(_,1,1,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,1),
    // b
    5,0,4,0,1,0, b4!(1,_,_,_),b4!(1,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,1,1,_),
    // c
    5,0,3,0,1,0, b4!(_,_,_,_),b4!(_,1,1,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(_,1,1,_),
    // d
    5,0,4,0,1,0, b4!(_,_,_,1),b4!(_,1,1,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,1),
    // e
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(_,1,1,_),b4!(1,_,1,1),b4!(1,1,_,_),b4!(_,1,1,1),
    // f
    5,0,3,0,1,0, b4!(_,_,1,_),b4!(_,1,_,_),b4!(1,1,1,_),b4!(_,1,_,_),b4!(_,1,_,_),
    // g
    7,0,4,0,1,0, b4!(_,_,_,_),b4!(_,1,1,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,1),b4!(_,_,_,1),b4!(_,1,1,_),
    // h
    5,0,4,0,1,0, b4!(1,_,_,_),b4!(1,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),
    // i
    5,0,1,0,1,0, b4!(1,_,_,_),b4!(_,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),
    // j
    7,0,2,0,1,0, b4!(_,1,_,_),b4!(_,_,_,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(1,_,_,_),
    // k
    5,0,4,0,1,0, b4!(1,_,_,_),b4!(1,_,_,1),b4!(1,_,1,_),b4!(1,1,1,_),b4!(1,_,_,1),
    // l
    5,0,1,0,1,0, b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),
    // m
    5,0,5,0,1,0, b8!(_,_,_,_,_,_,_,_),b8!(1,1,1,1,_,_,_,_),b8!(1,_,1,_,1,_,_,_),b8!(1,_,1,_,1,_,_,_),b8!(1,_,1,_,1,_,_,_),
    // n
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(1,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),
    // o
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(_,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,_),
    // p
    7,0,4,0,1,0, b4!(_,_,_,_),b4!(1,1,1,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,1,1,_),b4!(1,_,_,_),b4!(1,_,_,_),
    // q
    7,0,4,0,1,0, b4!(_,_,_,_),b4!(_,1,1,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,1),b4!(_,_,_,1),b4!(_,_,_,1),
    // r
    5,0,3,0,1,0, b4!(_,_,_,_),b4!(1,_,1,_),b4!(1,1,_,_),b4!(1,_,_,_),b4!(1,_,_,_),
    // s
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(_,1,1,1),b4!(1,1,_,_),b4!(_,_,1,1),b4!(1,1,1,_),
    // t
    5,0,3,0,1,0, b4!(_,1,_,_),b4!(1,1,1,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(_,_,1,_),
    // u
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,1),
    // v
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,1,_),b4!(_,1,_,_),
    // w
    5,0,5,0,1,0, b8!(_,_,_,_,_,_,_,_),b8!(1,_,1,_,1,_,_,_),b8!(1,_,1,_,1,_,_,_),b8!(_,1,_,1,_,_,_,_),b8!(_,1,_,1,_,_,_,_),
    // x
    5,0,3,0,1,0, b4!(_,_,_,_),b4!(1,_,1,_),b4!(_,1,_,_),b4!(_,1,_,_),b4!(1,_,1,_),
    // y
    7,0,4,0,1,0, b4!(_,_,_,_),b4!(1,_,_,1),b4!(1,_,_,1),b4!(1,_,_,1),b4!(_,1,1,1),b4!(_,_,_,1),b4!(_,1,1,_),
    // z
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(1,1,1,1),b4!(_,_,1,_),b4!(_,1,_,_),b4!(1,1,1,1),
    // {
    5,0,4,0,1,0, b4!(_,_,1,_),b4!(_,1,_,_),b4!(1,1,_,_),b4!(_,1,_,_),b4!(_,_,1,_),
    // |
    5,0,1,0,1,0, b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),b4!(1,_,_,_),
    // }
    5,0,4,0,1,0, b4!(1,_,_,_),b4!(_,1,_,_),b4!(_,1,1,_),b4!(_,1,_,_),b4!(1,_,_,_),
    // ~
    5,0,4,0,1,0, b4!(_,_,_,_),b4!(_,1,_,1),b4!(1,_,1,_),b4!(_,_,_,_),b4!(_,_,_,_),
];

static mut arrowhead_up_image_data: [u8; 10] = [
    4,0,7,0,1,0,
    b8!(_,_,_,1,_,_,_,_),
    b8!(_,_,1,1,1,_,_,_),
    b8!(_,1,1,1,1,1,_,_),
    b8!(1,1,1,1,1,1,1,_),
];
static mut arrowhead_down_image_data: [u8; 10] = [
    4,0,7,0,1,0,
    b8!(1,1,1,1,1,1,1,_),
    b8!(_,1,1,1,1,1,_,_),
    b8!(_,_,1,1,1,_,_,_),
    b8!(_,_,_,1,_,_,_,_),
];
static mut arrowhead_left_image_data: [u8; 11] = [
    5,0,3,0,1,0,
    b8!(_,_,1,_,_,_,_,_),
    b8!(_,1,1,_,_,_,_,_),
    b8!(1,1,1,_,_,_,_,_),
    b8!(_,1,1,_,_,_,_,_),
    b8!(_,_,1,_,_,_,_,_),
];
static mut arrowhead_right_image_data: [u8; 11] = [
    5,0,3,0,1,0,
    b8!(1,_,_,_,_,_,_,_),
    b8!(1,1,_,_,_,_,_,_),
    b8!(1,1,1,_,_,_,_,_),
    b8!(1,1,_,_,_,_,_,_),
    b8!(1,_,_,_,_,_,_,_),
];

static mut arrowhead_up_image: *mut image_type = null_mut();
static mut arrowhead_down_image: *mut image_type = null_mut();
static mut arrowhead_left_image: *mut image_type = null_mut();
static mut arrowhead_right_image: *mut image_type = null_mut();

unsafe fn load_arrowhead_images() {
    // Make a dummy palette for decode_image().
    let mut dat_pal: dat_pal_type = core::mem::zeroed();
    memset(addr_of_mut!(dat_pal) as *mut c_void, 0, core::mem::size_of::<dat_pal_type>());
    dat_pal.vga[1].r = 0x3F;
    dat_pal.vga[1].g = 0x3F;
    dat_pal.vga[1].b = 0x3F; // white
    if arrowhead_up_image.is_null() {
        arrowhead_up_image = decode_image(addr_of_mut!(arrowhead_up_image_data) as *mut image_data_type, addr_of_mut!(dat_pal));
    }
    if arrowhead_down_image.is_null() {
        arrowhead_down_image = decode_image(addr_of_mut!(arrowhead_down_image_data) as *mut image_data_type, addr_of_mut!(dat_pal));
    }
    if arrowhead_left_image.is_null() {
        arrowhead_left_image = decode_image(addr_of_mut!(arrowhead_left_image_data) as *mut image_data_type, addr_of_mut!(dat_pal));
    }
    if arrowhead_right_image.is_null() {
        arrowhead_right_image = decode_image(addr_of_mut!(arrowhead_right_image_data) as *mut image_data_type, addr_of_mut!(dat_pal));
    }
}
// ============================================================================
// Enums (as C-style sequential consts)
// ============================================================================
const MAX_MENU_ITEM_LENGTH: usize = 32;

#[repr(C)]
struct pause_menu_item_type {
    id: c_int,
    previous: *mut pause_menu_item_type,
    next: *mut pause_menu_item_type,
    required: *mut c_void,
    text: [c_char; MAX_MENU_ITEM_LENGTH],
}

// pause_menu_item_ids
const PAUSE_MENU_RESUME: c_int = 0;
const PAUSE_MENU_CHEATS: c_int = 1;
const PAUSE_MENU_SAVE_GAME: c_int = 2;
const PAUSE_MENU_LOAD_GAME: c_int = 3;
const PAUSE_MENU_RESTART_LEVEL: c_int = 4;
const PAUSE_MENU_SETTINGS: c_int = 5;
const PAUSE_MENU_RESTART_GAME: c_int = 6;
const PAUSE_MENU_QUIT_GAME: c_int = 7;
const SETTINGS_MENU_GENERAL: c_int = 8;
const SETTINGS_MENU_GAMEPLAY: c_int = 9;
const SETTINGS_MENU_VISUALS: c_int = 10;
const SETTINGS_MENU_MODS: c_int = 11;
const SETTINGS_MENU_LEVEL_CUSTOMIZATION: c_int = 12;
const SETTINGS_MENU_BACK: c_int = 13;
const SETTINGS_MENU_CONTROLS: c_int = 14;

// menu_dialog_ids
const DIALOG_NONE: c_int = 0;
const DIALOG_RESTORE_DEFAULT_SETTINGS: c_int = 1;
const DIALOG_CONFIRM_QUIT: c_int = 2;
const DIALOG_SELECT_LEVEL: c_int = 3;

// menu_setting_style_ids
const SETTING_STYLE_TOGGLE: c_int = 0;
const SETTING_STYLE_NUMBER: c_int = 1;
const SETTING_STYLE_TEXT_ONLY: c_int = 2;
const SETTING_STYLE_KEY: c_int = 3;

// menu_setting_number_type_ids
const SETTING_BYTE: u8 = 0;
const SETTING_SBYTE: u8 = 1;
const SETTING_WORD: u8 = 2;
const SETTING_SHORT: u8 = 3;
const SETTING_INT: u8 = 5;

// setting_ids (sequential, chained from 0)
const SETTING_RESET_ALL_SETTINGS: c_int = 0;
const SETTING_SHOW_MENU_ON_PAUSE: c_int = SETTING_RESET_ALL_SETTINGS + 1;
const SETTING_ENABLE_INFO_SCREEN: c_int = SETTING_SHOW_MENU_ON_PAUSE + 1;
const SETTING_ENABLE_SOUND: c_int = SETTING_ENABLE_INFO_SCREEN + 1;
const SETTING_ENABLE_MUSIC: c_int = SETTING_ENABLE_SOUND + 1;
const SETTING_ENABLE_CONTROLLER_RUMBLE: c_int = SETTING_ENABLE_MUSIC + 1;
const SETTING_JOYSTICK_THRESHOLD: c_int = SETTING_ENABLE_CONTROLLER_RUMBLE + 1;
const SETTING_JOYSTICK_ONLY_HORIZONTAL: c_int = SETTING_JOYSTICK_THRESHOLD + 1;
const SETTING_FULLSCREEN: c_int = SETTING_JOYSTICK_ONLY_HORIZONTAL + 1;
const SETTING_USE_HARDWARE_ACCELERATION: c_int = SETTING_FULLSCREEN + 1;
const SETTING_USE_CORRECT_ASPECT_RATIO: c_int = SETTING_USE_HARDWARE_ACCELERATION + 1;
const SETTING_USE_INTEGER_SCALING: c_int = SETTING_USE_CORRECT_ASPECT_RATIO + 1;
const SETTING_SCALING_TYPE: c_int = SETTING_USE_INTEGER_SCALING + 1;
const SETTING_ENABLE_FADE: c_int = SETTING_SCALING_TYPE + 1;
const SETTING_ENABLE_FLASH: c_int = SETTING_ENABLE_FADE + 1;
const SETTING_ENABLE_LIGHTING: c_int = SETTING_ENABLE_FLASH + 1;
const SETTING_ENABLE_CHEATS: c_int = SETTING_ENABLE_LIGHTING + 1;
const SETTING_ENABLE_COPYPROT: c_int = SETTING_ENABLE_CHEATS + 1;
const SETTING_ENABLE_QUICKSAVE: c_int = SETTING_ENABLE_COPYPROT + 1;
const SETTING_ENABLE_QUICKSAVE_PENALTY: c_int = SETTING_ENABLE_QUICKSAVE + 1;
const SETTING_ENABLE_REPLAY: c_int = SETTING_ENABLE_QUICKSAVE_PENALTY + 1;
const SETTING_USE_FIXES_AND_ENHANCEMENTS: c_int = SETTING_ENABLE_REPLAY + 1;
const SETTING_ENABLE_CROUCH_AFTER_CLIMBING: c_int = SETTING_USE_FIXES_AND_ENHANCEMENTS + 1;
const SETTING_ENABLE_FREEZE_TIME_DURING_END_MUSIC: c_int = SETTING_ENABLE_CROUCH_AFTER_CLIMBING + 1;
const SETTING_ENABLE_REMEMBER_GUARD_HP: c_int = SETTING_ENABLE_FREEZE_TIME_DURING_END_MUSIC + 1;
const SETTING_FIX_GATE_SOUNDS: c_int = SETTING_ENABLE_REMEMBER_GUARD_HP + 1;
const SETTING_TWO_COLL_BUG: c_int = SETTING_FIX_GATE_SOUNDS + 1;
const SETTING_FIX_INFINITE_DOWN_BUG: c_int = SETTING_TWO_COLL_BUG + 1;
const SETTING_FIX_GATE_DRAWING_BUG: c_int = SETTING_FIX_INFINITE_DOWN_BUG + 1;
const SETTING_FIX_BIGPILLAR_CLIMB: c_int = SETTING_FIX_GATE_DRAWING_BUG + 1;
const SETTING_FIX_JUMP_DISTANCE_AT_EDGE: c_int = SETTING_FIX_BIGPILLAR_CLIMB + 1;
const SETTING_FIX_EDGE_DISTANCE_CHECK_WHEN_CLIMBING: c_int = SETTING_FIX_JUMP_DISTANCE_AT_EDGE + 1;
const SETTING_FIX_PAINLESS_FALL_ON_GUARD: c_int = SETTING_FIX_EDGE_DISTANCE_CHECK_WHEN_CLIMBING + 1;
const SETTING_FIX_WALL_BUMP_TRIGGERS_TILE_BELOW: c_int = SETTING_FIX_PAINLESS_FALL_ON_GUARD + 1;
const SETTING_FIX_STAND_ON_THIN_AIR: c_int = SETTING_FIX_WALL_BUMP_TRIGGERS_TILE_BELOW + 1;
const SETTING_FIX_PRESS_THROUGH_CLOSED_GATES: c_int = SETTING_FIX_STAND_ON_THIN_AIR + 1;
const SETTING_FIX_GRAB_FALLING_SPEED: c_int = SETTING_FIX_PRESS_THROUGH_CLOSED_GATES + 1;
const SETTING_FIX_SKELETON_CHOMPER_BLOOD: c_int = SETTING_FIX_GRAB_FALLING_SPEED + 1;
const SETTING_FIX_MOVE_AFTER_DRINK: c_int = SETTING_FIX_SKELETON_CHOMPER_BLOOD + 1;
const SETTING_FIX_LOOSE_LEFT_OF_POTION: c_int = SETTING_FIX_MOVE_AFTER_DRINK + 1;
const SETTING_FIX_GUARD_FOLLOWING_THROUGH_CLOSED_GATES: c_int = SETTING_FIX_LOOSE_LEFT_OF_POTION + 1;
const SETTING_FIX_SAFE_LANDING_ON_SPIKES: c_int = SETTING_FIX_GUARD_FOLLOWING_THROUGH_CLOSED_GATES + 1;
const SETTING_FIX_GLIDE_THROUGH_WALL: c_int = SETTING_FIX_SAFE_LANDING_ON_SPIKES + 1;
const SETTING_FIX_DROP_THROUGH_TAPESTRY: c_int = SETTING_FIX_GLIDE_THROUGH_WALL + 1;
const SETTING_FIX_LAND_AGAINST_GATE_OR_TAPESTRY: c_int = SETTING_FIX_DROP_THROUGH_TAPESTRY + 1;
const SETTING_FIX_UNINTENDED_SWORD_STRIKE: c_int = SETTING_FIX_LAND_AGAINST_GATE_OR_TAPESTRY + 1;
const SETTING_FIX_RETREAT_WITHOUT_LEAVING_ROOM: c_int = SETTING_FIX_UNINTENDED_SWORD_STRIKE + 1;
const SETTING_FIX_RUNNING_JUMP_THROUGH_TAPESTRY: c_int = SETTING_FIX_RETREAT_WITHOUT_LEAVING_ROOM + 1;
const SETTING_FIX_PUSH_GUARD_INTO_WALL: c_int = SETTING_FIX_RUNNING_JUMP_THROUGH_TAPESTRY + 1;
const SETTING_FIX_JUMP_THROUGH_WALL_ABOVE_GATE: c_int = SETTING_FIX_PUSH_GUARD_INTO_WALL + 1;
const SETTING_FIX_CHOMPERS_NOT_STARTING: c_int = SETTING_FIX_JUMP_THROUGH_WALL_ABOVE_GATE + 1;
const SETTING_FIX_FEATHER_INTERRUPTED_BY_LEVELDOOR: c_int = SETTING_FIX_CHOMPERS_NOT_STARTING + 1;
const SETTING_FIX_OFFSCREEN_GUARDS_DISAPPEARING: c_int = SETTING_FIX_FEATHER_INTERRUPTED_BY_LEVELDOOR + 1;
const SETTING_FIX_MOVE_AFTER_SHEATHE: c_int = SETTING_FIX_OFFSCREEN_GUARDS_DISAPPEARING + 1;
const SETTING_FIX_HIDDEN_FLOORS_DURING_FLASHING: c_int = SETTING_FIX_MOVE_AFTER_SHEATHE + 1;
const SETTING_FIX_HANG_ON_TELEPORT: c_int = SETTING_FIX_HIDDEN_FLOORS_DURING_FLASHING + 1;
const SETTING_FIX_EXIT_DOOR: c_int = SETTING_FIX_HANG_ON_TELEPORT + 1;
const SETTING_FIX_QUICKSAVE_DURING_FEATHER: c_int = SETTING_FIX_EXIT_DOOR + 1;
const SETTING_FIX_CAPED_PRINCE_SLIDING_THROUGH_GATE: c_int = SETTING_FIX_QUICKSAVE_DURING_FEATHER + 1;
const SETTING_FIX_DOORTOP_DISABLING_GUARD: c_int = SETTING_FIX_CAPED_PRINCE_SLIDING_THROUGH_GATE + 1;
const SETTING_FIX_JUMPING_OVER_GUARD: c_int = SETTING_FIX_DOORTOP_DISABLING_GUARD + 1;
const SETTING_FIX_DROP_2_ROOMS_CLIMBING_LOOSE_TILE: c_int = SETTING_FIX_JUMPING_OVER_GUARD + 1;
const SETTING_FIX_FALLING_THROUGH_FLOOR_DURING_SWORD_STRIKE: c_int = SETTING_FIX_DROP_2_ROOMS_CLIMBING_LOOSE_TILE + 1;
const SETTING_FIX_REGISTER_QUICK_INPUT: c_int = SETTING_FIX_FALLING_THROUGH_FLOOR_DURING_SWORD_STRIKE + 1;
const SETTING_FIX_TURN_RUNNING_NEAR_WALL: c_int = SETTING_FIX_REGISTER_QUICK_INPUT + 1;
const SETTING_FIX_FEATHER_FALL_AFFECTS_GUARDS: c_int = SETTING_FIX_TURN_RUNNING_NEAR_WALL + 1;
const SETTING_FIX_ONE_HP_STOPS_BLINKING: c_int = SETTING_FIX_FEATHER_FALL_AFFECTS_GUARDS + 1;
const SETTING_FIX_DEAD_FLOATING_IN_AIR: c_int = SETTING_FIX_ONE_HP_STOPS_BLINKING + 1;
const SETTING_ENABLE_SUPER_HIGH_JUMP: c_int = SETTING_FIX_DEAD_FLOATING_IN_AIR + 1;
const SETTING_ENABLE_JUMP_GRAB: c_int = SETTING_ENABLE_SUPER_HIGH_JUMP + 1;
const SETTING_USE_CUSTOM_OPTIONS: c_int = SETTING_ENABLE_JUMP_GRAB + 1;
const SETTING_START_MINUTES_LEFT: c_int = SETTING_USE_CUSTOM_OPTIONS + 1;
const SETTING_START_TICKS_LEFT: c_int = SETTING_START_MINUTES_LEFT + 1;
const SETTING_START_HITP: c_int = SETTING_START_TICKS_LEFT + 1;
const SETTING_MAX_HITP_ALLOWED: c_int = SETTING_START_HITP + 1;
const SETTING_SAVING_ALLOWED_FIRST_LEVEL: c_int = SETTING_MAX_HITP_ALLOWED + 1;
const SETTING_SAVING_ALLOWED_LAST_LEVEL: c_int = SETTING_SAVING_ALLOWED_FIRST_LEVEL + 1;
const SETTING_START_UPSIDE_DOWN: c_int = SETTING_SAVING_ALLOWED_LAST_LEVEL + 1;
const SETTING_START_IN_BLIND_MODE: c_int = SETTING_START_UPSIDE_DOWN + 1;
const SETTING_COPYPROT_LEVEL: c_int = SETTING_START_IN_BLIND_MODE + 1;
const SETTING_DRAWN_TILE_TOP_LEVEL_EDGE: c_int = SETTING_COPYPROT_LEVEL + 1;
const SETTING_DRAWN_TILE_LEFT_LEVEL_EDGE: c_int = SETTING_DRAWN_TILE_TOP_LEVEL_EDGE + 1;
const SETTING_LEVEL_EDGE_HIT_TILE: c_int = SETTING_DRAWN_TILE_LEFT_LEVEL_EDGE + 1;
const SETTING_ALLOW_TRIGGERING_ANY_TILE: c_int = SETTING_LEVEL_EDGE_HIT_TILE + 1;
const SETTING_ENABLE_WDA_IN_PALACE: c_int = SETTING_ALLOW_TRIGGERING_ANY_TILE + 1;
const SETTING_FIRST_LEVEL: c_int = SETTING_ENABLE_WDA_IN_PALACE + 1;
const SETTING_SKIP_TITLE: c_int = SETTING_FIRST_LEVEL + 1;
const SETTING_SHIFT_L_ALLOWED_UNTIL_LEVEL: c_int = SETTING_SKIP_TITLE + 1;
const SETTING_SHIFT_L_REDUCED_MINUTES: c_int = SETTING_SHIFT_L_ALLOWED_UNTIL_LEVEL + 1;
const SETTING_SHIFT_L_REDUCED_TICKS: c_int = SETTING_SHIFT_L_REDUCED_MINUTES + 1;
const SETTING_DEMO_HITP: c_int = SETTING_SHIFT_L_REDUCED_TICKS + 1;
const SETTING_DEMO_END_ROOM: c_int = SETTING_DEMO_HITP + 1;
const SETTING_INTRO_MUSIC_LEVEL: c_int = SETTING_DEMO_END_ROOM + 1;
const SETTING_HAVE_SWORD_FROM_LEVEL: c_int = SETTING_INTRO_MUSIC_LEVEL + 1;
const SETTING_CHECKPOINT_LEVEL: c_int = SETTING_HAVE_SWORD_FROM_LEVEL + 1;
const SETTING_CHECKPOINT_RESPAWN_DIR: c_int = SETTING_CHECKPOINT_LEVEL + 1;
const SETTING_CHECKPOINT_RESPAWN_ROOM: c_int = SETTING_CHECKPOINT_RESPAWN_DIR + 1;
const SETTING_CHECKPOINT_RESPAWN_TILEPOS: c_int = SETTING_CHECKPOINT_RESPAWN_ROOM + 1;
const SETTING_CHECKPOINT_CLEAR_TILE_ROOM: c_int = SETTING_CHECKPOINT_RESPAWN_TILEPOS + 1;
const SETTING_CHECKPOINT_CLEAR_TILE_COL: c_int = SETTING_CHECKPOINT_CLEAR_TILE_ROOM + 1;
const SETTING_CHECKPOINT_CLEAR_TILE_ROW: c_int = SETTING_CHECKPOINT_CLEAR_TILE_COL + 1;
const SETTING_SKELETON_LEVEL: c_int = SETTING_CHECKPOINT_CLEAR_TILE_ROW + 1;
const SETTING_SKELETON_ROOM: c_int = SETTING_SKELETON_LEVEL + 1;
const SETTING_SKELETON_TRIGGER_COLUMN_1: c_int = SETTING_SKELETON_ROOM + 1;
const SETTING_SKELETON_TRIGGER_COLUMN_2: c_int = SETTING_SKELETON_TRIGGER_COLUMN_1 + 1;
const SETTING_SKELETON_COLUMN: c_int = SETTING_SKELETON_TRIGGER_COLUMN_2 + 1;
const SETTING_SKELETON_ROW: c_int = SETTING_SKELETON_COLUMN + 1;
const SETTING_SKELETON_REQUIRE_OPEN_LEVEL_DOOR: c_int = SETTING_SKELETON_ROW + 1;
const SETTING_SKELETON_SKILL: c_int = SETTING_SKELETON_REQUIRE_OPEN_LEVEL_DOOR + 1;
const SETTING_SKELETON_REAPPEAR_ROOM: c_int = SETTING_SKELETON_SKILL + 1;
const SETTING_SKELETON_REAPPEAR_X: c_int = SETTING_SKELETON_REAPPEAR_ROOM + 1;
const SETTING_SKELETON_REAPPEAR_ROW: c_int = SETTING_SKELETON_REAPPEAR_X + 1;
const SETTING_SKELETON_REAPPEAR_DIR: c_int = SETTING_SKELETON_REAPPEAR_ROW + 1;
const SETTING_MIRROR_LEVEL: c_int = SETTING_SKELETON_REAPPEAR_DIR + 1;
const SETTING_MIRROR_ROOM: c_int = SETTING_MIRROR_LEVEL + 1;
const SETTING_MIRROR_COLUMN: c_int = SETTING_MIRROR_ROOM + 1;
const SETTING_MIRROR_ROW: c_int = SETTING_MIRROR_COLUMN + 1;
const SETTING_MIRROR_TILE: c_int = SETTING_MIRROR_ROW + 1;
const SETTING_SHOW_MIRROR_IMAGE: c_int = SETTING_MIRROR_TILE + 1;
const SETTING_SHADOW_STEAL_LEVEL: c_int = SETTING_SHOW_MIRROR_IMAGE + 1;
const SETTING_SHADOW_STEAL_ROOM: c_int = SETTING_SHADOW_STEAL_LEVEL + 1;
const SETTING_SHADOW_STEP_LEVEL: c_int = SETTING_SHADOW_STEAL_ROOM + 1;
const SETTING_SHADOW_STEP_ROOM: c_int = SETTING_SHADOW_STEP_LEVEL + 1;
const SETTING_FALLING_EXIT_LEVEL: c_int = SETTING_SHADOW_STEP_ROOM + 1;
const SETTING_FALLING_EXIT_ROOM: c_int = SETTING_FALLING_EXIT_LEVEL + 1;
const SETTING_FALLING_ENTRY_LEVEL: c_int = SETTING_FALLING_EXIT_ROOM + 1;
const SETTING_FALLING_ENTRY_ROOM: c_int = SETTING_FALLING_ENTRY_LEVEL + 1;
const SETTING_MOUSE_LEVEL: c_int = SETTING_FALLING_ENTRY_ROOM + 1;
const SETTING_MOUSE_ROOM: c_int = SETTING_MOUSE_LEVEL + 1;
const SETTING_MOUSE_DELAY: c_int = SETTING_MOUSE_ROOM + 1;
const SETTING_MOUSE_OBJECT: c_int = SETTING_MOUSE_DELAY + 1;
const SETTING_MOUSE_START_X: c_int = SETTING_MOUSE_OBJECT + 1;
const SETTING_LOOSE_TILES_LEVEL: c_int = SETTING_MOUSE_START_X + 1;
const SETTING_LOOSE_TILES_ROOM_1: c_int = SETTING_LOOSE_TILES_LEVEL + 1;
const SETTING_LOOSE_TILES_ROOM_2: c_int = SETTING_LOOSE_TILES_ROOM_1 + 1;
const SETTING_LOOSE_TILES_FIRST_TILE: c_int = SETTING_LOOSE_TILES_ROOM_2 + 1;
const SETTING_LOOSE_TILES_LAST_TILE: c_int = SETTING_LOOSE_TILES_FIRST_TILE + 1;
const SETTING_JAFFAR_VICTORY_LEVEL: c_int = SETTING_LOOSE_TILES_LAST_TILE + 1;
const SETTING_JAFFAR_VICTORY_FLASH_TIME: c_int = SETTING_JAFFAR_VICTORY_LEVEL + 1;
const SETTING_HIDE_LEVEL_NUMBER_FIRST_LEVEL: c_int = SETTING_JAFFAR_VICTORY_FLASH_TIME + 1;
const SETTING_LEVEL_13_LEVEL_NUMBER: c_int = SETTING_HIDE_LEVEL_NUMBER_FIRST_LEVEL + 1;
const SETTING_VICTORY_STOPS_TIME_LEVEL: c_int = SETTING_LEVEL_13_LEVEL_NUMBER + 1;
const SETTING_WIN_LEVEL: c_int = SETTING_VICTORY_STOPS_TIME_LEVEL + 1;
const SETTING_WIN_ROOM: c_int = SETTING_WIN_LEVEL + 1;
const SETTING_LOOSE_FLOOR_DELAY: c_int = SETTING_WIN_ROOM + 1;
const SETTING_BASE_SPEED: c_int = SETTING_LOOSE_FLOOR_DELAY + 1;
const SETTING_FIGHT_SPEED: c_int = SETTING_BASE_SPEED + 1;
const SETTING_CHOMPER_SPEED: c_int = SETTING_FIGHT_SPEED + 1;
const SETTING_NO_MOUSE_IN_ENDING: c_int = SETTING_CHOMPER_SPEED + 1;
const SETTING_LEVEL_SETTINGS: c_int = SETTING_NO_MOUSE_IN_ENDING + 1;
const SETTING_LEVEL_TYPE: c_int = SETTING_LEVEL_SETTINGS + 1;
const SETTING_LEVEL_COLOR: c_int = SETTING_LEVEL_TYPE + 1;
const SETTING_GUARD_TYPE: c_int = SETTING_LEVEL_COLOR + 1;
const SETTING_GUARD_HP: c_int = SETTING_GUARD_TYPE + 1;
const SETTING_CUTSCENE: c_int = SETTING_GUARD_HP + 1;
const SETTING_ENTRY_POSE: c_int = SETTING_CUTSCENE + 1;
const SETTING_SEAMLESS_EXIT: c_int = SETTING_ENTRY_POSE + 1;
const SETTING_KEY_LEFT: c_int = SETTING_SEAMLESS_EXIT + 1;
const SETTING_KEY_RIGHT: c_int = SETTING_KEY_LEFT + 1;
const SETTING_KEY_UP: c_int = SETTING_KEY_RIGHT + 1;
const SETTING_KEY_DOWN: c_int = SETTING_KEY_UP + 1;
const SETTING_KEY_JUMP_LEFT: c_int = SETTING_KEY_DOWN + 1;
const SETTING_KEY_JUMP_RIGHT: c_int = SETTING_KEY_JUMP_LEFT + 1;
const SETTING_KEY_ACTION: c_int = SETTING_KEY_JUMP_RIGHT + 1;
const SETTING_KEY_ENTER: c_int = SETTING_KEY_ACTION + 1;
const SETTING_KEY_ESC: c_int = SETTING_KEY_ENTER + 1;

#[repr(C)]
struct setting_type {
    index: c_int,
    id: c_int,
    previous: c_int,
    next: c_int,
    style: u8,
    number_type: u8,
    linked: *mut c_void,
    required: *mut c_void,
    min: c_int,
    max: c_int,
    text: [c_char; 64],
    explanation: [c_char; 256],
    names_list: *mut names_list_type,
}

#[repr(C)]
struct settings_area_type {
    settings: *mut setting_type,
    setting_count: c_int,
}

const INT16_MAX: c_int = 32767;
const UINT16_MAX: c_int = 65535;
const UINT8_MAX: c_int = 255;

// ============================================================================
// Mutable menu state (file-scope globals in menu.c)
// ============================================================================
static mut hovering_pause_menu_item: c_int = PAUSE_MENU_RESUME;
static mut next_pause_menu_item: *mut pause_menu_item_type = null_mut();
static mut previous_pause_menu_item: *mut pause_menu_item_type = null_mut();
static mut drawn_menu: c_int = 0;
static mut pause_menu_alpha: u8 = 0;
static mut current_dialog_box: c_int = 0;
static mut current_dialog_text: *const c_char = null();
static mut menu_current_level: word = 1;
static mut need_close_menu: bool = false;

static mut active_settings_subsection: c_int = 0;
static mut highlighted_settings_subsection: c_int = 0;
static mut scroll_position: c_int = 0;
static mut menu_control_y: c_int = 0;
static mut menu_control_x: c_int = 0;
static mut menu_control_back: c_int = 0;

static mut highlighted_setting_id: c_int = SETTING_ENABLE_INFO_SCREEN;
static mut controlled_area: c_int = 0;
static mut next_setting_id: c_int = 0;
static mut previous_setting_id: c_int = 0;
static mut at_scroll_up_boundary: bool = false;
static mut at_scroll_down_boundary: bool = false;
static mut were_settings_changed: bool = false;
static mut need_full_menu_redraw_count: c_int = 0;
static mut integer_scaling_possible: c_int = 1;
static mut exe_crc: dword = 0;

static mut joy_ABXY_buttons_released: bool = false;
static mut joy_xy_released: bool = false;
static mut joy_xy_timeout_counter: u64 = 0;

static explanation_rect: rect_type = rect_type { top: 170, left: 20, bottom: 200, right: 300 };
static cancel_text_rect: rect_type = rect_type { top: 104, left: 162, bottom: 118, right: 212 };
static cancel_highlight_rect: rect_type = rect_type { top: 103, left: 162, bottom: 116, right: 212 };
static ok_text_rect: rect_type = rect_type { top: 104, left: 108, bottom: 118, right: 158 };
static ok_highlight_rect: rect_type = rect_type { top: 103, left: 108, bottom: 116, right: 158 };

const PAUSE_MENU_ITEMS_N: usize = 7;
static mut pause_menu_items: [pause_menu_item_type; PAUSE_MENU_ITEMS_N] = [
    pause_menu_item_type { id: PAUSE_MENU_RESUME,        previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"RESUME") },
    pause_menu_item_type { id: PAUSE_MENU_SAVE_GAME,     previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"QUICKSAVE (F6)") },
    pause_menu_item_type { id: PAUSE_MENU_LOAD_GAME,     previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"QUICKLOAD (F9)") },
    pause_menu_item_type { id: PAUSE_MENU_RESTART_LEVEL, previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"RESTART LEVEL") },
    pause_menu_item_type { id: PAUSE_MENU_SETTINGS,      previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"SETTINGS") },
    pause_menu_item_type { id: PAUSE_MENU_RESTART_GAME,  previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"RESTART GAME") },
    pause_menu_item_type { id: PAUSE_MENU_QUIT_GAME,     previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"QUIT GAME") },
];

const SETTINGS_MENU_ITEMS_N: usize = 6;
static mut settings_menu_items: [pause_menu_item_type; SETTINGS_MENU_ITEMS_N] = [
    pause_menu_item_type { id: SETTINGS_MENU_GENERAL,  previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"GENERAL") },
    pause_menu_item_type { id: SETTINGS_MENU_GAMEPLAY, previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"GAMEPLAY") },
    pause_menu_item_type { id: SETTINGS_MENU_VISUALS,  previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"VISUALS") },
    pause_menu_item_type { id: SETTINGS_MENU_MODS,     previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"MODS") },
    pause_menu_item_type { id: SETTINGS_MENU_CONTROLS, previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"CONTROLS") },
    pause_menu_item_type { id: SETTINGS_MENU_BACK,     previous: null_mut(), next: null_mut(), required: null_mut(), text: cstr(b"BACK") },
];

// ============================================================================
// NAMES_LIST / KEY_VALUE_LIST tables
// ============================================================================
const fn names_list_names(data: *const [[c_char; 20]; 0], count: word) -> names_list_type {
    names_list_type {
        type_: 0,
        __bindgen_anon_1: names_list_type__bindgen_ty_1 {
            names: names_list_type__bindgen_ty_1__bindgen_ty_1 { data, count },
        },
    }
}
const fn names_list_kv(data: *mut key_value_type, count: word) -> names_list_type {
    names_list_type {
        type_: 1,
        __bindgen_anon_1: names_list_type__bindgen_ty_1 {
            kv_pairs: names_list_type__bindgen_ty_1__bindgen_ty_2 { data, count },
        },
    }
}

static use_hardware_acceleration_setting_names: [[c_char; 20]; 3] = [cstr(b"OFF"), cstr(b"ON"), cstr(b"AUTO")];
static mut use_hardware_acceleration_setting_names_list: names_list_type =
    names_list_names(addr_of!(use_hardware_acceleration_setting_names) as *const [[c_char; 20]; 0], 3);

static scaling_type_setting_names: [[c_char; 20]; 3] = [cstr(b"Sharp"), cstr(b"Fuzzy"), cstr(b"Blurry")];
static mut scaling_type_setting_names_list: names_list_type =
    names_list_names(addr_of!(scaling_type_setting_names) as *const [[c_char; 20]; 0], 3);

static tile_type_setting_names: [[c_char; 20]; 32] = [
    cstr(b"Empty"), cstr(b"Floor"), cstr(b"Spikes"), cstr(b"Pillar"), cstr(b"Gate"),
    cstr(b"Stuck button"), cstr(b"Closer button"), cstr(b"Tapestry/floor"), cstr(b"Big pillar: bottom"), cstr(b"Big pillar: top"),
    cstr(b"Potion"), cstr(b"Loose floor"), cstr(b"Tapestry"), cstr(b"Mirror"), cstr(b"Floor/debris"),
    cstr(b"Raise button"), cstr(b"Level door: left"), cstr(b"Level door: right"), cstr(b"Chomper"), cstr(b"Torch"),
    cstr(b"Wall"), cstr(b"Skeleton"), cstr(b"Sword"), cstr(b"Balcony: left"), cstr(b"Balcony: right"),
    cstr(b"Lattice: pillar"), cstr(b"Lattice: down"), cstr(b"Lattice: small"), cstr(b"Lattice: left"), cstr(b"Lattice: right"),
    cstr(b"Torch/debris"), cstr(b"Tile 31 (unused)"),
];
static mut tile_type_setting_names_list: names_list_type =
    names_list_names(addr_of!(tile_type_setting_names) as *const [[c_char; 20]; 0], 32);

static row_setting_names: [[c_char; 20]; 3] = [cstr(b"Top"), cstr(b"Middle"), cstr(b"Bottom")];
static mut row_setting_names_list: names_list_type =
    names_list_names(addr_of!(row_setting_names) as *const [[c_char; 20]; 0], 3);

static direction_setting_names: [key_value_type; 2] = [
    key_value_type { key: cstr(b"Left"), value: -1 },  // dir_FF_left
    key_value_type { key: cstr(b"Right"), value: 0 },  // dir_0_right
];
static mut direction_setting_names_list: names_list_type =
    names_list_kv(addr_of!(direction_setting_names) as *mut key_value_type, 2);

static level_type_setting_names: [[c_char; 20]; 2] = [cstr(b"Dungeon"), cstr(b"Palace")];
static mut level_type_setting_names_list: names_list_type =
    names_list_names(addr_of!(level_type_setting_names) as *const [[c_char; 20]; 0], 2);

static guard_type_setting_names: [key_value_type; 6] = [
    key_value_type { key: cstr(b"None"), value: -1 },
    key_value_type { key: cstr(b"Normal"), value: 0 },
    key_value_type { key: cstr(b"Fat"), value: 1 },
    key_value_type { key: cstr(b"Skeleton"), value: 2 },
    key_value_type { key: cstr(b"Vizier"), value: 3 },
    key_value_type { key: cstr(b"Shadow"), value: 4 },
];
static mut guard_type_setting_names_list: names_list_type =
    names_list_kv(addr_of!(guard_type_setting_names) as *mut key_value_type, 6);

static entry_pose_setting_names: [[c_char; 20]; 3] = [cstr(b"Turning"), cstr(b"Falling"), cstr(b"Running")];
static mut entry_pose_setting_names_list: names_list_type =
    names_list_names(addr_of!(entry_pose_setting_names) as *const [[c_char; 20]; 0], 3);

static off_setting_name: [key_value_type; 1] = [key_value_type { key: cstr(b"Off"), value: -1 }];
static mut off_setting_name_list: names_list_type =
    names_list_kv(addr_of!(off_setting_name) as *mut key_value_type, 1);

// ============================================================================
// Settings tables
// ============================================================================
#[allow(clippy::too_many_arguments)]
const fn st(
    id: c_int,
    style: c_int,
    number_type: u8,
    linked: *mut c_void,
    required: *mut c_void,
    min: c_int,
    max: c_int,
    names_list: *mut names_list_type,
    text: [c_char; 64],
    explanation: [c_char; 256],
) -> setting_type {
    setting_type {
        index: 0,
        id,
        previous: 0,
        next: 0,
        style: style as u8,
        number_type,
        linked,
        required,
        min,
        max,
        text,
        explanation,
        names_list,
    }
}

const GENERAL_N: usize = 8;
static mut general_settings: [setting_type; GENERAL_N] = unsafe { [
    st(SETTING_SHOW_MENU_ON_PAUSE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_pause_menu) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enable pause menu"),
        cstr(b"Show the in-game menu when you pause the game.\nIf disabled, you can still bring up the menu by pressing Backspace.")),
    st(SETTING_ENABLE_INFO_SCREEN, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_info_screen) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Display info screen on launch"),
        cstr(b"Display the SDLPoP information screen when the game starts.")),
    st(SETTING_ENABLE_SOUND, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(is_sound_on) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enable sound"),
        cstr(b"Turn sound on or off.")),
    st(SETTING_ENABLE_MUSIC, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_music) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enable music"),
        cstr(b"Turn music on or off.")),
    st(SETTING_ENABLE_CONTROLLER_RUMBLE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_controller_rumble) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enable controller rumble"),
        cstr(b"If using a controller with a rumble motor, provide haptic feedback when the kid is hurt.")),
    st(SETTING_JOYSTICK_THRESHOLD, SETTING_STYLE_NUMBER, SETTING_INT, addr_of_mut!(joystick_threshold) as *mut c_void, null_mut(), 0, INT16_MAX, null_mut(),
        cstr(b"Joystick threshold"),
        cstr(b"Joystick 'dead zone' sensitivity threshold.")),
    st(SETTING_JOYSTICK_ONLY_HORIZONTAL, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(joystick_only_horizontal) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Horizontal joystick movement only"),
        cstr(b"Use joysticks for horizontal movement only, not all-directional. This may make the game easier to control for some controllers.")),
    st(SETTING_RESET_ALL_SETTINGS, SETTING_STYLE_TEXT_ONLY, 0, null_mut(), null_mut(), 0, 0, null_mut(),
        cstr(b"Restore defaults..."),
        cstr(b"Revert all settings to the default state.")),
] };

const VISUALS_N: usize = 8;
static mut visuals_settings: [setting_type; VISUALS_N] = unsafe { [
    st(SETTING_FULLSCREEN, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(start_fullscreen) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Start fullscreen"),
        cstr(b"Start the game in fullscreen mode.\nYou can also toggle fullscreen by pressing Alt+Enter.")),
    st(SETTING_USE_HARDWARE_ACCELERATION, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(use_hardware_acceleration) as *mut c_void, null_mut(), 0, 2, addr_of_mut!(use_hardware_acceleration_setting_names_list),
        cstr(b"Use hardware acceleration"),
        cstr(b"Auto - Use hardware acceleration, if available.\nOn - Force hardware acceleration.\nOff - Disable hardware acceleration.\nNote: This requires a restart.")),
    st(SETTING_USE_CORRECT_ASPECT_RATIO, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(use_correct_aspect_ratio) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Use 4:3 aspect ratio"),
        cstr(b"Render the game in the originally intended 4:3 aspect ratio.\nNB. Works best using a high resolution.")),
    st(SETTING_USE_INTEGER_SCALING, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(use_integer_scaling) as *mut c_void, addr_of_mut!(integer_scaling_possible) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Use integer scaling"),
        cstr(b"Enable pixel perfect scaling. That is, make all pixels the same size by forcing integer scale factors.\nCombining with 4:3 aspect ratio requires at least 1600x1200.\nYou need to compile with SDL 2.0.5 or newer to enable this.")),
    st(SETTING_SCALING_TYPE, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(scaling_type) as *mut c_void, null_mut(), 0, 2, addr_of_mut!(scaling_type_setting_names_list),
        cstr(b"Scaling method"),
        cstr(b"Sharp - Use nearest neighbour resampling.\nFuzzy - First upscale to double size, then use smooth scaling.\nBlurry - Use smooth scaling.")),
    st(SETTING_ENABLE_FADE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_fade) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Fading enabled"),
        cstr(b"Turn fading on or off.")),
    st(SETTING_ENABLE_FLASH, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_flash) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Flashing enabled"),
        cstr(b"Turn flashing on or off.")),
    st(SETTING_ENABLE_LIGHTING, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_lighting) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Torch shadows enabled"),
        cstr(b"Darken those parts of the screen which are not near a torch.")),
] };

const GAMEPLAY_N: usize = 54;
static mut gameplay_settings: [setting_type; GAMEPLAY_N] = unsafe { [
    st(SETTING_ENABLE_CHEATS, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(cheats_enabled) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enable cheats"), cstr(b"Turn cheats on or off.")),
    st(SETTING_ENABLE_COPYPROT, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_copyprot) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enable copy protection level"), cstr(b"Enable or disable the potions (copy protection) level.")),
    st(SETTING_ENABLE_QUICKSAVE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_quicksave) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enable quicksave"), cstr(b"Enable quicksave/load feature.\nPress F6 to quicksave, F9 to quickload.")),
    st(SETTING_ENABLE_QUICKSAVE_PENALTY, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_quicksave_penalty) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Quicksave time penalty"), cstr(b"Try to let time run out when quickloading (similar to dying).\nActually, the 'remaining time' will still be restored, but a penalty (up to one minute) will be applied.")),
    st(SETTING_ENABLE_REPLAY, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(enable_replay) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enable replays"), cstr(b"Enable recording/replay feature.\nPress Ctrl+Tab in-game to start recording.\nTo stop, press Ctrl+Tab again.")),
    st(SETTING_USE_FIXES_AND_ENHANCEMENTS, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enhanced mode (allow bug fixes)"), cstr(b"Turn on game fixes and enhancements.\nBelow, you can turn individual fixes/enhancements on or off.\nNOTE: Some fixes disable 'tricks' that depend on game quirks.")),
    st(SETTING_ENABLE_CROUCH_AFTER_CLIMBING, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.enable_crouch_after_climbing) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Enable crouching after climbing"), cstr(b"Adds a way to crouch immediately after climbing up: press down and forward simultaneously. In the original game, this could not be done (pressing down always causes the kid to climb down).")),
    st(SETTING_ENABLE_FREEZE_TIME_DURING_END_MUSIC, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.enable_freeze_time_during_end_music) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Freeze time during level end music"), cstr(b"Time runs out while the level ending music plays; however, the music can be skipped by disabling sound. This option stops time while the ending music is playing (so there is no need to disable sound).")),
    st(SETTING_ENABLE_REMEMBER_GUARD_HP, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.enable_remember_guard_hp) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Remember guard hitpoints"), cstr(b"Enable guard hitpoints not resetting to their default (maximum) value when re-entering the room.")),
    st(SETTING_ENABLE_SUPER_HIGH_JUMP, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.enable_super_high_jump) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Enable super high jump"), cstr(b"Prince in feather mode (after drinking a green potion) can jump 2 stories high.")),
    st(SETTING_ENABLE_JUMP_GRAB, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.enable_jump_grab) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Enable jump grab"), cstr(b"Prince can grab tiles on the floor above while jumping. Hold Shift and up arrow, but not the forward arrow key.")),
    st(SETTING_FIX_GATE_SOUNDS, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_gate_sounds) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix gate sounds bug"), cstr(b"If a room is linked to itself on the left, the closing sounds of the gates in that room can't be heard.")),
    st(SETTING_TWO_COLL_BUG, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_two_coll_bug) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix two collisions bug"), cstr(b"An open gate or chomper may enable the Kid to go through walls. (Trick 7, 37, 62)")),
    st(SETTING_FIX_INFINITE_DOWN_BUG, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_infinite_down_bug) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix infinite down bug"), cstr(b"If a room is linked to itself at the bottom, and the Kid's column has no floors, the game hangs.")),
    st(SETTING_FIX_GATE_DRAWING_BUG, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_gate_drawing_bug) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix gate drawing bug"), cstr(b"When a gate is under another gate, the top of the bottom gate is not visible.")),
    st(SETTING_FIX_BIGPILLAR_CLIMB, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_bigpillar_climb) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix big pillar climbing bug"), cstr(b"When climbing up to a floor with a big pillar top behind, turned right, Kid sees through floor.")),
    st(SETTING_FIX_JUMP_DISTANCE_AT_EDGE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_jump_distance_at_edge) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix jump distance at edge"), cstr(b"When climbing up two floors, turning around and jumping upward, the kid falls down. This fix makes the workaround of Trick 25 unnecessary.")),
    st(SETTING_FIX_EDGE_DISTANCE_CHECK_WHEN_CLIMBING, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_edge_distance_check_when_climbing) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix edge distance check when climbing"), cstr(b"When climbing to a higher floor, the game unnecessarily checks how far away the edge below is. Sometimes you will \"teleport\" some distance when climbing from firm ground.")),
    st(SETTING_FIX_PAINLESS_FALL_ON_GUARD, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_painless_fall_on_guard) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix painless fall on guard"), cstr(b"Falling from a great height directly on top of guards does not hurt.")),
    st(SETTING_FIX_WALL_BUMP_TRIGGERS_TILE_BELOW, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_wall_bump_triggers_tile_below) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix wall bump triggering tile below"), cstr(b"Bumping against a wall may cause a loose floor below to drop, even though it has not been touched. (Trick 18, 34)")),
    st(SETTING_FIX_STAND_ON_THIN_AIR, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_stand_on_thin_air) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix standing on thin air"), cstr(b"When pressing a loose tile, you can temporarily stand on thin air by standing up from crouching.")),
    st(SETTING_FIX_PRESS_THROUGH_CLOSED_GATES, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_press_through_closed_gates) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix pressing through closed gates"), cstr(b"Buttons directly to the right of gates can be pressed even though the gate is closed (Trick 1)")),
    st(SETTING_FIX_GRAB_FALLING_SPEED, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_grab_falling_speed) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix grab falling speed"), cstr(b"By jumping and bumping into a wall, you can sometimes grab a ledge two stories down (which should not be possible).")),
    st(SETTING_FIX_SKELETON_CHOMPER_BLOOD, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_skeleton_chomper_blood) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix skeleton chomper blood"), cstr(b"When chomped, skeletons cause the chomper to become bloody even though skeletons do not have blood.")),
    st(SETTING_FIX_MOVE_AFTER_DRINK, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_move_after_drink) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix movement after drinking"), cstr(b"Controls do not get released properly when drinking a potion, sometimes causing unintended movements.")),
    st(SETTING_FIX_LOOSE_LEFT_OF_POTION, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_loose_left_of_potion) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix loose floor left of potion"), cstr(b"A drawing bug occurs when a loose tile is placed to the left of a potion (or sword).")),
    st(SETTING_FIX_GUARD_FOLLOWING_THROUGH_CLOSED_GATES, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_guard_following_through_closed_gates) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix guards passing closed gates"), cstr(b"Guards may \"follow\" the kid to the room on the left or right, even though there is a closed gate in between.")),
    st(SETTING_FIX_SAFE_LANDING_ON_SPIKES, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_safe_landing_on_spikes) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix safe landing on spikes"), cstr(b"When landing on the edge of a spikes tile, it is considered safe. (Trick 65)")),
    st(SETTING_FIX_GLIDE_THROUGH_WALL, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_glide_through_wall) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix gliding through walls"), cstr(b"The kid may glide through walls after turning around while running (especially when weightless).")),
    st(SETTING_FIX_DROP_THROUGH_TAPESTRY, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_drop_through_tapestry) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix dropping through tapestries"), cstr(b"The kid can drop down through a closed gate, when there is a tapestry (doortop) above the gate.")),
    st(SETTING_FIX_LAND_AGAINST_GATE_OR_TAPESTRY, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_land_against_gate_or_tapestry) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix land against gate or tapestry"), cstr(b"When dropping down and landing right in front of a wall, the entire landing animation should normally play. However, when falling against a closed gate or a tapestry(+floor) tile, the animation aborts.")),
    st(SETTING_FIX_UNINTENDED_SWORD_STRIKE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_unintended_sword_strike) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix unintended sword strike"), cstr(b"Sometimes, the kid may automatically strike immediately after drawing the sword. This especially happens when dropping down from a higher floor and then turning towards the opponent.")),
    st(SETTING_FIX_RETREAT_WITHOUT_LEAVING_ROOM, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_retreat_without_leaving_room) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix retreat without leaving room"), cstr(b"By repeatedly pressing 'back' in a swordfight, you can retreat out of a room without the room changing. (Trick 35)")),
    st(SETTING_FIX_RUNNING_JUMP_THROUGH_TAPESTRY, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_running_jump_through_tapestry) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix running jumps through tapestries"), cstr(b"The kid can jump through a tapestry with a running jump to the left, if there is a floor above it.")),
    st(SETTING_FIX_PUSH_GUARD_INTO_WALL, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_push_guard_into_wall) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix pushing guards into walls"), cstr(b"Guards can be pushed into walls, because the game does not correctly check for walls located behind a guard.")),
    st(SETTING_FIX_JUMP_THROUGH_WALL_ABOVE_GATE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_jump_through_wall_above_gate) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix jump through wall above gate"), cstr(b"By doing a running jump into a wall, you can fall behind a closed gate two floors down. (e.g. skip in Level 7)")),
    st(SETTING_FIX_CHOMPERS_NOT_STARTING, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_chompers_not_starting) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix chompers not starting"), cstr(b"If you grab a ledge that is one or more floors down, the chompers on that row will not start.")),
    st(SETTING_FIX_FEATHER_INTERRUPTED_BY_LEVELDOOR, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_feather_interrupted_by_leveldoor) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix leveldoor interrupting feather fall"), cstr(b"As soon as a level door has completely opened, the feather fall effect is interrupted because the sound stops.")),
    st(SETTING_FIX_OFFSCREEN_GUARDS_DISAPPEARING, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_offscreen_guards_disappearing) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix offscreen guards disappearing"), cstr(b"Guards will often not reappear in another room if they have been pushed (partly or entirely) offscreen.")),
    st(SETTING_FIX_MOVE_AFTER_SHEATHE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_move_after_sheathe) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix movement after sheathing"), cstr(b"While putting the sword away, if you press forward and down, and then release down, the kid will still duck.")),
    st(SETTING_FIX_HIDDEN_FLOORS_DURING_FLASHING, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_hidden_floors_during_flashing) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix hidden floors during flashing"), cstr(b"After uniting with the shadow in level 12, the hidden floors will not appear until after the flashing stops.")),
    st(SETTING_FIX_HANG_ON_TELEPORT, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_hang_on_teleport) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix hang on teleport bug"), cstr(b"By jumping towards one of the bottom corners of the room and grabbing a ledge, you can teleport to the room above.")),
    st(SETTING_FIX_EXIT_DOOR, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_exit_door) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix exit doors"), cstr(b"You can enter closed exit doors after you met the shadow or Jaffar died, or after you opened one of multiple exits.")),
    st(SETTING_FIX_QUICKSAVE_DURING_FEATHER, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_quicksave_during_feather) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix quicksave in feather mode"), cstr(b"You cannot save game while floating in feather mode.")),
    st(SETTING_FIX_CAPED_PRINCE_SLIDING_THROUGH_GATE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_caped_prince_sliding_through_gate) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix sliding through closed gate"), cstr(b"If you are using the caped prince graphics, and crouch with your back towards a closed gate on the left edge on the room, then the prince will slide through the gate.")),
    st(SETTING_FIX_DOORTOP_DISABLING_GUARD, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_doortop_disabling_guard) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix door top disabling guard"), cstr(b"Guards become inactive if they are standing on a door top (with floor), or if the prince is standing on a door top.")),
    st(SETTING_FIX_JUMPING_OVER_GUARD, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_jumping_over_guard) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix jumping over guard"), cstr(b"Prince can jump over guards with a properly timed running jump.")),
    st(SETTING_FIX_DROP_2_ROOMS_CLIMBING_LOOSE_TILE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_drop_2_rooms_climbing_loose_tile) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix dropping 2 rooms with loose tile"), cstr(b"Prince can fall 2 rooms down while climbing a loose tile in a room above. (Trick 153)")),
    st(SETTING_FIX_FALLING_THROUGH_FLOOR_DURING_SWORD_STRIKE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_falling_through_floor_during_sword_strike) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix dropping through floor striking"), cstr(b"Prince or guard can fall through the floor during a sword strike sequence.")),
    st(SETTING_FIX_REGISTER_QUICK_INPUT, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_register_quick_input) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix fast inputs"), cstr(b"Input is ignored if a button or key is pressed and released between game ticks.")),
    st(SETTING_FIX_TURN_RUNNING_NEAR_WALL, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_turn_running_near_wall) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix run turning near wall"), cstr(b"Ensures Prince safe steps near a wall/gate when facing in an opposite direction.")),
    st(SETTING_FIX_FEATHER_FALL_AFFECTS_GUARDS, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_feather_fall_affects_guards) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix feather fall affecting guards"), cstr(b"Feather fall should not affect guards, because only the prince can drink the feather fall potion.")),
    st(SETTING_FIX_ONE_HP_STOPS_BLINKING, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_one_hp_stops_blinking) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix one hit point stops blinking"), cstr(b"If the prince has only one hit point when he defeats Jaffar, it stops blinking.")),
    st(SETTING_FIX_DEAD_FLOATING_IN_AIR, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(fixes_saved.fix_dead_floating_in_air) as *mut c_void, addr_of_mut!(use_fixes_and_enhancements) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Fix dead bodies floating in the air"), cstr(b"If the prince or a guard falls to his death onto a loose floor, the floor drops, but the body stays there in the air.")),
] };

const MODS_N: usize = 80;
static mut mods_settings: [setting_type; MODS_N] = unsafe { [
    st(SETTING_USE_CUSTOM_OPTIONS, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(use_custom_options) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Use customization options"), cstr(b"Turn customization options on or off.\n(default = OFF)")),
    st(SETTING_LEVEL_SETTINGS, SETTING_STYLE_TEXT_ONLY, 0, null_mut(), addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Customize level..."), cstr(b"Change level-specific options (such as level type, guard type, number of guard hitpoints).")),
    st(SETTING_START_MINUTES_LEFT, SETTING_STYLE_NUMBER, SETTING_SHORT, addr_of_mut!(custom_saved.start_minutes_left) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, -1, INT16_MAX, null_mut(),
        cstr(b"Starting minutes left"), cstr(b"Starting minutes left. (default = 60)\nTo disable the time limit completely, set this to -1.")),
    st(SETTING_START_TICKS_LEFT, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.start_ticks_left) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT16_MAX, null_mut(),
        cstr(b"Starting seconds left"), cstr(b"Starting number of seconds left in the first minute.\n(default = 59.92)")),
    st(SETTING_START_HITP, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.start_hitp) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT16_MAX, null_mut(),
        cstr(b"Starting hitpoints"), cstr(b"Starting hitpoints. (default = 3)")),
    st(SETTING_MAX_HITP_ALLOWED, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.max_hitp_allowed) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT16_MAX, null_mut(),
        cstr(b"Max hitpoints allowed"), cstr(b"Maximum number of hitpoints you can get. (default = 10)")),
    st(SETTING_SAVING_ALLOWED_FIRST_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.saving_allowed_first_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Saving allowed: first level"), cstr(b"First level where you can save the game. (default = 3)")),
    st(SETTING_SAVING_ALLOWED_LAST_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.saving_allowed_last_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Saving allowed: last level"), cstr(b"Last level where you can save the game. (default = 13)")),
    st(SETTING_START_UPSIDE_DOWN, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(custom_saved.start_upside_down) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Start with the screen flipped"), cstr(b"Start the game with the screen flipped upside down, similar to Shift+I (default = OFF)")),
    st(SETTING_START_IN_BLIND_MODE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(custom_saved.start_in_blind_mode) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Start in blind mode"), cstr(b"Start in blind mode, similar to Shift+B (default = OFF)")),
    st(SETTING_COPYPROT_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.copyprot_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Copy protection before level"), cstr(b"The potions level will appear before this level. (default = 2)")),
    st(SETTING_DRAWN_TILE_TOP_LEVEL_EDGE, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.drawn_tile_top_level_edge) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 31, addr_of_mut!(tile_type_setting_names_list),
        cstr(b"Drawn tile: top level edge"), cstr(b"Tile drawn at the top of the room if there is no room that way. (default = floor)")),
    st(SETTING_DRAWN_TILE_LEFT_LEVEL_EDGE, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.drawn_tile_left_level_edge) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 31, addr_of_mut!(tile_type_setting_names_list),
        cstr(b"Drawn tile: left level edge"), cstr(b"Tile drawn at the left of the room if there is no room that way. (default = wall)")),
    st(SETTING_LEVEL_EDGE_HIT_TILE, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.level_edge_hit_tile) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 31, addr_of_mut!(tile_type_setting_names_list),
        cstr(b"Level edge hit tile"), cstr(b"Tile behavior at the top or left of the room if there is no room that way (default = wall)")),
    st(SETTING_ALLOW_TRIGGERING_ANY_TILE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(custom_saved.allow_triggering_any_tile) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Allow triggering any tile"), cstr(b"Enable triggering any tile. For example a button could make loose floors fall, or start a stuck chomper. (default = OFF)")),
    st(SETTING_ENABLE_WDA_IN_PALACE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(custom_saved.enable_wda_in_palace) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Enable WDA in palace"), cstr(b"Enable the dungeon wall drawing algorithm in the palace.\nN.B. Use with a modified VPALACE.DAT that provides dungeon-like wall graphics! (default = OFF)")),
    st(SETTING_FIRST_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.first_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 15, null_mut(),
        cstr(b"First level"), cstr(b"Level that will be loaded when starting a new game.\n(default = 1)")),
    st(SETTING_SKIP_TITLE, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(custom_saved.skip_title) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Skip title sequence"), cstr(b"Always skip the title sequence: the first level will be loaded immediately.\n(default = OFF)")),
    st(SETTING_SHIFT_L_ALLOWED_UNTIL_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.shift_L_allowed_until_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Shift+L allowed until level"), cstr(b"First level where level skipping with Shift+L is denied in non-cheat mode.\n(default = 4)")),
    st(SETTING_SHIFT_L_REDUCED_MINUTES, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.shift_L_reduced_minutes) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT16_MAX, null_mut(),
        cstr(b"Minutes left after Shift+L used"), cstr(b"Number of minutes left after Shift+L is used in non-cheat mode.\n(default = 15)")),
    st(SETTING_SHIFT_L_REDUCED_TICKS, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.shift_L_reduced_ticks) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT16_MAX, null_mut(),
        cstr(b"Seconds left after Shift+L used"), cstr(b"Number of seconds left after Shift+L is used in non-cheat mode.\n(default = 59.92)")),
    st(SETTING_DEMO_HITP, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.demo_hitp) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT16_MAX, null_mut(),
        cstr(b"Demo level hitpoints"), cstr(b"Hitpoints the kid has on the demo level.\n(default = 4)")),
    st(SETTING_DEMO_END_ROOM, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.demo_end_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Demo level ending room"), cstr(b"Demo level ending room.\n(default = 24)")),
    st(SETTING_INTRO_MUSIC_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.intro_music_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Level with intro music"), cstr(b"Level where the presentation music is played when the kid crouches down. (default = 1)\nNote: only works if this level is the starting level.")),
    st(SETTING_HAVE_SWORD_FROM_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.have_sword_from_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Have sword from level"), cstr(b"First level (except the demo level) where kid has the sword.\n(default = 2)\n")),
    st(SETTING_CHECKPOINT_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.checkpoint_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Checkpoint level"), cstr(b"Level where there is a checkpoint. (default = 3)\nThe checkpoint is triggered when leaving room 7 to the left.")),
    st(SETTING_CHECKPOINT_RESPAWN_DIR, SETTING_STYLE_NUMBER, SETTING_SBYTE, addr_of_mut!(custom_saved.checkpoint_respawn_dir) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, -1, 0, addr_of_mut!(direction_setting_names_list),
        cstr(b"Checkpoint respawn direction"), cstr(b"Respawn direction after triggering the checkpoint.\n(default = left)")),
    st(SETTING_CHECKPOINT_RESPAWN_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.checkpoint_respawn_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Checkpoint respawn room"), cstr(b"Room where you respawn after triggering the checkpoint.\n(default = 2)")),
    st(SETTING_CHECKPOINT_RESPAWN_TILEPOS, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.checkpoint_respawn_tilepos) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 29, null_mut(),
        cstr(b"Checkpoint respawn tile position"), cstr(b"Tile position (0 to 29) where you respawn after triggering the checkpoint.\n(default = 6)")),
    st(SETTING_CHECKPOINT_CLEAR_TILE_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.checkpoint_clear_tile_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Checkpoint clear tile room"), cstr(b"Room where a tile is cleared after respawning at the checkpoint location.\n(default = 7)")),
    st(SETTING_CHECKPOINT_CLEAR_TILE_COL, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.checkpoint_clear_tile_col) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 9, null_mut(),
        cstr(b"Checkpoint clear tile column"), cstr(b"Location (column/row) of the cleared tile after respawning at the checkpoint location.\n(default: column = 4, row = top)")),
    st(SETTING_CHECKPOINT_CLEAR_TILE_ROW, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.checkpoint_clear_tile_row) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 2, addr_of_mut!(row_setting_names_list),
        cstr(b"Checkpoint clear tile row"), cstr(b"Location (column/row) of the cleared tile after respawning at the checkpoint location.\n(default: column = 4, row = top)")),
    st(SETTING_SKELETON_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.skeleton_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Skeleton awakes level"), cstr(b"Level and room where a skeleton can come alive.\n(default: level = 3, room = 1)")),
    st(SETTING_SKELETON_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.skeleton_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Skeleton awakes room"), cstr(b"Level and room where a skeleton can come alive.\n(default: level = 3, room = 1)")),
    st(SETTING_SKELETON_TRIGGER_COLUMN_1, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.skeleton_trigger_column_1) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 9, null_mut(),
        cstr(b"Skeleton trigger column (1)"), cstr(b"The skeleton will wake up if the kid is on one of these two columns.\n(defaults = 2,3)")),
    st(SETTING_SKELETON_TRIGGER_COLUMN_2, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.skeleton_trigger_column_2) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 9, null_mut(),
        cstr(b"Skeleton trigger column (2)"), cstr(b"The skeleton will wake up if the kid is on one of these two columns.\n(defaults = 2,3)")),
    st(SETTING_SKELETON_COLUMN, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.skeleton_column) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 9, null_mut(),
        cstr(b"Skeleton tile column"), cstr(b"Location (column/row) of the skeleton tile that will awaken.\n(default: column = 5, row = middle)")),
    st(SETTING_SKELETON_ROW, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.skeleton_row) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 2, addr_of_mut!(row_setting_names_list),
        cstr(b"Skeleton tile row"), cstr(b"Location (column/row) of the skeleton tile that will awaken.\n(default: column = 5, row = middle)")),
    st(SETTING_SKELETON_REQUIRE_OPEN_LEVEL_DOOR, SETTING_STYLE_TOGGLE, 0, addr_of_mut!(custom_saved.skeleton_require_open_level_door) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Skeleton requires level door"), cstr(b"Whether the level door must first be opened before the skeleton awakes.\n(default = true)")),
    st(SETTING_SKELETON_SKILL, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.skeleton_skill) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 15, null_mut(),
        cstr(b"Skeleton skill"), cstr(b"Skill of the awoken skeleton.\n(default = 2)")),
    st(SETTING_SKELETON_REAPPEAR_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.skeleton_reappear_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Skeleton reappear room"), cstr(b"If the skeleton falls into this room, it will reappear there.\n(default = 3)")),
    st(SETTING_SKELETON_REAPPEAR_X, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.skeleton_reappear_x) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 255, null_mut(),
        cstr(b"Skeleton reappear X coordinate"), cstr(b"Horizontal coordinate where the skeleton reappears.\n(default = 133)\n(58 = left edge of the room, 198 = right edge)")),
    st(SETTING_SKELETON_REAPPEAR_ROW, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.skeleton_reappear_row) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 2, addr_of_mut!(row_setting_names_list),
        cstr(b"Skeleton reappear row"), cstr(b"Row on which the skeleton reappears.\n(default = middle)")),
    st(SETTING_SKELETON_REAPPEAR_DIR, SETTING_STYLE_NUMBER, SETTING_SBYTE, addr_of_mut!(custom_saved.skeleton_reappear_dir) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, -1, 0, addr_of_mut!(direction_setting_names_list),
        cstr(b"Skeleton reappear direction"), cstr(b"Direction the skeleton is facing when it reappears.\n(default = right)")),
    st(SETTING_MIRROR_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.mirror_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Mirror level"), cstr(b"Level and room where the mirror appears.\n(default: level = 4, room = 4)")),
    st(SETTING_MIRROR_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.mirror_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Mirror room"), cstr(b"Level and room where the mirror appears.\n(default: level = 4, room = 4)")),
    st(SETTING_MIRROR_COLUMN, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.mirror_column) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 9, null_mut(),
        cstr(b"Mirror column"), cstr(b"Location (column/row) of the tile where the mirror appears.\n(default: column = 4, row = top)")),
    st(SETTING_MIRROR_ROW, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.mirror_row) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 2, addr_of_mut!(row_setting_names_list),
        cstr(b"Mirror row"), cstr(b"Location (column/row) of the tile where the mirror appears.\n(default: column = 4, row = top)")),
    st(SETTING_MIRROR_TILE, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.mirror_tile) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 31, addr_of_mut!(tile_type_setting_names_list),
        cstr(b"Mirror tile"), cstr(b"Tile type that appears when the mirror should appear.\n(default = mirror)")),
    st(SETTING_SHOW_MIRROR_IMAGE, SETTING_STYLE_TOGGLE, SETTING_BYTE, addr_of_mut!(custom_saved.show_mirror_image) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Show mirror image"), cstr(b"Show the kid's mirror image in the mirror.\n(default = true)")),
    st(SETTING_SHADOW_STEAL_LEVEL, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.shadow_steal_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Shadow steal level"), cstr(b"Level where the shadow steals a potion.\n(default = 5)")),
    st(SETTING_SHADOW_STEAL_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.shadow_steal_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Shadow steal room"), cstr(b"Room where the shadow steals a potion.\n(default = 24)")),
    st(SETTING_SHADOW_STEP_LEVEL, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.shadow_step_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Shadow step level"), cstr(b"Level where the shadow steps on a button.\n(default = 6)")),
    st(SETTING_SHADOW_STEP_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.shadow_step_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Shadow step room"), cstr(b"Room where the shadow steps on a button.\n(default = 1)")),
    st(SETTING_FALLING_EXIT_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.falling_exit_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Falling exit level"), cstr(b"Level where the kid can progress to the next level by falling off a specific room.\n(default = 6)")),
    st(SETTING_FALLING_EXIT_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.falling_exit_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Falling exit room"), cstr(b"Room where the kid can progress to the next level by falling down.\n(default = 1)")),
    st(SETTING_FALLING_ENTRY_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.falling_entry_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Falling entry level"), cstr(b"If the kid starts in this level in this room, the starting room will not be shown,\nbut the room below instead, to allow for a falling entry. (default: level = 7, room = 17)")),
    st(SETTING_FALLING_ENTRY_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.falling_entry_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Falling entry room"), cstr(b"If the kid starts in this level in this room, the starting room will not be shown,\nbut the room below instead, to allow for a falling entry. (default: level = 7, room = 17)")),
    st(SETTING_MOUSE_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.mouse_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Mouse level"), cstr(b"Level where the mouse appears.\n(default = 8)")),
    st(SETTING_MOUSE_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.mouse_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Mouse room"), cstr(b"Room where the mouse appears.\n(default = 16)")),
    st(SETTING_MOUSE_DELAY, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.mouse_delay) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT16_MAX, null_mut(),
        cstr(b"Mouse delay"), cstr(b"Number of seconds to wait before the mouse appears.\n(default = 12.5)")),
    st(SETTING_MOUSE_OBJECT, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.mouse_object) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 255, null_mut(),
        cstr(b"Mouse object"), cstr(b"Mouse object type. (default = 24)\nBe careful: a value not 24 will change the mouse for the kid.")),
    st(SETTING_MOUSE_START_X, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.mouse_start_x) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 255, null_mut(),
        cstr(b"Mouse start X coordinate"), cstr(b"Horizontal starting coordinate of the mouse.\n(default = 200)")),
    st(SETTING_LOOSE_TILES_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.loose_tiles_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Loose tiles level"), cstr(b"Level where loose floor tiles will fall down.\n(default = 13)")),
    st(SETTING_LOOSE_TILES_ROOM_1, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.loose_tiles_room_1) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Loose tiles room (1)"), cstr(b"Rooms where visible loose floor tiles will fall down.\n(default = 23, 16)")),
    st(SETTING_LOOSE_TILES_ROOM_2, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.loose_tiles_room_2) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Loose tiles room (2)"), cstr(b"Rooms where visible loose floor tiles will fall down.\n(default = 23, 16)")),
    st(SETTING_LOOSE_TILES_FIRST_TILE, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.loose_tiles_first_tile) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 29, null_mut(),
        cstr(b"Loose tiles first tile"), cstr(b"Range of loose floor tile positions that will be pressed.\n(default = 22 to 27)")),
    st(SETTING_LOOSE_TILES_LAST_TILE, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.loose_tiles_last_tile) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 29, null_mut(),
        cstr(b"Loose tiles last tile"), cstr(b"Range of loose floor tile positions that will be pressed.\n(default = 22 to 27)")),
    st(SETTING_JAFFAR_VICTORY_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.jaffar_victory_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Jaffar victory level"), cstr(b"Killing the guard in this level causes the screen to flash, and event 0 to be triggered upon leaving the room.\n(default = 13)")),
    st(SETTING_JAFFAR_VICTORY_FLASH_TIME, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.jaffar_victory_flash_time) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT16_MAX, null_mut(),
        cstr(b"Jaffar victory flash time"), cstr(b"How long the screen will flash after killing Jaffar.\n(default = 18)")),
    st(SETTING_HIDE_LEVEL_NUMBER_FIRST_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.hide_level_number_from_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Hide level number from level"), cstr(b"First level where the level number will not be displayed.\n(default = 14)")),
    st(SETTING_LEVEL_13_LEVEL_NUMBER, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.level_13_level_number) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT16_MAX, null_mut(),
        cstr(b"Level 13 displayed level number"), cstr(b"Level number displayed on level 13.\n(default = 12)")),
    st(SETTING_VICTORY_STOPS_TIME_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.victory_stops_time_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Victory stops time level"), cstr(b"Level where Jaffar's death stops time.\n(default = 13)")),
    st(SETTING_WIN_LEVEL, SETTING_STYLE_NUMBER, SETTING_WORD, addr_of_mut!(custom_saved.win_level) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 16, addr_of_mut!(never_is_16_list),
        cstr(b"Level where you can win"), cstr(b"Level and room where you can win the game.\n(default: level = 14, room = 5)")),
    st(SETTING_WIN_ROOM, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.win_room) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 24, null_mut(),
        cstr(b"Room where you can win"), cstr(b"Level and room where you can win the game.\n(default: level = 14, room = 5)")),
    st(SETTING_LOOSE_FLOOR_DELAY, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.loose_floor_delay) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 127, null_mut(),
        cstr(b"Loose floor delay"), cstr(b"Number of seconds to wait before a loose floor falls.\n(default = 0.92)")),
    st(SETTING_BASE_SPEED, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.base_speed) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 127, null_mut(),
        cstr(b"Base speed"), cstr(b"Game speed when not fighting (delay between frames in 1/60 seconds). Smaller is faster.\n(default = 5)")),
    st(SETTING_FIGHT_SPEED, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.fight_speed) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 1, 127, null_mut(),
        cstr(b"Fight speed"), cstr(b"Game speed when fighting (delay between frames in 1/60 seconds). Smaller is faster.\n(default = 6)")),
    st(SETTING_CHOMPER_SPEED, SETTING_STYLE_NUMBER, SETTING_BYTE, addr_of_mut!(custom_saved.chomper_speed) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 127, null_mut(),
        cstr(b"Chomper speed"), cstr(b"Chomper speed (length of the animation cycle in frames). Smaller is faster.\n(default = 15)")),
    st(SETTING_NO_MOUSE_IN_ENDING, SETTING_STYLE_TOGGLE, SETTING_BYTE, addr_of_mut!(custom_saved.no_mouse_in_ending) as *mut c_void, addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"No mouse in ending"), cstr(b"Skip the mouse in the ending scene.\n(default = false)")),
] };

const LEVEL_N: usize = 8;
static mut level_settings: [setting_type; LEVEL_N] = unsafe { [
    st(SETTING_LEVEL_SETTINGS, SETTING_STYLE_TEXT_ONLY, 0, null_mut(), addr_of_mut!(use_custom_options) as *mut c_void, 0, 0, null_mut(),
        cstr(b"Customize another level..."), cstr(b"Select another level to customize.")),
    st(SETTING_LEVEL_TYPE, SETTING_STYLE_NUMBER, SETTING_BYTE, null_mut(), addr_of_mut!(use_custom_options) as *mut c_void, 0, 1, addr_of_mut!(level_type_setting_names_list),
        cstr(b"Level type"), cstr(b"Which environment is used in this level.\n(either dungeon or palace)")),
    st(SETTING_LEVEL_COLOR, SETTING_STYLE_NUMBER, SETTING_WORD, null_mut(), addr_of_mut!(use_custom_options) as *mut c_void, 0, 4, null_mut(),
        cstr(b"Level color palette"), cstr(b"0: colors from VDUNGEON.DAT/VPALACE.DAT\n>0: colors from PRINCE.DAT.\nYou need a PRINCE.DAT from PoP 1.3 or 1.4 for this.")),
    st(SETTING_GUARD_TYPE, SETTING_STYLE_NUMBER, SETTING_SHORT, null_mut(), addr_of_mut!(use_custom_options) as *mut c_void, -1, 4, addr_of_mut!(guard_type_setting_names_list),
        cstr(b"Guard type"), cstr(b"Guard type used in this level (normal, fat, skeleton, vizier, or shadow).")),
    st(SETTING_GUARD_HP, SETTING_STYLE_NUMBER, SETTING_BYTE, null_mut(), addr_of_mut!(use_custom_options) as *mut c_void, 0, UINT8_MAX, null_mut(),
        cstr(b"Guard hitpoints"), cstr(b"Number of hitpoints guards have in this level.")),
    st(SETTING_CUTSCENE, SETTING_STYLE_NUMBER, SETTING_BYTE, null_mut(), addr_of_mut!(use_custom_options) as *mut c_void, 0, 15, null_mut(),
        cstr(b"Cutscene before level"), cstr(b"Cutscene that plays between the previous level and this level.\n0: none, 2 or 6: standing, 4: lying down, 8: mouse leaves,\n9: mouse returns, 12: standing or turn around")),
    st(SETTING_ENTRY_POSE, SETTING_STYLE_NUMBER, SETTING_BYTE, null_mut(), addr_of_mut!(use_custom_options) as *mut c_void, 0, 2, addr_of_mut!(entry_pose_setting_names_list),
        cstr(b"Entry pose"), cstr(b"The pose the kid has when the level starts.\n")),
    st(SETTING_SEAMLESS_EXIT, SETTING_STYLE_NUMBER, SETTING_SBYTE, null_mut(), addr_of_mut!(use_custom_options) as *mut c_void, -1, 24, addr_of_mut!(off_setting_name_list),
        cstr(b"Seamless exit"), cstr(b"Entering this room moves the kid to the next level.\nSet to -1 to disable.")),
] };

const CONTROLS_N: usize = 9;
static mut controls_settings: [setting_type; CONTROLS_N] = unsafe { [
    st(SETTING_KEY_LEFT, SETTING_STYLE_KEY, SETTING_INT, addr_of_mut!(key_left) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Left"), cstr(b"")),
    st(SETTING_KEY_RIGHT, SETTING_STYLE_KEY, SETTING_INT, addr_of_mut!(key_right) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Right"), cstr(b"")),
    st(SETTING_KEY_UP, SETTING_STYLE_KEY, SETTING_INT, addr_of_mut!(key_up) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Up"), cstr(b"")),
    st(SETTING_KEY_DOWN, SETTING_STYLE_KEY, SETTING_INT, addr_of_mut!(key_down) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Down"), cstr(b"")),
    st(SETTING_KEY_JUMP_LEFT, SETTING_STYLE_KEY, SETTING_INT, addr_of_mut!(key_jump_left) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Jump left"), cstr(b"")),
    st(SETTING_KEY_JUMP_RIGHT, SETTING_STYLE_KEY, SETTING_INT, addr_of_mut!(key_jump_right) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Jump right"), cstr(b"")),
    st(SETTING_KEY_ACTION, SETTING_STYLE_KEY, SETTING_INT, addr_of_mut!(key_action) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Action"), cstr(b"")),
    st(SETTING_KEY_ENTER, SETTING_STYLE_KEY, SETTING_INT, addr_of_mut!(key_enter) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Enter a menu"), cstr(b"")),
    st(SETTING_KEY_ESC, SETTING_STYLE_KEY, SETTING_INT, addr_of_mut!(key_esc) as *mut c_void, null_mut(), 0, 0, null_mut(),
        cstr(b"Exit a menu, pause"), cstr(b"")),
] };

static mut general_settings_area: settings_area_type = unsafe { settings_area_type { settings: addr_of_mut!(general_settings) as *mut setting_type, setting_count: GENERAL_N as c_int } };
static mut gameplay_settings_area: settings_area_type = unsafe { settings_area_type { settings: addr_of_mut!(gameplay_settings) as *mut setting_type, setting_count: GAMEPLAY_N as c_int } };
static mut visuals_settings_area: settings_area_type = unsafe { settings_area_type { settings: addr_of_mut!(visuals_settings) as *mut setting_type, setting_count: VISUALS_N as c_int } };
static mut mods_settings_area: settings_area_type = unsafe { settings_area_type { settings: addr_of_mut!(mods_settings) as *mut setting_type, setting_count: MODS_N as c_int } };
static mut level_settings_area: settings_area_type = unsafe { settings_area_type { settings: addr_of_mut!(level_settings) as *mut setting_type, setting_count: LEVEL_N as c_int } };
static mut controls_settings_area: settings_area_type = unsafe { settings_area_type { settings: addr_of_mut!(controls_settings) as *mut setting_type, setting_count: CONTROLS_N as c_int } };

unsafe fn get_settings_area(menu_item_id: c_int) -> *mut settings_area_type {
    match menu_item_id {
        SETTINGS_MENU_GENERAL => addr_of_mut!(general_settings_area),
        SETTINGS_MENU_GAMEPLAY => addr_of_mut!(gameplay_settings_area),
        SETTINGS_MENU_VISUALS => addr_of_mut!(visuals_settings_area),
        SETTINGS_MENU_MODS => addr_of_mut!(mods_settings_area),
        SETTINGS_MENU_LEVEL_CUSTOMIZATION => addr_of_mut!(level_settings_area),
        SETTINGS_MENU_CONTROLS => addr_of_mut!(controls_settings_area),
        _ => null_mut(),
    }
}

unsafe fn init_pause_menu_items(first_item: *mut pause_menu_item_type, item_count: c_int) {
    if item_count > 0 {
        for i in 0..item_count {
            let item = first_item.add(i as usize);
            (*item).previous = first_item.add(std::cmp::max(0, i - 1) as usize);
            (*item).next = first_item.add(std::cmp::min(item_count - 1, i + 1) as usize);
        }
        let last_item = first_item.add((item_count - 1) as usize);
        (*first_item).previous = last_item;
        (*last_item).next = first_item;
    }
}

unsafe fn init_settings_list(first_setting: *mut setting_type, setting_count: c_int) {
    if setting_count > 0 {
        for i in 0..setting_count {
            let item = first_setting.add(i as usize);
            (*item).index = i;
            (*item).previous = (*first_setting.add(std::cmp::max(0, i - 1) as usize)).id;
            (*item).next = (*first_setting.add(std::cmp::min(setting_count - 1, i + 1) as usize)).id;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn init_menu() {
    load_arrowhead_images();

    init_pause_menu_items(addr_of_mut!(pause_menu_items) as *mut pause_menu_item_type, PAUSE_MENU_ITEMS_N as c_int);
    init_pause_menu_items(addr_of_mut!(settings_menu_items) as *mut pause_menu_item_type, SETTINGS_MENU_ITEMS_N as c_int);

    init_settings_list(addr_of_mut!(general_settings) as *mut setting_type, GENERAL_N as c_int);
    init_settings_list(addr_of_mut!(visuals_settings) as *mut setting_type, VISUALS_N as c_int);
    init_settings_list(addr_of_mut!(gameplay_settings) as *mut setting_type, GAMEPLAY_N as c_int);
    init_settings_list(addr_of_mut!(mods_settings) as *mut setting_type, MODS_N as c_int);
    init_settings_list(addr_of_mut!(level_settings) as *mut setting_type, LEVEL_N as c_int);
    init_settings_list(addr_of_mut!(controls_settings) as *mut setting_type, CONTROLS_N as c_int);
}

unsafe fn is_mouse_over_rect(rect: *const rect_type) -> bool {
    mouse_x >= (*rect).left as c_int
        && mouse_x < (*rect).right as c_int
        && mouse_y >= (*rect).top as c_int
        && mouse_y < (*rect).bottom as c_int
}

// Maps the cursor position into a coordinate between (0,0) and (320,200).
unsafe fn read_mouse_state() {
    let mut scale_x: f32 = 0.0;
    let mut scale_y: f32 = 0.0;
    SDL_RenderGetScale(renderer_, &mut scale_x, &mut scale_y);
    let mut logical_width: c_int = 0;
    let mut logical_height: c_int = 0;
    SDL_RenderGetLogicalSize(renderer_, &mut logical_width, &mut logical_height);
    let logical_scale_x = logical_width / 320;
    let logical_scale_y = logical_height / 200;
    scale_x *= logical_scale_x as f32;
    scale_y *= logical_scale_y as f32;
    if !(scale_x > 0.0 && scale_y > 0.0 && logical_scale_x > 0 && logical_scale_y > 0) {
        return;
    }
    let mut viewport: SDL_Rect = core::mem::zeroed();
    SDL_RenderGetViewport(renderer_, &mut viewport);
    viewport.x /= logical_scale_x;
    viewport.y /= logical_scale_y;
    let last_mouse_x = mouse_x;
    let last_mouse_y = mouse_y;
    SDL_GetMouseState(&mut mouse_x, &mut mouse_y);
    mouse_x = (mouse_x as f32 / scale_x - viewport.x as f32 + 0.5) as c_int;
    mouse_y = (mouse_y as f32 / scale_y - viewport.y as f32 + 0.5) as c_int;
    mouse_moved = last_mouse_x != mouse_x || last_mouse_y != mouse_y;
}

unsafe fn play_menu_sound(sound_id: c_int) {
    play_sound(sound_id);
    play_next_sound();
}

unsafe fn enter_settings_subsection(settings_menu_id: c_int) {
    let settings_area = get_settings_area(settings_menu_id);
    if active_settings_subsection != settings_menu_id {
        highlighted_setting_id = (*(*settings_area).settings.add(0)).id;
    }
    active_settings_subsection = settings_menu_id;
    highlighted_settings_subsection = settings_menu_id;
    if !mouse_clicked {
        hovering_pause_menu_item = 0;
    }
    controlled_area = 1;
    scroll_position = 0;

    if settings_menu_id == SETTINGS_MENU_LEVEL_CUSTOMIZATION {
        let lvl = menu_current_level as usize;
        for i in 0..(*settings_area).setting_count {
            let setting = (*settings_area).settings.add(i as usize);
            match (*setting).id {
                SETTING_LEVEL_TYPE => { (*setting).linked = addr_of_mut!(custom_saved.tbl_level_type[lvl]) as *mut c_void; }
                SETTING_LEVEL_COLOR => { (*setting).linked = addr_of_mut!(custom_saved.tbl_level_color[lvl]) as *mut c_void; }
                SETTING_GUARD_TYPE => { (*setting).linked = addr_of_mut!(custom_saved.tbl_guard_type[lvl]) as *mut c_void; }
                SETTING_GUARD_HP => { (*setting).linked = addr_of_mut!(custom_saved.tbl_guard_hp[lvl]) as *mut c_void; }
                SETTING_CUTSCENE => { (*setting).linked = addr_of_mut!(custom_saved.tbl_cutscenes_by_index[lvl]) as *mut c_void; }
                SETTING_ENTRY_POSE => { (*setting).linked = addr_of_mut!(custom_saved.tbl_entry_pose[lvl]) as *mut c_void; }
                SETTING_SEAMLESS_EXIT => { (*setting).linked = addr_of_mut!(custom_saved.tbl_seamless_exit[lvl]) as *mut c_void; }
                _ => {}
            }
        }
    }
}

unsafe fn leave_settings_subsection() {
    if active_settings_subsection == SETTINGS_MENU_LEVEL_CUSTOMIZATION {
        enter_settings_subsection(SETTINGS_MENU_MODS);
    } else {
        controlled_area = 0;
        hovering_pause_menu_item = active_settings_subsection;
        active_settings_subsection = 0;
        highlighted_settings_subsection = 0;
    }
}

unsafe fn reset_paused_menu() {
    drawn_menu = 0;
    controlled_area = 0;
    hovering_pause_menu_item = PAUSE_MENU_RESUME;
}

unsafe fn pause_menu_clicked(item: *mut pause_menu_item_type) {
    play_menu_sound(soundids_sound_22_loose_shake_3 as c_int);
    match (*item).id {
        PAUSE_MENU_RESUME => {
            need_close_menu = true;
        }
        PAUSE_MENU_SAVE_GAME => {
            if Kid.alive < 0 {
                need_quick_save = 1;
            }
            need_close_menu = true;
        }
        PAUSE_MENU_LOAD_GAME => {
            need_quick_load = 1;
            need_close_menu = true;
            stop_sounds();
        }
        PAUSE_MENU_RESTART_LEVEL => {
            last_key_scancode = SDL_SCANCODE_A | WITH_CTRL;
        }
        PAUSE_MENU_SETTINGS => {
            drawn_menu = 1;
            hovering_pause_menu_item = SETTINGS_MENU_GENERAL;
            highlighted_settings_subsection = SETTINGS_MENU_GENERAL;
            active_settings_subsection = 0;
            controlled_area = 0;
        }
        PAUSE_MENU_RESTART_GAME => {
            last_key_scancode = SDL_SCANCODE_R | WITH_CTRL;
        }
        PAUSE_MENU_QUIT_GAME => {
            current_dialog_box = DIALOG_CONFIRM_QUIT;
            current_dialog_text = cs!("Quit SDLPoP?");
        }
        SETTINGS_MENU_GENERAL | SETTINGS_MENU_GAMEPLAY | SETTINGS_MENU_VISUALS | SETTINGS_MENU_MODS | SETTINGS_MENU_CONTROLS => {
            enter_settings_subsection((*item).id);
        }
        SETTINGS_MENU_BACK => {
            reset_paused_menu();
            hovering_pause_menu_item = PAUSE_MENU_SETTINGS;
        }
        _ => {}
    }
    clear_menu_controls();
}

unsafe fn draw_pause_menu_item(
    item: *mut pause_menu_item_type,
    parent: *const rect_type,
    y_offset: *mut c_int,
    inactive_text_color: c_int,
) {
    if !(*item).required.is_null() {
        if *((*item).required as *const i8) == 0 {
            return; // skip this item (disabled)
        }
    }

    let mut text_rect = *parent;
    text_rect.top = text_rect.top.wrapping_add(*y_offset as c_short);
    let mut text_color = inactive_text_color;

    let mut selection_box = text_rect;
    selection_box.bottom = selection_box.top + 8;
    selection_box.top -= 3;

    let mut highlighted = hovering_pause_menu_item == (*item).id;
    if have_mouse_input && is_mouse_over_rect(&selection_box) {
        hovering_pause_menu_item = (*item).id;
        highlighted = true;
    }

    if highlighted {
        previous_pause_menu_item = (*item).previous;
        next_pause_menu_item = (*item).next;
        // Skip over disabled items.
        if !(*previous_pause_menu_item).required.is_null() {
            while *((*previous_pause_menu_item).required as *const i8) == 0 {
                previous_pause_menu_item = (*previous_pause_menu_item).previous;
                if (*previous_pause_menu_item).required.is_null() {
                    break;
                }
            }
        }
        if !(*next_pause_menu_item).required.is_null() {
            while *((*next_pause_menu_item).required as *const i8) == 0 {
                next_pause_menu_item = (*next_pause_menu_item).next;
                if (*next_pause_menu_item).required.is_null() {
                    break;
                }
            }
        }
        text_color = colorids_color_15_brightwhite as c_int;
        draw_rect_contours(&selection_box, colorids_color_7_lightgray as u8);

        if mouse_clicked {
            if is_mouse_over_rect(&selection_box) {
                pause_menu_clicked(item);
            }
        } else if pressed_enter && (drawn_menu == 0 || (drawn_menu == 1 && controlled_area == 0)) {
            pause_menu_clicked(item);
        }
    }
    show_text_with_color(&text_rect, halign_center, valign_top, addr_of!((*item).text) as *const c_char, text_color);
    *y_offset += 13;
}

unsafe fn draw_pause_menu() {
    pause_menu_alpha = 120;
    draw_rect_with_alpha(addr_of!(screen_rect), colorids_color_0_black as u8, pause_menu_alpha);
    draw_rect_with_alpha(addr_of!(rect_bottom_text), colorids_color_0_black as u8, 0);
    let pause_rect_outer = rect_type { top: 0, left: 110, bottom: 192, right: 210 };
    let mut pause_rect_inner: rect_type = core::mem::zeroed();
    shrink2_rect(&mut pause_rect_inner, &pause_rect_outer, 5, 5);

    if !have_mouse_input {
        if menu_control_y == 1 {
            play_menu_sound(soundids_sound_21_loose_shake_2 as c_int);
            hovering_pause_menu_item = (*next_pause_menu_item).id;
        } else if menu_control_y == -1 {
            play_menu_sound(soundids_sound_21_loose_shake_2 as c_int);
            hovering_pause_menu_item = (*previous_pause_menu_item).id;
        }
    }

    let mut y_offset: c_int = 50;
    for i in 0..PAUSE_MENU_ITEMS_N {
        draw_pause_menu_item(addr_of_mut!(pause_menu_items[i]), &pause_rect_inner, &mut y_offset, colorids_color_15_brightwhite as c_int);
    }
}

unsafe fn turn_setting_on_off(setting_id: c_int, new_state: u8, linked: *mut c_void) {
    were_settings_changed = true;
    match setting_id {
        SETTING_FULLSCREEN => {
            start_fullscreen = new_state;
            SDL_SetWindowFullscreen(window_, (new_state != 0) as u32 * SDL_WINDOW_FULLSCREEN_DESKTOP);
        }
        SETTING_USE_CORRECT_ASPECT_RATIO => {
            use_correct_aspect_ratio = new_state;
            apply_aspect_ratio();
        }
        SETTING_USE_INTEGER_SCALING => {
            use_integer_scaling = new_state;
            if new_state != 0 {
                window_resized();
            } else {
                SDL_RenderSetIntegerScale(renderer_, SDL_FALSE);
            }
        }
        SETTING_ENABLE_LIGHTING => {
            enable_lighting = new_state;
            if new_state != 0 && lighting_mask.is_null() {
                init_lighting();
            }
            need_full_redraw = 1;
        }
        SETTING_ENABLE_SOUND => {
            turn_sound_on_off(((new_state != 0) as u8) * 15);
        }
        SETTING_ENABLE_MUSIC => {
            turn_music_on_off(new_state);
        }
        SETTING_USE_FIXES_AND_ENHANCEMENTS => {
            turn_fixes_and_enhancements_on_off(new_state);
        }
        SETTING_USE_CUSTOM_OPTIONS => {
            turn_custom_options_on_off(new_state);
        }
        _ => {
            if !linked.is_null() {
                *(linked as *mut u8) = new_state;
            }
        }
    }
}

unsafe fn turn_setting_on_off_with_sound(setting: *mut setting_type, new_state: u8) {
    play_menu_sound(soundids_sound_10_sword_vs_sword as c_int);
    turn_setting_on_off((*setting).id, new_state, (*setting).linked);
}

unsafe fn get_setting_value(setting: *mut setting_type) -> c_int {
    let mut value = 0;
    if !(*setting).linked.is_null() {
        value = match (*setting).number_type {
            SETTING_SBYTE => *((*setting).linked as *const i8) as c_int,
            SETTING_WORD => *((*setting).linked as *const u16) as c_int,
            SETTING_SHORT => *((*setting).linked as *const i16) as c_int,
            SETTING_INT => *((*setting).linked as *const c_int),
            _ => *((*setting).linked as *const u8) as c_int,
        };
    }
    value
}

unsafe fn set_setting_value(setting: *mut setting_type, value: c_int) {
    if !(*setting).linked.is_null() {
        match (*setting).number_type {
            SETTING_SBYTE => *((*setting).linked as *mut i8) = value as i8,
            SETTING_WORD => *((*setting).linked as *mut u16) = value as u16,
            SETTING_SHORT => *((*setting).linked as *mut i16) = value as i16,
            SETTING_INT => *((*setting).linked as *mut c_int) = value,
            _ => *((*setting).linked as *mut u8) = value as u8,
        }
    }
}

unsafe fn increase_setting(setting: *mut setting_type, old_value: c_int) {
    let new_value = if (*setting).id == SETTING_JOYSTICK_THRESHOLD {
        ((old_value / 1000) + 1) * 1000
    } else {
        old_value + 1
    };
    if !(*setting).linked.is_null() && new_value <= (*setting).max {
        were_settings_changed = true;
        set_setting_value(setting, new_value);
    }
}

unsafe fn decrease_setting(setting: *mut setting_type, old_value: c_int) {
    let new_value = if (*setting).id == SETTING_JOYSTICK_THRESHOLD {
        (((old_value + 999) / 1000) - 1) * 1000
    } else {
        old_value - 1
    };
    if !(*setting).linked.is_null() && new_value >= (*setting).min {
        were_settings_changed = true;
        set_setting_value(setting, new_value);
    }
}

unsafe fn draw_setting_explanation(setting: *mut setting_type) {
    show_text_with_color(&explanation_rect, halign_center, valign_top, addr_of!((*setting).explanation) as *const c_char, colorids_color_7_lightgray as c_int);
}

unsafe fn draw_image_with_blending(image: *mut image_type, xpos: c_int, ypos: c_int) {
    let src_rect = SDL_Rect { x: 0, y: 0, w: (*image).w, h: (*image).h };
    let mut dest_rect = SDL_Rect { x: xpos, y: ypos, w: (*image).w, h: (*image).h };
    SDL_SetColorKey(image, SDL_TRUE, 0);
    if SDL_BlitSurface(image, &src_rect, current_target_surface, &mut dest_rect) != 0 {
        sdlperror(cs!("SDL_BlitSurface"));
        quit(1);
    }
}

unsafe fn print_setting_value_(setting: *mut setting_type, value: c_int, buffer: *mut c_char, buffer_size: usize) -> *mut c_char {
    let mut has_name = false;
    let list = (*setting).names_list;
    let max_len = std::cmp::min(MAX_OPTION_VALUE_NAME_LENGTH as usize, buffer_size);
    if !list.is_null() {
        if (*list).type_ == 0 && value >= 0 && value < (*list).__bindgen_anon_1.names.count as c_int {
            let base = (*list).__bindgen_anon_1.names.data as *const [c_char; 20];
            strncpy(buffer, base.add(value as usize) as *const c_char, max_len);
            has_name = true;
        } else if (*list).type_ == 1 {
            let n = (*list).__bindgen_anon_1.kv_pairs.count as c_int;
            for i in 0..n {
                let kv = (*list).__bindgen_anon_1.kv_pairs.data.add(i as usize);
                if value == (*kv).value {
                    strncpy(buffer, addr_of!((*kv).key) as *const c_char, max_len);
                    has_name = true;
                    break;
                }
            }
        }
    }
    if !has_name {
        if (*setting).id == SETTING_START_TICKS_LEFT
            || (*setting).id == SETTING_SHIFT_L_REDUCED_TICKS
            || (*setting).id == SETTING_MOUSE_DELAY
            || (*setting).id == SETTING_LOOSE_FLOOR_DELAY
        {
            let seconds = (value as f32) * (1.0f32 / 12.0f32);
            snprintf(buffer, buffer_size, cs!("%.2f"), seconds as f64);
        } else {
            snprintf(buffer, buffer_size, cs!("%d"), value);
        }
    }
    buffer
}

unsafe fn draw_setting(setting: *mut setting_type, parent: *const rect_type, y_offset: *mut c_int, inactive_text_color: c_int) {
    let mut text_rect = *parent;
    text_rect.top = text_rect.top.wrapping_add(*y_offset as c_short);
    let mut text_color = inactive_text_color;
    let selected_color = colorids_color_15_brightwhite as c_int;
    let unselected_color = colorids_color_7_lightgray as c_int;

    let mut setting_box = text_rect;
    setting_box.top -= 5;
    setting_box.bottom = setting_box.top + 15;
    setting_box.left -= 10;
    setting_box.right += 10;

    if mouse_clicked && is_mouse_over_rect(&setting_box) {
        highlighted_setting_id = (*setting).id;
        controlled_area = 1;
    }

    if highlighted_setting_id == (*setting).id {
        next_setting_id = (*setting).next;
        previous_setting_id = (*setting).previous;
        at_scroll_up_boundary = (*setting).index == scroll_position;
        at_scroll_down_boundary = (*setting).index == scroll_position + 8;

        let mut dest_rect: SDL_Rect = core::mem::zeroed();
        rect_to_sdlrect(&setting_box, &mut dest_rect);
        let rgb_color = SDL_MapRGBA((*overlay_surface).format, 55, 55, 55, 255);
        if SDL_FillRect(overlay_surface, &dest_rect, rgb_color) != 0 {
            sdlperror(cs!("draw_setting: SDL_FillRect"));
            quit(1);
        }
        let mut left_side_of_setting_box = setting_box;
        left_side_of_setting_box.left = setting_box.left - 2;
        left_side_of_setting_box.right = setting_box.left;
        draw_rect(&left_side_of_setting_box, colorids_color_15_brightwhite as c_int);
        draw_setting_explanation(setting);
    }

    let mut disabled = false;
    if !(*setting).required.is_null() {
        disabled = *((*setting).required as *const u8) == 0;
    }
    if disabled {
        text_color = colorids_color_7_lightgray as c_int;
    }

    show_text_with_color(&text_rect, halign_left, valign_top, addr_of!((*setting).text) as *const c_char, text_color);

    if (*setting).style as c_int == SETTING_STYLE_TOGGLE && !disabled {
        let mut setting_enabled = true;
        if !(*setting).linked.is_null() {
            setting_enabled = *((*setting).linked as *const u8) != 0;
        }

        if highlighted_setting_id == (*setting).id {
            if mouse_clicked {
                if !setting_enabled {
                    let mut on_hitbox = setting_box;
                    on_hitbox.left = setting_box.right - 22;
                    if is_mouse_over_rect(&on_hitbox) {
                        turn_setting_on_off_with_sound(setting, 1);
                        setting_enabled = false;
                    }
                } else {
                    let mut off_hitbox = setting_box;
                    off_hitbox.left = setting_box.right - 49;
                    off_hitbox.right = setting_box.right - 22;
                    if is_mouse_over_rect(&off_hitbox) {
                        turn_setting_on_off_with_sound(setting, 0);
                        setting_enabled = true;
                    }
                }
            } else if setting_enabled && menu_control_x < 0 {
                turn_setting_on_off_with_sound(setting, 0);
                setting_enabled = false;
            } else if !setting_enabled && menu_control_x > 0 {
                turn_setting_on_off_with_sound(setting, 1);
                setting_enabled = true;
            }
        }

        let off_color = if setting_enabled { unselected_color } else { selected_color };
        let on_color = if setting_enabled { selected_color } else { unselected_color };
        show_text_with_color(&text_rect, halign_right, valign_top, cs!("ON"), on_color);
        text_rect.right -= 15;
        show_text_with_color(&text_rect, halign_right, valign_top, cs!("OFF"), off_color);
    } else if (*setting).style as c_int == SETTING_STYLE_NUMBER && !disabled {
        let mut value = get_setting_value(setting);
        if highlighted_setting_id == (*setting).id {
            if mouse_clicked {
                let right_hitbox = rect_type {
                    top: setting_box.top,
                    left: text_rect.right - 5,
                    bottom: setting_box.bottom,
                    right: text_rect.right + 10,
                };
                if is_mouse_over_rect(&right_hitbox) {
                    increase_setting(setting, value);
                } else {
                    let mut vbuf = [0 as c_char; 32];
                    let value_text = print_setting_value_(setting, value, vbuf.as_mut_ptr(), 32);
                    let value_text_width = get_line_width(value_text, strlen(value_text) as c_int);
                    let mut left_hitbox = right_hitbox;
                    left_hitbox.left = (left_hitbox.left as c_int - (value_text_width + 10)) as c_short;
                    left_hitbox.right = (left_hitbox.right as c_int - (value_text_width + 5)) as c_short;
                    if is_mouse_over_rect(&left_hitbox) {
                        decrease_setting(setting, value);
                    }
                }
            } else if menu_control_x > 0 {
                increase_setting(setting, value);
            } else if menu_control_x < 0 {
                decrease_setting(setting, value);
            }
        }

        value = get_setting_value(setting); // May have been updated.
        let mut vbuf2 = [0 as c_char; 32];
        let value_text = print_setting_value_(setting, value, vbuf2.as_mut_ptr(), 32);
        show_text_with_color(&text_rect, halign_right, valign_top, value_text, selected_color);

        if highlighted_setting_id == (*setting).id {
            let value_text_width = get_line_width(value_text, strlen(value_text) as c_int);
            draw_image_with_blending(arrowhead_right_image, text_rect.right as c_int + 2, text_rect.top as c_int);
            draw_image_with_blending(arrowhead_left_image, text_rect.right as c_int - value_text_width - 6, text_rect.top as c_int);
        }
    } else if (*setting).style as c_int == SETTING_STYLE_KEY && !disabled {
        let value = get_setting_value(setting);
        let mut value_text = [0 as c_char; 256];
        snprintf(value_text.as_mut_ptr(), 256, cs!("%s (%d)"), SDL_GetScancodeName(value as u32), value);
        show_text_with_color(&text_rect, 1, -1, value_text.as_ptr(), selected_color);
    } else {
        // show text only
        if highlighted_setting_id == (*setting).id
            && ((*setting).required.is_null() || *((*setting).required as *const i8) != 0)
        {
            if pressed_enter || (mouse_clicked && is_mouse_over_rect(&setting_box)) {
                if (*setting).id == SETTING_RESET_ALL_SETTINGS {
                    play_menu_sound(soundids_sound_22_loose_shake_3 as c_int);
                    current_dialog_box = DIALOG_RESTORE_DEFAULT_SETTINGS;
                    current_dialog_text = cs!("Restore all settings to their default values?");
                } else if (*setting).id == SETTING_LEVEL_SETTINGS {
                    play_menu_sound(soundids_sound_22_loose_shake_3 as c_int);
                    current_dialog_box = DIALOG_SELECT_LEVEL;
                }
            }
        }
    }

    *y_offset += 15;
}

unsafe fn handle_setting(setting: *mut setting_type, parent: *const rect_type, y_offset: *mut c_int, _inactive_text_color: c_int) {
    let mut text_rect = *parent;
    text_rect.top = text_rect.top.wrapping_add(*y_offset as c_short);

    let mut setting_box = text_rect;
    setting_box.top -= 5;
    setting_box.bottom = setting_box.top + 15;
    setting_box.left -= 10;
    setting_box.right += 10;

    let mut disabled = false;
    if !(*setting).required.is_null() {
        disabled = *((*setting).required as *const u8) == 0;
    }

    if (*setting).style as c_int == SETTING_STYLE_KEY && !disabled {
        if highlighted_setting_id == (*setting).id {
            if pressed_enter || (mouse_clicked && is_mouse_over_rect(&setting_box)) {
                let value_before = get_setting_value(setting);
                redefine_key(addr_of!((*setting).text) as *const c_char, (*setting).linked as *mut c_int);
                let value_after = get_setting_value(setting);
                if value_before != value_after {
                    were_settings_changed = true;
                }
            }
        }
    }

    *y_offset += 15;
}

#[no_mangle]
pub unsafe extern "C" fn menu_scroll(y: c_int) {
    let current_settings_area = get_settings_area(active_settings_subsection);
    if !current_settings_area.is_null() {
        let max_scroll = std::cmp::max(0, (*current_settings_area).setting_count - 9);
        if drawn_menu == 1 && controlled_area == 1 {
            if y < 0 && scroll_position > 0 {
                scroll_position -= 1;
            } else if y > 0 && scroll_position < max_scroll {
                scroll_position += 1;
            }
        }
    }
}

unsafe fn draw_settings_area(settings_area: *mut settings_area_type) {
    if settings_area.is_null() {
        return;
    }
    let mut settings_area_rect = rect_type { top: 0, left: 80, bottom: 170, right: 320 };
    shrink2_rect(addr_of_mut!(settings_area_rect), addr_of!(settings_area_rect), 20, 20);

    let mut start_y_offset: c_int = 0;

    if active_settings_subsection == SETTINGS_MENU_LEVEL_CUSTOMIZATION {
        start_y_offset = 15;
        let mut level_text = [0 as c_char; 16];
        snprintf(level_text.as_mut_ptr(), 16, cs!("LEVEL %d"), menu_current_level as c_int);
        show_text_with_color(&settings_area_rect, halign_center, valign_top, level_text.as_ptr(), colorids_color_15_brightwhite as c_int);
    }

    let mut y_offset = start_y_offset;
    let mut num_drawn_settings = 0;
    {
        let mut i = 0;
        while i < (*settings_area).setting_count && num_drawn_settings < 9 {
            if i >= scroll_position {
                num_drawn_settings += 1;
                draw_setting((*settings_area).settings.add(i as usize), &settings_area_rect, &mut y_offset, colorids_color_15_brightwhite as c_int);
            }
            i += 1;
        }
    }

    y_offset = start_y_offset;
    num_drawn_settings = 0;
    {
        let mut i = 0;
        while i < (*settings_area).setting_count && num_drawn_settings < 9 {
            if i >= scroll_position {
                num_drawn_settings += 1;
                handle_setting((*settings_area).settings.add(i as usize), &settings_area_rect, &mut y_offset, colorids_color_15_brightwhite as c_int);
            }
            i += 1;
        }
    }

    if scroll_position > 0 {
        draw_image_with_blending(arrowhead_up_image, 200, 10);
    }
    if scroll_position + num_drawn_settings < (*settings_area).setting_count {
        draw_image_with_blending(arrowhead_down_image, 200, 151);
    }

    // Draw a scroll bar if needed.
    if num_drawn_settings < (*settings_area).setting_count {
        let scrollbar_width: c_short = 2;
        let scrollbar_rect = rect_type {
            top: settings_area_rect.top - 5,
            bottom: settings_area_rect.bottom,
            left: settings_area_rect.right + 10 - scrollbar_width,
            right: settings_area_rect.right + 10,
        };
        method_5_rect(&scrollbar_rect, blitters_blitters_0_no_transp as c_int, colorids_color_8_darkgray as u8);

        let scrollbar_height = (scrollbar_rect.bottom - scrollbar_rect.top) as c_int;
        let count = (*settings_area).setting_count;
        let scrollbar_slider_rect = rect_type {
            top: (scrollbar_rect.top as c_int + scroll_position * scrollbar_height / count) as c_short,
            bottom: (scrollbar_rect.top as c_int + (scroll_position + num_drawn_settings) * scrollbar_height / count) as c_short,
            left: scrollbar_rect.left,
            right: scrollbar_rect.right,
        };
        method_5_rect(&scrollbar_slider_rect, blitters_blitters_0_no_transp as c_int, colorids_color_7_lightgray as u8);
    }
}

unsafe fn draw_settings_menu() {
    let settings_area = get_settings_area(active_settings_subsection);
    pause_menu_alpha = if settings_area.is_null() { 220 } else { 255 };
    draw_rect_with_alpha(addr_of!(screen_rect), colorids_color_0_black as u8, pause_menu_alpha);

    let pause_rect_outer = rect_type { top: 0, left: 10, bottom: 192, right: 80 };
    let mut pause_rect_inner: rect_type = core::mem::zeroed();
    shrink2_rect(&mut pause_rect_inner, &pause_rect_outer, 5, 5);

    if !have_mouse_input {
        let mut hovering_item_changed = false;
        if controlled_area == 0 {
            let old_hovering_item_id = hovering_pause_menu_item;
            if menu_control_y == 1 {
                hovering_pause_menu_item = (*next_pause_menu_item).id;
            } else if menu_control_y == -1 {
                hovering_pause_menu_item = (*previous_pause_menu_item).id;
            }
            if old_hovering_item_id != hovering_pause_menu_item {
                hovering_item_changed = true;
            }
        } else if controlled_area == 1 {
            let old_highlighted_setting_id = highlighted_setting_id;

            let current_settings_area = get_settings_area(active_settings_subsection);
            let mut highlighted_setting_index: c_int = -1;
            for i in 0..(*current_settings_area).setting_count {
                if highlighted_setting_id == (*(*current_settings_area).settings.add(i as usize)).id {
                    highlighted_setting_index = i;
                    break;
                }
            }

            let last = (*current_settings_area).setting_count - 1;
            let max_scroll = std::cmp::max(0, (*current_settings_area).setting_count - 9);

            if menu_control_y > 0 {
                highlighted_setting_index += menu_control_y;
                if highlighted_setting_index > last {
                    highlighted_setting_index = last;
                }
                if menu_control_y > 1 {
                    scroll_position += menu_control_y;
                }
            } else if menu_control_y < 0 {
                highlighted_setting_index += menu_control_y;
                if highlighted_setting_index < 0 {
                    highlighted_setting_index = 0;
                }
                if menu_control_y < -1 {
                    scroll_position += menu_control_y;
                }
            }

            if menu_control_y != 0 {
                if highlighted_setting_index - 8 > scroll_position {
                    scroll_position = highlighted_setting_index - 8;
                }
                if highlighted_setting_index < scroll_position {
                    scroll_position = highlighted_setting_index;
                }
                if scroll_position > max_scroll {
                    scroll_position = max_scroll;
                }
                if scroll_position < 0 {
                    scroll_position = 0;
                }
            }

            highlighted_setting_id = (*(*current_settings_area).settings.add(highlighted_setting_index as usize)).id;

            if old_highlighted_setting_id != highlighted_setting_id {
                hovering_item_changed = true;
            }
        }
        if hovering_item_changed {
            play_menu_sound(soundids_sound_21_loose_shake_2 as c_int);
        }
    }

    let mut y_offset: c_int = 50;
    for i in 0..SETTINGS_MENU_ITEMS_N {
        let item = addr_of_mut!(settings_menu_items[i]);
        let text_color = if highlighted_settings_subsection == (*item).id {
            colorids_color_15_brightwhite as c_int
        } else {
            colorids_color_7_lightgray as c_int
        };
        draw_pause_menu_item(addr_of_mut!(settings_menu_items[i]), &pause_rect_inner, &mut y_offset, text_color);
    }

    draw_settings_area(settings_area);
}

const DIALOG_BUTTON_CANCEL: c_int = 0;
const DIALOG_BUTTON_OK: c_int = 1;

unsafe fn confirmation_dialog_result(which_dialog: c_int, button: c_int) {
    if button == DIALOG_BUTTON_OK {
        if which_dialog == DIALOG_RESTORE_DEFAULT_SETTINGS {
            play_menu_sound(soundids_sound_10_sword_vs_sword as c_int);
            were_settings_changed = true;
            set_options_to_default();
            turn_setting_on_off(SETTING_USE_INTEGER_SCALING, use_integer_scaling, null_mut());
            turn_setting_on_off(SETTING_ENABLE_LIGHTING, enable_lighting, null_mut());
            apply_aspect_ratio();
            turn_sound_on_off(((is_sound_on != 0) as u8) * 15);
            turn_music_on_off(enable_music);
        } else if which_dialog == DIALOG_CONFIRM_QUIT {
            last_key_scancode = SDL_SCANCODE_Q | WITH_CTRL;
            key_test_quit();
        }
    } else {
        play_menu_sound(soundids_sound_22_loose_shake_3 as c_int);
    }
}

unsafe fn draw_confirmation_dialog(which_dialog: c_int, text: *const c_char) {
    let mut highlighted_button = DIALOG_BUTTON_OK;
    let mut old_highlighted_button = -1;
    loop {
        process_events();
        key_test_paused_menu(key_test_quit());
        process_additional_menu_input();

        if menu_control_back == 1 {
            confirmation_dialog_result(which_dialog, DIALOG_BUTTON_CANCEL);
            break;
        }

        if have_mouse_input {
            if is_mouse_over_rect(addr_of!(ok_highlight_rect)) {
                highlighted_button = DIALOG_BUTTON_OK;
            } else if is_mouse_over_rect(addr_of!(cancel_highlight_rect)) {
                highlighted_button = DIALOG_BUTTON_CANCEL;
            }
        }

        if menu_control_x < 0 {
            highlighted_button = DIALOG_BUTTON_OK;
        } else if menu_control_x > 0 {
            highlighted_button = DIALOG_BUTTON_CANCEL;
        } else if mouse_clicked || pressed_enter {
            confirmation_dialog_result(which_dialog, highlighted_button);
            break;
        }

        if highlighted_button != old_highlighted_button {
            old_highlighted_button = highlighted_button;
            let clear_color = SDL_MapRGBA((*current_target_surface).format, 0, 0, 0, 255);
            SDL_FillRect(overlay_surface, null(), clear_color);
            draw_rect(addr_of!((*copyprot_dialog).peel_rect), colorids_color_0_black as c_int);
            dialog_method_2_frame(copyprot_dialog);
            let mut rect: rect_type = core::mem::zeroed();
            shrink2_rect(&mut rect, addr_of!((*copyprot_dialog).text_rect), 2, 1);
            rect.bottom -= 14;
            show_text_with_color(&rect, halign_center, valign_middle, text, colorids_color_15_brightwhite as c_int);
            clear_kbd_buf();

            let highlight_rect: *const rect_type;
            let ok_text_color;
            let cancel_text_color;
            if highlighted_button == DIALOG_BUTTON_OK {
                highlight_rect = addr_of!(ok_highlight_rect);
                ok_text_color = colorids_color_15_brightwhite as c_int;
                cancel_text_color = colorids_color_7_lightgray as c_int;
            } else {
                highlight_rect = addr_of!(cancel_highlight_rect);
                ok_text_color = colorids_color_7_lightgray as c_int;
                cancel_text_color = colorids_color_15_brightwhite as c_int;
            }
            draw_rect(highlight_rect, colorids_color_8_darkgray as c_int);
            show_text_with_color(addr_of!(ok_text_rect), halign_center, valign_middle, cs!("OK"), ok_text_color);
            show_text_with_color(addr_of!(cancel_text_rect), halign_center, valign_middle, cs!("Cancel"), cancel_text_color);
            update_screen();
        }

        SDL_Delay(1);
    }
    current_dialog_box = 0;
    clear_menu_controls();
}

unsafe fn draw_select_level_dialog() {
    clear_menu_controls();
    let mut old_edited_level_number: c_int = -1;
    loop {
        process_events();
        key_test_paused_menu(key_test_quit());
        process_additional_menu_input();

        if menu_control_back == 1 {
            menu_control_back = 0;
            play_menu_sound(soundids_sound_22_loose_shake_3 as c_int);
            break;
        }

        if menu_control_x < 0 {
            menu_current_level = std::cmp::max(0, menu_current_level as c_int - 1) as word;
        } else if menu_control_x > 0 {
            menu_current_level = std::cmp::min(15, menu_current_level as c_int + 1) as word;
        } else if mouse_clicked || pressed_enter {
            enter_settings_subsection(SETTINGS_MENU_LEVEL_CUSTOMIZATION);
            highlighted_settings_subsection = SETTINGS_MENU_MODS;
            play_menu_sound(soundids_sound_22_loose_shake_3 as c_int);
            break;
        }

        if menu_current_level as c_int != old_edited_level_number {
            let saved_font = textstate.ptr_font;
            textstate.ptr_font = addr_of_mut!(hc_font);

            old_edited_level_number = menu_current_level as c_int;
            let clear_color = SDL_MapRGBA((*current_target_surface).format, 0, 0, 0, 255);
            SDL_FillRect(overlay_surface, null(), clear_color);
            draw_rect(addr_of!((*copyprot_dialog).peel_rect), colorids_color_0_black as c_int);
            dialog_method_2_frame(copyprot_dialog);
            let mut rect: rect_type = core::mem::zeroed();
            shrink2_rect(&mut rect, addr_of!((*copyprot_dialog).text_rect), 2, 1);
            rect.bottom -= 14;
            show_text_with_color(&rect, halign_center, valign_middle, cs!("Customize level..."), colorids_color_15_brightwhite as c_int);
            clear_kbd_buf();
            let input_rect = rect_type { top: 104, left: 64, bottom: 118, right: 256 };
            let mut level_text = [0 as c_char; 8];
            snprintf(level_text.as_mut_ptr(), 8, cs!("%d"), menu_current_level as c_int);
            show_text_with_color(&input_rect, halign_center, valign_middle, level_text.as_ptr(), colorids_color_15_brightwhite as c_int);
            draw_image_with_blending(arrowhead_right_image, 175, input_rect.top as c_int + 3);
            draw_image_with_blending(arrowhead_left_image, 145 - 3, input_rect.top as c_int + 3);

            update_screen();
            textstate.ptr_font = saved_font;
        }

        SDL_Delay(1);
    }
    clear_menu_controls();
}

#[no_mangle]
pub unsafe extern "C" fn draw_menu() {
    escape_key_suppressed = (key_states[SDL_SCANCODE_BACKSPACE as usize] & KEYSTATE_HELD_I as u8) != 0
        || (key_states[SDL_SCANCODE_ESCAPE as usize] & KEYSTATE_HELD_I as u8) != 0;
    let saved_target_surface = current_target_surface;
    current_target_surface = overlay_surface;

    need_close_menu = false;
    while !need_close_menu {
        clear_menu_controls();
        process_events();
        if process_key() != 0 {
            break;
        }
        process_additional_menu_input();

        if current_dialog_box != DIALOG_NONE {
            if current_dialog_box == DIALOG_SELECT_LEVEL {
                draw_select_level_dialog();
            } else {
                draw_confirmation_dialog(current_dialog_box, current_dialog_text);
            }
            current_dialog_box = DIALOG_NONE;
            clear_menu_controls();
        }

        if is_menu_shown == 1 {
            is_menu_shown = -1;
            need_full_menu_redraw_count = 2;
            reset_paused_menu();
        }
        if menu_control_back == 1 {
            play_menu_sound(soundids_sound_22_loose_shake_3 as c_int);
            if drawn_menu == 1 {
                if controlled_area == 1 {
                    leave_settings_subsection();
                } else {
                    reset_paused_menu();
                    hovering_pause_menu_item = PAUSE_MENU_SETTINGS;
                }
            } else {
                break;
            }
        }

        if menu_control_scroll_y != 0 {
            menu_scroll(menu_control_scroll_y);
        }

        if have_mouse_input || have_keyboard_or_controller_input {
            need_full_menu_redraw_count = 2;
        } else if need_full_menu_redraw_count == 0 {
            SDL_Delay(1);
            continue;
        }

        let saved_font = textstate.ptr_font;
        textstate.ptr_font = addr_of_mut!(hc_small_font);
        if drawn_menu == 0 {
            draw_pause_menu();
        } else if drawn_menu == 1 {
            draw_settings_menu();
        }
        textstate.ptr_font = saved_font;
        if !need_close_menu {
            update_screen();
        }

        need_full_menu_redraw_count -= 1;
    }

    current_target_surface = saved_target_surface;
}

#[no_mangle]
pub unsafe extern "C" fn clear_menu_controls() {
    pressed_enter = false;
    mouse_moved = false;
    mouse_clicked = false;
    mouse_button_clicked_right = false;
    have_mouse_input = false;
    have_keyboard_or_controller_input = false;
    menu_control_x = 0;
    menu_control_y = 0;
    menu_control_back = 0;
    menu_control_scroll_y = 0;
}

#[no_mangle]
pub unsafe extern "C" fn process_additional_menu_input() {
    read_mouse_state();
    have_keyboard_or_controller_input =
        menu_control_x != 0 || menu_control_y != 0 || menu_control_back != 0 || pressed_enter;
    have_mouse_input =
        mouse_moved || mouse_clicked || mouse_button_clicked_right || menu_control_scroll_y != 0;

    let flags = SDL_GetWindowFlags(window_);
    if flags & SDL_WINDOW_FULLSCREEN_DESKTOP != 0 {
        if have_mouse_input {
            SDL_ShowCursor(SDL_ENABLE);
        } else if have_keyboard_or_controller_input {
            SDL_ShowCursor(SDL_DISABLE);
        }
    } else {
        SDL_ShowCursor(SDL_ENABLE);
    }
}

#[no_mangle]
pub unsafe extern "C" fn key_test_paused_menu(mut key: c_int) -> c_int {
    menu_control_x = 0;
    menu_control_y = 0;
    menu_control_back = 0;

    if mouse_button_clicked_right {
        menu_control_back = 1;
    }

    if is_joyst_mode != 0 {
        let mut joy_x = 0;
        let mut joy_y = 0;
        if joy_button_states[JOYINPUT_DPAD_LEFT as usize] & KEYSTATE_HELD_I != 0 {
            joy_x = -1;
        } else if joy_button_states[JOYINPUT_DPAD_RIGHT as usize] & KEYSTATE_HELD_I != 0 {
            joy_x = 1;
        }
        if joy_button_states[JOYINPUT_DPAD_UP as usize] & KEYSTATE_HELD_I != 0 {
            joy_y = -1;
        } else if joy_button_states[JOYINPUT_DPAD_DOWN as usize] & KEYSTATE_HELD_I != 0 {
            joy_y = 1;
        }
        let y_threshold = 14000;
        let x_threshold = 26000;
        if joy_axis[SDL_CONTROLLER_AXIS_LEFTY] < -y_threshold {
            joy_y = -1;
        } else if joy_axis[SDL_CONTROLLER_AXIS_LEFTY] > y_threshold {
            joy_y = 1;
        } else if joy_axis[SDL_CONTROLLER_AXIS_LEFTX] < -x_threshold {
            joy_x = -1;
        } else if joy_axis[SDL_CONTROLLER_AXIS_LEFTX] > x_threshold {
            joy_x = 1;
        }

        let mut needed_timeout_s = 0.1f32;
        if joy_x == 0 && joy_y == 0 {
            joy_xy_released = true;
            joy_xy_timeout_counter = 0;
        } else {
            if joy_xy_released {
                needed_timeout_s = 0.3f32;
                joy_xy_released = false;
            }
            let current_counter = SDL_GetPerformanceCounter();
            if current_counter > joy_xy_timeout_counter {
                menu_control_x = joy_x;
                menu_control_y = joy_y;
                joy_xy_timeout_counter = current_counter + (SDL_GetPerformanceFrequency() as f32 * needed_timeout_s) as u64;
                return 0;
            }
        }

        if joy_button_states[JOYINPUT_A as usize] & KEYSTATE_HELD_I == 0
            && joy_button_states[JOYINPUT_Y as usize] & KEYSTATE_HELD_I == 0
            && joy_button_states[JOYINPUT_B as usize] & KEYSTATE_HELD_I == 0
        {
            joy_ABXY_buttons_released = true;
        } else if joy_ABXY_buttons_released {
            joy_ABXY_buttons_released = false;
            if joy_button_states[JOYINPUT_A as usize] & KEYSTATE_HELD_I != 0 {
                key = SDL_SCANCODE_RETURN;
                joy_button_states[JOYINPUT_A as usize] = 0;
            } else if joy_button_states[JOYINPUT_B as usize] & KEYSTATE_HELD_I != 0 {
                key = SDL_SCANCODE_ESCAPE;
            }
        }
    }

    // remap
    if key == key_up {
        key = SDL_SCANCODE_UP;
    } else if key == key_down {
        key = SDL_SCANCODE_DOWN;
    } else if key == key_left {
        key = SDL_SCANCODE_LEFT;
    } else if key == key_right {
        key = SDL_SCANCODE_RIGHT;
    } else if key == key_enter {
        key = SDL_SCANCODE_RETURN;
    } else if key == key_esc {
        key = SDL_SCANCODE_ESCAPE;
    }

    if key == SDL_SCANCODE_UP {
        menu_control_y = -1;
    } else if key == SDL_SCANCODE_DOWN {
        menu_control_y = 1;
    } else if key == SDL_SCANCODE_PAGEUP {
        menu_control_y = -9;
    } else if key == SDL_SCANCODE_PAGEDOWN {
        menu_control_y = 9;
    } else if key == SDL_SCANCODE_HOME {
        menu_control_y = -1000;
    } else if key == SDL_SCANCODE_END {
        menu_control_y = 1000;
    } else if key == SDL_SCANCODE_RIGHT {
        menu_control_x = 1;
    } else if key == SDL_SCANCODE_LEFT {
        menu_control_x = -1;
    } else if key == SDL_SCANCODE_RETURN || key == SDL_SCANCODE_SPACE {
        pressed_enter = true;
    } else if key == SDL_SCANCODE_ESCAPE || key == SDL_SCANCODE_BACKSPACE {
        menu_control_back = 1;
    } else if key == SDL_SCANCODE_F6 || key == (SDL_SCANCODE_F6 | WITH_SHIFT) {
        if Kid.alive < 0 {
            need_quick_save = 1;
        }
        need_close_menu = true;
    } else if key == SDL_SCANCODE_F9 || key == (SDL_SCANCODE_F9 | WITH_SHIFT) {
        need_quick_load = 1;
        need_close_menu = true;
    } else if key & WITH_CTRL != 0 {
        need_close_menu = true;
        return key;
    }
    0
}

// ============================================================================
// process_ingame_settings and save/load (ported from menu.c lines 2308-2475)
// ============================================================================

type rw_process_func_type = unsafe extern "C" fn(*mut SDL_RWops, *mut c_void, usize) -> c_int;

extern "C" {
    fn process_rw_write(rw: *mut SDL_RWops, data: *mut c_void, data_size: usize) -> c_int;
    fn process_rw_read(rw: *mut SDL_RWops, data: *mut c_void, data_size: usize) -> c_int;
}

macro_rules! process {
    ($rw:expr, $func:expr, $x:expr) => {
        if $func($rw, addr_of_mut!($x) as *mut c_void, std::mem::size_of_val(&$x)) == 0 { return; }
    }
}

unsafe fn process_ingame_settings_user_managed(rw: *mut SDL_RWops, process_func: rw_process_func_type) {
    process!(rw, process_func, enable_pause_menu);
    process!(rw, process_func, enable_info_screen);
    process!(rw, process_func, is_sound_on);
    process!(rw, process_func, enable_music);
    process!(rw, process_func, enable_controller_rumble);
    process!(rw, process_func, joystick_threshold);
    process!(rw, process_func, joystick_only_horizontal);
    process!(rw, process_func, enable_replay);
    process!(rw, process_func, start_fullscreen);
    process!(rw, process_func, use_hardware_acceleration);
    process!(rw, process_func, use_correct_aspect_ratio);
    process!(rw, process_func, use_integer_scaling);
    process!(rw, process_func, scaling_type);
    process!(rw, process_func, enable_fade);
    process!(rw, process_func, enable_flash);
    process!(rw, process_func, enable_lighting);
    process!(rw, process_func, key_left);
    process!(rw, process_func, key_right);
    process!(rw, process_func, key_up);
    process!(rw, process_func, key_down);
    process!(rw, process_func, key_jump_left);
    process!(rw, process_func, key_jump_right);
    process!(rw, process_func, key_action);
    process!(rw, process_func, key_enter);
    process!(rw, process_func, key_esc);
}

unsafe fn process_ingame_settings_mod_managed(rw: *mut SDL_RWops, process_func: rw_process_func_type) {
    process!(rw, process_func, enable_copyprot);
    process!(rw, process_func, enable_quicksave);
    process!(rw, process_func, enable_quicksave_penalty);
    process!(rw, process_func, use_fixes_and_enhancements);
    process!(rw, process_func, fixes_saved);
    process!(rw, process_func, use_custom_options);
    process!(rw, process_func, custom_saved);
}

unsafe fn crc32c(message: *mut u8, mut size: usize) -> u32 {
    static mut table: [u32; 256] = [0u32; 256];
    if table[1] == 0 {
        for byte in 0u32..=255 {
            let mut crc = byte;
            for _ in (0i32..=7).rev() {
                let mask = (0u32).wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xEDB88320u32 & mask);
            }
            table[byte as usize] = crc;
        }
    }
    let mut i: usize = 0;
    let mut crc: u32 = 0xFFFFFFFF;
    while size > 0 {
        size -= 1;
        let byte = *message.add(i) as u32;
        crc = (crc >> 8) ^ table[((crc ^ byte) & 0xFF) as usize];
        i += 1;
    }
    !crc
}

unsafe fn calculate_exe_crc() {
    if exe_crc == 0 {
        let exe_file = fopen(*g_argv.add(0), cs!("rb"));
        if !exe_file.is_null() {
            fseek(exe_file, 0, SEEK_END);
            let size = ftell(exe_file) as usize;
            fseek(exe_file, 0, SEEK_SET);
            if size > 0 {
                let buffer = malloc(size) as *mut u8;
                let bytes = fread(buffer as *mut c_void, 1, size, exe_file);
                let actual_size = if bytes != size {
                    fprintf(stderr, cs!("exec changed size during CRC32!?\n"));
                    bytes
                } else {
                    size
                };
                exe_crc = crc32c(buffer, actual_size);
                free(buffer as *mut c_void);
            }
            fclose(exe_file);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn save_ingame_settings() {
    let mut __lf = [0u8; POP_MAX_PATH];
    let rw = SDL_RWFromFile(locate_save_file_(cs!("SDLPoP.cfg"), __lf.as_mut_ptr() as *mut c_char, POP_MAX_PATH as c_int), cs!("wb"));
    if !rw.is_null() {
        calculate_exe_crc();
        SDL_RWwrite(rw, addr_of!(exe_crc) as *const c_void, std::mem::size_of::<u32>(), 1);
        let levelset_name_length = strnlen(levelset_name.as_ptr(), 255) as u8;
        SDL_RWwrite(rw, addr_of!(levelset_name_length) as *const c_void, 1, 1);
        SDL_RWwrite(rw, levelset_name.as_ptr() as *const c_void, levelset_name_length as usize, 1);
        process_ingame_settings_user_managed(rw, process_rw_write);
        process_ingame_settings_mod_managed(rw, process_rw_write);
        SDL_RWclose(rw);
    }
}

#[no_mangle]
pub unsafe extern "C" fn load_ingame_settings() {
    let mut __cfg_lf = [0u8; POP_MAX_PATH];
    let mut __ini_lf = [0u8; POP_MAX_PATH];
    let cfg_filename = locate_file_(cs!("SDLPoP.cfg"), __cfg_lf.as_mut_ptr() as *mut c_char, POP_MAX_PATH as c_int);
    let ini_filename = locate_file_(cs!("SDLPoP.ini"), __ini_lf.as_mut_ptr() as *mut c_char, POP_MAX_PATH as c_int);
    let mut st_ini: stat_t = std::mem::zeroed();
    let mut st_cfg: stat_t = std::mem::zeroed();
    if stat(cfg_filename, &mut st_cfg) == 0 && stat(ini_filename, &mut st_ini) == 0 {
        if st_ini.st_mtim[0] > st_cfg.st_mtim[0]
            || (st_ini.st_mtim[0] == st_cfg.st_mtim[0] && st_ini.st_mtim[1] > st_cfg.st_mtim[1])
        {
            return;
        }
    }
    let rw = SDL_RWFromFile(cfg_filename, cs!("rb"));
    if !rw.is_null() {
        calculate_exe_crc();
        let mut expected_crc: u32 = 0;
        SDL_RWread(rw, &mut expected_crc as *mut u32 as *mut c_void, std::mem::size_of::<u32>(), 1);
        if exe_crc == expected_crc {
            let mut cfg_levelset_name_length: u8 = 0;
            let mut cfg_levelset_name = [0u8; 256];
            SDL_RWread(rw, &mut cfg_levelset_name_length as *mut u8 as *mut c_void, 1, 1);
            SDL_RWread(rw, cfg_levelset_name.as_mut_ptr() as *mut c_void, cfg_levelset_name_length as usize, 1);
            process_ingame_settings_user_managed(rw, process_rw_read);
            if strncmp(levelset_name.as_ptr(), cfg_levelset_name.as_ptr() as *const c_char, 256) == 0 {
                process_ingame_settings_mod_managed(rw, process_rw_read);
            }
        }
        SDL_RWclose(rw);
    }
}

#[no_mangle]
pub unsafe extern "C" fn menu_was_closed() {
    is_paused = 0;
    is_menu_shown = 0;
    escape_key_suppressed =
        ((key_states[SDL_SCANCODE_BACKSPACE as usize] as u32) & KEYSTATE_HELD) != 0
        || ((key_states[SDL_SCANCODE_ESCAPE as usize] as u32) & KEYSTATE_HELD) != 0;
    if were_settings_changed {
        save_ingame_settings();
        were_settings_changed = false;
    }
    let flags = SDL_GetWindowFlags(window_);
    if flags & SDL_WINDOW_FULLSCREEN_DESKTOP != 0 {
        SDL_ShowCursor(SDL_DISABLE);
    } else {
        SDL_ShowCursor(SDL_ENABLE);
    }
}

// MENU_RS_END
