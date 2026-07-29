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

pub mod backend;
#[cfg(not(target_arch = "wasm32"))]
pub mod sdl;
// Also compiled on native under `cargo test` (not in normal native builds) so the
// pixel-parity test harness (Phase A, `docs/plans/13-platform-architecture-unification.md`)
// can run WasmRenderer's logic directly, no wasm32 target or browser needed -- it's plain
// portable Rust with no wasm-only crate dependencies.
#[cfg(any(target_arch = "wasm32", test))]
pub mod wasm;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod pixel_parity_tests;

// All ~224 real call sites reach the active backend through `crate::platform::sdl::
// shared_renderer()`/`shared_audio()`/`shared_input()` -- a hardcoded module path, not a
// backend-agnostic one (Step C never built that indirection; it only made the *type*
// backend-selected via `platform::backend::Active*`). Rather than touch every call site
// to route through some new backend-agnostic path, keep the `sdl` path itself valid on
// every target: on wasm32, `platform::sdl` becomes this tiny inline module forwarding to
// the real wasm32 implementation, instead of the file-based native module.
#[cfg(target_arch = "wasm32")]
pub mod sdl {
    pub use crate::platform::wasm::{shared_audio, shared_input, shared_renderer};
}

use std::os::raw::c_int;

use crate::{SDL_Color, SDL_PixelFormat, SDL_RWops, SDL_Rect, SDL_Surface};

/// Plain-data copy of the `SDL_PixelFormat` fields game logic actually reads (Phase A of
/// `docs/plans/13-platform-architecture-unification.md`) -- returned by
/// `Renderer::surface_format_info` instead of exposing `.format` for direct dereference.
/// `WasmRenderer` can populate this without needing a real `SDL_Palette`-shaped allocation
/// for anything except the actual indexed-color entries (see `surface_palette`).
#[derive(Clone, Copy, Debug)]
pub struct PixelFormatInfo {
    pub bits_per_pixel: u8,
    pub bytes_per_pixel: u8,
    pub rmask: u32,
    pub gmask: u32,
    pub bmask: u32,
    pub amask: u32,
    /// The raw `SDL_PixelFormatEnum` value (`(*format).format`) -- kept separate from the
    /// mask/depth fields above since it's an opaque enum, not a component derived from
    /// them. Needed for `SDL_ISPIXELFORMAT_INDEXED`-style checks (`seg009.rs`).
    pub format: u32,
}

pub trait Renderer {
    /// Full `SDL_CreateRGBSurface` signature (depth + RGBA masks), not just
    /// width/height -- callers need both the 8bpp-indexed surfaces most of the game
    /// uses (`depth: 8, masks: 0,0,0,0`) and true-color ones with explicit channel
    /// masks (`lighting.rs`'s 32bpp overlay).
    unsafe fn create_surface(&mut self, width: c_int, height: c_int, depth: c_int, rmask: u32, gmask: u32, bmask: u32, amask: u32) -> *mut SDL_Surface;
    unsafe fn free_surface(&mut self, surf: *mut SDL_Surface);
    /// Replaces direct `(*surf).w`/`.h` field reads (Phase A). Pure accessor, no
    /// C-visible side effect either way.
    unsafe fn surface_size(&mut self, surf: *mut SDL_Surface) -> (c_int, c_int);
    /// Replaces direct `(*surf).pitch` field reads.
    unsafe fn surface_pitch(&mut self, surf: *mut SDL_Surface) -> c_int;
    /// Replaces direct `(*surf).pixels` field reads. Raw pointer, same as everything else
    /// in this trait -- callers already do their own per-row pointer arithmetic using
    /// `surface_pitch`, so this stays low-level rather than wrapping a safe slice.
    unsafe fn surface_pixels(&mut self, surf: *mut SDL_Surface) -> *mut std::os::raw::c_void;
    /// Replaces direct `(*(*surf).format).BitsPerPixel`/`.Rmask`/etc. field reads.
    unsafe fn surface_format_info(&mut self, surf: *mut SDL_Surface) -> PixelFormatInfo;
    /// Replaces direct `(*(*surf).format).palette` field reads -- stays a raw handle,
    /// same reasoning as `set_palette_colors` already taking one.
    unsafe fn surface_palette(&mut self, surf: *mut SDL_Surface) -> *mut crate::SDL_Palette;
    /// Replaces direct `(*surf).format` reads -- the specific case Phase A's per-file
    /// migration passes deliberately left alone (`map_rgb`/`map_rgba`/`convert_surface`
    /// all take a real `*const SDL_PixelFormat` argument, and there was no accessor for
    /// "the format pointer itself," only its sub-fields via `surface_format_info`). Added
    /// once that gap actually mattered: `WasmRenderer`'s surfaces are opaque handles, not
    /// real memory, so `(*surf).format` is a wild-pointer dereference there, not just an
    /// encapsulation nicety like the other accessors.
    unsafe fn surface_format_ptr(&mut self, surf: *mut SDL_Surface) -> *mut SDL_PixelFormat;
    /// Loads an image (PNG via SDL2_image today) from an in-memory buffer -- the
    /// `IMG_Load_RW`-over-`SDL_RWFromConstMem` path `load_image` (seg009.rs) uses for
    /// sprite data -- or a file path (the window icon / lighting mask, both `IMG_Load`
    /// straight from disk today). Null on failure, matching `IMG_Load`/`IMG_Load_RW`.
    unsafe fn load_image_from_memory(&mut self, bytes: &[u8]) -> *mut SDL_Surface;
    unsafe fn load_image_from_file(&mut self, path: &std::ffi::CStr) -> *mut SDL_Surface;
    /// Raw `IMG_Load_RW` -- `load_image_from_memory` above bundles
    /// `SDL_RWFromConstMem`+`IMG_Load_RW`+`SDL_RWclose` into one call, which doesn't fit
    /// callers (seg009.rs's own `load_image`) that need distinct error handling/logging
    /// around each of the three steps.
    unsafe fn img_load_rw(&mut self, rw: *mut SDL_RWops, freesrc: c_int) -> *mut SDL_Surface;
    /// Returns the raw SDL result code (0 success) -- seg009.rs checks it, unlike the
    /// earlier callers that motivated the original void signature.
    unsafe fn lock_surface(&mut self, surf: *mut SDL_Surface) -> c_int;
    unsafe fn unlock_surface(&mut self, surf: *mut SDL_Surface);
    /// `enable` is the raw `SDL_SetColorKey` flag arg -- `hflip` (seg008.rs) both
    /// enables a color key (`true`, matching every earlier caller) and disables one
    /// (`false`), so this can't hardcode "always enable" like the original draft did.
    /// Returns the raw SDL result code, same reasoning as `lock_surface`.
    unsafe fn set_color_key(&mut self, surf: *mut SDL_Surface, enable: bool, key: u32) -> c_int;
    unsafe fn set_palette(&mut self, surf: *mut SDL_Surface, colors: *const SDL_Color, first_color: c_int, n_colors: c_int);
    /// Same underlying `SDL_SetPaletteColors` call as `set_palette`, but operating on
    /// an already-extracted `*mut SDL_Palette` directly -- some callers (seg009.rs)
    /// already have the palette pointer in hand rather than a surface to derive it
    /// from.
    unsafe fn set_palette_colors(&mut self, palette: *mut crate::SDL_Palette, colors: *const SDL_Color, first_color: c_int, n_colors: c_int) -> c_int;
    /// `SDL_SetSurfacePalette` -- installs an existing `SDL_Palette` object wholesale
    /// (`hflip`'s output surface reuses its source's palette), a different operation
    /// from `set_palette` above (which edits specific color entries in place).
    unsafe fn set_surface_palette(&mut self, surf: *mut SDL_Surface, palette: *mut crate::SDL_Palette) -> c_int;
    /// `SDL_ConvertSurface` -- clones a surface into a (possibly different) pixel
    /// format.
    unsafe fn convert_surface(&mut self, src: *mut SDL_Surface, fmt: *const SDL_PixelFormat, flags: u32) -> *mut SDL_Surface;
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
    unsafe fn rw_close(&mut self, rw: *mut SDL_RWops) -> c_int;
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
    unsafe fn performance_frequency(&mut self) -> u64;
    unsafe fn rw_from_file(&mut self, path: &std::ffi::CStr, mode: &std::ffi::CStr) -> *mut SDL_RWops;
    /// `SDL_GetScancodeName` -- human-readable key-bind label (menu.rs's controls page).
    unsafe fn get_scancode_name(&mut self, scancode: u32) -> *const std::os::raw::c_char;
    unsafe fn get_window_flags(&mut self, window: *mut crate::SDL_Window) -> u32;
    // The four below operate on the raw `*mut SDL_Renderer`/`*mut SDL_Window` the game
    // already holds in the `renderer_`/`window_` globals, not on `self`'s own
    // canvas/window -- same reasoning as `AudioBackend::lock`/`unlock`/`pause` reading
    // the raw `sdl_haptic`/etc. globals: seg009.rs's own window/renderer creation
    // hasn't migrated to this trait yet, so `self`'s canvas is unset for any instance
    // that exists today, but the real renderer_/window_ pointers are already live.
    unsafe fn render_get_scale(&mut self, renderer: *mut crate::SDL_Renderer) -> (f32, f32);
    unsafe fn render_get_logical_size(&mut self, renderer: *mut crate::SDL_Renderer) -> (c_int, c_int);
    unsafe fn render_get_viewport(&mut self, renderer: *mut crate::SDL_Renderer) -> SDL_Rect;
    unsafe fn render_set_integer_scale(&mut self, renderer: *mut crate::SDL_Renderer, enable: bool) -> c_int;

    // ------------------------------------------------------------------------------
    // seg009.rs's platform-init/lifecycle surface: SDL_Init/CreateWindow/CreateRenderer/
    // OpenAudio/controller+haptic setup/the event loop. Same relocation-not-redesign
    // rule as everywhere else in this trait -- these are still the exact SDL calls
    // seg009.rs made before, just behind the trait now. Actually restructuring game
    // startup to build and own a real `SdlPlatform` end to end (rather than populating
    // the same `window_`/`renderer_`/etc. globals these calls already write into) is
    // out of scope for Step C; see the plan's Step D notes on de-globalization.
    // ------------------------------------------------------------------------------
    unsafe fn map_rgb(&mut self, format: *const SDL_PixelFormat, r: u8, g: u8, b: u8) -> u32;
    unsafe fn set_clip_rect(&mut self, surf: *mut SDL_Surface, rect: *const SDL_Rect) -> c_int;
    unsafe fn convert_surface_format(&mut self, src: *mut SDL_Surface, pixel_format: u32, flags: u32) -> *mut SDL_Surface;
    unsafe fn blit_scaled(&mut self, src: *mut SDL_Surface, src_rect: *const SDL_Rect, dst: *mut SDL_Surface, dst_rect: *mut SDL_Rect) -> c_int;
    unsafe fn set_window_icon(&mut self, window: *mut crate::SDL_Window, icon: *mut SDL_Surface);
    unsafe fn rw_from_const_mem(&mut self, mem: *const std::os::raw::c_void, size: c_int) -> *mut SDL_RWops;
    unsafe fn create_texture(&mut self, renderer: *mut crate::SDL_Renderer, format: u32, access: c_int, w: c_int, h: c_int) -> *mut crate::SDL_Texture;
    unsafe fn update_texture(&mut self, texture: *mut crate::SDL_Texture, rect: *const SDL_Rect, pixels: *const std::os::raw::c_void, pitch: c_int) -> c_int;
    unsafe fn set_render_target(&mut self, renderer: *mut crate::SDL_Renderer, texture: *mut crate::SDL_Texture) -> c_int;
    unsafe fn render_clear(&mut self, renderer: *mut crate::SDL_Renderer) -> c_int;
    unsafe fn render_copy(&mut self, renderer: *mut crate::SDL_Renderer, texture: *mut crate::SDL_Texture, src_rect: *const SDL_Rect, dst_rect: *const SDL_Rect) -> c_int;
    unsafe fn render_present(&mut self, renderer: *mut crate::SDL_Renderer);
    unsafe fn render_set_logical_size(&mut self, renderer: *mut crate::SDL_Renderer, w: c_int, h: c_int) -> c_int;
    unsafe fn get_renderer_output_size(&mut self, renderer: *mut crate::SDL_Renderer) -> (c_int, c_int);
    /// Only `SDL_RendererInfo.flags` is ever read (checking `SDL_RENDERER_TARGETTEXTURE`
    /// support), so this returns just that instead of the full struct -- `SDL_RendererInfo`
    /// isn't otherwise shared across modules.
    unsafe fn get_renderer_info_flags(&mut self, renderer: *mut crate::SDL_Renderer) -> u32;
    unsafe fn set_hint(&mut self, name: &std::ffi::CStr, value: &std::ffi::CStr) -> c_int;
    unsafe fn sdl_init(&mut self, flags: u32) -> c_int;
    unsafe fn sdl_init_subsystem(&mut self, flags: u32) -> c_int;
    unsafe fn sdl_quit(&mut self);
    unsafe fn create_window(&mut self, title: &std::ffi::CStr, x: c_int, y: c_int, w: c_int, h: c_int, flags: u32) -> *mut crate::SDL_Window;
    unsafe fn create_renderer(&mut self, window: *mut crate::SDL_Window, index: c_int, flags: u32) -> *mut crate::SDL_Renderer;
    /// `desired`/`obtained` are `*mut SDL_AudioSpec` -- typed as raw `c_void` pointers
    /// because seg009.rs defines its own local (ABI-matching) `SDL_AudioSpec` struct
    /// rather than using a shared type, so the trait can't name it without creating a
    /// circular dependency back into seg009.rs.
    unsafe fn open_audio_raw(&mut self, desired: *mut std::os::raw::c_void, obtained: *mut std::os::raw::c_void) -> c_int;
    unsafe fn num_joysticks(&mut self) -> c_int;
    unsafe fn is_game_controller(&mut self, joystick_index: c_int) -> bool;
    unsafe fn game_controller_open(&mut self, joystick_index: c_int) -> *mut crate::SDL_GameController;
    unsafe fn game_controller_close(&mut self, controller: *mut crate::SDL_GameController);
    unsafe fn game_controller_from_instance_id(&mut self, joyid: i32) -> *mut crate::SDL_GameController;
    unsafe fn game_controller_add_mappings_from_file(&mut self, path: &std::ffi::CStr) -> c_int;
    unsafe fn joystick_open(&mut self, device_index: c_int) -> *mut crate::SDL_Joystick;
    unsafe fn haptic_open(&mut self, device_index: c_int) -> *mut crate::SDL_Haptic;
    unsafe fn haptic_rumble_init(&mut self, haptic: *mut crate::SDL_Haptic) -> c_int;
    /// `event` is `*mut SDL_Event` -- same reasoning as `open_audio_raw` for the
    /// `c_void` typing (seg009.rs's own hand-rolled, ABI-matching `SDL_Event` union).
    unsafe fn push_event(&mut self, event: *mut std::os::raw::c_void) -> c_int;
    unsafe fn poll_event(&mut self, event: *mut std::os::raw::c_void) -> c_int;
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
