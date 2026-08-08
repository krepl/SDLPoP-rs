//! WASM platform backend.
//!
//! **Surface/pixel manipulation is real** (Phase A of
//! `docs/plans/13-platform-architecture-unification.md`): `create_surface`, the five
//! surface accessor methods, `lock`/`unlock_surface`, `map_rgb`/`map_rgba`, `fill_rect`,
//! `blit`/`blit_scaled`, `set_color_key`/`set_blend_mode`/`set_alpha_mod`,
//! `set_palette`/`set_palette_colors`/`set_surface_palette`, and
//! `convert_surface`/`convert_surface_format` are backed by [`WasmSurface`], a plain
//! `Vec<u8>`-owning struct with no relationship to real SDL memory layout. This is what
//! Phase A's encapsulation of `SDL_Surface`/`SDL_PixelFormat` field access behind the
//! `Renderer` trait buys us: nothing here needs to fake SDL's actual struct layout.
//!
//! **Everything else is still `unimplemented!()`** -- window/init lifecycle, real audio
//! output, controller/joystick/haptic, and the event loop. Those are Phase B/C's job (or
//! later `WasmAudio`/`WasmInput` work), not part of surface encapsulation.
//!
//! **One real exception to "opaque, no SDL layout":** `surface_palette` must return a
//! genuinely dereferenceable `*mut SDL_Palette` -- `seg009.rs` reads `.ncolors` off it
//! directly (`(*current_palette).ncolors`), a pre-Phase-A direct field access that Phase A's
//! per-file passes haven't reached yet. `SDL_Palette` is a plain `#[repr(C)]` struct with no
//! bindgen-inserted padding, so a real, heap-allocated instance works correctly here the same
//! way `SDL_Surface`/`SDL_PixelFormat` do (see the wasm-milestone-1 memory notes on why this
//! is safe: rustc lays these out correctly for whatever target actually compiles them).

use std::collections::HashMap;
use std::os::raw::c_int;

use crate::{SDL_Color, SDL_PixelFormat, SDL_RWops, SDL_Rect, SDL_Surface};

use super::{AudioBackend, FileSystem, InputSource, PixelFormatInfo, Renderer};

// ============================================================================
// Surface store. Every `*mut SDL_Surface` handed to game code is really just an opaque
// `id as *mut SDL_Surface`, looked up in this table -- never a real SDL_Surface pointer.
// ============================================================================

struct WasmSurface {
    w: c_int,
    h: c_int,
    pitch: c_int,
    pixels: Vec<u8>,
    bits_per_pixel: u8,
    bytes_per_pixel: u8,
    rmask: u32,
    gmask: u32,
    bmask: u32,
    amask: u32,
    rshift: u32,
    gshift: u32,
    bshift: u32,
    ashift: u32,
    /// Real, heap-allocated `SDL_Palette` for 8bpp-indexed surfaces (`None` otherwise,
    /// matching real SDL's `format->palette == NULL` for non-indexed surfaces). Boxed so
    /// the returned pointer stays stable even if this `WasmSurface` moves within the
    /// `HashMap` (moving a `Box` moves only the pointer, not its heap contents).
    palette: Option<WasmPalette>,
    color_key: Option<u32>,
    blend_mode: c_int,
    alpha_mod: u8,
    clip_rect: Option<SDL_Rect>,
    /// Real, heap-allocated `SDL_PixelFormat` mirroring this surface's own mask/depth
    /// fields (and `palette`'s pointer, if any) -- exists solely so `surface_format_ptr`
    /// can hand back something genuinely dereferenceable for the handful of call sites
    /// that pass `(*surf).format` directly into `map_rgb`/`map_rgba`/`convert_surface`
    /// (no accessor replaces those calls themselves, only the field read feeding them).
    format_ptr: Box<SDL_PixelFormat>,
}

struct WasmPalette {
    // Kept alive by `header.colors` pointing into this box; never read directly except to
    // keep the allocation alive and for `set_palette`/`set_palette_colors` writes.
    colors: Box<[SDL_Color; 256]>,
    header: Box<crate::SDL_Palette>,
}

impl WasmPalette {
    fn new() -> Self {
        let mut colors = Box::new([SDL_Color { r: 0, g: 0, b: 0, a: 255 }; 256]);
        let header = Box::new(crate::SDL_Palette {
            ncolors: 256,
            colors: colors.as_mut_ptr(),
            version: 1,
            refcount: 1,
        });
        WasmPalette { colors, header }
    }

    fn as_ptr(&mut self) -> *mut crate::SDL_Palette {
        self.header.as_mut() as *mut crate::SDL_Palette
    }

    fn colors_mut(&mut self) -> &mut [SDL_Color; 256] {
        &mut self.colors
    }
}

const SDL_BLENDMODE_NONE: c_int = 0;
const SDL_BLENDMODE_BLEND: c_int = 1;

fn surfaces() -> &'static mut HashMap<usize, WasmSurface> {
    static mut SURFACES: Option<HashMap<usize, WasmSurface>> = None;
    unsafe {
        #[allow(static_mut_refs)]
        SURFACES.get_or_insert_with(HashMap::new)
    }
}

fn next_surface_id() -> usize {
    static mut NEXT_ID: usize = 1;
    unsafe {
        let id = NEXT_ID;
        NEXT_ID += 1;
        id
    }
}

unsafe fn surf_mut<'a>(surf: *mut SDL_Surface) -> &'a mut WasmSurface {
    surfaces()
        .get_mut(&(surf as usize))
        .expect("WasmRenderer: unknown surface handle")
}

// ============================================================================
// SDL_RWops store. Same opaque-handle pattern as surfaces (nothing dereferences
// SDL_RWops fields directly anywhere in the crate, confirmed by grep). Backed by the same
// virtual filesystem as `wasm_libc.rs`'s `fopen`-family functions (`rw_from_file` is the
// only real file-backed caller, `SDLPoP.cfg` in menu.rs) -- one filesystem, not two.
// ============================================================================

struct WasmRw {
    data: Vec<u8>,
    pos: usize,
    write_back_path: Option<String>,
}

fn rw_handles() -> &'static mut HashMap<usize, WasmRw> {
    static mut RW_HANDLES: Option<HashMap<usize, WasmRw>> = None;
    unsafe {
        #[allow(static_mut_refs)]
        RW_HANDLES.get_or_insert_with(HashMap::new)
    }
}

fn next_rw_id() -> usize {
    static mut NEXT_ID: usize = 1;
    unsafe {
        let id = NEXT_ID;
        NEXT_ID += 1;
        id
    }
}

fn shift_for(mask: u32) -> u32 {
    if mask == 0 { 0 } else { mask.trailing_zeros() }
}

/// `globalThis.performance.now()`, read reflectively rather than via `web_sys::window()`
/// specifically -- the eventual real target is a Worker (`self`, no `window` global at
/// all), and `Window`/`WorkerGlobalScope` both expose the same `Performance` interface
/// under the same `performance` property name, so reading it off `globalThis` works
/// identically in a Window, a Worker, or (handy for testing without a browser) Node, which
/// has provided a global `performance` object since Node 16.
///
/// `js_sys` is a wasm32-only dependency (`Cargo.toml`), but this module is also compiled
/// on native under `cargo test` (Phase A) -- so the real implementation is wasm32-only,
/// with a `std::time::Instant`-backed equivalent for native test builds (this function's
/// contract, milliseconds since some fixed start point, is identical either way; nothing
/// depends on it lining up with a real wall-clock epoch).
#[cfg(target_arch = "wasm32")]
fn performance_now_ms() -> f64 {
    use wasm_bindgen::JsCast;
    (|| {
        let global = js_sys::global();
        let perf = js_sys::Reflect::get(&global, &"performance".into()).ok()?;
        let now: js_sys::Function = js_sys::Reflect::get(&perf, &"now".into()).ok()?.dyn_into().ok()?;
        now.call0(&perf).ok()?.as_f64()
    })()
    .unwrap_or(0.0)
}

#[cfg(not(target_arch = "wasm32"))]
fn performance_now_ms() -> f64 {
    static mut START: Option<std::time::Instant> = None;
    unsafe {
        #[allow(static_mut_refs)]
        let start = START.get_or_insert_with(std::time::Instant::now);
        start.elapsed().as_secs_f64() * 1000.0
    }
}

/// Posts a presented frame out via `postMessage`, read reflectively off `globalThis` for the
/// same reason `performance_now_ms` does -- this runs inside a dedicated Worker in the real
/// harness (no `window`), but reflective access also works unmodified from a plain Node probe
/// (where `postMessage` doesn't exist at all: the lookup returns `None` and this is a no-op,
/// which is exactly what the headless `run_game()` probe needs -- it has no JS side listening
/// for frames and shouldn't need one).
///
/// This is frames-out only (Phase B item 2/5's first pass, "frames first, then input" per
/// explicit user decision -- see the plan doc). It deliberately does NOT attempt to solve
/// input delivery: a Worker's `onmessage` handler cannot run while `pop_main()`'s blocking
/// loop still occupies the call stack, so getting input *into* a running game needs
/// `SharedArrayBuffer`/`Atomics` (a separate, not-yet-started piece -- see the plan's Phase B
/// section), not another `postMessage`. Sending, unlike receiving, never needs the far end's
/// event loop to be idle, so this half works today without that.
#[cfg(target_arch = "wasm32")]
fn post_frame_to_js(w: c_int, h: c_int, bpp: usize, pixels: &[u8]) {
    use wasm_bindgen::JsCast;
    (|| -> Option<()> {
        let global = js_sys::global();
        let post_message: js_sys::Function =
            js_sys::Reflect::get(&global, &"postMessage".into()).ok()?.dyn_into().ok()?;
        let msg = js_sys::Object::new();
        js_sys::Reflect::set(&msg, &"type".into(), &"frame".into()).ok()?;
        js_sys::Reflect::set(&msg, &"w".into(), &(w as u32).into()).ok()?;
        js_sys::Reflect::set(&msg, &"h".into(), &(h as u32).into()).ok()?;
        js_sys::Reflect::set(&msg, &"bpp".into(), &(bpp as u32).into()).ok()?;
        let arr = js_sys::Uint8Array::new_with_length(pixels.len() as u32);
        arr.copy_from(pixels);
        js_sys::Reflect::set(&msg, &"pixels".into(), &arr).ok()?;
        post_message.call1(&global, &msg).ok()?;
        Some(())
    })();
}

#[cfg(not(target_arch = "wasm32"))]
fn post_frame_to_js(_w: c_int, _h: c_int, _bpp: usize, _pixels: &[u8]) {}

// ============================================================================
// Audio. Real SDL pulls `SDL_AudioSpec.callback` from a dedicated realtime audio thread,
// completely decoupled from the game's own timing -- there's no such thread here (wasm32 is
// single-threaded in this build), so the callback is instead pulled synchronously from every
// spin of `Renderer::delay`'s busy-wait (see there) and once per `render_present`, the two
// points the blocking game loop actually yields CPU time somewhat regularly. Each pulled PCM
// chunk is posted out to JS the same way a frame is (`post_frame_to_js`'s reflective-
// `postMessage` pattern), for the main thread to actually play via the Web Audio API.
// ============================================================================

/// Mirrors `seg009.rs`'s private `SDL_AudioSpec` layout exactly (same duplication pattern as
/// `stat_t`, redeclared per file rather than shared) -- `open_audio_raw` receives this same
/// struct through an untyped `*mut c_void`, built by `init_digi` (`seg009.rs`).
#[repr(C)]
struct WasmSdlAudioSpec {
    freq: c_int,
    format: u16,
    channels: u8,
    silence: u8,
    samples: u16,
    padding: u16,
    size: u32,
    callback: Option<unsafe extern "C" fn(*mut std::os::raw::c_void, *mut u8, c_int)>,
    userdata: *mut std::os::raw::c_void,
}

struct AudioSpecInfo {
    freq: c_int,
    format: u16,
    channels: u8,
    samples: u16,
    callback: unsafe extern "C" fn(*mut std::os::raw::c_void, *mut u8, c_int),
    userdata: *mut std::os::raw::c_void,
}

static mut AUDIO_SPEC: Option<AudioSpecInfo> = None;
static mut AUDIO_PAUSED: bool = true;
static mut NEXT_AUDIO_PUMP_MS: f64 = 0.0;

/// Pulls one chunk of PCM from the game's registered audio callback, if a full chunk's worth
/// of playback time has elapsed since the last pull, and posts it to JS. Cheap to call more
/// often than that -- the time check makes extra calls a no-op, so every `delay` spin
/// iteration and every `render_present` can call this unconditionally.
#[allow(static_mut_refs)]
unsafe fn pump_audio() {
    let Some(spec) = AUDIO_SPEC.as_ref() else { return };
    if AUDIO_PAUSED {
        return;
    }
    let now = performance_now_ms();

    // Keep at least this much audio queued up ahead of "now" in the JS-side scheduler
    // (index.html's nextPlayTime). Pumping exactly one chunk "just in time," with zero
    // margin (an earlier version of this function), meant any small timing jitter in
    // *when* this gets called -- busy-spin granularity is not sub-millisecond-precise, and
    // browsers throttle background work in various ways -- let real playback time catch up
    // to or pass the next scheduled chunk's start before this function got around to posting
    // it, producing an audible gap/click at that chunk boundary. Verified fixed: pulling a
    // real-time-paced trace of pump_audio's own counters over a clean 34s browser session
    // showed exactly the expected ~43 chunks/sec (1024 samples / 44100 Hz), not the ~5x
    // over-rate an earlier (contaminated by a stale background Worker from repeated test
    // navigations, not a real bug) measurement had suggested.
    const LOOKAHEAD_MS: f64 = 100.0;

    // If we've fallen way behind (audio was paused, or this is the first real pump long
    // after open_audio_raw's init timestamp), resume from "now" instead of synthesizing a
    // potentially huge backlog of missed audio all at once in a single burst.
    if NEXT_AUDIO_PUMP_MS < now - LOOKAHEAD_MS {
        NEXT_AUDIO_PUMP_MS = now;
    }

    let bytes_per_sample = ((spec.format & 0xFF) / 8).max(1) as usize;
    let chunk_duration_ms = (spec.samples as f64 / spec.freq as f64) * 1000.0;
    while NEXT_AUDIO_PUMP_MS < now + LOOKAHEAD_MS {
        // The low byte of an `SDL_AudioFormat` is its sample bit size (8 for `AUDIO_U8`, 16
        // for `AUDIO_S16*`) -- this codebase only ever requests one of those two (see
        // `init_digi`).
        let buf_len = spec.samples as usize * spec.channels as usize * bytes_per_sample;
        let mut buf = vec![0u8; buf_len];
        (spec.callback)(spec.userdata, buf.as_mut_ptr(), buf_len as c_int);
        post_audio_to_js(spec.freq, spec.channels, bytes_per_sample as u32, &buf);
        NEXT_AUDIO_PUMP_MS += chunk_duration_ms;
    }
}

#[cfg(target_arch = "wasm32")]
fn post_audio_to_js(freq: c_int, channels: u8, bytes_per_sample: u32, pcm: &[u8]) {
    use wasm_bindgen::JsCast;
    (|| -> Option<()> {
        let global = js_sys::global();
        let post_message: js_sys::Function =
            js_sys::Reflect::get(&global, &"postMessage".into()).ok()?.dyn_into().ok()?;
        let msg = js_sys::Object::new();
        js_sys::Reflect::set(&msg, &"type".into(), &"audio".into()).ok()?;
        js_sys::Reflect::set(&msg, &"freq".into(), &(freq as u32).into()).ok()?;
        js_sys::Reflect::set(&msg, &"channels".into(), &(channels as u32).into()).ok()?;
        js_sys::Reflect::set(&msg, &"bytesPerSample".into(), &bytes_per_sample.into()).ok()?;
        let arr = js_sys::Uint8Array::new_with_length(pcm.len() as u32);
        arr.copy_from(pcm);
        js_sys::Reflect::set(&msg, &"pcm".into(), &arr).ok()?;
        post_message.call1(&global, &msg).ok()?;
        Some(())
    })();
}

#[cfg(not(target_arch = "wasm32"))]
fn post_audio_to_js(_freq: c_int, _channels: u8, _bytes_per_sample: u32, _pcm: &[u8]) {}

// ============================================================================
// Texture / render-target store -- the software equivalent of SDL's GPU-backed textures.
// `seg009.rs`'s actual rendering pipeline (texture_sharp/blurry/fuzzy, all
// SDL_PIXELFORMAT_RGB24) goes through create_texture/update_texture/render_clear/
// render_copy/render_present against the renderer_ global directly, not through
// `Renderer::present` (native's own `present()` panics if reached -- seg009.rs's window/
// renderer creation was never migrated to build a real owned SdlPlatform, per Step C's
// notes -- so this family, not `present`, is the real pipeline to implement here too).
// ============================================================================

struct WasmTexture {
    w: c_int,
    h: c_int,
    bytes_per_pixel: usize,
    pixels: Vec<u8>,
}

fn textures() -> &'static mut HashMap<usize, WasmTexture> {
    static mut TEXTURES: Option<HashMap<usize, WasmTexture>> = None;
    unsafe {
        #[allow(static_mut_refs)]
        TEXTURES.get_or_insert_with(HashMap::new)
    }
}

fn next_texture_id() -> usize {
    static mut NEXT_ID: usize = 1;
    unsafe {
        let id = NEXT_ID;
        NEXT_ID += 1;
        id
    }
}

/// Bytes per pixel for the handful of `SDL_PIXELFORMAT_*` enum values this codebase
/// actually passes to `create_texture` (`RGB24`; `ARGB8888` isn't used for texture
/// creation today, but included for robustness). Anything else defaults to 4 -- a
/// reasonable guess, not a claim of correctness for a format this codebase never uses.
fn bytes_per_pixel_for_format(format: u32) -> usize {
    const SDL_PIXELFORMAT_RGB24: u32 = 386930691;
    const SDL_PIXELFORMAT_ARGB8888: u32 = 372645892;
    match format {
        SDL_PIXELFORMAT_RGB24 => 3,
        SDL_PIXELFORMAT_ARGB8888 => 4,
        _ => 4,
    }
}

/// The window's pixel dimensions, as given to `create_window`/`create_renderer` -- default
/// matches this game's native resolution (320x200) so a render pipeline probed before a
/// real window/renderer exists (shouldn't happen, but no reason to crash if it does) still
/// has a sane screen-buffer size.
static mut WINDOW_SIZE: (c_int, c_int) = (320, 200);

/// Set by `render_set_logical_size` (`seg009.rs` calls it with `(320*5, 200*6)` or
/// `(320, 200)` depending on aspect ratio -- both real, meaningful calls, not a no-op to
/// ignore). Defaulting to `(320, 200)` (rather than `(0, 0)`, real SDL's "never set" value)
/// matters: `menu.rs`'s `read_mouse_state` divides by this, and a real SDL game booting
/// straight into the pause menu before its first `render_set_logical_size` call would
/// otherwise divide by zero -- `(320, 200)` is a safe, faithful stand-in either way, since
/// it's this game's native resolution.
static mut LOGICAL_SIZE: (c_int, c_int) = (320, 200);

/// `None` means "the real screen" (matches SDL's `SDL_SetRenderTarget(renderer, NULL)`);
/// `Some(id)` means render calls act on that texture instead (render-to-texture).
static mut CURRENT_RENDER_TARGET: Option<usize> = None;

struct ScreenBuffer {
    w: c_int,
    h: c_int,
    bytes_per_pixel: usize,
    pixels: Vec<u8>,
}

fn screen_buffer() -> &'static mut ScreenBuffer {
    static mut SCREEN: Option<ScreenBuffer> = None;
    unsafe {
        #[allow(static_mut_refs)]
        SCREEN.get_or_insert_with(|| {
            let (w, h) = WINDOW_SIZE;
            ScreenBuffer { w, h, bytes_per_pixel: 3, pixels: vec![0u8; (w * h * 3) as usize] }
        })
    }
}

/// Resolves the current render target to a `(pixels, w, h, bytes_per_pixel)` view, whether
/// it's the screen or a texture. Returns `None` for a target texture id that no longer
/// exists (freed/never created) -- callers treat that as a no-op, matching SDL's own
/// behavior of silently failing render calls against an invalid target.
fn current_target_mut() -> Option<(&'static mut [u8], c_int, c_int, usize)> {
    unsafe {
        match CURRENT_RENDER_TARGET {
            None => {
                let s = screen_buffer();
                Some((&mut s.pixels, s.w, s.h, s.bytes_per_pixel))
            }
            Some(id) => textures().get_mut(&id).map(|t| (&mut t.pixels[..], t.w, t.h, t.bytes_per_pixel)),
        }
    }
}

/// The most recently `render_present`-ed frame, in whatever format the screen buffer was
/// last written in (today: `SDL_PIXELFORMAT_RGB24`, matching `texture_sharp`/etc.). This is
/// the real remaining piece of Phase B: a JS-facing accessor (converting to RGBA8 for
/// `ImageData`/canvas) belongs here once the Worker/JS harness exists to call it. For now
/// this just makes `render_present` observably do something testable without a browser.
static mut PRESENTED_FRAME: Option<(c_int, c_int, usize, Vec<u8>)> = None;

/// Reads back the most recent `render_present`-ed frame -- `(width, height,
/// bytes_per_pixel, pixels)`, or `None` if nothing has been presented yet. This is the real
/// accessor the eventual JS-facing "get current frame" call will use (converting to RGBA8
/// for `ImageData`/canvas); exposed now mainly so the pixel-parity test can verify
/// `render_present` actually did something, not just that it didn't panic.
pub(crate) fn last_presented_frame() -> Option<(c_int, c_int, usize, Vec<u8>)> {
    #[allow(static_mut_refs)]
    unsafe { PRESENTED_FRAME.clone() }
}

pub struct WasmRenderer;

// `platform::backend::ActiveRenderer` only resolves to `WasmRenderer` when actually
// targeting wasm32; on native (this module is also compiled there under `cargo test`, see
// platform/mod.rs) it resolves to `SdlRenderer` instead, so the static/accessor here must
// be typed directly rather than through the backend alias.
#[cfg(target_arch = "wasm32")]
static mut SHARED_RENDERER: crate::platform::backend::ActiveRenderer = WasmRenderer;
#[cfg(not(target_arch = "wasm32"))]
static mut SHARED_RENDERER: WasmRenderer = WasmRenderer;

#[cfg(target_arch = "wasm32")]
#[allow(static_mut_refs)]
pub fn shared_renderer() -> &'static mut crate::platform::backend::ActiveRenderer {
    unsafe { &mut SHARED_RENDERER }
}
#[cfg(not(target_arch = "wasm32"))]
#[allow(static_mut_refs)]
pub fn shared_renderer() -> &'static mut WasmRenderer {
    unsafe { &mut SHARED_RENDERER }
}

impl Renderer for WasmRenderer {
    unsafe fn create_surface(&mut self, width: c_int, height: c_int, depth: c_int, rmask: u32, gmask: u32, bmask: u32, amask: u32) -> *mut SDL_Surface {
        // Real SDL_CreateRGBSurface picks sensible non-degenerate default masks when asked
        // for a non-indexed surface with all-zero masks ("just give me a color format at
        // this depth"); this codebase's own RGB24_bug_check (seg009.rs) relies on exactly
        // that "auto-pick" behavior for its 1x1 depth-24 probe surface. Storing the literal
        // zeros instead (the prior behavior here) makes `pack_pixel`'s `if mask != 0` guard
        // never set any channel bit, so the probe's "did the red channel come back set?"
        // check degenerates to `0 & 0 == 0`, always true -- RGB24_bug_check always concludes
        // "channel-order bug present" on wasm and every 24bpp `safe_fill_rect` (palace wall
        // colours, HP flash, fades, ...) swaps R and B for a bug that doesn't exist in this
        // software renderer, producing wrong (but structurally intact) colors. Match this
        // codebase's one real 24bpp caller (`make_offscreen_buffer`, `Rmsk`/`Gmsk`/`Bmsk` in
        // seg009.rs) rather than inventing a new byte order.
        let (rmask, gmask, bmask, amask) = if depth != 8 && rmask == 0 && gmask == 0 && bmask == 0 {
            match depth {
                24 => (0x000000ffu32, 0x0000ff00u32, 0x00ff0000u32, 0u32),
                32 => (0x000000ffu32, 0x0000ff00u32, 0x00ff0000u32, 0xff000000u32),
                _ => (rmask, gmask, bmask, amask),
            }
        } else {
            (rmask, gmask, bmask, amask)
        };
        let bytes_per_pixel = ((depth.max(0) + 7) / 8) as u8;
        let pitch = width.max(0) * bytes_per_pixel as c_int;
        let pixels = vec![0u8; (pitch as usize) * (height.max(0) as usize)];
        let mut palette = if depth == 8 { Some(WasmPalette::new()) } else { None };
        let format_ptr = Box::new(SDL_PixelFormat {
            format: 0,
            palette: palette.as_mut().map_or(std::ptr::null_mut(), |p| p.as_ptr()),
            BitsPerPixel: depth as u8,
            BytesPerPixel: bytes_per_pixel,
            padding: [0; 2],
            Rmask: rmask, Gmask: gmask, Bmask: bmask, Amask: amask,
            // Every mask this codebase uses is a full byte-aligned 8-bit channel, so loss
            // (bits discarded packing a component into fewer bits) is always 0.
            Rloss: 0, Gloss: 0, Bloss: 0, Aloss: 0,
            Rshift: shift_for(rmask) as u8, Gshift: shift_for(gmask) as u8,
            Bshift: shift_for(bmask) as u8, Ashift: shift_for(amask) as u8,
            refcount: 1,
            next: std::ptr::null_mut(),
        });
        let s = WasmSurface {
            w: width,
            h: height,
            pitch,
            pixels,
            bits_per_pixel: depth as u8,
            bytes_per_pixel,
            rmask, gmask, bmask, amask,
            rshift: shift_for(rmask), gshift: shift_for(gmask), bshift: shift_for(bmask), ashift: shift_for(amask),
            palette,
            color_key: None,
            blend_mode: SDL_BLENDMODE_NONE,
            alpha_mod: 255,
            clip_rect: None,
            format_ptr,
        };
        let id = next_surface_id();
        surfaces().insert(id, s);
        id as *mut SDL_Surface
    }

    unsafe fn free_surface(&mut self, surf: *mut SDL_Surface) {
        surfaces().remove(&(surf as usize));
    }

    unsafe fn surface_size(&mut self, surf: *mut SDL_Surface) -> (c_int, c_int) {
        let s = surf_mut(surf);
        (s.w, s.h)
    }

    unsafe fn surface_pitch(&mut self, surf: *mut SDL_Surface) -> c_int {
        surf_mut(surf).pitch
    }

    unsafe fn surface_pixels(&mut self, surf: *mut SDL_Surface) -> *mut std::os::raw::c_void {
        surf_mut(surf).pixels.as_mut_ptr() as *mut std::os::raw::c_void
    }

    unsafe fn surface_format_info(&mut self, surf: *mut SDL_Surface) -> PixelFormatInfo {
        let s = surf_mut(surf);
        PixelFormatInfo {
            bits_per_pixel: s.bits_per_pixel,
            bytes_per_pixel: s.bytes_per_pixel,
            rmask: s.rmask,
            gmask: s.gmask,
            bmask: s.bmask,
            amask: s.amask,
            // Real SDL_PIXELFORMAT_INDEX8/ARGB8888 enum values -- only the "is this
            // indexed?" bit-pattern actually gets read anywhere (SDL_ISPIXELFORMAT_INDEXED
            // in seg009.rs), so reusing SDL's real constants for the two depths this
            // codebase actually creates is simpler and more honest than hand-encoding the
            // SDL_PIXELTYPE/ORDER/LAYOUT bit-packing macro ourselves.
            format: if s.bits_per_pixel == 8 { 0x13000801 } else { 0x16362004 },
        }
    }

    unsafe fn surface_palette(&mut self, surf: *mut SDL_Surface) -> *mut crate::SDL_Palette {
        match surf_mut(surf).palette.as_mut() {
            Some(p) => p.as_ptr(),
            None => std::ptr::null_mut(),
        }
    }

    unsafe fn surface_format_ptr(&mut self, surf: *mut SDL_Surface) -> *mut SDL_PixelFormat {
        surf_mut(surf).format_ptr.as_mut() as *mut SDL_PixelFormat
    }

    unsafe fn load_image_from_memory(&mut self, bytes: &[u8]) -> *mut SDL_Surface {
        self.decode_png_to_surface(bytes)
    }
    unsafe fn load_image_from_file(&mut self, path: &std::ffi::CStr) -> *mut SDL_Surface {
        match crate::wasm_vfs::vfs_read(&path.to_string_lossy()) {
            Some(bytes) => self.decode_png_to_surface(&bytes),
            None => std::ptr::null_mut(),
        }
    }
    unsafe fn img_load_rw(&mut self, rw: *mut SDL_RWops, freesrc: c_int) -> *mut SDL_Surface {
        let result = match rw_handles().get(&(rw as usize)) {
            Some(h) => self.decode_png_to_surface(&h.data),
            None => std::ptr::null_mut(),
        };
        if freesrc != 0 {
            self.rw_close(rw);
        }
        result
    }

    unsafe fn lock_surface(&mut self, _surf: *mut SDL_Surface) -> c_int {
        // Pixels are always accessible (plain owned Vec<u8>, no real device memory).
        0
    }
    unsafe fn unlock_surface(&mut self, _surf: *mut SDL_Surface) {}

    unsafe fn set_color_key(&mut self, surf: *mut SDL_Surface, enable: bool, key: u32) -> c_int {
        surf_mut(surf).color_key = if enable { Some(key) } else { None };
        0
    }

    unsafe fn set_palette(&mut self, surf: *mut SDL_Surface, colors: *const SDL_Color, first_color: c_int, n_colors: c_int) {
        let s = surf_mut(surf);
        if let Some(p) = s.palette.as_mut() {
            let dst = p.colors_mut();
            for i in 0..n_colors {
                let idx = (first_color + i) as usize;
                if idx < 256 {
                    dst[idx] = *colors.add(i as usize);
                }
            }
        }
    }

    unsafe fn set_palette_colors(&mut self, palette: *mut crate::SDL_Palette, colors: *const SDL_Color, first_color: c_int, n_colors: c_int) -> c_int {
        if palette.is_null() {
            return -1;
        }
        let dst_colors = (*palette).colors;
        for i in 0..n_colors {
            let idx = (first_color + i) as isize;
            if idx >= 0 && idx < (*palette).ncolors as isize {
                *dst_colors.offset(idx) = *colors.add(i as usize);
            }
        }
        0
    }

    unsafe fn set_surface_palette(&mut self, surf: *mut SDL_Surface, palette: *mut crate::SDL_Palette) -> c_int {
        // Real SDL shares the palette object by reference; we don't have surfaces sharing
        // storage, so copy the color entries instead -- observably equivalent for every
        // current caller (none mutate one surface's palette expecting another to change).
        if palette.is_null() {
            return -1;
        }
        let n = (*palette).ncolors.min(256);
        let src = (*palette).colors;
        let s = surf_mut(surf);
        if s.palette.is_none() {
            s.palette = Some(WasmPalette::new());
            // format_ptr.palette was null at creation time (this surface wasn't 8bpp
            // then) -- keep it in sync now that a palette actually exists, the same
            // invariant create_surface establishes up front for surfaces created with
            // depth == 8.
            s.format_ptr.palette = s.palette.as_mut().unwrap().as_ptr();
        }
        let dst = s.palette.as_mut().unwrap().colors_mut();
        for i in 0..n {
            dst[i as usize] = *src.offset(i as isize);
        }
        0
    }

    unsafe fn convert_surface(&mut self, src: *mut SDL_Surface, fmt: *const SDL_PixelFormat, _flags: u32) -> *mut SDL_Surface {
        let (bits_per_pixel, rmask, gmask, bmask, amask) = (
            (*fmt).BitsPerPixel,
            (*fmt).Rmask,
            (*fmt).Gmask,
            (*fmt).Bmask,
            (*fmt).Amask,
        );
        self.convert_to(src, bits_per_pixel as c_int, rmask, gmask, bmask, amask)
    }

    unsafe fn set_blend_mode(&mut self, surf: *mut SDL_Surface, mode: c_int) -> c_int {
        surf_mut(surf).blend_mode = mode;
        0
    }

    unsafe fn set_alpha_mod(&mut self, surf: *mut SDL_Surface, alpha: u8) {
        surf_mut(surf).alpha_mod = alpha;
    }

    unsafe fn map_rgba(&mut self, format: *const SDL_PixelFormat, r: u8, g: u8, b: u8, a: u8) -> u32 {
        let rmask = (*format).Rmask;
        let gmask = (*format).Gmask;
        let bmask = (*format).Bmask;
        let amask = (*format).Amask;
        pack_pixel(rmask, gmask, bmask, amask, r, g, b, a)
    }

    unsafe fn map_rgb(&mut self, format: *const SDL_PixelFormat, r: u8, g: u8, b: u8) -> u32 {
        self.map_rgba(format, r, g, b, 255)
    }

    unsafe fn save_png(&mut self, _surf: *mut SDL_Surface, _path: &std::ffi::CStr) -> c_int {
        // Reachable from live gameplay (F12 / Shift+F12 screenshot keys, screenshot.rs) --
        // was a real crash before this fix. Encoding itself would be easy (the `png` crate
        // is already a dependency, used by load_image_from_file/from_memory), but there's
        // nowhere on this target to put the result yet: no virtual filesystem write path
        // wired up for it (WasmFiles is unwired entirely -- see mod.rs) and no JS bridge to
        // trigger a browser download. Fail gracefully instead of panicking; the caller
        // (save_screenshot/save_level_screenshot, via show_result) already handles a
        // nonzero return by logging "Error saving screenshot" instead of crashing -- same
        // outcome a real disk-full/permission error would produce on native.
        -1
    }

    unsafe fn get_error(&mut self) -> *const std::os::raw::c_char {
        static MSG: &[u8] = b"(wasm renderer error)\0";
        MSG.as_ptr() as *const std::os::raw::c_char
    }

    unsafe fn blit(&mut self, src: *mut SDL_Surface, src_rect: *const SDL_Rect, dst: *mut SDL_Surface, dst_rect: *mut SDL_Rect) -> c_int {
        self.blit_impl(src, src_rect, dst, dst_rect, false)
    }

    unsafe fn blit_scaled(&mut self, src: *mut SDL_Surface, src_rect: *const SDL_Rect, dst: *mut SDL_Surface, dst_rect: *mut SDL_Rect) -> c_int {
        self.blit_impl(src, src_rect, dst, dst_rect, true)
    }

    unsafe fn fill_rect(&mut self, surf: *mut SDL_Surface, rect: *const SDL_Rect, color: u32) -> c_int {
        let s = surf_mut(surf);
        let (x0, y0, w, h) = match rect.as_ref() {
            Some(r) => (r.x, r.y, r.w, r.h),
            None => (0, 0, s.w, s.h),
        };
        let bpp = s.bytes_per_pixel as usize;
        let bytes = color.to_ne_bytes();
        for y in y0.max(0)..(y0 + h).min(s.h) {
            for x in x0.max(0)..(x0 + w).min(s.w) {
                let off = (y * s.pitch) as usize + (x as usize) * bpp;
                s.pixels[off..off + bpp].copy_from_slice(&bytes[..bpp]);
            }
        }
        0
    }

    unsafe fn present(&mut self, _frame: *mut SDL_Surface) {
        // Real canvas presentation is Phase B/C territory (needs the loop-driving JS
        // integration to know when a frame is actually ready to show); not implemented yet.
        unimplemented!("WasmRenderer::present")
    }
    unsafe fn set_fullscreen(&mut self, _fullscreen: bool) {
        // Reachable from the in-game settings menu's fullscreen toggle (menu.rs) and
        // seg009.rs's startup fullscreen handling. Real fullscreen needs the browser's
        // Fullscreen API (`element.requestFullscreen()`), which needs a JS bridge that
        // doesn't exist yet -- a no-op here is an honest "not supported yet," matching
        // get_window_flags always reporting "not fullscreen," rather than a placeholder
        // masking a real bug. Toggling the setting still works (it's stored regardless);
        // it just has no visible effect until a real bridge is wired up.
    }
    unsafe fn show_cursor(&mut self, _show: bool) {
        // Real cursor hiding needs DOM/CSS control over the canvas element (e.g. toggling
        // a `cursor: none` class), which no JS bridge exists for yet -- the browser's
        // default cursor is a reasonable interim behavior, not a placeholder that needs
        // fixing urgently. A no-op here (matching menu.rs's only caller,
        // process_additional_menu_input/menu_was_closed, which only ever hides the cursor
        // while the window is fullscreen -- get_window_flags always reports "not
        // fullscreen" on wasm, so `show(false)` is unreachable in practice today anyway).
    }
    unsafe fn delay(&mut self, ms: u32) {
        // Real busy-spin, the accepted first-pass tradeoff for frame pacing (see the plan
        // doc's Phase B "known tradeoff" note) -- no `SharedArrayBuffer`/`Atomics.wait`
        // available for a real blocking sleep inside a Worker without cross-origin-isolation
        // headers. Doubles as the audio pump's only real timing source: the game's audio
        // callback is normally pulled by a dedicated realtime thread real SDL owns, which
        // doesn't exist here, so `pump_audio` is called from every spin of every wait point
        // in the game (this, and once per `render_present`) instead.
        let target = performance_now_ms() + ms as f64;
        loop {
            pump_audio();
            if performance_now_ms() >= target {
                break;
            }
        }
    }
    unsafe fn rw_from_mem(&mut self, buf: *mut std::os::raw::c_void, size: c_int) -> *mut SDL_RWops {
        let data = std::slice::from_raw_parts(buf as *const u8, size.max(0) as usize).to_vec();
        let id = next_rw_id();
        rw_handles().insert(id, WasmRw { data, pos: 0, write_back_path: None });
        id as *mut SDL_RWops
    }
    unsafe fn rw_tell(&mut self, rw: *mut SDL_RWops) -> i64 {
        match rw_handles().get(&(rw as usize)) {
            Some(h) => h.pos as i64,
            None => -1,
        }
    }
    unsafe fn rw_close(&mut self, rw: *mut SDL_RWops) -> c_int {
        if let Some(h) = rw_handles().remove(&(rw as usize)) {
            if let Some(path) = h.write_back_path {
                crate::wasm_vfs::vfs_write(&path, h.data);
            }
        }
        0
    }
    unsafe fn rw_write(&mut self, rw: *mut SDL_RWops, ptr: *const std::os::raw::c_void, size: usize, num: usize) -> usize {
        let Some(h) = rw_handles().get_mut(&(rw as usize)) else { return 0 };
        if size == 0 { return 0; }
        let n = size * num;
        let end = h.pos + n;
        if h.data.len() < end {
            h.data.resize(end, 0);
        }
        let src = std::slice::from_raw_parts(ptr as *const u8, n);
        h.data[h.pos..end].copy_from_slice(src);
        h.pos = end;
        num
    }
    unsafe fn rw_read(&mut self, rw: *mut SDL_RWops, ptr: *mut std::os::raw::c_void, size: usize, maxnum: usize) -> usize {
        let Some(h) = rw_handles().get_mut(&(rw as usize)) else { return 0 };
        if size == 0 { return 0; }
        let want = size * maxnum;
        let avail = h.data.len().saturating_sub(h.pos);
        let n = want.min(avail);
        if n > 0 {
            std::ptr::copy_nonoverlapping(h.data[h.pos..h.pos + n].as_ptr(), ptr as *mut u8, n);
            h.pos += n;
        }
        n / size
    }
    unsafe fn show_message_box(&mut self, _title: &std::ffi::CStr, _message: &std::ffi::CStr) {
        unimplemented!("WasmRenderer::show_message_box")
    }
    unsafe fn linked_sdl_version(&mut self) -> (u8, u8, u8) {
        // No real SDL2 exists to ask -- report a recent version, so the one caller
        // (`init_digi`'s "SDL older than 2.0.4 has a resampling bug" workaround) takes the
        // modern (16-bit audio) branch, matching what real SDL2 on any current system reports.
        (2, 30, 0)
    }
    unsafe fn performance_counter(&mut self) -> u64 {
        (performance_now_ms() * 1000.0) as u64 // microsecond-resolution counter
    }
    unsafe fn performance_frequency(&mut self) -> u64 {
        1_000_000 // matches the microsecond unit above -- only the ratio between two
                  // counter readings is ever used, not any absolute epoch, so this and
                  // performance_counter just need to agree with each other.
    }
    unsafe fn rw_from_file(&mut self, path: &std::ffi::CStr, mode: &std::ffi::CStr) -> *mut SDL_RWops {
        let path = path.to_string_lossy().into_owned();
        let mode = mode.to_string_lossy();
        let writable = mode.starts_with('w') || mode.starts_with('a');
        let (data, write_back_path) = if writable {
            (Vec::new(), Some(path))
        } else {
            match crate::wasm_vfs::vfs_read(&path) {
                Some(bytes) => (bytes, None),
                None => return std::ptr::null_mut(),
            }
        };
        let id = next_rw_id();
        rw_handles().insert(id, WasmRw { data, pos: 0, write_back_path });
        id as *mut SDL_RWops
    }
    unsafe fn get_scancode_name(&mut self, scancode: u32) -> *const std::os::raw::c_char {
        // Only reachable from the in-game key-rebinding settings row (menu.rs), which
        // immediately CStr::from_ptr's the result -- must return a real, non-null,
        // NUL-terminated string, not a stub. Named the scancodes web/index.html's own
        // SCANCODE map actually forwards from the keyboard (the only ones a wasm build's
        // key-rebind screen could ever show as "already bound" or let you rebind to);
        // matches real SDL_GetScancodeName's naming, e.g. "Left Ctrl" not "LCtrl". Falls
        // back to a generic numbered name for anything else, formatted into a thread-local
        // buffer -- real SDL_GetScancodeName also returns a pointer into static/internal
        // storage the caller never frees, so a 'static buffer here matches that contract.
        let name: std::borrow::Cow<'static, str> = match scancode {
            80 => "Left".into(),
            79 => "Right".into(),
            82 => "Up".into(),
            81 => "Down".into(),
            44 => "Space".into(),
            40 => "Return".into(),
            41 => "Escape".into(),
            42 => "Backspace".into(),
            43 => "Tab".into(),
            76 => "Delete".into(),
            225 => "Left Shift".into(),
            229 => "Right Shift".into(),
            224 => "Left Ctrl".into(),
            228 => "Right Ctrl".into(),
            226 => "Left Alt".into(),
            230 => "Right Alt".into(),
            53 => "Grave".into(),
            58..=69 => format!("F{}", scancode - 57).into(),
            0 => "".into(),
            _ => format!("Key {}", scancode).into(),
        };
        thread_local! {
            static NAME_BUF: std::cell::RefCell<std::ffi::CString> =
                std::cell::RefCell::new(std::ffi::CString::default());
        }
        NAME_BUF.with(|buf| {
            let mut buf = buf.borrow_mut();
            *buf = std::ffi::CString::new(name.into_owned()).unwrap_or_default();
            buf.as_ptr()
        })
    }
    unsafe fn get_window_flags(&mut self, _window: *mut crate::SDL_Window) -> u32 {
        // Both call sites (menu.rs's process_additional_menu_input/menu_was_closed) only
        // ever check the SDL_WINDOW_FULLSCREEN_DESKTOP bit, to decide whether to hide the
        // mouse cursor while idle. No real OS window exists here -- the canvas is never
        // "fullscreen" in that sense (this build doesn't implement the Fullscreen API
        // either; set_fullscreen is its own separate unimplemented stub) -- so always
        // reporting no flags set is exactly correct, not a placeholder: it makes the game
        // always show the cursor, the right behavior for a normal browser tab.
        0
    }
    unsafe fn render_get_scale(&mut self, _renderer: *mut crate::SDL_Renderer) -> (f32, f32) {
        // Found wrong during the Phase D mouse-input followup (2026-08-08): this used to
        // hardcode (1.0, 1.0) on the theory that nothing here calls SDL_RenderSetScale, but
        // that missed that real SDL_RenderGetScale also reflects SDL_RenderSetLogicalSize
        // (apply_aspect_ratio, seg009.rs, always called at startup with 320x200 or 1600x1200)
        // -- SDL computes and applies a real output/logical scale factor internally, it isn't
        // just a passthrough for an explicit SetScale call. Native's own render_get_scale
        // (platform/sdl.rs) is a thin call to the real SDL_RenderGetScale, so it was already
        // correct; this wasm shim needs to compute the same ratio by hand: window output
        // pixels per logical unit. The mismatch was invisible until a real mouse click was
        // exercised in the browser for the first time (this session) -- `menu.rs`'s
        // `read_mouse_state` multiplies raw mouse coordinates (in real 640x400 output-pixel
        // space) by this scale to convert them into the 320x200-logical space menu item hit
        // rects are defined in; with a hardcoded 1.0, clicks landed roughly 2x too far from
        // the actual target, so hover/click never matched the row the cursor was visually
        // over.
        let (win_w, win_h) = WINDOW_SIZE;
        let (log_w, log_h) = LOGICAL_SIZE;
        if log_w == 0 || log_h == 0 {
            return (1.0, 1.0);
        }
        (win_w as f32 / log_w as f32, win_h as f32 / log_h as f32)
    }
    unsafe fn render_get_logical_size(&mut self, _renderer: *mut crate::SDL_Renderer) -> (c_int, c_int) {
        LOGICAL_SIZE
    }
    unsafe fn render_get_viewport(&mut self, _renderer: *mut crate::SDL_Renderer) -> SDL_Rect {
        // No SDL_RenderSetViewport equivalent exists here (nothing in this codebase calls
        // one -- confirmed by grep), so the viewport is always the full logical render
        // target, matching real SDL's own default.
        let (w, h) = LOGICAL_SIZE;
        SDL_Rect { x: 0, y: 0, w, h }
    }
    unsafe fn render_set_integer_scale(&mut self, _renderer: *mut crate::SDL_Renderer, _enable: bool) -> c_int {
        // Reachable from the pause menu's "Use integer scaling" toggle (menu.rs, both
        // turning it on and off) -- was a real crash before this fix. Same reasoning as
        // render_set_logical_size just above: display scaling of the presented frame is a
        // JS/CSS concern on this target (render_present hands the raw logical-size pixel
        // buffer to JS and stops), not something this layer applies itself, so there is no
        // real scale-mode state here to set. A no-op success return matches real SDL's own
        // behavior when integer scaling isn't supported for the current renderer/target.
        0
    }

    unsafe fn set_clip_rect(&mut self, surf: *mut SDL_Surface, rect: *const SDL_Rect) -> c_int {
        surf_mut(surf).clip_rect = rect.as_ref().copied();
        1
    }

    unsafe fn convert_surface_format(&mut self, src: *mut SDL_Surface, pixel_format: u32, _flags: u32) -> *mut SDL_Surface {
        const SDL_PIXELFORMAT_ARGB8888: u32 = 372645892;
        if pixel_format != SDL_PIXELFORMAT_ARGB8888 {
            unimplemented!("WasmRenderer::convert_surface_format for format {pixel_format} (only ARGB8888 is used by this codebase today)");
        }
        self.convert_to(src, 32, 0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000)
    }

    unsafe fn set_window_icon(&mut self, _window: *mut crate::SDL_Window, _icon: *mut SDL_Surface) {
        // No real OS window exists to have a titlebar icon; a browser tab uses a favicon
        // instead, a separate (and separately-configured) mechanism not modeled here.
    }
    unsafe fn rw_from_const_mem(&mut self, mem: *const std::os::raw::c_void, size: c_int) -> *mut SDL_RWops {
        let data = std::slice::from_raw_parts(mem as *const u8, size.max(0) as usize).to_vec();
        let id = next_rw_id();
        rw_handles().insert(id, WasmRw { data, pos: 0, write_back_path: None });
        id as *mut SDL_RWops
    }
    unsafe fn create_texture(&mut self, _renderer: *mut crate::SDL_Renderer, format: u32, _access: c_int, w: c_int, h: c_int) -> *mut crate::SDL_Texture {
        let bytes_per_pixel = bytes_per_pixel_for_format(format);
        let pixels = vec![0u8; (w.max(0) as usize) * (h.max(0) as usize) * bytes_per_pixel];
        let id = next_texture_id();
        textures().insert(id, WasmTexture { w, h, bytes_per_pixel, pixels });
        id as *mut crate::SDL_Texture
    }
    unsafe fn update_texture(&mut self, texture: *mut crate::SDL_Texture, rect: *const SDL_Rect, pixels: *const std::os::raw::c_void, pitch: c_int) -> c_int {
        let Some(t) = textures().get_mut(&(texture as usize)) else { return -1 };
        let (x0, y0, w, h) = match rect.as_ref() {
            Some(r) => (r.x, r.y, r.w, r.h),
            None => (0, 0, t.w, t.h),
        };
        let bpp = t.bytes_per_pixel;
        let src = pixels as *const u8;
        for row in 0..h {
            let dy = y0 + row;
            if dy < 0 || dy >= t.h { continue; }
            let src_off = (row * pitch) as usize;
            let dst_off = (dy * t.w) as usize * bpp + (x0.max(0) as usize) * bpp;
            let n = (w.min(t.w - x0.max(0))).max(0) as usize * bpp;
            if n == 0 { continue; }
            std::ptr::copy_nonoverlapping(src.add(src_off), t.pixels[dst_off..].as_mut_ptr(), n);
        }
        0
    }
    unsafe fn set_render_target(&mut self, _renderer: *mut crate::SDL_Renderer, texture: *mut crate::SDL_Texture) -> c_int {
        CURRENT_RENDER_TARGET = if texture.is_null() { None } else { Some(texture as usize) };
        0
    }
    unsafe fn render_clear(&mut self, _renderer: *mut crate::SDL_Renderer) -> c_int {
        let Some((pixels, _, _, _)) = current_target_mut() else { return -1 };
        pixels.fill(0);
        0
    }
    unsafe fn render_copy(&mut self, _renderer: *mut crate::SDL_Renderer, texture: *mut crate::SDL_Texture, src_rect: *const SDL_Rect, dst_rect: *const SDL_Rect) -> c_int {
        // Copying a texture onto itself (render target == source) isn't something real SDL
        // supports either -- reject it explicitly rather than let it slip through, since
        // `current_target_mut()`'s `'static` borrow doesn't let the compiler catch the
        // resulting aliased-mutable-and-immutable-reference-into-the-same-HashMap-entry
        // case for us the way it normally would.
        if CURRENT_RENDER_TARGET == Some(texture as usize) {
            return -1;
        }
        let Some(t) = textures().get(&(texture as usize)) else { return -1 };
        let (t_w, t_h, t_bpp) = (t.w, t.h, t.bytes_per_pixel);
        let (sx0, sy0, sw, sh) = match src_rect.as_ref() {
            Some(r) => (r.x, r.y, r.w, r.h),
            None => (0, 0, t_w, t_h),
        };
        let Some((dst_pixels, d_w, d_h, d_bpp)) = current_target_mut() else { return -1 };
        let (dx0, dy0, dw, dh) = match dst_rect.as_ref() {
            Some(r) => (r.x, r.y, r.w, r.h),
            None => (0, 0, d_w, d_h),
        };
        let t = &textures()[&(texture as usize)];
        for y in 0..dh {
            let sy = sy0 + if sh > 0 { y * sh / dh.max(1) } else { y };
            for x in 0..dw {
                let sx = sx0 + if sw > 0 { x * sw / dw.max(1) } else { x };
                let (dyy, dxx) = (dy0 + y, dx0 + x);
                if dyy < 0 || dyy >= d_h || dxx < 0 || dxx >= d_w { continue; }
                if sy < 0 || sy >= t_h || sx < 0 || sx >= t_w { continue; }
                let s_off = (sy * t_w) as usize * t_bpp + (sx as usize) * t_bpp;
                let d_off = (dyy * d_w) as usize * d_bpp + (dxx as usize) * d_bpp;
                let n = t_bpp.min(d_bpp);
                dst_pixels[d_off..d_off + n].copy_from_slice(&t.pixels[s_off..s_off + n]);
            }
        }
        0
    }
    unsafe fn render_present(&mut self, _renderer: *mut crate::SDL_Renderer) {
        pump_audio();
        let s = screen_buffer();
        post_frame_to_js(s.w, s.h, s.bytes_per_pixel, &s.pixels);
        PRESENTED_FRAME = Some((s.w, s.h, s.bytes_per_pixel, s.pixels.clone()));
    }
    unsafe fn render_set_logical_size(&mut self, _renderer: *mut crate::SDL_Renderer, w: c_int, h: c_int) -> c_int {
        // Actual canvas *scaling* is a JS/CSS concern in the real browser build, not
        // something this layer manages -- but the *value* itself must still be stored:
        // menu.rs's read_mouse_state divides by it (via render_get_logical_size) to convert
        // raw mouse coordinates into game-logical ones, real math this codebase depends on,
        // not just a display nicety.
        LOGICAL_SIZE = (w, h);
        0
    }
    unsafe fn get_renderer_output_size(&mut self, _renderer: *mut crate::SDL_Renderer) -> (c_int, c_int) {
        WINDOW_SIZE
    }
    unsafe fn get_renderer_info_flags(&mut self, _renderer: *mut crate::SDL_Renderer) -> u32 {
        // Report no flags set (in particular, no SDL_RENDERER_TARGETTEXTURE) -- steers the
        // game onto its non-target-texture rendering fallback, which is the simpler path
        // and doesn't need a real GPU-backed texture concept in a Canvas renderer.
        0
    }
    unsafe fn set_hint(&mut self, _name: &std::ffi::CStr, _value: &std::ffi::CStr) -> c_int {
        // Real SDL hints (render scale quality, etc.) have no equivalent in a Canvas-based
        // renderer -- there's no "hint system" to configure. Report success (SDL_TRUE) so
        // callers that check the return value don't treat this as an error.
        1
    }
    unsafe fn sdl_init(&mut self, _flags: u32) -> c_int {
        // No real SDL subsystems exist here (Canvas/Web Audio/message-passing input stand
        // in for video/audio/joystick) -- nothing to actually initialize. Report success.
        0
    }
    unsafe fn sdl_init_subsystem(&mut self, _flags: u32) -> c_int {
        0
    }
    unsafe fn sdl_quit(&mut self) {
        // No real subsystems were ever initialized by sdl_init above -- nothing to tear
        // down.
    }
    unsafe fn create_window(&mut self, _title: &std::ffi::CStr, _x: c_int, _y: c_int, w: c_int, h: c_int, _flags: u32) -> *mut crate::SDL_Window {
        // No real OS window exists (the browser tab/canvas is the "window"); a distinct
        // non-null sentinel is enough to satisfy code that only null-checks this and passes
        // it back into other Renderer methods, none of which currently dereference it.
        // Real size IS recorded (WINDOW_SIZE) -- get_renderer_output_size and the screen
        // buffer's own size need it.
        if w > 0 && h > 0 {
            WINDOW_SIZE = (w, h);
        }
        1 as *mut crate::SDL_Window
    }
    unsafe fn create_renderer(&mut self, _window: *mut crate::SDL_Window, _index: c_int, _flags: u32) -> *mut crate::SDL_Renderer {
        1 as *mut crate::SDL_Renderer
    }
    unsafe fn open_audio_raw(&mut self, desired: *mut std::os::raw::c_void, _obtained: *mut std::os::raw::c_void) -> c_int {
        let spec = desired as *const WasmSdlAudioSpec;
        let Some(callback) = (*spec).callback else { return -1 };
        AUDIO_SPEC = Some(AudioSpecInfo {
            freq: (*spec).freq,
            format: (*spec).format,
            channels: (*spec).channels,
            samples: (*spec).samples,
            callback,
            userdata: (*spec).userdata,
        });
        // Real SDL_OpenAudio starts a device paused; the game's own resume_sound()/similar
        // (-> AudioBackend::pause(false)) turns it on, matching real SDL semantics.
        AUDIO_PAUSED = true;
        NEXT_AUDIO_PUMP_MS = performance_now_ms();
        0
    }
    unsafe fn num_joysticks(&mut self) -> c_int {
        // No gamepad support yet -- report none, so callers (e.g. set_joy_mode) take the
        // keyboard-only branch and never reach the other joystick_open/game_controller_*
        // stubs below.
        0
    }
    unsafe fn is_game_controller(&mut self, _joystick_index: c_int) -> bool {
        unimplemented!("WasmRenderer::is_game_controller")
    }
    unsafe fn game_controller_open(&mut self, _joystick_index: c_int) -> *mut crate::SDL_GameController {
        unimplemented!("WasmRenderer::game_controller_open")
    }
    unsafe fn game_controller_close(&mut self, _controller: *mut crate::SDL_GameController) {
        unimplemented!("WasmRenderer::game_controller_close")
    }
    unsafe fn game_controller_from_instance_id(&mut self, _joyid: i32) -> *mut crate::SDL_GameController {
        unimplemented!("WasmRenderer::game_controller_from_instance_id")
    }
    unsafe fn game_controller_add_mappings_from_file(&mut self, _path: &std::ffi::CStr) -> c_int {
        unimplemented!("WasmRenderer::game_controller_add_mappings_from_file")
    }
    unsafe fn joystick_open(&mut self, _device_index: c_int) -> *mut crate::SDL_Joystick {
        unimplemented!("WasmRenderer::joystick_open")
    }
    unsafe fn haptic_open(&mut self, _device_index: c_int) -> *mut crate::SDL_Haptic {
        unimplemented!("WasmRenderer::haptic_open")
    }
    unsafe fn haptic_rumble_init(&mut self, _haptic: *mut crate::SDL_Haptic) -> c_int {
        unimplemented!("WasmRenderer::haptic_rumble_init")
    }
    unsafe fn push_event(&mut self, event: *mut std::os::raw::c_void) -> c_int {
        let mut bytes = [0u8; SDL_EVENT_SIZE];
        std::ptr::copy_nonoverlapping(event as *const u8, bytes.as_mut_ptr(), SDL_EVENT_SIZE);
        event_queue().push_back(bytes);
        1 // matches real SDL_PushEvent's success return value
    }
    unsafe fn poll_event(&mut self, event: *mut std::os::raw::c_void) -> c_int {
        // Live input (real key/mouse state written into a SharedArrayBuffer by the main
        // thread -- see `sync_shared_input`'s doc comment) plus edge detection turns the
        // level state `key_states()` holds into the discrete queued events `process_events`
        // (`seg009.rs`) actually consumes, the same way real SDL turns HID reports into an
        // event queue. `push_event` callers (native-only scripted-replay input injection,
        // and a couple of window/focus events -- none hit by the browser startup path yet)
        // feed the same queue.
        sync_shared_input();
        synthesize_key_edge_events();
        synthesize_mouse_edge_events();
        let Some(bytes) = event_queue().pop_front() else { return 0 };
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), event as *mut u8, SDL_EVENT_SIZE);
        1
    }
}

/// Unpacks PNG scanlines of a sub-8-bit indexed image into one byte (palette index) per
/// pixel. PNG bit-packing is MSB-first within each byte, and each scanline starts on a fresh
/// byte (trailing bits in a row's last byte, if width isn't a multiple of the pack ratio, are
/// padding -- ignored here since `row_bytes` is computed the same way and each row is sliced
/// independently). A no-op copy when `bit_depth == 8` (already one byte per pixel).
fn unpack_indexed_scanlines(packed: &[u8], width: usize, height: usize, bit_depth: u8) -> Vec<u8> {
    if bit_depth == 8 {
        return packed.to_vec();
    }
    let pixels_per_byte = 8 / bit_depth as usize;
    let row_bytes = width.div_ceil(pixels_per_byte);
    let mask: u8 = (1u16 << bit_depth) as u8 - 1;
    let mut out = vec![0u8; width * height];
    for y in 0..height {
        let row = &packed[y * row_bytes..(y * row_bytes + row_bytes).min(packed.len())];
        for x in 0..width {
            let byte_idx = x / pixels_per_byte;
            if byte_idx >= row.len() { break; }
            let shift = 8 - bit_depth as usize * (x % pixels_per_byte + 1);
            out[y * width + x] = (row[byte_idx] >> shift) & mask;
        }
    }
    out
}

/// Packs 8-bit RGBA components into a pixel value using the given channel masks --
/// `map_rgb`/`map_rgba`'s actual logic, generic over whatever masks a surface was created
/// with (SDLPoP always uses one of a few fixed mask sets, so this doesn't need to handle
/// arbitrary bit widths per channel, only arbitrary shift positions).
fn pack_pixel(rmask: u32, gmask: u32, bmask: u32, amask: u32, r: u8, g: u8, b: u8, a: u8) -> u32 {
    let mut v = 0u32;
    if rmask != 0 { v |= (r as u32) << shift_for(rmask); }
    if gmask != 0 { v |= (g as u32) << shift_for(gmask); }
    if bmask != 0 { v |= (b as u32) << shift_for(bmask); }
    if amask != 0 { v |= (a as u32) << shift_for(amask); }
    v
}

impl WasmRenderer {
    /// Decodes a PNG (via the `png` crate -- see the Phase C vetting notes in
    /// `docs/plans/13-platform-architecture-unification.md`), producing a surface shaped
    /// like real `IMG_Load` would: an 8bpp indexed surface with a real palette for a
    /// palette-based PNG (matching how `SDL_ISPIXELFORMAT_INDEXED`-gated code in
    /// `seg009.rs` branches on this), or a 32bpp RGBA surface for anything else. Null on
    /// decode failure, matching `IMG_Load`'s convention.
    unsafe fn decode_png_to_surface(&mut self, bytes: &[u8]) -> *mut SDL_Surface {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let Ok(mut reader) = decoder.read_info() else { return std::ptr::null_mut() };
        let (width, height) = (reader.info().width as c_int, reader.info().height as c_int);

        // Every real sprite/font asset in this codebase turned out to be Indexed color type
        // (confirmed empirically: 927/927 res*.png files), but NOT all 8-bit -- 1/2/4-bit
        // indexed PNGs are common (font glyphs especially). Unpacking those via this crate's
        // `EXPAND` transform (an earlier version of this function did that) resolves the
        // palette into real RGB(A) samples -- but callers like `method_3_blit_mono`
        // (`seg009.rs`) call `set_color_key(image, true, 0)` expecting pixel value `0` to mean
        // "palette index 0", a contract only an indexed surface satisfies; on an RGBA surface
        // "color key 0" almost never matches a real pixel, so the intended per-glyph
        // transparency silently stopped working (glyphs rendered as solid rectangles instead
        // of their real shape) even though the image data itself decoded successfully. `png`
        // 0.18 has no "unpack bits, keep the palette" transform (libpng's `PACKING` isn't
        // implemented here), so unpack sub-8-bit rows by hand instead, to keep every real
        // asset on the one indexed-surface-with-a-real-SDL-palette path below, matching what
        // real `IMG_Load` does for indexed PNGs regardless of bit depth.
        if reader.info().color_type == png::ColorType::Indexed {
            let bit_depth = reader.info().bit_depth as u8; // BitDepth's repr is its own bit count
            let palette = reader.info().palette.clone().unwrap_or_default();
            let trns = reader.info().trns.clone();
            let Some(buf_size) = reader.output_buffer_size() else { return std::ptr::null_mut() };
            let mut buf = vec![0u8; buf_size];
            let Ok(frame) = reader.next_frame(&mut buf) else { return std::ptr::null_mut() };
            let packed = &buf[..frame.buffer_size()];
            let indices = unpack_indexed_scanlines(packed, width as usize, height as usize, bit_depth);

            let surf = self.create_surface(width, height, 8, 0, 0, 0, 0);
            let s = surf_mut(surf);
            s.pixels[..indices.len()].copy_from_slice(&indices);
            let colors: Vec<SDL_Color> = (0..256)
                .map(|i| {
                    let (r, g, b) = if i * 3 + 2 < palette.len() {
                        (palette[i * 3], palette[i * 3 + 1], palette[i * 3 + 2])
                    } else {
                        (0, 0, 0)
                    };
                    let a = trns.as_ref().and_then(|t| t.get(i)).copied().unwrap_or(255);
                    SDL_Color { r, g, b, a }
                })
                .collect();
            self.set_palette(surf, colors.as_ptr(), 0, 256);
            return surf;
        }

        // Anything else (RGB/RGBA/Grayscale/GrayscaleAlpha) -- no real asset in this codebase
        // is one of these (see above), but keep a generic fallback for robustness. Only 8-bit
        // depth is handled; sub-8-bit non-indexed PNGs would need the same manual-unpack
        // treatment as above and aren't worth building until something actually needs it.
        if reader.info().bit_depth != png::BitDepth::Eight {
            return std::ptr::null_mut();
        }
        let Some(buf_size) = reader.output_buffer_size() else { return std::ptr::null_mut() };
        let mut buf = vec![0u8; buf_size];
        let Ok(frame) = reader.next_frame(&mut buf) else { return std::ptr::null_mut() };
        let pixels = &buf[..frame.buffer_size()];
        let channels = frame.color_type.samples();

        let (rmask, gmask, bmask, amask): (u32, u32, u32, u32) =
            (0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000);
        let surf = self.create_surface(width, height, 32, rmask, gmask, bmask, amask);
        let s = surf_mut(surf);
        for i in 0..(width as usize * height as usize) {
            let (r, g, b, a) = match channels {
                4 => (pixels[i * 4], pixels[i * 4 + 1], pixels[i * 4 + 2], pixels[i * 4 + 3]),
                3 => (pixels[i * 3], pixels[i * 3 + 1], pixels[i * 3 + 2], 255),
                2 => (pixels[i * 2], pixels[i * 2], pixels[i * 2], pixels[i * 2 + 1]),
                1 => (pixels[i], pixels[i], pixels[i], 255),
                _ => (0, 0, 0, 255),
            };
            let packed = pack_pixel(rmask, gmask, bmask, amask, r, g, b, a);
            s.pixels[i * 4..i * 4 + 4].copy_from_slice(&packed.to_ne_bytes());
        }
        surf
    }

    unsafe fn convert_to(&mut self, src: *mut SDL_Surface, depth: c_int, rmask: u32, gmask: u32, bmask: u32, amask: u32) -> *mut SDL_Surface {
        let (w, h) = self.surface_size(src);
        let dst = self.create_surface(w, h, depth, rmask, gmask, bmask, amask);
        let full_rect = SDL_Rect { x: 0, y: 0, w, h };
        let mut dst_rect = full_rect;
        self.blit_impl(src, &full_rect, dst, &mut dst_rect, false);
        dst
    }

    /// Shared `blit`/`blit_scaled` implementation. Honors color-key transparency always;
    /// for the common case (same bpp, no source palette, `SDL_BLENDMODE_NONE`) does the
    /// original plain byte copy, verified correct by every differential-harness replay to
    /// date (which exercises game *state*, not rendered pixels, but this fast path is
    /// unchanged from before this comment, so nothing here is newly at risk). Two real gaps
    /// were added to close, both found empirically via a real browser run rendering actual
    /// text (`method_3_blit_mono` in `seg009.rs`, which `convert_surface_format`s an indexed
    /// glyph to ARGB8888 and then blits it with `SDL_BLENDMODE_BLEND`):
    /// - Converting an indexed (paletted) source into a different-bpp destination now does a
    ///   real palette lookup (color *and* alpha, so `tRNS`-derived per-index transparency
    ///   survives the conversion) instead of truncating to a raw, meaningless byte copy.
    /// - `SDL_BLENDMODE_BLEND` now actually alpha-composites onto the destination's existing
    ///   pixel, instead of being silently ignored (previously: still a raw copy regardless of
    ///   blend mode, which is why a colored-but-still-per-pixel-transparent glyph surface
    ///   rendered as one solid opaque rectangle -- the "should be transparent here" alpha
    ///   value was simply never consulted).
    /// `ADD`/`MOD` compositing (`lighting.rs`'s overlay) is still not implemented -- nothing
    /// exercises it yet.
    unsafe fn blit_impl(&mut self, src: *mut SDL_Surface, src_rect: *const SDL_Rect, dst: *mut SDL_Surface, dst_rect: *mut SDL_Rect, scaled: bool) -> c_int {
        let s = surf_mut(src);
        let (sx0, sy0, sw, sh) = match src_rect.as_ref() {
            Some(r) => (r.x, r.y, r.w, r.h),
            None => (0, 0, s.w, s.h),
        };
        let (s_pitch, s_bpp, s_ckey, s_blend, s_alpha_mod) =
            (s.pitch, s.bytes_per_pixel as usize, s.color_key, s.blend_mode, s.alpha_mod);
        let (s_rmask, s_gmask, s_bmask, s_amask) = (s.rmask, s.gmask, s.bmask, s.amask);
        let (s_rshift, s_gshift, s_bshift, s_ashift) = (s.rshift, s.gshift, s.bshift, s.ashift);
        // Snapshot palette colors up front: avoids holding a live borrow of `s` at the same
        // time as `d` below (both come from the same `HashMap`, via the same `surf_mut`
        // aliasing this function already relied on before this change).
        let s_palette: Option<[SDL_Color; 256]> = s.palette.as_ref().map(|p| *p.colors);

        let d = surf_mut(dst);
        let (dx0, dy0) = match dst_rect.as_ref() {
            Some(r) => (r.x, r.y),
            None => (0, 0),
        };
        let (dw, dh) = if scaled {
            match dst_rect.as_ref() {
                Some(r) => (r.w, r.h),
                None => (sw, sh),
            }
        } else {
            (sw, sh)
        };
        let (d_pitch, d_bpp) = (d.pitch, d.bytes_per_pixel as usize);
        let (d_rmask, d_gmask, d_bmask, d_amask) = (d.rmask, d.gmask, d.bmask, d.amask);
        let (d_rshift, d_gshift, d_bshift, d_ashift) = (d.rshift, d.gshift, d.bshift, d.ashift);
        let d_is_indexed = d.palette.is_some();
        // SDL_UpperBlit/SDL_BlitSurface clip the destination write to the destination
        // surface's own clip rect (set_clip_rect/SDL_SetClipRect), not just its bounds --
        // seg008.rs's draw_mid relies on exactly this to hide the part of a character
        // sprite that shouldn't be visible yet (obj_clip_top/bottom/left/right, computed
        // per-frame by clip_char). Defaulting to the full surface when unset matches "no
        // clip rect installed" == "clip to the whole surface," same as real SDL.
        let (clip_x0, clip_y0, clip_x1, clip_y1) = match d.clip_rect {
            Some(r) => (r.x, r.y, r.x + r.w, r.y + r.h),
            None => (0, 0, d.w, d.h),
        };

        // Fast path only when no per-pixel reinterpretation is needed at all: same byte
        // width, no palette-to-truecolor resolution needed (indexed source AND indexed dest
        // is still a same-format raw index copy, not a conversion), no blending to perform.
        // This is the exact prior behavior, for the exact cases it was already used for.
        let needs_conversion =
            (s_palette.is_some() && !d_is_indexed) || s_bpp != d_bpp || s_blend == SDL_BLENDMODE_BLEND;

        for y in 0..dh {
            let sy = sy0 + if scaled && dh > 0 { y * sh / dh } else { y };
            for x in 0..dw {
                let sx = sx0 + if scaled && dw > 0 { x * sw / dw } else { x };
                let dyy = dy0 + y;
                let dxx = dx0 + x;
                if dyy < clip_y0 || dyy >= clip_y1 || dxx < clip_x0 || dxx >= clip_x1 { continue; }
                if sy < 0 || sy >= s.h || sx < 0 || sx >= s.w { continue; }

                let s_off = (sy * s_pitch) as usize + (sx as usize) * s_bpp;
                let pixel = &s.pixels[s_off..s_off + s_bpp];

                if let Some(key) = s_ckey {
                    let key_bytes = key.to_ne_bytes();
                    if pixel == &key_bytes[..s_bpp] { continue; }
                }

                let d_off = (dyy * d_pitch) as usize + (dxx as usize) * d_bpp;

                if !needs_conversion {
                    let n = s_bpp.min(d_bpp);
                    d.pixels[d_off..d_off + n].copy_from_slice(&pixel[..n]);
                    continue;
                }

                let (mut r, mut g, mut b, mut a) = if let Some(colors) = &s_palette {
                    let c = colors[pixel[0] as usize];
                    (c.r, c.g, c.b, c.a)
                } else {
                    let raw = read_native_u32(pixel);
                    let channel = |mask: u32, shift: u32, default: u8| {
                        if mask != 0 { ((raw >> shift) & 0xFF) as u8 } else { default }
                    };
                    (
                        channel(s_rmask, s_rshift, 0),
                        channel(s_gmask, s_gshift, 0),
                        channel(s_bmask, s_bshift, 0),
                        channel(s_amask, s_ashift, 255),
                    )
                };
                a = ((a as u32 * s_alpha_mod as u32) / 255) as u8;

                if s_blend == SDL_BLENDMODE_BLEND {
                    let draw = read_native_u32(&d.pixels[d_off..d_off + d_bpp]);
                    let dchannel = |mask: u32, shift: u32| {
                        if mask != 0 { ((draw >> shift) & 0xFF) as u8 } else { 0 }
                    };
                    let (dr, dg, db) = (dchannel(d_rmask, d_rshift), dchannel(d_gmask, d_gshift), dchannel(d_bmask, d_bshift));
                    let da = if d_amask != 0 { ((draw >> d_ashift) & 0xFF) as u8 } else { 255 };
                    let af = a as u32;
                    let over = |sc: u8, dc: u8| (((sc as u32 * af) + (dc as u32 * (255 - af))) / 255) as u8;
                    r = over(r, dr);
                    g = over(g, dg);
                    b = over(b, db);
                    a = (af + (da as u32 * (255 - af)) / 255).min(255) as u8;
                }

                let packed = pack_pixel(d_rmask, d_gmask, d_bmask, d_amask, r, g, b, a);
                d.pixels[d_off..d_off + d_bpp].copy_from_slice(&packed.to_ne_bytes()[..d_bpp]);
            }
        }
        0
    }
}

/// Zero-extends a short (1-4 byte) native-endian pixel value into a `u32`, so mask/shift
/// arithmetic (written for a full 32-bit pixel) works uniformly regardless of a surface's
/// actual bytes-per-pixel.
fn read_native_u32(bytes: &[u8]) -> u32 {
    let mut buf = [0u8; 4];
    buf[..bytes.len()].copy_from_slice(bytes);
    u32::from_ne_bytes(buf)
}

pub struct WasmAudio;

#[cfg(target_arch = "wasm32")]
static mut SHARED_AUDIO: crate::platform::backend::ActiveAudio = WasmAudio;
#[cfg(not(target_arch = "wasm32"))]
static mut SHARED_AUDIO: WasmAudio = WasmAudio;

#[cfg(target_arch = "wasm32")]
#[allow(static_mut_refs)]
pub fn shared_audio() -> &'static mut crate::platform::backend::ActiveAudio {
    unsafe { &mut SHARED_AUDIO }
}
#[cfg(not(target_arch = "wasm32"))]
#[allow(static_mut_refs)]
pub fn shared_audio() -> &'static mut WasmAudio {
    unsafe { &mut SHARED_AUDIO }
}

impl AudioBackend for WasmAudio {
    /// Not the real open path -- see `WasmRenderer::open_audio_raw`, which every actual
    /// caller in this codebase uses instead (confirmed by grep: nothing calls
    /// `AudioBackend::open`, only `.lock()`/`.unlock()`/`.pause()`, the same "trait exists,
    /// real call sites use something else" shape Phase C found for `FileSystem`). Kept
    /// unimplemented rather than faked, so a real future caller fails loudly instead of
    /// silently doing nothing.
    fn open(&mut self, _sample_rate: c_int, _channels: u8, _fill: Box<dyn FnMut(&mut [i16]) + Send>) -> Result<(), String> {
        unimplemented!("WasmAudio::open -- real call sites use WasmRenderer::open_audio_raw instead")
    }
    fn pause(&mut self, paused: bool) {
        unsafe { AUDIO_PAUSED = paused };
    }
    /// Single-threaded (wasm32 in this build has no real second thread pulling the audio
    /// callback concurrently -- `pump_audio` calls it synchronously from the game's own
    /// thread), so there's nothing to actually lock against. Real `SDL_LockAudio` exists to
    /// keep the realtime audio thread from calling back into the game mid-mutation; that
    /// race can't happen here.
    fn lock(&mut self) {}
    fn unlock(&mut self) {}
}

pub struct WasmInput;

#[cfg(target_arch = "wasm32")]
static mut SHARED_INPUT: crate::platform::backend::ActiveInput = WasmInput;
#[cfg(not(target_arch = "wasm32"))]
static mut SHARED_INPUT: WasmInput = WasmInput;

#[cfg(target_arch = "wasm32")]
#[allow(static_mut_refs)]
pub fn shared_input() -> &'static mut crate::platform::backend::ActiveInput {
    unsafe { &mut SHARED_INPUT }
}
#[cfg(not(target_arch = "wasm32"))]
#[allow(static_mut_refs)]
pub fn shared_input() -> &'static mut WasmInput {
    unsafe { &mut SHARED_INPUT }
}

// Real key/mouse state, updated by whatever relays browser input events into the wasm
// module (message-passing from the main thread, in the eventual Worker design -- not built
// yet). Module-level statics rather than WasmInput fields, matching this file's existing
// pattern (WasmInput itself stays a unit struct so `SHARED_INPUT`'s static initializer
// stays trivial).
fn key_states() -> &'static mut [bool; 512] {
    static mut KEY_STATES: [bool; 512] = [false; 512];
    #[allow(static_mut_refs)]
    unsafe { &mut KEY_STATES }
}

struct MouseState {
    x: c_int,
    y: c_int,
    left: bool,
    right: bool,
}

fn mouse_state_mut() -> &'static mut MouseState {
    static mut MOUSE: MouseState = MouseState { x: 0, y: 0, left: false, right: false };
    #[allow(static_mut_refs)]
    unsafe { &mut MOUSE }
}

/// JS-facing: report a key transition (SDL scancode + pressed/released), called from
/// whatever relays browser `keydown`/`keyup` events into the wasm module.
pub fn set_key_state(scancode: u32, pressed: bool) {
    if let Some(slot) = key_states().get_mut(scancode as usize) {
        *slot = pressed;
    }
}

/// JS-facing: report the latest mouse position/button state.
pub fn set_mouse_state(x: c_int, y: c_int, left: bool, right: bool) {
    let m = mouse_state_mut();
    m.x = x;
    m.y = y;
    m.left = left;
    m.right = right;
}

// ============================================================================
// Live input, via a `SharedArrayBuffer` the main thread writes into directly.
//
// `set_key_state`/`set_mouse_state` above assume a message-passing relay (main thread
// -> Worker `postMessage`) that was never built, because it can't work: a Worker's
// `onmessage` handler cannot run while `pop_main()`'s blocking loop still occupies the call
// stack (see the plan doc's Phase B `SharedArrayBuffer` note). The actual mechanism is a
// plain (not wasm's own linear memory -- that would need real wasm32 threads support, a much
// bigger lift) `SharedArrayBuffer` the main thread writes keyboard/mouse state into with
// ordinary (non-atomic) typed-array writes -- a torn read of a single already-atomic byte
// isn't a real correctness concern for polled input tolerant of at-most-one-frame staleness,
// so plain reads/writes are fine and simpler than `Atomics.load`/`store` per field.
//
// Buffer layout (521 bytes total), written by `web/index.html`, read by `sync_shared_input`:
//   [0..512)   one byte per SDL scancode, 1 = held, 0 = released
//   [512..516) mouse x, i32 little-endian
//   [516..520) mouse y, i32 little-endian
//   [520]      mouse buttons bitmask: bit0 = left, bit1 = right
//
// The user has explicitly said they'd like to move off `SharedArrayBuffer` eventually (see
// the plan doc) in favor of the `advance_one_frame()` redesign, where a non-blocking per-tick
// loop would let plain `postMessage` handle input with no shared memory at all -- this is a
// deliberate first-pass choice, not the intended long-term shape.
// ============================================================================

#[cfg(target_arch = "wasm32")]
static mut SHARED_INPUT_BUFFER: Option<js_sys::Uint8Array> = None;

/// JS-facing: hand the wasm module the `SharedArrayBuffer` the main thread will write
/// keyboard/mouse state into. Called once, before `run_game()`, from `worker.js` (which
/// waits for an `'init'` message carrying it -- the one message a Worker's `onmessage` *can*
/// receive, since it arrives before `pop_main()` starts blocking).
#[cfg(target_arch = "wasm32")]
#[allow(static_mut_refs)]
pub fn set_shared_input_buffer(buf: js_sys::SharedArrayBuffer) {
    unsafe { SHARED_INPUT_BUFFER = Some(js_sys::Uint8Array::new(&buf)) };
}
#[cfg(not(target_arch = "wasm32"))]
pub fn set_shared_input_buffer(_buf: ()) {}

/// Copies the shared buffer's current contents into `key_states()`/`mouse_state_mut()` --
/// the existing `InputSource::key_state`/`mouse_state` consumers, and `poll_event`'s edge
/// detector below, don't need to know this replaces a `postMessage` relay that was never
/// wired up. A no-op if no buffer has been set (native/test builds, or a wasm32 run before
/// `set_shared_input_buffer` -- e.g. the headless `run_game()` probe, which has no JS side to
/// supply one).
#[allow(static_mut_refs)]
unsafe fn sync_shared_input() {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(buf) = SHARED_INPUT_BUFFER.as_ref() else { return };
        let mut raw = [0u8; 521];
        buf.copy_to(&mut raw);
        let states = key_states();
        for i in 0..512 {
            states[i] = raw[i] != 0;
        }
        let m = mouse_state_mut();
        m.x = i32::from_le_bytes([raw[512], raw[513], raw[514], raw[515]]);
        m.y = i32::from_le_bytes([raw[516], raw[517], raw[518], raw[519]]);
        m.left = raw[520] & 1 != 0;
        m.right = raw[520] & 2 != 0;
    }
}

/// One `SDL_Event`'s worth of raw bytes -- real SDL2's `SDL_Event` union is 56 bytes on a
/// 64-bit target, matching `seg009.rs`'s local (private, so duplicated here by value rather
/// than referenced) definition. `push_event`/`poll_event` only ever see this many bytes
/// copied in or out, so treating events as opaque `[u8; 56]` blobs (rather than needing
/// `seg009.rs`'s actual `SDL_Event` type here) is exact, not an approximation.
const SDL_EVENT_SIZE: usize = 56;

fn event_queue() -> &'static mut std::collections::VecDeque<[u8; SDL_EVENT_SIZE]> {
    static mut QUEUE: Option<std::collections::VecDeque<[u8; SDL_EVENT_SIZE]>> = None;
    unsafe {
        #[allow(static_mut_refs)]
        QUEUE.get_or_insert_with(std::collections::VecDeque::new)
    }
}

/// Diffs the just-synced `key_states()` against the previous poll's snapshot and queues a
/// real `SDL_KEYDOWN`/`SDL_KEYUP` event (byte-for-byte matching `seg009.rs`'s private
/// `SDL_KeyboardEvent`/`SDL_Keysym` layout) for every scancode that changed -- turning level
/// state into the edge-triggered events `process_events` (`seg009.rs`) actually expects, the
/// same way real SDL turns raw HID reports into a queue of discrete events.
#[allow(static_mut_refs)]
fn synthesize_key_edge_events() {
    static mut PREV: [bool; 512] = [false; 512];
    const SDL_KEYDOWN: u32 = 0x300;
    const SDL_KEYUP: u32 = 0x301;
    // seg009.rs's SDL_SCANCODE_L{CTRL,SHIFT,ALT}/R{CTRL,SHIFT,ALT} constants.
    const LCTRL: usize = 224;
    const LSHIFT: usize = 225;
    const LALT: usize = 226;
    const RCTRL: usize = 228;
    const RSHIFT: usize = 229;
    const RALT: usize = 230;
    unsafe {
        let states = *key_states(); // snapshot -- key_states() is also read per-index below
        let mut mod_bits: u16 = 0;
        if states[LSHIFT] { mod_bits |= 0x0001; }
        if states[RSHIFT] { mod_bits |= 0x0002; }
        if states[LCTRL] { mod_bits |= 0x0040; }
        if states[RCTRL] { mod_bits |= 0x0080; }
        if states[LALT] { mod_bits |= 0x0100; }
        if states[RALT] { mod_bits |= 0x0200; }
        for sc in 0..512usize {
            if states[sc] == PREV[sc] {
                continue;
            }
            PREV[sc] = states[sc];
            let mut bytes = [0u8; SDL_EVENT_SIZE];
            let type_ = if states[sc] { SDL_KEYDOWN } else { SDL_KEYUP };
            bytes[0..4].copy_from_slice(&type_.to_ne_bytes());
            // timestamp (4..8) and windowID (8..12) stay 0 -- nothing reads them.
            bytes[12] = states[sc] as u8; // SDL_KeyboardEvent::state
            // repeat/padding2/padding3 (13..16) stay 0 -- real key-repeat isn't modeled;
            // process_events only branches on KEYDOWN vs KEYUP, not the repeat flag.
            bytes[16..20].copy_from_slice(&(sc as u32).to_ne_bytes()); // keysym.scancode
            // keysym.sym (20..24) stays 0 -- process_events reads scancode/mod, not sym.
            bytes[24..26].copy_from_slice(&mod_bits.to_ne_bytes()); // keysym.mod
            event_queue().push_back(bytes);
        }
    }
}

/// Same edge-triggering idea as `synthesize_key_edge_events`, for mouse buttons.
/// `process_events` (`seg009.rs`) only ever matches on `SDL_MOUSEBUTTONDOWN` (never
/// `SDL_MOUSEBUTTONUP`/`SDL_MOUSEMOTION` -- confirmed by grep; mouse *position* is polled
/// separately via `InputSource::mouse_state`/`read_mouse_state`, not delivered as motion
/// events), so this only needs to queue a down event on the false->true edge, matching a real
/// "click" rather than "held" semantics -- there is no corresponding up-edge event to
/// synthesize. Before this, `web/index.html` already wrote button state into the shared
/// buffer (mousedown/mouseup listeners, present since the input-buffer format was designed),
/// and `sync_shared_input` already copied it into `mouse_state_mut()`, but nothing ever
/// turned that level state into the discrete event `menu.rs`'s `mouse_clicked`/
/// `mouse_button_clicked_right` need -- so mouse position/hover worked in the browser build
/// but clicks never registered, until this fix.
#[allow(static_mut_refs)]
fn synthesize_mouse_edge_events() {
    static mut PREV_LEFT: bool = false;
    static mut PREV_RIGHT: bool = false;
    const SDL_MOUSEBUTTONDOWN: u32 = 0x401;
    const SDL_BUTTON_LEFT: u8 = 1;
    const SDL_BUTTON_RIGHT: u8 = 3;
    unsafe {
        let m = mouse_state_mut();
        let (x, y, left, right) = (m.x, m.y, m.left, m.right);
        let queue_click = |button: u8| {
            let mut bytes = [0u8; SDL_EVENT_SIZE];
            bytes[0..4].copy_from_slice(&SDL_MOUSEBUTTONDOWN.to_ne_bytes());
            // timestamp (4..8), windowID (8..12), which (12..16) stay 0 -- unread.
            bytes[16] = button;
            bytes[17] = 1; // SDL_PRESSED -- unread by process_events, set for fidelity.
            // clicks (18), padding1 (19) stay 0.
            bytes[20..24].copy_from_slice(&x.to_ne_bytes());
            bytes[24..28].copy_from_slice(&y.to_ne_bytes());
            event_queue().push_back(bytes);
        };
        if left && !PREV_LEFT {
            queue_click(SDL_BUTTON_LEFT);
        }
        if right && !PREV_RIGHT {
            queue_click(SDL_BUTTON_RIGHT);
        }
        PREV_LEFT = left;
        PREV_RIGHT = right;
    }
}

impl WasmInput {
    /// Mirrors `SdlInput::init` -- an inherent method (not part of `InputSource`) that
    /// `seg009.rs`'s startup path calls directly on `shared_input()`. Nothing to actually
    /// initialize here: `key_states()`/`mouse_state_mut()` are already zeroed statics, and
    /// there's no real event pump/video/timer subsystem to construct (unlike native).
    pub fn init(&mut self) -> Result<(), String> {
        Ok(())
    }
}

impl InputSource for WasmInput {
    fn key_state(&self, scancode: c_int) -> bool {
        key_states().get(scancode as usize).copied().unwrap_or(false)
    }
    fn mouse_state(&self) -> (c_int, c_int, bool, bool) {
        let m = mouse_state_mut();
        (m.x, m.y, m.left, m.right)
    }
    fn start_text_input(&mut self, _x: c_int, _y: c_int, _w: c_int, _h: c_int) {
        // IME/on-screen-keyboard hinting has no equivalent worth implementing yet --
        // physical-keyboard input already works via set_key_state.
    }
    fn stop_text_input(&mut self) {}
    fn add_one_shot_timer(&mut self, _delay_ms: u32, _callback: Box<dyn FnOnce() + Send>) -> bool {
        // Only real caller is the level-skip Shift-key debounce (a minor UX nicety, not
        // needed to boot a frame). Report "timer not created" rather than silently losing
        // the callback -- the one caller already handles that by not debouncing.
        false
    }
    fn rumble(&mut self, _strength: f32, _duration_ms: u32) {
        // No controller haptics in a browser tab (yet); silently doing nothing matches
        // what real SDL does on a controller with no rumble support.
    }
}

pub struct WasmFiles;

impl FileSystem for WasmFiles {
    fn read_file(&self, _path: &str) -> Result<Vec<u8>, String> {
        unimplemented!("WasmFiles::read_file")
    }
    fn write_file(&self, _path: &str, _data: &[u8]) -> Result<(), String> {
        unimplemented!("WasmFiles::write_file")
    }
    fn file_exists(&self, _path: &str) -> bool {
        unimplemented!("WasmFiles::file_exists")
    }
}

// Regression tests for the Phase D menu/keyboard reachability audit (2026-08-08, memory
// project_wasm_esc_menu_crash / docs/plans/13-platform-architecture-unification.md): both
// render_set_integer_scale (pause menu "Use integer scaling" toggle, both directions) and
// save_png (F12/Shift+F12 screenshot keys) were unimplemented!() stubs that would have
// panicked the first time a live user hit them, same class of bug as the original Esc
// crash. Unlike that fix, these two don't need the browser-only scripted-input harness --
// they're pure trait-method calls with no menu-navigation/event-loop involvement, so a
// plain #[test] against WasmRenderer directly (compiled on native under `cfg(test)`, see
// platform/mod.rs) is a faster, equally real regression check.
#[cfg(test)]
mod tests {
    use super::*;

    // Regression test for a real coordinate-mapping bug found live-testing the mouse-click
    // fix below (2026-08-08): render_get_scale used to hardcode (1.0, 1.0) on the theory that
    // nothing here calls SDL_RenderSetScale directly, missing that real SDL_RenderGetScale
    // also reflects SDL_RenderSetLogicalSize (which this codebase always calls at startup,
    // apply_aspect_ratio in seg009.rs). With window output 640x400 and the default 320x200
    // logical size, real SDL reports scale (2.0, 2.0); the wasm shim's stale (1.0, 1.0)
    // caused every menu mouse click to be computed against the wrong logical position --
    // confirmed live: clicking a visually correct row landed 2 rows off. Saves/restores the
    // module statics since they're shared mutable state other tests could also touch.
    #[test]
    fn render_get_scale_reflects_window_to_logical_ratio() {
        unsafe {
            let (saved_window, saved_logical) = (WINDOW_SIZE, LOGICAL_SIZE);
            WINDOW_SIZE = (640, 400);
            LOGICAL_SIZE = (320, 200);
            assert_eq!(shared_renderer().render_get_scale(std::ptr::null_mut()), (2.0, 2.0));
            LOGICAL_SIZE = (1600, 1200); // "correct aspect ratio" mode
            assert_eq!(shared_renderer().render_get_scale(std::ptr::null_mut()), (0.4, 1.0 / 3.0));
            LOGICAL_SIZE = (0, 0); // guards a div-by-zero rather than a real state
            assert_eq!(shared_renderer().render_get_scale(std::ptr::null_mut()), (1.0, 1.0));
            WINDOW_SIZE = saved_window;
            LOGICAL_SIZE = saved_logical;
        }
    }

    #[test]
    fn render_set_integer_scale_does_not_panic_either_direction() {
        let r = shared_renderer();
        unsafe {
            assert_eq!(r.render_set_integer_scale(std::ptr::null_mut(), true), 0);
            assert_eq!(r.render_set_integer_scale(std::ptr::null_mut(), false), 0);
        }
    }

    #[test]
    fn save_png_fails_gracefully_instead_of_panicking() {
        let r = shared_renderer();
        unsafe {
            let path = std::ffi::CString::new("screenshot.png").unwrap();
            assert_eq!(r.save_png(std::ptr::null_mut(), &path), -1);
        }
    }

    // Regression test for the mouse-click gap found during the Phase D reachability audit's
    // mouse-input followup (2026-08-08): mouse *position* already worked in the browser build
    // (web/index.html wrote it into the shared buffer, sync_shared_input copied it into
    // mouse_state_mut, menu.rs's read_mouse_state/is_mouse_over_rect consumed it), but nothing
    // ever turned a button going down into the SDL_MOUSEBUTTONDOWN event menu.rs's
    // mouse_clicked/mouse_button_clicked_right actually key off -- so hover-highlight worked
    // and clicking silently did nothing. Drives mouse_state_mut() directly (bypassing
    // sync_shared_input, which is only real on wasm32) since this only needs to verify
    // poll_event's edge detection, not the shared-buffer transport.
    #[test]
    fn mouse_click_synthesizes_one_down_event_per_press() {
        const SDL_MOUSEBUTTONDOWN: u32 = 0x401;
        const SDL_BUTTON_LEFT: u8 = 1;
        const SDL_BUTTON_RIGHT: u8 = 3;
        fn poll() -> Option<[u8; SDL_EVENT_SIZE]> {
            let mut bytes = [0u8; SDL_EVENT_SIZE];
            let n = unsafe {
                shared_renderer().poll_event(bytes.as_mut_ptr() as *mut std::os::raw::c_void)
            };
            (n != 0).then_some(bytes)
        }
        let m = mouse_state_mut();
        m.x = 0;
        m.y = 0;
        m.left = false;
        m.right = false;
        // Drain any event left over from a previous test's state transition (e.g. releasing
        // a button held at the end of another test) before asserting on fresh edges.
        while poll().is_some() {}

        m.x = 42;
        m.y = 7;
        m.left = true;
        let ev = poll().expect("left press must synthesize a down event");
        assert_eq!(u32::from_ne_bytes(ev[0..4].try_into().unwrap()), SDL_MOUSEBUTTONDOWN);
        assert_eq!(ev[16], SDL_BUTTON_LEFT);
        assert_eq!(i32::from_ne_bytes(ev[20..24].try_into().unwrap()), 42);
        assert_eq!(i32::from_ne_bytes(ev[24..28].try_into().unwrap()), 7);
        assert!(poll().is_none(), "holding the button must not re-fire");

        let m = mouse_state_mut();
        m.left = false;
        assert!(poll().is_none(), "release must not synthesize its own event");
        m.right = true;
        let ev = poll().expect("right press must synthesize a down event");
        assert_eq!(ev[16], SDL_BUTTON_RIGHT);

        let m = mouse_state_mut();
        m.left = false;
        m.right = false;
    }
}
