// Room renderer — ported from seg008.c.
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short, c_char, c_void};
use super::*;

extern "C" {
    fn SDL_ConvertSurface(src: *mut SDL_Surface, fmt: *const SDL_PixelFormat, flags: u32) -> *mut SDL_Surface;
    fn SDL_SetSurfacePalette(surface: *mut SDL_Surface, palette: *mut SDL_Palette) -> c_int;
    fn SDL_SetSurfaceBlendMode(surface: *mut SDL_Surface, blendMode: c_int) -> c_int;
    fn SDL_SetColorKey(surface: *mut SDL_Surface, flag: c_int, key: u32) -> c_int;
    fn SDL_SetSurfaceAlphaMod(surface: *mut SDL_Surface, alpha: u8) -> c_int;
    fn SDL_UpperBlit(src: *mut SDL_Surface, srcrect: *const SDL_Rect, dst: *mut SDL_Surface, dstrect: *mut SDL_Rect) -> c_int;
    fn SDL_FreeSurface(surface: *mut SDL_Surface);
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void;
}

// File-local statics (seg008.c data section).
static mut drawn_row: c_short = 0;
static mut draw_bottom_y: c_short = 0;
static mut draw_main_y: c_short = 0;
static mut drawn_col: c_short = 0;
static mut tile_left: u8 = 0;
static mut modifier_left: u8 = 0;
static mut gate_top_y: u16 = 0;
static mut gate_openness: u16 = 0;
static mut gate_bottom_y: u16 = 0;

type add_table_fn = unsafe extern "C" fn(c_short, c_int, i8, i8, c_int, c_int, u8) -> c_int;
static mut ptr_add_table: add_table_fn = add_backtable;

// data:259C
static tile_table: [piece; 31] = [
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  0, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x00 empty
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x01 floor
    piece { base_id: 127, floor_left: 1, base_y:   0, right_id: 133, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x02 spike
    piece { base_id:  92, floor_left: 1, base_y:   0, right_id:  93, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:  94, bottom_id: 43, fore_id:  95, fore_x: 1, fore_y:   0 }, // 0x03 pillar
    piece { base_id:  46, floor_left: 1, base_y:   0, right_id:  47, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:  48, bottom_id: 43, fore_id:  49, fore_x: 3, fore_y:   0 }, // 0x04 door
    piece { base_id:  41, floor_left: 1, base_y:   1, right_id:  35, floor_right: 1, right_y:  3, stripe_id: 145, topright_id:   0, bottom_id: 36, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x05 stuck floor
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 96, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x06 close button
    piece { base_id:  46, floor_left: 1, base_y:   0, right_id:   0, floor_right: 0, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:  49, fore_x: 3, fore_y:   0 }, // 0x07 door top with floor
    piece { base_id:  86, floor_left: 1, base_y:   0, right_id:  87, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:  88, fore_x: 1, fore_y:   0 }, // 0x08 big pillar bottom
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:  89, floor_right: 0, right_y:  3, stripe_id:   0, topright_id:  90, bottom_id:  0, fore_id:  91, fore_x: 1, fore_y:   3 }, // 0x09 big pillar top
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:  12, fore_x: 2, fore_y:  -3 }, // 0x0A potion
    piece { base_id:   0, floor_left: 1, base_y:   0, right_id:   0, floor_right: 0, right_y:  0, stripe_id: 145, topright_id:   0, bottom_id:  0, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x0B loose floor
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:   0, floor_right: 0, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 85, fore_id:  49, fore_x: 3, fore_y:   0 }, // 0x0C door top
    piece { base_id:  75, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:  77, fore_x: 0, fore_y:   0 }, // 0x0D mirror
    piece { base_id:  97, floor_left: 1, base_y:   0, right_id:  98, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id: 100, fore_x: 0, fore_y:   0 }, // 0x0E debris
    piece { base_id: 147, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  1, stripe_id: 145, topright_id:   0, bottom_id:149, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x0F open button
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  37, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:  38, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x10 leveldoor left
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:  39, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:  40, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x11 leveldoor right
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x12 chomper
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x13 torch
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:   1, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   2, bottom_id:  0, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x14 wall
    piece { base_id:  30, floor_left: 1, base_y:   0, right_id:  31, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x15 skeleton
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x16 sword
    piece { base_id:  41, floor_left: 1, base_y:   0, right_id:  10, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:  11, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x17 balcony left
    piece { base_id:   0, floor_left: 0, base_y:   0, right_id:  12, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:  13, bottom_id: 43, fore_id:   0, fore_x: 0, fore_y:   0 }, // 0x18 balcony right
    piece { base_id:  92, floor_left: 1, base_y:   0, right_id:  42, floor_right: 1, right_y:  2, stripe_id: 145, topright_id:   0, bottom_id: 43, fore_id:  95, fore_x: 1, fore_y:   0 }, // 0x19 lattice pillar
    piece { base_id:   1, floor_left: 0, base_y:   0, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  2, fore_id:   9, fore_x: 0, fore_y: -53 }, // 0x1A lattice down
    piece { base_id:   3, floor_left: 0, base_y: -10, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  0, fore_id:   9, fore_x: 0, fore_y: -53 }, // 0x1B lattice small
    piece { base_id:   4, floor_left: 0, base_y: -10, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  0, fore_id:   9, fore_x: 0, fore_y: -53 }, // 0x1C lattice left
    piece { base_id:   5, floor_left: 0, base_y: -10, right_id:   0, floor_right: 0, right_y:  0, stripe_id:   0, topright_id:   0, bottom_id:  0, fore_id:   9, fore_x: 0, fore_y: -53 }, // 0x1D lattice right
    piece { base_id:  97, floor_left: 1, base_y:   0, right_id:  98, floor_right: 1, right_y:  2, stripe_id:   0, topright_id:   0, bottom_id: 43, fore_id: 100, fore_x: 0, fore_y:   0 }, // 0x1E debris with torch
];

// data:24C6
static col_xh: [u16; 10] = [0, 4, 8, 12, 16, 20, 24, 28, 32, 36];

static doortop_fram_top: [u8; 4] = [0, 81, 83, 0];
static door_fram_top: [u8; 8] = [60, 61, 62, 63, 64, 65, 66, 67];
static blueline_fram1: [u8; 4] = [0, 124, 125, 126];
static blueline_fram_y: [i8; 4] = [0, -20, -20, 0];
static blueline_fram3: [u8; 4] = [44, 44, 45, 45];
static doortop_fram_bot: [u8; 4] = [78, 80, 82, 0];
static spikes_fram_right: [u8; 10] = [0, 134, 135, 136, 137, 138, 137, 135, 134, 0];
static loose_fram_right: [u8; 12] = [42, 71, 42, 72, 72, 42, 42, 42, 72, 72, 72, 0];
static wall_fram_bottom: [u8; 4] = [7, 9, 5, 3];
static loose_fram_bottom: [u8; 12] = [43, 73, 43, 74, 74, 43, 43, 43, 74, 74, 74, 0];
static loose_fram_left: [u8; 12] = [41, 69, 41, 70, 70, 41, 41, 41, 70, 70, 70, 0];
static spikes_fram_left: [u8; 10] = [0, 128, 129, 130, 131, 132, 131, 129, 128, 0];
static potion_fram_bubb: [u8; 8] = [0, 16, 17, 18, 19, 20, 21, 22];
static chomper_fram1: [u8; 8] = [3, 2, 0, 1, 4, 3, 3, 0];
static chomper_fram_bot: [u8; 6] = [101, 102, 103, 104, 105, 0];
static chomper_fram_top: [u8; 6] = [0, 0, 111, 112, 113, 0];
static chomper_fram_y: [u8; 5] = [0, 0, 0x25, 0x2F, 0x32];
static spikes_fram_fore: [u8; 10] = [0, 139, 140, 141, 142, 143, 142, 140, 139, 0];
static chomper_fram_for: [u8; 6] = [106, 107, 108, 109, 110, 0];
static wall_fram_main: [u8; 4] = [8, 10, 6, 4];
static door_fram_slice: [u8; 9] = [67, 59, 58, 57, 56, 55, 54, 53, 52];
static floor_left_overlay: [u16; 8] = [32, 151, 151, 150, 150, 151, 32, 32];

// tbl_line is an incomplete array in seg006; access via raw pointer.
unsafe fn tbl_line_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(tbl_line).cast::<u16>().add(idx)
}

// copyprot_room and copyprot_tile are incomplete arrays.
unsafe fn copyprot_room_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_room).cast::<u16>().add(idx)
}
unsafe fn copyprot_tile_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(copyprot_tile).cast::<u16>().add(idx)
}

// Shadow-sprite rendering, shared between case 0/4 (goto shadow) and case 1.
unsafe fn render_shadow_sprite() {
    if united_with_shadow == 2 {
        play_sound(soundids_sound_41_end_level_music as c_int);
    }
    add_midtable(
        obj_chtab as c_short, obj_id as c_int + 1,
        obj_xh as i8, obj_xl as i8, obj_y as c_int,
        blitters_blitters_2_or as c_int, 1,
    );
    add_midtable(
        obj_chtab as c_short, obj_id as c_int + 1,
        obj_xh as i8, obj_xl as i8 + 1, obj_y as c_int,
        blitters_blitters_3_xor as c_int, 1,
    );
}

// The shared wall-connection block reached by `goto label_wall_continued` in C.
// Called from load_alter_mod for wall tiles (after modifier adjustment) and for
// fake-wall floor/empty tiles.
unsafe fn wall_connection_block(tilepos: usize, curr_tile_modif: *mut u8, tiletype: u8) {
    let wall_connection = |adj_tile: u8, adj_tile_modif: u8| -> bool {
        (adj_tile == tiles_tiles_20_wall as u8
            && adj_tile_modif != 4 && (adj_tile_modif >> 4) != 4
            && adj_tile_modif != 6 && (adj_tile_modif >> 4) != 6)
        || (adj_tile == tiles_tiles_0_empty as u8
            && (adj_tile_modif == 5 || adj_tile_modif == 13
                || (adj_tile_modif >= 50 && adj_tile_modif <= 53)))
        || (adj_tile == tiles_tiles_1_floor as u8
            && (adj_tile_modif == 5 || adj_tile_modif == 13
                || (adj_tile_modif >= 50 && adj_tile_modif <= 53)))
    };

    if graphics_mode != grmodes_gmCga as u8 && graphics_mode != grmodes_gmHgaHerc as u8 {
        let mut wall_to_right: u32 = 1;
        let mut wall_to_left: u32 = 1;

        // Check left neighbor
        if tilepos % 10 == 0 {
            if room_L != 0 {
                let adj_tile_index = 30 * (room_L as usize - 1) + tilepos + 9;
                let adj_tile = level.fg[adj_tile_index] & 0x1F;
                let adj_tile_modif = level.bg[adj_tile_index];
                wall_to_left = wall_connection(adj_tile, adj_tile_modif) as u32;
            }
        } else {
            let adj_tile_index = tilepos - 1;
            let adj_tile = (*curr_room_tiles.add(adj_tile_index)) & 0x1F;
            let adj_tile_modif = *curr_room_modif.add(adj_tile_index);
            wall_to_left = wall_connection(adj_tile, adj_tile_modif) as u32;
        }

        // Check right neighbor
        if tilepos % 10 == 9 {
            if room_R != 0 {
                let adj_tile_index = 30 * (room_R as usize - 1) + tilepos - 9;
                let adj_tile = level.fg[adj_tile_index] & 0x1F;
                let adj_tile_modif = level.bg[adj_tile_index];
                wall_to_right = wall_connection(adj_tile, adj_tile_modif) as u32;
            }
        } else {
            let adj_tile_index = tilepos + 1;
            let adj_tile = (*curr_room_tiles.add(adj_tile_index)) & 0x1F;
            let adj_tile_modif = *curr_room_modif.add(adj_tile_index);
            wall_to_right = wall_connection(adj_tile, adj_tile_modif) as u32;
        }

        // USE_FAKE_TILES is always on: fake-wall floor/empty tiles get 50-53
        if tiletype == tiles_tiles_1_floor as u8 || tiletype == tiles_tiles_0_empty as u8 {
            if wall_to_left != 0 && wall_to_right != 0 {
                *curr_tile_modif = 53;
            } else if wall_to_left != 0 {
                *curr_tile_modif = 52;
            } else if wall_to_right != 0 {
                *curr_tile_modif = 51;
            }
            return;
        }

        if wall_to_left != 0 && wall_to_right != 0 {
            *curr_tile_modif |= 3;
        } else if wall_to_left != 0 {
            *curr_tile_modif |= 2;
        } else if wall_to_right != 0 {
            *curr_tile_modif |= 1;
        }
    } else {
        *curr_tile_modif = 3;
    }
}

// seg008:0006
#[no_mangle]
pub unsafe extern "C" fn redraw_room() {
    free_peels();
    memset(table_counts.as_mut_ptr() as *mut c_void, 0, core::mem::size_of_val(&table_counts));
    reset_obj_clip();
    draw_room();
    clear_tile_wipes();
}

// seg008:0035
#[no_mangle]
pub unsafe extern "C" fn load_room_links() {
    room_BR = 0;
    room_BL = 0;
    room_AR = 0;
    room_AL = 0;
    if drawn_room != 0 {
        get_room_address(drawn_room as c_int);
        room_L = level.roomlinks[drawn_room as usize - 1].left as u16;
        room_R = level.roomlinks[drawn_room as usize - 1].right as u16;
        room_A = level.roomlinks[drawn_room as usize - 1].up as u16;
        room_B = level.roomlinks[drawn_room as usize - 1].down as u16;
        if room_A != 0 {
            room_AL = level.roomlinks[room_A as usize - 1].left as u16;
            room_AR = level.roomlinks[room_A as usize - 1].right as u16;
        } else {
            if room_L != 0 {
                room_AL = level.roomlinks[room_L as usize - 1].up as u16;
            }
            if room_R != 0 {
                room_AR = level.roomlinks[room_R as usize - 1].up as u16;
            }
        }
        if room_B != 0 {
            room_BL = level.roomlinks[room_B as usize - 1].left as u16;
            room_BR = level.roomlinks[room_B as usize - 1].right as u16;
        } else {
            if room_L != 0 {
                room_BL = level.roomlinks[room_L as usize - 1].down as u16;
            }
            if room_R != 0 {
                room_BR = level.roomlinks[room_R as usize - 1].down as u16;
            }
        }
    } else {
        room_B = 0;
        room_A = 0;
        room_R = 0;
        room_L = 0;
    }
}

// seg008:0125
#[no_mangle]
pub unsafe extern "C" fn draw_room() {
    load_leftroom();
    for dr in (0i16..3).rev() {
        drawn_row = dr;
        load_rowbelow();
        draw_bottom_y = 63 * drawn_row + 65;
        draw_main_y = draw_bottom_y - 3;
        for dc in 0i16..10 {
            drawn_col = dc;
            load_curr_and_left_tile();
            draw_tile();
        }
    }
    let saved_room = drawn_room;
    drawn_room = room_A;
    load_room_links();
    load_leftroom();
    drawn_row = 2;
    load_rowbelow();
    for dc in 0i16..10 {
        drawn_col = dc;
        load_curr_and_left_tile();
        draw_main_y = -1;
        draw_bottom_y = 2;
        draw_tile_aboveroom();
    }
    drawn_room = saved_room;
    load_room_links();
}

// seg008:01C7
#[no_mangle]
pub unsafe extern "C" fn draw_tile() {
    draw_tile_floorright();
    draw_tile_anim_topright();
    draw_tile_right();
    draw_tile_anim_right();
    draw_tile_bottom(0);
    draw_loose(0);
    draw_tile_base();
    draw_tile_anim();
    draw_tile_fore();
}

// seg008:01F2
#[no_mangle]
pub unsafe extern "C" fn draw_tile_aboveroom() {
    draw_tile_floorright();
    draw_tile_anim_topright();
    draw_tile_right();
    draw_tile_bottom(0);
    draw_loose(0);
    draw_tile_fore();
}

// seg008:02FE
#[no_mangle]
pub unsafe extern "C" fn get_tile_to_draw(
    room: c_int, column: c_int, row: c_int,
    ptr_tiletype: *mut u8, ptr_modifier: *mut u8,
    tile_room0: u8,
) -> c_int {
    let tilepos = (tbl_line_at(row as usize) as usize).wrapping_add(column as usize);
    if column == -1 {
        *ptr_tiletype = leftroom_[row as usize].tiletype;
        *ptr_modifier = leftroom_[row as usize].modifier;
    } else if room != 0 {
        *ptr_tiletype = *curr_room_tiles.add(tilepos) & 0x1F;
        *ptr_modifier = *curr_room_modif.add(tilepos);
    } else {
        *ptr_modifier = 0;
        *ptr_tiletype = tile_room0;
    }

    let tiletype = (*ptr_tiletype) & 0x1F;
    let modifier = *ptr_modifier;

    if tiletype == tiles_tiles_6_closer as u8 {
        if get_doorlink_timer(modifier as c_short) > 1 {
            *ptr_tiletype = tiles_tiles_5_stuck as u8;
        }
    } else if tiletype == tiles_tiles_15_opener as u8 {
        if get_doorlink_timer(modifier as c_short) > 1 {
            *ptr_modifier = 0;
            *ptr_tiletype = tiles_tiles_1_floor as u8;
        }
    } else if tiletype == tiles_tiles_0_empty as u8 {
        // USE_FAKE_TILES (always on)
        if modifier == 4 || modifier == 12 {
            *ptr_tiletype = tiles_tiles_1_floor as u8;
            *ptr_modifier = if modifier == 12 { 1 } else { 0 };
        } else if modifier == 5 || modifier == 13 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = if modifier == 13 { 0x80 } else { 0 };
        } else if modifier == 50 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = 0;
        } else if modifier == 51 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = 1;
        } else if modifier == 52 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = 2;
        } else if modifier == 53 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = 3;
        }
    } else if tiletype == tiles_tiles_1_floor as u8 {
        if modifier == 6 || modifier == 14 {
            *ptr_tiletype = tiles_tiles_0_empty as u8;
            *ptr_modifier = if modifier == 14 { 1 } else { 0 };
        } else if modifier == 5 || modifier == 13 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = if modifier == 13 { 0x80 } else { 0 };
        } else if modifier == 50 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = 0;
        } else if modifier == 51 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = 1;
        } else if modifier == 52 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = 2;
        } else if modifier == 53 {
            *ptr_tiletype = tiles_tiles_20_wall as u8;
            *ptr_modifier = 3;
        }
    } else if tiletype == tiles_tiles_20_wall as u8 {
        if ((modifier >> 4) & 7) == 4 {
            *ptr_tiletype = tiles_tiles_1_floor as u8;
            *ptr_modifier = modifier >> 7;
        } else if ((modifier >> 4) & 7) == 6 {
            *ptr_tiletype = tiles_tiles_0_empty as u8;
            *ptr_modifier = if (modifier >> 7) != 0 { 1 } else { 0 };
        }
    } else if tiletype == tiles_tiles_11_loose as u8 {
        // FIX_LOOSE_LEFT_OF_POTION (always on)
        if (*fixes).fix_loose_left_of_potion != 0 {
            if ((*ptr_modifier) & 0x7F) == 0 {
                *ptr_tiletype = tiles_tiles_1_floor as u8;
            }
        }
    }

    *ptr_tiletype as c_int
}

// seg008:03BB
#[no_mangle]
pub unsafe extern "C" fn load_curr_and_left_tile() {
    let mut tiletype: u8 = tiles_tiles_20_wall as u8;
    if drawn_row == 2 {
        tiletype = (*custom).drawn_tile_top_level_edge;
    }
    get_tile_to_draw(drawn_room as c_int, drawn_col as c_int, drawn_row as c_int, &mut curr_tile, &mut curr_modifier, tiletype);
    get_tile_to_draw(drawn_room as c_int, drawn_col as c_int - 1, drawn_row as c_int, &mut tile_left, &mut modifier_left, tiletype);
    draw_xh = col_xh[drawn_col as usize];
}

// seg008:041A
#[no_mangle]
pub unsafe extern "C" fn load_leftroom() {
    get_room_address(room_L as c_int);
    for row in 0usize..3 {
        get_tile_to_draw(room_L as c_int, 9, row as c_int, &mut leftroom_[row].tiletype, &mut leftroom_[row].modifier, (*custom).drawn_tile_left_level_edge);
    }
}

// seg008:0460
#[no_mangle]
pub unsafe extern "C" fn load_rowbelow() {
    let row_below: usize;
    let room: u16;
    let room_left: u16;
    if drawn_row == 2 {
        room = room_B;
        room_left = room_BL;
        row_below = 0;
    } else {
        room = drawn_room;
        room_left = room_L;
        row_below = (drawn_row + 1) as usize;
    }
    get_room_address(room as c_int);
    for column in 1usize..10 {
        get_tile_to_draw(room as c_int, column as c_int - 1, row_below as c_int, &mut row_below_left_[column].tiletype, &mut row_below_left_[column].modifier, tiles_tiles_0_empty as u8);
    }
    get_room_address(room_left as c_int);
    get_tile_to_draw(room_left as c_int, 9, row_below as c_int, &mut row_below_left_[0].tiletype, &mut row_below_left_[0].modifier, tiles_tiles_20_wall as u8);
    get_room_address(drawn_room as c_int);
}

// seg008:04FA
#[no_mangle]
pub unsafe extern "C" fn draw_tile_floorright() {
    if can_see_bottomleft() == 0 { return; }
    draw_tile_topright();
    if tile_table[tile_left as usize].floor_right == 0 { return; }
    add_backtable(
        chtabs_id_chtab_6_environment as c_short,
        42, // floor right part
        draw_xh as i8, 0,
        tile_table[tiles_tiles_1_floor as usize].right_y as c_int + draw_main_y as c_int,
        blitters_blitters_9_black as c_int, 1,
    );
}

// seg008:053A
#[no_mangle]
pub unsafe extern "C" fn can_see_bottomleft() -> c_int {
    (curr_tile == tiles_tiles_0_empty as u8
        || curr_tile == tiles_tiles_9_bigpillar_top as u8
        || curr_tile == tiles_tiles_12_doortop as u8
        || curr_tile == tiles_tiles_26_lattice_down as u8) as c_int
}

// seg008:055A
#[no_mangle]
pub unsafe extern "C" fn draw_tile_topright() {
    let tiletype = row_below_left_[drawn_col as usize].tiletype;
    if tiletype == tiles_tiles_7_doortop_with_floor as u8 || tiletype == tiles_tiles_12_doortop as u8 {
        if (*custom).tbl_level_type[current_level as usize] == 0 { return; }
        add_backtable(
            chtabs_id_chtab_6_environment as c_short,
            doortop_fram_top[row_below_left_[drawn_col as usize].modifier as usize] as c_int,
            draw_xh as i8, 0, draw_bottom_y as c_int,
            blitters_blitters_2_or as c_int, 0,
        );
    } else if tiletype == tiles_tiles_20_wall as u8 {
        add_backtable(
            chtabs_id_chtab_7_environmentwall as c_short,
            2, draw_xh as i8, 0, draw_bottom_y as c_int,
            blitters_blitters_2_or as c_int, 0,
        );
    } else {
        let mut id = tile_table[tiletype as usize].topright_id as c_int;
        // USE_TELEPORTS (always on)
        if (tiletype == tiles_tiles_23_balcony_left as u8 && row_below_left_[drawn_col as usize].modifier != 0)
            || (tiletype == tiles_tiles_24_balcony_right as u8 && row_below_left_[drawn_col as usize].modifier == 1)
        {
            id += 4;
        }
        add_backtable(
            chtabs_id_chtab_6_environment as c_short,
            id, draw_xh as i8, 0, draw_bottom_y as c_int,
            blitters_blitters_2_or as c_int, 0,
        );
    }
}

// seg008:05D1
#[no_mangle]
pub unsafe extern "C" fn draw_tile_anim_topright() {
    if (curr_tile == tiles_tiles_0_empty as u8
        || curr_tile == tiles_tiles_9_bigpillar_top as u8
        || curr_tile == tiles_tiles_12_doortop as u8)
        && row_below_left_[drawn_col as usize].tiletype == tiles_tiles_4_gate as u8
    {
        add_backtable(
            chtabs_id_chtab_6_environment as c_short,
            68, // gate top mask
            draw_xh as i8, 0, draw_bottom_y as c_int,
            blitters_blitters_40h_mono as c_int, 0,
        );
        let modifier = {
            let m = row_below_left_[drawn_col as usize].modifier as u16;
            if m > 188 { 188 } else { m }
        };
        add_backtable(
            chtabs_id_chtab_6_environment as c_short,
            door_fram_top[((modifier >> 2) % 8) as usize] as c_int,
            draw_xh as i8, 0, draw_bottom_y as c_int,
            blitters_blitters_2_or as c_int, 0,
        );
    }
}

// seg008:066A
#[no_mangle]
pub unsafe extern "C" fn draw_tile_right() {
    if curr_tile == tiles_tiles_20_wall as u8 { return; }
    match tile_left {
        t if t == tiles_tiles_0_empty as u8 => {
            if modifier_left > 3 { return; }
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                blueline_fram1[modifier_left as usize] as c_int,
                draw_xh as i8, 0,
                blueline_fram_y[modifier_left as usize] as c_int + draw_main_y as c_int,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        t if t == tiles_tiles_1_floor as u8 => {
            ptr_add_table(
                chtabs_id_chtab_6_environment as c_short,
                42, // floor B
                draw_xh as i8, 0,
                tile_table[tile_left as usize].right_y as c_int + draw_main_y as c_int,
                blitters_blitters_10h_transp as c_int, 0,
            );
            let mut num = modifier_left;
            if num > 3 { num = 0; }
            let tlt = if (*custom).tbl_level_type[current_level as usize] != 0 { 1u8 } else { 0u8 };
            if num == tlt { return; }
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                blueline_fram3[num as usize] as c_int,
                draw_xh as i8, 0,
                draw_main_y as c_int - 20,
                blitters_blitters_0_no_transp as c_int, 0,
            );
        }
        t if t == tiles_tiles_7_doortop_with_floor as u8 || t == tiles_tiles_12_doortop as u8 => {
            if (*custom).tbl_level_type[current_level as usize] == 0 { return; }
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                doortop_fram_bot[modifier_left as usize] as c_int,
                draw_xh as i8, 0,
                tile_table[tile_left as usize].right_y as c_int + draw_main_y as c_int,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        t if t == tiles_tiles_20_wall as u8 => {
            if (*custom).tbl_level_type[current_level as usize] != 0 && (modifier_left & 0x80) == 0 {
                add_backtable(
                    chtabs_id_chtab_6_environment as c_short,
                    84, // wall stripe
                    draw_xh as i8 + 3, 0,
                    draw_main_y as c_int - 27,
                    blitters_blitters_0_no_transp as c_int, 0,
                );
            }
            add_backtable(
                chtabs_id_chtab_7_environmentwall as c_short,
                1,
                draw_xh as i8, 0,
                tile_table[tile_left as usize].right_y as c_int + draw_main_y as c_int,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        _ => {
            // default
            let mut id = tile_table[tile_left as usize].right_id as c_int;
            // USE_TELEPORTS (always on)
            if tile_left == tiles_tiles_23_balcony_left as u8 && modifier_left != 0 {
                id += 4;
            } else if tile_left == tiles_tiles_24_balcony_right as u8 && modifier_left == 1 {
                id += 4;
            }
            if id != 0 {
                let blit;
                if tile_left == tiles_tiles_5_stuck as u8 {
                    blit = blitters_blitters_10h_transp as c_int;
                    if curr_tile == tiles_tiles_0_empty as u8
                        || curr_tile == tiles_tiles_5_stuck as u8
                        || tile_is_floor(curr_tile as c_int) == 0
                    {
                        id = 42; // floor B
                    }
                } else {
                    blit = blitters_blitters_2_or as c_int;
                }
                add_backtable(
                    chtabs_id_chtab_6_environment as c_short,
                    id, draw_xh as i8, 0,
                    tile_table[tile_left as usize].right_y as c_int + draw_main_y as c_int,
                    blit, 0,
                );
            }
            if (*custom).tbl_level_type[current_level as usize] != 0 {
                add_backtable(
                    chtabs_id_chtab_6_environment as c_short,
                    tile_table[tile_left as usize].stripe_id as c_int,
                    draw_xh as i8, 0,
                    draw_main_y as c_int - 27,
                    blitters_blitters_2_or as c_int, 0,
                );
            }
            if tile_left == tiles_tiles_19_torch as u8 || tile_left == tiles_tiles_30_torch_with_debris as u8 {
                add_backtable(
                    chtabs_id_chtab_6_environment as c_short,
                    146, // torch base
                    draw_xh as i8, 0,
                    draw_bottom_y as c_int - 28,
                    blitters_blitters_0_no_transp as c_int, 0,
                );
            }
        }
    }
}

// seg008:08A0
#[no_mangle]
pub unsafe extern "C" fn get_spike_frame(modifier: u8) -> c_int {
    if modifier & 0x80 != 0 { 5 } else { modifier as c_int }
}

// seg008:08B5
#[no_mangle]
pub unsafe extern "C" fn draw_tile_anim_right() {
    match tile_left {
        t if t == tiles_tiles_2_spike as u8 => {
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                spikes_fram_right[get_spike_frame(modifier_left) as usize] as c_int,
                draw_xh as i8, 0,
                draw_main_y as c_int - 7,
                blitters_blitters_10h_transp as c_int, 0,
            );
        }
        t if t == tiles_tiles_4_gate as u8 => {
            draw_gate_back();
        }
        t if t == tiles_tiles_11_loose as u8 => {
            add_backtable(
                chtabs_id_chtab_6_environment as c_short,
                loose_fram_right[get_loose_frame(modifier_left) as usize] as c_int,
                draw_xh as i8, 0,
                draw_bottom_y as c_int - 1,
                blitters_blitters_2_or as c_int, 0,
            );
        }
        t if t == tiles_tiles_16_level_door_left as u8 => {
            draw_leveldoor();
        }
        t if t == tiles_tiles_19_torch as u8 || t == tiles_tiles_30_torch_with_debris as u8 => {
            if modifier_left < 9 {
                let mut blit = blitters_blitters_0_no_transp as c_int;
                // USE_COLORED_TORCHES (always on when USE_TEXT is on)
                let color = if drawn_col == 0 {
                    torch_colors[room_L as usize][drawn_row as usize * 10 + 9]
                } else {
                    torch_colors[drawn_room as usize][drawn_row as usize * 10 + drawn_col as usize - 1]
                };
                if color != 0 {
                    blit = blitters_blitters_colored_flame as c_int + (color as c_int & 0x3F);
                }
                add_backtable(
                    chtabs_id_chtab_1_flameswordpotion as c_short,
                    modifier_left as c_int + 1,
                    draw_xh as i8 + 1, 0,
                    draw_main_y as c_int - 40,
                    blit, 0,
                );
            }
        }
        _ => {}
    }
}

// seg008:0971
#[no_mangle]
pub unsafe extern "C" fn draw_tile_bottom(arg_0: u16) {
    let mut id: u8 = 0;
    let mut blit = blitters_blitters_0_no_transp as c_int;
    let mut chtab_id: u16 = chtabs_id_chtab_6_environment as u16;
    if curr_tile == tiles_tiles_20_wall as u8 {
        if (*custom).tbl_level_type[current_level as usize] == 0
            || (*custom).enable_wda_in_palace != 0
            || graphics_mode != grmodes_gmMcgaVga as u8
        {
            id = wall_fram_bottom[(curr_modifier & 0x7F) as usize];
        }
        chtab_id = chtabs_id_chtab_7_environmentwall as u16;
    } else if curr_tile == tiles_tiles_12_doortop as u8 {
        blit = blitters_blitters_2_or as c_int;
        id = tile_table[curr_tile as usize].bottom_id;
    } else {
        id = tile_table[curr_tile as usize].bottom_id;
    }
    if ptr_add_table(chtab_id as c_short, id as c_int, draw_xh as i8, 0, draw_bottom_y as c_int, blit, 0) != 0
        && arg_0 != 0
    {
        add_foretable(chtab_id as c_short, id as c_int, draw_xh as i8, 0, draw_bottom_y as c_int, blit, 0);
    }
    if chtab_id == chtabs_id_chtab_7_environmentwall as u16
        && graphics_mode != grmodes_gmCga as u8
        && graphics_mode != grmodes_gmHgaHerc as u8
    {
        wall_pattern(0, 0);
    }
}

// seg008:0A38
#[no_mangle]
pub unsafe extern "C" fn draw_loose(_arg_0: c_int) {
    if curr_tile == tiles_tiles_11_loose as u8 {
        let id = loose_fram_bottom[get_loose_frame(curr_modifier) as usize] as c_int;
        add_backtable(chtabs_id_chtab_6_environment as c_short, id, draw_xh as i8, 0, draw_bottom_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
        add_foretable(chtabs_id_chtab_6_environment as c_short, id, draw_xh as i8, 0, draw_bottom_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
    }
}

// seg008:0A8E
#[no_mangle]
pub unsafe extern "C" fn draw_tile_base() {
    let mut ybottom = draw_main_y as c_int;
    // USE_SUPER_HIGH_JUMP (always on)
    if (*fixes).enable_super_high_jump != 0 {
        if (curr_tile >= tiles_tiles_26_lattice_down as u8 && curr_tile <= tiles_tiles_29_lattice_right as u8)
            || (tile_left == tiles_tiles_26_lattice_down as u8 && curr_tile == tiles_tiles_12_doortop as u8)
        {
            return;
        }
    }
    if tile_left == tiles_tiles_26_lattice_down as u8 && curr_tile == tiles_tiles_12_doortop as u8 {
        let id = 6; // Lattice + door A
        ybottom += 3;
        ptr_add_table(chtabs_id_chtab_6_environment as c_short, id, draw_xh as i8, 0, tile_table[curr_tile as usize].base_y as c_int + ybottom, blitters_blitters_10h_transp as c_int, 0);
    } else {
        let id = if curr_tile == tiles_tiles_11_loose as u8 {
            loose_fram_left[get_loose_frame(curr_modifier) as usize] as c_int
        } else if curr_tile == tiles_tiles_15_opener as u8
            && tile_left == tiles_tiles_0_empty as u8
            && (*custom).tbl_level_type[current_level as usize] == 0
        {
            148 // left half of open button with no floor to the left
        } else {
            tile_table[curr_tile as usize].base_id as c_int
        };
        ptr_add_table(chtabs_id_chtab_6_environment as c_short, id, draw_xh as i8, 0, tile_table[curr_tile as usize].base_y as c_int + ybottom, blitters_blitters_10h_transp as c_int, 0);
    }
}

// seg008:0B2B
#[no_mangle]
pub unsafe extern "C" fn draw_tile_anim() {
    let mut color;
    let mut pot_size;
    match curr_tile {
        t if t == tiles_tiles_2_spike as u8 => {
            ptr_add_table(chtabs_id_chtab_6_environment as c_short, spikes_fram_left[get_spike_frame(curr_modifier) as usize] as c_int, draw_xh as i8, 0, draw_main_y as c_int - 2, blitters_blitters_10h_transp as c_int, 0);
        }
        t if t == tiles_tiles_10_potion as u8 => {
            // C: word color=12, pot_size=0; switch fallthrough: 3|4->color=10, then 2: pot_size=1
            color = 12;
            pot_size = 0u32;
            let ptype = (curr_modifier & 0xF8) >> 3;
            match ptype {
                0 => return,
                5 | 6 => { color = 9; }
                3 | 4 => { color = 10; pot_size = 1; }
                2 => { pot_size = 1; }
                _ => {}
            }
            add_backtable(chtabs_id_chtab_1_flameswordpotion as c_short, 23, draw_xh as i8 + 3, 1, draw_main_y as c_int - (pot_size as c_int * 4) - 14, blitters_blitters_40h_mono as c_int, 0);
            add_foretable(chtabs_id_chtab_1_flameswordpotion as c_short, potion_fram_bubb[(curr_modifier & 0x7) as usize] as c_int, draw_xh as i8 + 3, 1, draw_main_y as c_int - (pot_size as c_int * 4) - 14, color + blitters_blitters_40h_mono as c_int, 0);
        }
        t if t == tiles_tiles_22_sword as u8 => {
            add_midtable(chtabs_id_chtab_1_flameswordpotion as c_short, (curr_modifier == 1) as c_int + 10, draw_xh as i8, 0, draw_main_y as c_int - 3, blitters_blitters_10h_transp as c_int, (curr_modifier == 1) as u8);
        }
        t if t == tiles_tiles_18_chomper as u8 => {
            let chomper_num = chomper_fram1[(curr_modifier & 0x7F).min(6) as usize] as usize;
            add_backtable(chtabs_id_chtab_6_environment as c_short, chomper_fram_bot[chomper_num] as c_int, draw_xh as i8, 0, draw_main_y as c_int, blitters_blitters_10h_transp as c_int, 0);
            if curr_modifier & 0x80 != 0 {
                add_backtable(chtabs_id_chtab_6_environment as c_short, chomper_num as c_int + 114, draw_xh as i8 + 1, 4, draw_main_y as c_int - 6, blitters_blitters_4Ch_mono_12 as c_int, 0);
            }
            add_backtable(chtabs_id_chtab_6_environment as c_short, chomper_fram_top[chomper_num] as c_int, draw_xh as i8, 0, draw_main_y as c_int - chomper_fram_y[chomper_num] as c_int, blitters_blitters_10h_transp as c_int, 0);
        }
        _ => {}
    }
}

// seg008:0D15
#[no_mangle]
pub unsafe extern "C" fn draw_tile_fore() {
    if tile_left == tiles_tiles_4_gate as u8
        && Kid.curr_row == drawn_row as i8
        && Kid.curr_col == drawn_col as i8 - 1
        && Kid.room != room_R as u8
    {
        draw_gate_fore();
    }
    match curr_tile {
        t if t == tiles_tiles_2_spike as u8 => {
            add_foretable(chtabs_id_chtab_6_environment as c_short, spikes_fram_fore[get_spike_frame(curr_modifier) as usize] as c_int, draw_xh as i8, 0, draw_main_y as c_int - 2, blitters_blitters_10h_transp as c_int, 0);
        }
        t if t == tiles_tiles_18_chomper as u8 => {
            let chomper_num = chomper_fram1[(curr_modifier & 0x7F).min(6) as usize] as usize;
            add_foretable(chtabs_id_chtab_6_environment as c_short, chomper_fram_for[chomper_num] as c_int, draw_xh as i8, 0, draw_main_y as c_int, blitters_blitters_10h_transp as c_int, 0);
            if curr_modifier & 0x80 != 0 {
                add_foretable(chtabs_id_chtab_6_environment as c_short, chomper_num as c_int + 119, draw_xh as i8 + 1, 4, draw_main_y as c_int - 6, blitters_blitters_4Ch_mono_12 as c_int, 0);
            }
        }
        t if t == tiles_tiles_20_wall as u8 => {
            if (*custom).tbl_level_type[current_level as usize] == 0
                || (*custom).enable_wda_in_palace != 0
                || graphics_mode != grmodes_gmMcgaVga as u8
            {
                add_foretable(chtabs_id_chtab_7_environmentwall as c_short, wall_fram_main[(curr_modifier & 0x7F) as usize] as c_int, draw_xh as i8, 0, draw_main_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
            }
            if graphics_mode != grmodes_gmCga as u8 && graphics_mode != grmodes_gmHgaHerc as u8 {
                wall_pattern(1, 1);
            }
        }
        _ => {
            // USE_SUPER_HIGH_JUMP (always on): lattice tiles get fore drawn here
            if curr_tile >= tiles_tiles_26_lattice_down as u8 && curr_tile <= tiles_tiles_29_lattice_right as u8 {
                if (*fixes).enable_super_high_jump != 0 {
                    add_foretable(chtabs_id_chtab_6_environment as c_short, tile_table[curr_tile as usize].base_id as c_int, draw_xh as i8, 0, tile_table[curr_tile as usize].base_y as c_int + draw_main_y as c_int, blitters_blitters_10h_transp as c_int, 0);
                }
            }
            // Shared default (lattice falls through here)
            if (*fixes).enable_super_high_jump != 0
                && tile_left == tiles_tiles_26_lattice_down as u8
                && curr_tile == tiles_tiles_12_doortop as u8
            {
                add_foretable(chtabs_id_chtab_6_environment as c_short, 6, draw_xh as i8, 0, tile_table[curr_tile as usize].base_y as c_int + draw_main_y as c_int + 3, blitters_blitters_10h_transp as c_int, 0);
            }
            let mut id = tile_table[curr_tile as usize].fore_id as c_int;
            if id == 0 { return; }
            if curr_tile == tiles_tiles_10_potion as u8 {
                let potion_type = ((curr_modifier & 0xF8) >> 3) as c_int;
                if potion_type < 5 && potion_type >= 2 { id = 13; } // large pot
            }
            let xh = (tile_table[curr_tile as usize].fore_x as u16 + draw_xh) as i8;
            let ybottom = tile_table[curr_tile as usize].fore_y as c_int + draw_main_y as c_int;
            if curr_tile == tiles_tiles_10_potion as u8 {
                if (*custom).tbl_level_type[current_level as usize] != 0 { id += 2; }
                add_foretable(chtabs_id_chtab_1_flameswordpotion as c_short, id, xh, 6, ybottom, blitters_blitters_10h_transp as c_int, 0);
            } else {
                if (curr_tile == tiles_tiles_3_pillar as u8 && (*custom).tbl_level_type[current_level as usize] == 0)
                    || (curr_tile >= tiles_tiles_27_lattice_small as u8 && curr_tile < tiles_tiles_30_torch_with_debris as u8)
                {
                    add_foretable(chtabs_id_chtab_6_environment as c_short, id, xh, 0, ybottom, blitters_blitters_0_no_transp as c_int, 0);
                } else {
                    add_foretable(chtabs_id_chtab_6_environment as c_short, id, xh, 0, ybottom, blitters_blitters_10h_transp as c_int, 0);
                }
            }
        }
    }
}

// seg008:178E
#[no_mangle]
pub unsafe extern "C" fn calc_gate_pos() {
    gate_top_y = (draw_bottom_y as i32 - 62) as u16;
    gate_openness = (modifier_left.min(188) as u16 >> 2) + 1;
    gate_bottom_y = (draw_main_y as i32 - gate_openness as i32) as u16;
}

// seg008:17B7
#[no_mangle]
pub unsafe extern "C" fn draw_gate_back() {
    calc_gate_pos();
    if (gate_bottom_y as i16 + 12) < draw_main_y {
        add_backtable(chtabs_id_chtab_6_environment as c_short, 50, draw_xh as i8, 0, gate_bottom_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
    } else {
        add_backtable(chtabs_id_chtab_6_environment as c_short, tile_table[tiles_tiles_4_gate as usize].right_id as c_int, draw_xh as i8, 0, tile_table[tiles_tiles_4_gate as usize].right_y as c_int + draw_main_y as c_int, blitters_blitters_0_no_transp as c_int, 0);
        if can_see_bottomleft() != 0 { draw_tile_topright(); }
        // FIX_GATE_DRAWING_BUG (always on)
        if (*fixes).fix_gate_drawing_bug != 0 {
            draw_tile_anim_topright();
        }
        draw_tile_bottom(0);
        draw_loose(0);
        draw_tile_base();
        add_backtable(chtabs_id_chtab_6_environment as c_short, 51, draw_xh as i8, 0, gate_bottom_y as c_int - 2, blitters_blitters_10h_transp as c_int, 0);
    }
    let mut ybottom = gate_bottom_y as i16 - 12;
    if ybottom < 192 {
        while ybottom >= 0 && ybottom > 7 && ybottom - 7 > gate_top_y as i16 {
            add_backtable(chtabs_id_chtab_6_environment as c_short, 52, draw_xh as i8, 0, ybottom as c_int, blitters_blitters_0_no_transp as c_int, 0);
            ybottom -= 8;
        }
    }
    let gate_frame = ybottom - gate_top_y as i16 + 1;
    if gate_frame > 0 && gate_frame < 9 {
        add_backtable(chtabs_id_chtab_6_environment as c_short, door_fram_slice[gate_frame as usize] as c_int, draw_xh as i8, 0, ybottom as c_int, blitters_blitters_0_no_transp as c_int, 0);
    }
}

// seg008:18BE
#[no_mangle]
pub unsafe extern "C" fn draw_gate_fore() {
    calc_gate_pos();
    add_foretable(chtabs_id_chtab_6_environment as c_short, 51, draw_xh as i8, 0, gate_bottom_y as c_int - 2, blitters_blitters_10h_transp as c_int, 0);
    let mut ybottom = gate_bottom_y as i16 - 12;
    if ybottom < 192 {
        while ybottom >= 0 && ybottom > 7 && ybottom - 7 > gate_top_y as i16 {
            add_foretable(chtabs_id_chtab_6_environment as c_short, 52, draw_xh as i8, 0, ybottom as c_int, blitters_blitters_10h_transp as c_int, 0);
            ybottom -= 8;
        }
    }
}

// seg008:0FF6
#[no_mangle]
pub unsafe extern "C" fn get_loose_frame(modifier: u8) -> c_int {
    let mut m = modifier;
    if (m & 0x80) != 0 || (*custom).loose_floor_delay > 11 {
        m &= 0x7F;
        if m > 10 { return 1; }
    }
    m as c_int
}

// Get an image, with index and NULL checks.
#[no_mangle]
pub unsafe extern "C" fn get_image(chtab_id: c_short, id: c_int) -> *mut image_type {
    let n = core::mem::size_of_val(&chtab_addrs) / core::mem::size_of::<*mut chtab_type>();
    if chtab_id < 0 || chtab_id as usize >= n {
        return core::ptr::null_mut();
    }
    let chtab = chtab_addrs[chtab_id as usize];
    if chtab.is_null() { return core::ptr::null_mut(); }
    if id < 0 || id >= (*chtab).n_images as c_int {
        return core::ptr::null_mut();
    }
    // images is a flexible array; access via pointer cast
    core::ptr::addr_of!((*chtab).images).cast::<*mut image_type>().add(id as usize).read()
}

// seg008:10A8
#[no_mangle]
pub unsafe extern "C" fn add_backtable(chtab_id: c_short, id: c_int, xh: i8, xl: i8, ybottom: c_int, blit: c_int, _peel: u8) -> c_int {
    if id == 0 { return 0; }
    let index = table_counts[0] as usize;
    if index >= 200 {
        show_dialog(b"BackTable Overflow\0".as_ptr() as *const c_char);
        return 0;
    }
    let item = &mut backtable[index];
    item.xh = xh;
    item.xl = xl;
    item.chtab_id = chtab_id as u8;
    item.id = (id - 1) as u8;
    let image = get_image(chtab_id, id - 1);
    if image.is_null() { return 0; }
    item.y = (ybottom - (*image).h as c_int + 1) as c_short;
    item.blit = blit;
    if draw_mode != 0 {
        draw_back_fore(0, index as c_int);
    }
    table_counts[0] += 1;
    1
}

// seg008:1017
#[no_mangle]
pub unsafe extern "C" fn add_foretable(chtab_id: c_short, id: c_int, xh: i8, xl: i8, ybottom: c_int, blit: c_int, _peel: u8) -> c_int {
    if id == 0 { return 0; }
    let index = table_counts[1] as usize;
    if index >= 200 {
        show_dialog(b"ForeTable Overflow\0".as_ptr() as *const c_char);
        return 0;
    }
    let item = &mut foretable[index];
    item.xh = xh;
    item.xl = xl;
    item.chtab_id = chtab_id as u8;
    item.id = (id - 1) as u8;
    let image = get_image(chtab_id, id - 1);
    if image.is_null() { return 0; }
    item.y = (ybottom - (*image).h as c_int + 1) as c_short;
    item.blit = blit;
    if draw_mode != 0 {
        draw_back_fore(1, index as c_int);
    }
    table_counts[1] += 1;
    1
}

// seg008:113A
#[no_mangle]
pub unsafe extern "C" fn add_midtable(chtab_id: c_short, id: c_int, xh: i8, xl: i8, ybottom: c_int, mut blit: c_int, peel: u8) -> c_int {
    if id == 0 { return 0; }
    let index = table_counts[3] as usize;
    if index >= 50 {
        show_dialog(b"MidTable Overflow\0".as_ptr() as *const c_char);
        return 0;
    }
    let item = &mut midtable[index];
    item.xh = xh;
    item.xl = xl;
    item.chtab_id = chtab_id as u8;
    item.id = (id - 1) as u8;
    let image = get_image(chtab_id, id - 1);
    if image.is_null() { return 0; }
    item.y = (ybottom - (*image).h as c_int + 1) as c_short;
    if obj_direction == directions_dir_0_right as i8 && chtab_flip_clip[chtab_id as usize] != 0 {
        blit += 0x80;
    }
    item.blit = blit;
    item.peel = peel;
    item.clip.left = obj_clip_left;
    item.clip.right = obj_clip_right;
    item.clip.top = obj_clip_top;
    item.clip.bottom = obj_clip_bottom;
    if draw_mode != 0 {
        draw_mid(index as c_int);
    }
    table_counts[3] += 1;
    1
}

// seg008:1208
#[no_mangle]
pub unsafe extern "C" fn add_peel(left: c_int, right: c_int, top: c_int, height: c_int) {
    if peels_count >= 50 {
        show_dialog(b"Peels OverFlow\0".as_ptr() as *const c_char);
        return;
    }
    let rect = rect_type {
        left: left as c_short,
        right: right as c_short,
        top: top as c_short,
        bottom: (top + height) as c_short,
    };
    peels_table[peels_count as usize] = read_peel_from_screen(&rect);
    peels_count += 1;
}

// seg008:1254
#[no_mangle]
pub unsafe extern "C" fn add_wipetable(layer: i8, left: c_short, bottom: c_short, height: i8, width: c_short, color: i8) {
    let index = table_counts[2] as usize;
    if index >= 300 {
        show_dialog(b"WipeTable Overflow\0".as_ptr() as *const c_char);
        return;
    }
    let item = &mut wipetable[index];
    item.left = left;
    item.bottom = bottom + 1;
    item.height = height;
    item.width = width;
    item.color = color;
    item.layer = layer;
    if draw_mode != 0 {
        draw_wipe(index as c_int);
    }
    table_counts[2] += 1;
}

// seg008:12BB
#[no_mangle]
pub unsafe extern "C" fn draw_table(which_table: c_int) {
    let count = table_counts[which_table as usize];
    for index in 0..count as c_int {
        if which_table == 3 {
            draw_mid(index);
        } else {
            draw_back_fore(which_table, index);
        }
    }
}

// seg008:12FE
#[no_mangle]
pub unsafe extern "C" fn draw_wipes(which: c_int) {
    let count = table_counts[2] as usize;
    for index in 0..count {
        if which == wipetable[index].layer as c_int {
            draw_wipe(index as c_int);
        }
    }
}

// seg008:133B
#[no_mangle]
pub unsafe extern "C" fn draw_back_fore(which_table: c_int, index: c_int) {
    let table_entry = if which_table == 0 {
        &backtable[index as usize]
    } else {
        &foretable[index as usize]
    };
    let image = get_image(table_entry.chtab_id as c_short, table_entry.id as c_int);
    draw_image(image, image, table_entry.xh as c_int * 8 + table_entry.xl as c_int, table_entry.y as c_int, table_entry.blit);
}

// SDL_BLENDMODE_NONE = 0
fn hflip(input: *mut SDL_Surface) -> *mut SDL_Surface {
    unsafe {
        let width = (*input).w;
        let height = (*input).h;
        let output = SDL_ConvertSurface(input, (*input).format, 0);
        SDL_SetSurfacePalette(output, (*(*input).format).palette);
        if output.is_null() {
            sdlperror(b"hflip: SDL_ConvertSurface\0".as_ptr() as *const c_char);
            quit(1);
        }
        SDL_SetSurfaceBlendMode(input, 0); // SDL_BLENDMODE_NONE
        SDL_SetColorKey(input, 0, 0);      // SDL_FALSE
        SDL_SetColorKey(output, 0, 0);
        SDL_SetSurfaceAlphaMod(input, 255);
        let mut source_x = 0;
        let mut target_x = width - 1;
        while source_x < width {
            let srcrect = SDL_Rect { x: source_x, y: 0, w: 1, h: height };
            let mut dstrect = SDL_Rect { x: target_x, y: 0, w: 1, h: height };
            if SDL_UpperBlit(input, &srcrect, output, &mut dstrect) != 0 {
                sdlperror(b"hflip: SDL_BlitSurface\0".as_ptr() as *const c_char);
                quit(1);
            }
            source_x += 1;
            target_x -= 1;
        }
        output
    }
}

// seg008:140C
#[no_mangle]
pub unsafe extern "C" fn draw_mid(index: c_int) {
    let entry = &midtable[index as usize];
    let image_id = entry.id as c_int;
    let chtab_id = entry.chtab_id as c_short;
    let image = get_image(chtab_id, image_id);
    let mut xpos = entry.xh as c_int * 8 + entry.xl as c_int;
    let ypos = entry.y as c_int;
    let raw_blit = entry.blit;
    let blit_flip = raw_blit & 0x80;
    let blit = raw_blit & 0x7F;

    if chtab_flip_clip[chtab_id as usize] != 0 {
        set_clip_rect(&entry.clip);
        if chtab_id != chtabs_id_chtab_0_sword as c_short {
            xpos = calc_screen_x_coord(xpos as c_short) as c_int;
        }
    }

    let need_free_image;
    let draw_image_ptr;
    if blit_flip != 0 {
        xpos -= (*image).w as c_int;
        draw_image_ptr = hflip(image);
        need_free_image = true;
    } else {
        draw_image_ptr = image;
        need_free_image = false;
    }

    if entry.peel != 0 {
        add_peel(
            round_xpos_to_byte(xpos, 0),
            round_xpos_to_byte((*draw_image_ptr).w as c_int + xpos, 1),
            ypos,
            (*draw_image_ptr).h as c_int,
        );
    }
    draw_image(draw_image_ptr, draw_image_ptr, xpos, ypos, blit);

    if chtab_flip_clip[chtab_id as usize] != 0 {
        reset_clip_rect();
    }
    if need_free_image {
        SDL_FreeSurface(draw_image_ptr);
    }
}

// seg008:167B
#[no_mangle]
pub unsafe extern "C" fn draw_image(image: *mut image_type, mask: *mut image_type, xpos: c_int, ypos: c_int, blit: c_int) {
    match blit {
        b if b == blitters_blitters_10h_transp as c_int => {
            draw_image_transp(image, mask, xpos, ypos);
        }
        b if b == blitters_blitters_9_black as c_int => {
            method_6_blit_img_to_scr(mask, xpos, ypos, blitters_blitters_9_black as c_int);
        }
        b if b == blitters_blitters_0_no_transp as c_int
          || b == blitters_blitters_2_or as c_int
          || b == blitters_blitters_3_xor as c_int => {
            method_6_blit_img_to_scr(image, xpos, ypos, blit);
        }
        b => {
            if b >= 0x100 {
                method_6_blit_img_to_scr(mask, xpos, ypos, blit);
            } else {
                method_3_blit_mono(image, xpos, ypos, 0, (blit & 0xBF) as u8);
            }
        }
    }
    if need_drects != 0 {
        let rect = rect_type {
            left: xpos as c_short,
            right: (xpos + (*image).w as c_int) as c_short,
            top: ypos as c_short,
            bottom: (ypos + (*image).h as c_int) as c_short,
        };
        add_drect(&rect as *const rect_type as *mut rect_type);
    }
}

// seg008:1730
#[no_mangle]
pub unsafe extern "C" fn draw_wipe(index: c_int) {
    let ptr = &wipetable[index as usize];
    let rect = rect_type {
        left: ptr.left,
        right: ptr.left + ptr.width,
        top: ptr.bottom - ptr.height as c_short,
        bottom: ptr.bottom,
    };
    draw_rect(&rect, ptr.color as c_int);
    if need_drects != 0 {
        add_drect(&rect as *const rect_type as *mut rect_type);
    }
}

// seg008:1C4E
#[no_mangle]
pub unsafe extern "C" fn restore_peels() {
    while peels_count > 0 {
        peels_count -= 1;
        let peel = peels_table[peels_count as usize];
        if need_drects != 0 {
            add_drect(&(*peel).rect as *const rect_type as *mut rect_type);
        }
        restore_peel(peel);
    }
    peels_count = 0;
}

// seg008:1C8F
#[no_mangle]
pub unsafe extern "C" fn add_drect(source: *mut rect_type) {
    for index in 0..drects_count as usize {
        let mut target_rect = core::mem::zeroed::<rect_type>();
        if intersect_rect(&mut target_rect, shrink2_rect(&mut target_rect, source, -1, -1), &drects[index]) != 0 {
            let current_drect = &mut drects[index];
            union_rect(current_drect, current_drect, source);
            return;
        }
    }
    if drects_count >= 30 {
        show_dialog(b"DRects Overflow\0".as_ptr() as *const c_char);
        return;
    }
    drects[drects_count as usize] = *source;
    drects_count += 1;
}

// seg008:1BEB
#[no_mangle]
pub unsafe extern "C" fn draw_tables() {
    drects_count = 0;
    current_target_surface = offscreen_surface;
    if is_blind_mode != 0 {
        draw_rect(&rect_top, colorids_color_0_black as c_int);
    }
    restore_peels();
    draw_wipes(0);
    draw_table(0); // backtable
    // FIX_BLACK_RECT (always on)
    draw_wipes(1);
    draw_table(3); // midtable
    draw_wipes(1);
    draw_table(1); // foretable
    current_target_surface = onscreen_surface_;
    show_copyprot(1);
}

// seg008:2627
#[no_mangle]
pub unsafe extern "C" fn free_peels() {
    while peels_count > 0 {
        peels_count -= 1;
        free_peel(peels_table[peels_count as usize]);
    }
}

// seg008:1BCB
#[no_mangle]
pub unsafe extern "C" fn draw_tile_wipe(height: u8) {
    add_wipetable(0, (draw_xh * 8) as c_short, draw_bottom_y, height as i8, (4 * 8) as c_short, 0);
}

// seg008:1AF8
#[no_mangle]
pub unsafe extern "C" fn draw_moving() {
    draw_mobs();
    draw_people();
    redraw_needed_tiles();
}

// seg008:1B06
#[no_mangle]
pub unsafe extern "C" fn redraw_needed_tiles() {
    load_leftroom();
    draw_objtable_items_at_tile(30u8);
    for dr in (0i16..3).rev() {
        drawn_row = dr;
        load_rowbelow();
        draw_bottom_y = 63 * drawn_row + 65;
        draw_main_y = draw_bottom_y - 3;
        for dc in 0i16..10 {
            drawn_col = dc;
            load_curr_and_left_tile();
            redraw_needed((tbl_line_at(drawn_row as usize) as c_int + drawn_col as c_int) as c_short);
        }
    }
    let saved_drawn_room = drawn_room;
    drawn_room = room_A;
    load_room_links();
    load_leftroom();
    drawn_row = 2;
    load_rowbelow();
    for dc in 0i16..10 {
        drawn_col = dc;
        load_curr_and_left_tile();
        draw_main_y = -1;
        draw_bottom_y = 2;
        redraw_needed_above(drawn_col as c_int);
    }
    drawn_room = saved_drawn_room;
    load_room_links();
    draw_objtable_items_at_tile(255u8); // -1 as u8
}

// seg008:0211
#[no_mangle]
pub unsafe extern "C" fn redraw_needed(tilepos: c_short) {
    if wipe_frames[tilepos as usize] != 0 {
        wipe_frames[tilepos as usize] -= 1;
        draw_tile_wipe(wipe_heights[tilepos as usize] as u8);
    }
    if redraw_frames_full[tilepos as usize] != 0 {
        redraw_frames_full[tilepos as usize] -= 1;
        draw_tile();
    } else if redraw_frames_anim[tilepos as usize] != 0 {
        redraw_frames_anim[tilepos as usize] -= 1;
        draw_tile_anim_topright();
        draw_tile_anim_right();
        draw_tile_anim();
        // FIX_ABOVE_GATE (always on)
        draw_tile_fore();
        draw_tile_bottom(0);
    }
    if redraw_frames2[tilepos as usize] != 0 {
        redraw_frames2[tilepos as usize] -= 1;
        draw_other_overlay();
    } else if redraw_frames_floor_overlay[tilepos as usize] != 0 {
        redraw_frames_floor_overlay[tilepos as usize] -= 1;
        draw_floor_overlay();
    }
    if tile_object_redraw[tilepos as usize] != 0 {
        if tile_object_redraw[tilepos as usize] == 0xFF {
            draw_objtable_items_at_tile((tilepos - 1) as u8);
        }
        draw_objtable_items_at_tile(tilepos as u8);
        tile_object_redraw[tilepos as usize] = 0;
    }
    if redraw_frames_fore[tilepos as usize] != 0 {
        redraw_frames_fore[tilepos as usize] -= 1;
        draw_tile_fore();
    }
}

// seg008:02C1
#[no_mangle]
pub unsafe extern "C" fn redraw_needed_above(column: c_int) {
    if redraw_frames_above[column as usize] != 0 {
        redraw_frames_above[column as usize] -= 1;
        // FIX_BIGPILLAR_JUMP_UP (always on)
        if curr_tile != tiles_tiles_9_bigpillar_top as u8 {
            draw_tile_wipe(3);
            draw_tile_floorright();
        }
        draw_tile_anim_topright();
        draw_tile_right();
        draw_tile_bottom(1);
        draw_loose(1);
        draw_tile_fore();
    }
}

// seg008:1F67
#[no_mangle]
pub unsafe extern "C" fn draw_objtable_items_at_tile(tilepos: u8) {
    let obj_count = table_counts[4];
    if obj_count == 0 { return; }
    n_curr_objs = 0;
    for obj_index in (0..obj_count as c_short).rev() {
        if objtable[obj_index as usize].tilepos == tilepos {
            curr_objs[n_curr_objs as usize] = obj_index;
            n_curr_objs += 1;
        }
    }
    if n_curr_objs != 0 {
        sort_curr_objs();
        for obj_index in 0..n_curr_objs as usize {
            draw_objtable_item(curr_objs[obj_index] as c_int);
        }
    }
}

// seg008:1FDE
#[no_mangle]
pub unsafe extern "C" fn sort_curr_objs() {
    let last = n_curr_objs - 1;
    loop {
        let mut swapped = 0i16;
        for index in 0..last as usize {
            if compare_curr_objs(index as c_int, index as c_int + 1) != 0 {
                let temp = curr_objs[index];
                curr_objs[index] = curr_objs[index + 1];
                curr_objs[index + 1] = temp;
                swapped = 1;
            }
        }
        if swapped == 0 { break; }
    }
}

// seg008:203C
#[no_mangle]
pub unsafe extern "C" fn compare_curr_objs(index1: c_int, index2: c_int) -> c_int {
    let obj_index1 = curr_objs[index1 as usize] as usize;
    if objtable[obj_index1].obj_type == 1 { return 1; }
    let obj_index2 = curr_objs[index2 as usize] as usize;
    if objtable[obj_index2].obj_type == 1 { return 0; }
    if objtable[obj_index1].obj_type == 0x80 && objtable[obj_index2].obj_type == 0x80 {
        return (objtable[obj_index1].y < objtable[obj_index2].y) as c_int;
    }
    (objtable[obj_index1].y > objtable[obj_index2].y) as c_int
}

// seg008:20CA
#[no_mangle]
pub unsafe extern "C" fn draw_objtable_item(index: c_int) {
    match load_obj_from_objtable(index) {
        0 | 4 => {
            // Kid or mirror image
            if obj_id == 0xFF { return; }
            if united_with_shadow != 0 && (united_with_shadow % 2) == 0 {
                render_shadow_sprite();
                return;
            }
            add_midtable(obj_chtab as c_short, obj_id as c_int + 1, obj_xh as i8, obj_xl as i8, obj_y as c_int, blitters_blitters_10h_transp as c_int, 1);
        }
        2 | 3 | 5 => {
            // Guard, sword, hurt splash
            add_midtable(obj_chtab as c_short, obj_id as c_int + 1, obj_xh as i8, obj_xl as i8, obj_y as c_int, blitters_blitters_10h_transp as c_int, 1);
        }
        1 => {
            // Shadow
            render_shadow_sprite();
        }
        0x80 => {
            // Loose floor
            obj_direction = directions_dir_FF_left as i8;
            add_midtable(obj_chtab as c_short, loose_fram_left[obj_id as usize] as c_int, obj_xh as i8, obj_xl as i8, obj_y as c_int - 3, blitters_blitters_10h_transp as c_int, 1);
            add_midtable(obj_chtab as c_short, loose_fram_bottom[obj_id as usize] as c_int, obj_xh as i8, obj_xl as i8, obj_y as c_int, 0, 1);
            add_midtable(obj_chtab as c_short, loose_fram_right[obj_id as usize] as c_int, obj_x as c_short as i8 + 4, obj_xl as i8, obj_y as c_int - 1, blitters_blitters_10h_transp as c_int, 1);
        }
        _ => {}
    }
}

// seg008:2228
#[no_mangle]
pub unsafe extern "C" fn load_obj_from_objtable(index: c_int) -> c_int {
    let curr_obj = &objtable[index as usize];
    obj_xh = curr_obj.xh as u8;
    obj_x = curr_obj.xh as c_short;
    obj_xl = curr_obj.xl as u8;
    obj_y = curr_obj.y as u8;
    obj_id = curr_obj.id;
    obj_chtab = curr_obj.chtab_id;
    obj_direction = curr_obj.direction;
    obj_clip_top = curr_obj.clip.top;
    obj_clip_bottom = curr_obj.clip.bottom;
    obj_clip_left = curr_obj.clip.left;
    obj_clip_right = curr_obj.clip.right;
    curr_obj.obj_type as c_int
}

// seg008:228A
#[no_mangle]
pub unsafe extern "C" fn draw_people() {
    check_mirror();
    draw_kid();
    draw_guard();
    reset_obj_clip();
    draw_hp();
}

// seg008:22A2
#[no_mangle]
pub unsafe extern "C" fn draw_kid() {
    if Kid.room != 0 && Kid.room == drawn_room as u8 {
        add_kid_to_objtable();
        if hitp_delta < 0 {
            draw_hurt_splash();
        }
        add_sword_to_objtable();
    }
}

// seg008:22C9
#[no_mangle]
pub unsafe extern "C" fn draw_guard() {
    if Guard.direction != directions_dir_56_none as i8 && Guard.room == drawn_room as u8 {
        add_guard_to_objtable();
        if guardhp_delta < 0 {
            draw_hurt_splash();
        }
        add_sword_to_objtable();
    }
}

// seg008:22F0
#[no_mangle]
pub unsafe extern "C" fn add_kid_to_objtable() {
    loadkid();
    load_fram_det_col();
    load_frame_to_obj();
    stuck_lower();
    set_char_collision();
    set_objtile_at_char();
    redraw_at_char();
    redraw_at_char2();
    clip_char();
    add_objtable(0);
}

// seg008:2324
#[no_mangle]
pub unsafe extern "C" fn add_guard_to_objtable() {
    let obj_type;
    loadshad();
    load_fram_det_col();
    load_frame_to_obj();
    stuck_lower();
    set_char_collision();
    set_objtile_at_char();
    redraw_at_char();
    redraw_at_char2();
    clip_char();
    if Char.charid == charids_charid_1_shadow as u8 {
        if current_level == (*custom).mirror_level && Char.room == (*custom).mirror_room {
            obj_clip_left = 137;
            obj_clip_left += ((*custom).mirror_column as c_short - 4) * 32;
        }
        obj_type = 1u8; // shadow
    } else {
        obj_type = 2u8; // Guard
    }
    add_objtable(obj_type);
}

// seg008:2388
#[no_mangle]
pub unsafe extern "C" fn add_objtable(obj_type: u8) {
    let index = table_counts[4] as usize;
    table_counts[4] += 1;
    if index >= 50 {
        show_dialog(b"ObjTable Overflow\0".as_ptr() as *const c_char);
        return;
    }
    let entry = &mut objtable[index];
    entry.obj_type = obj_type;
    x_to_xh_and_xl(obj_x as c_int, &mut entry.xh, &mut entry.xl);
    entry.y = obj_y as c_short;
    entry.clip.top = obj_clip_top;
    entry.clip.bottom = obj_clip_bottom;
    entry.clip.left = obj_clip_left;
    entry.clip.right = obj_clip_right;
    entry.chtab_id = obj_chtab;
    entry.id = obj_id;
    entry.direction = obj_direction;
    mark_obj_tile_redraw(index as c_int);
}

// seg008:2423
#[no_mangle]
pub unsafe extern "C" fn mark_obj_tile_redraw(index: c_int) {
    objtable[index as usize].tilepos = obj_tilepos;
    if obj_tilepos < 30 {
        tile_object_redraw[obj_tilepos as usize] = 1;
    }
}

// seg008:2448
#[no_mangle]
pub unsafe extern "C" fn load_frame_to_obj() {
    let chtab_base = chtabs_id_chtab_2_kid as u8;
    reset_obj_clip();
    load_frame();
    obj_direction = Char.direction;
    obj_id = cur_frame.image;
    obj_chtab = chtab_base + (cur_frame.sword >> 6);
    obj_x = (char_dx_forward(cur_frame.dx as c_int) << 1) as c_short - 116;
    obj_y = (cur_frame.dy as c_int + Char.y as c_int) as u8;
    if (cur_frame.flags ^ obj_direction as u8) & 0x80 == 0 {
        obj_x += 1;
    }
}

// seg008:1E3A
#[no_mangle]
pub unsafe extern "C" fn draw_floor_overlay() {
    // FIX_BIGPILLAR_CLIMB (always on)
    if tile_left != tiles_tiles_0_empty as u8 {
        if (*fixes).fix_bigpillar_climb == 0 || tile_left != tiles_tiles_9_bigpillar_top as u8 {
            return;
        }
    }
    if curr_tile == tiles_tiles_1_floor as u8
        || curr_tile == tiles_tiles_3_pillar as u8
        || curr_tile == tiles_tiles_5_stuck as u8
        || curr_tile == tiles_tiles_19_torch as u8
    {
        if Kid.frame >= frameids_frame_137_climbing_3 as u8
            && Kid.frame <= frameids_frame_144_climbing_10 as u8
        {
            let overlay_id = floor_left_overlay[(Kid.frame - frameids_frame_137_climbing_3 as u8) as usize];
            add_midtable(
                chtabs_id_chtab_6_environment as c_short,
                overlay_id as c_int,
                draw_xh as i8, 0,
                (curr_tile == tiles_tiles_5_stuck as u8) as c_int + draw_main_y as c_int,
                blitters_blitters_10h_transp as c_int, 0,
            );
        }
        ptr_add_table = add_midtable;
        draw_tile_bottom(0);
        ptr_add_table = add_backtable;
    } else {
        draw_other_overlay();
    }
}

// seg008:1EB5
#[no_mangle]
pub unsafe extern "C" fn draw_other_overlay() {
    let mut tiletype: u8 = 0;
    let mut modifier: u8 = 0;
    if tile_left == tiles_tiles_0_empty as u8 {
        ptr_add_table = add_midtable;
        draw_tile2();
    } else if curr_tile != tiles_tiles_0_empty as u8
        && drawn_col > 0
        && get_tile_to_draw(
            drawn_room as c_int, drawn_col as c_int - 2, drawn_row as c_int,
            &mut tiletype, &mut modifier,
            tiles_tiles_0_empty as u8,
        ) == tiles_tiles_0_empty as c_int
    {
        ptr_add_table = add_midtable;
        draw_tile2();
        ptr_add_table = add_backtable;
        draw_tile2();
        tile_object_redraw[(tbl_line_at(drawn_row as usize) as c_int + drawn_col as c_int) as usize] = 0xFF;
    }
    ptr_add_table = add_backtable;
}

// seg008:1F48
#[no_mangle]
pub unsafe extern "C" fn draw_tile2() {
    draw_tile_right();
    draw_tile_anim_right();
    draw_tile_base();
    draw_tile_anim();
    draw_tile_bottom(0);
    draw_loose(0);
}

// seg008:1937
#[no_mangle]
pub unsafe extern "C" fn alter_mods_allrm() {
    // USE_COLORED_TORCHES (always on)
    memset(torch_colors.as_mut_ptr() as *mut c_void, 0, core::mem::size_of_val(&torch_colors));

    if level.used_rooms > 24 { level.used_rooms = 24; }
    for room in 1u16..=level.used_rooms as u16 {
        get_room_address(room as c_int);
        room_L = level.roomlinks[room as usize - 1].left as u16;
        room_R = level.roomlinks[room as usize - 1].right as u16;
        for tilepos in 0usize..30 {
            load_alter_mod(tilepos as c_int);
        }
    }
}

// seg008:198E
#[no_mangle]
pub unsafe extern "C" fn load_alter_mod(tilepos: c_int) {
    let curr_tile_modif = curr_room_modif.add(tilepos as usize);
    let tiletype = (*curr_room_tiles.add(tilepos as usize)) & 0x1F;
    match tiletype {
        t if t == tiles_tiles_4_gate as u8 => {
            if *curr_tile_modif == 1 { *curr_tile_modif = 188; } else { *curr_tile_modif = 0; }
        }
        t if t == tiles_tiles_11_loose as u8 => {
            *curr_tile_modif = 0;
        }
        t if t == tiles_tiles_10_potion as u8 => {
            *curr_tile_modif <<= 3;
            // USE_COPYPROT (always on)
            if current_level == 15 {
                if copyprot_room_at(copyprot_plac as usize) == loaded_room
                    && copyprot_tile_at(copyprot_plac as usize) == tilepos as u16
                {
                    *curr_tile_modif = 6 << 3; // open potion
                }
            }
        }
        t if t == tiles_tiles_20_wall as u8 => {
            let stored_modif = *curr_tile_modif;
            if stored_modif == 1 { *curr_tile_modif = 0x80; }
            else { *curr_tile_modif = stored_modif << 4; }
            wall_connection_block(tilepos as usize, curr_tile_modif, tiletype);
        }
        t if t == tiles_tiles_0_empty as u8 || t == tiles_tiles_1_floor as u8 => {
            // USE_FAKE_TILES (always on): fake walls
            if (*curr_tile_modif & 7) == 5 {
                wall_connection_block(tilepos as usize, curr_tile_modif, tiletype);
            }
        }
        t if t == tiles_tiles_19_torch as u8 || t == tiles_tiles_30_torch_with_debris as u8 => {
            // USE_COLORED_TORCHES (always on)
            torch_colors[loaded_room as usize][tilepos as usize] = *curr_tile_modif;
            *curr_tile_modif = 0;
        }
        _ => {}
    }
}

// seg008:1D29
#[no_mangle]
pub unsafe extern "C" fn draw_leveldoor() {
    let ybottom = draw_main_y as c_int - 13;
    leveldoor_right = (draw_xh << 3) as u16 + 48;
    if (*custom).tbl_level_type[current_level as usize] != 0 { leveldoor_right += 8; }
    add_backtable(chtabs_id_chtab_6_environment as c_short, 99, draw_xh as i8 + 1, 0, ybottom, blitters_blitters_0_no_transp as c_int, 0);
    if modifier_left != 0 {
        if level.start_room != drawn_room as u8 {
            add_backtable(chtabs_id_chtab_6_environment as c_short, 144, draw_xh as i8 + 1, 0, ybottom - 4, blitters_blitters_0_no_transp as c_int, 0);
        } else {
            let leveldoor_width: c_short = if (*custom).tbl_level_type[current_level as usize] == 0 { 39 } else { 48 };
            let x_low: i8 = if (*custom).tbl_level_type[current_level as usize] == 0 { 2 } else { 0 };
            add_wipetable(0, (8 * (draw_xh + 1)) as c_short + x_low as c_short, (ybottom - 4) as c_short, 45, leveldoor_width, 0);
        }
    }
    leveldoor_ybottom = (ybottom as c_int - (modifier_left & 3) as c_int - 48) as u16;
    let y = ybottom - modifier_left as c_int;
    loop {
        add_backtable(chtabs_id_chtab_6_environment as c_short, 33, draw_xh as i8 + 1, 0, leveldoor_ybottom as c_int, blitters_blitters_0_no_transp as c_int, 0);
        if y > leveldoor_ybottom as c_int { leveldoor_ybottom = (leveldoor_ybottom as c_int + 4) as u16; }
        else { break; }
    }
    add_backtable(chtabs_id_chtab_6_environment as c_short, 34, draw_xh as i8 + 1, 0, draw_main_y as c_int - 64, blitters_blitters_0_no_transp as c_int, 0);
}

// seg008:24A8
#[no_mangle]
pub unsafe extern "C" fn show_time() {
    // FIX_ONE_HP_STOPS_BLINKING (always on)
    global_blink_state = !global_blink_state;

    if Kid.alive < 0
        // FREEZE_TIME_DURING_END_MUSIC (always on)
        && !((*fixes).enable_freeze_time_during_end_music != 0 && next_level != current_level)
        // ALLOW_INFINITE_TIME (always on): prevent overflow
        && !(rem_min == i16::MIN && rem_tick == 1)
        && rem_min != 0
        && (current_level < (*custom).victory_stops_time_level
            || (current_level == (*custom).victory_stops_time_level && leveldoor_open == 0))
        && current_level < 15
    {
        rem_tick -= 1;
        if rem_tick == 0 {
            rem_tick = 719;
            rem_min -= 1;
            if rem_min > 0 && (rem_min <= 5 || rem_min % 5 == 0) {
                is_show_time = 1;
            } else if rem_min < 0 {
                is_show_time = if (!rem_min) % 5 == 0 { 1 } else { 0 };
            }
        } else if rem_min == 1 && rem_tick % 12 == 0 {
            is_show_time = 1;
            text_time_remaining = 0;
        }
    }
    if is_show_time != 0 && text_time_remaining == 0 {
        text_time_remaining = 24;
        text_time_total = 24;
        if rem_min > 0 {
            let mut buf = [0i8; 40];
            if rem_min == 1 {
                let rem_sec = (rem_tick + 1) / 12;
                if rem_sec == 1 {
                    let s = b"1 SECOND LEFT\0";
                    for (i, &b) in s.iter().enumerate() { buf[i] = b as i8; }
                    text_time_remaining = 12;
                    text_time_total = 12;
                } else {
                    let s = format!("{} SECONDS LEFT\0", rem_sec);
                    for (i, b) in s.bytes().enumerate().take(39) { buf[i] = b as i8; }
                }
            } else {
                let s = format!("{} MINUTES LEFT\0", rem_min);
                for (i, b) in s.bytes().enumerate().take(39) { buf[i] = b as i8; }
            }
            display_text_bottom(buf.as_ptr());
        } else {
            // ALLOW_INFINITE_TIME (always on)
            if rem_min < 0 {
                let mut buf = [0i8; 40];
                let inv = !rem_min;
                if inv == 0 {
                    text_time_remaining = 0;
                    text_time_total = 0;
                    // display empty string (clears text area)
                    display_text_bottom(buf.as_ptr());
                } else if inv == 1 {
                    let s = b"1 MINUTE PASSED\0";
                    for (i, &b) in s.iter().enumerate() { buf[i] = b as i8; }
                    display_text_bottom(buf.as_ptr());
                } else {
                    let s = format!("{} MINUTES PASSED\0", inv);
                    for (i, b) in s.bytes().enumerate().take(39) { buf[i] = b as i8; }
                    display_text_bottom(buf.as_ptr());
                }
            } else {
                // rem_min == 0
                display_text_bottom(b"TIME HAS EXPIRED!\0".as_ptr() as *const c_char);
            }
        }
        is_show_time = 0;
    }
}

// seg008:25A8
#[no_mangle]
pub unsafe extern "C" fn show_level() {
    // FIX_LEVEL_14_RESTARTING (always on)
    text_time_remaining = 0;
    text_time_total = 0;
    let disp_level = if current_level == 13 { (*custom).level_13_level_number as u16 } else { current_level };
    if disp_level != 0 && disp_level < (*custom).hide_level_number_from_level && seamless == 0 {
        text_time_remaining = 24;
        text_time_total = 24;
        let s = format!("LEVEL {}\0", disp_level);
        let mut buf = [0i8; 32];
        for (i, b) in s.bytes().enumerate().take(31) { buf[i] = b as i8; }
        display_text_bottom(buf.as_ptr());
        is_show_time = 1;
    }
    seamless = 0;
}

// seg008:2602
#[no_mangle]
pub unsafe extern "C" fn calc_screen_x_coord(logical_x: c_short) -> c_short {
    (logical_x as i32 * 320 / 280) as c_short
}

// seg008:2644
#[no_mangle]
pub unsafe extern "C" fn display_text_bottom(text: *const c_char) {
    draw_rect(&rect_bottom_text, colorids_color_0_black as c_int);
    show_text(&rect_bottom_text, halign_center as c_int, valign_bottom as c_int, text);
    // USE_TEXT is on, so SDL_SetWindowTitle is NOT called here
}

// seg008:266D
#[no_mangle]
pub unsafe extern "C" fn erase_bottom_text(arg_0: c_int) {
    draw_rect(&rect_bottom_text, colorids_color_0_black as c_int);
    if arg_0 != 0 {
        text_time_total = 0;
        text_time_remaining = 0;
    }
    // USE_TEXT is on, so SDL_SetWindowTitle is NOT called here
}

// Wall drawing constants (local to wall_pattern)
const RSET_WALL: c_short = 7;
const RES_WALL_FACE_MAIN: c_int = 1;
const RES_WALL_FACE_TOP: c_int = 2;
const RES_WALL_CENTRE_BASE: c_int = 3;
const RES_WALL_CENTRE_MAIN: c_int = 4;
const RES_WALL_RIGHT_BASE: c_int = 5;
const RES_WALL_RIGHT_MAIN: c_int = 6;
const RES_WALL_SINGLE_BASE: c_int = 7;
const RES_WALL_SINGLE_MAIN: c_int = 8;
const RES_WALL_LEFT_BASE: c_int = 9;
const RES_WALL_LEFT_MAIN: c_int = 10;
const RES_WALL_DIVIDER1: c_int = 11;
const RES_WALL_DIVIDER2: c_int = 12;
const RES_WALL_RNDBLOCK: c_int = 13;
const RES_WALL_MARK_TL: c_int = 14;
const RES_WALL_MARK_BL: c_int = 15;
const RES_WALL_MARK_TR: c_int = 16;
const RES_WALL_MARK_BR: c_int = 17;
const BLIT_NO_TRANS: c_int = 0;
const BLIT_TRANS: c_int = 16;
const WALL_MODIFIER_SWS: u8 = 0;
const WALL_MODIFIER_SWW: u8 = 1;
const WALL_MODIFIER_WWS: u8 = 2;
const WALL_MODIFIER_WWW: u8 = 3;

// seg008:268F
#[no_mangle]
pub unsafe extern "C" fn wall_pattern(which_part: c_int, which_table: c_int) {
    let saved_sim = ptr_add_table;
    if which_table == 0 { ptr_add_table = add_backtable; } else { ptr_add_table = add_foretable; }
    let saved_prng_state = random_seed;
    random_seed = (drawn_room as u32).wrapping_add(tbl_line_at(drawn_row as usize) as u32).wrapping_add(drawn_col as u32);
    prandom(1);
    let is_dungeon = ((*custom).tbl_level_type[current_level as usize] < 1) || (*custom).enable_wda_in_palace != 0;
    if !is_dungeon && graphics_mode == grmodes_gmMcgaVga as u8 {
        if which_part != 0 {
            add_wipetable(which_table as i8, (8 * draw_xh) as c_short, (draw_main_y - 40) as c_short, 20, (4 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + drawn_col as usize] as i8);
            add_wipetable(which_table as i8, (8 * draw_xh) as c_short, (draw_main_y - 19) as c_short, 21, (2 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 11 + drawn_col as usize] as i8);
            add_wipetable(which_table as i8, (8 * (draw_xh + 2)) as c_short, (draw_main_y - 19) as c_short, 21, (2 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 12 + drawn_col as usize] as i8);
            add_wipetable(which_table as i8, (8 * draw_xh) as c_short, draw_main_y as c_short, 19, (1 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 22 + drawn_col as usize] as i8);
            add_wipetable(which_table as i8, (8 * (draw_xh + 1)) as c_short, draw_main_y as c_short, 19, (3 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 23 + drawn_col as usize] as i8);
            ptr_add_table(RSET_WALL, prandom(2) as c_int + 3, draw_xh as i8 + 3, 0, draw_main_y as c_int - 53, blitters_blitters_46h_mono_6 as c_int, 0);
            ptr_add_table(RSET_WALL, prandom(2) as c_int + 6, draw_xh as i8, 0, draw_main_y as c_int - 34, blitters_blitters_46h_mono_6 as c_int, 0);
            ptr_add_table(RSET_WALL, prandom(2) as c_int + 9, draw_xh as i8, 0, draw_main_y as c_int - 13, blitters_blitters_46h_mono_6 as c_int, 0);
            ptr_add_table(RSET_WALL, prandom(2) as c_int + 12, draw_xh as i8, 0, draw_main_y as c_int, blitters_blitters_46h_mono_6 as c_int, 0);
        }
        add_wipetable(which_table as i8, (8 * draw_xh) as c_short, draw_bottom_y as c_short, 3, (4 * 8) as c_short, palace_wall_colors[44 * drawn_row as usize + 33 + drawn_col as usize] as i8);
        ptr_add_table(RSET_WALL, prandom(2) as c_int + 15, draw_xh as i8, 0, draw_bottom_y as c_int, blitters_blitters_46h_mono_6 as c_int, 0);
    } else {
        let middle_divider = prandom(1) as c_int;
        let middle_divider_offset = prandom(4) as c_int;
        let bottom_divider = prandom(1) as c_int;
        let bottom_divider_offset = prandom(4) as c_int;
        let bg_modifier = curr_modifier & 0x7F;
        match bg_modifier {
            WALL_MODIFIER_WWW => {
                if which_part != 0 {
                    if prandom(4) == 0 {
                        ptr_add_table(RSET_WALL, RES_WALL_RNDBLOCK, draw_xh as i8, 0, draw_bottom_y as c_int - 42, BLIT_NO_TRANS, 0);
                    }
                    ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + middle_divider, draw_xh as i8 + 1, middle_divider_offset as i8, draw_bottom_y as c_int - 21, BLIT_TRANS, 0);
                }
                ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + bottom_divider, draw_xh as i8, bottom_divider_offset as i8, draw_bottom_y as c_int, BLIT_TRANS, 0);
                if which_part != 0 && is_dungeon {
                    if prandom(4) == 0 { draw_right_mark(prandom(3) as u16, middle_divider_offset as u16); }
                    if prandom(4) == 0 { draw_left_mark(prandom(4) as u16, (middle_divider_offset - middle_divider) as u16, (bottom_divider_offset - bottom_divider) as u16); }
                }
            }
            WALL_MODIFIER_SWS => {
                if is_dungeon && which_part != 0 {
                    if prandom(6) == 0 { draw_left_mark(prandom(1) as u16, (middle_divider_offset - middle_divider) as u16, (bottom_divider_offset - bottom_divider) as u16); }
                }
            }
            WALL_MODIFIER_SWW => {
                if which_part != 0 {
                    if prandom(4) == 0 {
                        ptr_add_table(RSET_WALL, RES_WALL_RNDBLOCK, draw_xh as i8, 0, draw_bottom_y as c_int - 42, BLIT_NO_TRANS, 0);
                    }
                    ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + middle_divider, draw_xh as i8 + 1, middle_divider_offset as i8, draw_bottom_y as c_int - 21, BLIT_TRANS, 0);
                    if is_dungeon {
                        if prandom(4) == 0 { draw_right_mark(prandom(3) as u16, middle_divider_offset as u16); }
                        if prandom(4) == 0 { draw_left_mark(prandom(3) as u16, (middle_divider_offset - middle_divider) as u16, (bottom_divider_offset - bottom_divider) as u16); }
                    }
                }
            }
            WALL_MODIFIER_WWS => {
                if which_part != 0 {
                    ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + middle_divider, draw_xh as i8 + 1, middle_divider_offset as i8, draw_bottom_y as c_int - 21, BLIT_TRANS, 0);
                }
                ptr_add_table(RSET_WALL, RES_WALL_DIVIDER1 + bottom_divider, draw_xh as i8, bottom_divider_offset as i8, draw_bottom_y as c_int, BLIT_TRANS, 0);
                if which_part != 0 && is_dungeon {
                    if prandom(4) == 0 { draw_right_mark(prandom(1) as u16 + 2, middle_divider_offset as u16); }
                    if prandom(4) == 0 { draw_left_mark(prandom(4) as u16, (middle_divider_offset - middle_divider) as u16, (bottom_divider_offset - bottom_divider) as u16); }
                }
            }
            _ => {}
        }
    }
    random_seed = saved_prng_state;
    ptr_add_table = saved_sim;
}

#[no_mangle]
pub unsafe extern "C" fn draw_left_mark(decal_variant: u16, arg2: u16, arg1: u16) {
    static LPOS: [u16; 5] = [58, 41, 37, 20, 16];
    let mut image_id = RES_WALL_MARK_TL;
    if decal_variant % 2 != 0 { image_id = RES_WALL_MARK_BL; }
    let lv2 = if decal_variant > 3 {
        arg1.wrapping_add(6)
    } else if decal_variant > 1 {
        arg2.wrapping_add(6)
    } else {
        0
    };
    ptr_add_table(
        RSET_WALL, image_id,
        draw_xh as i8 + (decal_variant == 2 || decal_variant == 3) as i8,
        lv2 as i8,
        draw_bottom_y as c_int - LPOS[decal_variant as usize] as c_int,
        BLIT_TRANS, 0,
    );
}

#[no_mangle]
pub unsafe extern "C" fn draw_right_mark(decal_variant: u16, mut arg1: u16) {
    static RPOS: [u16; 4] = [52, 42, 31, 21];
    let mut image_id = RES_WALL_MARK_TR;
    if decal_variant % 2 != 0 { image_id = RES_WALL_MARK_BR; }
    if decal_variant < 2 {
        arg1 = 24;
    } else {
        arg1 = arg1.wrapping_sub(3);
    }
    ptr_add_table(
        RSET_WALL, image_id,
        draw_xh as i8 + (decal_variant > 1) as i8,
        arg1 as i8,
        draw_bottom_y as c_int - RPOS[decal_variant as usize] as c_int,
        BLIT_TRANS, 0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // can_see_bottomleft returns 1 only for the four tile types that allow
    // seeing the bottom-left corner: empty, bigpillar_top, doortop, lattice_down.
    #[test]
    fn can_see_bottomleft_transparent_tiles() {
        unsafe {
            for &t in &[
                tiles_tiles_0_empty as u8,
                tiles_tiles_9_bigpillar_top as u8,
                tiles_tiles_12_doortop as u8,
                tiles_tiles_26_lattice_down as u8,
            ] {
                curr_tile = t;
                assert_eq!(can_see_bottomleft(), 1, "tile {t} should be transparent");
            }
            // A solid tile must not be transparent.
            curr_tile = tiles_tiles_1_floor as u8;
            assert_eq!(can_see_bottomleft(), 0);
            curr_tile = tiles_tiles_20_wall as u8;
            assert_eq!(can_see_bottomleft(), 0);
        }
    }

    // get_spike_frame: modifier bit 7 set → frame 5; otherwise raw modifier.
    #[test]
    fn get_spike_frame_mapping() {
        unsafe {
            assert_eq!(get_spike_frame(0), 0);
            assert_eq!(get_spike_frame(3), 3);
            assert_eq!(get_spike_frame(0x80), 5); // bit 7 set → 5
            assert_eq!(get_spike_frame(0x83), 5); // any bit-7 value → 5
        }
    }

    // get_loose_frame: high bit clear and value ≤ 10 → raw value;
    // high bit set or value > 10 (with default loose_floor_delay ≤ 11) → masked + clamp to 1.
    #[test]
    fn get_loose_frame_normal_range() {
        setup();
        unsafe {
            for v in 0u8..=10 {
                assert_eq!(get_loose_frame(v), v as c_int, "modifier {v}");
            }
        }
    }

    #[test]
    fn get_loose_frame_high_bit_clamps() {
        setup();
        unsafe {
            // 0x80 | 0 = 0x80 → masked = 0 ≤ 10, so return 0
            assert_eq!(get_loose_frame(0x80), 0);
            // 0x80 | 11 = 0x8B → masked = 11 > 10, return 1
            assert_eq!(get_loose_frame(0x8B), 1);
            // 0x80 | 5 = 0x85 → masked = 5 ≤ 10, return 5
            assert_eq!(get_loose_frame(0x85), 5);
        }
    }

    // calc_screen_x_coord scales logical x coordinates from 280-pixel space to 320-pixel space.
    #[test]
    fn calc_screen_x_coord_scaling() {
        unsafe {
            assert_eq!(calc_screen_x_coord(0), 0);
            assert_eq!(calc_screen_x_coord(280), 320);
            assert_eq!(calc_screen_x_coord(140), 160);
        }
    }

    // tile_table spot-check: verify a few entries match the C initializer exactly.
    #[test]
    fn tile_table_spot_check() {
        // 0x01 floor: base_id=41, floor_left=1, base_y=0, right_id=42, bottom_id=43
        let floor = &tile_table[1];
        assert_eq!(floor.base_id, 41);
        assert_eq!(floor.floor_left, 1);
        assert_eq!(floor.base_y, 0);
        assert_eq!(floor.right_id, 42);
        assert_eq!(floor.bottom_id, 43);

        // 0x00 empty: all zeros
        let empty = &tile_table[0];
        assert_eq!(empty.base_id, 0);
        assert_eq!(empty.fore_id, 0);

        // 0x14 wall (index 20): base_id=0, right_id=1, topright_id=2
        let wall = &tile_table[20];
        assert_eq!(wall.base_id, 0);
        assert_eq!(wall.right_id, 1);
        assert_eq!(wall.topright_id, 2);

        // 0x1A lattice_down (index 26): base_id=1, fore_y=-53
        let lattice = &tile_table[26];
        assert_eq!(lattice.base_id, 1);
        assert_eq!(lattice.fore_y, -53);
    }

    // Regression: draw_other_overlay must look up the tile two columns to the
    // left into LOCAL temporaries, never into the global curr_tile/curr_modifier.
    // Clobbering curr_tile here makes a later draw_tile_fore render a wall in the
    // foreground (extra foretable entries) for a non-wall tile.
    #[test]
    fn draw_other_overlay_does_not_clobber_curr_tile() {
        unsafe {
            set_options_to_default();
            let mut tiles = [tiles_tiles_1_floor as u8; 30];
            let mut modifs = [0u8; 30];
            curr_room_tiles = tiles.as_mut_ptr();
            curr_room_modif = modifs.as_mut_ptr();
            drawn_room = 1;
            drawn_row = 0;
            drawn_col = 2;
            tile_left = tiles_tiles_1_floor as u8; // not empty -> first branch skipped
            curr_tile = tiles_tiles_20_wall as u8; // not empty -> enters else-if
            curr_modifier = 7;
            // Tile two columns left (col 0) is floor (non-empty), so the branch
            // body is not executed; only get_tile_to_draw's out-params are written.
            draw_other_overlay();
            assert_eq!(curr_tile, tiles_tiles_20_wall as u8,
                "draw_other_overlay must not modify global curr_tile");
            assert_eq!(curr_modifier, 7,
                "draw_other_overlay must not modify global curr_modifier");
            curr_room_tiles = core::ptr::null_mut();
            curr_room_modif = core::ptr::null_mut();
            set_options_to_default();
        }
    }
}

// seg008:1E0C
#[no_mangle]
pub unsafe extern "C" fn get_room_address(room: c_int) {
    loaded_room = room as u16;
    if room != 0 {
        curr_room_tiles = level.fg.as_mut_ptr().add((room as usize - 1) * 30);
        curr_room_modif = level.bg.as_mut_ptr().add((room as usize - 1) * 30);
    }
}
