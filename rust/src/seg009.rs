// Platform layer — ported from seg009.c.
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
    fn snprintf(buf: *mut c_char, size: usize, fmt: *const c_char, ...) -> c_int;
    fn printf(fmt: *const c_char, ...) -> c_int;
    fn fprintf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    fn puts(s: *const c_char) -> c_int;
    fn fscanf(stream: *mut FILE, fmt: *const c_char, ...) -> c_int;
    fn feof(stream: *mut FILE) -> c_int;
    fn fileno(stream: *mut FILE) -> c_int;
    fn time(t: *mut c_long) -> c_long;
    fn exit(code: c_int) -> !;
    fn access(path: *const c_char, mode: c_int) -> c_int;
    fn stat(path: *const c_char, buf: *mut stat_t) -> c_int;
    fn fstat(fd: c_int, buf: *mut stat_t) -> c_int;
    fn __errno_location() -> *mut c_int;
    static mut stderr: *mut FILE;
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
// SDL functions
// ============================================================================
extern "C" {
    fn SDL_GetError() -> *const c_char;
    fn SDL_Quit();
    fn SDL_Init(flags: u32) -> c_int;
    fn SDL_InitSubSystem(flags: u32) -> c_int;
    fn SDL_NumJoysticks() -> c_int;
    fn SDL_GameControllerAddMappingsFromRW(rw: *mut SDL_RWops, freesrc: c_int) -> c_int;
    fn SDL_RWFromFile(file: *const c_char, mode: *const c_char) -> *mut SDL_RWops;
    fn SDL_IsGameController(joystick_index: c_int) -> c_int;
    fn SDL_GameControllerOpen(joystick_index: c_int) -> *mut SDL_GameController;
    fn SDL_GameControllerClose(gamecontroller: *mut SDL_GameController);
    fn SDL_GameControllerFromInstanceID(joyid: i32) -> *mut SDL_GameController;
    fn SDL_JoystickOpen(device_index: c_int) -> *mut SDL_Joystick;
    fn SDL_HapticOpen(device_index: c_int) -> *mut SDL_Haptic;
    fn SDL_HapticRumbleInit(haptic: *mut SDL_Haptic) -> c_int;
    fn SDL_CreateRGBSurface(flags: u32, width: c_int, height: c_int, depth: c_int,
                            rmask: u32, gmask: u32, bmask: u32, amask: u32) -> *mut SDL_Surface;
    fn SDL_FreeSurface(surface: *mut SDL_Surface);
    fn SDL_LockSurface(surface: *mut SDL_Surface) -> c_int;
    fn SDL_UnlockSurface(surface: *mut SDL_Surface);
    fn SDL_SetColorKey(surface: *mut SDL_Surface, flag: c_int, key: u32) -> c_int;
    fn SDL_SetPaletteColors(palette: *mut SDL_Palette, colors: *const SDL_Color,
                            firstcolor: c_int, ncolors: c_int) -> c_int;
    fn SDL_RWFromConstMem(mem: *const c_void, size: c_int) -> *mut SDL_RWops;
    fn SDL_RWclose(context: *mut SDL_RWops) -> c_int;
    fn IMG_Load_RW(src: *mut SDL_RWops, freesrc: c_int) -> *mut SDL_Surface;
    fn IMG_Load(file: *const c_char) -> *mut SDL_Surface;
    fn SDL_ConvertSurfaceFormat(src: *mut SDL_Surface, pixel_format: u32, flags: u32) -> *mut SDL_Surface;
    fn SDL_ConvertSurface(src: *mut SDL_Surface, fmt: *const SDL_PixelFormat, flags: u32) -> *mut SDL_Surface;
    fn SDL_SetSurfaceBlendMode(surface: *mut SDL_Surface, blend_mode: c_int) -> c_int;
    fn SDL_SetSurfaceAlphaMod(surface: *mut SDL_Surface, alpha: u8) -> c_int;
    fn SDL_MapRGB(format: *const SDL_PixelFormat, r: u8, g: u8, b: u8) -> u32;
    fn SDL_MapRGBA(format: *const SDL_PixelFormat, r: u8, g: u8, b: u8, a: u8) -> u32;
    fn SDL_UpperBlit(src: *mut SDL_Surface, srcrect: *const SDL_Rect,
                     dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int;
    fn SDL_UpperBlitScaled(src: *mut SDL_Surface, srcrect: *const SDL_Rect,
                           dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int;
    fn SDL_FillRect(dst: *mut SDL_Surface, rect: *const SDL_Rect, color: u32) -> c_int;
    fn SDL_SetClipRect(surface: *mut SDL_Surface, rect: *const SDL_Rect) -> c_int;
    fn SDL_GetVersion(ver: *mut SDL_version);
    fn SDL_OpenAudio(desired: *mut SDL_AudioSpec, obtained: *mut SDL_AudioSpec) -> c_int;
    fn SDL_PauseAudio(pause_on: c_int);
    fn SDL_LockAudio();
    fn SDL_UnlockAudio();
    fn SDL_PushEvent(event: *mut SDL_Event) -> c_int;
    fn SDL_PollEvent(event: *mut SDL_Event) -> c_int;
    fn SDL_Delay(ms: u32);
    fn SDL_GetPerformanceCounter() -> u64;
    fn SDL_GetPerformanceFrequency() -> u64;
    fn SDL_RenderSetLogicalSize(renderer: *mut SDL_Renderer, w: c_int, h: c_int) -> c_int;
    fn SDL_GetRendererOutputSize(renderer: *mut SDL_Renderer, w: *mut c_int, h: *mut c_int) -> c_int;
    fn SDL_RenderGetLogicalSize(renderer: *mut SDL_Renderer, w: *mut c_int, h: *mut c_int);
    fn SDL_RenderSetIntegerScale(renderer: *mut SDL_Renderer, enable: c_int) -> c_int;
    fn SDL_CreateTexture(renderer: *mut SDL_Renderer, format: u32, access: c_int, w: c_int, h: c_int) -> *mut SDL_Texture;
    fn SDL_SetHint(name: *const c_char, value: *const c_char) -> c_int;
    fn SDL_CreateWindow(title: *const c_char, x: c_int, y: c_int, w: c_int, h: c_int, flags: u32) -> *mut SDL_Window;
    fn SDL_CreateRenderer(window: *mut SDL_Window, index: c_int, flags: u32) -> *mut SDL_Renderer;
    fn SDL_GetRendererInfo(renderer: *mut SDL_Renderer, info: *mut SDL_RendererInfo) -> c_int;
    fn SDL_SetWindowIcon(window: *mut SDL_Window, icon: *mut SDL_Surface);
    fn SDL_ShowCursor(toggle: c_int) -> c_int;
    fn SDL_UpdateTexture(texture: *mut SDL_Texture, rect: *const SDL_Rect, pixels: *const c_void, pitch: c_int) -> c_int;
    fn SDL_SetRenderTarget(renderer: *mut SDL_Renderer, texture: *mut SDL_Texture) -> c_int;
    fn SDL_RenderClear(renderer: *mut SDL_Renderer) -> c_int;
    fn SDL_RenderCopy(renderer: *mut SDL_Renderer, texture: *mut SDL_Texture, srcrect: *const SDL_Rect, dstrect: *const SDL_Rect) -> c_int;
    fn SDL_RenderPresent(renderer: *mut SDL_Renderer);
    fn SDL_GetWindowFlags(window: *mut SDL_Window) -> u32;
    fn SDL_SetWindowFullscreen(window: *mut SDL_Window, flags: u32) -> c_int;
    fn SDL_GetKeyboardState(numkeys: *mut c_int) -> *const u8;
    fn SDL_SetTextInputRect(rect: *mut SDL_Rect);
    fn SDL_StartTextInput();
    fn SDL_StopTextInput();
}

// Defined in menu.c (still compiled as C); not in proto.h.
extern "C" {
    static mut hc_small_font_data: [u8; 0];
}

// SDL_BlitSurface and SDL_BlitScaled are macros for SDL_UpperBlit / SDL_UpperBlitScaled.
#[inline]
unsafe fn SDL_BlitSurface(src: *mut SDL_Surface, srcrect: *const SDL_Rect,
                          dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int {
    SDL_UpperBlit(src, srcrect, dst, dstrect)
}
#[inline]
unsafe fn SDL_BlitScaled(src: *mut SDL_Surface, srcrect: *const SDL_Rect,
                         dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int {
    SDL_UpperBlitScaled(src, srcrect, dst, dstrect)
}

// IMG_GetError is a macro for SDL_GetError.
#[inline]
unsafe fn IMG_GetError() -> *const c_char {
    SDL_GetError()
}
// SDL_GameControllerAddMappingsFromFile is a macro.
#[inline]
unsafe fn SDL_GameControllerAddMappingsFromFile(file: *const c_char) -> c_int {
    SDL_GameControllerAddMappingsFromRW(SDL_RWFromFile(file, b"rb\0".as_ptr() as *const c_char), 1)
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

// snprintf_check macro (from common.h). On truncation: print and quit(2).
macro_rules! snprintf_check {
    ($dst:expr, $size:expr, $fmt:expr $(, $arg:expr)* $(,)?) => {{
        let __len = snprintf($dst, $size as usize, $fmt $(, $arg)*);
        if __len < 0 || __len >= ($size as c_int) {
            fprintf(stderr, cs!("%s: buffer truncation detected!\n"), cs!("seg009"));
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

static mut dat_chain_ptr: *mut dat_type = null_mut();
static mut last_text_input: c_char = 0;

static mut chtab_palette_bits: word = 1;

static mut palette: [rgb_type; 256] = [rgb_type { r: 0, g: 0, b: 0 }; 256];

static mut speaker_playing: c_short = 0;
static mut digi_playing: c_short = 0;
#[no_mangle]
pub static mut midi_playing: c_short = 0;
static mut ogg_playing: c_short = 0;

static mut current_speaker_sound: *mut speaker_type = null_mut();
static mut speaker_note_index: c_int = 0;
static mut current_speaker_note_samples_already_emitted: c_int = 0;

static mut digi_buffer: *mut byte = null_mut();
static mut digi_remaining_pos: *mut byte = null_mut();
static mut digi_remaining_length: c_int = 0;

#[no_mangle]
pub static mut digi_audiospec: *mut SDL_AudioSpec = null_mut();
const digi_samplerate: c_int = 44100;

static mut ogg_decoder: *mut stb_vorbis = null_mut();

static mut square_wave_state: c_short = 4000;
static mut square_wave_samples_since_last_flip: f32 = 0.0;

#[no_mangle]
pub static mut audio_speed: c_int = 1;

#[no_mangle]
pub static mut digi_unavailable: c_int = 0;

const sound_channel: c_int = 0;
const max_sound_id: c_int = 58;

static mut wave_version: c_int = -1;

static mut RGB24_bug_checked: bool = false;
static mut RGB24_bug_affected: bool = false;

static mut fps: c_int = BASE_FPS;
static mut milliseconds_per_tick: f32 = 1000.0 / (BASE_FPS as f32);
static mut timer_last_counter: [u64; NUM_TIMERS] = [0; NUM_TIMERS];
static mut wait_time: [c_int; NUM_TIMERS] = [0; NUM_TIMERS];

static mut ignore_tab: bool = false;
static mut word_1D63A: word = 1;

static mut onscreen_surface_2x: *mut SDL_Surface = null_mut();

// init_overlay / init_scaling "static bool initialized"
static mut overlay_initialized: bool = false;

// directory listing (dirent.h-based) — opaque to other modules via cast.
#[repr(C)]
struct DirectoryListing {
    dp: *mut c_void,
    found_filename: *mut c_char,
    extension: *const c_char,
}

include!("seg009_hc_font_data.rs");

// seg009: sdlperror
#[no_mangle]
pub unsafe extern "C" fn sdlperror(header: *const c_char) {
    let error = SDL_GetError();
    printf(cs!("%s: %s\n"), header, error);
}

unsafe fn find_exe_dir() {
    if found_exe_dir {
        return;
    }
    snprintf_check!(
        exe_dir.as_mut_ptr(),
        core::mem::size_of_val(&exe_dir),
        cs!("%s"),
        *g_argv.offset(0)
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

unsafe fn find_home_dir() {
    if found_home_dir {
        return;
    }
    let home_path = getenv(cs!("HOME"));
    snprintf_check!(home_dir.as_mut_ptr(), POP_MAX_PATH - 1, cs!("%s/.%s"), home_path, cs!("SDLPoP"));
    if file_exists(home_dir.as_ptr()) {
        found_home_dir = true;
    }
}

unsafe fn find_share_dir() {
    if found_share_dir {
        return;
    }
    snprintf_check!(share_dir.as_mut_ptr(), POP_MAX_PATH - 1, cs!("%s/%s"), cs!("/usr/share"), cs!("SDLPoP"));
    if file_exists(share_dir.as_ptr()) {
        found_share_dir = true;
    }
}

// seg009: file_exists
#[no_mangle]
pub unsafe extern "C" fn file_exists(filename: *const c_char) -> bool {
    access(filename, F_OK) != -1
}

unsafe fn find_first_file_match(dst: *mut c_char, size: c_int, format: *const c_char, filename: *const c_char) -> *const c_char {
    find_exe_dir();
    find_home_dir();
    find_share_dir();
    let dirs: [*mut c_char; 3] = [home_dir.as_mut_ptr(), share_dir.as_mut_ptr(), exe_dir.as_mut_ptr()];
    for i in 0..3 {
        snprintf_check!(dst, size, format, dirs[i], filename);
        if file_exists(dst) {
            break;
        }
    }
    dst as *const c_char
}

// seg009: locate_save_file_
#[no_mangle]
pub unsafe extern "C" fn locate_save_file_(filename: *const c_char, dst: *mut c_char, size: c_int) -> *const c_char {
    find_exe_dir();
    find_home_dir();
    find_share_dir();
    let dirs: [*mut c_char; 3] = [home_dir.as_mut_ptr(), share_dir.as_mut_ptr(), exe_dir.as_mut_ptr()];
    for i in 0..3 {
        let mut path_stat: stat_t = core::mem::zeroed();
        let result = stat(dirs[i], &mut path_stat);
        if result == 0 && S_ISDIR(path_stat.st_mode) && access(dirs[i], W_OK) == 0 {
            snprintf_check!(dst, size, cs!("%s/%s"), dirs[i], filename);
            break;
        }
    }
    dst as *const c_char
}

// seg009: locate_file_
#[no_mangle]
pub unsafe extern "C" fn locate_file_(filename: *const c_char, path_buffer: *mut c_char, buffer_size: c_int) -> *const c_char {
    if file_exists(filename) {
        filename
    } else {
        find_first_file_match(path_buffer, buffer_size, cs!("%s/%s"), filename)
    }
}

// seg009: create_directory_listing_and_find_first_file
#[no_mangle]
pub unsafe extern "C" fn create_directory_listing_and_find_first_file(directory: *const c_char, extension: *const c_char) -> *mut directory_listing_type {
    let data = calloc(1, core::mem::size_of::<DirectoryListing>()) as *mut DirectoryListing;
    let mut ok = false;
    (*data).dp = opendir(directory);
    if !(*data).dp.is_null() {
        loop {
            let ep = readdir((*data).dp);
            if ep.is_null() {
                break;
            }
            let dname = core::ptr::addr_of_mut!((*ep).d_name) as *mut c_char;
            let ext = strrchr(dname, '.' as c_int);
            if !ext.is_null() && strcasecmp(ext.add(1), extension) == 0 {
                (*data).found_filename = dname;
                (*data).extension = extension;
                ok = true;
                break;
            }
        }
    }
    if ok {
        data as *mut directory_listing_type
    } else {
        free(data as *mut c_void);
        null_mut()
    }
}

// seg009: get_current_filename_from_directory_listing
#[no_mangle]
pub unsafe extern "C" fn get_current_filename_from_directory_listing(data: *mut directory_listing_type) -> *mut c_char {
    let data = data as *mut DirectoryListing;
    (*data).found_filename
}

// seg009: find_next_file
#[no_mangle]
pub unsafe extern "C" fn find_next_file(data: *mut directory_listing_type) -> bool {
    let data = data as *mut DirectoryListing;
    let mut ok = false;
    loop {
        let ep = readdir((*data).dp);
        if ep.is_null() {
            break;
        }
        let dname = core::ptr::addr_of_mut!((*ep).d_name) as *mut c_char;
        let ext = strrchr(dname, '.' as c_int);
        if !ext.is_null() && strcasecmp(ext.add(1), (*data).extension) == 0 {
            (*data).found_filename = dname;
            ok = true;
            break;
        }
    }
    ok
}

// seg009: close_directory_listing
#[no_mangle]
pub unsafe extern "C" fn close_directory_listing(data: *mut directory_listing_type) {
    let data = data as *mut DirectoryListing;
    closedir((*data).dp);
    free(data as *mut c_void);
}

// seg009:000D read_key
#[no_mangle]
pub unsafe extern "C" fn read_key() -> c_int {
    let key = last_key_scancode;
    last_key_scancode = 0;
    key
}

// seg009:019A clear_kbd_buf
#[no_mangle]
pub unsafe extern "C" fn clear_kbd_buf() {
    last_key_scancode = 0;
    last_text_input = 0;
}

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

// seg009:0467 round_xpos_to_byte
#[no_mangle]
pub unsafe extern "C" fn round_xpos_to_byte(xpos: c_int, _round_direction: c_int) -> c_int {
    xpos
}

// seg009:0C7A quit
#[no_mangle]
pub unsafe extern "C" fn quit(exit_code: c_int) {
    restore_stuff();
    exit(exit_code);
}

// seg009:0C90 restore_stuff
#[no_mangle]
pub unsafe extern "C" fn restore_stuff() {
    SDL_Quit();
}

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

// seg009:0E54 check_param
#[no_mangle]
pub unsafe extern "C" fn check_param(param: *const c_char) -> *const c_char {
    static PARAMS: [&[u8]; 2] = [b"mod\0", b"validate\0"];
    let mut arg_index: c_short = 1;
    while (arg_index as c_int) < g_argc {
        let curr_arg = *g_argv.offset(arg_index as isize);
        if !strchr(curr_arg, '.' as c_int).is_null() {
            arg_index += 1;
            continue;
        }
        let mut curr_arg_has_one_subparam = false;
        for i in 0..PARAMS.len() {
            let p = PARAMS[i].as_ptr() as *const c_char;
            if strncasecmp(curr_arg, p, strlen(p)) == 0 {
                curr_arg_has_one_subparam = true;
                break;
            }
        }
        if curr_arg_has_one_subparam {
            arg_index += 1;
            if !((arg_index as c_int) < g_argc) {
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

// seg009:0EDF pop_wait
#[no_mangle]
pub unsafe extern "C" fn pop_wait(timer_index: c_int, time: c_int) -> c_int {
    start_timer(timer_index, time);
    do_wait(timer_index)
}

unsafe fn open_dat_from_root_or_data_dir(filename: *const c_char) -> *mut FILE {
    let mut fp: *mut FILE = fopen(filename, cs!("rb"));
    // if failed, try if the DAT file can be opened in the data/ directory
    if fp.is_null() {
        let mut data_path = [0 as c_char; POP_MAX_PATH];
        snprintf_check!(data_path.as_mut_ptr(), POP_MAX_PATH, cs!("data/%s"), filename);
        if !file_exists(data_path.as_ptr()) {
            find_first_file_match(data_path.as_mut_ptr(), POP_MAX_PATH as c_int, cs!("%s/data/%s"), filename);
        }
        let mut path_stat: stat_t = core::mem::zeroed();
        stat(data_path.as_ptr(), &mut path_stat);
        if S_ISREG(path_stat.st_mode) {
            fp = fopen(data_path.as_ptr(), cs!("rb"));
        }
    }
    fp
}

// seg009:0F58 open_dat
#[no_mangle]
pub unsafe extern "C" fn open_dat(filename: *const c_char, mut optional: c_int) -> *mut dat_type {
    let mut fp: *mut FILE = null_mut();
    if use_custom_levelset == 0 {
        fp = open_dat_from_root_or_data_dir(filename);
    } else {
        if !skip_mod_data_files && skip_normal_data_files {
            optional = 1;
        }
        if !skip_mod_data_files && !(always_use_original_graphics != 0 && optional == 'G' as c_int) {
            let mut filename_mod = [0 as c_char; POP_MAX_PATH];
            snprintf_check!(filename_mod.as_mut_ptr(), POP_MAX_PATH, cs!("%s/%s"), mod_data_path.as_ptr(), filename);
            fp = fopen(filename_mod.as_ptr(), cs!("rb"));
        }
        if fp.is_null() && !skip_normal_data_files {
            fp = open_dat_from_root_or_data_dir(filename);
        }
    }
    let mut dat_header: dat_header_type = core::mem::zeroed();
    let mut dat_table: *mut dat_table_type = null_mut();

    let pointer = calloc(1, core::mem::size_of::<dat_type>()) as *mut dat_type;
    snprintf_check!(core::ptr::addr_of_mut!((*pointer).filename) as *mut c_char, 256, cs!("%s"), filename);
    (*pointer).next_dat = dat_chain_ptr;
    dat_chain_ptr = pointer;

    if !fp.is_null() {
        let mut failed = false;
        'f: {
            if fread(core::ptr::addr_of_mut!(dat_header) as *mut c_void, 6, 1, fp) != 1 {
                failed = true;
                break 'f;
            }
            dat_table = malloc(swaple16(dat_header.table_size) as usize) as *mut dat_table_type;
            if dat_table.is_null()
                || fseek(fp, swaple32(dat_header.table_offset) as c_long, SEEK_SET) != 0
                || fread(dat_table as *mut c_void, swaple16(dat_header.table_size) as usize, 1, fp) != 1
            {
                failed = true;
                break 'f;
            }
            (*pointer).handle = fp;
            (*pointer).dat_table = dat_table;
        }
        if failed {
            perror(filename);
            if !fp.is_null() {
                fclose(fp);
            }
            if !dat_table.is_null() {
                free(dat_table as *mut c_void);
            }
        }
    } else if optional == 0 {
        let mut filename_no_ext = [0 as c_char; POP_MAX_PATH];
        strncpy(filename_no_ext.as_mut_ptr(), core::ptr::addr_of!((*pointer).filename) as *const c_char, POP_MAX_PATH);
        let len = strlen(filename_no_ext.as_ptr());
        if len >= 5 && filename_no_ext[len - 4] == '.' as c_char {
            filename_no_ext[len - 4] = 0;
        }
        let mut foldername = [0 as c_char; POP_MAX_PATH];
        snprintf_check!(foldername.as_mut_ptr(), POP_MAX_PATH, cs!("data/%s"), filename_no_ext.as_ptr());
        let mut __lf = [0 as c_char; POP_MAX_PATH];
        let data_path = locate_file_(foldername.as_ptr(), __lf.as_mut_ptr(), POP_MAX_PATH as c_int);
        let mut path_stat: stat_t = core::mem::zeroed();
        let result = stat(data_path, &mut path_stat);
        if result != 0 || !S_ISDIR(path_stat.st_mode) {
            let mut error_message = [0 as c_char; 256];
            snprintf_check!(error_message.as_mut_ptr(), 256, cs!("Cannot find a required data file: %s or folder: %s\nPress any key to quit."), filename, foldername.as_ptr());
            if !onscreen_surface_.is_null() && !copyprot_dialog.is_null() {
                showmessage(error_message.as_mut_ptr(), 1, key_test_quit as *mut c_void);
                quit(1);
            }
        }
    }
    pointer
}

// seg009:9CAC set_loaded_palette
#[no_mangle]
pub unsafe extern "C" fn set_loaded_palette(palette_ptr: *mut dat_pal_type) {
    let mut dest_index: c_int = 0;
    let mut source_row: c_int = 0;
    let vga_base = core::ptr::addr_of!((*palette_ptr).vga) as *const rgb_type;
    for dest_row in 0..16 {
        if ((*palette_ptr).row_bits as c_int) & (1 << dest_row) != 0 {
            set_pal_arr(dest_index, 16, vga_base.add((source_row * 0x10) as usize));
            source_row += 1;
        }
        dest_index += 0x10;
    }
}

// seg009:104E load_sprites_from_file
#[no_mangle]
pub unsafe extern "C" fn load_sprites_from_file(resource: c_int, palette_bits: c_int, quit_on_error: c_int) -> *mut chtab_type {
    let shpl = load_from_opendats_alloc(resource, cs!("pal"), null_mut(), null_mut()) as *mut dat_shpl_type;
    if shpl.is_null() {
        printf(cs!("Can't load sprites from resource %d.\n"), resource);
        if quit_on_error != 0 {
            let mut error_message = [0 as c_char; 256];
            snprintf_check!(error_message.as_mut_ptr(), 256, cs!("Can't load sprites from resource %d.\nThe last opened data file is: %s\nPress any key to quit."), resource, core::ptr::addr_of!((*dat_chain_ptr).filename) as *const c_char);
            showmessage(error_message.as_mut_ptr(), 1, key_test_quit as *mut c_void);
            quit(1);
        }
        return null_mut();
    }
    let pal_ptr = core::ptr::addr_of_mut!((*shpl).palette);
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int {
        if palette_bits == 0 {
            // (original body commented out)
        } else {
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
        let image = load_image(resource + i, pal_ptr);
        *images.add((i - 1) as usize) = image;
    }
    set_loaded_palette(pal_ptr);
    chtab
}

// seg009:11A8 free_chtab
#[no_mangle]
pub unsafe extern "C" fn free_chtab(chtab_ptr: *mut chtab_type) {
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int && (*chtab_ptr).has_palette_bits != 0 {
        chtab_palette_bits &= !(*chtab_ptr).chtab_palette_bits;
    }
    let n_images = (*chtab_ptr).n_images;
    let images = core::ptr::addr_of_mut!((*chtab_ptr).images) as *mut *mut image_type;
    let mut id: word = 0;
    while id < n_images {
        let curr_image = *images.add(id as usize);
        if !curr_image.is_null() {
            SDL_FreeSurface(curr_image);
        }
        id += 1;
    }
    free(chtab_ptr as *mut c_void);
}

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

// seg009:938E decompr_img
unsafe fn decompr_img(dest: *mut byte, source: *const image_data_type, decomp_size: c_int, cmeth: c_int, stride: c_int) {
    let data_ptr = core::ptr::addr_of!((*source).data) as *const byte;
    match cmeth {
        0 => {
            memcpy(dest as *mut c_void, data_ptr as *const c_void, decomp_size as usize);
        }
        1 => {
            decompress_rle_lr(dest, data_ptr, decomp_size);
        }
        2 => {
            decompress_rle_ud(dest, data_ptr, decomp_size, stride, swaple16((*source).height) as c_int);
        }
        3 => {
            decompress_lzg_lr(dest, data_ptr, decomp_size);
        }
        4 => {
            decompress_lzg_ud(dest, data_ptr, decomp_size, stride, swaple16((*source).height) as c_int);
        }
        _ => {}
    }
}

unsafe fn calc_stride(image_data: *mut image_data_type) -> c_int {
    let width = swaple16((*image_data).width) as c_int;
    let flags = swaple16((*image_data).flags) as c_int;
    let depth = ((flags >> 12) & 7) + 1;
    (depth * width + 7) / 8
}

unsafe fn conv_to_8bpp(in_data: *mut byte, width: c_int, height: c_int, stride: c_int, depth: c_int) -> *mut byte {
    let out_data = malloc((width * height) as usize) as *mut byte;
    let pixels_per_byte = 8 / depth;
    let mask = (1 << depth) - 1;
    for y in 0..height {
        let mut in_pos = in_data.offset((y * stride) as isize);
        let mut out_pos = out_data.offset((y * width) as isize);
        let mut x_pixel: c_int = 0;
        let mut x_byte: c_int = 0;
        while x_byte < stride {
            let v = *in_pos;
            let mut shift = 8;
            let mut pixel_in_byte = 0;
            while pixel_in_byte < pixels_per_byte && x_pixel < width {
                shift -= depth;
                *out_pos = (((v as c_int) >> shift) & mask) as byte;
                out_pos = out_pos.add(1);
                pixel_in_byte += 1;
                x_pixel += 1;
            }
            in_pos = in_pos.add(1);
            x_byte += 1;
        }
    }
    out_data
}

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
    let mut dest = malloc(dest_size as usize) as *mut byte;
    memset(dest as *mut c_void, 0, dest_size as usize);
    decompr_img(dest, image_data, dest_size, cmeth, stride);
    let mut image_8bpp = conv_to_8bpp(dest, width, height, stride, depth);
    free(dest as *mut c_void);
    dest = null_mut();
    let _ = dest;
    let image = SDL_CreateRGBSurface(0, width, height, 8, 0, 0, 0, 0);
    if image.is_null() {
        sdlperror(cs!("decode_image: SDL_CreateRGBSurface"));
        quit(1);
    }
    if SDL_LockSurface(image) != 0 {
        sdlperror(cs!("decode_image: SDL_LockSurface"));
    }
    for y in 0..height {
        memcpy(
            ((*image).pixels as *mut byte).offset((y * (*image).pitch) as isize) as *mut c_void,
            image_8bpp.offset((y * width) as isize) as *const c_void,
            width as usize,
        );
    }
    SDL_UnlockSurface(image);
    free(image_8bpp as *mut c_void);
    image_8bpp = null_mut();
    let _ = image_8bpp;
    let mut colors: [SDL_Color; 16] = core::mem::zeroed();
    let vga = core::ptr::addr_of!((*pal).vga) as *const rgb_type;
    for i in 0..16usize {
        let p = vga.add(i);
        colors[i].r = (((*p).r as c_int) << 2) as u8;
        colors[i].g = (((*p).g as c_int) << 2) as u8;
        colors[i].b = (((*p).b as c_int) << 2) as u8;
        colors[i].a = SDL_ALPHA_OPAQUE;
    }
    colors[0].r = 0;
    colors[0].g = 0;
    colors[0].b = 0;
    colors[0].a = SDL_ALPHA_TRANSPARENT;
    SDL_SetPaletteColors((*(*image).format).palette, colors.as_ptr(), 0, 16);
    image
}

// seg009:121A load_image
#[no_mangle]
pub unsafe extern "C" fn load_image(resource_id: c_int, pal: *mut dat_pal_type) -> *mut image_type {
    let mut result: data_location = 0;
    let mut size: c_int = 0;
    let image_data = load_from_opendats_alloc(resource_id, cs!("png"), &mut result, &mut size);
    let mut image: *mut image_type = null_mut();
    match result {
        data_location_data_none => {
            return null_mut();
        }
        data_location_data_DAT => {
            image = decode_image(image_data as *mut image_data_type, pal);
        }
        data_location_data_directory => {
            let rw = SDL_RWFromConstMem(image_data, size);
            if rw.is_null() {
                sdlperror(cs!("load_image: SDL_RWFromConstMem"));
                return null_mut();
            }
            image = IMG_Load_RW(rw, 0);
            if image.is_null() {
                printf(cs!("load_image: IMG_Load_RW: %s\n"), IMG_GetError());
            }
            if SDL_RWclose(rw) != 0 {
                sdlperror(cs!("load_image: SDL_RWclose"));
            }
        }
        _ => {}
    }
    if !image_data.is_null() {
        free(image_data);
    }
    if !image.is_null() {
        if SDL_SetColorKey(image, SDL_TRUE, 0) != 0 {
            sdlperror(cs!("load_image: SDL_SetColorKey"));
            quit(1);
        }
    }
    image
}

// seg009:13C4 draw_image_transp
#[no_mangle]
pub unsafe extern "C" fn draw_image_transp(image: *mut image_type, _mask: *mut image_type, xpos: c_int, ypos: c_int) {
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int {
        draw_image_transp_vga(image, xpos, ypos);
    }
}

// seg009:157E set_joy_mode
#[no_mangle]
pub unsafe extern "C" fn set_joy_mode() -> c_int {
    if SDL_NumJoysticks() < 1 {
        is_joyst_mode = 0;
    } else {
        if gamecontrollerdb_file[0] != 0 {
            SDL_GameControllerAddMappingsFromFile(gamecontrollerdb_file.as_ptr());
        }
        if SDL_IsGameController(0) != 0 {
            sdl_controller_ = SDL_GameControllerOpen(0);
            if sdl_controller_.is_null() {
                is_joyst_mode = 0;
            } else {
                is_joyst_mode = 1;
            }
        } else {
            sdl_joystick_ = SDL_JoystickOpen(0);
            is_joyst_mode = 1;
            using_sdl_joystick_interface = 1;
        }
    }
    if enable_controller_rumble != 0 && is_joyst_mode != 0 {
        sdl_haptic = SDL_HapticOpen(0);
        SDL_HapticRumbleInit(sdl_haptic);
    } else {
        sdl_haptic = null_mut();
    }
    is_keyboard_mode = (is_joyst_mode == 0) as word;
    is_joyst_mode as c_int
}

// seg009:178B make_offscreen_buffer
#[no_mangle]
pub unsafe extern "C" fn make_offscreen_buffer(rect: *const rect_type) -> *mut surface_type {
    SDL_CreateRGBSurface(0, (*rect).right as c_int, (*rect).bottom as c_int, 24, Rmsk, Gmsk, Bmsk, 0)
}

// seg009:17BD free_surface
#[no_mangle]
pub unsafe extern "C" fn free_surface(surface: *mut surface_type) {
    SDL_FreeSurface(surface);
}

// seg009:17EA free_peel
#[no_mangle]
pub unsafe extern "C" fn free_peel(peel_ptr: *mut peel_type) {
    SDL_FreeSurface((*peel_ptr).peel);
    free(peel_ptr as *mut c_void);
}

// seg009:182F set_hc_pal
#[no_mangle]
pub unsafe extern "C" fn set_hc_pal() {
    if graphics_mode as c_int == grmodes_gmMcgaVga as c_int {
        set_pal_arr(0, 16, core::ptr::addr_of!((*custom).vga_palette) as *const rgb_type);
    }
}

// seg009:2446 flip_not_ega
#[no_mangle]
pub unsafe extern "C" fn flip_not_ega(memory: *mut byte, height: c_int, stride: c_int) {
    let row_buffer = malloc(stride as usize) as *mut byte;
    let mut top_ptr = memory;
    let mut bottom_ptr = memory;
    bottom_ptr = bottom_ptr.offset(((height - 1) * stride) as isize);
    let mut rem_rows: c_short = (height >> 1) as c_short;
    loop {
        memcpy(row_buffer as *mut c_void, top_ptr as *const c_void, stride as usize);
        memcpy(top_ptr as *mut c_void, bottom_ptr as *const c_void, stride as usize);
        memcpy(bottom_ptr as *mut c_void, row_buffer as *const c_void, stride as usize);
        top_ptr = top_ptr.offset(stride as isize);
        bottom_ptr = bottom_ptr.offset(-(stride as isize));
        rem_rows -= 1;
        if rem_rows == 0 {
            break;
        }
    }
    free(row_buffer as *mut c_void);
}

// seg009:19B1 flip_screen
#[no_mangle]
pub unsafe extern "C" fn flip_screen(surface: *mut surface_type) {
    if graphics_mode as c_int != grmodes_gmEga as c_int {
        if SDL_LockSurface(surface) != 0 {
            sdlperror(cs!("flip_screen: SDL_LockSurface"));
            quit(1);
        }
        flip_not_ega((*surface).pixels as *mut byte, (*surface).h, (*surface).pitch);
        SDL_UnlockSurface(surface);
    }
}

// seg009:2288 draw_image_transp_vga
#[no_mangle]
pub unsafe extern "C" fn draw_image_transp_vga(image: *mut image_type, xpos: c_int, ypos: c_int) {
    method_6_blit_img_to_scr(image, xpos, ypos, blitters_blitters_10h_transp as c_int);
}

// ===================== USE_TEXT block =====================

unsafe fn load_font_character_offsets(data: *mut rawfont_type) {
    let n_chars = ((*data).last_char as c_int) - ((*data).first_char as c_int) + 1;
    let offsets = core::ptr::addr_of_mut!((*data).offsets) as *mut word;
    let mut pos = offsets.add(n_chars as usize) as *mut byte;
    for index in 0..n_chars {
        *offsets.add(index as usize) = swaple16((pos as usize - data as usize) as word);
        let image_data = pos as *mut image_data_type;
        let image_bytes = (swaple16((*image_data).height) as c_int) * calc_stride(image_data);
        pos = (core::ptr::addr_of_mut!((*image_data).data) as *mut byte).offset(image_bytes as isize);
    }
}

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
    if swaple16(*offsets.add(0)) == 0 {
        load_font_character_offsets(data);
    }
    let chtab = malloc(core::mem::size_of::<chtab_type>() + core::mem::size_of::<*mut image_type>() * (n_chars as usize)) as *mut chtab_type;
    let mut dat_pal: dat_pal_type = core::mem::zeroed();
    let dpvga = core::ptr::addr_of_mut!(dat_pal.vga) as *mut rgb_type;
    (*dpvga.add(1)).r = 0x3F;
    (*dpvga.add(1)).g = 0x3F;
    (*dpvga.add(1)).b = 0x3F;
    let images = core::ptr::addr_of_mut!((*chtab).images) as *mut *mut image_type;
    let mut index: c_int = 0;
    let mut chr: c_int = (*data).first_char as c_int;
    while chr <= (*data).last_char as c_int {
        let image_data = (data as *mut byte).offset(swaple16(*offsets.add(index as usize)) as isize) as *mut image_data_type;
        if (*image_data).height == swaple16(0) {
            (*image_data).height = swaple16(1);
        }
        let image = decode_image(image_data, &mut dat_pal);
        *images.add(index as usize) = image;
        if SDL_SetColorKey(image, SDL_TRUE, 0) != 0 {
            sdlperror(cs!("load_font_from_data: SDL_SetColorKey"));
            quit(1);
        }
        index += 1;
        chr += 1;
    }
    font.chtab = chtab;
    font
}

unsafe fn load_font() {
    // Try to load font from a file.
    let dathandle = open_dat(cs!("font"), 1);
    hc_font.chtab = load_sprites_from_file(1000, 1 << 1, 0);
    close_dat(dathandle);
    if hc_font.chtab.is_null() {
        hc_font = load_font_from_data(core::ptr::addr_of_mut!(hc_font_data) as *mut rawfont_type);
    }
    hc_small_font = load_font_from_data(core::ptr::addr_of_mut!(hc_small_font_data) as *mut rawfont_type);
}

// seg009:35C5 get_char_width
unsafe fn get_char_width(character: byte) -> c_int {
    let font = textstate.ptr_font;
    let mut width: c_int = 0;
    if character <= (*font).last_char && character >= (*font).first_char {
        let images = core::ptr::addr_of!((*(*font).chtab).images) as *const *mut image_type;
        let image = *images.add((character - (*font).first_char) as usize);
        if !image.is_null() {
            width += (*image).w;
            if width != 0 {
                width += (*font).space_between_chars as c_int;
            }
        }
    }
    width
}

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
                last_break_pos = curr_char_pos as c_short;
            }
        } else {
            if last_break_pos == 0 {
                return curr_char_pos;
            } else {
                return last_break_pos as c_int;
            }
        }
    }
    curr_char_pos
}

// seg009:403F get_line_width
#[no_mangle]
pub unsafe extern "C" fn get_line_width(text: *const c_char, mut length: c_int) -> c_int {
    let mut width: c_int = 0;
    let mut text_pos = text;
    loop {
        length -= 1;
        if length < 0 {
            break;
        }
        width += get_char_width(*text_pos as byte);
        text_pos = text_pos.add(1);
    }
    width
}

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
            width = (*font).space_between_chars as c_int + (*image).w;
        }
    }
    textstate.current_x = (textstate.current_x as c_int + width) as c_short;
    width
}

// seg009:377F draw_text_line
unsafe fn draw_text_line(text: *const c_char, mut length: c_int) -> c_int {
    let mut width: c_int = 0;
    let mut text_pos = text;
    loop {
        length -= 1;
        if length < 0 {
            break;
        }
        width += draw_text_character(*text_pos as byte);
        text_pos = text_pos.add(1);
    }
    width
}

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

// seg009:3F01 draw_text
unsafe fn draw_text(rect_ptr: *const rect_type, x_align: c_int, y_align: c_int, text: *const c_char, length: c_int) -> *const rect_type {
    let l_rect_top: c_short;
    let rect_height: c_short;
    let rect_width: c_short;
    let mut num_lines: c_short;
    let font_line_distance: c_short;
    set_clip_rect(rect_ptr);
    rect_width = (*rect_ptr).right - (*rect_ptr).left;
    l_rect_top = (*rect_ptr).top;
    rect_height = (*rect_ptr).bottom - (*rect_ptr).top;
    num_lines = 0;
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
            printf(cs!("draw_text(): Too many lines!\n"));
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
    font_line_distance = (*font).height_above_baseline + (*font).height_below_baseline + (*font).space_between_lines;
    let text_height = (font_line_distance as c_int) * (num_lines as c_int) - (*font).space_between_lines as c_int;
    let mut text_top = l_rect_top as c_int;
    if y_align >= 0 {
        if y_align <= 0 {
            text_top += (rect_height as c_int + 1) / 2 - (text_height + 1) / 2;
        } else {
            text_top += rect_height as c_int - text_height;
        }
    }
    textstate.current_y = (text_top + (*font).height_above_baseline as c_int) as c_short;
    for i in 0..num_lines as usize {
        let mut line_pos = line_starts[i];
        let mut line_length = line_lengths[i];
        if x_align < 0 && *line_pos == ' ' as c_char && i != 0 && *line_pos.offset(-1) != '\n' as c_char {
            line_pos = line_pos.add(1);
            line_length -= 1;
            if line_length != 0 && *line_pos == ' ' as c_char && *line_pos.offset(-2) == '.' as c_char {
                line_pos = line_pos.add(1);
                line_length -= 1;
            }
        }
        let line_width = get_line_width(line_pos, line_length);
        let mut text_left = (*rect_ptr).left as c_int;
        if x_align >= 0 {
            if x_align <= 0 {
                text_left += rect_width as c_int / 2 - line_width / 2;
            } else {
                text_left += rect_width as c_int - line_width;
            }
        }
        textstate.current_x = text_left as c_short;
        draw_text_line(line_pos, line_length);
        textstate.current_y = (textstate.current_y as c_int + font_line_distance as c_int) as c_short;
    }
    reset_clip_rect();
    rect_ptr
}

// seg009:3E4F show_text
#[no_mangle]
pub unsafe extern "C" fn show_text(rect_ptr: *const rect_type, x_align: c_int, y_align: c_int, text: *const c_char) {
    draw_text(rect_ptr, x_align, y_align, text, strlen(text) as c_int);
}

// seg009:04FF show_text_with_color
#[no_mangle]
pub unsafe extern "C" fn show_text_with_color(rect_ptr: *const rect_type, x_align: c_int, y_align: c_int, text: *const c_char, color: c_int) {
    let saved_textcolor: c_short = textstate.textcolor;
    textstate.textcolor = color as c_short;
    show_text(rect_ptr, x_align, y_align, text);
    textstate.textcolor = saved_textcolor;
}

// seg009:3A91 set_curr_pos
#[no_mangle]
pub unsafe extern "C" fn set_curr_pos(xpos: c_int, ypos: c_int) {
    textstate.current_x = xpos as c_short;
    textstate.current_y = ypos as c_short;
}

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

// seg009:0838 showmessage
#[no_mangle]
pub unsafe extern "C" fn showmessage(text: *mut c_char, _arg_4: c_int, _arg_0: *mut c_void) -> c_int {
    let mut key: word = 0;
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
        key = key_test_quit() as word;
        if key != 0 {
            break;
        }
    }
    need_full_redraw = 1;
    key as c_int
}

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

// seg009:0BE7 calc_dialog_peel_rect
#[no_mangle]
pub unsafe extern "C" fn calc_dialog_peel_rect(dialog: *mut dialog_type) {
    let settings = (*dialog).settings;
    (*dialog).peel_rect.left = (*dialog).text_rect.left - (*settings).left_border;
    (*dialog).peel_rect.top = (*dialog).text_rect.top - (*settings).top_border;
    (*dialog).peel_rect.right = (*dialog).text_rect.right + (*settings).right_border + (*settings).shadow_right;
    (*dialog).peel_rect.bottom = (*dialog).text_rect.bottom + (*settings).bottom_border + (*settings).shadow_bottom;
}

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

// seg009:09DE draw_dialog_frame
#[no_mangle]
pub unsafe extern "C" fn draw_dialog_frame(dialog: *mut dialog_type) {
    ((*(*dialog).settings).method_2_frame).unwrap()(dialog);
}

// seg009:096F add_dialog_rect
#[no_mangle]
pub unsafe extern "C" fn add_dialog_rect(dialog: *mut dialog_type) {
    draw_rect(core::ptr::addr_of!((*dialog).text_rect), colorids_color_0_black as c_int);
}

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

// seg009:0C44 show_dialog
#[no_mangle]
pub unsafe extern "C" fn show_dialog(text: *const c_char) {
    let mut string = [0 as c_char; 256];
    snprintf(string.as_mut_ptr(), 256, cs!("%s\n\nPress any key to continue."), text);
    showmessage(string.as_mut_ptr(), 1, key_test_quit as *mut c_void);
}

// seg009:0791 get_text_center_y
unsafe fn get_text_center_y(rect: *const rect_type) -> c_int {
    let font = core::ptr::addr_of!(hc_font);
    let empty_height: c_short = (*rect).bottom - (*font).height_above_baseline - (*font).height_below_baseline - (*rect).top;
    (((empty_height as c_int) - (empty_height as c_int) % 2) >> 1) + (*font).height_above_baseline as c_int + (empty_height as c_int) % 2 + (*rect).top as c_int
}

// seg009:3E77 get_cstring_width
unsafe fn get_cstring_width(text: *const c_char) -> c_int {
    let mut width: c_int = 0;
    let mut text_pos = text;
    loop {
        let curr_char = *text_pos;
        if curr_char == 0 {
            break;
        }
        text_pos = text_pos.add(1);
        width += get_char_width(curr_char as byte);
    }
    width
}

// seg009:0767 draw_text_cursor
unsafe fn draw_text_cursor(xpos: c_int, ypos: c_int, color: c_int) {
    set_curr_pos(xpos, ypos);
    textstate.textcolor = color as c_short;
    draw_text_character('_' as byte);
    textstate.textcolor = 15;
}

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
    SDL_SetTextInputRect(&mut sdlrect);
    SDL_StartTextInput();

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
            if cursor_visible != 0 {
                draw_text_cursor(current_xpos as c_int, ypos as c_int, color);
            } else {
                draw_text_cursor(current_xpos as c_int, ypos as c_int, bgcolor);
            }
            cursor_visible = (cursor_visible == 0) as c_short;
            start_timer(timerids_timer_0 as c_int, 6);
            if key != 0 {
                if cursor_visible != 0 {
                    draw_text_cursor(current_xpos as c_int, ypos as c_int, color);
                    cursor_visible = (cursor_visible == 0) as c_short;
                }
                if key as c_int == SDL_SCANCODE_RETURN {
                    *buffer.offset(length as isize) = 0;
                    SDL_StopTextInput();
                    return length as c_int;
                } else {
                    break;
                }
            }
            while has_timer_stopped(timerids_timer_0 as c_int) == 0 && {
                key = key_test_quit() as word;
                key == 0
            } {
                idle();
            }
        }
        let entered_char: c_char = if last_text_input <= 0x7E { last_text_input } else { 0 };
        clear_kbd_buf();

        if key as c_int == SDL_SCANCODE_ESCAPE {
            draw_rect(rect, bgcolor);
            *buffer.offset(0) = 0;
            SDL_StopTextInput();
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

// seg009:37E8 draw_rect
#[no_mangle]
pub unsafe extern "C" fn draw_rect(rect: *const rect_type, color: c_int) {
    method_5_rect(rect, blitters_blitters_0_no_transp as c_int, color as byte);
}

// seg009:3985 rect_sthg
#[no_mangle]
pub unsafe extern "C" fn rect_sthg(surface: *mut surface_type, _rect: *const rect_type) -> *mut surface_type {
    surface
}

// seg009:39CE shrink2_rect
#[no_mangle]
pub unsafe extern "C" fn shrink2_rect(target_rect: *mut rect_type, source_rect: *const rect_type, delta_x: c_int, delta_y: c_int) -> *mut rect_type {
    (*target_rect).top = ((*source_rect).top as c_int + delta_y) as c_short;
    (*target_rect).left = ((*source_rect).left as c_int + delta_x) as c_short;
    (*target_rect).bottom = ((*source_rect).bottom as c_int - delta_y) as c_short;
    (*target_rect).right = ((*source_rect).right as c_int - delta_x) as c_short;
    target_rect
}

// seg009:3BBA restore_peel
#[no_mangle]
pub unsafe extern "C" fn restore_peel(peel_ptr: *mut peel_type) {
    method_6_blit_img_to_scr((*peel_ptr).peel, (*peel_ptr).rect.left as c_int, (*peel_ptr).rect.top as c_int, 0);
    free_peel(peel_ptr);
}

// seg009:3BE9 read_peel_from_screen
#[no_mangle]
pub unsafe extern "C" fn read_peel_from_screen(rect: *const rect_type) -> *mut peel_type {
    let result = calloc(1, core::mem::size_of::<peel_type>()) as *mut peel_type;
    (*result).rect = *rect;
    let peel_surface = SDL_CreateRGBSurface(0, ((*rect).right - (*rect).left) as c_int, ((*rect).bottom - (*rect).top) as c_int, 24, Rmsk, Gmsk, Bmsk, 0);
    if peel_surface.is_null() {
        sdlperror(cs!("read_peel_from_screen: SDL_CreateRGBSurface"));
        quit(1);
    }
    (*result).peel = peel_surface;
    let target_rect = rect_type {
        top: 0,
        left: 0,
        bottom: (*rect).right - (*rect).left,
        right: (*rect).bottom - (*rect).top,
    };
    method_1_blit_rect((*result).peel, current_target_surface, &target_rect, rect, 0);
    result
}

// seg009:3D95 intersect_rect
#[no_mangle]
pub unsafe extern "C" fn intersect_rect(output: *mut rect_type, input1: *const rect_type, input2: *const rect_type) -> c_int {
    let left = if (*input1).left > (*input2).left { (*input1).left } else { (*input2).left };
    let right = if (*input1).right < (*input2).right { (*input1).right } else { (*input2).right };
    if left < right {
        (*output).left = left;
        (*output).right = right;
        let top = if (*input1).top > (*input2).top { (*input1).top } else { (*input2).top };
        let bottom = if (*input1).bottom < (*input2).bottom { (*input1).bottom } else { (*input2).bottom };
        if top < bottom {
            (*output).top = top;
            (*output).bottom = bottom;
            return 1;
        }
    }
    memset(output as *mut c_void, 0, core::mem::size_of::<rect_type>());
    0
}

// seg009:4063 union_rect
#[no_mangle]
pub unsafe extern "C" fn union_rect(output: *mut rect_type, input1: *const rect_type, input2: *const rect_type) -> *mut rect_type {
    let top = if (*input1).top < (*input2).top { (*input1).top } else { (*input2).top };
    let left = if (*input1).left < (*input2).left { (*input1).left } else { (*input2).left };
    let bottom = if (*input1).bottom > (*input2).bottom { (*input1).bottom } else { (*input2).bottom };
    let right = if (*input1).right > (*input2).right { (*input1).right } else { (*input2).right };
    (*output).top = top;
    (*output).left = left;
    (*output).bottom = bottom;
    (*output).right = right;
    output
}

// ===================== audio =====================

unsafe fn speaker_sound_stop() {
    if speaker_playing == 0 {
        return;
    }
    SDL_LockAudio();
    speaker_playing = 0;
    current_speaker_sound = null_mut();
    speaker_note_index = 0;
    current_speaker_note_samples_already_emitted = 0;
    SDL_UnlockAudio();
}

unsafe fn stop_digi() {
    if digi_playing == 0 {
        return;
    }
    SDL_LockAudio();
    digi_playing = 0;
    digi_buffer = null_mut();
    digi_remaining_length = 0;
    digi_remaining_pos = null_mut();
    SDL_UnlockAudio();
}

unsafe fn stop_ogg() {
    SDL_PauseAudio(1);
    if ogg_playing == 0 {
        return;
    }
    ogg_playing = 0;
    SDL_LockAudio();
    ogg_decoder = null_mut();
    SDL_UnlockAudio();
}

// seg009:7214 stop_sounds
#[no_mangle]
pub unsafe extern "C" fn stop_sounds() {
    stop_digi();
    stop_midi();
    speaker_sound_stop();
    stop_ogg();
}

unsafe fn generate_square_wave(mut stream: *mut byte, note_freq: f32, samples: c_int) {
    let channels = (*digi_audiospec).channels as c_int;
    let half_period_in_samples: f32 = ((*digi_audiospec).freq as f32 / note_freq) * 0.5f32;

    let mut samples_left = samples;
    while samples_left > 0 {
        if square_wave_samples_since_last_flip > half_period_in_samples {
            square_wave_state = !square_wave_state;
            square_wave_samples_since_last_flip -= half_period_in_samples;
        } else {
            let mut samples_until_next_flip = (half_period_in_samples - square_wave_samples_since_last_flip) as c_int;
            samples_until_next_flip += 1;
            let samples_to_emit = MIN_i(samples_until_next_flip, samples_left);
            let mut i = 0;
            while i < samples_to_emit * channels {
                *(stream as *mut c_short) = square_wave_state;
                stream = stream.add(core::mem::size_of::<c_short>());
                i += 1;
            }
            samples_left -= samples_to_emit;
            square_wave_samples_since_last_flip += samples_to_emit as f32;
        }
    }
}

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
        if swaple16((*note).frequency) == 0x12 {
            speaker_playing = 0;
            current_speaker_sound = null_mut();
            speaker_note_index = 0;
            let mut event: SDL_Event = core::mem::zeroed();
            event.type_ = SDL_USEREVENT;
            event.user.code = userevent_SOUND;
            SDL_PushEvent(&mut event);
            return;
        }

        let note_length_in_samples = ((*note).length as c_int * (*digi_audiospec).freq) / tempo as c_int;
        let note_samples_to_emit = MIN_i(note_length_in_samples - current_speaker_note_samples_already_emitted, total_samples_left);
        total_samples_left -= note_samples_to_emit;
        let copy_len = note_samples_to_emit as usize * bytes_per_sample as usize;
        if swaple16((*note).frequency) <= 0x01 {
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

// seg009:7640 play_speaker_sound
unsafe fn play_speaker_sound(buffer: *mut sound_buffer_type) {
    speaker_sound_stop();
    stop_sounds();
    current_speaker_sound = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut speaker_type;
    speaker_note_index = 0;
    speaker_playing = 1;
    SDL_PauseAudio(0);
}

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
        SDL_PushEvent(&mut event);
    }
    digi_remaining_length -= copy_len;
    digi_remaining_pos = digi_remaining_pos.add(copy_len as usize);
}

unsafe fn ogg_callback(_userdata: *mut c_void, stream: *mut u8, len: c_int) {
    let output_channels = (*digi_audiospec).channels as c_int;
    let bytes_per_sample = core::mem::size_of::<c_short>() as c_int * output_channels;
    let samples_requested = len / bytes_per_sample;

    let samples_filled: c_int;
    if is_sound_on != 0 {
        samples_filled = stb_vorbis_get_samples_short_interleaved(ogg_decoder, output_channels, stream as *mut c_short, len / core::mem::size_of::<c_short>() as c_int);
        if samples_filled < samples_requested {
            let bytes_filled = samples_filled * bytes_per_sample;
            let remaining_bytes = (samples_requested - samples_filled) * bytes_per_sample;
            memset(stream.add(bytes_filled as usize) as *mut c_void, (*digi_audiospec).silence as c_int, remaining_bytes as usize);
        }
    } else {
        memset(stream as *mut c_void, (*digi_audiospec).silence as c_int, len as usize);
        let discarded_samples = malloc(len as usize) as *mut u8;
        samples_filled = stb_vorbis_get_samples_short_interleaved(ogg_decoder, output_channels, discarded_samples as *mut c_short, len / core::mem::size_of::<c_short>() as c_int);
        free(discarded_samples as *mut c_void);
    }
    if samples_filled == 0 {
        let mut event: SDL_Event = core::mem::zeroed();
        event.type_ = SDL_USEREVENT;
        event.user.code = userevent_SOUND;
        ogg_playing = 0;
        SDL_PushEvent(&mut event);
    }
}

pub unsafe extern "C" fn audio_callback(userdata: *mut c_void, stream_orig: *mut u8, len_orig: c_int) {
    let stream: *mut u8;
    let len: c_int;
    if audio_speed > 1 {
        len = len_orig * audio_speed;
        stream = malloc(len as usize) as *mut u8;
    } else {
        len = len_orig;
        stream = stream_orig;
    }

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

    if audio_speed > 1 {
        // FAST_FORWARD_MUTE and FAST_FORWARD_RESAMPLE_SOUND are off:
        // Hack: use the beginning of the buffer instead of resampling.
        memcpy(stream_orig as *mut c_void, stream as *const c_void, len_orig as usize);
        free(stream as *mut c_void);
    }
}

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
    let mut version: SDL_version = core::mem::zeroed();
    SDL_GetVersion(&mut version);
    if version.major <= 2 && version.minor <= 0 && version.patch <= 3 {
        desired_audioformat = AUDIO_U8;
        printf(cs!("Your SDL.dll is older than 2.0.4. Using 8-bit audio format to work around resampling bug."));
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
    if SDL_OpenAudio(desired, null_mut()) != 0 {
        sdlperror(cs!("init_digi: SDL_OpenAudio"));
        digi_unavailable = 1;
        return;
    }
    digi_audiospec = desired;
}

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
    while feof(fp) == 0 {
        let mut index: c_int = 0;
        let mut name = [0 as c_char; POP_MAX_PATH];
        if fscanf(fp, cs!("%d=%255s\n"), &mut index, name.as_mut_ptr()) != 2 {
            perror(names_path);
            continue;
        }
        if index >= 0 && index < max_sound_id {
            *sound_names.offset(index as isize) = strdup(name.as_ptr());
        }
    }
    fclose(fp);
}

unsafe fn sound_name(index: c_int) -> *mut c_char {
    if !sound_names.is_null() && index >= 0 && index < max_sound_id {
        *sound_names.offset(index as isize)
    } else {
        null_mut()
    }
}

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
                    snprintf_check!(filename.as_mut_ptr(), POP_MAX_PATH, cs!("%s/music/%s.ogg"), mod_data_path.as_ptr(), sound_name(index));
                    fp = fopen(filename.as_ptr(), cs!("rb"));
                }
                if fp.is_null() && !skip_normal_data_files {
                    snprintf_check!(filename.as_mut_ptr(), POP_MAX_PATH, cs!("data/music/%s.ogg"), sound_name(index));
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
                let mut error: c_int = 0;
                let decoder = stb_vorbis_open_memory(file_contents as *const u8, file_size as c_int, &mut error, null_mut());
                if decoder.is_null() {
                    printf(cs!("Error %d when creating decoder from file \"%s\"!\n"), error, filename.as_ptr());
                    free(file_contents as *mut c_void);
                    break 'do_once;
                }
                result = malloc(core::mem::size_of::<sound_buffer_type>()) as *mut sound_buffer_type;
                (*result).type_ = sound_type_sound_ogg as byte;
                let ogg = core::ptr::addr_of_mut!((*result).__bindgen_anon_1) as *mut ogg_type;
                (*ogg).total_length = (stb_vorbis_stream_length_in_samples(decoder) as usize * core::mem::size_of::<c_short>()) as c_int;
                (*ogg).file_contents = file_contents;
                (*ogg).decoder = decoder;
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
        fprintf(stderr, cs!("Failed to load sound %d '%s'\n"), index, sound_name(index));
    }
    result
}

unsafe fn play_ogg_sound(buffer: *mut sound_buffer_type) {
    init_digi();
    if digi_unavailable != 0 {
        return;
    }
    stop_sounds();
    let ogg = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut ogg_type;
    // Need to rewind the music, or else the decoder might continue where it left off.
    stb_vorbis_seek_start((*ogg).decoder);
    SDL_LockAudio();
    ogg_decoder = (*ogg).decoder;
    SDL_UnlockAudio();
    SDL_PauseAudio(0);
    ogg_playing = 1;
}

#[repr(C)]
struct waveinfo_type {
    sample_rate: c_int,
    sample_size: c_int,
    sample_count: c_int,
    samples: *mut byte,
}

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
        1 => {
            let digi = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut digi_type;
            (*waveinfo).sample_rate = swaple16((*digi).sample_rate) as c_int;
            (*waveinfo).sample_size = (*digi).sample_size as c_int;
            (*waveinfo).sample_count = swaple16((*digi).sample_count) as c_int;
            (*waveinfo).samples = core::ptr::addr_of_mut!((*digi).samples) as *mut byte;
            true
        }
        2 => {
            let digi_new = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut digi_new_type;
            (*waveinfo).sample_rate = swaple16((*digi_new).sample_rate) as c_int;
            (*waveinfo).sample_size = (*digi_new).sample_size as c_int;
            (*waveinfo).sample_count = swaple16((*digi_new).sample_count) as c_int;
            (*waveinfo).samples = core::ptr::addr_of_mut!((*digi_new).samples) as *mut byte;
            true
        }
        3 => {
            printf(cs!("Warning: Ambiguous wave version.\n"));
            false
        }
        _ => {
            printf(cs!("Warning: Can't determine wave version.\n"));
            false
        }
    }
}

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

    for i in 0..expanded_frames {
        let src_frame_float: f32 = i as f32 * freq_ratio;
        let src_frame_0 = src_frame_float as c_int; // truncation

        let sample_0 = ((*source.offset(src_frame_0 as isize) as c_int) | ((*source.offset(src_frame_0 as isize) as c_int) << 8)) - 32768;
        let interpolated_sample: c_short;
        if src_frame_0 >= waveinfo.sample_count - 1 {
            interpolated_sample = sample_0 as c_short;
        } else {
            let src_frame_1 = src_frame_0 + 1;
            let alpha: f32 = src_frame_float - src_frame_0 as f32;
            let sample_1 = ((*source.offset(src_frame_1 as isize) as c_int) | ((*source.offset(src_frame_1 as isize) as c_int) << 8)) - 32768;
            interpolated_sample = ((1.0f32 - alpha) * sample_0 as f32 + alpha * sample_1 as f32) as c_short;
        }
        let mut channel = 0;
        while channel < (*digi_audiospec).channels as c_int {
            *dest = interpolated_sample;
            dest = dest.add(1);
            channel += 1;
        }
    }

    converted_buffer
}

// seg009:74F0 play_digi_sound
unsafe fn play_digi_sound(buffer: *mut sound_buffer_type) {
    init_digi();
    if digi_unavailable != 0 {
        return;
    }
    stop_digi();
    if ((*buffer).type_ & 7) != sound_type_sound_digi_converted as byte {
        printf(cs!("Tried to play unconverted digi sound.\n"));
        return;
    }
    let converted = core::ptr::addr_of!((*buffer).__bindgen_anon_1) as *const converted_audio_type;
    SDL_LockAudio();
    digi_buffer = (*converted).samples as *mut byte;
    digi_playing = 1;
    digi_remaining_length = (*converted).length;
    digi_remaining_pos = digi_buffer;
    SDL_UnlockAudio();
    SDL_PauseAudio(0);
}

// seg009 free_sound
#[no_mangle]
pub unsafe extern "C" fn free_sound(buffer: *mut sound_buffer_type) {
    if buffer.is_null() {
        return;
    }
    if (*buffer).type_ == sound_type_sound_ogg as byte {
        let ogg = core::ptr::addr_of_mut!((*buffer).__bindgen_anon_1) as *mut ogg_type;
        stb_vorbis_close((*ogg).decoder);
        free((*ogg).file_contents as *mut c_void);
    }
    free(buffer as *mut c_void);
}

// seg009:7220 play_sound_from_buffer
#[no_mangle]
pub unsafe extern "C" fn play_sound_from_buffer(buffer: *mut sound_buffer_type) {
    if replaying != 0 && skipping_replay != 0 {
        return;
    }
    if buffer.is_null() {
        printf(cs!("Tried to play NULL sound.\n"));
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
            printf(cs!("Tried to play unimplemented sound type %d.\n"), (*buffer).type_ as c_int);
            quit(1);
        }
    }
}

// seg009 turn_music_on_off
#[no_mangle]
pub unsafe extern "C" fn turn_music_on_off(new_state: byte) {
    enable_music = new_state;
    turn_sound_on_off(is_sound_on);
}

// seg009:7273 turn_sound_on_off
#[no_mangle]
pub unsafe extern "C" fn turn_sound_on_off(new_state: byte) {
    is_sound_on = new_state;
}

// seg009:7299 check_sound_playing
#[no_mangle]
pub unsafe extern "C" fn check_sound_playing() -> c_int {
    (speaker_playing != 0 || digi_playing != 0 || midi_playing != 0 || ogg_playing != 0) as c_int
}

// seg009:9289 set_pal_arr
#[no_mangle]
pub unsafe extern "C" fn set_pal_arr(start: c_int, count: c_int, array: *const rgb_type) {
    for i in 0..count {
        if !array.is_null() {
            let p = array.offset(i as isize);
            set_pal(start + i, (*p).r as c_int, (*p).g as c_int, (*p).b as c_int);
        } else {
            set_pal(start + i, 0, 0, 0);
        }
    }
}

// seg009:92DF set_pal
#[no_mangle]
pub unsafe extern "C" fn set_pal(index: c_int, red: c_int, green: c_int, blue: c_int) {
    palette[index as usize].r = red as byte;
    palette[index as usize].g = green as byte;
    palette[index as usize].b = blue as byte;
}

// seg009:969C add_palette_bits
#[no_mangle]
pub unsafe extern "C" fn add_palette_bits(_n_colors: byte) -> c_int {
    0
}

// seg009:9C36 find_first_pal_row
#[no_mangle]
pub unsafe extern "C" fn find_first_pal_row(which_rows_mask: c_int) -> c_int {
    let mut which_row: word = 0;
    let mut row_mask: word = 1;
    loop {
        if (row_mask as c_int) & which_rows_mask != 0 {
            return which_row as c_int;
        }
        which_row += 1;
        row_mask <<= 1;
        if !(which_row < 16) {
            break;
        }
    }
    0
}

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
            let mut i: c_int = 0;
            while i < res_count {
                if swaple16((*entries.offset(i as isize)).id) as c_int == resource_id {
                    break;
                }
                i += 1;
            }
            if i < res_count {
                // found
                *result = data_location_data_DAT;
                *size = swaple16((*entries.offset(i as isize)).size) as c_int;
                if strcmp(extension, cs!("png")) == 0 && *size <= 2 {
                    // Skip empty images in DATs, so we can fall back to directories.
                    fp = null_mut();
                    *result = data_location_data_none;
                    *size = 0;
                } else if fseek(fp, swaple32((*entries.offset(i as isize)).offset) as c_long, SEEK_SET) != 0
                    || fread(checksum as *mut c_void, 1, 1, fp) != 1
                {
                    printf(cs!("Cannot seek or cannot read checksum: "));
                    perror(core::ptr::addr_of!((*pointer).filename) as *const c_char);
                    fp = null_mut();
                }
            } else {
                // not found
                fp = null_mut();
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
            snprintf_check!(image_filename.as_mut_ptr(), POP_MAX_PATH, cs!("data/%s/res%d.%s"), filename_no_ext.as_ptr(), resource_id, extension);
            if use_custom_levelset == 0 {
                let mut __lf = [0 as c_char; POP_MAX_PATH];
                fp = fopen(locate_file_(image_filename.as_ptr(), __lf.as_mut_ptr(), POP_MAX_PATH as c_int), cs!("rb"));
            } else {
                if !skip_mod_data_files {
                    let mut image_filename_mod = [0 as c_char; POP_MAX_PATH];
                    snprintf_check!(image_filename_mod.as_mut_ptr(), POP_MAX_PATH, cs!("%s/%s"), mod_data_path.as_ptr(), image_filename.as_ptr());
                    let mut __lf = [0 as c_char; POP_MAX_PATH];
                    fp = fopen(locate_file_(image_filename_mod.as_ptr(), __lf.as_mut_ptr(), POP_MAX_PATH as c_int), cs!("rb"));
                }
                if fp.is_null() && !skip_normal_data_files {
                    let mut __lf = [0 as c_char; POP_MAX_PATH];
                    fp = fopen(locate_file_(image_filename.as_ptr(), __lf.as_mut_ptr(), POP_MAX_PATH as c_int), cs!("rb"));
                }
            }
            if !fp.is_null() {
                let mut buf: stat_t = core::mem::zeroed();
                if fstat(fileno(fp), &mut buf) == 0 {
                    *result = data_location_data_directory;
                    *size = buf.st_size as c_int;
                } else {
                    printf(cs!("Cannot fstat: "));
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
        fprintf(stderr, cs!("%s: %s, resource %d, size %d, failed: %s\n"), cs!("load_from_opendats_alloc"), core::ptr::addr_of!((*pointer).filename) as *const c_char, resource, size, strerror(errno()));
        free(area);
        area = null_mut();
    }
    if result == data_location_data_directory {
        fclose(fp);
    }
    area
}

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
        fprintf(stderr, cs!("%s: %s, resource %d, size %d, failed: %s\n"), cs!("load_from_opendats_to_area"), core::ptr::addr_of!((*pointer).filename) as *const c_char, resource, size, strerror(errno()));
        memset(area, 0, MIN_i(size, length) as usize);
    }
    if result == data_location_data_directory {
        fclose(fp);
    }
    0
}

// seg009 rect_to_sdlrect
#[no_mangle]
pub unsafe extern "C" fn rect_to_sdlrect(rect: *const rect_type, sdlrect: *mut SDL_Rect) {
    (*sdlrect).x = (*rect).left as c_int;
    (*sdlrect).y = (*rect).top as c_int;
    (*sdlrect).w = ((*rect).right - (*rect).left) as c_int;
    (*sdlrect).h = ((*rect).bottom - (*rect).top) as c_int;
}

// seg009 method_1_blit_rect
#[no_mangle]
pub unsafe extern "C" fn method_1_blit_rect(target_surface: *mut surface_type, source_surface: *mut surface_type, target_rect: *const rect_type, source_rect: *const rect_type, blit: c_int) {
    let mut src_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(source_rect, &mut src_rect);
    let mut dest_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(target_rect, &mut dest_rect);

    if blit == blitters_blitters_0_no_transp as c_int {
        // Disable transparency.
        if SDL_SetColorKey(source_surface, 0, 0) != 0 {
            sdlperror(cs!("method_1_blit_rect: SDL_SetColorKey"));
            quit(1);
        }
    } else {
        // Enable transparency.
        if SDL_SetColorKey(source_surface, SDL_TRUE, 0) != 0 {
            sdlperror(cs!("method_1_blit_rect: SDL_SetColorKey"));
            quit(1);
        }
    }
    if SDL_BlitSurface(source_surface, &src_rect, target_surface, &mut dest_rect) != 0 {
        sdlperror(cs!("method_1_blit_rect: SDL_BlitSurface"));
        quit(1);
    }
}

// seg009 method_3_blit_mono
#[no_mangle]
pub unsafe extern "C" fn method_3_blit_mono(image: *mut image_type, xpos: c_int, ypos: c_int, _blitter: c_int, color: byte) -> *mut image_type {
    let w = (*image).w;
    let h = (*image).h;
    if SDL_SetColorKey(image, SDL_TRUE, 0) != 0 {
        sdlperror(cs!("method_3_blit_mono: SDL_SetColorKey"));
        quit(1);
    }
    let colored_image = SDL_ConvertSurfaceFormat(image, SDL_PIXELFORMAT_ARGB8888, 0);

    SDL_SetSurfaceBlendMode(colored_image, SDL_BLENDMODE_NONE);

    if SDL_LockSurface(colored_image) != 0 {
        sdlperror(cs!("method_3_blit_mono: SDL_LockSurface"));
        quit(1);
    }

    let pr = palette[color as usize].r;
    let pg = palette[color as usize].g;
    let pb = palette[color as usize].b;
    let rgb_color: u32 = SDL_MapRGB((*colored_image).format, ((pr as c_int) << 2) as u8, ((pg as c_int) << 2) as u8, ((pb as c_int) << 2) as u8) & 0xFFFFFF;
    let stride = (*colored_image).pitch;
    for y in 0..h {
        let mut pixel_ptr = ((*colored_image).pixels as *mut byte).offset((stride * y) as isize) as *mut u32;
        for _x in 0..w {
            *pixel_ptr = (*pixel_ptr & 0xFF000000) | rgb_color;
            pixel_ptr = pixel_ptr.add(1);
        }
    }
    SDL_UnlockSurface(colored_image);

    let src_rect = SDL_Rect { x: 0, y: 0, w: (*image).w, h: (*image).h };
    let mut dest_rect = SDL_Rect { x: xpos, y: ypos, w: (*image).w, h: (*image).h };

    SDL_SetSurfaceBlendMode(colored_image, SDL_BLENDMODE_BLEND);
    SDL_SetSurfaceBlendMode(current_target_surface, SDL_BLENDMODE_BLEND);
    SDL_SetSurfaceAlphaMod(colored_image, 255);
    if SDL_BlitSurface(colored_image, &src_rect, current_target_surface, &mut dest_rect) != 0 {
        sdlperror(cs!("method_3_blit_mono: SDL_BlitSurface"));
        quit(1);
    }
    SDL_FreeSurface(colored_image);

    image
}

unsafe fn RGB24_bug_check() -> bool {
    if !RGB24_bug_checked {
        let test_surface = SDL_CreateRGBSurface(0, 1, 1, 24, 0, 0, 0, 0);
        if test_surface.is_null() {
            sdlperror(cs!("SDL_CreateSurface in RGB24_bug_check"));
        }
        SDL_FillRect(test_surface, core::ptr::null(), SDL_MapRGB((*test_surface).format, 0xFF, 0, 0));
        if SDL_LockSurface(test_surface) != 0 {
            sdlperror(cs!("SDL_LockSurface in RGB24_bug_check"));
        }
        RGB24_bug_affected = (*((*test_surface).pixels as *const u32) & (*(*test_surface).format).Rmask) == 0;
        SDL_UnlockSurface(test_surface);
        SDL_FreeSurface(test_surface);
        RGB24_bug_checked = true;
    }
    RGB24_bug_affected
}

unsafe fn safe_SDL_FillRect(dst: *mut SDL_Surface, rect: *const SDL_Rect, mut color: u32) -> c_int {
    if (*(*dst).format).BitsPerPixel == 24 && RGB24_bug_check() {
        color = ((color & 0xFF) << 16) | (color & 0xFF00) | ((color & 0xFF0000) >> 16);
    }
    SDL_FillRect(dst, rect, color)
}

// seg009 method_5_rect
#[no_mangle]
pub unsafe extern "C" fn method_5_rect(rect: *const rect_type, _blit: c_int, color: byte) -> *const rect_type {
    let mut dest_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(rect, &mut dest_rect);
    let pr = palette[color as usize].r;
    let pg = palette[color as usize].g;
    let pb = palette[color as usize].b;
    let rgb_color: u32 = SDL_MapRGBA((*current_target_surface).format, ((pr as c_int) << 2) as u8, ((pg as c_int) << 2) as u8, ((pb as c_int) << 2) as u8, 0xFF);
    if safe_SDL_FillRect(current_target_surface, &dest_rect, rgb_color) != 0 {
        sdlperror(cs!("method_5_rect: SDL_FillRect"));
        quit(1);
    }
    rect
}

// seg009 draw_rect_with_alpha
#[no_mangle]
pub unsafe extern "C" fn draw_rect_with_alpha(rect: *const rect_type, color: byte, alpha: byte) {
    let mut dest_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(rect, &mut dest_rect);
    let pr = palette[color as usize].r;
    let pg = palette[color as usize].g;
    let pb = palette[color as usize].b;
    let rgb_color: u32 = SDL_MapRGBA((*overlay_surface).format, ((pr as c_int) << 2) as u8, ((pg as c_int) << 2) as u8, ((pb as c_int) << 2) as u8, alpha);
    if safe_SDL_FillRect(current_target_surface, &dest_rect, rgb_color) != 0 {
        sdlperror(cs!("draw_rect_with_alpha: SDL_FillRect"));
        quit(1);
    }
}

// seg009 draw_rect_contours
#[no_mangle]
pub unsafe extern "C" fn draw_rect_contours(rect: *const rect_type, color: byte) {
    if (*(*current_target_surface).format).BitsPerPixel != 32 {
        printf(cs!("draw_rect_contours: not implemented for %d bit surfaces\n"), (*(*current_target_surface).format).BitsPerPixel as c_int);
        return;
    }
    let mut dest_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(rect, &mut dest_rect);
    let pr = palette[color as usize].r;
    let pg = palette[color as usize].g;
    let pb = palette[color as usize].b;
    let rgb_color: u32 = SDL_MapRGBA((*overlay_surface).format, ((pr as c_int) << 2) as u8, ((pg as c_int) << 2) as u8, ((pb as c_int) << 2) as u8, 0xFF);
    if SDL_LockSurface(current_target_surface) != 0 {
        sdlperror(cs!("draw_rect_contours: SDL_LockSurface"));
        quit(1);
    }
    let bytes_per_pixel = (*(*current_target_surface).format).BytesPerPixel as c_int;
    let pitch = (*current_target_surface).pitch;
    let pixels = (*current_target_surface).pixels as *mut byte;
    let xmin = MIN_i(dest_rect.x, (*current_target_surface).w);
    let xmax = MIN_i(dest_rect.x + dest_rect.w, (*current_target_surface).w);
    let ymin = MIN_i(dest_rect.y, (*current_target_surface).h);
    let ymax = MIN_i(dest_rect.y + dest_rect.h, (*current_target_surface).h);
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

    SDL_UnlockSurface(current_target_surface);
}

unsafe fn blit_xor(target_surface: *mut SDL_Surface, dest_rect: *mut SDL_Rect, image: *mut SDL_Surface, src_rect: *mut SDL_Rect) {
    if (*dest_rect).w != (*src_rect).w || (*dest_rect).h != (*src_rect).h {
        printf(cs!("blit_xor: dest_rect and src_rect have different sizes\n"));
        quit(1);
    }
    let helper_surface = SDL_CreateRGBSurface(0, (*dest_rect).w, (*dest_rect).h, 24, Rmsk, Gmsk, Bmsk, 0);
    if helper_surface.is_null() {
        sdlperror(cs!("blit_xor: SDL_CreateRGBSurface"));
        quit(1);
    }
    let image_24 = SDL_ConvertSurface(image, (*helper_surface).format, 0);
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
    if SDL_LockSurface(image_24) != 0 {
        sdlperror(cs!("blit_xor: SDL_LockSurface"));
        quit(1);
    }
    if SDL_LockSurface(helper_surface) != 0 {
        sdlperror(cs!("blit_xor: SDL_LockSurface"));
        quit(1);
    }
    let size = (*helper_surface).h * (*helper_surface).pitch;
    let mut p_src = (*image_24).pixels as *mut byte;
    let mut p_dest = (*helper_surface).pixels as *mut byte;

    // Xor the old area with the image.
    for _i in 0..size {
        *p_dest ^= *p_src;
        p_src = p_src.add(1);
        p_dest = p_dest.add(1);
    }
    SDL_UnlockSurface(image_24);
    SDL_UnlockSurface(helper_surface);
    // Put the new area in place of the old one.
    if SDL_BlitSurface(helper_surface, src_rect, target_surface, dest_rect) != 0 {
        sdlperror(cs!("blit_xor: SDL_BlitSurface 2065"));
        quit(1);
    }
    SDL_FreeSurface(image_24);
    SDL_FreeSurface(helper_surface);
}

// USE_COLORED_TORCHES
unsafe fn draw_colored_torch(color: c_int, image: *mut SDL_Surface, xpos: c_int, ypos: c_int) {
    if SDL_SetColorKey(image, SDL_TRUE, 0) != 0 {
        sdlperror(cs!("draw_colored_torch: SDL_SetColorKey"));
        quit(1);
    }

    let colored_image = SDL_ConvertSurfaceFormat(image, SDL_PIXELFORMAT_ARGB8888, 0);
    SDL_SetSurfaceBlendMode(colored_image, SDL_BLENDMODE_NONE);

    if SDL_LockSurface(colored_image) != 0 {
        sdlperror(cs!("draw_colored_torch: SDL_LockSurface"));
        quit(1);
    }

    let w = (*colored_image).w;
    let h = (*colored_image).h;
    let iRed = ((color >> 4) & 3) * 85;
    let iGreen = ((color >> 2) & 3) * 85;
    let iBlue = ((color >> 0) & 3) * 85;
    let old_color: u32 = SDL_MapRGB((*colored_image).format, 0xFC, 0x84, 0x00) & 0xFFFFFF;
    let new_color: u32 = SDL_MapRGB((*colored_image).format, iRed as u8, iGreen as u8, iBlue as u8) & 0xFFFFFF;
    let stride = (*colored_image).pitch;
    for y in 0..h {
        let mut pixel_ptr = ((*colored_image).pixels as *mut byte).offset((stride * y) as isize) as *mut u32;
        for _x in 0..w {
            if (*pixel_ptr & 0xFFFFFF) == old_color {
                *pixel_ptr = (*pixel_ptr & 0xFF000000) | new_color;
            }
            pixel_ptr = pixel_ptr.add(1);
        }
    }
    SDL_UnlockSurface(colored_image);

    method_6_blit_img_to_scr(colored_image, xpos, ypos, blitters_blitters_0_no_transp as c_int);
    SDL_FreeSurface(colored_image);
}

// seg009 method_6_blit_img_to_scr
#[no_mangle]
pub unsafe extern "C" fn method_6_blit_img_to_scr(image: *mut image_type, xpos: c_int, ypos: c_int, blit: c_int) -> *mut image_type {
    if image.is_null() {
        printf(cs!("method_6_blit_img_to_scr: image == NULL\n"));
        return null_mut();
    }

    if blit == blitters_blitters_9_black as c_int {
        method_3_blit_mono(image, xpos, ypos, blitters_blitters_9_black as c_int, 0);
        return image;
    }

    let mut src_rect = SDL_Rect { x: 0, y: 0, w: (*image).w, h: (*image).h };
    let mut dest_rect = SDL_Rect { x: xpos, y: ypos, w: (*image).w, h: (*image).h };

    if blit == blitters_blitters_3_xor as c_int {
        blit_xor(current_target_surface, &mut dest_rect, image, &mut src_rect);
        return image;
    }

    if blit >= blitters_blitters_colored_flame as c_int && blit <= blitters_blitters_colored_flame_last as c_int {
        draw_colored_torch(blit - blitters_blitters_colored_flame as c_int, image, xpos, ypos);
        return image;
    }

    SDL_SetSurfaceBlendMode(image, SDL_BLENDMODE_NONE);
    SDL_SetColorKey(image, SDL_FALSE, 0);
    SDL_SetSurfaceAlphaMod(image, 255);

    if blit == blitters_blitters_0_no_transp as c_int {
        if SDL_ISPIXELFORMAT_INDEXED((*(*image).format).format) {
            SDL_SetColorKey(image, SDL_FALSE, 0);
        } else {
            SDL_SetSurfaceBlendMode(image, SDL_BLENDMODE_NONE);
        }
    } else {
        if SDL_ISPIXELFORMAT_INDEXED((*(*image).format).format) {
            SDL_SetColorKey(image, SDL_TRUE, 0);
        } else {
            SDL_SetSurfaceBlendMode(image, SDL_BLENDMODE_BLEND);
        }
    }
    if SDL_BlitSurface(image, &src_rect, current_target_surface, &mut dest_rect) != 0 {
        sdlperror(cs!("method_6_blit_img_to_scr: SDL_BlitSurface 2247"));
    }
    image
}

// seg009 apply_aspect_ratio
#[no_mangle]
pub unsafe extern "C" fn apply_aspect_ratio() {
    if use_correct_aspect_ratio != 0 {
        SDL_RenderSetLogicalSize(renderer_, 320 * 5, 200 * 6); // 4:3
    } else {
        SDL_RenderSetLogicalSize(renderer_, 320, 200); // 16:10
    }
    window_resized();
}

// seg009 window_resized
#[no_mangle]
pub unsafe extern "C" fn window_resized() {
    if use_integer_scaling != 0 {
        let mut window_width: c_int = 0;
        let mut window_height: c_int = 0;
        SDL_GetRendererOutputSize(renderer_, &mut window_width, &mut window_height);
        let mut render_width: c_int = 0;
        let mut render_height: c_int = 0;
        SDL_RenderGetLogicalSize(renderer_, &mut render_width, &mut render_height);
        let makes_sense = (window_width >= render_width && window_height >= render_height) as c_int;
        SDL_RenderSetIntegerScale(renderer_, makes_sense);
    }
}

unsafe fn init_overlay() {
    if !overlay_initialized {
        overlay_surface = SDL_CreateRGBSurface(0, 320, 200, 32, Rmsk, Gmsk, Bmsk, Amsk);
        merged_surface = SDL_CreateRGBSurface(0, 320, 200, 24, Rmsk, Gmsk, Bmsk, 0);
        overlay_initialized = true;
    }
}

unsafe fn init_scaling() {
    // Don't crash in validate mode.
    if renderer_.is_null() {
        return;
    }
    if texture_sharp.is_null() {
        texture_sharp = SDL_CreateTexture(renderer_, SDL_PIXELFORMAT_RGB24, SDL_TEXTUREACCESS_STREAMING, 320, 200);
    }
    if scaling_type == 1 {
        if !is_renderer_targettexture_supported && onscreen_surface_2x.is_null() {
            onscreen_surface_2x = SDL_CreateRGBSurface(0, 320 * 2, 200 * 2, 24, Rmsk, Gmsk, Bmsk, 0);
        }
        if texture_fuzzy.is_null() {
            SDL_SetHint(SDL_HINT_RENDER_SCALE_QUALITY.as_ptr() as *const c_char, cs!("1"));
            let access = if is_renderer_targettexture_supported { SDL_TEXTUREACCESS_TARGET } else { SDL_TEXTUREACCESS_STREAMING };
            texture_fuzzy = SDL_CreateTexture(renderer_, SDL_PIXELFORMAT_RGB24, access, 320 * 2, 200 * 2);
            SDL_SetHint(SDL_HINT_RENDER_SCALE_QUALITY.as_ptr() as *const c_char, cs!("0"));
        }
        target_texture = texture_fuzzy;
    } else if scaling_type == 2 {
        if texture_blurry.is_null() {
            SDL_SetHint(SDL_HINT_RENDER_SCALE_QUALITY.as_ptr() as *const c_char, cs!("1"));
            texture_blurry = SDL_CreateTexture(renderer_, SDL_PIXELFORMAT_RGB24, SDL_TEXTUREACCESS_STREAMING, 320, 200);
            SDL_SetHint(SDL_HINT_RENDER_SCALE_QUALITY.as_ptr() as *const c_char, cs!("0"));
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

// seg009:38ED set_gr_mode
#[no_mangle]
pub unsafe extern "C" fn set_gr_mode(_grmode: byte) {
    SDL_SetHint(SDL_HINT_WINDOWS_DISABLE_THREAD_NAMING.as_ptr() as *const c_char, cs!("1"));
    if SDL_Init(SDL_INIT_VIDEO | SDL_INIT_TIMER | SDL_INIT_NOPARACHUTE | SDL_INIT_GAMECONTROLLER) != 0 {
        sdlperror(cs!("set_gr_mode: SDL_Init"));
        quit(1);
    }
    if enable_controller_rumble != 0 {
        if SDL_InitSubSystem(SDL_INIT_HAPTIC) != 0 {
            printf(cs!("Warning: Haptic subsystem unavailable, ignoring enable_controller_rumble = true\n"));
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
        window_ = SDL_CreateWindow(
            cs!("Prince of Persia (SDLPoP) v1.24 RC"),
            SDL_WINDOWPOS_UNDEFINED,
            SDL_WINDOWPOS_UNDEFINED,
            pop_window_width as c_int,
            pop_window_height as c_int,
            flags,
        );
    }
    // Make absolutely sure that VSync will be off, to prevent timer issues.
    SDL_SetHint(SDL_HINT_RENDER_VSYNC.as_ptr() as *const c_char, cs!("0"));
    flags = 0;
    match use_hardware_acceleration {
        0 => {
            flags |= SDL_RENDERER_SOFTWARE;
        }
        1 => {
            flags |= SDL_RENDERER_ACCELERATED;
        }
        _ => {}
    }
    renderer_ = SDL_CreateRenderer(window_, -1, flags | SDL_RENDERER_TARGETTEXTURE);
    let mut renderer_info: SDL_RendererInfo = core::mem::zeroed();
    if SDL_GetRendererInfo(renderer_, &mut renderer_info) == 0 {
        if renderer_info.flags & SDL_RENDERER_TARGETTEXTURE != 0 {
            is_renderer_targettexture_supported = true;
        }
    }
    if use_integer_scaling != 0 {
        SDL_RenderSetIntegerScale(renderer_, SDL_TRUE);
    }

    let mut __icon_lf = [0 as c_char; POP_MAX_PATH];
    let icon = IMG_Load(locate_file_(cs!("data/icon.png"), __icon_lf.as_mut_ptr(), POP_MAX_PATH as c_int));
    if icon.is_null() {
        sdlperror(cs!("set_gr_mode: Could not load icon"));
    } else {
        SDL_SetWindowIcon(window_, icon);
    }

    apply_aspect_ratio();
    window_resized();

    onscreen_surface_ = SDL_CreateRGBSurface(0, 320, 200, 24, Rmsk, Gmsk, Bmsk, 0);
    if onscreen_surface_.is_null() {
        sdlperror(cs!("set_gr_mode: SDL_CreateRGBSurface"));
        quit(1);
    }
    init_overlay();
    init_scaling();
    if start_fullscreen != 0 {
        SDL_ShowCursor(SDL_DISABLE);
    }

    graphics_mode = grmodes_gmMcgaVga as byte;
    load_font();
}

// seg009 get_final_surface
#[no_mangle]
pub unsafe extern "C" fn get_final_surface() -> *mut SDL_Surface {
    if !is_overlay_displayed {
        onscreen_surface_
    } else {
        merged_surface
    }
}

unsafe fn draw_overlay() {
    let mut overlay: c_int = 0;
    is_overlay_displayed = false;
    if is_timer_displayed != 0 && start_level > 0 {
        overlay = 1; // Timer overlay
    } else if (*fixes).fix_quicksave_during_feather != 0
        && is_feather_timer_displayed != 0
        && start_level > 0
        && is_feather_fall > 0
    {
        overlay = 3; // Feather timer overlay
    }
    // Menu overlay
    if is_paused != 0 && is_menu_shown != 0 {
        overlay = 2;
    }
    if overlay != 0 {
        is_overlay_displayed = true;
        let saved_target_surface = current_target_surface;
        current_target_surface = overlay_surface;
        let drawn_rect: rect_type;
        if overlay == 1 {
            let mut timer_text = [0 as c_char; 32];
            if rem_min < 0 {
                snprintf(timer_text.as_mut_ptr(), 32, cs!("%02d:%02d:%02d"), -((rem_min as c_int) + 1), (719 - rem_tick as c_int) / 12, (719 - rem_tick as c_int) % 12);
            } else {
                snprintf(timer_text.as_mut_ptr(), 32, cs!("%02d:%02d:%02d"), rem_min as c_int - 1, rem_tick as c_int / 12, rem_tick as c_int % 12);
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
                snprintf(ticks_text.as_mut_ptr(), 12, cs!("T: %d"), curr_tick);
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
        } else if overlay == 3 {
            // Feather timer
            let mut timer_text = [0 as c_char; 32];
            let ticks_per_sec = get_ticks_per_sec(timerids_timer_1 as c_int) as c_int;
            snprintf(timer_text.as_mut_ptr(), 32, cs!("%02d:%02d"), is_feather_fall as c_int / ticks_per_sec, is_feather_fall as c_int % ticks_per_sec);
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

// seg009 update_screen
#[no_mangle]
pub unsafe extern "C" fn update_screen() {
    draw_overlay();
    let mut surface = get_final_surface();
    init_scaling();
    if scaling_type == 1 {
        // Make "fuzzy pixels" like DOSBox does.
        if is_renderer_targettexture_supported {
            SDL_UpdateTexture(texture_sharp, core::ptr::null(), (*surface).pixels, (*surface).pitch);
            SDL_SetHint(SDL_HINT_RENDER_SCALE_QUALITY.as_ptr() as *const c_char, cs!("1"));
            SDL_SetRenderTarget(renderer_, target_texture);
            SDL_SetHint(SDL_HINT_RENDER_SCALE_QUALITY.as_ptr() as *const c_char, cs!("0"));
            SDL_RenderClear(renderer_);
            SDL_RenderCopy(renderer_, texture_sharp, core::ptr::null(), core::ptr::null());
            SDL_SetRenderTarget(renderer_, null_mut());
        } else {
            SDL_BlitScaled(surface, core::ptr::null(), onscreen_surface_2x, null_mut());
            surface = onscreen_surface_2x;
            SDL_UpdateTexture(target_texture, core::ptr::null(), (*surface).pixels, (*surface).pitch);
        }
    } else {
        SDL_UpdateTexture(target_texture, core::ptr::null(), (*surface).pixels, (*surface).pitch);
    }
    SDL_RenderClear(renderer_);
    SDL_RenderCopy(renderer_, target_texture, core::ptr::null(), core::ptr::null());
    SDL_RenderPresent(renderer_);
}

// seg009 reset_timer
#[no_mangle]
pub unsafe extern "C" fn reset_timer(timer_index: c_int) {
    timer_last_counter[timer_index as usize] = SDL_GetPerformanceCounter();
}

// seg009 get_ticks_per_sec
#[no_mangle]
pub unsafe extern "C" fn get_ticks_per_sec(timer_index: c_int) -> f64 {
    fps as f64 / wait_time[timer_index as usize] as f64
}

unsafe fn recalculate_feather_fall_timer(previous_ticks_per_second: f64, ticks_per_second: f64) {
    let m = if previous_ticks_per_second > ticks_per_second { previous_ticks_per_second } else { ticks_per_second };
    if (is_feather_fall as f64) <= m || previous_ticks_per_second == ticks_per_second {
        return;
    }
    is_feather_fall = (is_feather_fall as f64 / previous_ticks_per_second * ticks_per_second) as word;
}

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

// seg009 start_timer
#[no_mangle]
pub unsafe extern "C" fn start_timer(timer_index: c_int, length: c_int) {
    if replaying != 0 && skipping_replay != 0 {
        return;
    }
    timer_last_counter[timer_index as usize] = SDL_GetPerformanceCounter();
    wait_time[timer_index as usize] = length;
}

unsafe fn toggle_fullscreen() {
    let flags = SDL_GetWindowFlags(window_);
    if flags & SDL_WINDOW_FULLSCREEN_DESKTOP != 0 {
        SDL_SetWindowFullscreen(window_, 0);
        SDL_ShowCursor(SDL_ENABLE);
    } else {
        SDL_SetWindowFullscreen(window_, SDL_WINDOW_FULLSCREEN_DESKTOP);
        SDL_ShowCursor(SDL_DISABLE);
    }
}

// seg009 process_events
#[no_mangle]
pub unsafe extern "C" fn process_events() {
    let mut event: SDL_Event = core::mem::zeroed();
    while SDL_PollEvent(&mut event) == 1 {
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
                SDL_GameControllerOpen(event.cdevice.which);
                if gamecontrollerdb_file[0] != 0 {
                    SDL_GameControllerAddMappingsFromFile(gamecontrollerdb_file.as_ptr());
                }
                is_joyst_mode = 1;
                using_sdl_joystick_interface = 0;
            }
            x if x == SDL_CONTROLLERDEVICEREMOVED => {
                if sdl_controller_ == SDL_GameControllerFromInstanceID(event.cdevice.which) {
                    sdl_controller_ = null_mut();
                    is_joyst_mode = 0;
                    is_keyboard_mode = 1;
                }
                SDL_GameControllerClose(SDL_GameControllerFromInstanceID(event.cdevice.which));
            }
            x if x == SDL_CONTROLLERBUTTONDOWN => {
                sdl_controller_ = SDL_GameControllerFromInstanceID(event.cdevice.which);
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
                    let state = SDL_GetKeyboardState(core::ptr::null_mut());
                    if *state.offset(SDL_SCANCODE_TAB as isize) != 0 {
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

// seg009 idle
#[no_mangle]
pub unsafe extern "C" fn idle() {
    process_events();
    update_screen();
}

// seg009 do_simple_wait
#[no_mangle]
pub unsafe extern "C" fn do_simple_wait(timer_index: c_int) {
    if (replaying != 0 && skipping_replay != 0) || is_validate_mode != 0 {
        return;
    }
    update_screen();
    while has_timer_stopped(timer_index) == 0 {
        SDL_Delay(1);
        process_events();
    }
}

// seg009 do_wait
#[no_mangle]
pub unsafe extern "C" fn do_wait(timer_index: c_int) -> c_int {
    if (replaying != 0 && skipping_replay != 0) || is_validate_mode != 0 {
        return 0;
    }
    update_screen();
    while has_timer_stopped(timer_index) == 0 {
        SDL_Delay(1);
        process_events();
        let key = do_paused();
        if key != 0 && (word_1D63A != 0 || key == 0x1B) {
            return 1;
        }
    }
    0
}

// seg009:78E9 init_timer
#[no_mangle]
pub unsafe extern "C" fn init_timer(frequency: c_int) {
    perf_frequency = SDL_GetPerformanceFrequency();
    fps = frequency;
    milliseconds_per_tick = 1000.0f32 / fps as f32;
    perf_counters_per_tick = perf_frequency / fps as u64;
    milliseconds_per_counter = 1000.0f32 / perf_frequency as f32;
}

// seg009:35F6 set_clip_rect
#[no_mangle]
pub unsafe extern "C" fn set_clip_rect(rect: *const rect_type) {
    let mut clip_rect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(rect, &mut clip_rect);
    SDL_SetClipRect(current_target_surface, &clip_rect);
}

// seg009:365C reset_clip_rect
#[no_mangle]
pub unsafe extern "C" fn reset_clip_rect() {
    SDL_SetClipRect(current_target_surface, core::ptr::null());
}

// seg009:1983 set_bg_attr
#[no_mangle]
pub unsafe extern "C" fn set_bg_attr(vga_pal_index: c_int, hc_pal_index: c_int) {
    if enable_flash == 0 {
        return;
    }
    if vga_pal_index == 0 {
        // Make the black pixels transparent.
        if SDL_SetColorKey(offscreen_surface, SDL_TRUE, 0) != 0 {
            sdlperror(cs!("set_bg_attr: SDL_SetColorKey"));
            quit(1);
        }
        let mut rect = SDL_Rect { x: 0, y: 0, w: 0, h: 0 };
        rect.w = (*offscreen_surface).w;
        rect.h = (*offscreen_surface).h;
        let pr = palette[hc_pal_index as usize].r;
        let pg = palette[hc_pal_index as usize].g;
        let pb = palette[hc_pal_index as usize].b;
        let rgb_color: u32 = SDL_MapRGB((*onscreen_surface_).format, ((pr as c_int) << 2) as u8, ((pg as c_int) << 2) as u8, ((pb as c_int) << 2) as u8);
        // First clear the screen with the color of the flash.
        if safe_SDL_FillRect(onscreen_surface_, &rect, rgb_color) != 0 {
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
        if SDL_SetColorKey(offscreen_surface, 0, 0) != 0 {
            sdlperror(cs!("set_bg_attr: SDL_SetColorKey"));
            quit(1);
        }
    }
}

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

// seg009:3AA5 offset2_rect
#[no_mangle]
pub unsafe extern "C" fn offset2_rect(dest: *mut rect_type, source: *const rect_type, delta_x: c_int, delta_y: c_int) -> *mut rect_type {
    (*dest).top = ((*source).top as c_int + delta_y) as c_short;
    (*dest).left = ((*source).left as c_int + delta_x) as c_short;
    (*dest).bottom = ((*source).bottom as c_int + delta_y) as c_short;
    (*dest).right = ((*source).right as c_int + delta_x) as c_short;
    dest
}

// ===================== USE_FADE =====================

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
    let mut curr_row: word = 0;
    let mut curr_row_mask: word = 1;
    while curr_row < 0x10 {
        if which_rows & (curr_row_mask as c_int) != 0 {
            memset(faded.add((curr_row as usize) << 4) as *mut c_void, 0, core::mem::size_of::<[rgb_type; 0x10]>());
            set_pal_arr((curr_row as c_int) << 4, 0x10, core::ptr::null());
        }
        curr_row += 1;
        curr_row_mask <<= 1;
    }
    palette_buffer
}

// seg009:1B64 pal_restore_free_fadein
#[no_mangle]
pub unsafe extern "C" fn pal_restore_free_fadein(palette_buffer: *mut palette_fade_type) {
    set_pal_256(core::ptr::addr_of_mut!((*palette_buffer).original_pal) as *mut rgb_type);
    free(palette_buffer as *mut c_void);
    method_1_blit_rect(onscreen_surface_, offscreen_surface, core::ptr::addr_of!(screen_rect), core::ptr::addr_of!(screen_rect), 0);
}

// seg009:1B88 fade_in_frame
#[no_mangle]
pub unsafe extern "C" fn fade_in_frame(palette_buffer: *mut palette_fade_type) -> c_int {
    start_timer(timerids_timer_1 as c_int, (*palette_buffer).wait_time as c_int);

    (*palette_buffer).fade_pos = (*palette_buffer).fade_pos.wrapping_sub(1);
    let mut start: word = 0;
    let mut current_row_mask: word = 1;
    while start < 0x100 {
        if (*palette_buffer).which_rows & current_row_mask != 0 {
            let original_pal_ptr = (core::ptr::addr_of!((*palette_buffer).original_pal) as *const rgb_type).add(start as usize);
            let faded_pal_ptr = (core::ptr::addr_of_mut!((*palette_buffer).faded_pal) as *mut rgb_type).add(start as usize);
            let mut column: word = 0;
            while column < 0x10 {
                if (*original_pal_ptr.add(column as usize)).r as c_int > (*palette_buffer).fade_pos as c_int {
                    (*faded_pal_ptr.add(column as usize)).r = (*faded_pal_ptr.add(column as usize)).r.wrapping_add(1);
                }
                if (*original_pal_ptr.add(column as usize)).g as c_int > (*palette_buffer).fade_pos as c_int {
                    (*faded_pal_ptr.add(column as usize)).g = (*faded_pal_ptr.add(column as usize)).g.wrapping_add(1);
                }
                if (*original_pal_ptr.add(column as usize)).b as c_int > (*palette_buffer).fade_pos as c_int {
                    (*faded_pal_ptr.add(column as usize)).b = (*faded_pal_ptr.add(column as usize)).b.wrapping_add(1);
                }
                column += 1;
            }
        }
        start = start.wrapping_add(0x10);
        current_row_mask <<= 1;
    }
    let mut start: word = 0;
    let mut current_row_mask: word = 1;
    while start < 0x100 {
        if (*palette_buffer).which_rows & current_row_mask != 0 {
            set_pal_arr(start as c_int, 0x10, (core::ptr::addr_of!((*palette_buffer).faded_pal) as *const rgb_type).add(start as usize));
        }
        start = start.wrapping_add(0x10);
        current_row_mask <<= 1;
    }

    let h = (*offscreen_surface).h;
    if SDL_LockSurface(onscreen_surface_) != 0 {
        sdlperror(cs!("fade_in_frame: SDL_LockSurface"));
        quit(1);
    }
    if SDL_LockSurface(offscreen_surface) != 0 {
        sdlperror(cs!("fade_in_frame: SDL_LockSurface"));
        quit(1);
    }
    let on_stride = (*onscreen_surface_).pitch;
    let off_stride = (*offscreen_surface).pitch;
    let fade_pos = (*palette_buffer).fade_pos as c_int;
    for y in 0..h {
        let mut on_pixel_ptr = ((*onscreen_surface_).pixels as *mut byte).offset((on_stride * y) as isize);
        let mut off_pixel_ptr = ((*offscreen_surface).pixels as *mut byte).offset((off_stride * y) as isize);
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
    SDL_UnlockSurface(onscreen_surface_);
    SDL_UnlockSurface(offscreen_surface);

    do_simple_wait(1); // can interrupt fading of cutscene
    ((*palette_buffer).fade_pos == 0) as c_int
}

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

// seg009:1DF7 fade_out_frame
#[no_mangle]
pub unsafe extern "C" fn fade_out_frame(palette_buffer: *mut palette_fade_type) -> c_int {
    let mut finished_fading: word = 1;
    (*palette_buffer).fade_pos = (*palette_buffer).fade_pos.wrapping_add(1);
    start_timer(timerids_timer_1 as c_int, (*palette_buffer).wait_time as c_int);
    let mut start: word = 0;
    let mut current_row_mask: word = 1;
    while start < 0x100 {
        if (*palette_buffer).which_rows & current_row_mask != 0 {
            let faded_pal_ptr = (core::ptr::addr_of_mut!((*palette_buffer).faded_pal) as *mut rgb_type).add(start as usize);
            let mut column: word = 0;
            while column < 0x10 {
                let curr = faded_pal_ptr.add(column as usize);
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
                column += 1;
            }
        }
        start = start.wrapping_add(0x10);
        current_row_mask <<= 1;
    }
    let mut start: word = 0;
    let mut current_row_mask: word = 1;
    while start < 0x100 {
        if (*palette_buffer).which_rows & current_row_mask != 0 {
            set_pal_arr(start as c_int, 0x10, (core::ptr::addr_of!((*palette_buffer).faded_pal) as *const rgb_type).add(start as usize));
        }
        start = start.wrapping_add(0x10);
        current_row_mask <<= 1;
    }

    let h = (*offscreen_surface).h;
    if SDL_LockSurface(onscreen_surface_) != 0 {
        sdlperror(cs!("fade_out_frame: SDL_LockSurface"));
        quit(1);
    }
    if SDL_LockSurface(offscreen_surface) != 0 {
        sdlperror(cs!("fade_out_frame: SDL_LockSurface"));
        quit(1);
    }
    let on_stride = (*onscreen_surface_).pitch;
    let off_stride = (*offscreen_surface).pitch;
    let fade_pos = (*palette_buffer).fade_pos as c_int;
    for y in 0..h {
        let mut on_pixel_ptr = ((*onscreen_surface_).pixels as *mut byte).offset((on_stride * y) as isize);
        let mut off_pixel_ptr = ((*offscreen_surface).pixels as *mut byte).offset((off_stride * y) as isize);
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
    SDL_UnlockSurface(onscreen_surface_);
    SDL_UnlockSurface(offscreen_surface);

    do_simple_wait(timerids_timer_1 as c_int); // can interrupt fading of cutscene
    finished_fading as c_int
}

// seg009:1F28 read_palette_256
#[no_mangle]
pub unsafe extern "C" fn read_palette_256(target: *mut rgb_type) {
    for i in 0..256usize {
        (*target.add(i)).r = palette[i].r;
        (*target.add(i)).g = palette[i].g;
        (*target.add(i)).b = palette[i].b;
    }
}

// seg009:1F5E set_pal_256
#[no_mangle]
pub unsafe extern "C" fn set_pal_256(source: *mut rgb_type) {
    for i in 0..256usize {
        palette[i].r = (*source.add(i)).r;
        palette[i].g = (*source.add(i)).g;
        palette[i].b = (*source.add(i)).b;
    }
}

// seg009 set_chtab_palette
#[no_mangle]
pub unsafe extern "C" fn set_chtab_palette(chtab: *mut chtab_type, mut colors: *mut byte, n_colors: c_int) {
    if !chtab.is_null() {
        let scolors = malloc(n_colors as usize * core::mem::size_of::<SDL_Color>()) as *mut SDL_Color;
        for i in 0..n_colors {
            (*scolors.offset(i as isize)).r = ((*colors as c_int) << 2) as u8;
            colors = colors.add(1);
            (*scolors.offset(i as isize)).g = ((*colors as c_int) << 2) as u8;
            colors = colors.add(1);
            (*scolors.offset(i as isize)).b = ((*colors as c_int) << 2) as u8;
            colors = colors.add(1);
            (*scolors.offset(i as isize)).a = SDL_ALPHA_OPAQUE;
        }
        // Color 0 of the palette data is not used; replaced by the background color.
        (*scolors.offset(0)).r = 0;
        (*scolors.offset(0)).g = 0;
        (*scolors.offset(0)).b = 0;
        (*scolors.offset(0)).a = SDL_ALPHA_TRANSPARENT;

        let images = core::ptr::addr_of!((*chtab).images) as *const *mut image_type;
        for i in 0..(*chtab).n_images as c_int {
            let current_image = *images.offset(i as isize);
            if !current_image.is_null() {
                let mut n_colors_to_be_set = n_colors;
                let current_palette = (*(*current_image).format).palette;
                if !current_palette.is_null() {
                    if (*current_palette).ncolors < n_colors_to_be_set {
                        n_colors_to_be_set = (*current_palette).ncolors;
                    }
                    if SDL_SetPaletteColors(current_palette, scolors, 0, n_colors_to_be_set) != 0 {
                        sdlperror(cs!("set_chtab_palette: SDL_SetPaletteColors"));
                        quit(1);
                    }
                }
            }
        }
        free(scolors as *mut c_void);
    }
}

// seg009 has_timer_stopped
#[no_mangle]
pub unsafe extern "C" fn has_timer_stopped(index: c_int) -> c_int {
    if (replaying != 0 && skipping_replay != 0) || is_validate_mode != 0 {
        return 1;
    }
    let mut current_counter = SDL_GetPerformanceCounter();
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
