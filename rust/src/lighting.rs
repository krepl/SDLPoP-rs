#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int};
use super::*;

macro_rules! cs {
    ($s:literal) => {
        concat!($s, "\0").as_ptr() as *const c_char
    };
}

extern "C" {
    fn IMG_Load(file: *const c_char) -> *mut SDL_Surface;
    fn SDL_CreateRGBSurface(
        flags: u32,
        width: c_int,
        height: c_int,
        depth: c_int,
        Rmask: u32,
        Gmask: u32,
        Bmask: u32,
        Amask: u32,
    ) -> *mut SDL_Surface;
    fn SDL_SetSurfaceBlendMode(surface: *mut SDL_Surface, blend_mode: c_int) -> c_int;
    fn SDL_FillRect(dst: *mut SDL_Surface, rect: *const SDL_Rect, color: u32) -> c_int;
    fn SDL_MapRGBA(format: *const SDL_PixelFormat, r: u8, g: u8, b: u8, a: u8) -> u32;
    // SDL_BlitSurface is a macro for SDL_UpperBlit.
    fn SDL_UpperBlit(
        src: *mut SDL_Surface,
        srcrect: *const SDL_Rect,
        dst: *mut SDL_Surface,
        dstrect: *mut SDL_Rect,
    ) -> c_int;
}

#[inline]
unsafe fn SDL_BlitSurface(
    src: *mut SDL_Surface,
    srcrect: *const SDL_Rect,
    dst: *mut SDL_Surface,
    dstrect: *mut SDL_Rect,
) -> c_int {
    SDL_UpperBlit(src, srcrect, dst, dstrect)
}

const SDL_BLENDMODE_ADD: c_int = 2;
const SDL_BLENDMODE_MOD: c_int = 4;
const SDL_ALPHA_OPAQUE: u8 = 255;

static mut screen_overlay: *mut image_type = core::ptr::null_mut();
static mut bgcolor: u32 = 0;

const mask_filename: *const c_char = cs!("data/light.png");
const ambient_level: u8 = 128;

// Called once at startup.
#[no_mangle]
pub unsafe extern "C" fn init_lighting() {
    if enable_lighting == 0 {
        return;
    }

    let mut __lf = [0 as c_char; POP_MAX_PATH as usize];
    lighting_mask = IMG_Load(locate_file_(
        mask_filename,
        __lf.as_mut_ptr(),
        POP_MAX_PATH as c_int,
    ));
    if lighting_mask.is_null() {
        sdlperror(cs!("IMG_Load (lighting_mask)"));
        enable_lighting = 0;
        return;
    }

    screen_overlay = SDL_CreateRGBSurface(0, 320, 192, 32, Rmsk, Gmsk, Bmsk, Amsk);
    if screen_overlay.is_null() {
        sdlperror(cs!("SDL_CreateRGBSurface (screen_overlay)"));
        enable_lighting = 0;
        return;
    }

    // "color modulate", i.e. multiply.
    let mut result = SDL_SetSurfaceBlendMode(screen_overlay, SDL_BLENDMODE_MOD);
    if result != 0 {
        sdlperror(cs!("SDL_SetSurfaceBlendMode (screen_overlay)"));
    }

    result = SDL_SetSurfaceBlendMode(lighting_mask, SDL_BLENDMODE_ADD);
    if result != 0 {
        sdlperror(cs!("SDL_SetSurfaceBlendMode (lighting_mask)"));
    }

    // ambient lighting
    bgcolor = SDL_MapRGBA(
        (*screen_overlay).format,
        ambient_level,
        ambient_level,
        ambient_level,
        SDL_ALPHA_OPAQUE,
    );
}

// Recreate the lighting overlay based on the torches in the current room.
// Called when the current room changes.
#[no_mangle]
pub unsafe extern "C" fn redraw_lighting() {
    if enable_lighting == 0 {
        return;
    }
    if lighting_mask.is_null() {
        return;
    }
    if curr_room_tiles.is_null() {
        return;
    }
    if is_cutscene != 0 {
        return;
    }

    let result = SDL_FillRect(screen_overlay, core::ptr::null(), bgcolor);
    if result != 0 {
        sdlperror(cs!("SDL_FillRect (screen_overlay)"));
    }

    // TODO: Also process nearby offscreen torches?
    for tile_pos in 0..30 {
        let tile_type = (*curr_room_tiles.add(tile_pos as usize) & 0x1F) as c_int;
        if tile_type == tiles_tiles_19_torch as c_int
            || tile_type == tiles_tiles_30_torch_with_debris as c_int
        {
            // Center of the flame.
            let x = (tile_pos % 10) * 32 + 48;
            let y = (tile_pos / 10) * 63 + 22;

            // Align the center of lighting mask to the center of the flame.
            let mut dest_rect: SDL_Rect = core::mem::zeroed();
            dest_rect.x = x - (*lighting_mask).w / 2;
            dest_rect.y = y - (*lighting_mask).h / 2;
            dest_rect.w = (*lighting_mask).w;
            dest_rect.h = (*lighting_mask).h;

            let result = SDL_BlitSurface(
                lighting_mask,
                core::ptr::null(),
                screen_overlay,
                &mut dest_rect,
            );
            if result != 0 {
                sdlperror(cs!("SDL_BlitSurface (lighting_mask)"));
            }
        }
    }
    if upside_down != 0 {
        flip_screen(screen_overlay);
    }
}

// Copy a part of the lighting overlay onto the screen.
// Called when the screen is updated.
#[no_mangle]
pub unsafe extern "C" fn update_lighting(target_rect_ptr: *const rect_type) {
    if enable_lighting == 0 {
        return;
    }
    if lighting_mask.is_null() {
        return;
    }
    if curr_room_tiles.is_null() {
        return;
    }
    if is_cutscene != 0 {
        return;
    }

    let mut sdlrect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(target_rect_ptr, &mut sdlrect);
    let p = &mut sdlrect as *mut SDL_Rect;
    let result = SDL_BlitSurface(screen_overlay, p, onscreen_surface_, p);
    if result != 0 {
        sdlperror(cs!("SDL_BlitSurface (screen_overlay)"));
    }
}
