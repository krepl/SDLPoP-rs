//! Platform layer — everything the game needs from the machine it runs on.
//!
//! Ported from `src/seg009.c`. Unlike the other `seg*` modules this one is *not*
//! a translation of original DOS code: the disassembly offsets in the comments
//! name the routine each function replaces, but the bodies are SDLPoP's own
//! reimplementations on top of SDL2. That is why the shapes here are C-shaped
//! rather than 8086-shaped, and why so few functions have a `word`-arithmetic
//! quirk to preserve.
//!
//! Everything below the game logic lives here, in roughly this order:
//!
//! * **Path resolution** — the game looks for a file in three places, in a
//!   fixed order: `$HOME/.SDLPoP`, `/usr/share/SDLPoP`, and the directory the
//!   executable was launched from. [`locate_file_`] walks that list for reads;
//!   [`locate_save_file_`] walks it for writes, taking the first entry that is
//!   a directory the user can write to.
//!
//! * **DAT archives** — a `.DAT` is the original DOS archive format: a 6-byte
//!   header, a table of `(id, offset, size)` records, and the payloads. Every
//!   open DAT is threaded onto the `dat_chain_ptr` singly-linked list, most
//!   recently opened first, and a resource lookup walks that list until it
//!   hits. A DAT entry can also be shadowed by a loose file on disk
//!   (`data/<dat>/res<id>.<ext>`), which is how mods override individual
//!   resources without shipping a whole DAT; `load_from_opendats_metadata` is
//!   the single place that decides which of the two a resource came from, and
//!   reports it back as a [`data_location`].
//!
//! * **Image decoding** — sprites in a DAT are stored 1..8 bits per pixel and
//!   optionally compressed by one of four schemes (RLE or LZ-with-a-1KB-window,
//!   each in left-to-right or up-to-down pixel order). `decode_image` runs the
//!   pipeline: decompress into a packed buffer, expand to one byte per pixel,
//!   hand the result to SDL as an 8-bit paletted surface with colour 0 made
//!   transparent.
//!
//! * **Text** — a font is a `chtab` (an array of one small surface per glyph)
//!   plus baseline metrics. `draw_text` breaks a string into lines that fit a
//!   rect, then aligns and blits them glyph by glyph. Two fonts are always
//!   available even with no data files present, because both are also embedded
//!   in the binary (`hc_font_data` here, `hc_small_font_data` in `menu.c`).
//!
//! * **Audio** — one SDL audio device, one callback, and four mutually
//!   overlapping sources: PC-speaker square waves synthesised from note lists,
//!   digitised samples resampled to the device rate, MIDI through the OPL3
//!   emulator in `midi.rs`, and Ogg Vorbis music. `audio_callback` mixes at
//!   most one of {digi, speaker} with at most one of {midi, ogg}, and posts an
//!   `SDL_USEREVENT` when a sound finishes so the game loop can notice.
//!
//! * **Screen** — the game draws into a 320x200 24-bit `onscreen_surface_`,
//!   never to the window. `update_screen` uploads that surface to a texture and
//!   lets SDL scale it, optionally through a 2x intermediate to imitate DOSBox's
//!   "fuzzy pixels". Overlays (the level timer, the pause menu) are composited
//!   into a separate `merged_surface` so they never touch the game's own
//!   framebuffer.
//!
//! * **Timers** — three independent tick counters driven by SDL's performance
//!   counter rather than by wall-clock milliseconds, so the game's notion of a
//!   tick stays exact under fast-forward and under the feather-fall rescaling
//!   in [`set_timer_length`].
//!
//! * **Events** — [`process_events`] is the single point where the outside
//!   world reaches the game: keyboard, game controller, legacy joystick, mouse,
//!   window and quit events all land in globals (`last_key_scancode`,
//!   `key_states`, `joy_axis`, ...) that the gameplay code polls.
//!
//! ## Notes for readers coming from the other `seg*` modules
//!
//! This module was deliberately left out of the `&mut State` migration. It is
//! platform-init-heavy, most of its state is genuinely process-global (SDL
//! handles, the open-DAT chain, the audio mixer's cursor), and threading a
//! `State` through it would buy no testability.
//!
//! It *is* fully migrated to the [`crate::platform`] traits: no `SDL_*` symbol
//! is called directly. Everything goes through
//! `crate::platform::sdl::shared_renderer()` / `shared_audio()` /
//! `shared_input()`, which is what lets the headless backend exist. The handful
//! of `SDL_`-named `#[inline]` functions below are C *macros* being
//! reimplemented, not FFI declarations.

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]
#![allow(unused_assignments)]

use std::os::raw::{c_char, c_int, c_long, c_short, c_void};
use core::ptr::null_mut;
use super::*;

// ============================================================================
// libc (the shared set — fopen/fread/fwrite/fclose/fseek/remove/perror/getenv —
// comes from lib.rs via `use super::*`). Declare the rest locally.
// ============================================================================
extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn calloc(nmemb: usize, size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn memcpy(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void;
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    fn strlen(s: *const c_char) -> usize;
    fn strnlen(s: *const c_char, maxlen: usize) -> usize;
    fn strcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char;
    fn strncpy(dst: *mut c_char, src: *const c_char, n: usize) -> *mut c_char;
    fn strdup(s: *const c_char) -> *mut c_char;
    fn strchr(s: *const c_char, c: c_int) -> *mut c_char;
    fn strrchr(s: *const c_char, c: c_int) -> *mut c_char;
    fn strcmp(a: *const c_char, b: *const c_char) -> c_int;
    fn strcasecmp(a: *const c_char, b: *const c_char) -> c_int;
    fn strncasecmp(a: *const c_char, b: *const c_char, n: usize) -> c_int;
    fn strerror(errnum: c_int) -> *mut c_char;
    fn feof(stream: *mut FILE) -> c_int;
    fn fgetc(stream: *mut FILE) -> c_int;
    fn fileno(stream: *mut FILE) -> c_int;
    fn time(t: *mut c_long) -> c_long;
    fn exit(code: c_int) -> !;
    fn access(path: *const c_char, mode: c_int) -> c_int;
    fn stat(path: *const c_char, buf: *mut stat_t) -> c_int;
    fn fstat(fd: c_int, buf: *mut stat_t) -> c_int;
    fn __errno_location() -> *mut c_int;
    // POSIX directory listing
    fn opendir(name: *const c_char) -> *mut c_void;
    fn readdir(dirp: *mut c_void) -> *mut dirent;
    fn closedir(dirp: *mut c_void) -> c_int;
}

#[inline]
unsafe fn errno() -> c_int { *__errno_location() }

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

#[repr(C)]
struct dirent {
    d_ino: u64,
    d_off: i64,
    d_reclen: u16,
    d_type: u8,
    d_name: [c_char; 256],
}

const F_OK: c_int = 0;
const W_OK: c_int = 2;
const SEEK_SET: c_int = 0;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;
#[inline]
fn S_ISDIR(m: u32) -> bool { (m & S_IFMT) == S_IFDIR }
#[inline]
fn S_ISREG(m: u32) -> bool { (m & S_IFMT) == S_IFREG }

// ============================================================================
// SDL types not in bindings.rs
// ============================================================================
#[repr(C)]
struct SDL_version {
    major: u8,
    minor: u8,
    patch: u8,
}

#[repr(C)]
pub struct SDL_AudioSpec {
    freq: c_int,
    format: u16, // SDL_AudioFormat
    channels: u8,
    silence: u8,
    samples: u16,
    padding: u16,
    size: u32,
    callback: Option<unsafe extern "C" fn(*mut c_void, *mut u8, c_int)>,
    userdata: *mut c_void,
}

#[repr(C)]
struct SDL_RendererInfo {
    name: *const c_char,
    flags: u32,
    num_texture_formats: u32,
    texture_formats: [u32; 16],
    max_texture_width: c_int,
    max_texture_height: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_Keysym {
    scancode: u32,
    sym: i32,
    r#mod: u16,
    unused: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_KeyboardEvent {
    type_: u32,
    timestamp: u32,
    windowID: u32,
    state: u8,
    repeat: u8,
    padding2: u8,
    padding3: u8,
    keysym: SDL_Keysym,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_TextInputEvent {
    type_: u32,
    timestamp: u32,
    windowID: u32,
    text: [c_char; 32],
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_WindowEvent {
    type_: u32,
    timestamp: u32,
    windowID: u32,
    event: u8,
    padding1: u8,
    padding2: u8,
    padding3: u8,
    data1: i32,
    data2: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_MouseButtonEvent {
    type_: u32,
    timestamp: u32,
    windowID: u32,
    which: u32,
    button: u8,
    state: u8,
    clicks: u8,
    padding1: u8,
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_MouseWheelEvent {
    type_: u32,
    timestamp: u32,
    windowID: u32,
    which: u32,
    x: i32,
    y: i32,
    direction: u32,
    preciseX: f32,
    preciseY: f32,
    mouseX: i32,
    mouseY: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_ControllerAxisEvent {
    type_: u32,
    timestamp: u32,
    which: i32,
    axis: u8,
    padding1: u8,
    padding2: u8,
    padding3: u8,
    value: i16,
    padding4: u16,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_ControllerButtonEvent {
    type_: u32,
    timestamp: u32,
    which: i32,
    button: u8,
    state: u8,
    padding1: u8,
    padding2: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_ControllerDeviceEvent {
    type_: u32,
    timestamp: u32,
    which: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_JoyAxisEvent {
    type_: u32,
    timestamp: u32,
    which: i32,
    axis: u8,
    padding1: u8,
    padding2: u8,
    padding3: u8,
    value: i16,
    padding4: u16,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_JoyButtonEvent {
    type_: u32,
    timestamp: u32,
    which: i32,
    button: u8,
    state: u8,
    padding1: u8,
    padding2: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct SDL_UserEvent {
    type_: u32,
    timestamp: u32,
    windowID: u32,
    code: i32,
    data1: *mut c_void,
    data2: *mut c_void,
}
#[repr(C)]
union SDL_Event {
    type_: u32,
    key: SDL_KeyboardEvent,
    text: SDL_TextInputEvent,
    window: SDL_WindowEvent,
    button: SDL_MouseButtonEvent,
    wheel: SDL_MouseWheelEvent,
    caxis: SDL_ControllerAxisEvent,
    cbutton: SDL_ControllerButtonEvent,
    cdevice: SDL_ControllerDeviceEvent,
    jaxis: SDL_JoyAxisEvent,
    jbutton: SDL_JoyButtonEvent,
    user: SDL_UserEvent,
    padding: [u8; 56],
}

// ============================================================================
// SDL functions -- all behind the Renderer/AudioBackend/InputSource traits now
// (rust/src/platform/), reached via crate::platform::sdl::shared_renderer()/
// shared_audio()/shared_input(). See those modules for the raw sdl2::sys calls.
// ============================================================================
use crate::platform::{AudioBackend, InputSource, Renderer};

// Defined in menu.c (still compiled as C); not in proto.h.
extern "C" {
    static mut hc_small_font_data: [u8; 0];
}

extern "C" {
    fn ftell(stream: *mut FILE) -> c_long;
}

// SDL_BlitSurface and SDL_BlitScaled are macros for SDL_UpperBlit / SDL_UpperBlitScaled.
#[inline]
unsafe fn SDL_BlitSurface(src: *mut SDL_Surface, srcrect: *const SDL_Rect,
                          dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int {
    crate::platform::sdl::shared_renderer().blit(src, srcrect, dst, dstrect)
}
#[inline]
unsafe fn SDL_BlitScaled(src: *mut SDL_Surface, srcrect: *const SDL_Rect,
                         dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int {
    crate::platform::sdl::shared_renderer().blit_scaled(src, srcrect, dst, dstrect)
}

// IMG_GetError is a macro for SDL_GetError.
#[inline]
unsafe fn IMG_GetError() -> *const c_char {
    crate::platform::sdl::shared_renderer().get_error()
}
// SDL_GameControllerAddMappingsFromFile is a macro.
#[inline]
unsafe fn SDL_GameControllerAddMappingsFromFile(file: *const c_char) -> c_int {
    crate::platform::sdl::shared_renderer().game_controller_add_mappings_from_file(std::ffi::CStr::from_ptr(file))
}

// SDL_ISPIXELFORMAT_INDEXED macro
#[inline]
fn SDL_PIXELFLAG(x: u32) -> u32 { (x >> 28) & 0x0F }
#[inline]
fn SDL_PIXELTYPE(x: u32) -> u32 { (x >> 24) & 0x0F }
#[inline]
fn SDL_ISPIXELFORMAT_FOURCC(format: u32) -> bool { format != 0 && SDL_PIXELFLAG(format) != 1 }
const SDL_PIXELTYPE_INDEX1: u32 = 1;
const SDL_PIXELTYPE_INDEX4: u32 = 2;
const SDL_PIXELTYPE_INDEX8: u32 = 3;
#[inline]
fn SDL_ISPIXELFORMAT_INDEXED(format: u32) -> bool {
    !SDL_ISPIXELFORMAT_FOURCC(format)
        && (SDL_PIXELTYPE(format) == SDL_PIXELTYPE_INDEX1
            || SDL_PIXELTYPE(format) == SDL_PIXELTYPE_INDEX4
            || SDL_PIXELTYPE(format) == SDL_PIXELTYPE_INDEX8)
}

// ============================================================================
// SDL / libc constants
// ============================================================================
const SDL_TRUE: c_int = 1;
const SDL_FALSE: c_int = 0;
const SDL_ALPHA_OPAQUE: u8 = 255;
const SDL_ALPHA_TRANSPARENT: u8 = 0;
const SDL_ENABLE: c_int = 1;
const SDL_DISABLE: c_int = 0;
const SDL_BLENDMODE_NONE: c_int = 0;
const SDL_BLENDMODE_BLEND: c_int = 1;

const SDL_INIT_TIMER: u32 = 0x00000001;
const SDL_INIT_VIDEO: u32 = 0x00000020;
const SDL_INIT_HAPTIC: u32 = 0x00001000;
const SDL_INIT_GAMECONTROLLER: u32 = 0x00002000;
const SDL_INIT_NOPARACHUTE: u32 = 0x00100000;

const SDL_WINDOW_FULLSCREEN_DESKTOP: u32 = 4097;
const SDL_WINDOW_RESIZABLE: u32 = 32;
const SDL_WINDOW_ALLOW_HIGHDPI: u32 = 8192;
const SDL_WINDOWPOS_UNDEFINED: c_int = 0x1FFF0000;

const SDL_RENDERER_SOFTWARE: u32 = 1;
const SDL_RENDERER_ACCELERATED: u32 = 2;
const SDL_RENDERER_TARGETTEXTURE: u32 = 8;

const SDL_TEXTUREACCESS_STREAMING: c_int = 1;
const SDL_TEXTUREACCESS_TARGET: c_int = 2;

const SDL_PIXELFORMAT_RGB24: u32 = 386930691;
const SDL_PIXELFORMAT_ARGB8888: u32 = 372645892;

const AUDIO_U8: u16 = 0x0008;
const AUDIO_S16SYS: u16 = 0x8010;

const KMOD_SHIFT: c_int = 3;
const KMOD_CTRL: c_int = 192;
const KMOD_ALT: c_int = 768;

const SDL_BUTTON_LEFT: u8 = 1;
const SDL_BUTTON_RIGHT: u8 = 3;
const SDL_BUTTON_X1: u8 = 4;

// SDL event types
const SDL_QUIT: u32 = 0x100;
const SDL_WINDOWEVENT: u32 = 0x200;
const SDL_KEYDOWN: u32 = 0x300;
const SDL_KEYUP: u32 = 0x301;
const SDL_TEXTINPUT: u32 = 0x303;
const SDL_MOUSEBUTTONDOWN: u32 = 0x401;
const SDL_MOUSEWHEEL: u32 = 0x403;
const SDL_JOYAXISMOTION: u32 = 0x600;
const SDL_JOYBUTTONDOWN: u32 = 0x603;
const SDL_JOYBUTTONUP: u32 = 0x604;
const SDL_CONTROLLERAXISMOTION: u32 = 0x650;
const SDL_CONTROLLERBUTTONDOWN: u32 = 0x651;
const SDL_CONTROLLERBUTTONUP: u32 = 0x652;
const SDL_CONTROLLERDEVICEADDED: u32 = 0x653;
const SDL_CONTROLLERDEVICEREMOVED: u32 = 0x654;
const SDL_USEREVENT: u32 = 0x7F01;

// SDL window event ids
const SDL_WINDOWEVENT_EXPOSED: u8 = 3;
const SDL_WINDOWEVENT_SIZE_CHANGED: u8 = 6;
const SDL_WINDOWEVENT_FOCUS_GAINED: u8 = 12;

// SDL controller buttons / axes
const SDL_CONTROLLER_BUTTON_A: u8 = 0;
const SDL_CONTROLLER_BUTTON_B: u8 = 1;
const SDL_CONTROLLER_BUTTON_X: u8 = 2;
const SDL_CONTROLLER_BUTTON_Y: u8 = 3;
const SDL_CONTROLLER_BUTTON_BACK: u8 = 4;
const SDL_CONTROLLER_BUTTON_START: u8 = 6;
const SDL_CONTROLLER_BUTTON_DPAD_UP: u8 = 11;
const SDL_CONTROLLER_BUTTON_DPAD_DOWN: u8 = 12;
const SDL_CONTROLLER_BUTTON_DPAD_LEFT: u8 = 13;
const SDL_CONTROLLER_BUTTON_DPAD_RIGHT: u8 = 14;
const SDL_CONTROLLER_AXIS_LEFTX: c_int = 0;
const SDL_CONTROLLER_AXIS_LEFTY: c_int = 1;

// SDL scancodes (not emitted by bindgen)
const SDL_SCANCODE_Q: c_int = 20;
const SDL_SCANCODE_RETURN: c_int = 40;
const SDL_SCANCODE_ESCAPE: c_int = 41;
const SDL_SCANCODE_BACKSPACE: c_int = 42;
const SDL_SCANCODE_DELETE: c_int = 76;
const SDL_SCANCODE_GRAVE: c_int = 53;
const SDL_SCANCODE_F12: c_int = 69;
const SDL_SCANCODE_LCTRL: c_int = 224;
const SDL_SCANCODE_LSHIFT: c_int = 225;
const SDL_SCANCODE_LALT: c_int = 226;
const SDL_SCANCODE_LGUI: c_int = 227;
const SDL_SCANCODE_RCTRL: c_int = 228;
const SDL_SCANCODE_RSHIFT: c_int = 229;
const SDL_SCANCODE_RALT: c_int = 230;
const SDL_SCANCODE_RGUI: c_int = 231;
const SDL_SCANCODE_CAPSLOCK: c_int = 57;
const SDL_SCANCODE_SCROLLLOCK: c_int = 71;
const SDL_SCANCODE_NUMLOCKCLEAR: c_int = 83;
const SDL_SCANCODE_APPLICATION: c_int = 101;
const SDL_SCANCODE_PRINTSCREEN: c_int = 70;
const SDL_SCANCODE_VOLUMEUP: c_int = 128;
const SDL_SCANCODE_VOLUMEDOWN: c_int = 129;
const SDL_SCANCODE_MUTE: c_int = 127;
const SDL_SCANCODE_AUDIOMUTE: c_int = 262;
const SDL_SCANCODE_PAUSE: c_int = 72;
const SDL_SCANCODE_TAB: c_int = 43;
const SDL_SCANCODE_LEFT: c_int = 80;
const SDL_SCANCODE_RIGHT: c_int = 79;
const SDL_SCANCODE_UP: c_int = 82;
const SDL_SCANCODE_DOWN: c_int = 81;
const SDL_SCANCODE_CLEAR: c_int = 156;
const SDL_SCANCODE_HOME: c_int = 74;
const SDL_SCANCODE_PAGEUP: c_int = 75;
const SDL_SCANCODE_KP_2: c_int = 90;
const SDL_SCANCODE_KP_4: c_int = 92;
const SDL_SCANCODE_KP_5: c_int = 93;
const SDL_SCANCODE_KP_6: c_int = 94;
const SDL_SCANCODE_KP_7: c_int = 95;
const SDL_SCANCODE_KP_8: c_int = 96;
const SDL_SCANCODE_KP_9: c_int = 97;
const SDL_SCANCODE_KP_MINUS: c_int = 86;
const SDL_SCANCODE_KP_PLUS: c_int = 87;
const SDL_SCANCODE_SPACE: c_int = 44;

// SDL joystick mapping (PoP config.h #defines)
const SDL_JOYSTICK_BUTTON_Y: u8 = 2;
const SDL_JOYSTICK_BUTTON_X: u8 = 3;
const SDL_JOYSTICK_X_AXIS: u8 = 0;
const SDL_JOYSTICK_Y_AXIS: u8 = 1;

// Masks (little-endian, matches types.h since USE_ALPHA is off)
const Rmsk: u32 = 0x000000ff;
const Gmsk: u32 = 0x0000ff00;
const Bmsk: u32 = 0x00ff0000;
const Amsk: u32 = 0xff000000;

const POP_MAX_PATH: usize = 256;
const BASE_FPS: c_int = 60;
const FAST_FORWARD_RATIO: c_int = 10;
const NUM_TIMERS: usize = 3;

// SDL hint strings
const SDL_HINT_RENDER_SCALE_QUALITY: &[u8] = b"SDL_RENDER_SCALE_QUALITY\0";
const SDL_HINT_RENDER_VSYNC: &[u8] = b"SDL_RENDER_VSYNC\0";
const SDL_HINT_WINDOWS_DISABLE_THREAD_NAMING: &[u8] = b"SDL_WINDOWS_DISABLE_THREAD_NAMING\0";

// userevents enum
const userevent_SOUND: i32 = 0;
const userevent_TIMER: i32 = 1;

// ============================================================================
// helper macros / helpers
// ============================================================================
macro_rules! cs {
    ($s:literal) => {
        concat!($s, "\0").as_ptr() as *const c_char
    };
}

// SDL_SwapLE16 / 32 are no-ops on little-endian.
#[inline]
fn swaple16(x: u16) -> u16 { x }
#[inline]
fn swaple32(x: u32) -> u32 { x }

#[inline]
fn MIN_i(a: c_int, b: c_int) -> c_int { if a < b { a } else { b } }
#[inline]
fn MAX_i(a: c_int, b: c_int) -> c_int { if a > b { a } else { b } }

/// Renders a NUL-terminated C string the way `printf("%s", p)` would, so the
/// `format!` calls that replaced this file's `printf`/`snprintf` calls produce
/// byte-identical output -- including glibc's `(null)` for a null pointer.
#[inline]
unsafe fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        "(null)".to_string()
    } else {
        std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

/// `EOF`, and a sentinel distinct from it for "no pushed-back character" --
/// both used by the hand-rolled `fscanf` stand-in in [`load_sound_names`].
const EOF_CH: c_int = -1;
const NO_PENDING: c_int = -2;

/// C's `isspace` for the default locale, which is what `scanf`'s whitespace
/// directives use. (Rust's `is_ascii_whitespace` omits the vertical tab.)
#[inline]
fn is_c_space(c: c_int) -> bool {
    matches!(c as u8, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

// snprintf_check macro (from common.h). On truncation: print and quit(2).
// Takes Rust `format!` arguments rather than a C format string + varargs.
macro_rules! snprintf_check {
    ($dst:expr, $size:expr, $($arg:tt)*) => {{
        let __len = crate::write_c_str_truncating($dst, $size as usize, &format!($($arg)*));
        if __len < 0 || __len >= ($size as c_int) {
            crate::c_log_err("seg009: buffer truncation detected!\n");
            quit(2);
        }
    }};
}

// ============================================================================
// File-local statics (seg009.c data section).
// audio_speed, midi_playing, digi_audiospec, digi_unavailable are referenced
// from other translation units, so they are #[no_mangle].
// ============================================================================
static mut exe_dir: [c_char; POP_MAX_PATH] = {
    let mut a = [0 as c_char; POP_MAX_PATH];
    a[0] = b'.' as c_char;
    a
};
static mut found_exe_dir: bool = false;
static mut home_dir: [c_char; POP_MAX_PATH] = [0; POP_MAX_PATH];
static mut found_home_dir: bool = false;
static mut share_dir: [c_char; POP_MAX_PATH] = [0; POP_MAX_PATH];
static mut found_share_dir: bool = false;

/// Head of the open-DAT chain, most recently opened first. See [`open_dat`].
static mut dat_chain_ptr: *mut dat_type = null_mut();
/// Last printable character typed, from SDL_TEXTINPUT. See [`input_str`].
static mut last_text_input: c_char = 0;

/// Bitmask of palette rows currently claimed by loaded sprite sheets.
static mut chtab_palette_bits: word = 1;

/// The game's own 256-entry palette, in 6-bit VGA components.
static mut palette: [rgb_type; 256] = [rgb_type { r: 0, g: 0, b: 0 }; 256];

static mut speaker_playing: c_short = 0;
static mut digi_playing: c_short = 0;
#[no_mangle]
pub static mut midi_playing: c_short = 0;
static mut ogg_playing: c_short = 0;

/// The currently playing sound buffer for the PC speaker.
static mut current_speaker_sound: *mut speaker_type = null_mut();
/// Index for which note is currently playing.
static mut speaker_note_index: c_int = 0;
/// How long the last (partially played) speaker note has been playing.
static mut current_speaker_note_samples_already_emitted: c_int = 0;

/// The current buffer, holds the resampled sound data.
static mut digi_buffer: *mut byte = null_mut();
/// The current position in digi_buffer.
static mut digi_remaining_pos: *mut byte = null_mut();
/// The remaining length.
static mut digi_remaining_length: c_int = 0;

/// The properties of the audio device. Null until [`init_digi`] succeeds.
#[no_mangle]
pub static mut digi_audiospec: *mut SDL_AudioSpec = null_mut();
/// The desired samplerate. Everything will be resampled to this.
const digi_samplerate: c_int = 44100;

/// Decoder for the currently playing OGG sound (also holds the playback
/// position).
static mut ogg_decoder: *mut crate::ogg_decode::OggDecoder = null_mut();

/// Current PC-speaker output level. If the amplitude is too high, the speaker
/// sounds will be really loud!
static mut square_wave_state: c_short = 4000;
/// Fractional phase carried between audio callbacks.
static mut square_wave_samples_since_last_flip: f32 = 0.0;

/// Fast-forward multiplier applied to the audio clock. See [`audio_callback`].
#[no_mangle]
pub static mut audio_speed: c_int = 1;

/// Latches once the audio device fails to open, disabling all audio.
#[no_mangle]
pub static mut digi_unavailable: c_int = 0;

const sound_channel: c_int = 0;
const max_sound_id: c_int = 58;

/// Cached digi layout version; -1 until determined. See
/// [`determine_wave_version`].
static mut wave_version: c_int = -1;

static mut RGB24_bug_checked: bool = false;
static mut RGB24_bug_affected: bool = false;

/// Game ticks per second. Multiplied by FAST_FORWARD_RATIO while
/// fast-forwarding.
static mut fps: c_int = BASE_FPS;
static mut milliseconds_per_tick: f32 = 1000.0 / (BASE_FPS as f32);
/// Per-timer start point, in performance-counter units.
static mut timer_last_counter: [u64; NUM_TIMERS] = [0; NUM_TIMERS];
/// Per-timer length, in game ticks.
static mut wait_time: [c_int; NUM_TIMERS] = [0; NUM_TIMERS];

/// Set when Tab is already held on window focus (from Alt+Tab), so the
/// keystroke is swallowed rather than reaching the game.
static mut ignore_tab: bool = false;
/// Whether any key interrupts [`do_wait`], or only Escape.
static mut word_1D63A: word = 1;

/// 2x intermediate surface for CPU-side "fuzzy" scaling, when the renderer
/// cannot do it on a render target. See [`init_scaling`].
static mut onscreen_surface_2x: *mut SDL_Surface = null_mut();

// init_overlay / init_scaling "static bool initialized"
static mut overlay_initialized: bool = false;

/// The `directory_listing_type` other modules see only as an opaque pointer.
///
/// `found_filename` borrows the `DIR`'s own dirent buffer, so it is invalidated
/// by the next [`find_next_file`] -- callers must copy it before advancing.
#[repr(C)]
struct DirectoryListing {
    dp: *mut c_void,
    found_filename: *mut c_char,
    extension: *const c_char,
}

include!("seg009_hc_font_data.rs");

/// Prints the last SDL error, prefixed with `header`, in the style of `perror`.
///
/// Unlike `perror` this never exits: SDLPoP calls it from paths that go on to
/// `quit(1)` themselves and from paths that shrug the failure off.
// seg009: sdlperror
#[no_mangle]
pub unsafe extern "C" fn sdlperror(header: *const c_char) {
    let error = crate::platform::sdl::shared_renderer().get_error();
    crate::c_log(&format!("{}: {}\n", cstr(header), cstr(error)));
}

/// Fills `exe_dir` with the directory part of `argv[0]`; idempotent.
///
/// Truncating at the last `/` or `\` leaves `.` (the static's initialiser) when
/// the program was found on `$PATH` and `argv[0]` has no separator at all.
unsafe fn find_exe_dir() {
    if found_exe_dir {
        return;
    }
    snprintf_check!(
        exe_dir.as_mut_ptr(),
        core::mem::size_of_val(&exe_dir),
        "{}",
        cstr(*g_argv.offset(0))
    );
    let mut last_slash: *mut c_char = null_mut();
    let mut pos = exe_dir.as_mut_ptr();
    let mut c = *pos;
    while c != 0 {
        if c == b'/' as c_char || c == b'\\' as c_char {
            last_slash = pos;
        }
        pos = pos.add(1);
        c = *pos;
    }
    if !last_slash.is_null() {
        *last_slash = 0;
    }
    found_exe_dir = true;
}

/// Fills `home_dir` with `$HOME/.SDLPoP`, and records whether it exists.
///
/// `found_home_dir` stays false if the directory is absent, but `home_dir` is
/// written either way -- so the search order in [`find_first_file_match`] still
/// probes the path, it just re-derives it on every call.
unsafe fn find_home_dir() {
    if found_home_dir {
        return;
    }
    let home_path = getenv(cs!("HOME"));
    snprintf_check!(home_dir.as_mut_ptr(), POP_MAX_PATH - 1, "{}/.{}", cstr(home_path), "SDLPoP");
    if file_exists(home_dir.as_ptr()) {
        found_home_dir = true;
    }
}

/// Fills `share_dir` with `/usr/share/SDLPoP`; see [`find_home_dir`].
unsafe fn find_share_dir() {
    if found_share_dir {
        return;
    }
    snprintf_check!(share_dir.as_mut_ptr(), POP_MAX_PATH - 1, "{}/{}", "/usr/share", "SDLPoP");
    if file_exists(share_dir.as_ptr()) {
        found_share_dir = true;
    }
}

/// True if `filename` names something reachable -- file, directory or otherwise.
// seg009: file_exists
#[no_mangle]
pub unsafe extern "C" fn file_exists(filename: *const c_char) -> bool {
    access(filename, F_OK) != -1
}

/// The ordered list of directories the game searches for its data.
///
/// User overrides first, then the system-wide install, then wherever the
/// executable lives. Recomputed on each call because [`find_home_dir`] and
/// friends only cache the *result of the existence check*, not the path.
unsafe fn search_dirs() -> [*mut c_char; 3] {
    find_exe_dir();
    find_home_dir();
    find_share_dir();
    [home_dir.as_mut_ptr(), share_dir.as_mut_ptr(), exe_dir.as_mut_ptr()]
}

/// Formats `format` against each search directory in turn and stops at the
/// first combination that exists.
///
/// If nothing matches, `dst` is left holding the *last* candidate tried
/// (`exe_dir`), which is what makes the caller's subsequent `fopen` fail with a
/// useful path in the error message rather than with an empty string.
///
/// `middle` is what C passed as a format string: the two call sites use
/// `"%s/%s"` and `"%s/data/%s"`, i.e. the only variable part is the text
/// between the directory and the filename, so it is passed as that text (`"/"`
/// or `"/data/"`) instead of a `printf` format.
unsafe fn find_first_file_match(dst: *mut c_char, size: c_int, middle: &str, filename: *const c_char) -> *const c_char {
    for dir in search_dirs() {
        snprintf_check!(dst, size, "{}{}{}", cstr(dir), middle, cstr(filename));
        if file_exists(dst) {
            break;
        }
    }
    dst as *const c_char
}

/// Builds a path for a file the game intends to *write* (saves, config,
/// screenshots).
///
/// Takes the first search directory that both exists and is writable, rather
/// than the first that contains the file -- the file typically does not exist
/// yet. Falls through to leaving `dst` untouched if none qualifies.
// seg009: locate_save_file_
#[no_mangle]
pub unsafe extern "C" fn locate_save_file_(filename: *const c_char, dst: *mut c_char, size: c_int) -> *const c_char {
    for dir in search_dirs() {
        let mut path_stat: stat_t = core::mem::zeroed();
        let result = stat(dir, &mut path_stat);
        if result == 0 && S_ISDIR(path_stat.st_mode) && access(dir, W_OK) == 0 {
            snprintf_check!(dst, size, "{}/{}", cstr(dir), cstr(filename));
            break;
        }
    }
    dst as *const c_char
}

/// Resolves a path for reading: `filename` as given if it already resolves,
/// otherwise the first search-directory hit.
///
/// Note the return value aliases *either* the caller's `filename` or the
/// caller's `path_buffer`, so `path_buffer` must outlive the result.
// seg009: locate_file_
#[no_mangle]
pub unsafe extern "C" fn locate_file_(filename: *const c_char, path_buffer: *mut c_char, buffer_size: c_int) -> *const c_char {
    if file_exists(filename) {
        filename
    } else {
        find_first_file_match(path_buffer, buffer_size, "/", filename)
    }
}

/// Advances `data`'s `readdir` cursor to the next entry whose extension matches,
/// and returns whether one was found.
///
/// Shared by [`create_directory_listing_and_find_first_file`] and
/// [`find_next_file`]; the only difference between them is that the former also
/// opens the directory first.
unsafe fn advance_to_next_matching_file(data: *mut DirectoryListing) -> bool {
    loop {
        let ep = readdir((*data).dp);
        if ep.is_null() {
            return false;
        }
        // d_name is an inline array in the dirent, so this borrows the DIR's
        // buffer -- it stays valid only until the next readdir on this handle.
        let dname = core::ptr::addr_of_mut!((*ep).d_name) as *mut c_char;
        let ext = strrchr(dname, '.' as c_int);
        if !ext.is_null() && strcasecmp(ext.add(1), (*data).extension) == 0 {
            (*data).found_filename = dname;
            return true;
        }
    }
}

/// Opens `directory` and positions the cursor on the first `*.extension` entry.
///
/// Returns null (having freed everything) if the directory cannot be opened or
/// holds no matching file, so callers can treat "no listing" and "empty
/// listing" identically. Used to enumerate mods and replay files.
// seg009: create_directory_listing_and_find_first_file
#[no_mangle]
pub unsafe extern "C" fn create_directory_listing_and_find_first_file(directory: *const c_char, extension: *const c_char) -> *mut directory_listing_type {
    let data = calloc(1, core::mem::size_of::<DirectoryListing>()) as *mut DirectoryListing;
    (*data).dp = opendir(directory);
    (*data).extension = extension;
    let ok = !(*data).dp.is_null() && advance_to_next_matching_file(data);
    if ok {
        data as *mut directory_listing_type
    } else {
        free(data as *mut c_void);
        null_mut()
    }
}

/// The filename the listing's cursor currently sits on.
// seg009: get_current_filename_from_directory_listing
#[no_mangle]
pub unsafe extern "C" fn get_current_filename_from_directory_listing(data: *mut directory_listing_type) -> *mut c_char {
    let data = data as *mut DirectoryListing;
    (*data).found_filename
}

/// Advances the listing to the next matching file; false once exhausted.
// seg009: find_next_file
#[no_mangle]
pub unsafe extern "C" fn find_next_file(data: *mut directory_listing_type) -> bool {
    advance_to_next_matching_file(data as *mut DirectoryListing)
}

/// Closes the directory handle and frees the listing.
// seg009: close_directory_listing
#[no_mangle]
pub unsafe extern "C" fn close_directory_listing(data: *mut directory_listing_type) {
    let data = data as *mut DirectoryListing;
    closedir((*data).dp);
    free(data as *mut c_void);
}

/// Consumes and returns the pending keystroke, or 0 if there is none.
///
/// The scancode carries modifier bits in its high bits (see
/// `key_modifiers_WITH_*`), which is why the game compares against composite
/// values like `SDL_SCANCODE_Q | WITH_CTRL`.
// seg009:000D read_key
#[no_mangle]
pub unsafe extern "C" fn read_key() -> c_int {
    let key = last_key_scancode;
    last_key_scancode = 0;
    key
}

/// Drops any pending keystroke and typed character.
// seg009:019A clear_kbd_buf
#[no_mangle]
pub unsafe extern "C" fn clear_kbd_buf() {
    last_key_scancode = 0;
    last_text_input = 0;
}

/// Returns a pseudo-random value in `0..=max`.
///
/// The original MSVC LCG, kept bit-exact because replays depend on it: every
/// guard's decision and every random level detail comes out of this sequence,
/// so any change here desynchronises every recorded replay. The seed is taken
/// from the clock on first use unless a replay (or `seed=`) has already set it.
// seg009:040A prandom
#[no_mangle]
pub unsafe extern "C" fn prandom(max: word) -> word {
    if seed_was_init == 0 {
        random_seed = time(null_mut()) as dword;
        seed_was_init = 1;
    }
    random_seed = random_seed.wrapping_mul(214013).wrapping_add(2531011);
    ((random_seed >> 16) % ((max as dword) + 1)) as word
}

/// Identity: rounding x to a byte boundary was an EGA-era concern with no
/// meaning for the SDL renderer.
// seg009:0467 round_xpos_to_byte
#[no_mangle]
pub unsafe extern "C" fn round_xpos_to_byte(xpos: c_int, _round_direction: c_int) -> c_int {
    xpos
}

/// Tears SDL down and exits the process. Never returns.
// seg009:0C7A quit
#[no_mangle]
pub unsafe extern "C" fn quit(exit_code: c_int) {
    restore_stuff();
    exit(exit_code);
}

/// Shuts SDL down. Named for the DOS routine that restored the text-mode video
/// state.
// seg009:0C90 restore_stuff
#[no_mangle]
pub unsafe extern "C" fn restore_stuff() {
    crate::platform::sdl::shared_renderer().sdl_quit();
}

/// [`read_key`] plus the global Ctrl+Q quit handler.
///
/// Called from every modal loop in the game (dialogs, fades, `input_str`), which
/// is what makes Ctrl+Q work everywhere rather than only in gameplay. Saving the
/// replay and closing the menu happen first so neither is lost on the way out.
// seg009:0E33 key_test_quit
#[no_mangle]
pub unsafe extern "C" fn key_test_quit() -> c_int {
    let key: word = read_key() as word;
    if key as c_int == (SDL_SCANCODE_Q | (key_modifiers_WITH_CTRL as c_int)) {
        if recording != 0 {
            save_recorded_replay_dialog();
        }
        if is_menu_shown != 0 {
            menu_was_closed();
        }
        quit(0);
    }
    key as c_int
}

/// Finds `param` on the command line and returns the argument that follows it.
///
/// The command line is positional and untyped: `prince megahit 3` means "cheats
/// on, start at level 3", so an option's *value* is simply the next argv entry.
/// Two options (`mod`, `validate`) always consume a following argument, so they
/// are skipped past before the name comparison -- otherwise
/// `prince mod full validate` would see `full` as a candidate option name.
/// Arguments containing a `.` are skipped entirely: they are filenames.
///
/// Matching is a case-insensitive *prefix* test, so `check_param("full")` also
/// answers for `fullscreen`.
// seg009:0E54 check_param
#[no_mangle]
pub unsafe extern "C" fn check_param(param: *const c_char) -> *const c_char {
    /// The options that take a following argument.
    static PARAMS_WITH_ONE_SUBPARAM: [&[u8]; 2] = [b"mod\0", b"validate\0"];
    let mut arg_index: c_short = 1;
    while (arg_index as c_int) < g_argc {
        let curr_arg = *g_argv.offset(arg_index as isize);
        if !strchr(curr_arg, '.' as c_int).is_null() {
            arg_index += 1;
            continue;
        }
        let curr_arg_has_one_subparam = PARAMS_WITH_ONE_SUBPARAM.iter().any(|p| {
            let p = p.as_ptr() as *const c_char;
            strncasecmp(curr_arg, p, strlen(p)) == 0
        });
        if curr_arg_has_one_subparam {
            arg_index += 1;
            if (arg_index as c_int) >= g_argc {
                return null_mut();
            }
        }
        if strncasecmp(curr_arg, param, strlen(param)) == 0 {
            return *g_argv.offset(arg_index as isize);
        }
        arg_index += 1;
    }
    null_mut()
}

/// Starts timer `timer_index` for `time` ticks and blocks until it expires.
// seg009:0EDF pop_wait
#[no_mangle]
pub unsafe extern "C" fn pop_wait(timer_index: c_int, time: c_int) -> c_int {
    start_timer(timer_index, time);
    do_wait(timer_index)
}

/// Opens a `.DAT` from the working directory, else from `data/`, else from a
/// `data/` under any search directory.
///
/// The `S_ISREG` guard matters because the fallback layout for a *missing* DAT
/// is a *directory* of the same name (see [`open_dat`]); without it, `fopen`
/// would be handed a directory path.
unsafe fn open_dat_from_root_or_data_dir(filename: *const c_char) -> *mut FILE {
    let mut fp: *mut FILE = fopen(filename, cs!("rb"));

    // if failed, try if the DAT file can be opened in the data/ directory, instead of the main folder
    if fp.is_null() {
        let mut data_path = [0 as c_char; POP_MAX_PATH];
        snprintf_check!(data_path.as_mut_ptr(), POP_MAX_PATH, "data/{}", cstr(filename));
        if !file_exists(data_path.as_ptr()) {
            find_first_file_match(data_path.as_mut_ptr(), POP_MAX_PATH as c_int, "/data/", filename);
        }
        // verify that this is a regular file and not a directory (otherwise, don't open)
        let mut path_stat: stat_t = core::mem::zeroed();
        stat(data_path.as_ptr(), &mut path_stat);
        if S_ISREG(path_stat.st_mode) {
            fp = fopen(data_path.as_ptr(), cs!("rb"));
        }
    }
    fp
}

/// Opens a `.DAT` archive and pushes it onto the front of the open-DAT chain.
///
/// A `dat_type` node is always allocated and always linked, even when the file
/// could not be opened or is corrupt -- a node with a null `handle` still
/// participates in resource lookup, because
/// [`load_from_opendats_metadata`] will then look for loose `data/<name>/res*`
/// files under that name instead. Being at the *front* of the chain is what
/// gives later-opened DATs priority over earlier ones.
///
/// `optional` is tri-state despite its `int` type: 0 means "required, complain
/// loudly if absent", nonzero means "quietly tolerate absence", and the
/// specific value `'G'` marks a graphics DAT, which the
/// `always_use_original_graphics` option uses to skip the mod folder.
// seg009:0F58 open_dat
#[no_mangle]
pub unsafe extern "C" fn open_dat(filename: *const c_char, mut optional: c_int) -> *mut dat_type {
    let mut fp: *mut FILE = null_mut();
    if use_custom_levelset == 0 {
        fp = open_dat_from_root_or_data_dir(filename);
    } else {
        // Don't complain about missing data files if we are only looking in the mod folder, because
        // they might exist in the data folder. (Possible only if open_dat() was called by
        // load_all_sounds().)
        if !skip_mod_data_files && skip_normal_data_files {
            optional = 1;
        }
        if !skip_mod_data_files && !(always_use_original_graphics != 0 && optional == 'G' as c_int) {
            // before checking the root directory, first try mods/MODNAME/
            let mut filename_mod = [0 as c_char; POP_MAX_PATH];
            snprintf_check!(filename_mod.as_mut_ptr(), POP_MAX_PATH, "{}/{}", cstr(mod_data_path.as_ptr()), cstr(filename));
            fp = fopen(filename_mod.as_ptr(), cs!("rb"));
        }
        if fp.is_null() && !skip_normal_data_files {
            fp = open_dat_from_root_or_data_dir(filename);
        }
    }

    let pointer = calloc(1, core::mem::size_of::<dat_type>()) as *mut dat_type;
    snprintf_check!(core::ptr::addr_of_mut!((*pointer).filename) as *mut c_char, 256, "{}", cstr(filename));
    (*pointer).next_dat = dat_chain_ptr;
    dat_chain_ptr = pointer;

    if !fp.is_null() {
        read_dat_table_into(pointer, fp, filename);
    } else if optional == 0 {
        complain_if_no_fallback_folder(pointer, filename);
    }
    pointer
}

/// Reads a DAT's 6-byte header and its resource table, attaching both to
/// `pointer` on success.
///
/// C's `goto failed` tail: on any of the three failure points the handle and
/// the partially-read table are released and `pointer` is left with its
/// calloc'd null `handle`, which downgrades it to a directory-only entry rather
/// than removing it from the chain.
unsafe fn read_dat_table_into(pointer: *mut dat_type, fp: *mut FILE, filename: *const c_char) {
    let mut dat_header: dat_header_type = core::mem::zeroed();
    let mut dat_table: *mut dat_table_type = null_mut();
    let ok = 'read: {
        if fread(core::ptr::addr_of_mut!(dat_header) as *mut c_void, 6, 1, fp) != 1 {
            break 'read false;
        }
        dat_table = malloc(swaple16(dat_header.table_size) as usize) as *mut dat_table_type;
        !dat_table.is_null()
            && fseek(fp, swaple32(dat_header.table_offset) as c_long, SEEK_SET) == 0
            && fread(dat_table as *mut c_void, swaple16(dat_header.table_size) as usize, 1, fp) == 1
    };
    if ok {
        (*pointer).handle = fp;
        (*pointer).dat_table = dat_table;
    } else {
        perror(filename);
        fclose(fp);
        if !dat_table.is_null() {
            free(dat_table as *mut c_void);
        }
    }
}

/// A required DAT is missing: check whether the directory that may stand in for
/// it exists, and if not, tell the player and quit.
///
/// The fallback for `TITLE.DAT` is a folder simply named `data/TITLE`, holding
/// one file per resource. Showing the message needs a screen and a dialog
/// template, so if either is still uninitialised the game carries on and fails
/// later, more obscurely -- which is why `pop_main` moves the first `open_dat`
/// call after `init_copyprot_dialog`.
unsafe fn complain_if_no_fallback_folder(pointer: *mut dat_type, filename: *const c_char) {
    // strip the .DAT file extension from the filename (use folders simply named TITLE, KID, ...)
    let mut filename_no_ext = [0 as c_char; POP_MAX_PATH];
    strncpy(filename_no_ext.as_mut_ptr(), core::ptr::addr_of!((*pointer).filename) as *const c_char, POP_MAX_PATH);
    let len = strlen(filename_no_ext.as_ptr());
    if len >= 5 && filename_no_ext[len - 4] == '.' as c_char {
        filename_no_ext[len - 4] = 0;
    }
    let mut foldername = [0 as c_char; POP_MAX_PATH];
    snprintf_check!(foldername.as_mut_ptr(), POP_MAX_PATH, "data/{}", cstr(filename_no_ext.as_ptr()));
    let mut located = [0 as c_char; POP_MAX_PATH];
    let data_path = locate_file_(foldername.as_ptr(), located.as_mut_ptr(), POP_MAX_PATH as c_int);
    let mut path_stat: stat_t = core::mem::zeroed();
    let result = stat(data_path, &mut path_stat);
    if result != 0 || !S_ISDIR(path_stat.st_mode) {
        let mut error_message = [0 as c_char; 256];
        snprintf_check!(error_message.as_mut_ptr(), 256, "Cannot find a required data file: {} or folder: {}\nPress any key to quit.", cstr(filename), cstr(foldername.as_ptr()));
        // otherwise showmessage will crash
        if !onscreen_surface_.is_null() && !copyprot_dialog.is_null() {
            showmessage(error_message.as_mut_ptr(), 1, key_test_quit as *mut c_void);
            quit(1);
        }
    }
}

/// Installs a sprite sheet's palette into the 16 VGA palette rows it claims.
///
/// `row_bits` is a bitmask over the 16 rows of 16 colours each; the source rows
/// are packed consecutively, so `source_row` advances only on a set bit while
/// `dest_index` advances every iteration.
// seg009:9CAC set_loaded_palette
#[no_mangle]
pub unsafe extern "C" fn set_loaded_palette(palette_ptr: *mut dat_pal_type) {
    let mut source_row: c_int = 0;
    let vga_base = core::ptr::addr_of!((*palette_ptr).vga) as *const rgb_type;
    for dest_row in 0..16 {
        if ((*palette_ptr).row_bits as c_int) & (1 << dest_row) != 0 {
            set_pal_arr(dest_row * 0x10, 16, vga_base.add((source_row * 0x10) as usize));
            source_row += 1;
        }
    }
}

/// Loads a whole sprite sheet: the palette at `resource`, then the `n_images`
/// sprites at `resource + 1 ..= resource + n_images`.
///
/// The returned `chtab_type` is a header followed by a flexible array of image
/// pointers, so it is allocated as one block and the pointers are reached
/// through `addr_of_mut!`. Individual images may be null (missing resources are
/// tolerated); `n_images` counts slots, not successes.
// seg009:104E load_sprites_from_file
#[no_mangle]
pub unsafe extern "C" fn load_sprites_from_file(resource: c_int, palette_bits: c_int, quit_on_error: c_int) -> *mut chtab_type {
    let shpl = load_from_opendats_alloc(resource, cs!("pal"), null_mut(), null_mut()) as *mut dat_shpl_type;
    if shpl.is_null() {
        crate::c_log(&format!("Can't load sprites from resource {}.\n", resource));
        if quit_on_error != 0 {
            // Unfortunately we don't know at this point which data file is missing, so we use the
            // name of the last opened DAT file. It's also possible that the DAT file exists and it
            // just doesn't contain the needed resource.
            let mut error_message = [0 as c_char; 256];
            snprintf_check!(error_message.as_mut_ptr(), 256, "Can't load sprites from resource {}.\nThe last opened data file is: {}\nPress any key to quit.", resource, cstr(core::ptr::addr_of!((*dat_chain_ptr).filename) as *const c_char));
            showmessage(error_message.as_mut_ptr(), 1, key_test_quit as *mut c_void);
            quit(1);
        }
        return null_mut();
    }
    let pal_ptr = core::ptr::addr_of_mut!((*shpl).palette);
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int {
        // The palette_bits == 0 branch (auto-allocating rows via add_palette_bits) is commented out
        // in the C source; add_palette_bits is a stub returning 0, so it would have quit(1).
        if palette_bits != 0 {
            chtab_palette_bits |= palette_bits as word;
        }
        (*pal_ptr).row_bits = palette_bits as word;
    }
    let n_images = (*shpl).n_images as c_int;
    let alloc_size = core::mem::size_of::<chtab_type>() + core::mem::size_of::<*mut c_void>() * (n_images as usize);
    let chtab = malloc(alloc_size) as *mut chtab_type;
    memset(chtab as *mut c_void, 0, alloc_size);
    (*chtab).n_images = n_images as word;
    let images = core::ptr::addr_of_mut!((*chtab).images) as *mut *mut image_type;
    for i in 1..=n_images {
        *images.add((i - 1) as usize) = load_image(resource + i, pal_ptr);
    }
    set_loaded_palette(pal_ptr);
    chtab
}

/// Frees a sprite sheet and every surface in it, releasing its palette rows.
// seg009:11A8 free_chtab
#[no_mangle]
pub unsafe extern "C" fn free_chtab(chtab_ptr: *mut chtab_type) {
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int && (*chtab_ptr).has_palette_bits != 0 {
        chtab_palette_bits &= !(*chtab_ptr).chtab_palette_bits;
    }
    let images = core::ptr::addr_of_mut!((*chtab_ptr).images) as *mut *mut image_type;
    for id in 0..(*chtab_ptr).n_images as usize {
        let curr_image = *images.add(id);
        if !curr_image.is_null() {
            crate::platform::sdl::shared_renderer().free_surface(curr_image);
        }
    }
    free(chtab_ptr as *mut c_void);
}

/// RLE, left-to-right: a run header byte, then either literal bytes or one byte
/// to repeat.
///
/// A non-negative header `n` means "copy the next `n + 1` literal bytes"; a
/// negative header means "repeat the next byte `-n` times". Both `count`
/// updates deliberately wrap in an `i8`, exactly as the C `sbyte` does: `++count`
/// on 127 lands on -128, and `-count` on -128 stays -128. Neither is reachable
/// from well-formed data, but the wrap is what the C source does, so it is kept.
// seg009:8CE6 decompress_rle_lr
unsafe fn decompress_rle_lr(destination: *mut byte, source: *const byte, dest_length: c_int) {
    let mut src_pos = source;
    let mut dest_pos = destination;
    let mut rem_length: c_short = dest_length as c_short;
    while rem_length != 0 {
        let mut count: i8 = *src_pos as i8;
        src_pos = src_pos.add(1);
        if count >= 0 {
            count = count.wrapping_add(1);
            loop {
                *dest_pos = *src_pos;
                dest_pos = dest_pos.add(1);
                src_pos = src_pos.add(1);
                rem_length -= 1;
                count = count.wrapping_sub(1);
                if !(count != 0 && rem_length != 0) {
                    break;
                }
            }
        } else {
            let al = *src_pos;
            src_pos = src_pos.add(1);
            count = count.wrapping_neg();
            loop {
                *dest_pos = al;
                dest_pos = dest_pos.add(1);
                rem_length -= 1;
                count = count.wrapping_sub(1);
                if !(count != 0 && rem_length != 0) {
                    break;
                }
            }
        }
    }
}

/// RLE, up-to-down: same run encoding as [`decompress_rle_lr`], but the output
/// cursor walks down a column and wraps to the top of the next one.
///
/// `dest_pos` advances by `width` (pre-decremented, so `+1 + width` = one full
/// row) after each byte; when `rem_height` hits zero it jumps back by
/// `dest_length - 1` to the top of the following column.
// seg009:8D1C decompress_rle_ud
unsafe fn decompress_rle_ud(destination: *mut byte, source: *const byte, mut dest_length: c_int, mut width: c_int, height: c_int) {
    let mut rem_height: c_short = height as c_short;
    let mut src_pos = source;
    let mut dest_pos = destination;
    let mut rem_length: c_short = dest_length as c_short;
    dest_length -= 1;
    width -= 1;
    while rem_length != 0 {
        let mut count: i8 = *src_pos as i8;
        src_pos = src_pos.add(1);
        if count >= 0 {
            count = count.wrapping_add(1);
            loop {
                *dest_pos = *src_pos;
                dest_pos = dest_pos.add(1);
                src_pos = src_pos.add(1);
                dest_pos = dest_pos.offset(width as isize);
                rem_height -= 1;
                if rem_height == 0 {
                    dest_pos = dest_pos.offset(-(dest_length as isize));
                    rem_height = height as c_short;
                }
                rem_length -= 1;
                count = count.wrapping_sub(1);
                if !(count != 0 && rem_length != 0) {
                    break;
                }
            }
        } else {
            let al = *src_pos;
            src_pos = src_pos.add(1);
            count = count.wrapping_neg();
            loop {
                *dest_pos = al;
                dest_pos = dest_pos.add(1);
                dest_pos = dest_pos.offset(width as isize);
                rem_height -= 1;
                if rem_height == 0 {
                    dest_pos = dest_pos.offset(-(dest_length as isize));
                    rem_height = height as c_short;
                }
                rem_length -= 1;
                count = count.wrapping_sub(1);
                if !(count != 0 && rem_length != 0) {
                    break;
                }
            }
        }
    }
}

/// LZ77-style, left-to-right, over a 1 KB circular window.
///
/// The stream is a sequence of *groups*: one flag byte, then up to eight items,
/// one per bit, low bit first. A 1 bit is a literal byte; a 0 bit is a two-byte
/// back-reference, big-endian, packing a 10-bit window offset in the low bits
/// and a length-minus-3 in the high six.
///
/// The `mask` register does double duty as both the flag bits and the counter
/// for how many are left: refilling it as `byte | 0xFF00` seeds the high byte
/// with ones, and `(mask & 0xFF00) == 0` becomes true exactly when all eight
/// original bits have been shifted out. That is the 8086 idiom, kept verbatim.
///
/// The window starts zeroed with the cursor at `0x400 - 0x42`, so early
/// back-references can legitimately point at never-written bytes and read
/// zeroes -- that is by design, not a bug.
// seg009:90FA decompress_lzg_lr
unsafe fn decompress_lzg_lr(dest: *mut byte, source: *const byte, dest_length: c_int) -> *mut byte {
    let window = malloc(0x400) as *mut byte;
    if window.is_null() {
        return null_mut();
    }
    memset(window as *mut c_void, 0, 0x400);
    let mut window_pos = window.add(0x400 - 0x42);
    let mut remaining: c_short = dest_length as c_short;
    let window_end = window.add(0x400);
    let mut source_pos = source;
    let mut dest_pos = dest;
    let mut mask: word = 0;
    loop {
        mask >>= 1;
        if (mask & 0xFF00) == 0 {
            mask = (*source_pos as word) | 0xFF00;
            source_pos = source_pos.add(1);
        }
        if mask & 1 != 0 {
            let v = *source_pos;
            *window_pos = v;
            *dest_pos = v;
            window_pos = window_pos.add(1);
            dest_pos = dest_pos.add(1);
            source_pos = source_pos.add(1);
            if window_pos >= window_end {
                window_pos = window;
            }
            remaining = remaining.wrapping_sub(1);
        } else {
            let mut copy_info: word = *source_pos as word;
            source_pos = source_pos.add(1);
            copy_info = (copy_info << 8) | (*source_pos as word);
            source_pos = source_pos.add(1);
            let mut copy_source = window.add((copy_info & 0x3FF) as usize);
            let mut copy_length: byte = ((copy_info >> 10) + 3) as byte;
            loop {
                let v = *copy_source;
                *window_pos = v;
                *dest_pos = v;
                window_pos = window_pos.add(1);
                dest_pos = dest_pos.add(1);
                copy_source = copy_source.add(1);
                if copy_source >= window_end {
                    copy_source = window;
                }
                if window_pos >= window_end {
                    window_pos = window;
                }
                remaining = remaining.wrapping_sub(1);
                copy_length = copy_length.wrapping_sub(1);
                if !(remaining != 0 && copy_length != 0) {
                    break;
                }
            }
        }
        if remaining == 0 {
            break;
        }
    }
    free(window as *mut c_void);
    dest
}

/// LZ77 as in [`decompress_lzg_lr`], but writing down columns.
///
/// Note the two counters are *not* the same quantity here: `remaining` counts
/// down the current column (reloaded from `height` at each wrap) while
/// `dest_length` counts total bytes left and is what terminates the loop.
// seg009:91AD decompress_lzg_ud
unsafe fn decompress_lzg_ud(dest: *mut byte, source: *const byte, mut dest_length: c_int, stride: c_int, height: c_int) -> *mut byte {
    let window = malloc(0x400) as *mut byte;
    if window.is_null() {
        return null_mut();
    }
    memset(window as *mut c_void, 0, 0x400);
    let mut window_pos = window.add(0x400 - 0x42);
    let mut remaining: c_short = height as c_short;
    let window_end = window.add(0x400);
    let mut source_pos = source;
    let mut dest_pos = dest;
    let mut mask: word = 0;
    let dest_end: c_short = (dest_length - 1) as c_short;
    loop {
        mask >>= 1;
        if (mask & 0xFF00) == 0 {
            mask = (*source_pos as word) | 0xFF00;
            source_pos = source_pos.add(1);
        }
        if mask & 1 != 0 {
            let v = *source_pos;
            *window_pos = v;
            *dest_pos = v;
            window_pos = window_pos.add(1);
            source_pos = source_pos.add(1);
            dest_pos = dest_pos.offset(stride as isize);
            remaining = remaining.wrapping_sub(1);
            if remaining == 0 {
                dest_pos = dest_pos.offset(-(dest_end as isize));
                remaining = height as c_short;
            }
            if window_pos >= window_end {
                window_pos = window;
            }
            dest_length -= 1;
        } else {
            let mut copy_info: word = *source_pos as word;
            source_pos = source_pos.add(1);
            copy_info = (copy_info << 8) | (*source_pos as word);
            source_pos = source_pos.add(1);
            let mut copy_source = window.add((copy_info & 0x3FF) as usize);
            let mut copy_length: byte = ((copy_info >> 10) + 3) as byte;
            loop {
                let v = *copy_source;
                *window_pos = v;
                *dest_pos = v;
                window_pos = window_pos.add(1);
                copy_source = copy_source.add(1);
                dest_pos = dest_pos.offset(stride as isize);
                remaining = remaining.wrapping_sub(1);
                if remaining == 0 {
                    dest_pos = dest_pos.offset(-(dest_end as isize));
                    remaining = height as c_short;
                }
                if copy_source >= window_end {
                    copy_source = window;
                }
                if window_pos >= window_end {
                    window_pos = window;
                }
                dest_length -= 1;
                copy_length = copy_length.wrapping_sub(1);
                if !(dest_length != 0 && copy_length != 0) {
                    break;
                }
            }
        }
        if dest_length == 0 {
            break;
        }
    }
    free(window as *mut c_void);
    dest
}

/// Dispatches to the decompressor named by the image's compression-method field.
///
/// Method 0 is stored raw. An unrecognised method leaves `dest` as the caller
/// zeroed it, producing a blank sprite rather than a crash.
// seg009:938E decompr_img
unsafe fn decompr_img(dest: *mut byte, source: *const image_data_type, decomp_size: c_int, cmeth: c_int, stride: c_int) {
    let data_ptr = core::ptr::addr_of!((*source).data) as *const byte;
    let height = swaple16((*source).height) as c_int;
    match cmeth {
        0 => { memcpy(dest as *mut c_void, data_ptr as *const c_void, decomp_size as usize); } // RAW left-to-right
        1 => { decompress_rle_lr(dest, data_ptr, decomp_size); }                               // RLE left-to-right
        2 => { decompress_rle_ud(dest, data_ptr, decomp_size, stride, height); }               // RLE up-to-down
        3 => { decompress_lzg_lr(dest, data_ptr, decomp_size); }                               // LZG left-to-right
        4 => { decompress_lzg_ud(dest, data_ptr, decomp_size, stride, height); }               // LZG up-to-down
        _ => {}
    }
}

/// Bytes per packed row: `width` pixels at 1..8 bits each, rounded up.
///
/// Bit depth lives in bits 12..14 of the image flags, stored biased by one.
unsafe fn calc_stride(image_data: *mut image_data_type) -> c_int {
    let width = swaple16((*image_data).width) as c_int;
    let flags = swaple16((*image_data).flags) as c_int;
    let depth = ((flags >> 12) & 7) + 1;
    (depth * width + 7) / 8
}

/// Expands a packed `depth`-bits-per-pixel bitmap to one byte per pixel.
///
/// Pixels are unpacked most-significant-bits first within each byte, and the
/// row ends at `width` pixels even when the last byte holds more -- which is why
/// the inner loop is bounded by both `pixels_per_byte` and `x_pixel < width`.
unsafe fn conv_to_8bpp(in_data: *mut byte, width: c_int, height: c_int, stride: c_int, depth: c_int) -> *mut byte {
    let out_data = malloc((width * height) as usize) as *mut byte;
    let pixels_per_byte = 8 / depth;
    let mask = (1 << depth) - 1;
    for y in 0..height {
        let mut out_pos = out_data.offset((y * width) as isize);
        let mut x_pixel: c_int = 0;
        for x_byte in 0..stride {
            let packed = *in_data.offset((y * stride + x_byte) as isize) as c_int;
            let mut shift = 8;
            let mut pixel_in_byte = 0;
            while pixel_in_byte < pixels_per_byte && x_pixel < width {
                shift -= depth;
                *out_pos = ((packed >> shift) & mask) as byte;
                out_pos = out_pos.add(1);
                pixel_in_byte += 1;
                x_pixel += 1;
            }
        }
    }
    out_data
}

/// Turns a DAT image resource into an 8-bit paletted SDL surface.
///
/// Decompress into a packed buffer, expand to 8bpp, copy row by row into the
/// surface (`pitch` is not necessarily `width`), then attach the 16-colour
/// palette. VGA components are 6-bit, hence the `<< 2` to reach 8-bit; colour 0
/// is forced to transparent black because that is the game's universal
/// "background" index.
///
/// A zero-height image is a legitimate encoding of "nothing here" and yields a
/// null surface, which callers treat as an absent sprite.
// seg009 decode_image
#[no_mangle]
pub unsafe extern "C" fn decode_image(image_data: *mut image_data_type, pal: *mut dat_pal_type) -> *mut image_type {
    let height = swaple16((*image_data).height) as c_int;
    if height == 0 {
        return null_mut();
    }
    let width = swaple16((*image_data).width) as c_int;
    let flags = swaple16((*image_data).flags) as c_int;
    let depth = ((flags >> 12) & 7) + 1;
    let cmeth = (flags >> 8) & 0x0F;
    let stride = calc_stride(image_data);
    let dest_size = stride * height;
    let dest = malloc(dest_size as usize) as *mut byte;
    memset(dest as *mut c_void, 0, dest_size as usize);
    decompr_img(dest, image_data, dest_size, cmeth, stride);
    let image_8bpp = conv_to_8bpp(dest, width, height, stride, depth);
    free(dest as *mut c_void);
    let image = crate::platform::sdl::shared_renderer().create_surface(width, height, 8, 0, 0, 0, 0);
    if image.is_null() {
        sdlperror(cs!("decode_image: SDL_CreateRGBSurface"));
        quit(1);
    }
    if crate::platform::sdl::shared_renderer().lock_surface(image) != 0 {
        sdlperror(cs!("decode_image: SDL_LockSurface"));
    }
    let image_pixels = crate::platform::sdl::shared_renderer().surface_pixels(image) as *mut byte;
    let image_pitch = crate::platform::sdl::shared_renderer().surface_pitch(image);
    for y in 0..height {
        memcpy(
            image_pixels.offset((y * image_pitch) as isize) as *mut c_void,
            image_8bpp.offset((y * width) as isize) as *const c_void,
            width as usize,
        );
    }
    crate::platform::sdl::shared_renderer().unlock_surface(image);
    free(image_8bpp as *mut c_void);

    let mut colors: [SDL_Color; 16] = core::mem::zeroed();
    let vga = core::ptr::addr_of!((*pal).vga) as *const rgb_type;
    for (i, color) in colors.iter_mut().enumerate() {
        let p = vga.add(i);
        // VGA palette components are 6-bit; scale them into SDL's 8-bit range.
        color.r = (((*p).r as c_int) << 2) as u8;
        color.g = (((*p).g as c_int) << 2) as u8;
        color.b = (((*p).b as c_int) << 2) as u8;
        color.a = SDL_ALPHA_OPAQUE;
    }
    // Colour 0 is the transparent background everywhere in the game's art.
    colors[0] = SDL_Color { r: 0, g: 0, b: 0, a: SDL_ALPHA_TRANSPARENT };
    let image_palette = crate::platform::sdl::shared_renderer().surface_palette(image);
    crate::platform::sdl::shared_renderer().set_palette_colors(image_palette, colors.as_ptr(), 0, 16);
    image
}

/// Loads one image resource, from whichever source
/// [`load_from_opendats_metadata`] found it in.
///
/// A resource in a DAT is the game's own format and goes through
/// [`decode_image`]; a resource found as a loose file is handed to SDL_image
/// as a PNG. Either way colour 0 ends up transparent, so callers do not need to
/// know which path was taken.
// seg009:121A load_image
#[no_mangle]
pub unsafe extern "C" fn load_image(resource_id: c_int, pal: *mut dat_pal_type) -> *mut image_type {
    let mut result: data_location = 0;
    let mut size: c_int = 0;
    let image_data = load_from_opendats_alloc(resource_id, cs!("png"), &mut result, &mut size);
    let image = match result {
        data_location_data_none => return null_mut(),
        data_location_data_DAT => decode_image(image_data as *mut image_data_type, pal),
        data_location_data_directory => {
            let rw = crate::platform::sdl::shared_renderer().rw_from_const_mem(image_data, size);
            if rw.is_null() {
                // Note: this leaks image_data, matching the C source.
                sdlperror(cs!("load_image: SDL_RWFromConstMem"));
                return null_mut();
            }
            let image = crate::platform::sdl::shared_renderer().img_load_rw(rw, 0);
            if image.is_null() {
                crate::c_log(&format!("load_image: IMG_Load_RW: {}\n", cstr(IMG_GetError())));
            }
            if crate::platform::sdl::shared_renderer().rw_close(rw) != 0 {
                sdlperror(cs!("load_image: SDL_RWclose"));
            }
            image
        }
        _ => null_mut(),
    };
    if !image_data.is_null() {
        free(image_data);
    }
    if !image.is_null() && crate::platform::sdl::shared_renderer().set_color_key(image, true, 0) != 0 {
        sdlperror(cs!("load_image: SDL_SetColorKey"));
        quit(1);
    }
    image
}

/// Blits an image with colour 0 treated as transparent.
///
/// The `mask` parameter is vestigial: the CGA/EGA modes that needed a separate
/// mask bitmap are not implemented, so only the VGA path survives.
// seg009:13C4 draw_image_transp
#[no_mangle]
pub unsafe extern "C" fn draw_image_transp(image: *mut image_type, _mask: *mut image_type, xpos: c_int, ypos: c_int) {
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int {
        draw_image_transp_vga(image, xpos, ypos);
    }
}

/// Opens the first joystick or game controller, and picks the input mode.
///
/// A device SDL has a mapping for is opened through the GameController API;
/// anything else falls back to the raw joystick API, flagged by
/// `using_sdl_joystick_interface` so [`process_events`] knows which family of
/// events to listen for. Keyboard and joystick modes are mutually exclusive and
/// each is re-selected automatically on the next input of that kind.
// seg009:157E set_joy_mode
#[no_mangle]
pub unsafe extern "C" fn set_joy_mode() -> c_int {
    if crate::platform::sdl::shared_renderer().num_joysticks() < 1 {
        is_joyst_mode = 0;
    } else {
        if gamecontrollerdb_file[0] != 0 {
            SDL_GameControllerAddMappingsFromFile(gamecontrollerdb_file.as_ptr());
        }
        if crate::platform::sdl::shared_renderer().is_game_controller(0) {
            sdl_controller_ = crate::platform::sdl::shared_renderer().game_controller_open(0);
            if sdl_controller_.is_null() {
                is_joyst_mode = 0;
            } else {
                is_joyst_mode = 1;
            }
        } else {
            sdl_joystick_ = crate::platform::sdl::shared_renderer().joystick_open(0);
            is_joyst_mode = 1;
            using_sdl_joystick_interface = 1;
        }
    }
    if enable_controller_rumble != 0 && is_joyst_mode != 0 {
        sdl_haptic = crate::platform::sdl::shared_renderer().haptic_open(0);
        crate::platform::sdl::shared_renderer().haptic_rumble_init(sdl_haptic);
    } else {
        sdl_haptic = null_mut();
    }
    is_keyboard_mode = (is_joyst_mode == 0) as word;
    is_joyst_mode as c_int
}

/// Allocates a 24-bit drawing surface `rect.right` by `rect.bottom`.
///
/// The rect's `left`/`top` are ignored -- the game only ever asks for buffers
/// anchored at the origin.
// seg009:178B make_offscreen_buffer
#[no_mangle]
pub unsafe extern "C" fn make_offscreen_buffer(rect: *const rect_type) -> *mut surface_type {
    crate::platform::sdl::shared_renderer().create_surface((*rect).right as c_int, (*rect).bottom as c_int, 24, Rmsk, Gmsk, Bmsk, 0)
}

/// Frees a drawing surface.
// seg009:17BD free_surface
#[no_mangle]
pub unsafe extern "C" fn free_surface(surface: *mut surface_type) {
    crate::platform::sdl::shared_renderer().free_surface(surface);
}

/// Frees a "peel" -- a saved rectangle of screen content plus its surface.
///
/// Peels are how the game restores what a dialog covered up: read the region
/// before drawing, blit it back afterwards. See [`read_peel_from_screen`].
// seg009:17EA free_peel
#[no_mangle]
pub unsafe extern "C" fn free_peel(peel_ptr: *mut peel_type) {
    crate::platform::sdl::shared_renderer().free_surface((*peel_ptr).peel);
    free(peel_ptr as *mut c_void);
}

/// Loads the 16-colour "high contrast" palette used by menus and dialogs.
// seg009:182F set_hc_pal
#[no_mangle]
pub unsafe extern "C" fn set_hc_pal() {
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int {
        set_pal_arr(0, 16, core::ptr::addr_of!((*custom).vga_palette) as *const rgb_type);
    }
}

/// Flips a raw pixel buffer vertically, in place, by swapping row pairs.
///
/// Used for the upside-down rooms. Only `height / 2` swaps are needed; an odd
/// middle row is already in place.
///
/// Kept as a do-while rather than a `for` range: C runs the body once *before*
/// testing, so a `height` below 2 would swap row 0 against row -1 and then spin
/// `rem_rows` down through the whole `short` range. Only ever called on the
/// 200-row screen surface, so that path is unreachable -- but it is not our
/// business to fix it here.
// seg009:2446 flip_not_ega
#[no_mangle]
pub unsafe extern "C" fn flip_not_ega(memory: *mut byte, height: c_int, stride: c_int) {
    let row_buffer = malloc(stride as usize) as *mut byte;
    let mut top_ptr = memory;
    let mut bottom_ptr = memory.offset(((height - 1) * stride) as isize);
    let mut rem_rows: c_short = (height >> 1) as c_short;
    loop {
        memcpy(row_buffer as *mut c_void, top_ptr as *const c_void, stride as usize);
        memcpy(top_ptr as *mut c_void, bottom_ptr as *const c_void, stride as usize);
        memcpy(bottom_ptr as *mut c_void, row_buffer as *const c_void, stride as usize);
        top_ptr = top_ptr.offset(stride as isize);
        bottom_ptr = bottom_ptr.offset(-(stride as isize));
        rem_rows = rem_rows.wrapping_sub(1);
        if rem_rows == 0 {
            break;
        }
    }
    free(row_buffer as *mut c_void);
}

/// Flips a whole surface vertically. See [`flip_not_ega`].
// seg009:19B1 flip_screen
#[no_mangle]
pub unsafe extern "C" fn flip_screen(surface: *mut surface_type) {
    if graphics_mode as c_int != grmodes_gmEga as c_int {
        if crate::platform::sdl::shared_renderer().lock_surface(surface) != 0 {
            sdlperror(cs!("flip_screen: SDL_LockSurface"));
            quit(1);
        }
        let renderer = crate::platform::sdl::shared_renderer();
        let surface_pixels = renderer.surface_pixels(surface) as *mut byte;
        let (_, surface_h) = renderer.surface_size(surface);
        let surface_pitch = renderer.surface_pitch(surface);
        flip_not_ega(surface_pixels, surface_h, surface_pitch);
        crate::platform::sdl::shared_renderer().unlock_surface(surface);
    }
}

/// Blits an image to the current target with colour-key transparency.
// seg009:2288 draw_image_transp_vga
#[no_mangle]
pub unsafe extern "C" fn draw_image_transp_vga(image: *mut image_type, xpos: c_int, ypos: c_int) {
    method_6_blit_img_to_scr(image, xpos, ypos, blitters_blitters_10h_transp as c_int);
}

// ============================================================================
// Text (USE_TEXT)
//
// A `rawfont_type` is the on-disk shape: metrics, then an offset table with one
// entry per character, then the glyph bitmaps packed end to end. A `font_type`
// is the in-memory shape: the same metrics plus a chtab of decoded surfaces.
// ============================================================================

/// Rebuilds a raw font's per-character offset table by walking the glyph
/// bitmaps.
///
/// Some fonts (notably the two embedded in the binary) ship with the table
/// zeroed, because the offsets are derivable: each glyph is an `image_data_type`
/// header followed by `height * stride` bytes, so the next glyph starts
/// immediately after. Offsets are stored relative to the start of `data`.
unsafe fn load_font_character_offsets(data: *mut rawfont_type) {
    let n_chars = ((*data).last_char as c_int) - ((*data).first_char as c_int) + 1;
    let offsets = core::ptr::addr_of_mut!((*data).offsets) as *mut word;
    let mut pos = offsets.add(n_chars as usize) as *mut byte;
    for index in 0..n_chars {
        // `data` (and thus `offsets`, at a fixed even byte offset from it) is not guaranteed
        // 2-byte aligned -- it may point into a `[u8; N]` static (e.g. the embedded hc_font
        // data), so this must be an unaligned write, not a plain `*ptr = ...` store.
        offsets.add(index as usize).write_unaligned(swaple16((pos as usize - data as usize) as word));
        let image_data = pos as *mut image_data_type;
        let image_bytes = (swaple16((*image_data).height) as c_int) * calc_stride(image_data);
        pos = (core::ptr::addr_of_mut!((*image_data).data) as *mut byte).offset(image_bytes as isize);
    }
}

/// Decodes every glyph of a raw font into a `chtab` and returns the usable
/// `font_type`.
///
/// A one-colour palette (index 1 = white) is enough, because glyphs are
/// monochrome and are recoloured at blit time by [`method_3_blit_mono`].
unsafe fn load_font_from_data(data: *mut rawfont_type) -> font_type {
    let mut font: font_type = core::mem::zeroed();
    font.first_char = (*data).first_char;
    font.last_char = (*data).last_char;
    font.height_above_baseline = swaple16((*data).height_above_baseline as u16) as c_short;
    font.height_below_baseline = swaple16((*data).height_below_baseline as u16) as c_short;
    font.space_between_lines = swaple16((*data).space_between_lines as u16) as c_short;
    font.space_between_chars = swaple16((*data).space_between_chars as u16) as c_short;
    let n_chars = (font.last_char as c_int) - (font.first_char as c_int) + 1;
    let offsets = core::ptr::addr_of_mut!((*data).offsets) as *mut word;
    // Allow loading a font even if the offsets for each character image were not supplied.
    // Unaligned read: see the comment in load_font_character_offsets above.
    if swaple16(offsets.add(0).read_unaligned()) == 0 {
        load_font_character_offsets(data);
    }
    let chtab = malloc(core::mem::size_of::<chtab_type>() + core::mem::size_of::<*mut image_type>() * (n_chars as usize)) as *mut chtab_type;
    // Make a dummy palette for decode_image().
    let mut dat_pal: dat_pal_type = core::mem::zeroed();
    let dpvga = core::ptr::addr_of_mut!(dat_pal.vga) as *mut rgb_type;
    (*dpvga.add(1)) = rgb_type { r: 0x3F, g: 0x3F, b: 0x3F }; // white
    let images = core::ptr::addr_of_mut!((*chtab).images) as *mut *mut image_type;
    for index in 0..n_chars as usize {
        let image_data = (data as *mut byte).offset(swaple16(offsets.add(index).read_unaligned()) as isize) as *mut image_data_type;
        // HACK: decode_image() returns NULL if height==0.
        if (*image_data).height == swaple16(0) {
            (*image_data).height = swaple16(1);
        }
        let image = decode_image(image_data, &mut dat_pal);
        *images.add(index) = image;
        if crate::platform::sdl::shared_renderer().set_color_key(image, true, 0) != 0 {
            sdlperror(cs!("load_font_from_data: SDL_SetColorKey"));
            quit(1);
        }
    }
    font.chtab = chtab;
    font
}

/// Loads the two fonts the game uses, at `set_gr_mode` time.
///
/// The main font is preferred from `font.dat`/`data/font/`; if that is absent
/// the copy embedded in this binary is used instead, so the game can always
/// display its "cannot find a required data file" message. The small menu font
/// only ever comes from the embedded copy in `menu.c`.
unsafe fn load_font() {
    // Try to load font from a file.
    let dathandle = open_dat(cs!("font"), 1);
    hc_font.chtab = load_sprites_from_file(1000, 1 << 1, 0);
    close_dat(dathandle);
    if hc_font.chtab.is_null() {
        // Use built-in font.
        hc_font = load_font_from_data(core::ptr::addr_of_mut!(hc_font_data) as *mut rawfont_type);
    }
    hc_small_font = load_font_from_data(core::ptr::addr_of_mut!(hc_small_font_data) as *mut rawfont_type);
}

/// Advance width of one character in the current font, inter-character spacing
/// included; 0 for a character the font does not cover.
///
/// The `width != 0` guard means a zero-width glyph (the space character in some
/// fonts) contributes nothing at all, not even the spacing.
// seg009:35C5 get_char_width
unsafe fn get_char_width(character: byte) -> c_int {
    let font = textstate.ptr_font;
    let mut width: c_int = 0;
    if character <= (*font).last_char && character >= (*font).first_char {
        let images = core::ptr::addr_of!((*(*font).chtab).images) as *const *mut image_type;
        let image = *images.add((character - (*font).first_char) as usize);
        if !image.is_null() {
            width += crate::platform::sdl::shared_renderer().surface_size(image).0;
            if width != 0 {
                width += (*font).space_between_chars as c_int;
            }
        }
    }
    width
}

/// Returns how many characters of `text` belong on the next line.
///
/// Accumulates glyph widths until the line would exceed `break_width`, then
/// backs up to the last remembered break opportunity: after a hyphen, or (when
/// the text is left- or centre-aligned) around a space. If the very first word
/// is itself wider than `break_width` there is no break opportunity to fall back
/// to, and the word is broken mid-way instead.
///
/// A `\n` ends the line immediately and is counted as part of it.
// seg009:3E99 find_linebreak
unsafe fn find_linebreak(text: *const c_char, length: c_int, break_width: c_int, x_align: c_int) -> c_int {
    let mut curr_char_pos: c_int = 0;
    let mut last_break_pos: c_short = 0;
    let mut curr_line_width: c_short = 0;
    let mut text_pos = text;
    while curr_char_pos < length {
        curr_line_width = curr_line_width.wrapping_add(get_char_width(*text_pos as byte) as c_short);
        if (curr_line_width as c_int) <= break_width {
            curr_char_pos += 1;
            let curr_char = *text_pos;
            text_pos = text_pos.add(1);
            if curr_char == '\n' as c_char {
                return curr_char_pos;
            }
            if curr_char == '-' as c_char
                || (x_align <= 0 && (curr_char == ' ' as c_char || *text_pos == ' ' as c_char))
                || (*text_pos == ' ' as c_char && curr_char == ' ' as c_char)
            {
                // May break here.
                last_break_pos = curr_char_pos as c_short;
            }
        } else if last_break_pos == 0 {
            // If the first word is wider than break_width then break it.
            return curr_char_pos;
        } else {
            // Otherwise break at the last space.
            return last_break_pos as c_int;
        }
    }
    curr_char_pos
}

/// Total advance width of `length` characters of `text`, in pixels.
// seg009:403F get_line_width
#[no_mangle]
pub unsafe extern "C" fn get_line_width(text: *const c_char, length: c_int) -> c_int {
    let mut width: c_int = 0;
    for i in 0..length {
        width += get_char_width(*text.offset(i as isize) as byte);
    }
    width
}

/// Blits one character at the text cursor and advances the cursor.
///
/// `current_y` is the *baseline*, so the glyph is drawn `height_above_baseline`
/// pixels higher. Characters outside the font's range draw nothing and advance
/// by zero.
// seg009:3706 draw_text_character
#[no_mangle]
pub unsafe extern "C" fn draw_text_character(character: byte) -> c_int {
    let font = textstate.ptr_font;
    let mut width: c_int = 0;
    if character <= (*font).last_char && character >= (*font).first_char {
        let images = core::ptr::addr_of!((*(*font).chtab).images) as *const *mut image_type;
        let image = *images.add((character - (*font).first_char) as usize);
        if !image.is_null() {
            method_3_blit_mono(
                image,
                textstate.current_x as c_int,
                (textstate.current_y as c_int) - ((*font).height_above_baseline as c_int),
                textstate.textblit as c_int,
                textstate.textcolor as byte,
            );
            width = (*font).space_between_chars as c_int + crate::platform::sdl::shared_renderer().surface_size(image).0;
        }
    }
    textstate.current_x = (textstate.current_x as c_int + width) as c_short;
    width
}

/// Draws `length` characters starting at the text cursor; returns the width
/// drawn.
// seg009:377F draw_text_line
unsafe fn draw_text_line(text: *const c_char, length: c_int) -> c_int {
    let mut width: c_int = 0;
    for i in 0..length {
        width += draw_text_character(*text.offset(i as isize) as byte);
    }
    width
}

/// Draws a NUL-terminated string at the text cursor; returns the width drawn.
// seg009:3755 draw_cstring
unsafe fn draw_cstring(string: *const c_char) -> c_int {
    let mut width: c_int = 0;
    let mut text_pos = string;
    while *text_pos != 0 {
        width += draw_text_character(*text_pos as byte);
        text_pos = text_pos.add(1);
    }
    width
}

/// Lays out and draws a block of text inside `rect_ptr`.
///
/// Both alignment arguments use the same three-way sign convention: negative
/// means left/top, zero means centre/middle, positive means right/bottom. The
/// text is first broken into lines that fit the rect's width (see
/// [`find_linebreak`]), then the block as a whole is positioned vertically and
/// each line positioned horizontally.
///
/// Drawing is clipped to the rect, so overflowing text is cut off rather than
/// spilling; more than `MAX_LINES` lines is a fatal error.
// seg009:3F01 draw_text
unsafe fn draw_text(rect_ptr: *const rect_type, x_align: c_int, y_align: c_int, text: *const c_char, length: c_int) -> *const rect_type {
    set_clip_rect(rect_ptr);
    let rect_width: c_short = (*rect_ptr).right - (*rect_ptr).left;
    let l_rect_top: c_short = (*rect_ptr).top;
    let rect_height: c_short = (*rect_ptr).bottom - (*rect_ptr).top;
    let mut num_lines: c_short = 0;
    let mut rem_length = length;
    let mut line_start = text;
    const MAX_LINES: usize = 100;
    let mut line_starts: [*const c_char; MAX_LINES] = [core::ptr::null(); MAX_LINES];
    let mut line_lengths: [c_int; MAX_LINES] = [0; MAX_LINES];
    loop {
        let line_length = find_linebreak(line_start, rem_length, rect_width as c_int, x_align);
        if line_length == 0 {
            break;
        }
        if (num_lines as usize) >= MAX_LINES {
            crate::c_log("draw_text(): Too many lines!\n");
            quit(1);
        }
        line_starts[num_lines as usize] = line_start;
        line_lengths[num_lines as usize] = line_length;
        num_lines += 1;
        line_start = line_start.offset(line_length as isize);
        rem_length -= line_length;
        if rem_length == 0 {
            break;
        }
    }
    let font = textstate.ptr_font;
    let font_line_distance: c_short =
        (*font).height_above_baseline + (*font).height_below_baseline + (*font).space_between_lines;
    // The last line has no trailing inter-line gap, hence the subtraction.
    let text_height = (font_line_distance as c_int) * (num_lines as c_int) - (*font).space_between_lines as c_int;
    let mut text_top = l_rect_top as c_int;
    if y_align >= 0 {
        text_top += if y_align <= 0 {
            // middle. The +1 is for simulating SHR + ADC/SBB.
            (rect_height as c_int + 1) / 2 - (text_height + 1) / 2
        } else {
            // bottom
            rect_height as c_int - text_height
        };
    }
    textstate.current_y = (text_top + (*font).height_above_baseline as c_int) as c_short;
    for i in 0..num_lines as usize {
        let mut line_pos = line_starts[i];
        let mut line_length = line_lengths[i];
        if x_align < 0 && *line_pos == ' ' as c_char && i != 0 && *line_pos.offset(-1) != '\n' as c_char {
            // Skip over space if it's not at the beginning of a line.
            line_pos = line_pos.add(1);
            line_length -= 1;
            if line_length != 0 && *line_pos == ' ' as c_char && *line_pos.offset(-2) == '.' as c_char {
                // Skip over second space after point.
                line_pos = line_pos.add(1);
                line_length -= 1;
            }
        }
        let line_width = get_line_width(line_pos, line_length);
        let mut text_left = (*rect_ptr).left as c_int;
        if x_align >= 0 {
            text_left += if x_align <= 0 {
                rect_width as c_int / 2 - line_width / 2 // center
            } else {
                rect_width as c_int - line_width // right
            };
        }
        textstate.current_x = text_left as c_short;
        draw_text_line(line_pos, line_length);
        textstate.current_y = (textstate.current_y as c_int + font_line_distance as c_int) as c_short;
    }
    reset_clip_rect();
    rect_ptr
}

/// [`draw_text`] for a NUL-terminated string.
// seg009:3E4F show_text
#[no_mangle]
pub unsafe extern "C" fn show_text(rect_ptr: *const rect_type, x_align: c_int, y_align: c_int, text: *const c_char) {
    draw_text(rect_ptr, x_align, y_align, text, strlen(text) as c_int);
}

/// [`show_text`] in a given colour, restoring the previous text colour after.
// seg009:04FF show_text_with_color
#[no_mangle]
pub unsafe extern "C" fn show_text_with_color(rect_ptr: *const rect_type, x_align: c_int, y_align: c_int, text: *const c_char, color: c_int) {
    let saved_textcolor: c_short = textstate.textcolor;
    textstate.textcolor = color as c_short;
    show_text(rect_ptr, x_align, y_align, text);
    textstate.textcolor = saved_textcolor;
}

/// Moves the text cursor. `ypos` is a baseline, not a top edge.
// seg009:3A91 set_curr_pos
#[no_mangle]
pub unsafe extern "C" fn set_curr_pos(xpos: c_int, ypos: c_int) {
    textstate.current_x = xpos as c_short;
    textstate.current_y = ypos as c_short;
}

/// Builds the one reusable dialog box, and saves the screen behind it.
///
/// Named for the copy-protection prompt it was originally for, but it is the
/// template every message box in the game uses -- which is why `showmessage`
/// can be called from anywhere once this has run, and crashes if called before.
// seg009:145A init_copyprot_dialog
#[no_mangle]
pub unsafe extern "C" fn init_copyprot_dialog() {
    copyprot_dialog = make_dialog_info(
        core::ptr::addr_of_mut!(dialog_settings),
        core::ptr::addr_of_mut!(dialog_rect_1),
        core::ptr::addr_of_mut!(dialog_rect_1),
        null_mut(),
    );
    (*copyprot_dialog).peel = read_peel_from_screen(core::ptr::addr_of!((*copyprot_dialog).peel_rect));
}

/// Shows a modal message box and blocks until any key is pressed.
///
/// The two trailing arguments are vestigial in SDLPoP: the original took a
/// key-handler callback, but this implementation always uses
/// [`key_test_quit`], so Ctrl+Q still works from inside the box.
// seg009:0838 showmessage
#[no_mangle]
pub unsafe extern "C" fn showmessage(text: *mut c_char, _arg_4: c_int, _arg_0: *mut c_void) -> c_int {
    let key: word;
    let mut rect: rect_type = core::mem::zeroed();
    if offscreen_surface.is_null() {
        offscreen_surface = make_offscreen_buffer(core::ptr::addr_of!(screen_rect));
    }
    method_1_blit_rect(
        offscreen_surface,
        onscreen_surface_,
        core::ptr::addr_of!((*copyprot_dialog).peel_rect),
        core::ptr::addr_of!((*copyprot_dialog).peel_rect),
        0,
    );
    draw_dialog_frame(copyprot_dialog);
    shrink2_rect(&mut rect, core::ptr::addr_of!((*copyprot_dialog).text_rect), 2, 1);
    show_text_with_color(&rect, halign_center as c_int, valign_middle as c_int, text, colorids_color_15_brightwhite as c_int);
    clear_kbd_buf();
    loop {
        idle();
        let pressed = key_test_quit() as word;
        if pressed != 0 {
            key = pressed;
            break;
        }
    }
    need_full_redraw = 1;
    key as c_int
}

/// Allocates a dialog descriptor from a settings template and a text rect.
///
/// The peel rect (the region that must be saved and restored) is derived from
/// the text rect plus the template's borders and drop shadow.
// seg009:08FB make_dialog_info
#[no_mangle]
pub unsafe extern "C" fn make_dialog_info(settings: *mut dialog_settings_type, _dialog_rect: *mut rect_type, text_rect: *mut rect_type, dialog_peel: *mut peel_type) -> *mut dialog_type {
    let dialog_info = malloc(core::mem::size_of::<dialog_type>()) as *mut dialog_type;
    (*dialog_info).settings = settings;
    (*dialog_info).has_peel = 0;
    (*dialog_info).peel = dialog_peel;
    if !text_rect.is_null() {
        (*dialog_info).text_rect = *text_rect;
    }
    calc_dialog_peel_rect(dialog_info);
    if !text_rect.is_null() {
        read_dialog_peel(dialog_info);
    }
    dialog_info
}

/// Grows the dialog's text rect by its borders and shadow to get the region
/// that must be saved before drawing.
// seg009:0BE7 calc_dialog_peel_rect
#[no_mangle]
pub unsafe extern "C" fn calc_dialog_peel_rect(dialog: *mut dialog_type) {
    let settings = (*dialog).settings;
    (*dialog).peel_rect.left = (*dialog).text_rect.left - (*settings).left_border;
    (*dialog).peel_rect.top = (*dialog).text_rect.top - (*settings).top_border;
    (*dialog).peel_rect.right = (*dialog).text_rect.right + (*settings).right_border + (*settings).shadow_right;
    (*dialog).peel_rect.bottom = (*dialog).text_rect.bottom + (*settings).bottom_border + (*settings).shadow_bottom;
}

/// Saves the screen behind the dialog and draws its frame.
///
/// Note the guard: this is a no-op unless `has_peel` is *already* set, which
/// [`make_dialog_info`] never does -- so the peel is in practice taken by
/// [`init_copyprot_dialog`] directly instead. Faithful to the C source; left
/// alone because whether that is a bug depends on data files we do not control.
// seg009:0BB0 read_dialog_peel
#[no_mangle]
pub unsafe extern "C" fn read_dialog_peel(dialog: *mut dialog_type) {
    if (*dialog).has_peel != 0 {
        if (*dialog).peel.is_null() {
            (*dialog).peel = read_peel_from_screen(core::ptr::addr_of!((*dialog).peel_rect));
        }
        (*dialog).has_peel = 1;
        draw_dialog_frame(dialog);
    }
}

/// Draws the dialog's frame through its settings' frame method.
///
/// Indirect because the settings struct is data: different dialog styles supply
/// different frame renderers. In practice only [`dialog_method_2_frame`] is
/// ever installed.
// seg009:09DE draw_dialog_frame
#[no_mangle]
pub unsafe extern "C" fn draw_dialog_frame(dialog: *mut dialog_type) {
    ((*(*dialog).settings).method_2_frame).unwrap()(dialog);
}

/// Clears the dialog's text area to black.
// seg009:096F add_dialog_rect
#[no_mangle]
pub unsafe extern "C" fn add_dialog_rect(dialog: *mut dialog_type) {
    draw_rect(core::ptr::addr_of!((*dialog).text_rect), colorids_color_0_black as c_int);
}

/// Draws the standard dialog chrome: black backing, a dark-grey drop shadow on
/// the right and bottom, and a white inner bevel on all four sides.
///
/// Each piece is a filled rectangle rather than a line, which is why the four
/// bevel edges overlap at the corners.
// seg009:09F0 dialog_method_2_frame
#[no_mangle]
pub unsafe extern "C" fn dialog_method_2_frame(dialog: *mut dialog_type) {
    let mut rect: rect_type;
    let shadow_right = (*(*dialog).settings).shadow_right;
    let shadow_bottom = (*(*dialog).settings).shadow_bottom;
    let bottom_border = (*(*dialog).settings).bottom_border;
    let outer_border = (*(*dialog).settings).outer_border;
    let peel_top = (*dialog).peel_rect.top;
    let peel_left = (*dialog).peel_rect.left;
    let peel_bottom = (*dialog).peel_rect.bottom;
    let peel_right = (*dialog).peel_rect.right;
    let text_top = (*dialog).text_rect.top;
    let text_left = (*dialog).text_rect.left;
    let text_bottom = (*dialog).text_rect.bottom;
    let text_right = (*dialog).text_rect.right;
    // Draw outer border
    rect = rect_type { top: peel_top, left: peel_left, bottom: peel_bottom - shadow_bottom, right: peel_right - shadow_right };
    draw_rect(&rect, colorids_color_0_black as c_int);
    // Draw shadow (right)
    rect = rect_type { top: text_top, left: peel_right - shadow_right, bottom: peel_bottom, right: peel_right };
    draw_rect(&rect, get_text_color(0, colorids_color_8_darkgray as c_int, 0));
    // Draw shadow (bottom)
    rect = rect_type { top: peel_bottom - shadow_bottom, left: text_left, bottom: peel_bottom, right: peel_right };
    draw_rect(&rect, get_text_color(0, colorids_color_8_darkgray as c_int, 0));
    // Draw inner border (left)
    rect = rect_type { top: peel_top + outer_border, left: peel_left + outer_border, bottom: text_bottom, right: text_left };
    draw_rect(&rect, colorids_color_15_brightwhite as c_int);
    // Draw inner border (top)
    rect = rect_type { top: peel_top + outer_border, left: text_left, bottom: text_top, right: text_right + (*(*dialog).settings).right_border - outer_border };
    draw_rect(&rect, colorids_color_15_brightwhite as c_int);
    // Draw inner border (right)
    rect.top = text_top;
    rect.left = text_right;
    rect.bottom = text_bottom + bottom_border - outer_border;
    draw_rect(&rect, colorids_color_15_brightwhite as c_int);
    // Draw inner border (bottom)
    rect = rect_type { top: text_bottom, left: peel_left + outer_border, bottom: text_bottom + bottom_border - outer_border, right: text_right };
    draw_rect(&rect, colorids_color_15_brightwhite as c_int);
}

/// [`showmessage`] with the standard "Press any key to continue." suffix.
// seg009:0C44 show_dialog
#[no_mangle]
pub unsafe extern "C" fn show_dialog(text: *const c_char) {
    let mut string = [0 as c_char; 256];
    crate::write_c_str_truncating(string.as_mut_ptr(), 256, &format!("{}\n\nPress any key to continue.", cstr(text)));
    showmessage(string.as_mut_ptr(), 1, key_test_quit as *mut c_void);
}

/// Baseline y for a single line of text vertically centred in `rect`.
///
/// `empty_height` is the leftover space above plus below the line. Half of it
/// goes above, and the odd pixel (`% 2`) is pushed to the top as well -- the
/// `(h - h % 2) >> 1` shape is the C source's way of flooring toward zero for
/// negative heights, which `>> 1` alone would not do.
///
/// Computed in `c_int`: C promotes each operand of the subtraction chain to
/// `int` and only the store into the `short` truncates. The result is identical
/// either way, but doing it in `i16` would give Rust an overflow panic in debug
/// builds where C simply wraps.
// seg009:0791 get_text_center_y
unsafe fn get_text_center_y(rect: *const rect_type) -> c_int {
    let font = core::ptr::addr_of!(hc_font);
    // height of empty space above+below the line of text
    let empty_height = ((*rect).bottom as c_int
        - (*font).height_above_baseline as c_int
        - (*font).height_below_baseline as c_int
        - (*rect).top as c_int) as c_short as c_int;
    ((empty_height - empty_height % 2) >> 1)
        + (*font).height_above_baseline as c_int
        + empty_height % 2
        + (*rect).top as c_int
}

/// Total advance width of a NUL-terminated string, in pixels.
// seg009:3E77 get_cstring_width
unsafe fn get_cstring_width(text: *const c_char) -> c_int {
    let mut width: c_int = 0;
    let mut text_pos = text;
    while *text_pos != 0 {
        width += get_char_width(*text_pos as byte);
        text_pos = text_pos.add(1);
    }
    width
}

/// Draws (or, in the background colour, erases) the `_` text-entry caret.
// seg009:0767 draw_text_cursor
unsafe fn draw_text_cursor(xpos: c_int, ypos: c_int, color: c_int) {
    set_curr_pos(xpos, ypos);
    textstate.textcolor = color as c_short;
    draw_text_character('_' as byte);
    textstate.textcolor = 15;
}

/// Runs a modal single-line text field and blocks until Enter or Escape.
///
/// Returns the number of characters entered, or -1 if the player cancelled with
/// Escape (in which case `buffer` is emptied). Used for naming a replay and for
/// the copy-protection prompt.
///
/// Two input streams are read at once and neither alone would do: SDL's
/// `SDL_TEXTINPUT` supplies the printable character (correct for the player's
/// keyboard layout, and the only source that knows about shift and dead keys),
/// while the raw scancode supplies Enter, Escape and Backspace. `arg_4` is the
/// left padding inside the rect.
///
/// The blinking caret is driven by `timer_0` at a 6-tick period; the inner loop
/// spins on [`idle`] so the rest of the game keeps redrawing while the field is
/// up. New characters are rejected outright once the caret plus the character
/// would reach the rect's right edge -- the field does not scroll.
// seg009:053C input_str
#[no_mangle]
pub unsafe extern "C" fn input_str(
    rect: *const rect_type,
    buffer: *mut c_char,
    max_length: c_int,
    initial: *const c_char,
    has_initial: c_int,
    arg_4: c_int,
    color: c_int,
    bgcolor: c_int,
) -> c_int {
    let mut sdlrect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(rect, &mut sdlrect);
    crate::platform::sdl::shared_input().start_text_input(sdlrect.x, sdlrect.y, sdlrect.w, sdlrect.h);

    let mut key: word;
    let mut current_xpos: c_short;
    let mut length: c_short = 0;
    let mut cursor_visible: c_short = 0;
    draw_rect(rect, bgcolor);
    let init_length: c_short = strlen(initial) as c_short;
    if has_initial != 0 {
        strcpy(buffer, initial);
        length = init_length;
    }
    current_xpos = ((*rect).left as c_int + arg_4) as c_short;
    let ypos: c_short = get_text_center_y(rect) as c_short;
    set_curr_pos(current_xpos as c_int, ypos as c_int);
    textstate.textcolor = color as c_short;
    draw_cstring(initial);
    current_xpos = (current_xpos as c_int + get_cstring_width(initial) + (init_length != 0) as c_int * arg_4) as c_short;
    loop {
        key = 0;
        loop {
            draw_text_cursor(
                current_xpos as c_int,
                ypos as c_int,
                if cursor_visible != 0 { color } else { bgcolor },
            );
            cursor_visible = (cursor_visible == 0) as c_short;
            start_timer(timerids_timer_0 as c_int, 6);
            if key != 0 {
                if cursor_visible != 0 {
                    draw_text_cursor(current_xpos as c_int, ypos as c_int, color);
                    cursor_visible = (cursor_visible == 0) as c_short;
                }
                if key as c_int != SDL_SCANCODE_RETURN {
                    break;
                }
                *buffer.offset(length as isize) = 0;
                crate::platform::sdl::shared_input().stop_text_input();
                return length as c_int;
            }
            // Spin until the caret's blink timer expires or a key arrives.
            // The assignment to `key` inside the condition is load-bearing: it
            // is what carries the keystroke out to the enclosing loop.
            while has_timer_stopped(timerids_timer_0 as c_int) == 0 && {
                key = key_test_quit() as word;
                key == 0
            } {
                idle();
            }
        }
        // Only use the printable ASCII chars (UTF-8 encoding)
        let entered_char: c_char = if last_text_input <= 0x7E { last_text_input } else { 0 };
        clear_kbd_buf();

        if key as c_int == SDL_SCANCODE_ESCAPE {
            draw_rect(rect, bgcolor);
            *buffer.offset(0) = 0;
            crate::platform::sdl::shared_input().stop_text_input();
            return -1;
        }
        if length != 0 && (key as c_int == SDL_SCANCODE_BACKSPACE || key as c_int == SDL_SCANCODE_DELETE) {
            length -= 1;
            draw_text_cursor(current_xpos as c_int, ypos as c_int, bgcolor);
            current_xpos = (current_xpos as c_int - get_char_width(*buffer.offset(length as isize) as byte)) as c_short;
            set_curr_pos(current_xpos as c_int, ypos as c_int);
            textstate.textcolor = bgcolor as c_short;
            draw_text_character(*buffer.offset(length as isize) as byte);
            draw_text_cursor(current_xpos as c_int, ypos as c_int, color);
        } else if entered_char >= 0x20 && entered_char <= 0x7E && (length as c_int) < max_length {
            // Would the new character make the cursor go past the right side of the rect?
            if (get_char_width('_' as byte) + get_char_width(entered_char as byte) + current_xpos as c_int)
                < ((*rect).right as c_int)
            {
                draw_text_cursor(current_xpos as c_int, ypos as c_int, bgcolor);
                set_curr_pos(current_xpos as c_int, ypos as c_int);
                textstate.textcolor = color as c_short;
                *buffer.offset(length as isize) = entered_char;
                length += 1;
                current_xpos = (current_xpos as c_int + draw_text_character(entered_char as byte)) as c_short;
            }
        }
    }
}

/// Fills a rect with a palette colour.
// seg009:37E8 draw_rect
#[no_mangle]
pub unsafe extern "C" fn draw_rect(rect: *const rect_type, color: c_int) {
    method_5_rect(rect, blitters_blitters_0_no_transp as c_int, color as byte);
}

/// Identity: the original set a clipping window on the surface; SDL does that
/// per-blit instead.
// seg009:3985 rect_sthg
#[no_mangle]
pub unsafe extern "C" fn rect_sthg(surface: *mut surface_type, _rect: *const rect_type) -> *mut surface_type {
    surface
}

/// Insets a rect by `delta_x` horizontally and `delta_y` vertically.
// seg009:39CE shrink2_rect
#[no_mangle]
pub unsafe extern "C" fn shrink2_rect(target_rect: *mut rect_type, source_rect: *const rect_type, delta_x: c_int, delta_y: c_int) -> *mut rect_type {
    (*target_rect).top = ((*source_rect).top as c_int + delta_y) as c_short;
    (*target_rect).left = ((*source_rect).left as c_int + delta_x) as c_short;
    (*target_rect).bottom = ((*source_rect).bottom as c_int - delta_y) as c_short;
    (*target_rect).right = ((*source_rect).right as c_int - delta_x) as c_short;
    target_rect
}

/// Blits a saved peel back where it came from and frees it.
// seg009:3BBA restore_peel
#[no_mangle]
pub unsafe extern "C" fn restore_peel(peel_ptr: *mut peel_type) {
    method_6_blit_img_to_scr((*peel_ptr).peel, (*peel_ptr).rect.left as c_int, (*peel_ptr).rect.top as c_int, 0);
    free_peel(peel_ptr);
}

/// Copies a region of the current target surface into a freshly allocated peel.
// seg009:3BE9 read_peel_from_screen
#[no_mangle]
pub unsafe extern "C" fn read_peel_from_screen(rect: *const rect_type) -> *mut peel_type {
    let result = calloc(1, core::mem::size_of::<peel_type>()) as *mut peel_type;
    (*result).rect = *rect;
    let peel_surface = crate::platform::sdl::shared_renderer().create_surface(((*rect).right - (*rect).left) as c_int, ((*rect).bottom - (*rect).top) as c_int, 24, Rmsk, Gmsk, Bmsk, 0);
    if peel_surface.is_null() {
        sdlperror(cs!("read_peel_from_screen: SDL_CreateRGBSurface"));
        quit(1);
    }
    (*result).peel = peel_surface;
    // C writes this as a positional initialiser over `short top,left,bottom,right`,
    // so `bottom` receives the *width* and `right` receives the *height* -- they
    // are transposed. Harmless, because the only thing method_1_blit_rect reads
    // out of a destination rect is its top-left corner (SDL_BlitSurface takes
    // the size from the source), and both are 0 here. Reproduced as written.
    let target_rect = rect_type {
        top: 0,
        left: 0,
        bottom: (*rect).right - (*rect).left,
        right: (*rect).bottom - (*rect).top,
    };
    method_1_blit_rect((*result).peel, current_target_surface, &target_rect, rect, 0);
    result
}

/// Intersects two rects into `output`; returns 1 if the result is non-empty.
///
/// On an empty intersection `output` is zeroed. Note the early half-write: if
/// the horizontal ranges overlap but the vertical ones do not, `left`/`right`
/// have already been stored before the zeroing wipes them again.
// seg009:3D95 intersect_rect
#[no_mangle]
pub unsafe extern "C" fn intersect_rect(output: *mut rect_type, input1: *const rect_type, input2: *const rect_type) -> c_int {
    let left = (*input1).left.max((*input2).left);
    let right = (*input1).right.min((*input2).right);
    if left < right {
        (*output).left = left;
        (*output).right = right;
        let top = (*input1).top.max((*input2).top);
        let bottom = (*input1).bottom.min((*input2).bottom);
        if top < bottom {
            (*output).top = top;
            (*output).bottom = bottom;
            return 1;
        }
    }
    memset(output as *mut c_void, 0, core::mem::size_of::<rect_type>());
    0
}

/// Writes the bounding box of two rects into `output`.
// seg009:4063 union_rect
#[no_mangle]
pub unsafe extern "C" fn union_rect(output: *mut rect_type, input1: *const rect_type, input2: *const rect_type) -> *mut rect_type {
    *output = rect_type {
        top: (*input1).top.min((*input2).top),
        left: (*input1).left.min((*input2).left),
        bottom: (*input1).bottom.max((*input2).bottom),
        right: (*input1).right.max((*input2).right),
    };
    output
}

// ============================================================================
// Audio
//
// There is one SDL audio device and one callback. Four sources can feed it, and
// the four `*_playing` flags below say which are live. `audio_callback` mixes at
// most one "sound effect" source (digi or speaker) with at most one "music"
// source (midi or ogg) -- which is why starting a sound effect does not cut the
// music, but starting a second sound effect does.
//
// Each source is stopped by clearing its flag under the audio lock, so a
// callback already in flight on the audio thread cannot observe a half-torn-down
// source. When a source runs out it posts an SDL_USEREVENT so the game loop can
// notice from the main thread rather than being called back on the audio one.
// ============================================================================

/// Silences the PC-speaker source and rewinds it to its first note.
unsafe fn speaker_sound_stop() {
    if speaker_playing == 0 {
        return;
    }
    crate::platform::sdl::shared_audio().lock();
    speaker_playing = 0;
    current_speaker_sound = null_mut();
    speaker_note_index = 0;
    current_speaker_note_samples_already_emitted = 0;
    crate::platform::sdl::shared_audio().unlock();
}

/// Silences the digitised-sample source.
unsafe fn stop_digi() {
    if digi_playing == 0 {
        return;
    }
    crate::platform::sdl::shared_audio().lock();
    digi_playing = 0;
    digi_buffer = null_mut();
    digi_remaining_length = 0;
    digi_remaining_pos = null_mut();
    crate::platform::sdl::shared_audio().unlock();
}

/// Silences the Ogg music source.
///
/// Note the device is paused *before* the `ogg_playing` check, so this also
/// serves as the general "stop the audio device" call that
/// [`stop_sounds`] relies on.
unsafe fn stop_ogg() {
    crate::platform::sdl::shared_audio().pause(true);
    if ogg_playing == 0 {
        return;
    }
    ogg_playing = 0;
    crate::platform::sdl::shared_audio().lock();
    ogg_decoder = null_mut();
    crate::platform::sdl::shared_audio().unlock();
}

/// Silences every audio source.
// seg009:7214 stop_sounds
#[no_mangle]
pub unsafe extern "C" fn stop_sounds() {
    stop_digi();
    stop_midi();
    speaker_sound_stop();
    stop_ogg();
}

/// Synthesises `samples` frames of a square wave at `note_freq` into `stream`.
///
/// State carries across calls through `square_wave_state` (the current level)
/// and `square_wave_samples_since_last_flip` (the fractional phase), because a
/// note routinely straddles several audio-callback buffers.
///
/// `!square_wave_state` is a *bitwise* NOT here, matching C's `~` on a `short`
/// -- the value is an amplitude, not a boolean, so this is the one place in the
/// port where Rust's integer `!` is the right translation of C rather than the
/// classic trap. It flips 4000 to -4001, an off-by-one asymmetry that is
/// inaudible and is what the C source does.
unsafe fn generate_square_wave(mut stream: *mut byte, note_freq: f32, samples: c_int) {
    let channels = (*digi_audiospec).channels as c_int;
    let half_period_in_samples: f32 = ((*digi_audiospec).freq as f32 / note_freq) * 0.5f32;

    let mut samples_left = samples;
    while samples_left > 0 {
        if square_wave_samples_since_last_flip > half_period_in_samples {
            // Produce a square wave by flipping the signal.
            square_wave_state = !square_wave_state;
            square_wave_samples_since_last_flip -= half_period_in_samples;
        } else {
            // Round up, so a sub-sample remainder still emits one sample and
            // the loop cannot stall.
            let samples_until_next_flip =
                (half_period_in_samples - square_wave_samples_since_last_flip) as c_int + 1;
            let samples_to_emit = MIN_i(samples_until_next_flip, samples_left);
            for _ in 0..samples_to_emit * channels {
                *(stream as *mut c_short) = square_wave_state;
                stream = stream.add(core::mem::size_of::<c_short>());
            }
            samples_left -= samples_to_emit;
            square_wave_samples_since_last_flip += samples_to_emit as f32;
        }
    }
}

/// Renders the PC-speaker source: walks the note list, emitting a square wave
/// (or silence, for a rest) for each note's share of the buffer.
///
/// A note's duration is `length` ticks scaled by the tune's `tempo` and the
/// device rate. Frequency `0x12` is the end-of-tune sentinel and frequency
/// `<= 1` is a rest.
unsafe fn speaker_callback(_userdata: *mut c_void, mut stream: *mut u8, len: c_int) {
    let output_channels = (*digi_audiospec).channels as c_int;
    let bytes_per_sample = core::mem::size_of::<c_short>() as c_int * output_channels;
    let samples_requested = len / bytes_per_sample;

    if current_speaker_sound.is_null() {
        return;
    }
    let tempo = swaple16((*current_speaker_sound).tempo);

    let mut total_samples_left = samples_requested;
    while total_samples_left > 0 {
        let notes = core::ptr::addr_of!((*current_speaker_sound).notes) as *const note_type;
        let note = notes.add(speaker_note_index as usize);
        if swaple16((*note).frequency) == 0x12 /* end */ {
            speaker_playing = 0;
            current_speaker_sound = null_mut();
            speaker_note_index = 0;
            let mut event: SDL_Event = core::mem::zeroed();
            event.type_ = SDL_USEREVENT;
            event.user.code = userevent_SOUND;
            crate::platform::sdl::shared_renderer().push_event(&mut event as *mut SDL_Event as *mut c_void);
            return;
        }

        let note_length_in_samples = ((*note).length as c_int * (*digi_audiospec).freq) / tempo as c_int;
        let note_samples_to_emit = MIN_i(note_length_in_samples - current_speaker_note_samples_already_emitted, total_samples_left);
        total_samples_left -= note_samples_to_emit;
        let copy_len = note_samples_to_emit as usize * bytes_per_sample as usize;
        if swaple16((*note).frequency) <= 0x01 /* rest */ {
            memset(stream as *mut c_void, (*digi_audiospec).silence as c_int, copy_len);
        } else {
            generate_square_wave(stream as *mut byte, swaple16((*note).frequency) as f32, note_samples_to_emit);
        }
        stream = stream.add(copy_len);

        let note_samples_emitted = current_speaker_note_samples_already_emitted + note_samples_to_emit;
        if note_samples_emitted < note_length_in_samples {
            current_speaker_note_samples_already_emitted += note_samples_to_emit;
        } else {
            speaker_note_index += 1;
            current_speaker_note_samples_already_emitted = 0;
        }
    }
}

/// Starts a PC-speaker tune, cutting whatever was playing.
// seg009:7640 play_speaker_sound
unsafe fn play_speaker_sound(buffer: *mut sound_buffer_type) {
    speaker_sound_stop();
    stop_sounds();
    current_speaker_sound = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut speaker_type;
    speaker_note_index = 0;
    speaker_playing = 1;
    crate::platform::sdl::shared_audio().pause(false);
}

/// Renders the digitised-sample source: copies from the resampled buffer and
/// pads the tail with silence.
///
/// When sound is off the samples are still *consumed* (the cursor advances) --
/// the sound plays out silently rather than pausing, so timing stays the same
/// whether or not the player has sound enabled.
unsafe fn digi_callback(_userdata: *mut c_void, stream: *mut u8, len: c_int) {
    let copy_len = MIN_i(len, digi_remaining_length);
    if is_sound_on != 0 {
        memcpy(stream as *mut c_void, digi_remaining_pos as *const c_void, copy_len as usize);
        memset(stream.add(copy_len as usize) as *mut c_void, (*digi_audiospec).silence as c_int, (len - copy_len) as usize);
    } else {
        memset(stream as *mut c_void, (*digi_audiospec).silence as c_int, len as usize);
    }
    if digi_playing != 0 && digi_remaining_length == 0 {
        let mut event: SDL_Event = core::mem::zeroed();
        event.type_ = SDL_USEREVENT;
        event.user.code = userevent_SOUND;
        digi_playing = 0;
        crate::platform::sdl::shared_renderer().push_event(&mut event as *mut SDL_Event as *mut c_void);
    }
    digi_remaining_length -= copy_len;
    digi_remaining_pos = digi_remaining_pos.add(copy_len as usize);
}

/// Renders the Ogg music source through the Vorbis decoder.
///
/// With sound off the decoder is still pumped, into a scratch buffer that is
/// thrown away -- same reasoning as [`digi_callback`]: the music must reach its
/// end at the same wall-clock moment either way, since that is what posts the
/// finished-playing event.
unsafe fn ogg_callback(_userdata: *mut c_void, stream: *mut u8, len: c_int) {
    let output_channels = (*digi_audiospec).channels as c_int;
    let bytes_per_sample = core::mem::size_of::<c_short>() as c_int * output_channels;
    let samples_requested = len / bytes_per_sample;

    let samples_filled: c_int;
    if is_sound_on != 0 {
        let out = core::slice::from_raw_parts_mut(stream as *mut c_short, (len / core::mem::size_of::<c_short>() as c_int) as usize);
        samples_filled = (*ogg_decoder).get_samples_interleaved(output_channels as usize, out) as c_int;
        if samples_filled < samples_requested {
            let bytes_filled = samples_filled * bytes_per_sample;
            let remaining_bytes = (samples_requested - samples_filled) * bytes_per_sample;
            memset(stream.add(bytes_filled as usize) as *mut c_void, (*digi_audiospec).silence as c_int, remaining_bytes as usize);
        }
    } else {
        memset(stream as *mut c_void, (*digi_audiospec).silence as c_int, len as usize);
        let discarded_samples = malloc(len as usize) as *mut u8;
        let out = core::slice::from_raw_parts_mut(discarded_samples as *mut c_short, (len / core::mem::size_of::<c_short>() as c_int) as usize);
        samples_filled = (*ogg_decoder).get_samples_interleaved(output_channels as usize, out) as c_int;
        free(discarded_samples as *mut c_void);
    }
    if samples_filled == 0 {
        let mut event: SDL_Event = core::mem::zeroed();
        event.type_ = SDL_USEREVENT;
        event.user.code = userevent_SOUND;
        ogg_playing = 0;
        crate::platform::sdl::shared_renderer().push_event(&mut event as *mut SDL_Event as *mut c_void);
    }
}

/// SDL's audio callback: the single mixing point for all four sources.
///
/// Runs on SDL's audio thread, which is why every source's start/stop takes the
/// audio lock.
///
/// Under fast-forward (`audio_speed > 1`) the sources are asked for `speed`
/// times as much audio into a scratch buffer, of which only the first
/// `len_orig` bytes are handed back. That keeps the *game's* audio clock running
/// at the accelerated rate -- sounds still end when they should relative to
/// gameplay -- at the cost of the pitch being wrong, which the C source's own
/// comment calls a hack.
pub unsafe extern "C" fn audio_callback(userdata: *mut c_void, stream_orig: *mut u8, len_orig: c_int) {
    let fast_forwarding = audio_speed > 1;
    let len = if fast_forwarding { len_orig * audio_speed } else { len_orig };
    let stream = if fast_forwarding { malloc(len as usize) as *mut u8 } else { stream_orig };

    memset(stream as *mut c_void, (*digi_audiospec).silence as c_int, len as usize);
    if digi_playing != 0 {
        digi_callback(userdata, stream, len);
    } else if speaker_playing != 0 {
        speaker_callback(userdata, stream, len);
    }
    if midi_playing != 0 {
        midi_callback(userdata, stream, len);
    } else if ogg_playing != 0 {
        ogg_callback(userdata, stream, len);
    }

    if fast_forwarding {
        // FAST_FORWARD_MUTE and FAST_FORWARD_RESAMPLE_SOUND are off:
        // Hack: use the beginning of the buffer instead of resampling.
        memcpy(stream_orig as *mut c_void, stream as *const c_void, len_orig as usize);
        free(stream as *mut c_void);
    }
}

/// Opens the audio device, once, on first use.
///
/// Every `play_*_sound` calls this, so audio is initialised lazily rather than
/// at startup. If opening fails, `digi_unavailable` latches and all later audio
/// calls become no-ops instead of retrying.
///
/// SDL older than 2.0.4 has a resampling bug that garbles 16-bit output, so on
/// those versions the device is opened as 8-bit instead.
// seg009 init_digi
#[no_mangle]
pub unsafe extern "C" fn init_digi() {
    if digi_unavailable != 0 {
        return;
    }
    if !digi_audiospec.is_null() {
        return;
    }
    let desired_audioformat: u16;
    let (vmajor, vminor, vpatch) = crate::platform::sdl::shared_renderer().linked_sdl_version();
    let version = SDL_version { major: vmajor, minor: vminor, patch: vpatch };
    if version.major <= 2 && version.minor <= 0 && version.patch <= 3 {
        desired_audioformat = AUDIO_U8;
        crate::c_log("Your SDL.dll is older than 2.0.4. Using 8-bit audio format to work around resampling bug.");
    } else {
        desired_audioformat = AUDIO_S16SYS;
    }

    let desired = malloc(core::mem::size_of::<SDL_AudioSpec>()) as *mut SDL_AudioSpec;
    memset(desired as *mut c_void, 0, core::mem::size_of::<SDL_AudioSpec>());
    (*desired).freq = digi_samplerate;
    (*desired).format = desired_audioformat;
    (*desired).channels = 2;
    (*desired).samples = 1024;
    (*desired).callback = Some(audio_callback);
    (*desired).userdata = null_mut();
    if crate::platform::sdl::shared_renderer().open_audio_raw(desired as *mut c_void, null_mut()) != 0 {
        sdlperror(cs!("init_digi: SDL_OpenAudio"));
        digi_unavailable = 1;
        return;
    }
    digi_audiospec = desired;
}

/// Reads `data/music/names.txt`, mapping sound ids to Ogg filenames.
///
/// This is what makes replacement music possible: a sound with a name here is
/// looked for as `music/<name>.ogg` before falling back to the DAT resource.
/// Lines are `index=name`; a malformed line is reported and skipped, which with
/// `fscanf` failing to consume anything can spin until EOF -- faithful to the C
/// source, and harmless for the shipped file.
// seg009 load_sound_names
#[no_mangle]
pub unsafe extern "C" fn load_sound_names() {
    let mut __lf = [0 as c_char; POP_MAX_PATH];
    let names_path = locate_file_(cs!("data/music/names.txt"), __lf.as_mut_ptr(), POP_MAX_PATH as c_int);
    if !sound_names.is_null() {
        return;
    }
    let fp = fopen(names_path, cs!("rt"));
    if fp.is_null() {
        return;
    }
    sound_names = calloc(core::mem::size_of::<*mut c_char>() * max_sound_id as usize, 1) as *mut *mut c_char;
    // Hand-rolled stand-in for C's `fscanf(fp, "%d=%255s\n", &index, name)`.
    // `pending` is the one-character pushback that `scanf` performs internally
    // (and that the trailing whitespace directive relies on).
    let mut pending: c_int = NO_PENDING;
    while feof(fp) == 0 {
        let mut index: c_int = 0;
        let mut name = [0 as c_char; POP_MAX_PATH];
        macro_rules! next_ch {
            () => {{
                if pending != NO_PENDING {
                    let __c = pending;
                    pending = NO_PENDING;
                    __c
                } else {
                    fgetc(fp)
                }
            }};
        }
        // Returns the number of items converted, exactly as fscanf would.
        let mut matched: c_int = 0;
        'scan: {
            // "%d": skip leading whitespace, then optional sign and digits.
            let mut c = next_ch!();
            while c != EOF_CH && is_c_space(c) {
                c = next_ch!();
            }
            if c == EOF_CH {
                break 'scan;
            }
            let negative = c == b'-' as c_int;
            if negative || c == b'+' as c_int {
                c = next_ch!();
            }
            let mut value: c_int = 0;
            let mut digits = 0;
            while c != EOF_CH && (c as u8).is_ascii_digit() {
                value = value.wrapping_mul(10).wrapping_add((c as u8 - b'0') as c_int);
                digits += 1;
                c = next_ch!();
            }
            if digits == 0 {
                if c != EOF_CH {
                    pending = c;
                }
                break 'scan;
            }
            index = if negative { -value } else { value };
            matched = 1;
            // "=": a literal directive, so no whitespace is skipped first.
            if c == EOF_CH {
                break 'scan;
            }
            if c != b'=' as c_int {
                pending = c;
                break 'scan;
            }
            // "%255s": skip leading whitespace, then up to 255 non-whitespace chars.
            let mut c = next_ch!();
            while c != EOF_CH && is_c_space(c) {
                c = next_ch!();
            }
            if c == EOF_CH {
                break 'scan;
            }
            let mut len = 0usize;
            loop {
                if c == EOF_CH {
                    break;
                }
                if is_c_space(c) || len == 255 {
                    pending = c;
                    break;
                }
                name[len] = c as c_char;
                len += 1;
                c = next_ch!();
            }
            name[len] = 0;
            matched = 2;
            // Trailing "\n": a whitespace directive -- consume any run of
            // whitespace and push back the first character that follows.
            let mut c = next_ch!();
            while c != EOF_CH && is_c_space(c) {
                c = next_ch!();
            }
            if c != EOF_CH {
                pending = c;
            }
        }
        if matched != 2 {
            perror(names_path);
            continue;
        }
        if index >= 0 && index < max_sound_id {
            *sound_names.offset(index as isize) = strdup(name.as_ptr());
        }
    }
    fclose(fp);
}

/// The Ogg filename registered for a sound id, or null.
unsafe fn sound_name(index: c_int) -> *mut c_char {
    if !sound_names.is_null() && index >= 0 && index < max_sound_id {
        *sound_names.offset(index as isize)
    } else {
        null_mut()
    }
}

/// Loads one sound by id, preferring a replacement Ogg over the DAT resource.
///
/// The Ogg is read into memory whole but *not* decoded -- decoding every track
/// up front would make loading far slower, so only a decoder is constructed and
/// the audio callback pulls chunks from it during playback. Digitised sounds
/// from the DAT, by contrast, are resampled eagerly by [`convert_digi_sound`],
/// because they are short and must start instantly.
// seg009 load_sound
#[no_mangle]
pub unsafe extern "C" fn load_sound(index: c_int) -> *mut sound_buffer_type {
    let mut result: *mut sound_buffer_type = null_mut();
    init_digi();
    if enable_music != 0 && digi_unavailable == 0 && result.is_null() && index >= 0 && index < max_sound_id {
        if !sound_names.is_null() && !sound_name(index).is_null() {
            'do_once: {
                let mut fp: *mut FILE = null_mut();
                let mut filename = [0 as c_char; POP_MAX_PATH];
                if !skip_mod_data_files {
                    snprintf_check!(filename.as_mut_ptr(), POP_MAX_PATH, "{}/music/{}.ogg", cstr(mod_data_path.as_ptr()), cstr(sound_name(index)));
                    fp = fopen(filename.as_ptr(), cs!("rb"));
                }
                if fp.is_null() && !skip_normal_data_files {
                    snprintf_check!(filename.as_mut_ptr(), POP_MAX_PATH, "data/music/{}.ogg", cstr(sound_name(index)));
                    let mut __lf = [0 as c_char; POP_MAX_PATH];
                    fp = fopen(locate_file_(filename.as_ptr(), __lf.as_mut_ptr(), POP_MAX_PATH as c_int), cs!("rb"));
                }
                if fp.is_null() {
                    break 'do_once;
                }
                let mut info: stat_t = core::mem::zeroed();
                if fstat(fileno(fp), &mut info) != 0 {
                    break 'do_once;
                }
                let file_size: usize = if info.st_size > 0 { info.st_size as usize } else { 0 };
                let file_contents = malloc(file_size) as *mut byte;
                if fread(file_contents as *mut c_void, 1, file_size, fp) != file_size {
                    free(file_contents as *mut c_void);
                    fclose(fp);
                    break 'do_once;
                }
                fclose(fp);
                let file_bytes = core::slice::from_raw_parts(file_contents, file_size).to_vec();
                let decoder_box = crate::ogg_decode::OggDecoder::open(file_bytes);
                let decoder = match decoder_box {
                    Some(b) => Box::into_raw(b),
                    None => {
                        crate::c_log(&format!("Error creating decoder from file \"{}\"!\n", cstr(filename.as_ptr())));
                        free(file_contents as *mut c_void);
                        break 'do_once;
                    }
                };
                result = malloc(core::mem::size_of::<sound_buffer_type>()) as *mut sound_buffer_type;
                (*result).type_ = sound_type_sound_ogg as byte;
                let ogg = core::ptr::addr_of_mut!((*result).__bindgen_anon_1) as *mut ogg_type;
                (*ogg).total_length = ((*decoder).total_length_samples() as usize * core::mem::size_of::<c_short>()) as c_int;
                (*ogg).file_contents = file_contents;
                (*ogg).decoder = decoder as *mut stb_vorbis;
            }
        }
    }
    if result.is_null() {
        result = load_from_opendats_alloc(index + 10000, cs!("bin"), null_mut(), null_mut()) as *mut sound_buffer_type;
    }
    if !result.is_null() && ((*result).type_ & 7) == sound_type_sound_digi as byte {
        let converted = convert_digi_sound(result);
        free(result as *mut c_void);
        result = converted;
    }
    if result.is_null() && !skip_normal_data_files {
        crate::c_log_err(&format!("Failed to load sound {} '{}'\n", index, cstr(sound_name(index))));
    }
    result
}

/// Starts an Ogg track from the beginning, cutting whatever was playing.
unsafe fn play_ogg_sound(buffer: *mut sound_buffer_type) {
    init_digi();
    if digi_unavailable != 0 {
        return;
    }
    stop_sounds();
    let ogg = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut ogg_type;
    let decoder = (*ogg).decoder as *mut crate::ogg_decode::OggDecoder;
    // Need to rewind the music, or else the decoder might continue where it left off.
    (*decoder).seek_start();
    crate::platform::sdl::shared_audio().lock();
    ogg_decoder = decoder;
    crate::platform::sdl::shared_audio().unlock();
    crate::platform::sdl::shared_audio().pause(false);
    ogg_playing = 1;
}

#[repr(C)]
struct waveinfo_type {
    sample_rate: c_int,
    sample_size: c_int,
    sample_count: c_int,
    samples: *mut byte,
}

/// Works out which of two incompatible digitised-sound layouts a buffer uses,
/// and fills in `waveinfo` accordingly.
///
/// PoP 1.0/1.1 and PoP 1.3/1.4 put the sample-rate, size and count fields at
/// different offsets, and nothing in the data says which. The heuristic reads
/// the byte that would be `sample_size` under each layout and takes the one that
/// says 8 bits: exactly one match identifies the version, both matching is
/// ambiguous, neither is unrecognisable.
///
/// The answer is cached in `wave_version` after the first *unambiguous* result,
/// because a whole data set is always one version -- but an ambiguous or unknown
/// buffer deliberately does not poison the cache.
unsafe fn determine_wave_version(buffer: *mut sound_buffer_type, waveinfo: *mut waveinfo_type) -> bool {
    let mut version = wave_version;
    if version == -1 {
        // Determine the version of the wave data.
        version = 0;
        let digi = core::ptr::addr_of!((*buffer).__bindgen_anon_1) as *const digi_type;
        let digi_new = core::ptr::addr_of!((*buffer).__bindgen_anon_1) as *const digi_new_type;
        if (*digi).sample_size == 8 {
            version += 1;
        }
        if (*digi_new).sample_size == 8 {
            version += 2;
        }
        if version == 1 || version == 2 {
            wave_version = version;
        }
    }
    match version {
        // 1.0 and 1.1
        1 => {
            let digi = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut digi_type;
            (*waveinfo).sample_rate = swaple16((*digi).sample_rate) as c_int;
            (*waveinfo).sample_size = (*digi).sample_size as c_int;
            (*waveinfo).sample_count = swaple16((*digi).sample_count) as c_int;
            (*waveinfo).samples = core::ptr::addr_of_mut!((*digi).samples) as *mut byte;
            true
        }
        // 1.3 and 1.4 (and PoP2)
        2 => {
            let digi_new = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut digi_new_type;
            (*waveinfo).sample_rate = swaple16((*digi_new).sample_rate) as c_int;
            (*waveinfo).sample_size = (*digi_new).sample_size as c_int;
            (*waveinfo).sample_count = swaple16((*digi_new).sample_count) as c_int;
            (*waveinfo).samples = core::ptr::addr_of_mut!((*digi_new).samples) as *mut byte;
            true
        }
        // ambiguous
        3 => {
            crate::c_log("Warning: Ambiguous wave version.\n");
            false
        }
        // case 0, unknown
        _ => {
            crate::c_log("Warning: Can't determine wave version.\n");
            false
        }
    }
}

/// Resamples an 8-bit unsigned mono DAT sound to the device's rate, in 16-bit
/// signed stereo.
///
/// Linear interpolation between adjacent source samples, with the final sample
/// held rather than interpolated past the end. The source byte is widened by
/// `b | (b << 8)` -- replicating the byte into both halves, so 0x00 maps to 0
/// and 0xFF maps to 0xFFFF, giving full-scale coverage that a plain `<< 8` would
/// not -- then biased by -32768 from unsigned to signed.
///
/// The output buffer is over-allocated: it is sized in *shorts* from a length
/// already counted in *bytes*, so it is twice as large as needed. That waste is
/// in the C source and is left alone. Note also that `converted.samples` points
/// at a separate allocation, not into the trailing space of `converted_buffer`,
/// so [`free_sound`] on a converted sound leaks it -- again as in C.
unsafe fn convert_digi_sound(buf: *mut sound_buffer_type) -> *mut sound_buffer_type {
    init_digi();
    if digi_unavailable != 0 {
        return null_mut();
    }
    let mut waveinfo: waveinfo_type = core::mem::zeroed();
    if !determine_wave_version(buf, &mut waveinfo) {
        return null_mut();
    }

    let freq_ratio: f32 = waveinfo.sample_rate as f32 / (*digi_audiospec).freq as f32;

    let source_length = waveinfo.sample_count;
    let expanded_frames = source_length * (*digi_audiospec).freq / waveinfo.sample_rate;
    let expanded_length = expanded_frames * 2 * core::mem::size_of::<c_short>() as c_int;
    let converted_buffer = malloc(core::mem::size_of::<sound_buffer_type>() + expanded_length as usize) as *mut sound_buffer_type;

    (*converted_buffer).type_ = sound_type_sound_digi_converted as byte;
    let converted = core::ptr::addr_of_mut!((*converted_buffer).__bindgen_anon_1) as *mut converted_audio_type;
    (*converted).length = expanded_length;

    let source = waveinfo.samples;
    let mut dest = malloc(core::mem::size_of::<c_short>() * (*converted).length as usize) as *mut c_short;
    (*converted).samples = dest;

    // Widens one unsigned 8-bit source sample to signed 16-bit.
    let sample_at = |frame: c_int| -> c_int {
        let b = *source.offset(frame as isize) as c_int;
        (b | (b << 8)) - 32768
    };

    for i in 0..expanded_frames {
        let src_frame_float: f32 = i as f32 * freq_ratio;
        let src_frame_0 = src_frame_float as c_int; // truncation

        let sample_0 = sample_at(src_frame_0);
        let interpolated_sample: c_short = if src_frame_0 >= waveinfo.sample_count - 1 {
            sample_0 as c_short
        } else {
            let alpha: f32 = src_frame_float - src_frame_0 as f32;
            let sample_1 = sample_at(src_frame_0 + 1);
            ((1.0f32 - alpha) * sample_0 as f32 + alpha * sample_1 as f32) as c_short
        };
        for _ in 0..(*digi_audiospec).channels as c_int {
            *dest = interpolated_sample;
            dest = dest.add(1);
        }
    }

    converted_buffer
}

/// Starts a digitised sound. The buffer must already have been through
/// [`convert_digi_sound`].
// seg009:74F0 play_digi_sound
unsafe fn play_digi_sound(buffer: *mut sound_buffer_type) {
    init_digi();
    if digi_unavailable != 0 {
        return;
    }
    stop_digi();
    if ((*buffer).type_ & 7) != sound_type_sound_digi_converted as byte {
        crate::c_log("Tried to play unconverted digi sound.\n");
        return;
    }
    let converted = core::ptr::addr_of!((*buffer).__bindgen_anon_1) as *const converted_audio_type;
    crate::platform::sdl::shared_audio().lock();
    digi_buffer = (*converted).samples as *mut byte;
    digi_playing = 1;
    digi_remaining_length = (*converted).length;
    digi_remaining_pos = digi_buffer;
    crate::platform::sdl::shared_audio().unlock();
    crate::platform::sdl::shared_audio().pause(false);
}

/// Frees a loaded sound, including an Ogg's decoder and file bytes.
// seg009 free_sound
#[no_mangle]
pub unsafe extern "C" fn free_sound(buffer: *mut sound_buffer_type) {
    if buffer.is_null() {
        return;
    }
    if (*buffer).type_ == sound_type_sound_ogg as byte {
        let ogg = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut ogg_type;
        drop(Box::from_raw((*ogg).decoder as *mut crate::ogg_decode::OggDecoder));
        free((*ogg).file_contents as *mut c_void);
    }
    free(buffer as *mut c_void);
}

/// Plays a loaded sound, dispatching on the type tag in its low three bits.
///
/// Silently does nothing while fast-forwarding through a replay, which is what
/// keeps a skipped replay from firing hundreds of overlapping sounds.
// seg009:7220 play_sound_from_buffer
#[no_mangle]
pub unsafe extern "C" fn play_sound_from_buffer(buffer: *mut sound_buffer_type) {
    if replaying != 0 && skipping_replay != 0 {
        return;
    }
    if buffer.is_null() {
        crate::c_log("Tried to play NULL sound.\n");
        return;
    }
    match ((*buffer).type_ & 7) as u32 {
        sound_type_sound_speaker => {
            play_speaker_sound(buffer);
        }
        sound_type_sound_digi_converted | sound_type_sound_digi => {
            play_digi_sound(buffer);
        }
        sound_type_sound_midi => {
            play_midi_sound(buffer);
        }
        sound_type_sound_ogg => {
            play_ogg_sound(buffer);
        }
        _ => {
            crate::c_log(&format!("Tried to play unimplemented sound type {}.\n", (*buffer).type_ as c_int));
            quit(1);
        }
    }
}

/// Toggles music. Re-applies the sound setting, since the two share a device.
// seg009 turn_music_on_off
#[no_mangle]
pub unsafe extern "C" fn turn_music_on_off(new_state: byte) {
    enable_music = new_state;
    turn_sound_on_off(is_sound_on);
}

/// Toggles sound. Playback keeps running; the callbacks emit silence instead.
// seg009:7273 turn_sound_on_off
#[no_mangle]
pub unsafe extern "C" fn turn_sound_on_off(new_state: byte) {
    is_sound_on = new_state;
}

/// True if any of the four sources is still playing.
// seg009:7299 check_sound_playing
#[no_mangle]
pub unsafe extern "C" fn check_sound_playing() -> c_int {
    (speaker_playing != 0 || digi_playing != 0 || midi_playing != 0 || ogg_playing != 0) as c_int
}

// ============================================================================
// Palette
//
// The game keeps its own 256-entry palette in 6-bit VGA components, and only
// converts to 8-bit SDL colours at blit time. That indirection is what lets the
// fades and the flash effect work by rewriting the palette rather than by
// touching pixels.
// ============================================================================

/// Writes `count` palette entries starting at `start`; a null `array` writes
/// black (which is how the fade-in blanks a row).
// seg009:9289 set_pal_arr
#[no_mangle]
pub unsafe extern "C" fn set_pal_arr(start: c_int, count: c_int, array: *const rgb_type) {
    for i in 0..count {
        if array.is_null() {
            set_pal(start + i, 0, 0, 0);
        } else {
            let p = array.offset(i as isize);
            set_pal(start + i, (*p).r as c_int, (*p).g as c_int, (*p).b as c_int);
        }
    }
}

/// Writes one palette entry. Components are 6-bit VGA values (0..=63).
// seg009:92DF set_pal
#[no_mangle]
pub unsafe extern "C" fn set_pal(index: c_int, red: c_int, green: c_int, blue: c_int) {
    palette[index as usize] = rgb_type { r: red as byte, g: green as byte, b: blue as byte };
}

/// Stub: the original allocated free palette rows for a new sprite sheet.
///
/// Always returning 0 means "no rows available", so the one caller
/// ([`load_sprites_from_file`]) has its auto-allocation path commented out
/// rather than quitting.
// seg009:969C add_palette_bits
#[no_mangle]
pub unsafe extern "C" fn add_palette_bits(_n_colors: byte) -> c_int {
    0
}

/// Index of the lowest set bit of a 16-bit palette-row mask; 0 if none is set.
// seg009:9C36 find_first_pal_row
#[no_mangle]
pub unsafe extern "C" fn find_first_pal_row(which_rows_mask: c_int) -> c_int {
    for which_row in 0..16 {
        if (1 << which_row) & which_rows_mask != 0 {
            return which_row;
        }
    }
    0
}

/// Maps a logical colour to a palette index for the active graphics mode.
///
/// In VGA mode a colour is a row (the high nibble, selected by
/// `high_half_mask`) plus a column (`low_half`); CGA and Hercules ignore both
/// and take `cga_color` directly.
// seg009:9C6C get_text_color
#[no_mangle]
pub unsafe extern "C" fn get_text_color(cga_color: c_int, low_half: c_int, high_half_mask: c_int) -> c_int {
    if graphics_mode as c_int == grmodes_gmCga as c_int || graphics_mode as c_int == grmodes_gmHgaHerc as c_int {
        cga_color
    } else if graphics_mode as c_int == grmodes_gmMcgaVga as c_int && high_half_mask != 0 {
        (find_first_pal_row(high_half_mask) << 4) + low_half
    } else {
        low_half
    }
}

/// Finds a resource and leaves a `FILE*` positioned at its first data byte.
///
/// This is the single place that decides where a resource comes from. It walks
/// the open-DAT chain from most recently opened to least, and for each entry
/// tries two sources in order:
///
/// 1. The DAT's own resource table, if the DAT actually opened.
/// 2. A loose file `data/<datname>/res<id>.<ext>` -- and under a mod, the same
///    path inside `mods/<MODNAME>/` first.
///
/// The first hit wins, and `result` reports which kind it was, because the two
/// need different handling downstream: a DAT resource shares the DAT's file
/// handle and must not be closed, whereas a directory resource owns its handle
/// and the caller must close it.
///
/// A DAT entry for a `png` of two bytes or fewer is treated as *absent* rather
/// than as an empty image, specifically so a mod can blank out a base-game
/// sprite in its DAT and have the directory fallback take over.
unsafe fn load_from_opendats_metadata(
    resource_id: c_int,
    extension: *const c_char,
    out_fp: *mut *mut FILE,
    result: *mut data_location,
    checksum: *mut byte,
    size: *mut c_int,
    out_pointer: *mut *mut dat_type,
) {
    let mut image_filename = [0 as c_char; POP_MAX_PATH];
    let mut fp: *mut FILE = null_mut();
    *result = data_location_data_none;
    // Go through all open DAT files.
    let mut pointer = dat_chain_ptr;
    while fp.is_null() && !pointer.is_null() {
        *out_pointer = pointer;
        if !(*pointer).handle.is_null() {
            // If it's an actual DAT file:
            fp = (*pointer).handle;
            let dat_table = (*pointer).dat_table;
            let entries = core::ptr::addr_of!((*dat_table).entries) as *const dat_res_type;
            let res_count = swaple16((*dat_table).res_count) as c_int;
            let found = (0..res_count)
                .find(|&i| swaple16((*entries.offset(i as isize)).id) as c_int == resource_id);
            match found {
                Some(i) => {
                    let entry = entries.offset(i as isize);
                    *result = data_location_data_DAT;
                    *size = swaple16((*entry).size) as c_int;
                    if strcmp(extension, cs!("png")) == 0 && *size <= 2 {
                        // Skip empty images in DATs, so we can fall back to directories.
                        fp = null_mut();
                        *result = data_location_data_none;
                        *size = 0;
                    } else if fseek(fp, swaple32((*entry).offset) as c_long, SEEK_SET) != 0
                        || fread(checksum as *mut c_void, 1, 1, fp) != 1
                    {
                        crate::c_log("Cannot seek or cannot read checksum: ");
                        perror(core::ptr::addr_of!((*pointer).filename) as *const c_char);
                        fp = null_mut();
                    }
                }
                // not found
                None => fp = null_mut(),
            }
        }
        // If the image is not in the DAT then try the directory as well.
        if *result == data_location_data_none {
            let mut filename_no_ext = [0 as c_char; POP_MAX_PATH];
            strncpy(filename_no_ext.as_mut_ptr(), core::ptr::addr_of!((*pointer).filename) as *const c_char, POP_MAX_PATH);
            let len = strlen(filename_no_ext.as_ptr());
            if len >= 5 && filename_no_ext[len - 4] == '.' as c_char {
                filename_no_ext[len - 4] = 0;
            }
            snprintf_check!(image_filename.as_mut_ptr(), POP_MAX_PATH, "data/{}/res{}.{}", cstr(filename_no_ext.as_ptr()), resource_id, cstr(extension));
            // Opens a path after running it through the search-directory list.
            let open_located = |path: *const c_char| -> *mut FILE {
                let mut located = [0 as c_char; POP_MAX_PATH];
                fopen(locate_file_(path, located.as_mut_ptr(), POP_MAX_PATH as c_int), cs!("rb"))
            };
            if use_custom_levelset == 0 {
                fp = open_located(image_filename.as_ptr());
            } else {
                // before checking the root directory, first try mods/MODNAME/
                if !skip_mod_data_files {
                    let mut image_filename_mod = [0 as c_char; POP_MAX_PATH];
                    snprintf_check!(image_filename_mod.as_mut_ptr(), POP_MAX_PATH, "{}/{}", cstr(mod_data_path.as_ptr()), cstr(image_filename.as_ptr()));
                    fp = open_located(image_filename_mod.as_ptr());
                }
                if fp.is_null() && !skip_normal_data_files {
                    fp = open_located(image_filename.as_ptr());
                }
            }
            if !fp.is_null() {
                let mut buf: stat_t = core::mem::zeroed();
                if fstat(fileno(fp), &mut buf) == 0 {
                    *result = data_location_data_directory;
                    *size = buf.st_size as c_int;
                } else {
                    crate::c_log("Cannot fstat: ");
                    perror(image_filename.as_ptr());
                    fclose(fp);
                    fp = null_mut();
                }
            }
        }
        pointer = (*pointer).next_dat;
    }
    *out_fp = fp;
    if fp.is_null() {
        *result = data_location_data_none;
    }
}

/// Unlinks a DAT from the open-DAT chain and frees it.
///
/// Silently does nothing if `pointer` is not in the chain.
// seg009:9F34 close_dat
#[no_mangle]
pub unsafe extern "C" fn close_dat(pointer: *mut dat_type) {
    let mut prev: *mut *mut dat_type = core::ptr::addr_of_mut!(dat_chain_ptr);
    let mut curr = dat_chain_ptr;
    while !curr.is_null() {
        if curr == pointer {
            *prev = (*curr).next_dat;
            if !(*curr).handle.is_null() {
                fclose((*curr).handle);
            }
            if !(*curr).dat_table.is_null() {
                free((*curr).dat_table as *mut c_void);
            }
            free(curr as *mut c_void);
            return;
        }
        curr = (*curr).next_dat;
        prev = core::ptr::addr_of_mut!((**prev).next_dat);
    }
}

/// Loads a resource into a freshly allocated buffer the caller must free.
///
/// Returns null if the resource does not exist or could not be read; the
/// optional out-params report where it was found and how big it is.
// seg009:9F80 load_from_opendats_alloc
#[no_mangle]
pub unsafe extern "C" fn load_from_opendats_alloc(resource: c_int, extension: *const c_char, out_result: *mut data_location, out_size: *mut c_int) -> *mut c_void {
    let mut pointer: *mut dat_type = null_mut();
    let mut result: data_location = 0;
    let mut checksum: byte = 0;
    let mut size: c_int = 0;
    let mut fp: *mut FILE = null_mut();
    load_from_opendats_metadata(resource, extension, &mut fp, &mut result, &mut checksum, &mut size, &mut pointer);
    if !out_result.is_null() {
        *out_result = result;
    }
    if !out_size.is_null() {
        *out_size = size;
    }
    if result == data_location_data_none {
        return null_mut();
    }
    let mut area = malloc(size as usize);
    if fread(area, size as usize, 1, fp) != 1 {
        crate::c_log_err(&format!("{}: {}, resource {}, size {}, failed: {}\n", "load_from_opendats_alloc", cstr(core::ptr::addr_of!((*pointer).filename) as *const c_char), resource, size, cstr(strerror(errno()))));
        free(area);
        area = null_mut();
    }
    if result == data_location_data_directory {
        fclose(fp);
    }
    area
}

/// Loads a resource into a caller-supplied buffer, truncating to `length`.
///
/// On a read failure the buffer is zeroed rather than left with stale bytes.
/// The return value is always 0; callers do not check it.
// seg009:A172 load_from_opendats_to_area
#[no_mangle]
pub unsafe extern "C" fn load_from_opendats_to_area(resource: c_int, area: *mut c_void, length: c_int, extension: *const c_char) -> c_int {
    let mut pointer: *mut dat_type = null_mut();
    let mut result: data_location = 0;
    let mut checksum: byte = 0;
    let mut size: c_int = 0;
    let mut fp: *mut FILE = null_mut();
    load_from_opendats_metadata(resource, extension, &mut fp, &mut result, &mut checksum, &mut size, &mut pointer);
    if result == data_location_data_none {
        return 0;
    }
    if fread(area, MIN_i(size, length) as usize, 1, fp) != 1 {
        crate::c_log_err(&format!("{}: {}, resource {}, size {}, failed: {}\n", "load_from_opendats_to_area", cstr(core::ptr::addr_of!((*pointer).filename) as *const c_char), resource, size, cstr(strerror(errno()))));
        memset(area, 0, MIN_i(size, length) as usize);
    }
    if result == data_location_data_directory {
        fclose(fp);
    }
    0
}

// ============================================================================
// Blitting
//
// The `method_N_*` names are inherited from the original's jump table of
// drawing primitives. Each takes a `blitters` id saying how transparency
// should be handled; only a few of those ids are still meaningful.
// ============================================================================

/// A palette entry as 8-bit RGB, ready for `SDL_MapRGB`.
///
/// The stored components are 6-bit VGA values, so each is shifted up by two.
#[inline]
unsafe fn palette_rgb8(color: byte) -> (u8, u8, u8) {
    let entry = palette[color as usize];
    (
        ((entry.r as c_int) << 2) as u8,
        ((entry.g as c_int) << 2) as u8,
        ((entry.b as c_int) << 2) as u8,
    )
}

/// Converts the game's `(top, left, bottom, right)` rect to SDL's
/// `(x, y, w, h)`.
// seg009 rect_to_sdlrect
#[no_mangle]
pub unsafe extern "C" fn rect_to_sdlrect(rect: *const rect_type, sdlrect: *mut SDL_Rect) {
    (*sdlrect).x = (*rect).left as c_int;
    (*sdlrect).y = (*rect).top as c_int;
    (*sdlrect).w = ((*rect).right - (*rect).left) as c_int;
    (*sdlrect).h = ((*rect).bottom - (*rect).top) as c_int;
}

/// Blits a rectangle between two surfaces.
///
/// Only the destination rect's *position* matters: SDL takes the size from the
/// source rect.
// seg009 method_1_blit_rect
#[no_mangle]
pub unsafe extern "C" fn method_1_blit_rect(target_surface: *mut surface_type, source_surface: *mut surface_type, target_rect: *const rect_type, source_rect: *const rect_type, blit: c_int) {
    let mut src_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(source_rect, &mut src_rect);
    let mut dest_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(target_rect, &mut dest_rect);

    let transparent = blit != blitters_blitters_0_no_transp as c_int;
    if crate::platform::sdl::shared_renderer().set_color_key(source_surface, transparent, 0) != 0 {
        sdlperror(cs!("method_1_blit_rect: SDL_SetColorKey"));
        quit(1);
    }
    if SDL_BlitSurface(source_surface, &src_rect, target_surface, &mut dest_rect) != 0 {
        sdlperror(cs!("method_1_blit_rect: SDL_BlitSurface"));
        quit(1);
    }
}

/// Blits an image recoloured to a single palette colour, preserving its alpha.
///
/// This is how text is drawn: the glyph surfaces are monochrome, and every
/// pixel's RGB is overwritten with the requested colour while the alpha channel
/// (which came from the colour key) decides what actually shows. Converting to
/// ARGB8888 first is what makes that separation available.
// seg009 method_3_blit_mono
#[no_mangle]
pub unsafe extern "C" fn method_3_blit_mono(image: *mut image_type, xpos: c_int, ypos: c_int, _blitter: c_int, color: byte) -> *mut image_type {
    let (w, h) = crate::platform::sdl::shared_renderer().surface_size(image);
    if crate::platform::sdl::shared_renderer().set_color_key(image, true, 0) != 0 {
        sdlperror(cs!("method_3_blit_mono: SDL_SetColorKey"));
        quit(1);
    }
    let colored_image = crate::platform::sdl::shared_renderer().convert_surface_format(image, SDL_PIXELFORMAT_ARGB8888, 0);

    crate::platform::sdl::shared_renderer().set_blend_mode(colored_image, SDL_BLENDMODE_NONE);

    if crate::platform::sdl::shared_renderer().lock_surface(colored_image) != 0 {
        sdlperror(cs!("method_3_blit_mono: SDL_LockSurface"));
        quit(1);
    }

    let (pr, pg, pb) = palette_rgb8(color);
    let rgb_color: u32 = crate::platform::sdl::shared_renderer().map_rgb(crate::platform::sdl::shared_renderer().surface_format_ptr(colored_image), pr, pg, pb) & 0xFFFFFF;
    let stride = crate::platform::sdl::shared_renderer().surface_pitch(colored_image);
    let colored_pixels = crate::platform::sdl::shared_renderer().surface_pixels(colored_image) as *mut byte;
    for y in 0..h {
        let mut pixel_ptr = colored_pixels.offset((stride * y) as isize) as *mut u32;
        for _x in 0..w {
            *pixel_ptr = (*pixel_ptr & 0xFF000000) | rgb_color;
            pixel_ptr = pixel_ptr.add(1);
        }
    }
    crate::platform::sdl::shared_renderer().unlock_surface(colored_image);

    let (image_w, image_h) = crate::platform::sdl::shared_renderer().surface_size(image);
    let src_rect = SDL_Rect { x: 0, y: 0, w: image_w, h: image_h };
    let mut dest_rect = SDL_Rect { x: xpos, y: ypos, w: image_w, h: image_h };

    crate::platform::sdl::shared_renderer().set_blend_mode(colored_image, SDL_BLENDMODE_BLEND);
    crate::platform::sdl::shared_renderer().set_blend_mode(current_target_surface, SDL_BLENDMODE_BLEND);
    crate::platform::sdl::shared_renderer().set_alpha_mod(colored_image, 255);
    if SDL_BlitSurface(colored_image, &src_rect, current_target_surface, &mut dest_rect) != 0 {
        sdlperror(cs!("method_3_blit_mono: SDL_BlitSurface"));
        quit(1);
    }
    crate::platform::sdl::shared_renderer().free_surface(colored_image);

    image
}

/// Detects, once, whether this SDL build byte-swaps 24-bit `SDL_FillRect`
/// colours.
///
/// Some SDL versions get the channel order wrong for 24-bit surfaces. The probe
/// fills a 1x1 surface with pure red and checks whether the red mask actually
/// came back set. Result is cached in `RGB24_bug_affected`.
unsafe fn RGB24_bug_check() -> bool {
    if !RGB24_bug_checked {
        let test_surface = crate::platform::sdl::shared_renderer().create_surface(1, 1, 24, 0, 0, 0, 0);
        if test_surface.is_null() {
            sdlperror(cs!("SDL_CreateSurface in RGB24_bug_check"));
        }
        crate::platform::sdl::shared_renderer().fill_rect(test_surface, core::ptr::null(), crate::platform::sdl::shared_renderer().map_rgb(crate::platform::sdl::shared_renderer().surface_format_ptr(test_surface), 0xFF, 0, 0));
        if crate::platform::sdl::shared_renderer().lock_surface(test_surface) != 0 {
            sdlperror(cs!("SDL_LockSurface in RGB24_bug_check"));
        }
        let test_pixels = crate::platform::sdl::shared_renderer().surface_pixels(test_surface);
        let test_rmask = crate::platform::sdl::shared_renderer().surface_format_info(test_surface).rmask;
        RGB24_bug_affected = (*(test_pixels as *const u32) & test_rmask) == 0;
        crate::platform::sdl::shared_renderer().unlock_surface(test_surface);
        crate::platform::sdl::shared_renderer().free_surface(test_surface);
        RGB24_bug_checked = true;
    }
    RGB24_bug_affected
}

/// `SDL_FillRect` with the 24-bit channel order pre-swapped on affected SDL
/// builds. See [`RGB24_bug_check`].
unsafe fn safe_fill_rect(dst: *mut SDL_Surface, rect: *const SDL_Rect, mut color: u32) -> c_int {
    if crate::platform::sdl::shared_renderer().surface_format_info(dst).bits_per_pixel == 24 && RGB24_bug_check() {
        color = ((color & 0xFF) << 16) | (color & 0xFF00) | ((color & 0xFF0000) >> 16);
    }
    crate::platform::sdl::shared_renderer().fill_rect(dst, rect, color)
}

/// Fills a rect on the current target with an opaque palette colour.
// seg009 method_5_rect
#[no_mangle]
pub unsafe extern "C" fn method_5_rect(rect: *const rect_type, _blit: c_int, color: byte) -> *const rect_type {
    let mut dest_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(rect, &mut dest_rect);
    let (pr, pg, pb) = palette_rgb8(color);
    let rgb_color: u32 = crate::platform::sdl::shared_renderer().map_rgba(crate::platform::sdl::shared_renderer().surface_format_ptr(current_target_surface), pr, pg, pb, 0xFF);
    if safe_fill_rect(current_target_surface, &dest_rect, rgb_color) != 0 {
        sdlperror(cs!("method_5_rect: SDL_FillRect"));
        quit(1);
    }
    rect
}

/// Fills a rect with a semi-transparent palette colour -- the backing of the
/// timer and menu overlays.
///
/// Note the alpha is mapped against `overlay_surface`'s format but filled into
/// `current_target_surface`. That works because the overlay *is* the target
/// whenever this is called, and is left as the C source has it.
// seg009 draw_rect_with_alpha
#[no_mangle]
pub unsafe extern "C" fn draw_rect_with_alpha(rect: *const rect_type, color: byte, alpha: byte) {
    let mut dest_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(rect, &mut dest_rect);
    let (pr, pg, pb) = palette_rgb8(color);
    let rgb_color: u32 = crate::platform::sdl::shared_renderer().map_rgba(crate::platform::sdl::shared_renderer().surface_format_ptr(overlay_surface), pr, pg, pb, alpha);
    if safe_fill_rect(current_target_surface, &dest_rect, rgb_color) != 0 {
        sdlperror(cs!("draw_rect_with_alpha: SDL_FillRect"));
        quit(1);
    }
}

/// Draws a one-pixel outline around a rect, clipped to the surface.
///
/// Writes pixels directly rather than through SDL, so it only supports 32-bit
/// targets; anything else prints a warning and gives up. Used to highlight menu
/// items.
// seg009 draw_rect_contours
#[no_mangle]
pub unsafe extern "C" fn draw_rect_contours(rect: *const rect_type, color: byte) {
    if crate::platform::sdl::shared_renderer().surface_format_info(current_target_surface).bits_per_pixel != 32 {
        crate::c_log(&format!("draw_rect_contours: not implemented for {} bit surfaces\n", crate::platform::sdl::shared_renderer().surface_format_info(current_target_surface).bits_per_pixel as c_int));
        return;
    }
    let mut dest_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(rect, &mut dest_rect);
    let (pr, pg, pb) = palette_rgb8(color);
    let rgb_color: u32 = crate::platform::sdl::shared_renderer().map_rgba(crate::platform::sdl::shared_renderer().surface_format_ptr(overlay_surface), pr, pg, pb, 0xFF);
    if crate::platform::sdl::shared_renderer().lock_surface(current_target_surface) != 0 {
        sdlperror(cs!("draw_rect_contours: SDL_LockSurface"));
        quit(1);
    }
    let bytes_per_pixel = crate::platform::sdl::shared_renderer().surface_format_info(current_target_surface).bytes_per_pixel as c_int;
    let pitch = crate::platform::sdl::shared_renderer().surface_pitch(current_target_surface);
    let pixels = crate::platform::sdl::shared_renderer().surface_pixels(current_target_surface) as *mut byte;
    let (target_w, target_h) = crate::platform::sdl::shared_renderer().surface_size(current_target_surface);
    let xmin = MIN_i(dest_rect.x, target_w);
    let xmax = MIN_i(dest_rect.x + dest_rect.w, target_w);
    let ymin = MIN_i(dest_rect.y, target_h);
    let ymax = MIN_i(dest_rect.y + dest_rect.h, target_h);
    let mut row = pixels.offset((ymin * pitch) as isize);
    let mut pixel = row.offset((xmin * bytes_per_pixel) as isize) as *mut u32;
    for _x in xmin..xmax {
        *pixel = rgb_color;
        pixel = pixel.add(1);
    }
    for _y in (ymin + 1)..(ymax - 1) {
        row = row.offset(pitch as isize);
        *(row.offset((xmin * bytes_per_pixel) as isize) as *mut u32) = rgb_color;
        *(row.offset(((xmax - 1) * bytes_per_pixel) as isize) as *mut u32) = rgb_color;
    }
    pixel = pixels.offset(((ymax - 1) * pitch + xmin * bytes_per_pixel) as isize) as *mut u32;
    for _x in xmin..xmax {
        *pixel = rgb_color;
        pixel = pixel.add(1);
    }

    crate::platform::sdl::shared_renderer().unlock_surface(current_target_surface);
}

/// XOR-blits an image onto a surface, via a read-modify-write through a scratch
/// 24-bit surface.
///
/// SDL has no XOR blend mode, so the destination region is read out, XORed byte
/// by byte with the image, and written back. Used for the flashing "press any
/// key" style effects.
unsafe fn blit_xor(target_surface: *mut SDL_Surface, dest_rect: *mut SDL_Rect, image: *mut SDL_Surface, src_rect: *mut SDL_Rect) {
    if (*dest_rect).w != (*src_rect).w || (*dest_rect).h != (*src_rect).h {
        crate::c_log("blit_xor: dest_rect and src_rect have different sizes\n");
        quit(1);
    }
    let helper_surface = crate::platform::sdl::shared_renderer().create_surface((*dest_rect).w, (*dest_rect).h, 24, Rmsk, Gmsk, Bmsk, 0);
    if helper_surface.is_null() {
        sdlperror(cs!("blit_xor: SDL_CreateRGBSurface"));
        quit(1);
    }
    let image_24 = crate::platform::sdl::shared_renderer().convert_surface(image, crate::platform::sdl::shared_renderer().surface_format_ptr(helper_surface), 0);
    if image_24.is_null() {
        sdlperror(cs!("blit_xor: SDL_CreateRGBSurface"));
        quit(1);
    }
    let mut dest_rect2: SDL_Rect = *src_rect;
    // Read what is currently where we want to draw the new image.
    if SDL_BlitSurface(target_surface, dest_rect, helper_surface, &mut dest_rect2) != 0 {
        sdlperror(cs!("blit_xor: SDL_BlitSurface"));
        quit(1);
    }
    if crate::platform::sdl::shared_renderer().lock_surface(image_24) != 0 {
        sdlperror(cs!("blit_xor: SDL_LockSurface"));
        quit(1);
    }
    if crate::platform::sdl::shared_renderer().lock_surface(helper_surface) != 0 {
        sdlperror(cs!("blit_xor: SDL_LockSurface"));
        quit(1);
    }
    let renderer = crate::platform::sdl::shared_renderer();
    let size = renderer.surface_size(helper_surface).1 * renderer.surface_pitch(helper_surface);
    let mut p_src = renderer.surface_pixels(image_24) as *mut byte;
    let mut p_dest = renderer.surface_pixels(helper_surface) as *mut byte;

    // Xor the old area with the image.
    for _i in 0..size {
        *p_dest ^= *p_src;
        p_src = p_src.add(1);
        p_dest = p_dest.add(1);
    }
    crate::platform::sdl::shared_renderer().unlock_surface(image_24);
    crate::platform::sdl::shared_renderer().unlock_surface(helper_surface);
    // Put the new area in place of the old one.
    if SDL_BlitSurface(helper_surface, src_rect, target_surface, dest_rect) != 0 {
        sdlperror(cs!("blit_xor: SDL_BlitSurface 2065"));
        quit(1);
    }
    crate::platform::sdl::shared_renderer().free_surface(image_24);
    crate::platform::sdl::shared_renderer().free_surface(helper_surface);
}

/// Draws a torch flame recoloured to an arbitrary RGB (USE_COLORED_TORCHES).
///
/// `color` packs three 2-bit channels (`rrggbb`), each scaled by 85 to span
/// 0..255. Every pixel matching the flame's stock orange `#FC8400` is replaced;
/// the rest of the sprite is untouched, so the torch bracket keeps its colour.
unsafe fn draw_colored_torch(color: c_int, image: *mut SDL_Surface, xpos: c_int, ypos: c_int) {
    if crate::platform::sdl::shared_renderer().set_color_key(image, true, 0) != 0 {
        sdlperror(cs!("draw_colored_torch: SDL_SetColorKey"));
        quit(1);
    }

    let colored_image = crate::platform::sdl::shared_renderer().convert_surface_format(image, SDL_PIXELFORMAT_ARGB8888, 0);
    crate::platform::sdl::shared_renderer().set_blend_mode(colored_image, SDL_BLENDMODE_NONE);

    if crate::platform::sdl::shared_renderer().lock_surface(colored_image) != 0 {
        sdlperror(cs!("draw_colored_torch: SDL_LockSurface"));
        quit(1);
    }

    let (w, h) = crate::platform::sdl::shared_renderer().surface_size(colored_image);
    let iRed = ((color >> 4) & 3) * 85;
    let iGreen = ((color >> 2) & 3) * 85;
    let iBlue = ((color >> 0) & 3) * 85;
    let colored_image_format = crate::platform::sdl::shared_renderer().surface_format_ptr(colored_image);
    let old_color: u32 = crate::platform::sdl::shared_renderer().map_rgb(colored_image_format, 0xFC, 0x84, 0x00) & 0xFFFFFF;
    let new_color: u32 = crate::platform::sdl::shared_renderer().map_rgb(colored_image_format, iRed as u8, iGreen as u8, iBlue as u8) & 0xFFFFFF;
    let stride = crate::platform::sdl::shared_renderer().surface_pitch(colored_image);
    let colored_pixels = crate::platform::sdl::shared_renderer().surface_pixels(colored_image) as *mut byte;
    for y in 0..h {
        let mut pixel_ptr = colored_pixels.offset((stride * y) as isize) as *mut u32;
        for _x in 0..w {
            if (*pixel_ptr & 0xFFFFFF) == old_color {
                *pixel_ptr = (*pixel_ptr & 0xFF000000) | new_color;
            }
            pixel_ptr = pixel_ptr.add(1);
        }
    }
    crate::platform::sdl::shared_renderer().unlock_surface(colored_image);

    method_6_blit_img_to_scr(colored_image, xpos, ypos, blitters_blitters_0_no_transp as c_int);
    crate::platform::sdl::shared_renderer().free_surface(colored_image);
}

/// The general image blitter -- every sprite in the game reaches the screen
/// through here.
///
/// Dispatches on the `blitters` id: black-silhouette, XOR, and the range of
/// coloured-flame ids each go to a specialised path, and everything else is a
/// plain blit with transparency on or off. Indexed surfaces use a colour key,
/// truecolour ones use a blend mode, since only one of the two applies to each.
// seg009 method_6_blit_img_to_scr
#[no_mangle]
pub unsafe extern "C" fn method_6_blit_img_to_scr(image: *mut image_type, xpos: c_int, ypos: c_int, blit: c_int) -> *mut image_type {
    if image.is_null() {
        crate::c_log("method_6_blit_img_to_scr: image == NULL\n");
        return null_mut();
    }

    if blit == blitters_blitters_9_black as c_int {
        method_3_blit_mono(image, xpos, ypos, blitters_blitters_9_black as c_int, 0);
        return image;
    }

    let (image_w, image_h) = crate::platform::sdl::shared_renderer().surface_size(image);
    let mut src_rect = SDL_Rect { x: 0, y: 0, w: image_w, h: image_h };
    let mut dest_rect = SDL_Rect { x: xpos, y: ypos, w: image_w, h: image_h };

    if blit == blitters_blitters_3_xor as c_int {
        blit_xor(current_target_surface, &mut dest_rect, image, &mut src_rect);
        return image;
    }

    let colored_flames = (blitters_blitters_colored_flame as c_int)..=(blitters_blitters_colored_flame_last as c_int);
    if colored_flames.contains(&blit) {
        draw_colored_torch(blit - blitters_blitters_colored_flame as c_int, image, xpos, ypos);
        return image;
    }

    crate::platform::sdl::shared_renderer().set_blend_mode(image, SDL_BLENDMODE_NONE);
    crate::platform::sdl::shared_renderer().set_color_key(image, false, 0);
    crate::platform::sdl::shared_renderer().set_alpha_mod(image, 255);

    let transparent = blit != blitters_blitters_0_no_transp as c_int;
    if SDL_ISPIXELFORMAT_INDEXED(crate::platform::sdl::shared_renderer().surface_format_info(image).format) {
        crate::platform::sdl::shared_renderer().set_color_key(image, transparent, 0);
    } else {
        let mode = if transparent { SDL_BLENDMODE_BLEND } else { SDL_BLENDMODE_NONE };
        crate::platform::sdl::shared_renderer().set_blend_mode(image, mode);
    }
    if SDL_BlitSurface(image, &src_rect, current_target_surface, &mut dest_rect) != 0 {
        sdlperror(cs!("method_6_blit_img_to_scr: SDL_BlitSurface 2247"));
    }
    image
}

// ============================================================================
// Screen: window, renderer, scaling and presentation
// ============================================================================

/// Sets the renderer's logical size to the chosen aspect ratio.
///
/// The 320x200 framebuffer was displayed on 4:3 CRTs with non-square pixels.
/// "Correct" aspect ratio reproduces that by declaring a 1600x1200 logical size;
/// the alternative shows the raw 16:10 pixels.
// seg009 apply_aspect_ratio
#[no_mangle]
pub unsafe extern "C" fn apply_aspect_ratio() {
    if use_correct_aspect_ratio != 0 {
        crate::platform::sdl::shared_renderer().render_set_logical_size(renderer_, 320 * 5, 200 * 6); // 4:3
    } else {
        crate::platform::sdl::shared_renderer().render_set_logical_size(renderer_, 320, 200); // 16:10
    }
    window_resized();
}

/// Re-evaluates whether integer scaling is usable at the current window size.
///
/// Integer scaling is switched off when the window is *smaller* than the
/// logical size, since the smallest integer factor (1x) would then crop rather
/// than fit.
// seg009 window_resized
#[no_mangle]
pub unsafe extern "C" fn window_resized() {
    if use_integer_scaling != 0 {
        let renderer = crate::platform::sdl::shared_renderer();
        let (window_width, window_height) = renderer.get_renderer_output_size(renderer_);
        let (render_width, render_height) = renderer.render_get_logical_size(renderer_);
        let makes_sense = window_width >= render_width && window_height >= render_height;
        renderer.render_set_integer_scale(renderer_, makes_sense);
    }
}

/// `SDL_SetHint` over the NUL-terminated byte-string constants above.
#[inline]
unsafe fn set_hint(name: &[u8], value: *const c_char) {
    crate::platform::sdl::shared_renderer().set_hint(
        std::ffi::CStr::from_ptr(name.as_ptr() as *const c_char),
        std::ffi::CStr::from_ptr(value),
    );
}

/// Sets `SDL_RENDER_SCALE_QUALITY`, which SDL samples when a texture is
/// *created*, not when it is drawn.
#[inline]
unsafe fn set_scale_quality_linear(linear: bool) {
    set_hint(SDL_HINT_RENDER_SCALE_QUALITY, if linear { cs!("1") } else { cs!("0") });
}

/// Allocates the overlay and compositing surfaces, once.
unsafe fn init_overlay() {
    if !overlay_initialized {
        overlay_surface = crate::platform::sdl::shared_renderer().create_surface(320, 200, 32, Rmsk, Gmsk, Bmsk, Amsk);
        merged_surface = crate::platform::sdl::shared_renderer().create_surface(320, 200, 24, Rmsk, Gmsk, Bmsk, 0);
        overlay_initialized = true;
    }
}

/// Creates whichever texture the current `scaling_type` needs and points
/// `target_texture` at it. Idempotent, and called every frame.
///
/// * 0 (sharp) -- a 320x200 nearest-neighbour texture.
/// * 1 (fuzzy) -- upscale 2x with linear filtering *first*, then let the
///   renderer scale the result, which is how DOSBox gets its soft-but-not-blurry
///   look. Done on the GPU as a render target where available, otherwise via a
///   CPU `SDL_BlitScaled` into `onscreen_surface_2x`.
/// * 2 (blurry) -- a 320x200 texture with linear filtering, so the renderer
///   smooths it on the way up.
///
/// The scale-quality hint is set and cleared around each creation because SDL
/// samples it when the texture is made, not when it is drawn.
unsafe fn init_scaling() {
    // Don't crash in validate mode.
    if renderer_.is_null() {
        return;
    }
    if texture_sharp.is_null() {
        texture_sharp = crate::platform::sdl::shared_renderer().create_texture(renderer_, SDL_PIXELFORMAT_RGB24, SDL_TEXTUREACCESS_STREAMING, 320, 200);
    }
    if scaling_type == 1 {
        if !is_renderer_targettexture_supported && onscreen_surface_2x.is_null() {
            onscreen_surface_2x = crate::platform::sdl::shared_renderer().create_surface(320 * 2, 200 * 2, 24, Rmsk, Gmsk, Bmsk, 0);
        }
        if texture_fuzzy.is_null() {
            set_scale_quality_linear(true);
            let access = if is_renderer_targettexture_supported { SDL_TEXTUREACCESS_TARGET } else { SDL_TEXTUREACCESS_STREAMING };
            texture_fuzzy = crate::platform::sdl::shared_renderer().create_texture(renderer_, SDL_PIXELFORMAT_RGB24, access, 320 * 2, 200 * 2);
            set_scale_quality_linear(false);
        }
        target_texture = texture_fuzzy;
    } else if scaling_type == 2 {
        if texture_blurry.is_null() {
            set_scale_quality_linear(true);
            texture_blurry = crate::platform::sdl::shared_renderer().create_texture(renderer_, SDL_PIXELFORMAT_RGB24, SDL_TEXTUREACCESS_STREAMING, 320, 200);
            set_scale_quality_linear(false);
        }
        target_texture = texture_blurry;
    } else {
        target_texture = texture_sharp;
    }
    if target_texture.is_null() {
        sdlperror(cs!("init_scaling: SDL_CreateTexture"));
        quit(1);
    }
}

/// Brings up everything graphical: SDL, the window, the renderer, the
/// framebuffer surfaces and the fonts.
///
/// The `grmode` argument is ignored -- only the VGA mode is implemented.
///
/// In validate (replay-checking) mode no window is created at all, but the
/// renderer and surfaces still are, so the game's drawing code runs unchanged
/// and headless replays exercise the same paths.
///
/// VSync is explicitly disabled: the game's timing is driven by the performance
/// counter, and letting the display block presentation would drag those timers
/// with it.
// seg009:38ED set_gr_mode
#[no_mangle]
pub unsafe extern "C" fn set_gr_mode(_grmode: byte) {
    set_hint(SDL_HINT_WINDOWS_DISABLE_THREAD_NAMING, cs!("1"));
    if crate::platform::sdl::shared_renderer().sdl_init(SDL_INIT_VIDEO | SDL_INIT_TIMER | SDL_INIT_NOPARACHUTE | SDL_INIT_GAMECONTROLLER) != 0 {
        sdlperror(cs!("set_gr_mode: SDL_Init"));
        quit(1);
    }
    if let Err(e) = crate::platform::sdl::shared_input().init() {
        eprintln!("set_gr_mode: failed to initialize input (event pump): {e}");
        quit(1);
    }
    if enable_controller_rumble != 0 {
        if crate::platform::sdl::shared_renderer().sdl_init_subsystem(SDL_INIT_HAPTIC) != 0 {
            crate::c_log("Warning: Haptic subsystem unavailable, ignoring enable_controller_rumble = true\n");
        }
    }

    let mut flags: u32 = 0;
    if start_fullscreen == 0 {
        start_fullscreen = (!check_param(cs!("full")).is_null()) as byte;
    }
    if start_fullscreen != 0 {
        flags |= SDL_WINDOW_FULLSCREEN_DESKTOP;
    }
    flags |= SDL_WINDOW_RESIZABLE;
    flags |= SDL_WINDOW_ALLOW_HIGHDPI; // for Retina displays

    // Should use different default window dimensions when using 4:3 aspect ratio
    if use_correct_aspect_ratio != 0 && pop_window_width == 640 && pop_window_height == 400 {
        pop_window_height = 480;
    }

    if is_validate_mode == 0 {
        // run without a window if validating a replay
        window_ = crate::platform::sdl::shared_renderer().create_window(
            std::ffi::CStr::from_ptr(cs!("Prince of Persia (SDLPoP) v1.24 RC")),
            SDL_WINDOWPOS_UNDEFINED,
            SDL_WINDOWPOS_UNDEFINED,
            pop_window_width as c_int,
            pop_window_height as c_int,
            flags,
        );
    }
    // Make absolutely sure that VSync will be off, to prevent timer issues.
    set_hint(SDL_HINT_RENDER_VSYNC, cs!("0"));
    // Anything other than 0 or 1 means "let SDL choose".
    flags = match use_hardware_acceleration {
        0 => SDL_RENDERER_SOFTWARE,
        1 => SDL_RENDERER_ACCELERATED,
        _ => 0,
    };
    renderer_ = crate::platform::sdl::shared_renderer().create_renderer(window_, -1, flags | SDL_RENDERER_TARGETTEXTURE);
    let renderer_info_flags = crate::platform::sdl::shared_renderer().get_renderer_info_flags(renderer_);
    if renderer_info_flags & SDL_RENDERER_TARGETTEXTURE != 0 {
        is_renderer_targettexture_supported = true;
    }
    if use_integer_scaling != 0 {
        crate::platform::sdl::shared_renderer().render_set_integer_scale(renderer_, true);
    }

    let mut __icon_lf = [0 as c_char; POP_MAX_PATH];
    let icon = crate::platform::sdl::shared_renderer().load_image_from_file(std::ffi::CStr::from_ptr(locate_file_(cs!("data/icon.png"), __icon_lf.as_mut_ptr(), POP_MAX_PATH as c_int)));
    if icon.is_null() {
        sdlperror(cs!("set_gr_mode: Could not load icon"));
    } else {
        crate::platform::sdl::shared_renderer().set_window_icon(window_, icon);
    }

    apply_aspect_ratio();
    window_resized();

    onscreen_surface_ = crate::platform::sdl::shared_renderer().create_surface(320, 200, 24, Rmsk, Gmsk, Bmsk, 0);
    if onscreen_surface_.is_null() {
        sdlperror(cs!("set_gr_mode: SDL_CreateRGBSurface"));
        quit(1);
    }
    init_overlay();
    init_scaling();
    if start_fullscreen != 0 {
        crate::platform::sdl::shared_renderer().show_cursor(false);
    }

    graphics_mode = grmodes_gmMcgaVga as byte;
    load_font();
}

/// The surface that should actually be shown: the plain framebuffer, or the
/// composited one when an overlay is up.
// seg009 get_final_surface
#[no_mangle]
pub unsafe extern "C" fn get_final_surface() -> *mut SDL_Surface {
    if !is_overlay_displayed {
        onscreen_surface_
    } else {
        merged_surface
    }
}

/// Which overlay, if any, should be composited over the frame.
///
/// The pause menu wins over either timer; the level timer wins over the feather
/// timer.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Overlay {
    None,
    LevelTimer,
    Menu,
    FeatherTimer,
}

/// Draws the current overlay into `overlay_surface` and composites it with the
/// frame into `merged_surface`.
///
/// The game's own framebuffer is never touched, so the overlay can appear and
/// disappear without the frame underneath being redrawn.
unsafe fn draw_overlay() {
    is_overlay_displayed = false;
    let mut overlay = if is_timer_displayed != 0 && start_level > 0 {
        Overlay::LevelTimer
    } else if (*fixes).fix_quicksave_during_feather != 0
        && is_feather_timer_displayed != 0
        && start_level > 0
        && is_feather_fall > 0
    {
        Overlay::FeatherTimer
    } else {
        Overlay::None
    };
    // Menu overlay
    if is_paused != 0 && is_menu_shown != 0 {
        overlay = Overlay::Menu;
    }
    if overlay != Overlay::None {
        is_overlay_displayed = true;
        let saved_target_surface = current_target_surface;
        current_target_surface = overlay_surface;
        let drawn_rect: rect_type;
        if overlay == Overlay::LevelTimer {
            let mut timer_text = [0 as c_char; 32];
            if rem_min < 0 {
                crate::write_c_str_truncating(timer_text.as_mut_ptr(), 32, &format!("{:02}:{:02}:{:02}", -((rem_min as c_int) + 1), (719 - rem_tick as c_int) / 12, (719 - rem_tick as c_int) % 12));
            } else {
                crate::write_c_str_truncating(timer_text.as_mut_ptr(), 32, &format!("{:02}:{:02}:{:02}", rem_min as c_int - 1, rem_tick as c_int / 12, rem_tick as c_int % 12));
            }
            let expected_numeric_chars = 6;
            let extra_numeric_chars = MAX_i(0, strnlen(timer_text.as_ptr(), 32) as c_int - 8);
            let line_width = 5 + (expected_numeric_chars + extra_numeric_chars) * 9;

            let mut timer_box_rect = rect_type { top: 0, left: 0, bottom: 11, right: (2 + line_width) as c_short };
            let timer_text_rect = rect_type { top: 2, left: 2, bottom: 10, right: 100 };
            draw_rect_with_alpha(&timer_box_rect, colorids_color_0_black as byte, 128);
            show_text(&timer_text_rect, halign_left as c_int, valign_top as c_int, timer_text.as_ptr());

            // During playback, display the number of ticks since start.
            if replaying != 0 {
                let mut ticks_text = [0 as c_char; 12];
                crate::write_c_str_truncating(ticks_text.as_mut_ptr(), 12, &format!("T: {}", curr_tick));
                let mut ticks_box_rect = timer_box_rect;
                ticks_box_rect.top += 12;
                ticks_box_rect.bottom += 12;
                let mut ticks_text_rect = timer_text_rect;
                ticks_text_rect.top += 12;
                ticks_text_rect.bottom += 12;

                draw_rect_with_alpha(&ticks_box_rect, colorids_color_0_black as byte, 128);
                show_text(&ticks_text_rect, halign_left as c_int, valign_top as c_int, ticks_text.as_ptr());

                timer_box_rect.bottom += 12;
            }

            drawn_rect = timer_box_rect;
        } else if overlay == Overlay::FeatherTimer {
            // Feather timer
            let mut timer_text = [0 as c_char; 32];
            let ticks_per_sec = get_ticks_per_sec(timerids_timer_1 as c_int) as c_int;
            crate::write_c_str_truncating(timer_text.as_mut_ptr(), 32, &format!("{:02}:{:02}", is_feather_fall as c_int / ticks_per_sec, is_feather_fall as c_int % ticks_per_sec));
            let expected_numeric_chars = 6;
            let extra_numeric_chars = MAX_i(0, strnlen(timer_text.as_ptr(), 32) as c_int - 8);
            let line_width = 5 + (expected_numeric_chars + extra_numeric_chars) * 9;

            let timer_box_rect = rect_type { top: 0, left: 0, bottom: 11, right: (2 + line_width) as c_short };
            let timer_text_rect = rect_type { top: 2, left: 2, bottom: 10, right: 100 };
            draw_rect_with_alpha(&timer_box_rect, colorids_color_0_black as byte, 128);
            show_text_with_color(&timer_text_rect, halign_left as c_int, valign_top as c_int, timer_text.as_ptr(), colorids_color_10_brightgreen as c_int);

            drawn_rect = timer_box_rect;
        } else {
            drawn_rect = screen_rect;
        }
        let mut sdl_rect: SDL_Rect = core::mem::zeroed();
        rect_to_sdlrect(&drawn_rect, &mut sdl_rect);
        SDL_BlitSurface(onscreen_surface_, core::ptr::null(), merged_surface, null_mut());
        SDL_BlitSurface(overlay_surface, &sdl_rect, merged_surface, &mut sdl_rect);
        current_target_surface = saved_target_surface;
    }
}

/// Presents one frame: composite overlays, upload to a texture, scale, present.
// seg009 update_screen
#[no_mangle]
pub unsafe extern "C" fn update_screen() {
    draw_overlay();
    let mut surface = get_final_surface();
    init_scaling();
    if scaling_type == 1 {
        // Make "fuzzy pixels" like DOSBox does.
        if is_renderer_targettexture_supported {
            let surface_pixels = crate::platform::sdl::shared_renderer().surface_pixels(surface);
            let surface_pitch = crate::platform::sdl::shared_renderer().surface_pitch(surface);
            crate::platform::sdl::shared_renderer().update_texture(texture_sharp, core::ptr::null(), surface_pixels, surface_pitch);
            set_scale_quality_linear(true);
            crate::platform::sdl::shared_renderer().set_render_target(renderer_, target_texture);
            set_scale_quality_linear(false);
            crate::platform::sdl::shared_renderer().render_clear(renderer_);
            crate::platform::sdl::shared_renderer().render_copy(renderer_, texture_sharp, core::ptr::null(), core::ptr::null());
            crate::platform::sdl::shared_renderer().set_render_target(renderer_, null_mut());
        } else {
            SDL_BlitScaled(surface, core::ptr::null(), onscreen_surface_2x, null_mut());
            surface = onscreen_surface_2x;
            let surface_pixels = crate::platform::sdl::shared_renderer().surface_pixels(surface);
            let surface_pitch = crate::platform::sdl::shared_renderer().surface_pitch(surface);
            crate::platform::sdl::shared_renderer().update_texture(target_texture, core::ptr::null(), surface_pixels, surface_pitch);
        }
    } else {
        let surface_pixels = crate::platform::sdl::shared_renderer().surface_pixels(surface);
        let surface_pitch = crate::platform::sdl::shared_renderer().surface_pitch(surface);
        crate::platform::sdl::shared_renderer().update_texture(target_texture, core::ptr::null(), surface_pixels, surface_pitch);
    }
    crate::platform::sdl::shared_renderer().render_clear(renderer_);
    crate::platform::sdl::shared_renderer().render_copy(renderer_, target_texture, core::ptr::null(), core::ptr::null());
    crate::platform::sdl::shared_renderer().render_present(renderer_);
}

// ============================================================================
// Timers
//
// Three independent timers, driven by SDL's performance counter rather than by
// millisecond ticks. Each holds a start counter and a length in *game ticks*;
// `has_timer_stopped` converts the elapsed counter difference into ticks and
// compares. `fps` is the tick rate, which fast-forward multiplies by
// FAST_FORWARD_RATIO -- so speeding the game up is a matter of redefining how
// long a tick is, not of skipping frames.
// ============================================================================

/// Restarts a timer's clock without changing its length.
// seg009 reset_timer
#[no_mangle]
pub unsafe extern "C" fn reset_timer(timer_index: c_int) {
    timer_last_counter[timer_index as usize] = crate::platform::sdl::shared_renderer().performance_counter();
}

/// How many times per second this timer currently expires.
///
/// Its length is in ticks and `fps` ticks pass per second, so this is the
/// timer's real-time rate -- used to convert the feather-fall countdown between
/// game speeds.
// seg009 get_ticks_per_sec
#[no_mangle]
pub unsafe extern "C" fn get_ticks_per_sec(timer_index: c_int) -> f64 {
    fps as f64 / wait_time[timer_index as usize] as f64
}

/// Rescales the feather-fall countdown when the game speed changes underneath
/// it.
///
/// Feather fall is counted in timer expiries, not seconds, so changing the timer
/// length would otherwise silently lengthen or shorten the effect. Only scales
/// *down*-going conversions where the remaining count exceeds both rates, which
/// is the C source's guard against rounding a nearly-expired counter to zero.
unsafe fn recalculate_feather_fall_timer(previous_ticks_per_second: f64, ticks_per_second: f64) {
    if (is_feather_fall as f64) <= previous_ticks_per_second.max(ticks_per_second)
        || previous_ticks_per_second == ticks_per_second
    {
        return;
    }
    // there are more ticks per second in base mode vs fight mode so
    // feather fall length needs to be recalculated
    is_feather_fall = (is_feather_fall as f64 / previous_ticks_per_second * ticks_per_second) as word;
}

/// Sets a timer's length in ticks.
///
/// With `fix_quicksave_during_feather` on, changing the length while feather
/// fall is active also rescales the countdown -- but only when the *old* length
/// was one of the two gameplay speeds, so setting a cutscene or menu timer does
/// not disturb it.
// seg009 set_timer_length
#[no_mangle]
pub unsafe extern "C" fn set_timer_length(timer_index: c_int, length: c_int) {
    if (*fixes).fix_quicksave_during_feather == 0 {
        wait_time[timer_index as usize] = length;
        return;
    }
    if is_feather_fall == 0 || wait_time[timer_index as usize] < (*custom).base_speed as c_int || wait_time[timer_index as usize] > (*custom).fight_speed as c_int {
        wait_time[timer_index as usize] = length;
        return;
    }
    let previous_ticks_per_second: f64 = get_ticks_per_sec(timer_index);
    wait_time[timer_index as usize] = length;
    let ticks_per_second: f64 = get_ticks_per_sec(timer_index);
    recalculate_feather_fall_timer(previous_ticks_per_second, ticks_per_second);
}

/// Starts a timer: reset its clock and set its length.
///
/// Skipped entirely while fast-forwarding a replay, so the game never waits.
// seg009 start_timer
#[no_mangle]
pub unsafe extern "C" fn start_timer(timer_index: c_int, length: c_int) {
    if replaying != 0 && skipping_replay != 0 {
        return;
    }
    timer_last_counter[timer_index as usize] = crate::platform::sdl::shared_renderer().performance_counter();
    wait_time[timer_index as usize] = length;
}

/// Toggles borderless fullscreen, hiding the cursor while fullscreen.
unsafe fn toggle_fullscreen() {
    let flags = crate::platform::sdl::shared_renderer().get_window_flags(window_);
    if flags & SDL_WINDOW_FULLSCREEN_DESKTOP != 0 {
        crate::platform::sdl::shared_renderer().set_fullscreen(false);
        crate::platform::sdl::shared_renderer().show_cursor(true);
    } else {
        crate::platform::sdl::shared_renderer().set_fullscreen(true);
        crate::platform::sdl::shared_renderer().show_cursor(false);
    }
}

// ============================================================================
// Scripted input injection -- not in the original C. Lets a headless run (see
// the `headless` flag in seg000.rs's pop_main) drive actual gameplay instead
// of idling at the title screen: synthetic SDL_KEYDOWN/KEYUP events are pushed
// onto SDL's real event queue at scripted tick numbers, so process_events()
// below picks them up through the exact same code path as real keyboard input
// -- no separate key-injection path to keep in sync with the real one.
//
// Script format (path from the POPTRACE_INPUT env var, one event per line):
//     <tick> <key> <down|up>
//     <tick> mousemove <x> <y>
//     <tick> mouseleft <down|up>
//     <tick> mouseright <down|up>
//     # blank lines and lines starting with '#' are ignored
// <tick> is the same per-simulation-tick clock the POPTRACE_OUT trace uses
// (state_dump.rs's tick_counter, exposed via next_tick()) -- NOT a count of
// process_events() calls, which fire a variable number of times per tick
// while do_simple_wait spins on real wall-clock time. Keying off next_tick()
// instead keeps a script's behavior identical regardless of how fast the
// machine running it is.
// <key> is one of: left right up down shift lshift rshift space return escape
//
// `mousemove` sets the tracked mouse position directly (`InputSource::warp_mouse` -- see its
// doc comment for why a pushed SDL_MOUSEMOTION event can't do this on native). `mouseleft`/
// `mouseright` only need a `down` line to have an effect: menu.rs's mouse_clicked/
// mouse_button_clicked_right are set from the discrete SDL_MOUSEBUTTONDOWN event alone (real
// SDL never delivers/consumes a corresponding BUTTONUP for this codebase's purposes --
// confirmed by grep, process_events has no SDL_MOUSEBUTTONUP arm), so `up` lines exist only
// for script readability/symmetry with the `<key> down/up` pairs and inject nothing.
// ============================================================================
enum ScriptedEvent {
    Key { scancode: c_int, down: bool },
    MouseMove { x: c_int, y: c_int },
    MouseButton { button: u8 },
}
static mut SCRIPT_EVENTS: Vec<(u32, ScriptedEvent)> = Vec::new();
static mut SCRIPT_LOADED: bool = false;
static mut SCRIPT_INDEX: usize = 0;

/// Maps a script's key name to an SDL scancode.
fn scancode_from_key_name(name: &str) -> Option<c_int> {
    Some(match name.to_ascii_lowercase().as_str() {
        "left" => SDL_SCANCODE_LEFT,
        "right" => SDL_SCANCODE_RIGHT,
        "up" => SDL_SCANCODE_UP,
        "down" => SDL_SCANCODE_DOWN,
        "shift" | "lshift" => SDL_SCANCODE_LSHIFT,
        "rshift" => SDL_SCANCODE_RSHIFT,
        "space" => SDL_SCANCODE_SPACE,
        "return" | "enter" => SDL_SCANCODE_RETURN,
        "escape" => SDL_SCANCODE_ESCAPE,
        _ => return None,
    })
}

/// Parses the `POPTRACE_INPUT` script into `SCRIPT_EVENTS`, once.
///
/// Malformed lines are reported and skipped rather than fatal, so a typo in a
/// long script does not lose the rest of it.
///
/// Reads via the shared `getenv`/`fopen`-family primitives (not `std::env`/`std::fs`,
/// which have no wasm32-unknown-unknown backing) so this same code path works
/// unchanged on both native and wasm: natively `getenv` is the real libc call and
/// `fopen` opens a real file; on wasm they resolve to `wasm_libc`'s fake-env table
/// (`wasm_setenv`) and virtual filesystem (`preload_file`) instead -- see
/// `web/headless.mjs`'s scripted-input mode for the wasm-side setup.
unsafe fn load_scripted_input() {
    SCRIPT_LOADED = true;
    let path_ptr = getenv(b"POPTRACE_INPUT\0".as_ptr() as *const c_char);
    if path_ptr.is_null() {
        return;
    }
    let path = std::ffi::CStr::from_ptr(path_ptr).to_string_lossy().into_owned();
    let fp = fopen(path_ptr, b"rb\0".as_ptr() as *const c_char);
    if fp.is_null() {
        eprintln!("scripted_input: could not open {}", path);
        return;
    }
    fseek(fp, 0, 2 /* SEEK_END */);
    let size = ftell(fp);
    fseek(fp, 0, 0 /* SEEK_SET */);
    let mut buf = vec![0u8; size.max(0) as usize];
    if !buf.is_empty() {
        fread(buf.as_mut_ptr() as *mut c_void, 1, buf.len(), fp);
    }
    fclose(fp);
    let contents = String::from_utf8_lossy(&buf).into_owned();
    for (lineno, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let Some(tick) = parts.first().and_then(|s| s.parse::<u32>().ok()) else {
            eprintln!("scripted_input: {}:{}: bad tick number", path, lineno + 1);
            continue;
        };
        let event = match parts.get(1).copied() {
            Some("mousemove") => {
                let (Some(x), Some(y)) = (
                    parts.get(2).and_then(|s| s.parse::<c_int>().ok()),
                    parts.get(3).and_then(|s| s.parse::<c_int>().ok()),
                ) else {
                    eprintln!("scripted_input: {}:{}: expected '<tick> mousemove <x> <y>'", path, lineno + 1);
                    continue;
                };
                ScriptedEvent::MouseMove { x, y }
            }
            Some(name @ ("mouseleft" | "mouseright")) => {
                match parts.get(2).copied() {
                    Some("down") => ScriptedEvent::MouseButton {
                        button: if name == "mouseleft" { SDL_BUTTON_LEFT } else { SDL_BUTTON_RIGHT },
                    },
                    Some("up") => continue, // no event to inject -- see the format doc comment
                    _ => { eprintln!("scripted_input: {}:{}: expected 'down' or 'up'", path, lineno + 1); continue; }
                }
            }
            Some(key_name) => {
                let Some(scancode) = scancode_from_key_name(key_name) else {
                    eprintln!("scripted_input: {}:{}: unknown key name '{}'", path, lineno + 1, key_name);
                    continue;
                };
                let down = match parts.get(2).copied() {
                    Some("down") => true,
                    Some("up") => false,
                    _ => { eprintln!("scripted_input: {}:{}: expected 'down' or 'up'", path, lineno + 1); continue; }
                };
                ScriptedEvent::Key { scancode, down }
            }
            None => {
                eprintln!("scripted_input: {}:{}: expected '<tick> <key> <down|up>'", path, lineno + 1);
                continue;
            }
        };
        SCRIPT_EVENTS.push((tick, event));
    }
    SCRIPT_EVENTS.sort_by_key(|e| e.0);
}

/// Pushes any scripted events whose tick has arrived onto SDL's real event
/// queue.
///
/// `<=` rather than `==` so an event whose tick was skipped (the tick clock can
/// advance by more than one between calls) still fires, late, rather than being
/// lost.
unsafe fn inject_scripted_input() {
    if !SCRIPT_LOADED {
        load_scripted_input();
    }
    if SCRIPT_EVENTS.is_empty() {
        return;
    }
    // The pause menu's own inner loop (draw_menu) blocks the outer per-tick loop for as long
    // as it's open, freezing next_tick() solid -- so a menu-navigation script (open, click,
    // click, close) could never schedule more than one action, all of it landing on whatever
    // tick the menu happened to open at (see project_wasm_esc_menu_crash memory / the
    // open_menu.txt header for the original discovery of this). MENU_POLL_COUNTER gives
    // menu-internal scripts their own advancing clock riding on top of the frozen tick:
    // it's 0 whenever the menu is closed (so effective_tick == next_tick() exactly,
    // identical to every existing gameplay script's behavior -- nothing here changes outside
    // a menu), and increments by 1 on every process_events() call while the menu is open
    // (draw_menu calls process_events() once per loop iteration), letting a script simply
    // keep counting up ticks past the menu's opening tick to schedule a sequence of steps.
    static mut MENU_POLL_COUNTER: u32 = 0;
    if is_menu_shown != 0 {
        MENU_POLL_COUNTER += 1;
    } else {
        MENU_POLL_COUNTER = 0;
    }
    let tick = crate::state_dump::next_tick() + MENU_POLL_COUNTER;
    while SCRIPT_INDEX < SCRIPT_EVENTS.len() && SCRIPT_EVENTS[SCRIPT_INDEX].0 <= tick {
        match &SCRIPT_EVENTS[SCRIPT_INDEX].1 {
            ScriptedEvent::Key { scancode, down } => {
                let mut event: SDL_Event = core::mem::zeroed();
                event.key = SDL_KeyboardEvent {
                    type_: if *down { SDL_KEYDOWN } else { SDL_KEYUP },
                    timestamp: 0,
                    windowID: 0,
                    state: if *down { 1 } else { 0 },
                    repeat: 0,
                    padding2: 0,
                    padding3: 0,
                    keysym: SDL_Keysym { scancode: *scancode as u32, sym: 0, r#mod: 0, unused: 0 },
                };
                crate::platform::sdl::shared_renderer().push_event(&mut event as *mut SDL_Event as *mut c_void);
            }
            ScriptedEvent::MouseMove { x, y } => {
                crate::platform::sdl::shared_input().warp_mouse(*x, *y);
            }
            ScriptedEvent::MouseButton { button } => {
                let mut event: SDL_Event = core::mem::zeroed();
                event.button = SDL_MouseButtonEvent {
                    type_: SDL_MOUSEBUTTONDOWN,
                    timestamp: 0,
                    windowID: 0,
                    which: 0,
                    button: *button,
                    state: 1,
                    clicks: 1,
                    padding1: 0,
                    x: 0,
                    y: 0,
                };
                crate::platform::sdl::shared_renderer().push_event(&mut event as *mut SDL_Event as *mut c_void);
            }
        }
        SCRIPT_INDEX += 1;
    }
}

/// Drains SDL's event queue into the game's input globals.
///
/// This is the only door between the outside world and the game. It writes:
/// `last_key_scancode` (the one pending keystroke, modifier bits folded into
/// its high bits), `last_any_key_scancode`, `key_states` (held/newly-held bits
/// per scancode), `last_text_input`, the joystick axis and button arrays, and
/// the mouse flags the menu reads. Nothing here interprets those -- the
/// gameplay code polls them on its own schedule.
///
/// Called from [`idle`] and from every wait loop, so it runs many times per
/// game tick.
///
/// Three input families are handled in parallel because they are not
/// interchangeable: the GameController API for devices SDL has a mapping for,
/// the raw joystick API for those it does not (gated on
/// `using_sdl_joystick_interface`), and the keyboard. Keyboard and joystick
/// modes are mutually exclusive and each switches itself on when used.
// seg009 process_events
#[no_mangle]
pub unsafe extern "C" fn process_events() {
    inject_scripted_input();
    let mut event: SDL_Event = core::mem::zeroed();
    while crate::platform::sdl::shared_renderer().poll_event(&mut event as *mut SDL_Event as *mut c_void) == 1 {
        match event.type_ {
            x if x == SDL_KEYDOWN => 'kd: {
                let modifier = event.key.keysym.r#mod as c_int;
                let scancode = event.key.keysym.scancode as c_int;

                if scancode == SDL_SCANCODE_GRAVE {
                    init_timer(BASE_FPS * FAST_FORWARD_RATIO); // fast-forward on
                    audio_speed = FAST_FORWARD_RATIO;
                    break 'kd;
                }
                if scancode == SDL_SCANCODE_F12 {
                    if modifier & KMOD_SHIFT != 0 {
                        save_level_screenshot((modifier & KMOD_CTRL) != 0);
                    } else {
                        save_screenshot();
                    }
                } else if escape_key_suppressed
                    && (scancode == SDL_SCANCODE_BACKSPACE || (enable_pause_menu != 0 && scancode == SDL_SCANCODE_ESCAPE))
                {
                    break 'kd; // Prevent repeated keystrokes opening/closing the menu.
                } else if (modifier & KMOD_ALT) != 0 && scancode == SDL_SCANCODE_RETURN {
                    if (key_states[scancode as usize] as c_int & KEYSTATE_HELD as c_int) == 0 {
                        toggle_fullscreen();
                        key_states[scancode as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as byte;
                    }
                } else {
                    last_any_key_scancode = scancode;
                    key_states[scancode as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as byte;
                    match scancode {
                        SDL_SCANCODE_LCTRL
                        | SDL_SCANCODE_LSHIFT
                        | SDL_SCANCODE_LALT
                        | SDL_SCANCODE_LGUI
                        | SDL_SCANCODE_RCTRL
                        | SDL_SCANCODE_RSHIFT
                        | SDL_SCANCODE_RALT
                        | SDL_SCANCODE_RGUI
                        | SDL_SCANCODE_CAPSLOCK
                        | SDL_SCANCODE_SCROLLLOCK
                        | SDL_SCANCODE_NUMLOCKCLEAR
                        | SDL_SCANCODE_APPLICATION
                        | SDL_SCANCODE_PRINTSCREEN
                        | SDL_SCANCODE_VOLUMEUP
                        | SDL_SCANCODE_VOLUMEDOWN
                        | SDL_SCANCODE_MUTE
                        | SDL_SCANCODE_AUDIOMUTE
                        | SDL_SCANCODE_PAUSE => {}
                        _ => {
                            if scancode == SDL_SCANCODE_TAB && ignore_tab {
                                // ignore
                            } else {
                                last_key_scancode = scancode;
                                if modifier & KMOD_SHIFT != 0 {
                                    last_key_scancode |= key_modifiers_WITH_SHIFT as c_int;
                                }
                                if modifier & KMOD_CTRL != 0 {
                                    last_key_scancode |= key_modifiers_WITH_CTRL as c_int;
                                }
                                if modifier & KMOD_ALT != 0 {
                                    last_key_scancode |= key_modifiers_WITH_ALT as c_int;
                                }
                            }
                        }
                    }

                    // USE_AUTO_INPUT_MODE
                    match scancode {
                        SDL_SCANCODE_LSHIFT
                        | SDL_SCANCODE_RSHIFT
                        | SDL_SCANCODE_LEFT
                        | SDL_SCANCODE_RIGHT
                        | SDL_SCANCODE_UP
                        | SDL_SCANCODE_DOWN
                        | SDL_SCANCODE_CLEAR
                        | SDL_SCANCODE_HOME
                        | SDL_SCANCODE_PAGEUP
                        | SDL_SCANCODE_KP_2
                        | SDL_SCANCODE_KP_4
                        | SDL_SCANCODE_KP_5
                        | SDL_SCANCODE_KP_6
                        | SDL_SCANCODE_KP_7
                        | SDL_SCANCODE_KP_8
                        | SDL_SCANCODE_KP_9 => {
                            if is_keyboard_mode == 0 {
                                is_keyboard_mode = 1;
                                is_joyst_mode = 0;
                            }
                        }
                        _ => {}
                    }
                }
            }
            x if x == SDL_KEYUP => 'ku: {
                if event.key.keysym.scancode as c_int == SDL_SCANCODE_TAB && ignore_tab {
                    ignore_tab = false;
                }
                if event.key.keysym.scancode as c_int == SDL_SCANCODE_GRAVE {
                    init_timer(BASE_FPS); // fast-forward off
                    audio_speed = 1;
                    break 'ku;
                }
                key_states[event.key.keysym.scancode as usize] &= !(KEYSTATE_HELD as byte);
                if event.key.keysym.scancode as c_int == SDL_SCANCODE_BACKSPACE || event.key.keysym.scancode as c_int == SDL_SCANCODE_ESCAPE {
                    escape_key_suppressed = false;
                }
            }
            x if x == SDL_CONTROLLERAXISMOTION => {
                if (event.caxis.axis as c_int) < 6 {
                    joy_axis[event.caxis.axis as usize] = event.caxis.value as c_int;
                    if (event.caxis.value as c_int).abs() > joy_axis_max[event.caxis.axis as usize].abs() {
                        joy_axis_max[event.caxis.axis as usize] = event.caxis.value as c_int;
                    }
                    if is_joyst_mode == 0 && (event.caxis.value as c_int >= joystick_threshold || (event.caxis.value as c_int) <= -joystick_threshold) {
                        is_joyst_mode = 1;
                        is_keyboard_mode = 0;
                    }
                }
            }
            x if x == SDL_CONTROLLERDEVICEADDED => {
                crate::platform::sdl::shared_renderer().game_controller_open(event.cdevice.which);
                if gamecontrollerdb_file[0] != 0 {
                    SDL_GameControllerAddMappingsFromFile(gamecontrollerdb_file.as_ptr());
                }
                is_joyst_mode = 1;
                using_sdl_joystick_interface = 0;
            }
            x if x == SDL_CONTROLLERDEVICEREMOVED => {
                if sdl_controller_ == crate::platform::sdl::shared_renderer().game_controller_from_instance_id(event.cdevice.which) {
                    sdl_controller_ = null_mut();
                    is_joyst_mode = 0;
                    is_keyboard_mode = 1;
                }
                crate::platform::sdl::shared_renderer().game_controller_close(crate::platform::sdl::shared_renderer().game_controller_from_instance_id(event.cdevice.which));
            }
            x if x == SDL_CONTROLLERBUTTONDOWN => {
                sdl_controller_ = crate::platform::sdl::shared_renderer().game_controller_from_instance_id(event.cdevice.which);
                if is_joyst_mode == 0 {
                    is_joyst_mode = 1;
                    is_keyboard_mode = 0;
                }
                match event.cbutton.button {
                    SDL_CONTROLLER_BUTTON_DPAD_LEFT => {
                        joy_button_states[JOYINPUT_DPAD_LEFT as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    }
                    SDL_CONTROLLER_BUTTON_DPAD_RIGHT => {
                        joy_button_states[JOYINPUT_DPAD_RIGHT as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    }
                    SDL_CONTROLLER_BUTTON_DPAD_UP => {
                        joy_button_states[JOYINPUT_DPAD_UP as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    }
                    SDL_CONTROLLER_BUTTON_DPAD_DOWN => {
                        joy_button_states[JOYINPUT_DPAD_DOWN as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    }
                    SDL_CONTROLLER_BUTTON_A => {
                        joy_button_states[JOYINPUT_A as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    }
                    SDL_CONTROLLER_BUTTON_Y => {
                        joy_button_states[JOYINPUT_Y as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    }
                    SDL_CONTROLLER_BUTTON_X => {
                        joy_button_states[JOYINPUT_X as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    }
                    SDL_CONTROLLER_BUTTON_B => {
                        joy_button_states[JOYINPUT_B as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    }
                    SDL_CONTROLLER_BUTTON_START | SDL_CONTROLLER_BUTTON_BACK => {
                        if event.cbutton.button == SDL_CONTROLLER_BUTTON_START {
                            joy_button_states[JOYINPUT_START as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                        } else if event.cbutton.button == SDL_CONTROLLER_BUTTON_BACK {
                            joy_button_states[JOYINPUT_BACK as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                        }
                        last_key_scancode = SDL_SCANCODE_BACKSPACE; // bring up pause menu
                    }
                    _ => {}
                }
            }
            x if x == SDL_CONTROLLERBUTTONUP => match event.cbutton.button {
                SDL_CONTROLLER_BUTTON_DPAD_LEFT => {
                    joy_button_states[JOYINPUT_DPAD_LEFT as usize] &= !(KEYSTATE_HELD as c_int);
                }
                SDL_CONTROLLER_BUTTON_DPAD_RIGHT => {
                    joy_button_states[JOYINPUT_DPAD_RIGHT as usize] &= !(KEYSTATE_HELD as c_int);
                }
                SDL_CONTROLLER_BUTTON_DPAD_UP => {
                    joy_button_states[JOYINPUT_DPAD_UP as usize] &= !(KEYSTATE_HELD as c_int);
                }
                SDL_CONTROLLER_BUTTON_DPAD_DOWN => {
                    joy_button_states[JOYINPUT_DPAD_DOWN as usize] &= !(KEYSTATE_HELD as c_int);
                }
                SDL_CONTROLLER_BUTTON_A => {
                    joy_button_states[JOYINPUT_A as usize] &= !(KEYSTATE_HELD as c_int);
                }
                SDL_CONTROLLER_BUTTON_Y => {
                    joy_button_states[JOYINPUT_Y as usize] &= !(KEYSTATE_HELD as c_int);
                }
                SDL_CONTROLLER_BUTTON_X => {
                    joy_button_states[JOYINPUT_X as usize] &= !(KEYSTATE_HELD as c_int);
                }
                SDL_CONTROLLER_BUTTON_B => {
                    joy_button_states[JOYINPUT_B as usize] &= !(KEYSTATE_HELD as c_int);
                }
                SDL_CONTROLLER_BUTTON_START => {
                    joy_button_states[JOYINPUT_START as usize] &= !(KEYSTATE_HELD as c_int);
                }
                SDL_CONTROLLER_BUTTON_BACK => {
                    joy_button_states[JOYINPUT_BACK as usize] &= !(KEYSTATE_HELD as c_int);
                }
                _ => {}
            },
            x if x == SDL_JOYBUTTONDOWN || x == SDL_JOYBUTTONUP || x == SDL_JOYAXISMOTION => 'joy: {
                if using_sdl_joystick_interface == 0 {
                    break 'joy;
                }
                if event.type_ == SDL_JOYAXISMOTION {
                    let mut axis: c_int = -1;
                    if event.jaxis.axis == SDL_JOYSTICK_X_AXIS {
                        axis = SDL_CONTROLLER_AXIS_LEFTX;
                    } else if event.jaxis.axis == SDL_JOYSTICK_Y_AXIS {
                        axis = SDL_CONTROLLER_AXIS_LEFTY;
                    }
                    if axis == -1 {
                        break 'joy;
                    }
                    joy_axis[axis as usize] = event.jaxis.value as c_int;
                    if (event.jaxis.value as c_int).abs() > joy_axis_max[axis as usize].abs() {
                        joy_axis_max[axis as usize] = event.jaxis.value as c_int;
                    }
                    let joy_x = joy_axis[SDL_CONTROLLER_AXIS_LEFTX as usize];
                    let joy_y = joy_axis[SDL_CONTROLLER_AXIS_LEFTY as usize];
                    if ((joy_x.wrapping_mul(joy_x)) as u32).wrapping_add((joy_y.wrapping_mul(joy_y)) as u32)
                        < (joystick_threshold.wrapping_mul(joystick_threshold)) as u32
                    {
                        break 'joy;
                    }
                }
                if is_joyst_mode == 0 {
                    is_joyst_mode = 1;
                    is_keyboard_mode = 0;
                }
                if event.type_ == SDL_JOYBUTTONDOWN {
                    if event.jbutton.button == SDL_JOYSTICK_BUTTON_Y {
                        joy_button_states[JOYINPUT_Y as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    } else if event.jbutton.button == SDL_JOYSTICK_BUTTON_X {
                        joy_button_states[JOYINPUT_X as usize] |= (KEYSTATE_HELD | KEYSTATE_HELD_NEW) as c_int;
                    }
                } else if event.type_ == SDL_JOYBUTTONUP {
                    if event.jbutton.button == SDL_JOYSTICK_BUTTON_Y {
                        joy_button_states[JOYINPUT_Y as usize] &= !(KEYSTATE_HELD as c_int);
                    } else if event.jbutton.button == SDL_JOYSTICK_BUTTON_X {
                        joy_button_states[JOYINPUT_X as usize] &= !(KEYSTATE_HELD as c_int);
                    }
                }
            }
            x if x == SDL_TEXTINPUT => {
                last_text_input = event.text.text[0]; // UTF-8 formatted char text input
                match last_text_input as u8 {
                    b'-' => {
                        last_key_scancode = SDL_SCANCODE_KP_MINUS;
                    }
                    b'+' => {
                        last_key_scancode = SDL_SCANCODE_KP_PLUS;
                    }
                    _ => {}
                }
            }
            x if x == SDL_WINDOWEVENT => {
                if event.window.event == SDL_WINDOWEVENT_SIZE_CHANGED {
                    window_resized();
                    update_screen();
                } else if event.window.event == SDL_WINDOWEVENT_EXPOSED {
                    update_screen();
                } else if event.window.event == SDL_WINDOWEVENT_FOCUS_GAINED {
                    // If Alt is held down from Alt+Tab: ignore it until it's released.
                    if crate::platform::sdl::shared_input().key_state(SDL_SCANCODE_TAB) {
                        ignore_tab = true;
                    }
                }
            }
            x if x == SDL_USEREVENT => {
                if event.user.code == userevent_TIMER {
                    // USE_COMPAT_TIMER off: nothing
                } else if event.user.code == userevent_SOUND {
                    // nothing
                }
            }
            x if x == SDL_MOUSEBUTTONDOWN => match event.button.button {
                SDL_BUTTON_LEFT => {
                    if is_menu_shown == 0 {
                        last_key_scancode = SDL_SCANCODE_BACKSPACE;
                    } else {
                        mouse_clicked = true;
                    }
                }
                SDL_BUTTON_RIGHT | SDL_BUTTON_X1 => {
                    mouse_button_clicked_right = true;
                }
                _ => {}
            },
            x if x == SDL_MOUSEWHEEL => {
                if is_menu_shown != 0 {
                    menu_control_scroll_y = -event.wheel.y;
                }
            }
            x if x == SDL_QUIT => {
                if is_menu_shown != 0 {
                    menu_was_closed();
                }
                quit(0);
            }
            _ => {}
        }
    }
}

/// One turn of the "nothing to do" loop: pump input, redraw.
// seg009 idle
#[no_mangle]
pub unsafe extern "C" fn idle() {
    process_events();
    update_screen();
}

/// Blocks until the timer expires, keeping input and rendering alive.
///
/// Returns immediately when there is no real time to spend: fast-forwarding a
/// replay, or validating headlessly.
// seg009 do_simple_wait
#[no_mangle]
pub unsafe extern "C" fn do_simple_wait(timer_index: c_int) {
    if (replaying != 0 && skipping_replay != 0) || is_validate_mode != 0 {
        return;
    }
    update_screen();
    while has_timer_stopped(timer_index) == 0 {
        crate::platform::sdl::shared_renderer().delay(1);
        process_events();
    }
}

/// [`do_simple_wait`] that can also be interrupted by a keystroke.
///
/// Returns 1 if the wait was cut short by a key, 0 if the timer ran out. The
/// `word_1D63A` flag decides whether *any* key interrupts or only Escape
/// (`0x1B`) -- it is what makes cutscenes skippable but some prompts not.
// seg009 do_wait
#[no_mangle]
pub unsafe extern "C" fn do_wait(timer_index: c_int) -> c_int {
    if (replaying != 0 && skipping_replay != 0) || is_validate_mode != 0 {
        return 0;
    }
    update_screen();
    while has_timer_stopped(timer_index) == 0 {
        crate::platform::sdl::shared_renderer().delay(1);
        process_events();
        let key = do_paused();
        if key != 0 && (word_1D63A != 0 || key == 0x1B) {
            return 1;
        }
    }
    0
}

/// Sets the global tick rate and recomputes the performance-counter
/// conversions.
///
/// Called at startup with `BASE_FPS`, and by the fast-forward key with
/// `BASE_FPS * FAST_FORWARD_RATIO`.
// seg009:78E9 init_timer
#[no_mangle]
pub unsafe extern "C" fn init_timer(frequency: c_int) {
    perf_frequency = crate::platform::sdl::shared_renderer().performance_frequency();
    fps = frequency;
    milliseconds_per_tick = 1000.0f32 / fps as f32;
    perf_counters_per_tick = perf_frequency / fps as u64;
    milliseconds_per_counter = 1000.0f32 / perf_frequency as f32;
}

/// Restricts drawing on the current target to `rect`.
// seg009:35F6 set_clip_rect
#[no_mangle]
pub unsafe extern "C" fn set_clip_rect(rect: *const rect_type) {
    let mut clip_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(rect, &mut clip_rect);
    crate::platform::sdl::shared_renderer().set_clip_rect(current_target_surface, &clip_rect);
}

/// Removes the clip rect from the current target.
// seg009:365C reset_clip_rect
#[no_mangle]
pub unsafe extern "C" fn reset_clip_rect() {
    crate::platform::sdl::shared_renderer().set_clip_rect(current_target_surface, core::ptr::null());
}

/// Draws the screen-flash effect (USE_FLASH): the hit-taken red, the
/// potion-drunk white.
///
/// Implemented as "clear the screen to the flash colour, then blit the frame
/// over it with black made transparent", so the flash shows through wherever
/// the frame is black -- which reproduces what the original got for free by
/// reprogramming the VGA border/background attribute. Only `vga_pal_index == 0`
/// is implemented; other values are no-ops.
// seg009:1983 set_bg_attr
#[no_mangle]
pub unsafe extern "C" fn set_bg_attr(vga_pal_index: c_int, hc_pal_index: c_int) {
    if enable_flash == 0 {
        return;
    }
    if vga_pal_index == 0 {
        // Make the black pixels transparent.
        if crate::platform::sdl::shared_renderer().set_color_key(offscreen_surface, true, 0) != 0 {
            sdlperror(cs!("set_bg_attr: SDL_SetColorKey"));
            quit(1);
        }
        let mut rect = SDL_Rect { x: 0, y: 0, w: 0, h: 0 };
        let (offscreen_w, offscreen_h) = crate::platform::sdl::shared_renderer().surface_size(offscreen_surface);
        rect.w = offscreen_w;
        rect.h = offscreen_h;
        let (pr, pg, pb) = palette_rgb8(hc_pal_index as byte);
        let rgb_color: u32 = crate::platform::sdl::shared_renderer().map_rgb(crate::platform::sdl::shared_renderer().surface_format_ptr(onscreen_surface_), pr, pg, pb);
        // First clear the screen with the color of the flash.
        if safe_fill_rect(onscreen_surface_, &rect, rgb_color) != 0 {
            sdlperror(cs!("set_bg_attr: SDL_FillRect"));
            quit(1);
        }
        if upside_down != 0 {
            flip_screen(offscreen_surface);
        }
        // Then draw the offscreen image onto it.
        let rp = &mut rect as *mut SDL_Rect;
        if SDL_BlitSurface(offscreen_surface, rp, onscreen_surface_, rp) != 0 {
            sdlperror(cs!("set_bg_attr: SDL_BlitSurface"));
            quit(1);
        }
        if hc_pal_index == 0 {
            update_lighting(core::ptr::addr_of!(rect_top));
        }
        if upside_down != 0 {
            flip_screen(offscreen_surface);
        }
        if crate::platform::sdl::shared_renderer().set_color_key(offscreen_surface, false, 0) != 0 {
            sdlperror(cs!("set_bg_attr: SDL_SetColorKey"));
            quit(1);
        }
    }
}

/// Copies a rect, adding an independent delta to each of its four edges.
// seg009:07EB offset4_rect_add
#[no_mangle]
pub unsafe extern "C" fn offset4_rect_add(dest: *mut rect_type, source: *const rect_type, d_left: c_int, d_top: c_int, d_right: c_int, d_bottom: c_int) -> *mut rect_type {
    *dest = *source;
    (*dest).left = ((*dest).left as c_int + d_left) as c_short;
    (*dest).top = ((*dest).top as c_int + d_top) as c_short;
    (*dest).right = ((*dest).right as c_int + d_right) as c_short;
    (*dest).bottom = ((*dest).bottom as c_int + d_bottom) as c_short;
    dest
}

/// Copies a rect, translated by `(delta_x, delta_y)`.
// seg009:3AA5 offset2_rect
#[no_mangle]
pub unsafe extern "C" fn offset2_rect(dest: *mut rect_type, source: *const rect_type, delta_x: c_int, delta_y: c_int) -> *mut rect_type {
    (*dest).top = ((*source).top as c_int + delta_y) as c_short;
    (*dest).left = ((*source).left as c_int + delta_x) as c_short;
    (*dest).bottom = ((*source).bottom as c_int + delta_y) as c_short;
    (*dest).right = ((*source).right as c_int + delta_x) as c_short;
    dest
}

// ============================================================================
// Fades (USE_FADE)
//
// A fade runs as 0x40 frames, each advancing `fade_pos` by one, and works on
// two levels at once: the palette entries are ramped toward (or away from)
// their original values, *and* the framebuffer pixels are darkened by
// `fade_pos * 4`. The palette half is what the original did; the pixel half is
// needed because SDLPoP's onscreen surface is truecolour and no longer changes
// when the palette does.
//
// `which_rows` is a 16-bit mask selecting which palette rows participate, which
// is how a cutscene can fade its background while leaving the text visible.
//
// A fade is driven by the caller: `make_pal_buffer_*` sets it up,
// `fade_*_frame` is called until it returns nonzero, `pal_restore_free_*`
// cleans up. The buffer also carries function pointers to its own frame and
// cleanup routines so a generic caller can drive either direction.
// ============================================================================

/// The palette rows a fade's `which_rows` mask selects, as index ranges into
/// the 256-entry palette.
///
/// Each of the 16 rows covers 16 consecutive palette entries, so row *n*
/// (selected by bit *n*) is `n * 16 .. n * 16 + 16`.
unsafe fn selected_pal_rows(
    palette_buffer: *const palette_fade_type,
) -> impl Iterator<Item = core::ops::Range<usize>> {
    let which_rows = (*palette_buffer).which_rows;
    (0..0x10usize)
        .filter(move |row| which_rows & (1u16 << row) != 0)
        .map(|row| (row << 4)..((row << 4) + 0x10))
}

/// Fades the screen in from black, blocking until done.
// seg009:19EF fade_in_2
#[no_mangle]
pub unsafe extern "C" fn fade_in_2(source_surface: *mut surface_type, which_rows: c_int) {
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int {
        let palette_buffer = make_pal_buffer_fadein(source_surface, which_rows, 2);
        while fade_in_frame(palette_buffer) == 0 {
            process_events();
            do_paused();
        }
        pal_restore_free_fadein(palette_buffer);
    }
}

/// Sets up a fade-in: saves the target palette and blanks the selected rows.
// seg009:1A51 make_pal_buffer_fadein
#[no_mangle]
pub unsafe extern "C" fn make_pal_buffer_fadein(_source_surface: *mut surface_type, which_rows: c_int, wait_time_arg: c_int) -> *mut palette_fade_type {
    let palette_buffer = malloc(core::mem::size_of::<palette_fade_type>()) as *mut palette_fade_type;
    (*palette_buffer).which_rows = which_rows as word;
    (*palette_buffer).wait_time = wait_time_arg as word;
    (*palette_buffer).fade_pos = 0x40;
    (*palette_buffer).proc_restore_free = Some(pal_restore_free_fadein);
    (*palette_buffer).proc_fade_frame = Some(fade_in_frame);
    read_palette_256(core::ptr::addr_of_mut!((*palette_buffer).original_pal) as *mut rgb_type);
    let faded = core::ptr::addr_of_mut!((*palette_buffer).faded_pal) as *mut rgb_type;
    let orig = core::ptr::addr_of!((*palette_buffer).original_pal) as *const rgb_type;
    memcpy(faded as *mut c_void, orig as *const c_void, core::mem::size_of::<[rgb_type; 256]>());
    for curr_row in 0..0x10usize {
        if which_rows & (1 << curr_row) != 0 {
            memset(faded.add(curr_row << 4) as *mut c_void, 0, core::mem::size_of::<[rgb_type; 0x10]>());
            set_pal_arr((curr_row as c_int) << 4, 0x10, core::ptr::null());
        }
    }
    palette_buffer
}

/// Finishes a fade-in: restores the real palette and repaints from the
/// offscreen buffer.
// seg009:1B64 pal_restore_free_fadein
#[no_mangle]
pub unsafe extern "C" fn pal_restore_free_fadein(palette_buffer: *mut palette_fade_type) {
    set_pal_256(core::ptr::addr_of_mut!((*palette_buffer).original_pal) as *mut rgb_type);
    free(palette_buffer as *mut c_void);
    method_1_blit_rect(onscreen_surface_, offscreen_surface, core::ptr::addr_of!(screen_rect), core::ptr::addr_of!(screen_rect), 0);
}

/// Advances a fade-in by one step; returns nonzero once it has finished.
///
/// Each selected palette component is nudged up by one if it has not yet
/// reached its target, and the framebuffer is rebuilt from the offscreen copy
/// darkened by the current `fade_pos`. Completion is by counter, not by
/// convergence: `fade_pos` starts at 0x40 and the fade ends when it hits 0.
// seg009:1B88 fade_in_frame
#[no_mangle]
pub unsafe extern "C" fn fade_in_frame(palette_buffer: *mut palette_fade_type) -> c_int {
    start_timer(timerids_timer_1 as c_int, (*palette_buffer).wait_time as c_int);

    (*palette_buffer).fade_pos = (*palette_buffer).fade_pos.wrapping_sub(1);
    let fade_pos = (*palette_buffer).fade_pos as c_int;
    for row in selected_pal_rows(palette_buffer) {
        let original_pal_ptr = (core::ptr::addr_of!((*palette_buffer).original_pal) as *const rgb_type).add(row.start);
        let faded_pal_ptr = (core::ptr::addr_of_mut!((*palette_buffer).faded_pal) as *mut rgb_type).add(row.start);
        for column in 0..0x10usize {
            let original = original_pal_ptr.add(column);
            let faded = faded_pal_ptr.add(column);
            if (*original).r as c_int > fade_pos {
                (*faded).r = (*faded).r.wrapping_add(1);
            }
            if (*original).g as c_int > fade_pos {
                (*faded).g = (*faded).g.wrapping_add(1);
            }
            if (*original).b as c_int > fade_pos {
                (*faded).b = (*faded).b.wrapping_add(1);
            }
        }
    }
    for row in selected_pal_rows(palette_buffer) {
        set_pal_arr(row.start as c_int, 0x10, (core::ptr::addr_of!((*palette_buffer).faded_pal) as *const rgb_type).add(row.start));
    }

    let h = crate::platform::sdl::shared_renderer().surface_size(offscreen_surface).1;
    if crate::platform::sdl::shared_renderer().lock_surface(onscreen_surface_) != 0 {
        sdlperror(cs!("fade_in_frame: SDL_LockSurface"));
        quit(1);
    }
    if crate::platform::sdl::shared_renderer().lock_surface(offscreen_surface) != 0 {
        sdlperror(cs!("fade_in_frame: SDL_LockSurface"));
        quit(1);
    }
    let on_stride = crate::platform::sdl::shared_renderer().surface_pitch(onscreen_surface_);
    let off_stride = crate::platform::sdl::shared_renderer().surface_pitch(offscreen_surface);
    let on_pixels = crate::platform::sdl::shared_renderer().surface_pixels(onscreen_surface_) as *mut byte;
    let off_pixels = crate::platform::sdl::shared_renderer().surface_pixels(offscreen_surface) as *mut byte;
    let fade_pos = (*palette_buffer).fade_pos as c_int;
    for y in 0..h {
        let mut on_pixel_ptr = on_pixels.offset((on_stride * y) as isize);
        let mut off_pixel_ptr = off_pixels.offset((off_stride * y) as isize);
        for _x in 0..on_stride {
            let mut v = *off_pixel_ptr as c_int - fade_pos * 4;
            if v < 0 {
                v = 0;
            }
            *on_pixel_ptr = v as byte;
            on_pixel_ptr = on_pixel_ptr.add(1);
            off_pixel_ptr = off_pixel_ptr.add(1);
        }
    }
    crate::platform::sdl::shared_renderer().unlock_surface(onscreen_surface_);
    crate::platform::sdl::shared_renderer().unlock_surface(offscreen_surface);

    do_simple_wait(1); // can interrupt fading of cutscene
    ((*palette_buffer).fade_pos == 0) as c_int
}

/// Fades the screen out to black, blocking until done.
// seg009:1CC9 fade_out_2
#[no_mangle]
pub unsafe extern "C" fn fade_out_2(rows: c_int) {
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int {
        let palette_buffer = make_pal_buffer_fadeout(rows, 2);
        while fade_out_frame(palette_buffer) == 0 {
            process_events();
            do_paused();
        }
        pal_restore_free_fadeout(palette_buffer);
    }
}

/// Sets up a fade-out: saves the current palette and snapshots the screen into
/// the offscreen buffer, which every frame then re-darkens.
// seg009:1D28 make_pal_buffer_fadeout
#[no_mangle]
pub unsafe extern "C" fn make_pal_buffer_fadeout(which_rows: c_int, wait_time_arg: c_int) -> *mut palette_fade_type {
    let palette_buffer = malloc(core::mem::size_of::<palette_fade_type>()) as *mut palette_fade_type;
    (*palette_buffer).which_rows = which_rows as word;
    (*palette_buffer).wait_time = wait_time_arg as word;
    (*palette_buffer).fade_pos = 0;
    (*palette_buffer).proc_restore_free = Some(pal_restore_free_fadeout);
    (*palette_buffer).proc_fade_frame = Some(fade_out_frame);
    read_palette_256(core::ptr::addr_of_mut!((*palette_buffer).original_pal) as *mut rgb_type);
    let faded = core::ptr::addr_of_mut!((*palette_buffer).faded_pal) as *mut rgb_type;
    let orig = core::ptr::addr_of!((*palette_buffer).original_pal) as *const rgb_type;
    memcpy(faded as *mut c_void, orig as *const c_void, core::mem::size_of::<[rgb_type; 256]>());
    method_1_blit_rect(onscreen_surface_, offscreen_surface, core::ptr::addr_of!(screen_rect), core::ptr::addr_of!(screen_rect), 0);
    palette_buffer
}

/// Finishes a fade-out: blacks out both surfaces and restores the real palette.
// seg009:1DAF pal_restore_free_fadeout
#[no_mangle]
pub unsafe extern "C" fn pal_restore_free_fadeout(palette_buffer: *mut palette_fade_type) {
    let surface = current_target_surface;
    current_target_surface = onscreen_surface_;
    draw_rect(core::ptr::addr_of!(screen_rect), colorids_color_0_black as c_int);
    current_target_surface = surface;
    set_pal_256(core::ptr::addr_of_mut!((*palette_buffer).original_pal) as *mut rgb_type);
    free(palette_buffer as *mut c_void);
    method_5_rect(core::ptr::addr_of!(screen_rect), 0, colorids_color_0_black as byte);
}

/// Advances a fade-out by one step; returns nonzero once it has finished.
///
/// Mirror of [`fade_in_frame`], except completion is by *convergence*: it
/// reports done when no selected palette component was still above zero this
/// step. `fade_pos` here only drives the pixel darkening.
// seg009:1DF7 fade_out_frame
#[no_mangle]
pub unsafe extern "C" fn fade_out_frame(palette_buffer: *mut palette_fade_type) -> c_int {
    let mut finished_fading: word = 1;
    (*palette_buffer).fade_pos = (*palette_buffer).fade_pos.wrapping_add(1);
    start_timer(timerids_timer_1 as c_int, (*palette_buffer).wait_time as c_int);
    for row in selected_pal_rows(palette_buffer) {
        let faded_pal_ptr = (core::ptr::addr_of_mut!((*palette_buffer).faded_pal) as *mut rgb_type).add(row.start);
        for column in 0..0x10usize {
            let curr = faded_pal_ptr.add(column);
            if (*curr).r != 0 {
                (*curr).r = (*curr).r.wrapping_sub(1);
                finished_fading = 0;
            }
            if (*curr).g != 0 {
                (*curr).g = (*curr).g.wrapping_sub(1);
                finished_fading = 0;
            }
            if (*curr).b != 0 {
                (*curr).b = (*curr).b.wrapping_sub(1);
                finished_fading = 0;
            }
        }
    }
    for row in selected_pal_rows(palette_buffer) {
        set_pal_arr(row.start as c_int, 0x10, (core::ptr::addr_of!((*palette_buffer).faded_pal) as *const rgb_type).add(row.start));
    }

    let h = crate::platform::sdl::shared_renderer().surface_size(offscreen_surface).1;
    if crate::platform::sdl::shared_renderer().lock_surface(onscreen_surface_) != 0 {
        sdlperror(cs!("fade_out_frame: SDL_LockSurface"));
        quit(1);
    }
    if crate::platform::sdl::shared_renderer().lock_surface(offscreen_surface) != 0 {
        sdlperror(cs!("fade_out_frame: SDL_LockSurface"));
        quit(1);
    }
    let on_stride = crate::platform::sdl::shared_renderer().surface_pitch(onscreen_surface_);
    let off_stride = crate::platform::sdl::shared_renderer().surface_pitch(offscreen_surface);
    let on_pixels = crate::platform::sdl::shared_renderer().surface_pixels(onscreen_surface_) as *mut byte;
    let off_pixels = crate::platform::sdl::shared_renderer().surface_pixels(offscreen_surface) as *mut byte;
    let fade_pos = (*palette_buffer).fade_pos as c_int;
    for y in 0..h {
        let mut on_pixel_ptr = on_pixels.offset((on_stride * y) as isize);
        let mut off_pixel_ptr = off_pixels.offset((off_stride * y) as isize);
        for _x in 0..on_stride {
            let mut v = *off_pixel_ptr as c_int - fade_pos * 4;
            if v < 0 {
                v = 0;
            }
            *on_pixel_ptr = v as byte;
            on_pixel_ptr = on_pixel_ptr.add(1);
            off_pixel_ptr = off_pixel_ptr.add(1);
        }
    }
    crate::platform::sdl::shared_renderer().unlock_surface(onscreen_surface_);
    crate::platform::sdl::shared_renderer().unlock_surface(offscreen_surface);

    do_simple_wait(timerids_timer_1 as c_int); // can interrupt fading of cutscene
    finished_fading as c_int
}

/// Snapshots the whole 256-entry palette into `target`.
// seg009:1F28 read_palette_256
#[no_mangle]
pub unsafe extern "C" fn read_palette_256(target: *mut rgb_type) {
    for i in 0..256usize {
        *target.add(i) = palette[i];
    }
}

/// Restores the whole 256-entry palette from `source`.
// seg009:1F5E set_pal_256
#[no_mangle]
pub unsafe extern "C" fn set_pal_256(source: *mut rgb_type) {
    for i in 0..256usize {
        palette[i] = *source.add(i);
    }
}

/// Repaints an entire sprite sheet by swapping the palette of each of its
/// surfaces.
///
/// This is how one set of guard sprites yields the differently-coloured guards
/// of each level: `colors` is a flat run of `n_colors` RGB triples in 6-bit VGA
/// components. Each image's palette may be shorter than `n_colors`, so the
/// count is clamped per image.
// seg009 set_chtab_palette
#[no_mangle]
pub unsafe extern "C" fn set_chtab_palette(chtab: *mut chtab_type, mut colors: *mut byte, n_colors: c_int) {
    if chtab.is_null() {
        return;
    }
    let scolors = malloc(n_colors as usize * core::mem::size_of::<SDL_Color>()) as *mut SDL_Color;
    for i in 0..n_colors as isize {
        let mut next = || {
            let component = ((*colors as c_int) << 2) as u8;
            colors = colors.add(1);
            component
        };
        *scolors.offset(i) = SDL_Color { r: next(), g: next(), b: next(), a: SDL_ALPHA_OPAQUE };
    }
    // Color 0 of the palette data is not used; replaced by the background color.
    *scolors = SDL_Color { r: 0, g: 0, b: 0, a: SDL_ALPHA_TRANSPARENT };

    let images = core::ptr::addr_of!((*chtab).images) as *const *mut image_type;
    for i in 0..(*chtab).n_images as isize {
        let current_image = *images.offset(i);
        if current_image.is_null() {
            continue;
        }
        let current_palette = crate::platform::sdl::shared_renderer().surface_palette(current_image);
        if current_palette.is_null() {
            continue;
        }
        let n_colors_to_be_set = n_colors.min((*current_palette).ncolors);
        if crate::platform::sdl::shared_renderer().set_palette_colors(current_palette, scolors, 0, n_colors_to_be_set) != 0 {
            sdlperror(cs!("set_chtab_palette: SDL_SetPaletteColors"));
            quit(1);
        }
    }
    free(scolors as *mut c_void);
}

/// True once the timer's length has elapsed; also rearms it for the next
/// interval.
///
/// Rearming keeps the *phase*: instead of restarting from now, the new start
/// counter is backed up by however far this call overshot, so a run of intervals
/// does not accumulate drift. The correction is capped at 3 ticks, because a
/// larger overshoot means the game genuinely stalled (a breakpoint, a slow
/// frame) and pretending otherwise would make it try to catch up.
///
/// Always true when fast-forwarding a replay or validating headlessly, so the
/// game never actually waits.
// seg009 has_timer_stopped
#[no_mangle]
pub unsafe extern "C" fn has_timer_stopped(index: c_int) -> c_int {
    if (replaying != 0 && skipping_replay != 0) || is_validate_mode != 0 {
        return 1;
    }
    let mut current_counter = crate::platform::sdl::shared_renderer().performance_counter();
    let ticks_elapsed = ((current_counter / perf_counters_per_tick) - (timer_last_counter[index as usize] / perf_counters_per_tick)) as c_int;
    let overshoot = ticks_elapsed - wait_time[index as usize];
    if overshoot >= 0 {
        if overshoot > 0 && overshoot <= 3 {
            current_counter -= overshoot as u64 * perf_counters_per_tick;
        }
        timer_last_counter[index as usize] = current_counter;
        1
    } else {
        0
    }
}

// PORT_END
