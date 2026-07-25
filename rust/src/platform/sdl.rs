//! The native SDL2 backend. This is the only module in the crate allowed to reference
//! `SDL_*`/`IMG_*` directly (through the `sdl2` crate's safe wrappers, with a narrow raw
//! `sdl2::sys` escape hatch for the handful of operations the crate doesn't expose --
//! `IMG_Load_RW`, notably, since the crate's `LoadSurface` trait only offers
//! `from_file`/`from_xpm_array`, not a load-from-memory-buffer path).
//!
//! Not yet wired into game logic -- this module stands alone, proving the trait shapes
//! in `platform::mod` against the real `sdl2` crate API before the call-site migration
//! (moving seg009.rs/seg008.rs/menu.rs/etc.'s direct SDL calls to go through it) happens.

use std::os::raw::c_int;

use sdl2::event::Event;
use sdl2::image::LoadSurface;
use sdl2::keyboard::Scancode;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::WindowCanvas;
use sdl2::rwops::RWops;
use sdl2::surface::Surface as SdlSurfaceOwned;
use sdl2::AudioSubsystem;
use sdl2::EventPump;
use sdl2::TimerSubsystem;

use super::{AudioBackend, FileSystem, InputSource, Platform, Renderer, Surface};

pub struct SdlOwnedSurface(SdlSurfaceOwned<'static>);

impl Surface for SdlOwnedSurface {
    fn width(&self) -> c_int {
        self.0.width() as c_int
    }
    fn height(&self) -> c_int {
        self.0.height() as c_int
    }
    fn pitch(&self) -> c_int {
        self.0.pitch() as c_int
    }
    fn with_pixels_mut<R>(&mut self, f: impl FnOnce(&mut [u8]) -> R) -> R {
        self.0.with_lock_mut(f)
    }
    fn set_color_key(&mut self, key: u32) -> Result<(), String> {
        self.0.set_color_key(true, Color::RGB(0, 0, 0)).map(|_| ())?;
        // Overwrite with the exact palette-index key rather than the RGB placeholder
        // above -- the game's DAT-derived surfaces are indexed, so the key is a raw
        // index byte, not an RGB triple; `sdl2::pixels::Color` has no raw-index
        // constructor, so go through the ll surface for the actual key value.
        let raw = self.0.raw();
        unsafe {
            sdl2::sys::SDL_SetColorKey(raw, 1, key);
        }
        Ok(())
    }
    fn set_palette(&mut self, colors: &[(u8, u8, u8)]) -> Result<(), String> {
        let sdl_colors: Vec<Color> = colors.iter().map(|&(r, g, b)| Color::RGB(r, g, b)).collect();
        let palette = sdl2::pixels::Palette::with_colors(&sdl_colors)?;
        self.0.set_palette(&palette)
    }
    fn set_blend_mode(&mut self, blend: bool) -> Result<(), String> {
        let mode = if blend { sdl2::render::BlendMode::Blend } else { sdl2::render::BlendMode::None };
        self.0.set_blend_mode(mode)
    }
    fn set_alpha_mod(&mut self, alpha: u8) -> Result<(), String> {
        self.0.set_alpha_mod(alpha);
        Ok(())
    }
}

pub struct SdlRenderer {
    canvas: WindowCanvas,
    _image_context: sdl2::image::Sdl2ImageContext,
}

impl Renderer for SdlRenderer {
    type Surf = SdlOwnedSurface;

    fn create_surface(&mut self, width: c_int, height: c_int) -> Self::Surf {
        let surf = SdlSurfaceOwned::new(width as u32, height as u32, PixelFormatEnum::Index8)
            .expect("create_surface: SDL_CreateRGBSurface");
        SdlOwnedSurface(surf)
    }

    fn load_image_from_memory(&mut self, bytes: &[u8]) -> Result<Self::Surf, String> {
        // `sdl2::image::LoadSurface` only exposes `from_file`/`from_xpm_array`; the crate
        // has no safe load-from-memory-buffer wrapper (matches IMG_Load_RW), so this is
        // exactly the narrow raw-sys escape hatch flagged in the Step C plan.
        let rwops = RWops::from_bytes(bytes)?;
        unsafe {
            let raw = sdl2::sys::image::IMG_Load_RW(rwops.raw(), 0);
            if raw.is_null() {
                Err(sdl2::get_error())
            } else {
                Ok(SdlOwnedSurface(SdlSurfaceOwned::from_ll(raw)))
            }
        }
    }

    fn load_image_from_file(&mut self, path: &str) -> Result<Self::Surf, String> {
        SdlSurfaceOwned::from_file(path).map(SdlOwnedSurface)
    }

    fn blit(&mut self, src: &Self::Surf, dst: &mut Self::Surf, dst_x: c_int, dst_y: c_int) {
        let dst_rect = Rect::new(dst_x, dst_y, src.0.width(), src.0.height());
        src.0.blit(None, &mut dst.0, dst_rect).expect("blit");
    }

    fn fill_rect(&mut self, surf: &mut Self::Surf, x: c_int, y: c_int, w: c_int, h: c_int, color: u32) {
        let rect = Rect::new(x, y, w as u32, h as u32);
        surf.0.fill_rect(rect, Color::RGBA((color >> 24) as u8, (color >> 16) as u8, (color >> 8) as u8, color as u8)).expect("fill_rect");
    }

    fn present(&mut self, frame: &Self::Surf) {
        let texture_creator = self.canvas.texture_creator();
        let texture = texture_creator
            .create_texture_from_surface(&frame.0)
            .expect("present: create_texture_from_surface");
        self.canvas.clear();
        self.canvas.copy(&texture, None, None).expect("present: copy");
        self.canvas.present();
    }

    fn set_fullscreen(&mut self, fullscreen: bool) -> Result<(), String> {
        let mode = if fullscreen { sdl2::video::FullscreenType::Desktop } else { sdl2::video::FullscreenType::Off };
        self.canvas.window_mut().set_fullscreen(mode)
    }

    fn show_cursor(&mut self, show: bool) {
        self.canvas.window().subsystem().sdl().mouse().show_cursor(show);
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
static mut SHARED_AUDIO: SdlAudio = SdlAudio { subsystem: None, device: None };

#[allow(static_mut_refs)]
pub fn shared_audio() -> &'static mut SdlAudio {
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
    event_pump: EventPump,
    video: sdl2::VideoSubsystem,
    timer: TimerSubsystem,
    controller: Option<sdl2::controller::GameController>,
    joystick: Option<sdl2::joystick::Joystick>,
    haptic: Option<sdl2::haptic::Haptic>,
}

impl InputSource for SdlInput {
    fn key_state(&self, scancode: c_int) -> bool {
        let Some(code) = Scancode::from_i32(scancode) else { return false };
        self.event_pump.keyboard_state().is_scancode_pressed(code)
    }
    fn mouse_state(&self) -> (c_int, c_int, bool, bool) {
        let state = self.event_pump.mouse_state();
        (state.x(), state.y(), state.left(), state.right())
    }
    fn start_text_input(&mut self, x: c_int, y: c_int, w: c_int, h: c_int) {
        self.video.text_input().set_rect(Rect::new(x, y, w as u32, h as u32));
        self.video.text_input().start();
    }
    fn stop_text_input(&mut self) {
        self.video.text_input().stop();
    }
    fn add_one_shot_timer(&mut self, delay_ms: u32, callback: Box<dyn FnOnce() + Send>) {
        let mut callback = Some(callback);
        let timer = self.timer.add_timer(
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
    }
    fn rumble(&mut self, strength: f32, duration_ms: u32) {
        if let Some(haptic) = &mut self.haptic {
            haptic.rumble_play(strength, duration_ms);
        } else if let Some(controller) = &mut self.controller {
            let _ = controller.set_rumble((strength * 65535.0) as u16, (strength * 65535.0) as u16, duration_ms);
        } else if let Some(joystick) = &mut self.joystick {
            let _ = joystick.set_rumble((strength * 65535.0) as u16, (strength * 65535.0) as u16, duration_ms);
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
            renderer: SdlRenderer { canvas, _image_context: image_context },
            audio: SdlAudio { subsystem: Some(audio_subsystem), device: None },
            input: SdlInput { event_pump, video, timer, controller, joystick: None, haptic },
            files: SdlFiles,
        })
    }

    pub fn poll_events(&mut self) -> Vec<Event> {
        self.input.event_pump.poll_iter().collect()
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
