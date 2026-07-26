//! Torchlight — the optional effect that darkens a room and puts a pool of
//! light around each burning torch.
//!
//! The dungeon is normally drawn at full brightness. With lighting enabled
//! (`enable_lighting`, off by default and settable from `SDLPoP.ini`) the game
//! keeps a second 320×192 surface, the *screen overlay*, which is multiplied
//! over the finished frame. Where the overlay is dark grey the room is dimmed;
//! where it is white the room shows through unchanged.
//!
//! The overlay is built from two pieces:
//!
//! * a flat [`ambient_level`] grey fill, which is how bright an unlit part of
//!   the room ends up, and
//! * one copy of `data/light.png` — the *lighting mask*, a soft white blob —
//!   additively blended over the fill at the centre of every torch flame in the
//!   room. Overlapping torches therefore brighten each other, and enough of
//!   them saturate back to plain white.
//!
//! The two SDL blend modes carry all of that: the mask is `SDL_BLENDMODE_ADD`
//! so torches accumulate into the overlay, and the overlay itself is
//! `SDL_BLENDMODE_MOD` ("color modulate", i.e. multiply) so it dims the screen
//! rather than painting over it.
//!
//! Because the overlay only depends on where the torches are, it is rebuilt
//! just once per room change ([`redraw_lighting`]) and then merely blitted onto
//! whatever rectangle of the screen is being refreshed ([`update_lighting`]).
//! Cutscenes are excluded: they are not rooms, so `curr_room_tiles` does not
//! describe what is on screen.
//!
//! Ported from `lighting.c` (`USE_LIGHTING`).

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int};
use super::*;
use crate::platform::Renderer;

macro_rules! cs {
    ($s:literal) => {
        concat!($s, "\0").as_ptr() as *const c_char
    };
}

#[inline]
unsafe fn SDL_BlitSurface(
    src: *mut SDL_Surface,
    srcrect: *const SDL_Rect,
    dst: *mut SDL_Surface,
    dstrect: *mut SDL_Rect,
) -> c_int {
    crate::platform::sdl::shared_renderer().blit(src, srcrect, dst, dstrect)
}

/// Source and destination are added: torches accumulate into the overlay.
const SDL_BLENDMODE_ADD: c_int = 2;
/// Source and destination are multiplied: the overlay dims the screen.
const SDL_BLENDMODE_MOD: c_int = 4;
const SDL_ALPHA_OPAQUE: u8 = 255;

/// The 320×192 multiply overlay composited over the finished frame. Rebuilt by
/// [`redraw_lighting`] on every room change.
static mut screen_overlay: *mut image_type = core::ptr::null_mut();
/// Packed pixel value of the ambient fill, in `screen_overlay`'s pixel format.
static mut bgcolor: u32 = 0;

/// The soft white blob blitted at each torch flame.
const mask_filename: *const c_char = cs!("data/light.png");
/// Brightness of a part of the room no torch reaches, out of 255.
const ambient_level: u8 = 128;

/// Whether the overlay currently describes what is on screen.
///
/// Lighting has to be switched on, both surfaces have to exist, and there has
/// to be a room to light — during a cutscene `curr_room_tiles` does not
/// describe the visible screen, so the overlay would be stale.
unsafe fn lighting_applies() -> bool {
    enable_lighting != 0
        && !lighting_mask.is_null()
        && !curr_room_tiles.is_null()
        && is_cutscene == 0
}

/// Load the lighting mask and build the overlay surface. Called once at startup.
///
/// Any failure here switches `enable_lighting` back off, so the rest of the
/// module simply never runs; the game then renders undimmed.
#[no_mangle]
pub unsafe extern "C" fn init_lighting() {
    if enable_lighting == 0 {
        return;
    }

    let mut __lf = [0 as c_char; POP_MAX_PATH as usize];
    let mask_path = locate_file_(mask_filename, __lf.as_mut_ptr(), POP_MAX_PATH as c_int);
    lighting_mask = crate::platform::sdl::shared_renderer().load_image_from_file(std::ffi::CStr::from_ptr(mask_path));
    if lighting_mask.is_null() {
        sdlperror(cs!("IMG_Load (lighting_mask)"));
        enable_lighting = 0;
        return;
    }

    screen_overlay = crate::platform::sdl::shared_renderer().create_surface(320, 192, 32, Rmsk, Gmsk, Bmsk, Amsk);
    if screen_overlay.is_null() {
        sdlperror(cs!("SDL_CreateRGBSurface (screen_overlay)"));
        enable_lighting = 0;
        return;
    }

    // "color modulate", i.e. multiply.
    let mut result = crate::platform::sdl::shared_renderer().set_blend_mode(screen_overlay, SDL_BLENDMODE_MOD);
    if result != 0 {
        sdlperror(cs!("SDL_SetSurfaceBlendMode (screen_overlay)"));
    }

    result = crate::platform::sdl::shared_renderer().set_blend_mode(lighting_mask, SDL_BLENDMODE_ADD);
    if result != 0 {
        sdlperror(cs!("SDL_SetSurfaceBlendMode (lighting_mask)"));
    }

    // ambient lighting
    bgcolor = crate::platform::sdl::shared_renderer().map_rgba(
        (*screen_overlay).format,
        ambient_level,
        ambient_level,
        ambient_level,
        SDL_ALPHA_OPAQUE,
    );
}

/// Recreate the lighting overlay based on the torches in the current room.
/// Called when the current room changes.
///
/// The overlay is filled with the ambient grey, then one copy of the lighting
/// mask is added at the flame of each of the room's torches. `flip_screen` is
/// applied last so the overlay matches an upside-down (level 12) screen.
#[no_mangle]
pub unsafe extern "C" fn redraw_lighting() {
    if !lighting_applies() {
        return;
    }

    let result = crate::platform::sdl::shared_renderer().fill_rect(screen_overlay, core::ptr::null(), bgcolor);
    if result != 0 {
        sdlperror(cs!("SDL_FillRect (screen_overlay)"));
    }

    // TODO: Also process nearby offscreen torches?
    for tile_pos in 0..30usize {
        // The high three bits of a tile byte are flags, not part of the id.
        let tile_type = (*curr_room_tiles.add(tile_pos) & 0x1F) as tiles;
        if !matches!(
            tile_type,
            tiles_tiles_19_torch | tiles_tiles_30_torch_with_debris
        ) {
            continue;
        }

        // Center of the flame, in the room's 10-column by 3-row tile grid.
        let x = (tile_pos % 10) as c_int * 32 + 48;
        let y = (tile_pos / 10) as c_int * 63 + 22;

        // Align the center of lighting mask to the center of the flame.
        let mut dest_rect = SDL_Rect {
            x: x - (*lighting_mask).w / 2,
            y: y - (*lighting_mask).h / 2,
            w: (*lighting_mask).w,
            h: (*lighting_mask).h,
        };

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
    if upside_down != 0 {
        flip_screen(screen_overlay);
    }
}

/// Copy a part of the lighting overlay onto the screen.
/// Called when the screen is updated.
///
/// The overlay's `SDL_BLENDMODE_MOD` makes this a multiply, dimming the given
/// rectangle by however much light reaches it. Source and destination
/// rectangles are the same object, exactly as in the C: overlay and screen
/// share coordinates, and SDL clips the blit through that one rect.
#[no_mangle]
pub unsafe extern "C" fn update_lighting(target_rect_ptr: *const rect_type) {
    if !lighting_applies() {
        return;
    }

    let mut sdlrect: SDL_Rect = core::mem::zeroed();
    rect_to_sdlrect(target_rect_ptr, &mut sdlrect);
    let rect = &mut sdlrect as *mut SDL_Rect;
    let result = SDL_BlitSurface(screen_overlay, rect, onscreen_surface_, rect);
    if result != 0 {
        sdlperror(cs!("SDL_BlitSurface (screen_overlay)"));
    }
}
