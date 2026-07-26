//! WASM platform backend (Phase 2 of the WASM/game-beating/fuzzing plan,
//! `docs/plans/12-wasm-cfg-fuzz.md`).
//!
//! **Milestone 1 stub.** Every trait method here is `unimplemented!()`. The goal of this
//! first pass is only to get `cargo check --target wasm32-unknown-unknown` past
//! compilation -- proving `build.rs`/`Cargo.toml` no longer pull in real SDL2 for this
//! target, and that the `Renderer`/`AudioBackend`/`InputSource`/`FileSystem` trait surface
//! (already fully relocated behind `platform::backend::Active*` in Step C) is enough to
//! swap backends with zero call-site changes. Real Canvas/Web Audio/`localStorage`
//! implementations land incrementally after this compiles, per-method, the same
//! batch-then-verify discipline as the rest of the port -- there is no harness to check
//! against here (the 30-replay differential trace harness only runs a native binary), so
//! each real implementation should instead be checked by hand in a browser once enough of
//! the surface is filled in to boot a frame.

use std::os::raw::c_int;

use crate::{SDL_Color, SDL_PixelFormat, SDL_RWops, SDL_Rect, SDL_Surface};

use super::{AudioBackend, FileSystem, InputSource, Renderer};

pub struct WasmRenderer;

static mut SHARED_RENDERER: crate::platform::backend::ActiveRenderer = WasmRenderer;

#[allow(static_mut_refs)]
pub fn shared_renderer() -> &'static mut crate::platform::backend::ActiveRenderer {
    unsafe { &mut SHARED_RENDERER }
}

impl Renderer for WasmRenderer {
    unsafe fn create_surface(&mut self, _width: c_int, _height: c_int, _depth: c_int, _rmask: u32, _gmask: u32, _bmask: u32, _amask: u32) -> *mut SDL_Surface {
        unimplemented!("WasmRenderer::create_surface")
    }
    unsafe fn free_surface(&mut self, _surf: *mut SDL_Surface) {
        unimplemented!("WasmRenderer::free_surface")
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
        unimplemented!("WasmRenderer::lock_surface")
    }
    unsafe fn unlock_surface(&mut self, _surf: *mut SDL_Surface) {
        unimplemented!("WasmRenderer::unlock_surface")
    }
    unsafe fn set_color_key(&mut self, _surf: *mut SDL_Surface, _enable: bool, _key: u32) -> c_int {
        unimplemented!("WasmRenderer::set_color_key")
    }
    unsafe fn set_palette(&mut self, _surf: *mut SDL_Surface, _colors: *const SDL_Color, _first_color: c_int, _n_colors: c_int) {
        unimplemented!("WasmRenderer::set_palette")
    }
    unsafe fn set_palette_colors(&mut self, _palette: *mut crate::SDL_Palette, _colors: *const SDL_Color, _first_color: c_int, _n_colors: c_int) -> c_int {
        unimplemented!("WasmRenderer::set_palette_colors")
    }
    unsafe fn set_surface_palette(&mut self, _surf: *mut SDL_Surface, _palette: *mut crate::SDL_Palette) -> c_int {
        unimplemented!("WasmRenderer::set_surface_palette")
    }
    unsafe fn convert_surface(&mut self, _src: *mut SDL_Surface, _fmt: *const SDL_PixelFormat, _flags: u32) -> *mut SDL_Surface {
        unimplemented!("WasmRenderer::convert_surface")
    }
    unsafe fn set_blend_mode(&mut self, _surf: *mut SDL_Surface, _mode: c_int) -> c_int {
        unimplemented!("WasmRenderer::set_blend_mode")
    }
    unsafe fn set_alpha_mod(&mut self, _surf: *mut SDL_Surface, _alpha: u8) {
        unimplemented!("WasmRenderer::set_alpha_mod")
    }
    unsafe fn map_rgba(&mut self, _format: *const SDL_PixelFormat, _r: u8, _g: u8, _b: u8, _a: u8) -> u32 {
        unimplemented!("WasmRenderer::map_rgba")
    }
    unsafe fn save_png(&mut self, _surf: *mut SDL_Surface, _path: &std::ffi::CStr) -> c_int {
        unimplemented!("WasmRenderer::save_png")
    }
    unsafe fn get_error(&mut self) -> *const std::os::raw::c_char {
        unimplemented!("WasmRenderer::get_error")
    }
    unsafe fn blit(&mut self, _src: *mut SDL_Surface, _src_rect: *const SDL_Rect, _dst: *mut SDL_Surface, _dst_rect: *mut SDL_Rect) -> c_int {
        unimplemented!("WasmRenderer::blit")
    }
    unsafe fn fill_rect(&mut self, _surf: *mut SDL_Surface, _rect: *const SDL_Rect, _color: u32) -> c_int {
        unimplemented!("WasmRenderer::fill_rect")
    }
    unsafe fn present(&mut self, _frame: *mut SDL_Surface) {
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
    unsafe fn map_rgb(&mut self, _format: *const SDL_PixelFormat, _r: u8, _g: u8, _b: u8) -> u32 {
        unimplemented!("WasmRenderer::map_rgb")
    }
    unsafe fn set_clip_rect(&mut self, _surf: *mut SDL_Surface, _rect: *const SDL_Rect) -> c_int {
        unimplemented!("WasmRenderer::set_clip_rect")
    }
    unsafe fn convert_surface_format(&mut self, _src: *mut SDL_Surface, _pixel_format: u32, _flags: u32) -> *mut SDL_Surface {
        unimplemented!("WasmRenderer::convert_surface_format")
    }
    unsafe fn blit_scaled(&mut self, _src: *mut SDL_Surface, _src_rect: *const SDL_Rect, _dst: *mut SDL_Surface, _dst_rect: *mut SDL_Rect) -> c_int {
        unimplemented!("WasmRenderer::blit_scaled")
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

pub struct WasmAudio;

static mut SHARED_AUDIO: crate::platform::backend::ActiveAudio = WasmAudio;

#[allow(static_mut_refs)]
pub fn shared_audio() -> &'static mut crate::platform::backend::ActiveAudio {
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

static mut SHARED_INPUT: crate::platform::backend::ActiveInput = WasmInput;

#[allow(static_mut_refs)]
pub fn shared_input() -> &'static mut crate::platform::backend::ActiveInput {
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
