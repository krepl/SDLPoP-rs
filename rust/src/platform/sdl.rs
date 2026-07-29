//! The native SDL2 backend. This is the only module in the crate allowed to reference
//! `SDL_*`/`IMG_*` directly (through the `sdl2` crate's safe wrappers, with a narrow raw
//! `sdl2::sys` escape hatch for the handful of operations the crate doesn't expose --
//! `IMG_Load_RW`, notably, since the crate's `LoadSurface` trait only offers
//! `from_file`/`from_xpm_array`, not a load-from-memory-buffer path).
//!
//! Game logic reaches this backend through the `shared_renderer()`/`shared_audio()`/
//! `shared_input()` singletons, not through `SdlPlatform::new()` -- the real startup
//! path (seg009.rs's `set_gr_mode`) drives SDL init via raw FFI calls on those
//! singletons directly. `SdlPlatform` itself is currently only exercised by tests.

use std::os::raw::c_int;

use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::rect::Rect;
use sdl2::render::WindowCanvas;
use sdl2::rwops::RWops;
use sdl2::surface::SurfaceRef;
use sdl2::AudioSubsystem;
use sdl2::EventPump;
use sdl2::TimerSubsystem;

use crate::{SDL_Color, SDL_PixelFormat, SDL_Rect, SDL_Surface};

use super::{AudioBackend, FileSystem, InputSource, Platform, Renderer};

// This crate's own bindgen invocation (build.rs, allowlisted to src/ only) doesn't
// emit SDL_* function declarations -- only the struct types (SDL_Surface/SDL_Rect/
// SDL_Color, referenced from project structs bindgen does allowlist). The actual
// function bodies below call through `sdl2::sys`, which has its own independent
// bindgen run over the full SDL headers. Both runs describe the same C structs, so a
// pointer cast between `crate::SDL_Surface` and `sdl2::sys::SDL_Surface` is sound --
// these pointers are only ever passed opaquely to C, never field-accessed as the
// `crate::` type on the Rust side once cast.
unsafe fn as_sys_surface(surf: *mut SDL_Surface) -> *mut sdl2::sys::SDL_Surface {
    surf as *mut sdl2::sys::SDL_Surface
}
unsafe fn as_sys_rect(rect: *const SDL_Rect) -> *const sdl2::sys::SDL_Rect {
    rect as *const sdl2::sys::SDL_Rect
}

pub struct SdlRenderer {
    canvas: Option<WindowCanvas>,
    _image_context: Option<sdl2::image::Sdl2ImageContext>,
}

// Like `shared_audio()`: create_surface/free_surface/load_image_*/lock_surface/
// unlock_surface/set_*/blit/fill_rect/delay are all free-standing SDL calls that don't
// touch `canvas` -- only `present`/`set_fullscreen`/`show_cursor` need a real window,
// which nothing constructs yet (seg009.rs's own window/canvas init hasn't migrated).
// Those three panic with a clear message if reached before a real `SdlPlatform` exists.
// Typed as the backend-selected alias (see platform::backend), not `SdlRenderer` directly,
// so a second backend only ever needs a new cfg arm there -- this declaration doesn't
// change, and neither does any of the ~224 call sites that call `shared_renderer()`.
static mut SHARED_RENDERER: crate::platform::backend::ActiveRenderer =
    SdlRenderer { canvas: None, _image_context: None };

#[allow(static_mut_refs)]
pub fn shared_renderer() -> &'static mut crate::platform::backend::ActiveRenderer {
    unsafe { &mut SHARED_RENDERER }
}

impl Renderer for SdlRenderer {
    unsafe fn create_surface(&mut self, width: c_int, height: c_int, depth: c_int, rmask: u32, gmask: u32, bmask: u32, amask: u32) -> *mut SDL_Surface {
        sdl2::sys::SDL_CreateRGBSurface(0, width, height, depth, rmask, gmask, bmask, amask) as *mut SDL_Surface
    }

    unsafe fn free_surface(&mut self, surf: *mut SDL_Surface) {
        sdl2::sys::SDL_FreeSurface(as_sys_surface(surf));
    }

    unsafe fn surface_size(&mut self, surf: *mut SDL_Surface) -> (c_int, c_int) {
        ((*surf).w, (*surf).h)
    }

    unsafe fn surface_pitch(&mut self, surf: *mut SDL_Surface) -> c_int {
        (*surf).pitch
    }

    unsafe fn surface_pixels(&mut self, surf: *mut SDL_Surface) -> *mut std::os::raw::c_void {
        (*surf).pixels
    }

    unsafe fn surface_format_info(&mut self, surf: *mut SDL_Surface) -> crate::platform::PixelFormatInfo {
        let format = (*surf).format;
        crate::platform::PixelFormatInfo {
            bits_per_pixel: (*format).BitsPerPixel,
            bytes_per_pixel: (*format).BytesPerPixel,
            rmask: (*format).Rmask,
            gmask: (*format).Gmask,
            bmask: (*format).Bmask,
            amask: (*format).Amask,
            format: (*format).format,
        }
    }

    unsafe fn surface_palette(&mut self, surf: *mut SDL_Surface) -> *mut crate::SDL_Palette {
        (*(*surf).format).palette
    }

    unsafe fn surface_format_ptr(&mut self, surf: *mut SDL_Surface) -> *mut SDL_PixelFormat {
        (*surf).format
    }

    unsafe fn load_image_from_memory(&mut self, bytes: &[u8]) -> *mut SDL_Surface {
        // `sdl2::image::LoadSurface` only exposes `from_file`/`from_xpm_array`; the crate
        // has no safe load-from-memory-buffer wrapper (matches IMG_Load_RW), so this is
        // exactly the narrow raw-sys escape hatch flagged in the Step C plan.
        let Ok(rwops) = RWops::from_bytes(bytes) else { return std::ptr::null_mut() };
        sdl2::sys::image::IMG_Load_RW(rwops.raw(), 0) as *mut SDL_Surface
    }

    unsafe fn load_image_from_file(&mut self, path: &std::ffi::CStr) -> *mut SDL_Surface {
        sdl2::sys::image::IMG_Load(path.as_ptr()) as *mut SDL_Surface
    }

    unsafe fn img_load_rw(&mut self, rw: *mut crate::SDL_RWops, freesrc: c_int) -> *mut SDL_Surface {
        sdl2::sys::image::IMG_Load_RW(rw as *mut sdl2::sys::SDL_RWops, freesrc) as *mut SDL_Surface
    }

    unsafe fn lock_surface(&mut self, surf: *mut SDL_Surface) -> c_int {
        sdl2::sys::SDL_LockSurface(as_sys_surface(surf))
    }

    unsafe fn unlock_surface(&mut self, surf: *mut SDL_Surface) {
        sdl2::sys::SDL_UnlockSurface(as_sys_surface(surf));
    }

    unsafe fn set_color_key(&mut self, surf: *mut SDL_Surface, enable: bool, key: u32) -> c_int {
        sdl2::sys::SDL_SetColorKey(as_sys_surface(surf), enable as c_int, key)
    }

    unsafe fn set_palette(&mut self, surf: *mut SDL_Surface, colors: *const SDL_Color, first_color: c_int, n_colors: c_int) {
        let sys_surf = as_sys_surface(surf);
        let palette = (*(*sys_surf).format).palette;
        sdl2::sys::SDL_SetPaletteColors(palette, colors as *const sdl2::sys::SDL_Color, first_color, n_colors);
    }

    unsafe fn set_palette_colors(&mut self, palette: *mut crate::SDL_Palette, colors: *const SDL_Color, first_color: c_int, n_colors: c_int) -> c_int {
        sdl2::sys::SDL_SetPaletteColors(palette as *mut sdl2::sys::SDL_Palette, colors as *const sdl2::sys::SDL_Color, first_color, n_colors)
    }

    unsafe fn set_surface_palette(&mut self, surf: *mut SDL_Surface, palette: *mut crate::SDL_Palette) -> c_int {
        sdl2::sys::SDL_SetSurfacePalette(as_sys_surface(surf), palette as *mut sdl2::sys::SDL_Palette)
    }

    unsafe fn convert_surface(&mut self, src: *mut SDL_Surface, fmt: *const SDL_PixelFormat, flags: u32) -> *mut SDL_Surface {
        sdl2::sys::SDL_ConvertSurface(as_sys_surface(src), fmt as *const sdl2::sys::SDL_PixelFormat, flags) as *mut SDL_Surface
    }

    unsafe fn set_blend_mode(&mut self, surf: *mut SDL_Surface, mode: c_int) -> c_int {
        // Raw SDL_BlendMode values are 0/1/2/4 (NONE/BLEND/ADD/MOD), not contiguous, so
        // this can't be a `mem::transmute` -- match the ones this codebase actually uses.
        let mode = match mode {
            0 => sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_NONE,
            1 => sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_BLEND,
            2 => sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_ADD,
            4 => sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_MOD,
            _ => sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_INVALID,
        };
        sdl2::sys::SDL_SetSurfaceBlendMode(as_sys_surface(surf), mode)
    }

    unsafe fn set_alpha_mod(&mut self, surf: *mut SDL_Surface, alpha: u8) {
        sdl2::sys::SDL_SetSurfaceAlphaMod(as_sys_surface(surf), alpha);
    }

    unsafe fn map_rgba(&mut self, format: *const crate::SDL_PixelFormat, r: u8, g: u8, b: u8, a: u8) -> u32 {
        sdl2::sys::SDL_MapRGBA(format as *const sdl2::sys::SDL_PixelFormat, r, g, b, a)
    }

    unsafe fn save_png(&mut self, surf: *mut SDL_Surface, path: &std::ffi::CStr) -> c_int {
        sdl2::sys::image::IMG_SavePNG(as_sys_surface(surf), path.as_ptr())
    }

    unsafe fn get_error(&mut self) -> *const std::os::raw::c_char {
        sdl2::sys::SDL_GetError()
    }

    unsafe fn blit(&mut self, src: *mut SDL_Surface, src_rect: *const SDL_Rect, dst: *mut SDL_Surface, dst_rect: *mut SDL_Rect) -> c_int {
        sdl2::sys::SDL_UpperBlit(as_sys_surface(src), as_sys_rect(src_rect), as_sys_surface(dst), dst_rect as *mut sdl2::sys::SDL_Rect)
    }

    unsafe fn fill_rect(&mut self, surf: *mut SDL_Surface, rect: *const SDL_Rect, color: u32) -> c_int {
        sdl2::sys::SDL_FillRect(as_sys_surface(surf), as_sys_rect(rect), color)
    }

    unsafe fn present(&mut self, frame: *mut SDL_Surface) {
        let canvas = self.canvas.as_mut().expect("present: no window (SdlPlatform not constructed yet)");
        let surf_ref = SurfaceRef::from_ll_mut(as_sys_surface(frame));
        let texture_creator = canvas.texture_creator();
        let texture = texture_creator.create_texture_from_surface(&*surf_ref).expect("present: create_texture_from_surface");
        canvas.clear();
        canvas.copy(&texture, None, None).expect("present: copy");
        canvas.present();
    }

    // Reads/writes the raw `window_` global directly, not `self.canvas` -- same
    // reasoning as the render_get_*/get_window_flags family below: seg009.rs's own
    // window creation hasn't migrated to this trait yet, so `self.canvas` is unset for
    // any instance that exists today, but `window_` is already live.
    unsafe fn set_fullscreen(&mut self, fullscreen: bool) {
        const SDL_WINDOW_FULLSCREEN_DESKTOP: u32 = 0x1001;
        sdl2::sys::SDL_SetWindowFullscreen(crate::window_ as *mut sdl2::sys::SDL_Window, if fullscreen { SDL_WINDOW_FULLSCREEN_DESKTOP } else { 0 });
    }

    unsafe fn show_cursor(&mut self, show: bool) {
        sdl2::sys::SDL_ShowCursor(show as c_int);
    }

    unsafe fn delay(&mut self, ms: u32) {
        sdl2::sys::SDL_Delay(ms);
    }

    unsafe fn rw_from_mem(&mut self, buf: *mut std::os::raw::c_void, size: c_int) -> *mut crate::SDL_RWops {
        sdl2::sys::SDL_RWFromMem(buf, size) as *mut crate::SDL_RWops
    }

    unsafe fn rw_tell(&mut self, rw: *mut crate::SDL_RWops) -> i64 {
        sdl2::sys::SDL_RWtell(rw as *mut sdl2::sys::SDL_RWops)
    }

    unsafe fn rw_close(&mut self, rw: *mut crate::SDL_RWops) -> c_int {
        sdl2::sys::SDL_RWclose(rw as *mut sdl2::sys::SDL_RWops)
    }

    unsafe fn show_message_box(&mut self, title: &std::ffi::CStr, message: &std::ffi::CStr) {
        const SDL_MESSAGEBOX_ERROR: u32 = 0x00000010;
        sdl2::sys::SDL_ShowSimpleMessageBox(SDL_MESSAGEBOX_ERROR, title.as_ptr(), message.as_ptr(), std::ptr::null_mut());
    }

    unsafe fn rw_write(&mut self, rw: *mut crate::SDL_RWops, ptr: *const std::os::raw::c_void, size: usize, num: usize) -> usize {
        sdl2::sys::SDL_RWwrite(rw as *mut sdl2::sys::SDL_RWops, ptr, size, num)
    }

    unsafe fn rw_read(&mut self, rw: *mut crate::SDL_RWops, ptr: *mut std::os::raw::c_void, size: usize, maxnum: usize) -> usize {
        sdl2::sys::SDL_RWread(rw as *mut sdl2::sys::SDL_RWops, ptr, size, maxnum)
    }

    unsafe fn linked_sdl_version(&mut self) -> (u8, u8, u8) {
        let mut ver: sdl2::sys::SDL_version = std::mem::zeroed();
        sdl2::sys::SDL_GetVersion(&mut ver);
        (ver.major, ver.minor, ver.patch)
    }

    unsafe fn performance_counter(&mut self) -> u64 {
        sdl2::sys::SDL_GetPerformanceCounter()
    }

    unsafe fn performance_frequency(&mut self) -> u64 {
        sdl2::sys::SDL_GetPerformanceFrequency()
    }

    unsafe fn rw_from_file(&mut self, path: &std::ffi::CStr, mode: &std::ffi::CStr) -> *mut crate::SDL_RWops {
        sdl2::sys::SDL_RWFromFile(path.as_ptr(), mode.as_ptr()) as *mut crate::SDL_RWops
    }

    unsafe fn get_scancode_name(&mut self, scancode: u32) -> *const std::os::raw::c_char {
        sdl2::sys::SDL_GetScancodeName(std::mem::transmute::<u32, sdl2::sys::SDL_Scancode>(scancode))
    }

    unsafe fn get_window_flags(&mut self, window: *mut crate::SDL_Window) -> u32 {
        sdl2::sys::SDL_GetWindowFlags(window as *mut sdl2::sys::SDL_Window)
    }

    unsafe fn render_get_scale(&mut self, renderer: *mut crate::SDL_Renderer) -> (f32, f32) {
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        sdl2::sys::SDL_RenderGetScale(renderer as *mut sdl2::sys::SDL_Renderer, &mut sx, &mut sy);
        (sx, sy)
    }

    unsafe fn render_get_logical_size(&mut self, renderer: *mut crate::SDL_Renderer) -> (c_int, c_int) {
        let (mut w, mut h) = (0, 0);
        sdl2::sys::SDL_RenderGetLogicalSize(renderer as *mut sdl2::sys::SDL_Renderer, &mut w, &mut h);
        (w, h)
    }

    unsafe fn render_get_viewport(&mut self, renderer: *mut crate::SDL_Renderer) -> SDL_Rect {
        let mut rect: sdl2::sys::SDL_Rect = std::mem::zeroed();
        sdl2::sys::SDL_RenderGetViewport(renderer as *mut sdl2::sys::SDL_Renderer, &mut rect);
        SDL_Rect { x: rect.x, y: rect.y, w: rect.w, h: rect.h }
    }

    unsafe fn render_set_integer_scale(&mut self, renderer: *mut crate::SDL_Renderer, enable: bool) -> c_int {
        let flag = if enable { sdl2::sys::SDL_bool::SDL_TRUE } else { sdl2::sys::SDL_bool::SDL_FALSE };
        sdl2::sys::SDL_RenderSetIntegerScale(renderer as *mut sdl2::sys::SDL_Renderer, flag)
    }

    unsafe fn map_rgb(&mut self, format: *const crate::SDL_PixelFormat, r: u8, g: u8, b: u8) -> u32 {
        sdl2::sys::SDL_MapRGB(format as *const sdl2::sys::SDL_PixelFormat, r, g, b)
    }

    unsafe fn set_clip_rect(&mut self, surf: *mut SDL_Surface, rect: *const SDL_Rect) -> c_int {
        sdl2::sys::SDL_SetClipRect(as_sys_surface(surf), as_sys_rect(rect)) as c_int
    }

    unsafe fn convert_surface_format(&mut self, src: *mut SDL_Surface, pixel_format: u32, flags: u32) -> *mut SDL_Surface {
        sdl2::sys::SDL_ConvertSurfaceFormat(as_sys_surface(src), pixel_format, flags) as *mut SDL_Surface
    }

    unsafe fn blit_scaled(&mut self, src: *mut SDL_Surface, src_rect: *const SDL_Rect, dst: *mut SDL_Surface, dst_rect: *mut SDL_Rect) -> c_int {
        sdl2::sys::SDL_UpperBlitScaled(as_sys_surface(src), as_sys_rect(src_rect), as_sys_surface(dst), dst_rect as *mut sdl2::sys::SDL_Rect)
    }

    unsafe fn set_window_icon(&mut self, window: *mut crate::SDL_Window, icon: *mut SDL_Surface) {
        sdl2::sys::SDL_SetWindowIcon(window as *mut sdl2::sys::SDL_Window, as_sys_surface(icon));
    }

    unsafe fn rw_from_const_mem(&mut self, mem: *const std::os::raw::c_void, size: c_int) -> *mut crate::SDL_RWops {
        sdl2::sys::SDL_RWFromConstMem(mem, size) as *mut crate::SDL_RWops
    }

    unsafe fn create_texture(&mut self, renderer: *mut crate::SDL_Renderer, format: u32, access: c_int, w: c_int, h: c_int) -> *mut crate::SDL_Texture {
        sdl2::sys::SDL_CreateTexture(renderer as *mut sdl2::sys::SDL_Renderer, format, access, w, h) as *mut crate::SDL_Texture
    }

    unsafe fn update_texture(&mut self, texture: *mut crate::SDL_Texture, rect: *const SDL_Rect, pixels: *const std::os::raw::c_void, pitch: c_int) -> c_int {
        sdl2::sys::SDL_UpdateTexture(texture as *mut sdl2::sys::SDL_Texture, as_sys_rect(rect), pixels, pitch)
    }

    unsafe fn set_render_target(&mut self, renderer: *mut crate::SDL_Renderer, texture: *mut crate::SDL_Texture) -> c_int {
        sdl2::sys::SDL_SetRenderTarget(renderer as *mut sdl2::sys::SDL_Renderer, texture as *mut sdl2::sys::SDL_Texture)
    }

    unsafe fn render_clear(&mut self, renderer: *mut crate::SDL_Renderer) -> c_int {
        sdl2::sys::SDL_RenderClear(renderer as *mut sdl2::sys::SDL_Renderer)
    }

    unsafe fn render_copy(&mut self, renderer: *mut crate::SDL_Renderer, texture: *mut crate::SDL_Texture, src_rect: *const SDL_Rect, dst_rect: *const SDL_Rect) -> c_int {
        sdl2::sys::SDL_RenderCopy(renderer as *mut sdl2::sys::SDL_Renderer, texture as *mut sdl2::sys::SDL_Texture, as_sys_rect(src_rect), as_sys_rect(dst_rect))
    }

    unsafe fn render_present(&mut self, renderer: *mut crate::SDL_Renderer) {
        sdl2::sys::SDL_RenderPresent(renderer as *mut sdl2::sys::SDL_Renderer);
    }

    unsafe fn render_set_logical_size(&mut self, renderer: *mut crate::SDL_Renderer, w: c_int, h: c_int) -> c_int {
        sdl2::sys::SDL_RenderSetLogicalSize(renderer as *mut sdl2::sys::SDL_Renderer, w, h)
    }

    unsafe fn get_renderer_output_size(&mut self, renderer: *mut crate::SDL_Renderer) -> (c_int, c_int) {
        let (mut w, mut h) = (0, 0);
        sdl2::sys::SDL_GetRendererOutputSize(renderer as *mut sdl2::sys::SDL_Renderer, &mut w, &mut h);
        (w, h)
    }

    unsafe fn get_renderer_info_flags(&mut self, renderer: *mut crate::SDL_Renderer) -> u32 {
        let mut info: sdl2::sys::SDL_RendererInfo = std::mem::zeroed();
        if sdl2::sys::SDL_GetRendererInfo(renderer as *mut sdl2::sys::SDL_Renderer, &mut info) == 0 {
            info.flags
        } else {
            0
        }
    }

    unsafe fn set_hint(&mut self, name: &std::ffi::CStr, value: &std::ffi::CStr) -> c_int {
        sdl2::sys::SDL_SetHint(name.as_ptr(), value.as_ptr()) as c_int
    }

    unsafe fn sdl_init(&mut self, flags: u32) -> c_int {
        sdl2::sys::SDL_Init(flags)
    }

    unsafe fn sdl_init_subsystem(&mut self, flags: u32) -> c_int {
        sdl2::sys::SDL_InitSubSystem(flags)
    }

    unsafe fn sdl_quit(&mut self) {
        sdl2::sys::SDL_Quit();
    }

    unsafe fn create_window(&mut self, title: &std::ffi::CStr, x: c_int, y: c_int, w: c_int, h: c_int, flags: u32) -> *mut crate::SDL_Window {
        sdl2::sys::SDL_CreateWindow(title.as_ptr(), x, y, w, h, flags) as *mut crate::SDL_Window
    }

    unsafe fn create_renderer(&mut self, window: *mut crate::SDL_Window, index: c_int, flags: u32) -> *mut crate::SDL_Renderer {
        sdl2::sys::SDL_CreateRenderer(window as *mut sdl2::sys::SDL_Window, index, flags) as *mut crate::SDL_Renderer
    }

    unsafe fn open_audio_raw(&mut self, desired: *mut std::os::raw::c_void, obtained: *mut std::os::raw::c_void) -> c_int {
        sdl2::sys::SDL_OpenAudio(desired as *mut sdl2::sys::SDL_AudioSpec, obtained as *mut sdl2::sys::SDL_AudioSpec)
    }

    unsafe fn num_joysticks(&mut self) -> c_int {
        sdl2::sys::SDL_NumJoysticks()
    }

    unsafe fn is_game_controller(&mut self, joystick_index: c_int) -> bool {
        sdl2::sys::SDL_IsGameController(joystick_index) == sdl2::sys::SDL_bool::SDL_TRUE
    }

    unsafe fn game_controller_open(&mut self, joystick_index: c_int) -> *mut crate::SDL_GameController {
        sdl2::sys::SDL_GameControllerOpen(joystick_index) as *mut crate::SDL_GameController
    }

    unsafe fn game_controller_close(&mut self, controller: *mut crate::SDL_GameController) {
        sdl2::sys::SDL_GameControllerClose(controller as *mut sdl2::sys::SDL_GameController);
    }

    unsafe fn game_controller_from_instance_id(&mut self, joyid: i32) -> *mut crate::SDL_GameController {
        sdl2::sys::SDL_GameControllerFromInstanceID(joyid) as *mut crate::SDL_GameController
    }

    unsafe fn game_controller_add_mappings_from_file(&mut self, path: &std::ffi::CStr) -> c_int {
        let rw = sdl2::sys::SDL_RWFromFile(path.as_ptr(), b"rb\0".as_ptr() as *const std::os::raw::c_char);
        sdl2::sys::SDL_GameControllerAddMappingsFromRW(rw, 1)
    }

    unsafe fn joystick_open(&mut self, device_index: c_int) -> *mut crate::SDL_Joystick {
        sdl2::sys::SDL_JoystickOpen(device_index) as *mut crate::SDL_Joystick
    }

    unsafe fn haptic_open(&mut self, device_index: c_int) -> *mut crate::SDL_Haptic {
        sdl2::sys::SDL_HapticOpen(device_index) as *mut crate::SDL_Haptic
    }

    unsafe fn haptic_rumble_init(&mut self, haptic: *mut crate::SDL_Haptic) -> c_int {
        sdl2::sys::SDL_HapticRumbleInit(haptic as *mut sdl2::sys::SDL_Haptic)
    }

    unsafe fn push_event(&mut self, event: *mut std::os::raw::c_void) -> c_int {
        sdl2::sys::SDL_PushEvent(event as *mut sdl2::sys::SDL_Event)
    }

    unsafe fn poll_event(&mut self, event: *mut std::os::raw::c_void) -> c_int {
        sdl2::sys::SDL_PollEvent(event as *mut sdl2::sys::SDL_Event)
    }
}

pub struct SdlAudio {
    subsystem: Option<AudioSubsystem>,
    device: Option<sdl2::audio::AudioDevice<Callback>>,
}

// lock()/unlock()/pause() below are self-independent (they call the raw legacy global
// SDL audio API, not anything through `subsystem`/`device`), so callers that only need
// those three -- midi.rs, so far -- don't need a live `SdlPlatform` (which requires a
// real window) to reach them. `shared_audio()` is a minimal always-available singleton
// for exactly that: no init beyond what SDL_Init already did elsewhere (still via raw
// FFI in seg009.rs, not yet migrated -- see the exit-criteria note on `open()` below).
// See shared_renderer()'s comment: typed as the backend-selected alias.
static mut SHARED_AUDIO: crate::platform::backend::ActiveAudio =
    SdlAudio { subsystem: None, device: None };

#[allow(static_mut_refs)]
pub fn shared_audio() -> &'static mut crate::platform::backend::ActiveAudio {
    unsafe { &mut SHARED_AUDIO }
}

struct Callback(Box<dyn FnMut(&mut [i16]) + Send>);

impl sdl2::audio::AudioCallback for Callback {
    type Channel = i16;
    fn callback(&mut self, out: &mut [i16]) {
        (self.0)(out);
    }
}

impl AudioBackend for SdlAudio {
    fn open(&mut self, sample_rate: c_int, channels: u8, fill: Box<dyn FnMut(&mut [i16]) + Send>) -> Result<(), String> {
        let desired = sdl2::audio::AudioSpecDesired {
            freq: Some(sample_rate),
            channels: Some(channels),
            samples: Some(1024),
        };
        let subsystem = self.subsystem.as_ref().ok_or("SdlAudio::open: no AudioSubsystem (not constructed via SdlPlatform::new)")?;
        let device = subsystem.open_playback(None, &desired, |_spec| Callback(fill))?;
        device.resume();
        self.device = Some(device);
        Ok(())
    }
    // lock/unlock/pause go through the raw legacy (single implicit device) SDL API --
    // SDL_LockAudio/SDL_UnlockAudio/SDL_PauseAudio -- rather than the sdl2 crate's
    // per-device AudioDevice methods. seg009.rs's own audio init (SDL_OpenAudio) hasn't
    // migrated to this trait's open() yet, so `self.device` is unset for any SdlAudio
    // instance that exists today; these three calls need to keep operating on whatever
    // device seg009.rs's raw SDL_OpenAudio call actually opened, which the legacy global
    // API does regardless of which module opened it. Once seg009.rs's init moves to
    // open() (the modern per-device API), these three should move to self.device's
    // lock()/resume()/pause() in that same change -- mixing the two audio APIs against
    // the same device is what would actually break.
    fn pause(&mut self, paused: bool) {
        unsafe { sdl2::sys::SDL_PauseAudio(paused as c_int) };
    }
    fn lock(&mut self) {
        unsafe { sdl2::sys::SDL_LockAudio() };
    }
    fn unlock(&mut self) {
        unsafe { sdl2::sys::SDL_UnlockAudio() };
    }
}

pub struct SdlInput {
    event_pump: Option<EventPump>,
    video: Option<sdl2::VideoSubsystem>,
    timer: Option<TimerSubsystem>,
    // Opened at SdlPlatform::new() time for future use (hotplug handling, once that
    // migrates too), but NOT what rumble() below reads from -- see its comment.
    #[allow(dead_code)]
    controller: Option<sdl2::controller::GameController>,
    #[allow(dead_code)]
    joystick: Option<sdl2::joystick::Joystick>,
    #[allow(dead_code)]
    haptic: Option<sdl2::haptic::Haptic>,
}

// See shared_renderer()'s comment: typed as the backend-selected alias.
static mut SHARED_INPUT: crate::platform::backend::ActiveInput =
    SdlInput { event_pump: None, video: None, timer: None, controller: None, joystick: None, haptic: None };

#[allow(static_mut_refs)]
pub fn shared_input() -> &'static mut crate::platform::backend::ActiveInput {
    unsafe { &mut SHARED_INPUT }
}

impl SdlInput {
    // Populates event_pump/video/timer so key_state()/mouse_state()/text-input/timer
    // calls work. The real startup path (seg009.rs's set_gr_mode) initializes SDL via
    // raw SDL_Init/SDL_CreateWindow FFI calls on `shared_renderer()`, never through
    // `SdlPlatform::new()` -- so nothing populated these fields before this method was
    // added. `sdl2::init()` is safe to call here too: SDL ref-counts each subsystem's
    // init count internally, so this doesn't double-initialize anything the raw
    // SDL_Init call above already brought up.
    pub fn init(&mut self) -> Result<(), String> {
        let sdl = sdl2::init()?;
        self.event_pump = Some(sdl.event_pump()?);
        self.video = Some(sdl.video()?);
        self.timer = Some(sdl.timer()?);
        Ok(())
    }
}

impl InputSource for SdlInput {
    fn key_state(&self, scancode: c_int) -> bool {
        let Some(code) = Scancode::from_i32(scancode) else { return false };
        let event_pump = self.event_pump.as_ref().expect("key_state: no event pump (SdlPlatform not constructed yet)");
        event_pump.keyboard_state().is_scancode_pressed(code)
    }
    fn mouse_state(&self) -> (c_int, c_int, bool, bool) {
        let event_pump = self.event_pump.as_ref().expect("mouse_state: no event pump (SdlPlatform not constructed yet)");
        let state = event_pump.mouse_state();
        (state.x(), state.y(), state.left(), state.right())
    }
    fn start_text_input(&mut self, x: c_int, y: c_int, w: c_int, h: c_int) {
        let video = self.video.as_ref().expect("start_text_input: no video subsystem (SdlPlatform not constructed yet)");
        video.text_input().set_rect(Rect::new(x, y, w as u32, h as u32));
        video.text_input().start();
    }
    fn stop_text_input(&mut self) {
        let video = self.video.as_ref().expect("stop_text_input: no video subsystem (SdlPlatform not constructed yet)");
        video.text_input().stop();
    }
    fn add_one_shot_timer(&mut self, delay_ms: u32, callback: Box<dyn FnOnce() + Send>) -> bool {
        let timer_subsystem = self.timer.as_ref().expect("add_one_shot_timer: no timer subsystem (SdlPlatform not constructed yet)");
        let mut callback = Some(callback);
        let timer = timer_subsystem.add_timer(
            delay_ms,
            Box::new(move || {
                if let Some(cb) = callback.take() {
                    cb();
                }
                0
            }),
        );
        // Fire-and-forget, matching the C original's `SDL_AddTimer` call (which never
        // calls `SDL_RemoveTimer`): the `sdl2` crate's `Timer` cancels on drop, so it
        // must be leaked, not held, for a genuinely one-shot fire-once timer.
        std::mem::forget(timer);
        // The crate's `Timer` doesn't expose the raw SDL_TimerID, so the original
        // `SDL_AddTimer(...) == 0` failure check can't be replicated exactly here.
        // SDL_AddTimer only fails on internal allocation failure -- unreachable in
        // practice -- so this always reports success rather than adding a raw-sys
        // bypass just to check a hasn't-been-observed-to-fail return value.
        true
    }
    // Reads the same raw sdl_haptic/sdl_controller_/sdl_joystick_ globals seg003.rs
    // read directly before this migration (populated by seg009.rs's raw-FFI controller
    // init, which hasn't itself migrated to this trait yet -- see the AudioBackend
    // lock/unlock/pause comment on SdlAudio for the identical reasoning). Preserves the
    // exact haptic -> controller -> joystick fallback order, including the original's
    // quirk of calling SDL_JoystickRumble even when sdl_joystick_ is null if neither of
    // the other two is available (SDL_JoystickRumble on a null pointer just returns an
    // error code, so this was always harmless, not a bug worth fixing here).
    fn rumble(&mut self, strength: f32, duration_ms: u32) {
        unsafe {
            let level = (strength * 65535.0) as u16;
            if !crate::sdl_haptic.is_null() {
                sdl2::sys::SDL_HapticRumblePlay(crate::sdl_haptic as *mut sdl2::sys::SDL_Haptic, strength, duration_ms);
            } else if !crate::sdl_controller_.is_null() {
                sdl2::sys::SDL_GameControllerRumble(crate::sdl_controller_ as *mut sdl2::sys::SDL_GameController, level, level, duration_ms);
            } else {
                sdl2::sys::SDL_JoystickRumble(crate::sdl_joystick_ as *mut sdl2::sys::SDL_Joystick, level, level, duration_ms);
            }
        }
    }
}

pub struct SdlFiles;

impl FileSystem for SdlFiles {
    fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }
    fn write_file(&self, path: &str, data: &[u8]) -> Result<(), String> {
        std::fs::write(path, data).map_err(|e| e.to_string())
    }
    fn file_exists(&self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
}

pub struct SdlPlatform {
    renderer: SdlRenderer,
    audio: SdlAudio,
    input: SdlInput,
    files: SdlFiles,
}

impl SdlPlatform {
    pub fn new(title: &str, width: u32, height: u32) -> Result<Self, String> {
        let sdl = sdl2::init()?;
        let video = sdl.video()?;
        let window = video.window(title, width, height).position_centered().build().map_err(|e| e.to_string())?;
        let canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
        let image_context = sdl2::image::init(sdl2::image::InitFlag::PNG)?;
        let audio_subsystem = sdl.audio()?;
        let event_pump = sdl.event_pump()?;
        let timer = sdl.timer()?;

        let controller = (0..sdl.game_controller()?.num_joysticks().unwrap_or(0))
            .find(|&i| sdl.game_controller().unwrap().is_game_controller(i))
            .and_then(|i| sdl.game_controller().unwrap().open(i).ok());
        let haptic = controller.as_ref().and_then(|_| sdl.haptic().ok()).and_then(|h| h.open_from_joystick_id(0).ok());

        Ok(SdlPlatform {
            renderer: SdlRenderer { canvas: Some(canvas), _image_context: Some(image_context) },
            audio: SdlAudio { subsystem: Some(audio_subsystem), device: None },
            input: SdlInput { event_pump: Some(event_pump), video: Some(video), timer: Some(timer), controller, joystick: None, haptic },
            files: SdlFiles,
        })
    }

    pub fn poll_events(&mut self) -> Vec<Event> {
        self.input.event_pump.as_mut().expect("poll_events: no event pump").poll_iter().collect()
    }
}

impl Platform for SdlPlatform {
    type Rend = SdlRenderer;
    type Audio = SdlAudio;
    type Input = SdlInput;
    type Files = SdlFiles;

    fn renderer(&mut self) -> &mut Self::Rend {
        &mut self.renderer
    }
    fn audio(&mut self) -> &mut Self::Audio {
        &mut self.audio
    }
    fn input(&mut self) -> &mut Self::Input {
        &mut self.input
    }
    fn files(&mut self) -> &mut Self::Files {
        &mut self.files
    }
}
