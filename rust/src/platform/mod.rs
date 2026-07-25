//! Platform abstraction traits (Step C of the post-port refactor plan).
//!
//! Every direct SDL2/SDL2_image call in the codebase (89 distinct functions across
//! seg009.rs, seg008.rs, menu.rs, seg000.rs, seg001.rs, seg003.rs, lighting.rs, replay.rs,
//! screenshot.rs, midi.rs, sdl_rw_wrappers.rs -- see the Step C inventory in
//! `docs/plans` / the planning session that produced this module) is meant to end up
//! behind one of the four traits below. `SdlPlatform` (rust/src/platform/sdl.rs) is the
//! only module allowed to reference `SDL_*`/`IMG_*` directly; everywhere else calls a
//! trait method instead.
//!
//! **This is a pure relocation, not a redesign.** Trait method shapes mirror the SDL
//! operations the game actually performs today (surface locking, blitting, palette sets,
//! raw scancode arrays, etc.) rather than a higher-level reinterpretation -- that
//! idiomatic-rewrite pass is explicitly Step D's job, done module-by-module once code is
//! already open for de-globalization. Growing this trait surface as more call sites move
//! behind it (rather than getting every method signature right up front) is expected.
//!
//! **`Renderer` operates on raw `*mut SDL_Surface` pointers, not an opaque owned type.**
//! An earlier draft of this trait modeled surfaces as a backend-owned `Surf` associated
//! type (safe, RAII lock/unlock). That doesn't fit how this codebase actually works:
//! surfaces are `*mut SDL_Surface` threaded through globals, `chtab_type`/`image_type`
//! struct fields, and function return values almost everywhere (`get_image`,
//! `chtab_addrs`, `offscreen_surface`, ...), never locally created-and-owned. Modeling
//! `Renderer` around ownership would mean rewriting that plumbing everywhere just to
//! reach the trait -- a real redesign, not the relocation this step is scoped to. So
//! `Renderer`'s methods take the same raw pointers the game already passes around, and
//! are `unsafe fn` like virtually everything else in this crate. Full ownership safety
//! for surfaces is Step D's job, alongside de-globalization.

pub mod sdl;

use std::os::raw::c_int;

use crate::{SDL_Color, SDL_PixelFormat, SDL_RWops, SDL_Rect, SDL_Surface};

pub trait Renderer {
    /// Full `SDL_CreateRGBSurface` signature (depth + RGBA masks), not just
    /// width/height -- callers need both the 8bpp-indexed surfaces most of the game
    /// uses (`depth: 8, masks: 0,0,0,0`) and true-color ones with explicit channel
    /// masks (`lighting.rs`'s 32bpp overlay).
    unsafe fn create_surface(&mut self, width: c_int, height: c_int, depth: c_int, rmask: u32, gmask: u32, bmask: u32, amask: u32) -> *mut SDL_Surface;
    unsafe fn free_surface(&mut self, surf: *mut SDL_Surface);
    /// Loads an image (PNG via SDL2_image today) from an in-memory buffer -- the
    /// `IMG_Load_RW`-over-`SDL_RWFromConstMem` path `load_image` (seg009.rs) uses for
    /// sprite data -- or a file path (the window icon / lighting mask, both `IMG_Load`
    /// straight from disk today). Null on failure, matching `IMG_Load`/`IMG_Load_RW`.
    unsafe fn load_image_from_memory(&mut self, bytes: &[u8]) -> *mut SDL_Surface;
    unsafe fn load_image_from_file(&mut self, path: &std::ffi::CStr) -> *mut SDL_Surface;
    unsafe fn lock_surface(&mut self, surf: *mut SDL_Surface);
    unsafe fn unlock_surface(&mut self, surf: *mut SDL_Surface);
    unsafe fn set_color_key(&mut self, surf: *mut SDL_Surface, key: u32);
    unsafe fn set_palette(&mut self, surf: *mut SDL_Surface, colors: *const SDL_Color, first_color: c_int, n_colors: c_int);
    /// `mode` is a raw `SDL_BlendMode` value (`SDL_BLENDMODE_NONE`/`_BLEND`/`_ADD`/
    /// `_MOD`, i.e. 0/1/2/4) -- lighting.rs needs `_ADD`/`_MOD` specifically, not just
    /// on/off blending, so this takes the actual mode rather than a bool. Returns the
    /// raw SDL result code (0 success, negative on error) -- several callers log via
    /// `sdlperror` when this is nonzero, so the trait preserves that rather than
    /// swallowing it.
    unsafe fn set_blend_mode(&mut self, surf: *mut SDL_Surface, mode: c_int) -> c_int;
    unsafe fn set_alpha_mod(&mut self, surf: *mut SDL_Surface, alpha: u8);
    unsafe fn map_rgba(&mut self, format: *const SDL_PixelFormat, r: u8, g: u8, b: u8, a: u8) -> u32;
    /// `IMG_SavePNG`. Returns the raw SDL result code, same reasoning as `set_blend_mode`.
    unsafe fn save_png(&mut self, surf: *mut SDL_Surface, path: &std::ffi::CStr) -> c_int;
    /// `SDL_GetError` -- the error string for the most recent failing SDL call on this
    /// thread. Not owned by the caller; SDL invalidates it on the next SDL call.
    unsafe fn get_error(&mut self) -> *const std::os::raw::c_char;
    /// Returns the raw SDL result code, same reasoning as `set_blend_mode`.
    unsafe fn blit(&mut self, src: *mut SDL_Surface, src_rect: *const SDL_Rect, dst: *mut SDL_Surface, dst_rect: *mut SDL_Rect) -> c_int;
    /// Returns the raw SDL result code, same reasoning as `set_blend_mode`.
    unsafe fn fill_rect(&mut self, surf: *mut SDL_Surface, rect: *const SDL_Rect, color: u32) -> c_int;
    /// Pushes a surface to the screen (the present step at the end of each game-loop
    /// tick -- `SDL_UpdateTexture` + `SDL_RenderCopy` + `SDL_RenderPresent` today).
    unsafe fn present(&mut self, frame: *mut SDL_Surface);
    unsafe fn set_fullscreen(&mut self, fullscreen: bool);
    unsafe fn show_cursor(&mut self, show: bool);
    /// Frame pacing (`SDL_Delay`). Not really a "renderer" operation, but every current
    /// caller is inside a render/game loop, and there's no better-fitting trait yet.
    unsafe fn delay(&mut self, ms: u32);
    /// Wraps a memory buffer as an `SDL_RWops` stream (`SDL_RWFromMem`) -- used for
    /// replay-options serialization (replay.rs) via the same `rw_process_fn`/
    /// `section_fn` function-pointer types `sdl_rw_wrappers.rs`/`menu.rs` share, which
    /// still take `*mut SDL_RWops` directly; not a "renderer" concern either, same
    /// reasoning as `delay`.
    unsafe fn rw_from_mem(&mut self, buf: *mut std::os::raw::c_void, size: c_int) -> *mut SDL_RWops;
    unsafe fn rw_tell(&mut self, rw: *mut SDL_RWops) -> i64;
    unsafe fn rw_close(&mut self, rw: *mut SDL_RWops);
    unsafe fn rw_write(&mut self, rw: *mut SDL_RWops, ptr: *const std::os::raw::c_void, size: usize, num: usize) -> usize;
    unsafe fn rw_read(&mut self, rw: *mut SDL_RWops, ptr: *mut std::os::raw::c_void, size: usize, maxnum: usize) -> usize;
    /// `SDL_ShowSimpleMessageBox` -- the modal error dialog shown when a replay file
    /// fails to load outside `--validate` mode.
    unsafe fn show_message_box(&mut self, title: &std::ffi::CStr, message: &std::ffi::CStr);
    /// `SDL_GetVersion` -- the linked (runtime) SDL version, shown in the debug version
    /// readout (Ctrl+V). Not a "renderer" op either; same grab-bag reasoning as `delay`.
    unsafe fn linked_sdl_version(&mut self) -> (u8, u8, u8);
    /// `SDL_GetPerformanceCounter` -- high-resolution frame-timing counter.
    unsafe fn performance_counter(&mut self) -> u64;
}

/// The mixed digi/speaker/MIDI/OGG output sink. `opl3.rs`'s synth math and the mixing
/// logic in seg009.rs's `*_callback` functions stay pure Rust and backend-agnostic --
/// this trait is only where the finished sample stream goes.
pub trait AudioBackend {
    /// Registers the pull callback the backend will invoke on its audio thread to fill
    /// an output buffer of `i16` samples (matches `SDL_AudioSpec.callback` /
    /// `audio_callback` today -- see seg009.rs).
    fn open(&mut self, sample_rate: c_int, channels: u8, fill: Box<dyn FnMut(&mut [i16]) + Send>) -> Result<(), String>;
    fn pause(&mut self, paused: bool);
    /// Guards the shared mixing state the callback and the game thread both touch
    /// (`SDL_LockAudio`/`SDL_UnlockAudio` today).
    fn lock(&mut self);
    fn unlock(&mut self);
}

pub trait InputSource {
    /// Raw scancode-indexed key-down state (`SDL_GetKeyboardState` today).
    fn key_state(&self, scancode: c_int) -> bool;
    fn mouse_state(&self) -> (c_int, c_int, bool, bool);
    fn start_text_input(&mut self, x: c_int, y: c_int, w: c_int, h: c_int);
    fn stop_text_input(&mut self);
    /// One-shot timer (the level-skip shift-key debounce is the only current caller --
    /// `SDL_AddTimer` in seg000.rs). `delay_ms` after this call, `callback` fires once.
    /// Returns whether the timer was created (`SDL_AddTimer` returning a nonzero ID) --
    /// the caller logs via `sdlperror` and exits on failure.
    fn add_one_shot_timer(&mut self, delay_ms: u32, callback: Box<dyn FnOnce() + Send>) -> bool;
    fn rumble(&mut self, strength: f32, duration_ms: u32);
}

/// DAT/asset loading and quicksave/HOF/long-term-save I/O. Already env-var-redirectable
/// per the quicksave unit tests (`SDLPOP_SAVE_PATH`) -- this generalizes that pattern into
/// a real trait so a WASM backend can route the same calls to `localStorage`/`IndexedDB`.
pub trait FileSystem {
    fn read_file(&self, path: &str) -> Result<Vec<u8>, String>;
    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), String>;
    fn file_exists(&self, path: &str) -> bool;
}

/// A backend bundles the four capabilities. `&mut dyn Platform` (or a generic `<P:
/// Platform>`) is what Step D's `&mut State`-taking functions will additionally take,
/// once a module's SDL calls have moved behind this trait and its globals have moved
/// into `State`.
pub trait Platform {
    type Rend: Renderer;
    type Audio: AudioBackend;
    type Input: InputSource;
    type Files: FileSystem;

    fn renderer(&mut self) -> &mut Self::Rend;
    fn audio(&mut self) -> &mut Self::Audio;
    fn input(&mut self) -> &mut Self::Input;
    fn files(&mut self) -> &mut Self::Files;
}
