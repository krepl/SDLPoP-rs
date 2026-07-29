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
        unimplemented!("WasmRenderer::save_png")
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
        unimplemented!("WasmRenderer::set_fullscreen")
    }
    unsafe fn show_cursor(&mut self, _show: bool) {
        unimplemented!("WasmRenderer::show_cursor")
    }
    unsafe fn delay(&mut self, _ms: u32) {
        unimplemented!("WasmRenderer::delay")
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
        unimplemented!("WasmRenderer::linked_sdl_version")
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
    unsafe fn get_scancode_name(&mut self, _scancode: u32) -> *const std::os::raw::c_char {
        unimplemented!("WasmRenderer::get_scancode_name")
    }
    unsafe fn get_window_flags(&mut self, _window: *mut crate::SDL_Window) -> u32 {
        unimplemented!("WasmRenderer::get_window_flags")
    }
    unsafe fn render_get_scale(&mut self, _renderer: *mut crate::SDL_Renderer) -> (f32, f32) {
        unimplemented!("WasmRenderer::render_get_scale")
    }
    unsafe fn render_get_logical_size(&mut self, _renderer: *mut crate::SDL_Renderer) -> (c_int, c_int) {
        unimplemented!("WasmRenderer::render_get_logical_size")
    }
    unsafe fn render_get_viewport(&mut self, _renderer: *mut crate::SDL_Renderer) -> SDL_Rect {
        unimplemented!("WasmRenderer::render_get_viewport")
    }
    unsafe fn render_set_integer_scale(&mut self, _renderer: *mut crate::SDL_Renderer, _enable: bool) -> c_int {
        unimplemented!("WasmRenderer::render_set_integer_scale")
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
        let s = screen_buffer();
        PRESENTED_FRAME = Some((s.w, s.h, s.bytes_per_pixel, s.pixels.clone()));
    }
    unsafe fn render_set_logical_size(&mut self, _renderer: *mut crate::SDL_Renderer, _w: c_int, _h: c_int) -> c_int {
        // Canvas scaling is a JS/CSS concern in the real browser build, not something this
        // layer manages -- report success and defer actual scaling to that layer.
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
    unsafe fn open_audio_raw(&mut self, _desired: *mut std::os::raw::c_void, _obtained: *mut std::os::raw::c_void) -> c_int {
        unimplemented!("WasmRenderer::open_audio_raw")
    }
    unsafe fn num_joysticks(&mut self) -> c_int {
        unimplemented!("WasmRenderer::num_joysticks")
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
    unsafe fn push_event(&mut self, _event: *mut std::os::raw::c_void) -> c_int {
        unimplemented!("WasmRenderer::push_event")
    }
    unsafe fn poll_event(&mut self, _event: *mut std::os::raw::c_void) -> c_int {
        unimplemented!("WasmRenderer::poll_event")
    }
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

        // Bit depths other than 8 (1/2/4/16-bit PNGs) would need extra unpacking this
        // doesn't implement -- every real asset in this codebase is 8-bit; fail cleanly
        // (matching IMG_Load's null-on-failure convention) rather than read garbage.
        if reader.info().bit_depth != png::BitDepth::Eight {
            return std::ptr::null_mut();
        }

        if reader.info().color_type == png::ColorType::Indexed {
            let palette = reader.info().palette.clone().unwrap_or_default();
            let trns = reader.info().trns.clone();
            let Some(buf_size) = reader.output_buffer_size() else { return std::ptr::null_mut() };
            let mut buf = vec![0u8; buf_size];
            let Ok(frame) = reader.next_frame(&mut buf) else { return std::ptr::null_mut() };
            let indices = &buf[..frame.buffer_size()];

            let surf = self.create_surface(width, height, 8, 0, 0, 0, 0);
            let s = surf_mut(surf);
            s.pixels[..indices.len()].copy_from_slice(indices);
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

        // Anything else (RGB/RGBA/Grayscale/GrayscaleAlpha) -- the sample-count-based match
        // below handles each natively-decoded shape without needing an EXPAND
        // transformation (which would have to be set before `read_info`, before the
        // color-type check above -- easier to just handle each real shape directly).
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

    /// Shared `blit`/`blit_scaled` implementation. Honors color-key transparency; does
    /// **not** yet implement alpha/`ADD`/`MOD` blend-mode compositing (`SDL_BLENDMODE_NONE`
    /// is a plain copy, which covers every current Phase A test scene and file migration to
    /// date) -- add real blend math here once a file migration actually exercises it
    /// (`lighting.rs`'s `ADD`/`MOD` overlay is the known future case).
    unsafe fn blit_impl(&mut self, src: *mut SDL_Surface, src_rect: *const SDL_Rect, dst: *mut SDL_Surface, dst_rect: *mut SDL_Rect, scaled: bool) -> c_int {
        let s = surf_mut(src);
        let (sx0, sy0, sw, sh) = match src_rect.as_ref() {
            Some(r) => (r.x, r.y, r.w, r.h),
            None => (0, 0, s.w, s.h),
        };
        let (s_pitch, s_bpp, s_ckey) = (s.pitch, s.bytes_per_pixel as usize, s.color_key);

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

        for y in 0..dh {
            let sy = sy0 + if scaled && dh > 0 { y * sh / dh } else { y };
            for x in 0..dw {
                let sx = sx0 + if scaled && dw > 0 { x * sw / dw } else { x };
                let dyy = dy0 + y;
                let dxx = dx0 + x;
                if dyy < 0 || dyy >= d.h || dxx < 0 || dxx >= d.w { continue; }
                if sy < 0 || sy >= s.h || sx < 0 || sx >= s.w { continue; }

                let s_off = (sy * s_pitch) as usize + (sx as usize) * s_bpp;
                let pixel = &s.pixels[s_off..s_off + s_bpp];

                if let Some(key) = s_ckey {
                    let key_bytes = key.to_ne_bytes();
                    if pixel == &key_bytes[..s_bpp] { continue; }
                }

                let d_off = (dyy * d_pitch) as usize + (dxx as usize) * d_bpp;
                let n = s_bpp.min(d_bpp);
                d.pixels[d_off..d_off + n].copy_from_slice(&pixel[..n]);
            }
        }
        0
    }
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
    fn open(&mut self, _sample_rate: c_int, _channels: u8, _fill: Box<dyn FnMut(&mut [i16]) + Send>) -> Result<(), String> {
        unimplemented!("WasmAudio::open")
    }
    fn pause(&mut self, _paused: bool) {
        unimplemented!("WasmAudio::pause")
    }
    fn lock(&mut self) {
        unimplemented!("WasmAudio::lock")
    }
    fn unlock(&mut self) {
        unimplemented!("WasmAudio::unlock")
    }
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
