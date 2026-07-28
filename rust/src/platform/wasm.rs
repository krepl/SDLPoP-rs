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

fn shift_for(mask: u32) -> u32 {
    if mask == 0 { 0 } else { mask.trailing_zeros() }
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
        let palette = if depth == 8 { Some(WasmPalette::new()) } else { None };
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
        }
    }

    unsafe fn surface_palette(&mut self, surf: *mut SDL_Surface) -> *mut crate::SDL_Palette {
        match surf_mut(surf).palette.as_mut() {
            Some(p) => p.as_ptr(),
            None => std::ptr::null_mut(),
        }
    }

    unsafe fn load_image_from_memory(&mut self, _bytes: &[u8]) -> *mut SDL_Surface {
        unimplemented!("WasmRenderer::load_image_from_memory")
    }
    unsafe fn load_image_from_file(&mut self, _path: &std::ffi::CStr) -> *mut SDL_Surface {
        unimplemented!("WasmRenderer::load_image_from_file")
    }
    unsafe fn img_load_rw(&mut self, _rw: *mut SDL_RWops, _freesrc: c_int) -> *mut SDL_Surface {
        unimplemented!("WasmRenderer::img_load_rw")
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
    unsafe fn rw_from_mem(&mut self, _buf: *mut std::os::raw::c_void, _size: c_int) -> *mut SDL_RWops {
        unimplemented!("WasmRenderer::rw_from_mem")
    }
    unsafe fn rw_tell(&mut self, _rw: *mut SDL_RWops) -> i64 {
        unimplemented!("WasmRenderer::rw_tell")
    }
    unsafe fn rw_close(&mut self, _rw: *mut SDL_RWops) -> c_int {
        unimplemented!("WasmRenderer::rw_close")
    }
    unsafe fn rw_write(&mut self, _rw: *mut SDL_RWops, _ptr: *const std::os::raw::c_void, _size: usize, _num: usize) -> usize {
        unimplemented!("WasmRenderer::rw_write")
    }
    unsafe fn rw_read(&mut self, _rw: *mut SDL_RWops, _ptr: *mut std::os::raw::c_void, _size: usize, _maxnum: usize) -> usize {
        unimplemented!("WasmRenderer::rw_read")
    }
    unsafe fn show_message_box(&mut self, _title: &std::ffi::CStr, _message: &std::ffi::CStr) {
        unimplemented!("WasmRenderer::show_message_box")
    }
    unsafe fn linked_sdl_version(&mut self) -> (u8, u8, u8) {
        unimplemented!("WasmRenderer::linked_sdl_version")
    }
    unsafe fn performance_counter(&mut self) -> u64 {
        unimplemented!("WasmRenderer::performance_counter")
    }
    unsafe fn performance_frequency(&mut self) -> u64 {
        unimplemented!("WasmRenderer::performance_frequency")
    }
    unsafe fn rw_from_file(&mut self, _path: &std::ffi::CStr, _mode: &std::ffi::CStr) -> *mut SDL_RWops {
        unimplemented!("WasmRenderer::rw_from_file")
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
        unimplemented!("WasmRenderer::set_window_icon")
    }
    unsafe fn rw_from_const_mem(&mut self, _mem: *const std::os::raw::c_void, _size: c_int) -> *mut SDL_RWops {
        unimplemented!("WasmRenderer::rw_from_const_mem")
    }
    unsafe fn create_texture(&mut self, _renderer: *mut crate::SDL_Renderer, _format: u32, _access: c_int, _w: c_int, _h: c_int) -> *mut crate::SDL_Texture {
        unimplemented!("WasmRenderer::create_texture")
    }
    unsafe fn update_texture(&mut self, _texture: *mut crate::SDL_Texture, _rect: *const SDL_Rect, _pixels: *const std::os::raw::c_void, _pitch: c_int) -> c_int {
        unimplemented!("WasmRenderer::update_texture")
    }
    unsafe fn set_render_target(&mut self, _renderer: *mut crate::SDL_Renderer, _texture: *mut crate::SDL_Texture) -> c_int {
        unimplemented!("WasmRenderer::set_render_target")
    }
    unsafe fn render_clear(&mut self, _renderer: *mut crate::SDL_Renderer) -> c_int {
        unimplemented!("WasmRenderer::render_clear")
    }
    unsafe fn render_copy(&mut self, _renderer: *mut crate::SDL_Renderer, _texture: *mut crate::SDL_Texture, _src_rect: *const SDL_Rect, _dst_rect: *const SDL_Rect) -> c_int {
        unimplemented!("WasmRenderer::render_copy")
    }
    unsafe fn render_present(&mut self, _renderer: *mut crate::SDL_Renderer) {
        unimplemented!("WasmRenderer::render_present")
    }
    unsafe fn render_set_logical_size(&mut self, _renderer: *mut crate::SDL_Renderer, _w: c_int, _h: c_int) -> c_int {
        unimplemented!("WasmRenderer::render_set_logical_size")
    }
    unsafe fn get_renderer_output_size(&mut self, _renderer: *mut crate::SDL_Renderer) -> (c_int, c_int) {
        unimplemented!("WasmRenderer::get_renderer_output_size")
    }
    unsafe fn get_renderer_info_flags(&mut self, _renderer: *mut crate::SDL_Renderer) -> u32 {
        unimplemented!("WasmRenderer::get_renderer_info_flags")
    }
    unsafe fn set_hint(&mut self, _name: &std::ffi::CStr, _value: &std::ffi::CStr) -> c_int {
        unimplemented!("WasmRenderer::set_hint")
    }
    unsafe fn sdl_init(&mut self, _flags: u32) -> c_int {
        unimplemented!("WasmRenderer::sdl_init")
    }
    unsafe fn sdl_init_subsystem(&mut self, _flags: u32) -> c_int {
        unimplemented!("WasmRenderer::sdl_init_subsystem")
    }
    unsafe fn sdl_quit(&mut self) {
        unimplemented!("WasmRenderer::sdl_quit")
    }
    unsafe fn create_window(&mut self, _title: &std::ffi::CStr, _x: c_int, _y: c_int, _w: c_int, _h: c_int, _flags: u32) -> *mut crate::SDL_Window {
        unimplemented!("WasmRenderer::create_window")
    }
    unsafe fn create_renderer(&mut self, _window: *mut crate::SDL_Window, _index: c_int, _flags: u32) -> *mut crate::SDL_Renderer {
        unimplemented!("WasmRenderer::create_renderer")
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

impl WasmInput {
    /// Mirrors `SdlInput::init` -- an inherent method (not part of `InputSource`) that
    /// `seg009.rs`'s startup path calls directly on `shared_input()`.
    pub fn init(&mut self) -> Result<(), String> {
        unimplemented!("WasmInput::init")
    }
}

impl InputSource for WasmInput {
    fn key_state(&self, _scancode: c_int) -> bool {
        unimplemented!("WasmInput::key_state")
    }
    fn mouse_state(&self) -> (c_int, c_int, bool, bool) {
        unimplemented!("WasmInput::mouse_state")
    }
    fn start_text_input(&mut self, _x: c_int, _y: c_int, _w: c_int, _h: c_int) {
        unimplemented!("WasmInput::start_text_input")
    }
    fn stop_text_input(&mut self) {
        unimplemented!("WasmInput::stop_text_input")
    }
    fn add_one_shot_timer(&mut self, _delay_ms: u32, _callback: Box<dyn FnOnce() + Send>) -> bool {
        unimplemented!("WasmInput::add_one_shot_timer")
    }
    fn rumble(&mut self, _strength: f32, _duration_ms: u32) {
        unimplemented!("WasmInput::rumble")
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
