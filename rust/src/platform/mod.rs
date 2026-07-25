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

pub mod sdl;

use std::os::raw::c_int;

/// An opaque, backend-owned pixel surface. The game manipulates raw indexed-palette
/// pixel buffers directly (`SDL_LockSurface` + pointer writes) throughout seg008.rs's
/// drawing code, so `lock`/`unlock` return a raw byte slice rather than a higher-level
/// pixel API -- this matches current behavior exactly rather than introducing a new
/// abstraction the drawing algorithm would need to be rewritten around.
pub trait Surface {
    fn width(&self) -> c_int;
    fn height(&self) -> c_int;
    fn pitch(&self) -> c_int;
    /// Raw pixel bytes for the surface's native format (8bpp indexed for most game
    /// surfaces), for the duration of `f`. A closure, not a `lock`/`unlock` pair, so the
    /// backend (the `sdl2` crate's `Surface::with_lock_mut` on the SDL side) can enforce
    /// lock/unlock pairing itself rather than the caller having to remember to unlock.
    fn with_pixels_mut<R>(&mut self, f: impl FnOnce(&mut [u8]) -> R) -> R;
    fn set_color_key(&mut self, key: u32) -> Result<(), String>;
    fn set_palette(&mut self, colors: &[(u8, u8, u8)]) -> Result<(), String>;
    fn set_blend_mode(&mut self, blend: bool) -> Result<(), String>;
    fn set_alpha_mod(&mut self, alpha: u8) -> Result<(), String>;
}

pub trait Renderer {
    type Surf: Surface;

    fn create_surface(&mut self, width: c_int, height: c_int) -> Self::Surf;
    /// Loads an image (PNG via SDL2_image today) from an in-memory buffer -- the
    /// `IMG_Load_RW`-over-`SDL_RWFromConstMem` path `load_image` (seg009.rs) uses for
    /// sprite data, and `IMG_Load`'s plain-file-path form for the window icon / lighting
    /// mask.
    fn load_image_from_memory(&mut self, bytes: &[u8]) -> Result<Self::Surf, String>;
    fn load_image_from_file(&mut self, path: &str) -> Result<Self::Surf, String>;
    fn blit(&mut self, src: &Self::Surf, dst: &mut Self::Surf, dst_x: c_int, dst_y: c_int);
    fn fill_rect(&mut self, surf: &mut Self::Surf, x: c_int, y: c_int, w: c_int, h: c_int, color: u32);
    /// Pushes the frame surface to the screen (the present step at the end of each
    /// game-loop tick -- `SDL_UpdateTexture` + `SDL_RenderCopy` + `SDL_RenderPresent`
    /// today).
    fn present(&mut self, frame: &Self::Surf);
    fn set_fullscreen(&mut self, fullscreen: bool) -> Result<(), String>;
    fn show_cursor(&mut self, show: bool);
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
    fn add_one_shot_timer(&mut self, delay_ms: u32, callback: Box<dyn FnOnce() + Send>);
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
