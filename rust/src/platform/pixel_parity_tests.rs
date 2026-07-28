//! Pixel-parity tests (Phase A, `docs/plans/13-platform-architecture-unification.md`):
//! run the same sequence of `Renderer` calls against `SdlRenderer` (real SDL2, headless via
//! `SDL_VIDEODRIVER=dummy`) and `WasmRenderer` (plain `Vec<u8>` buffers, no wasm32 target or
//! browser needed -- see `platform::wasm`'s module doc), and assert identical output. This
//! is the primary, cheap, fast-feedback check that `WasmRenderer`'s from-scratch
//! reimplementation of surface/pixel semantics actually matches real SDL behavior, growing
//! alongside the `Renderer` trait as Phase A's per-file passes land.

use std::sync::{Mutex, Once};

use crate::platform::wasm::WasmRenderer;
use crate::platform::Renderer;
use crate::{SDL_Color, SDL_Rect};

static SDL_VIDEO_INIT: Once = Once::new();

/// `cargo test` runs test functions concurrently on separate threads by default. Real SDL2
/// calls (via the shared `SdlRenderer` singleton, `&'static mut` and hence not naturally
/// serialized) aren't safe to call from two threads at once -- observed as a SIGSEGV when
/// this file had two tests both touching `shared_renderer()` in parallel. Every test in
/// this file must hold this lock for its whole body.
static SDL_TEST_LOCK: Mutex<()> = Mutex::new(());

fn init_sdl_headless() {
    SDL_VIDEO_INIT.call_once(|| unsafe {
        std::env::set_var("SDL_VIDEODRIVER", "dummy");
        let rc = sdl2::sys::SDL_Init(sdl2::sys::SDL_INIT_VIDEO);
        assert_eq!(rc, 0, "SDL_Init(SDL_INIT_VIDEO) failed under SDL_VIDEODRIVER=dummy");
    });
}

/// A fixed, deterministic sequence of `Renderer` calls exercising every surface/pixel
/// method Phase A has added or relocated so far: creating indexed and truecolor surfaces,
/// palette manipulation, `fill_rect`, `map_rgb`/`map_rgba`, color-keyed `blit`, and the five
/// accessor methods. Returns the final destination surface's raw pixel bytes. Extend this
/// scene (don't replace it) as later Phase A passes add coverage for more of the trait.
unsafe fn draw_test_scene<R: Renderer>(r: &mut R) -> Vec<u8> {
    // 4x4 8bpp indexed source surface: palette index 5 mapped to an arbitrary color,
    // filled entirely with that index, then one pixel overwritten with a color-keyed
    // "transparent" index that should NOT show up in the destination after blit.
    let src = r.create_surface(4, 4, 8, 0, 0, 0, 0);
    let color = SDL_Color { r: 10, g: 20, b: 30, a: 255 };
    r.set_palette(src, &color as *const SDL_Color, 5, 1);
    let key = SDL_Color { r: 0, g: 0, b: 0, a: 0 };
    r.set_palette(src, &key as *const SDL_Color, 7, 1);
    r.fill_rect(src, std::ptr::null(), 5);
    // Punch one color-keyed hole at (1,1).
    let hole_rect = SDL_Rect { x: 1, y: 1, w: 1, h: 1 };
    r.fill_rect(src, &hole_rect as *const SDL_Rect, 7);
    r.set_color_key(src, true, 7);

    // 4x4 8bpp indexed destination, pre-filled with a sentinel index so we can see the
    // color-keyed hole survive the blit untouched. Real SDLPoP always keeps blitted
    // indexed surfaces' palettes synced (screen/sprite chtabs share one global palette) --
    // without that, SDL_BlitSurface between two indexed surfaces with *different* palettes
    // does palette-aware RGB color-matching rather than a raw index copy (discovered via
    // this test: an unsynced dst produced an all-zero result, since every color "matched
    // closest" to dst's all-black default palette). Mirror real usage with
    // set_surface_palette so a raw-index-copy blit is actually what's under test.
    let dst = r.create_surface(4, 4, 8, 0, 0, 0, 0);
    let src_palette = r.surface_palette(src);
    r.set_surface_palette(dst, src_palette);
    r.fill_rect(dst, std::ptr::null(), 9);

    let full = SDL_Rect { x: 0, y: 0, w: 4, h: 4 };
    let mut dst_rect = full;
    let blit_result = r.blit(src, &full as *const SDL_Rect, dst, &mut dst_rect as *mut SDL_Rect);
    assert_eq!(blit_result, 0, "blit failed");

    let (w, h) = r.surface_size(dst);
    let pitch = r.surface_pitch(dst);
    assert_eq!((w, h), (4, 4));
    assert_eq!(pitch, 4);

    let fmt = r.surface_format_info(dst);
    assert_eq!(fmt.bits_per_pixel, 8);
    assert_eq!(fmt.bytes_per_pixel, 1);

    let pixels_ptr = r.surface_pixels(dst) as *const u8;
    let pixels = std::slice::from_raw_parts(pixels_ptr, (pitch * h) as usize).to_vec();

    r.free_surface(src);
    r.free_surface(dst);
    pixels
}

/// Same scene, but for a 32bpp truecolor surface -- exercises `map_rgb`/`map_rgba`'s pixel
/// packing (the masks used here match what `seg009.rs` actually passes to
/// `create_surface`/`convert_surface_format` for its ARGB8888 conversions).
unsafe fn draw_truecolor_scene<R: Renderer>(r: &mut R) -> Vec<u8> {
    let (rmask, gmask, bmask, amask): (u32, u32, u32, u32) = (0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000);
    let surf = r.create_surface(2, 2, 32, rmask, gmask, bmask, amask);

    // map_rgb/map_rgba need a *const SDL_PixelFormat; build one matching the surface's own
    // masks (mirrors how call sites do `(*surf).format` today, pre-Phase-A -- post-Phase-A
    // callers use `surface_format_info` instead, exercised in the indexed scene above).
    // Real SDL_MapRGB/MapRGBA use Rshift/Rloss (not just the masks) to pack a value --
    // Rloss is 0 here since every mask below is a full byte-aligned 8-bit channel.
    let info = r.surface_format_info(surf);
    let shift_for = |mask: u32| if mask == 0 { 0 } else { mask.trailing_zeros() as u8 };
    let sdl_format = crate::SDL_PixelFormat {
        format: 0,
        palette: std::ptr::null_mut(),
        BitsPerPixel: info.bits_per_pixel,
        BytesPerPixel: info.bytes_per_pixel,
        padding: [0; 2],
        Rmask: info.rmask,
        Gmask: info.gmask,
        Bmask: info.bmask,
        Amask: info.amask,
        Rloss: 0, Gloss: 0, Bloss: 0, Aloss: 0,
        Rshift: shift_for(info.rmask), Gshift: shift_for(info.gmask), Bshift: shift_for(info.bmask), Ashift: shift_for(info.amask),
        refcount: 1,
        next: std::ptr::null_mut(),
    };
    let color = r.map_rgba(&sdl_format as *const crate::SDL_PixelFormat, 200, 100, 50, 255);
    r.fill_rect(surf, std::ptr::null(), color);

    let pitch = r.surface_pitch(surf);
    let pixels_ptr = r.surface_pixels(surf) as *const u8;
    let pixels = std::slice::from_raw_parts(pixels_ptr, (pitch * 2) as usize).to_vec();
    r.free_surface(surf);
    pixels
}

#[test]
fn indexed_surface_scene_matches_between_sdl_and_wasm_renderers() {
    let _guard = SDL_TEST_LOCK.lock().unwrap();
    init_sdl_headless();
    let sdl_pixels = unsafe { draw_test_scene(crate::platform::sdl::shared_renderer()) };
    let wasm_pixels = unsafe { draw_test_scene(&mut WasmRenderer) };
    assert_eq!(sdl_pixels, wasm_pixels, "indexed-surface blit/fill_rect/color-key output diverged between SdlRenderer and WasmRenderer");

    // Sanity check the actual values, not just cross-backend equality -- both backends
    // could agree while both being wrong. (1,1) is the color-keyed hole in src (index 7):
    // it should NOT get copied, so dst keeps its own pre-fill sentinel (9) there.
    let mut want = vec![5u8; 16];
    want[4 + 1] = 9;
    assert_eq!(sdl_pixels, want, "unexpected pixel values from SdlRenderer itself");
}

#[test]
fn truecolor_surface_scene_matches_between_sdl_and_wasm_renderers() {
    let _guard = SDL_TEST_LOCK.lock().unwrap();
    init_sdl_headless();
    let sdl_pixels = unsafe { draw_truecolor_scene(crate::platform::sdl::shared_renderer()) };
    let wasm_pixels = unsafe { draw_truecolor_scene(&mut WasmRenderer) };
    assert_eq!(sdl_pixels, wasm_pixels, "truecolor map_rgba/fill_rect output diverged between SdlRenderer and WasmRenderer");

    // Expected bytes for ARGB8888 (little-endian in-memory: B,G,R,A) mapping (200,100,50,255).
    let want: Vec<u8> = vec![50, 100, 200, 255, 50, 100, 200, 255, 50, 100, 200, 255, 50, 100, 200, 255];
    assert_eq!(sdl_pixels, want, "unexpected pixel values from SdlRenderer itself");
}
