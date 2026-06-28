// Tile/character system — ported from seg006.c.
// All public functions are #[no_mangle] extern "C" for transparent C linkage.

#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_int, c_short};
use super::*;

// seqtbl is defined in seqtbl.c with no header; declare it directly.
extern "C" {
    #[link_name = "seqtbl"]
    static seqtbl_data: [u8; 0];
}
const SEQTBL_BASE: u16 = 0x196E;
unsafe fn seqtbl_byte(idx: usize) -> u8 {
    *core::ptr::addr_of!(seqtbl_data).cast::<u8>().add(idx)
}

// ── SEQ opcode constants ──────────────────────────────────────────────────────

const SEQ_DX:             u8 = 0xFB;
const SEQ_DY:             u8 = 0xFA;
const SEQ_FLIP:           u8 = 0xFE;
const SEQ_JMP_IF_FEATHER: u8 = 0xF7;
const SEQ_JMP:            u8 = 0xFF;
const SEQ_UP:             u8 = 0xFD;
const SEQ_DOWN:           u8 = 0xFC;
const SEQ_ACTION:         u8 = 0xF9;
const SEQ_SET_FALL:       u8 = 0xF8;
const SEQ_KNOCK_UP:       u8 = 0xF5;
const SEQ_KNOCK_DOWN:     u8 = 0xF4;
const SEQ_SOUND:          u8 = 0xF2;
const SEQ_END_LEVEL:      u8 = 0xF1;
const SEQ_GET_ITEM:       u8 = 0xF3;
const SEQ_DIE:            u8 = 0xF6;

const SND_SILENT:   u8 = 0;
const SND_FOOTSTEP: u8 = 1;
const SND_BUMP:     u8 = 2;
const SND_DRINK:    u8 = 3;
const SND_LEVEL:    u8 = 4;

// ── Compile-time constants (all feature flags active) ─────────────────────────

const SCREENSPACE_X: i32 = 58;
const TILE_SIZEX:    i32 = 14;
const TILE_SIZEY:    i32 = 63;
const TILE_MIDX:     i32 = 7;
const TILE_RIGHTX:   i32 = 13;
const FIRST_ONSCREEN_COLUMN: i32 = 5;
const FALLING_SPEED_MAX:           i8 = 33;
const FALLING_SPEED_ACCEL:         i8 = 3;
const FALLING_SPEED_MAX_FEATHER:   i8 = 4;
const FALLING_SPEED_ACCEL_FEATHER: i8 = 1;

// ── Raw-pointer helpers for incomplete-array globals ──────────────────────────

// dir_front / dir_behind: extern const sbyte[] (incomplete), bindgen → [i8; 0].
unsafe fn dir_front_at(idx: usize) -> i8 {
    *core::ptr::addr_of!(dir_front).cast::<i8>().add(idx)
}
unsafe fn dir_behind_at(idx: usize) -> i8 {
    *core::ptr::addr_of!(dir_behind).cast::<i8>().add(idx)
}
// tbl_line: extern const word[] (incomplete), bindgen → [u16; 0].
unsafe fn tbl_line_at(idx: usize) -> u16 {
    *core::ptr::addr_of!(tbl_line).cast::<u16>().add(idx)
}
// y_clip: extern const short[] (incomplete), bindgen → [i16; 0].
unsafe fn y_clip_at(idx: usize) -> i16 {
    *core::ptr::addr_of!(y_clip).cast::<i16>().add(idx)
}

// ── Const constructors for table types ───────────────────────────────────────

const fn ft(image: u8, sword: u8, dx: i8, dy: i8, flags: u8) -> frame_type {
    frame_type { image, sword, dx, dy, flags }
}
const fn st(id: u8, x: i8, y: i8) -> sword_table_type {
    sword_table_type { id, x, y }
}

// ── DOS overflow-simulation tables for get_tile_div_mod ──────────────────────

#[rustfmt::skip]
static TILE_DIV_TBL: [i8; 256] = [
    -5,-5,
    -4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,-4,
    -3,-3,-3,-3,-3,-3,-3,-3,-3,-3,-3,-3,-3,-3,
    -2,-2,-2,-2,-2,-2,-2,-2,-2,-2,-2,-2,-2,-2,
    -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
     1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
     2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
     3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
     4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
     5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
     6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
     7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
     8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
     9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9,
    10,10,10,10,10,10,10,10,10,10,10,10,10,10,
    11,11,11,11,11,11,11,11,11,11,11,11,11,11,
    12,12,12,12,12,12,12,12,12,12,12,12,12,12,
    13,13,13,13,13,13,13,13,13,13,13,13,13,13,
    14,14,
];

#[rustfmt::skip]
static TILE_MOD_TBL: [u8; 256] = [
    12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,
     0, 1,
];

// Bytes immediately before tile_div_tbl[] in DOS memory (for negative xpos).
static BOGUS_BEFORE: [u8; 34] = [
    0x02,0x00,0x41,0x00,0x80,0x00,0xBF,0x00,0xFE,0x00,0xFF,0x01,0x01,0xFF,
    0xC4,0xFF,0x03,0x00,0x42,0x00,0x81,0x00,0xC0,0x00,0xF8,0xFF,0x37,0x00,
    0x76,0x00,0xB5,0x00,0xF4,0x00,
];

// Bytes immediately after tile_mod_tbl[] in DOS memory (for positive overflow).
static BOGUS_AFTER: [u8; 34] = [
    0xF4,0x02,0x10,0x1E,0x2C,0x3A,0x48,0x56,0x64,0x72,0x80,0x8E,0x9C,0xAA,
    0xB8,0xC6,0xD4,0xE2,0xF0,0xFE,0x00,0x0A,0x00,0xFF,0x00,0x00,0x00,0x00,
    0x0A,0x0D,0x00,0x00,0x00,0x00,
];

// ── Frame tables ──────────────────────────────────────────────────────────────

#[rustfmt::skip]
static FRAME_TABLE_KID: [frame_type; 241] = [
ft(255,0x00| 0,  0,  0,0x00| 0), // 0
ft(  0,0x00| 0,  1,  0,0xC0| 4),
ft(  1,0x00| 0,  1,  0,0x40| 4),
ft(  2,0x00| 0,  3,  0,0x40| 7),
ft(  3,0x00| 0,  4,  0,0x40| 8),
ft(  4,0x00| 0,  0,  0,0xE0| 6),
ft(  5,0x00| 0,  0,  0,0x40| 9),
ft(  6,0x00| 0,  0,  0,0x40|10),
ft(  7,0x00| 0,  0,  0,0xC0| 5),
ft(  8,0x00| 0,  0,  0,0x40| 4),
ft(  9,0x00| 0,  0,  0,0x40| 7), // 10
ft( 10,0x00| 0,  0,  0,0x40|11),
ft( 11,0x00| 0,  0,  0,0x40| 3),
ft( 12,0x00| 0,  0,  0,0xC0| 3),
ft( 13,0x00| 0,  0,  0,0x40| 7),
ft( 14,0x00| 9,  0,  0,0x40| 3),
ft( 15,0x00| 0,  0,  0,0xC0| 3),
ft( 16,0x00| 0,  0,  0,0x40| 4),
ft( 17,0x00| 0,  0,  0,0x40| 6),
ft( 18,0x00| 0,  0,  0,0x40| 8),
ft( 19,0x00| 0,  0,  0,0x80| 9), // 20
ft( 20,0x00| 0,  0,  0,0x00|11),
ft( 21,0x00| 0,  0,  0,0x80|11),
ft( 22,0x00| 0,  0,  0,0x00|17),
ft( 23,0x00| 0,  0,  0,0x00| 7),
ft( 24,0x00| 0,  0,  0,0x00| 5),
ft( 25,0x00| 0,  0,  0,0xC0| 1),
ft( 26,0x00| 0,  0,  0,0xC0| 6),
ft( 27,0x00| 0,  0,  0,0x40| 3),
ft( 28,0x00| 0,  0,  0,0x40| 8),
ft( 29,0x00| 0,  0,  0,0x40| 2), // 30
ft( 30,0x00| 0,  0,  0,0x40| 2),
ft( 31,0x00| 0,  0,  0,0xC0| 2),
ft( 32,0x00| 0,  0,  0,0xC0| 2),
ft( 33,0x00| 0,  0,  0,0x40| 3),
ft( 34,0x00| 0,  0,  0,0x40| 8),
ft( 35,0x00| 0,  0,  0,0xC0|14),
ft( 36,0x00| 0,  0,  0,0xC0| 1),
ft( 37,0x00| 0,  0,  0,0x40| 5),
ft( 38,0x00| 0,  0,  0,0x80|14),
ft( 39,0x00| 0,  0,  0,0x00|11), // 40
ft( 40,0x00| 0,  0,  0,0x80|11),
ft( 41,0x00| 0,  0,  0,0x80|10),
ft( 42,0x00| 0,  0,  0,0x00| 1),
ft( 43,0x00| 0,  0,  0,0xC0| 4),
ft( 44,0x00| 0,  0,  0,0xC0| 3),
ft( 45,0x00| 0,  0,  0,0xC0| 3),
ft( 46,0x00| 0,  0,  0,0xA0| 5),
ft( 47,0x00| 0,  0,  0,0xA0| 4),
ft( 48,0x00| 0,  0,  0,0x60| 6),
ft( 49,0x00| 0,  4,  0,0x60| 7), // 50
ft( 50,0x00| 0,  3,  0,0x60| 6),
ft( 51,0x00| 0,  1,  0,0x40| 4),
ft( 64,0x00| 0,  0,  0,0xC0| 2),
ft( 65,0x00| 0,  0,  0,0x40| 1),
ft( 66,0x00| 0,  0,  0,0x40| 2),
ft( 67,0x00| 0,  0,  0,0x00| 0),
ft( 68,0x00| 0,  0,  0,0x00| 0),
ft( 69,0x00| 0,  0,  0,0x80| 0),
ft( 70,0x00| 0,  0,  0,0x00| 0),
ft( 71,0x00| 0,  0,  0,0x80| 0), // 60
ft( 72,0x00| 0,  0,  0,0x00| 0),
ft( 73,0x00| 0,  0,  0,0x80| 0),
ft( 74,0x00| 0,  0,  0,0x00| 0),
ft( 75,0x00| 0,  0,  0,0x00| 0),
ft( 76,0x00| 0,  0,  0,0x80| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 80,0x00| 0, -2,  0,0x40| 1),
ft( 81,0x00| 0, -2,  0,0x40| 1),
ft( 82,0x00| 0, -1,  0,0xC0| 2),
ft( 83,0x00| 0, -2,  0,0x40| 2), // 70
ft( 84,0x00| 0, -2,  0,0x40| 1),
ft( 85,0x00| 0, -2,  0,0x40| 1),
ft( 86,0x00| 0, -2,  0,0x40| 1),
ft( 87,0x00| 0, -1,  0,0x00| 7),
ft( 88,0x00| 0, -1,  0,0x00| 5),
ft( 89,0x00| 0,  2,  0,0x00| 7),
ft( 90,0x00| 0,  2,  0,0x00| 7),
ft( 91,0x00| 0,  2, -3,0x00| 0),
ft( 92,0x00| 0,  2,-10,0x00| 0),
ft( 93,0x00| 0,  2,-11,0x80| 0), // 80
ft( 94,0x00| 0,  3, -2,0x40| 3),
ft( 95,0x00| 0,  3,  0,0xC0| 3),
ft( 96,0x00| 0,  3,  0,0xC0| 3),
ft( 97,0x00| 0,  3,  0,0x60| 3),
ft( 98,0x00| 0,  4,  0,0xE0| 3),
ft( 28,0x00| 0,  0,  0,0x00| 0),
ft( 99,0x00| 0,  7,-14,0x80| 0),
ft(100,0x00| 0,  7,-12,0x80| 0),
ft(101,0x00| 0,  4,-12,0x00| 0),
ft(102,0x00| 0,  3,-10,0x80| 0), // 90
ft(103,0x00| 0,  2,-10,0x80| 0),
ft(104,0x00| 0,  1,-10,0x80| 0),
ft(105,0x00| 0,  0,-11,0x00| 0),
ft(106,0x00| 0, -1,-12,0x00| 0),
ft(107,0x00| 0, -1,-14,0x00| 0),
ft(108,0x00| 0, -1,-14,0x00| 0),
ft(109,0x00| 0, -1,-15,0x80| 0),
ft(110,0x00| 0, -1,-15,0x80| 0),
ft(111,0x00| 0,  0,-15,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0), // 100
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(112,0x00| 0,  0,  0,0xC0| 6),
ft(113,0x00| 0,  0,  0,0x40| 6),
ft(114,0x00| 0,  0,  0,0xC0| 5),
ft(115,0x00| 0,  0,  0,0x40| 5),
ft(116,0x00| 0,  0,  0,0xC0| 2),
ft(117,0x00| 0,  0,  0,0xC0| 4),
ft(118,0x00| 0,  0,  0,0xC0| 5),
ft(119,0x00| 0,  0,  0,0x40| 6),
ft(120,0x00| 0,  0,  0,0x40| 7), // 110
ft(121,0x00| 0,  0,  0,0x40| 7),
ft(122,0x00| 0,  0,  0,0x40| 9),
ft(123,0x00| 0,  0,  0,0xC0| 8),
ft(124,0x00| 0,  0,  0,0xC0| 9),
ft(125,0x00| 0,  0,  0,0x40| 9),
ft(126,0x00| 0,  0,  0,0x40| 5),
ft(127,0x00| 0,  2,  0,0x40| 5),
ft(128,0x00| 0,  2,  0,0xC0| 5),
ft(129,0x00| 0,  0,  0,0xC0| 3),
ft(255,0x00| 0,  0,  0,0x00| 0), // 120
ft(133,0x00| 0,  0,  0,0x40| 3),
ft(134,0x00| 0,  0,  0,0xC0| 4),
ft(135,0x00| 0,  0,  0,0xC0| 5),
ft(136,0x00| 0,  0,  0,0x40| 8),
ft(137,0x00| 0,  0,  0,0x60|12),
ft(138,0x00| 0,  0,  0,0xE0|15),
ft(139,0x00| 0,  0,  0,0x60| 3),
ft(140,0x00| 0,  0,  0,0xC0| 3),
ft(141,0x00| 0,  0,  0,0x40| 3),
ft(142,0x00| 0,  0,  0,0x40| 3), // 130
ft(143,0x00| 0,  0,  0,0x40| 4),
ft(144,0x00| 0,  0,  0,0x40| 4),
ft(172,0x00| 0,  0,  1,0xC0| 1),
ft(173,0x00| 0,  0,  1,0xC0| 7),
ft(145,0x00| 0,  0,-12,0x00| 1),
ft(146,0x00| 0,  0,-21,0x00| 0),
ft(147,0x00| 0,  1,-26,0x80| 0),
ft(148,0x00| 0,  4,-32,0x80| 0),
ft(149,0x00| 0,  6,-36,0x80| 1),
ft(150,0x00| 0,  7,-41,0x80| 2), // 140
ft(151,0x00| 0,  2, 17,0x40| 2),
ft(152,0x00| 0,  4,  9,0xC0| 4),
ft(153,0x00| 0,  4,  5,0xC0| 9),
ft(154,0x00| 0,  4,  4,0xC0| 8),
ft(155,0x00| 0,  5,  0,0x60| 9),
ft(156,0x00| 0,  5,  0,0xE0| 9),
ft(157,0x00| 0,  5,  0,0xE0| 8),
ft(158,0x00| 0,  5,  0,0x60| 9),
ft(159,0x00| 0,  5,  0,0x60| 9),
ft(184,0x00|16,  0,  2,0x80| 0), // 150
ft(174,0x00|26,  0,  2,0x80| 0),
ft(175,0x00|18,  3,  2,0x00| 0),
ft(176,0x00|22,  7,  2,0xC0| 4),
ft(177,0x00|21, 10,  2,0x00| 0),
ft(178,0x00|23,  7,  2,0x80| 0),
ft(179,0x00|25,  4,  2,0x80| 0),
ft(180,0x00|24,  0,  2,0xC0|14),
ft(181,0x00|15,  0,  2,0xC0|13),
ft(182,0x00|20,  3,  2,0x00| 0),
ft(183,0x00|31,  3,  2,0x00| 0), // 160
ft(184,0x00|16,  0,  2,0x80| 0),
ft(185,0x00|17,  0,  2,0x80| 0),
ft(186,0x00|32,  0,  2,0x00| 0),
ft(187,0x00|33,  0,  2,0x80| 0),
ft(188,0x00|34,  2,  2,0xC0| 3),
ft( 14,0x00| 0,  0,  0,0x40| 3),
ft(189,0x00|19,  7,  2,0x80| 0),
ft(190,0x00|14,  1,  2,0x80| 0),
ft(191,0x00|27,  0,  2,0x80| 0),
ft(181,0x00|15,  0,  2,0xC0|13), // 170
ft(181,0x00|15,  0,  2,0xC0|13),
ft(112,0x00|43,  0,  0,0xC0| 6), // 172
ft(113,0x00|44,  0,  0,0x40| 6),
ft(114,0x00|45,  0,  0,0xC0| 5),
ft(115,0x00|46,  0,  0,0x40| 5),
ft(114,0x00| 0,  0,  0,0xC0| 5),
ft( 78,0x00| 0,  0,  3,0x80|10),
ft( 77,0x00| 0,  4,  3,0x80| 7),
ft(211,0x00| 0,  0,  1,0x40| 4),
ft(212,0x00| 0,  0,  1,0x40| 4),
ft(213,0x00| 0,  0,  1,0x40| 4), // 181
ft(214,0x00| 0,  0,  1,0x40| 7),
ft(215,0x00| 0,  0,  7,0x40|11),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 79,0x00| 0,  4,  7,0x40| 9),
ft(130,0x00| 0,  0,  0,0x40| 4),
ft(131,0x00| 0,  0,  0,0x40| 4),
ft(132,0x00| 0,  0,  2,0x40| 4),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(192,0x00| 0,  0,  0,0x00| 0), // 191
ft(193,0x00| 0,  0,  1,0x00| 0),
ft(194,0x00| 0,  0,  0,0x80| 0),
ft(195,0x00| 0,  0,  0,0x00| 0),
ft(196,0x00| 0, -1,  0,0x00| 0),
ft(197,0x00| 0, -1,  0,0x00| 0),
ft(198,0x00| 0, -1,  0,0x00| 0),
ft(199,0x00| 0, -4,  0,0x00| 0),
ft(200,0x00| 0, -4,  0,0x80| 0),
ft(201,0x00| 0, -4,  0,0x00| 0),
ft(202,0x00| 0, -4,  0,0x00| 0), // 201
ft(203,0x00| 0, -4,  0,0x00| 0),
ft(204,0x00| 0, -4,  0,0x00| 0),
ft(205,0x00| 0, -5,  0,0x00| 0),
ft(206,0x00| 0, -5,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(207,0x00| 0,  0,  1,0x40| 6),
ft(208,0x00| 0,  0,  1,0xC0| 6),
ft(209,0x00| 0,  0,  1,0xC0| 8),
ft(210,0x00| 0,  0,  1,0x40|10),
ft(255,0x00| 0,  0,  0,0x00| 0), // 211
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 52,0x00| 0,  0,  0,0x80| 0),
ft( 53,0x00| 0,  0,  0,0x00| 0),
ft( 54,0x00| 0,  0,  0,0x00| 0),
ft( 55,0x00| 0,  0,  0,0x00| 0),
ft( 56,0x00| 0,  0,  0,0x80| 0), // 221
ft( 57,0x00| 0,  0,  0,0x00| 0),
ft( 58,0x00| 0,  0,  0,0x00| 0),
ft( 59,0x00| 0,  0,  0,0x00| 0),
ft( 60,0x00| 0,  0,  0,0x80| 0),
ft( 61,0x00| 0,  0,  0,0x00| 0),
ft( 62,0x00| 0,  0,  0,0x80| 0),
ft( 63,0x00| 0,  0,  0,0x00| 0),
ft(160,0x00|35,  1,  1,0xC0| 3),
ft(161,0x00|36,  0,  1,0x40| 9),
ft(162,0x00|37,  0,  1,0xC0| 3), // 231
ft(163,0x00|38,  0,  1,0x40| 9),
ft(164,0x00|39,  0,  1,0xC0| 3),
ft(165,0x00|40,  1,  1,0x40| 9),
ft(166,0x00|41,  1,  1,0x40| 3),
ft(167,0x00|42,  1,  1,0xC0| 9),
ft(168,0x00| 0,  4,  1,0xC0| 6),
ft(169,0x00| 0,  3,  1,0xC0|10),
ft(170,0x00| 0,  1,  1,0x40| 3),
ft(171,0x00| 0,  1,  1,0xC0| 8), // 240
];

#[rustfmt::skip]
static FRAME_TBL_GUARD: [frame_type; 41] = [
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 12,0xC0|13,  2,  1,0x00| 0),
ft(  2,0xC0| 1,  3,  1,0x00| 0),
ft(  3,0xC0| 2,  4,  1,0x00| 0),
ft(  4,0xC0| 3,  7,  1,0x40| 4),
ft(  5,0xC0| 4, 10,  1,0x00| 0),
ft(  6,0xC0| 5,  7,  1,0x80| 0),
ft(  7,0xC0| 6,  4,  1,0x80| 0),
ft(  8,0xC0| 7,  0,  1,0x80| 0),
ft(  9,0xC0| 8,  0,  1,0xC0|13),
ft( 10,0xC0|11,  7,  1,0x80| 0),
ft( 11,0xC0|12,  3,  1,0x00| 0),
ft( 12,0xC0|13,  2,  1,0x00| 0),
ft( 13,0xC0| 0,  2,  1,0x00| 0),
ft( 14,0xC0|28,  0,  1,0x00| 0),
ft( 15,0xC0|29,  0,  1,0x80| 0),
ft( 16,0xC0|30,  2,  1,0xC0| 3),
ft( 17,0xC0| 9, -1,  1,0x40| 8),
ft( 18,0xC0|10,  7,  1,0x80| 0),
ft( 19,0xC0|14,  3,  1,0x80| 0),
ft(  9,0xC0| 8,  0,  1,0x80| 0),
ft( 20,0xC0| 8,  0,  1,0xC0|13),
ft( 21,0xC0| 8,  0,  1,0xC0|13),
ft( 22,0xC0|47,  0,  0,0xC0| 6),
ft( 23,0xC0|48,  0,  0,0x40| 6),
ft( 24,0xC0|49,  0,  0,0xC0| 5),
ft( 24,0xC0|49,  0,  0,0xC0| 5),
ft( 24,0xC0|49,  0,  0,0xC0| 5),
ft( 26,0xC0| 0,  0,  3,0x80|10),
ft( 27,0xC0| 0,  4,  4,0x80| 7),
ft( 28,0xC0| 0, -2,  1,0x40| 4),
ft( 29,0xC0| 0, -2,  1,0x40| 4),
ft( 30,0xC0| 0, -2,  1,0x40| 4),
ft( 31,0xC0| 0, -2,  2,0x40| 7),
ft( 32,0xC0| 0, -2,  2,0x40|10),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 33,0xC0| 0,  3,  4,0xC0| 9),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
];

#[rustfmt::skip]
static FRAME_TBL_CUTS: [frame_type; 86] = [
ft(255,0x00| 0,  0,  0,0x00| 0),
ft( 15,0x40| 0,  0,  0,0x00| 0),
ft(  1,0x40| 0,  0,  0,0x80| 0),
ft(  2,0x40| 0,  0,  0,0x80| 0),
ft(  3,0x40| 0,  0,  0,0x80| 0),
ft(  4,0x40| 0, -1,  0,0x00| 0),
ft(  5,0x40| 0,  2,  0,0x80| 0),
ft(  6,0x40| 0,  2,  0,0x00| 0),
ft(  7,0x40| 0,  0,  0,0x80| 0),
ft(  8,0x40| 0,  1,  0,0x80| 0),
ft(255,0x00| 0,  0,  0,0x00| 0),
ft(  0,0x40| 0,  0,  0,0x80| 0),
ft(  9,0x40| 0,  0,  0,0x80| 0),
ft( 10,0x40| 0,  0,  0,0x00| 0),
ft( 11,0x40| 0,  0,  0,0x80| 0),
ft( 12,0x40| 0,  0,  0,0x80| 0),
ft( 13,0x40| 0,  0,  0,0x80| 0),
ft( 14,0x40| 0,  0,  0,0x00| 0),
ft( 16,0x40| 0,  0,  0,0x00| 0),
ft(  0,0x80| 0,  0,  0,0x00| 0),
ft(  2,0x80| 0,  0,  0,0x00| 0),
ft(  3,0x80| 0,  0,  0,0x00| 0),
ft(  4,0x80| 0,  0,  0,0x80| 0),
ft(  5,0x80| 0,  0,  0,0x00| 0),
ft(  6,0x80| 0,  0,  0,0x80| 0),
ft(  7,0x80| 0,  0,  0,0x80| 0),
ft(  8,0x80| 0,  0,  0,0x00| 0),
ft(  9,0x80| 0,  0,  0,0x00| 0),
ft( 10,0x80| 0,  0,  0,0x00| 0),
ft( 11,0x80| 0,  0,  0,0x00| 0),
ft( 12,0x80| 0,  0,  0,0x00| 0),
ft( 13,0x80| 0,  0,  0,0x00| 0),
ft( 14,0x80| 0,  0,  0,0x00| 0),
ft( 15,0x80| 0,  0,  0,0x00| 0),
ft( 16,0x80| 0,  0,  0,0x00| 0),
ft( 17,0x80| 0,  0,  0,0x00| 0),
ft( 18,0x80| 0,  0,  0,0x00| 0),
ft( 19,0x80| 0,  0,  0,0x00| 0),
ft( 20,0x80| 0,  0,  0,0x80| 0),
ft( 21,0x80| 0,  0,  0,0x80| 0),
ft( 22,0x80| 0,  1,  0,0x00| 0),
ft( 23,0x80| 0, -1,  0,0x00| 0),
ft( 24,0x80| 0,  2,  0,0x00| 0),
ft( 25,0x80| 0,  1,  0,0x80| 0),
ft( 26,0x80| 0,  0,  0,0x80| 0),
ft( 27,0x80| 0,  0,  0,0x80| 0),
ft( 28,0x80| 0,  0,  0,0x80| 0),
ft( 29,0x80| 0, -1,  0,0x00| 0),
ft(  0,0x80| 0,  0,  0,0x80| 0),
ft(  1,0x80| 0,  0,  0,0x80| 0),
ft(  2,0x80| 0,  0,  0,0x80| 0),
ft(  3,0x80| 0,  0,  0,0x00| 0),
ft(  4,0x80| 0,  0,  0,0x00| 0),
ft(  5,0x80| 0,  0,  0,0x80| 0),
ft(  6,0x80| 0,  0,  0,0x80| 0),
ft(  7,0x80| 0,  0,  0,0x80| 0),
ft(  8,0x80| 0,  0,  0,0x80| 0),
ft(  9,0x80| 0,  0,  0,0x80| 0),
ft( 10,0x80| 0,  0,  0,0x80| 0),
ft( 11,0x80| 0,  0,  0,0x80| 0),
ft( 12,0x80| 0,  0,  0,0x80| 0),
ft( 13,0x80| 0,  0,  0,0x00| 0),
ft( 14,0x80| 0,  0,  0,0x80| 0),
ft( 15,0x80| 0,  0,  0,0x00| 0),
ft( 16,0x80| 0,  0,  0,0x00| 0),
ft( 17,0x80| 0,  0,  0,0x80| 0),
ft( 18,0x80| 0,  0,  0,0x00| 0),
ft( 19,0x80| 0,  3,  0,0x00| 0),
ft( 20,0x80| 0,  3,  0,0x00| 0),
ft( 21,0x80| 0,  3,  0,0x00| 0),
ft( 22,0x80| 0,  2,  0,0x00| 0),
ft( 23,0x80| 0,  3,  0,0x80| 0),
ft( 24,0x80| 0,  5,  0,0x00| 0),
ft( 25,0x80| 0,  5,  0,0x00| 0),
ft( 26,0x80| 0,  1,  0,0x80| 0),
ft( 27,0x80| 0,  2,  0,0x80| 0),
ft( 28,0x80| 0,  2,  0,0x80| 0),
ft( 29,0x80| 0,  1,  0,0x80| 0),
ft( 30,0x80| 0,  1,  0,0x00| 0),
ft( 31,0x80| 0,  2,  0,0x00| 0),
ft( 32,0x80| 0,  3,  0,0x00| 0),
ft( 33,0x80| 0,  3,  0,0x00| 0),
ft( 34,0x80| 0,  0,  0,0x80| 0),
ft( 35,0x80| 0,  2,  0,0x80| 0),
ft( 36,0x80| 0,  2,  0,0x80| 0),
ft( 37,0x80| 0,  1,  0,0x00| 0),
];

#[rustfmt::skip]
static SWORD_TBL: [sword_table_type; 51] = [
st(255,   0,   0),
st(  0,   0,  -9),
st(  5,  -9, -29),
st(  1,   7, -25),
st(  2,  17, -26),
st(  6,   7, -14),
st(  7,   0,  -5),
st(  3,  17, -16),
st(  4,  16, -19),
st( 30,  12,  -9),
st(  8,  13, -34),
st(  9,   7, -25),
st( 10,  10, -16),
st( 11,  10, -11),
st( 12,  22, -21),
st( 13,  28, -23),
st( 14,  13, -35),
st( 15,   0, -38),
st( 16,   0, -29),
st( 17,  21, -19),
st( 18,  14, -23),
st( 19,  21, -22),
st( 19,  22, -23),
st( 17,   7, -13),
st( 17,  15, -18),
st(  7,   0,  -8),
st(  1,   7, -27),
st( 28,  14, -28),
st(  8,   7, -27),
st(  4,   6, -23),
st(  4,   9, -21),
st( 10,  11, -18),
st( 13,  24, -23),
st( 13,  19, -23),
st( 13,  21, -23),
st( 20,   7, -32),
st( 21,  14, -32),
st( 22,  14, -31),
st( 23,  14, -29),
st( 24,  28, -28),
st( 25,  28, -28),
st( 26,  21, -25),
st( 27,  14, -22),
st(255,  14, -25),
st(255,  21, -25),
st( 29,   0, -16),
st(  8,   8, -37),
st( 31,  14, -24),
st( 32,  14, -24),
st( 33,   7, -14),
st(  8,   8, -37),
];

// ── obj2 state ────────────────────────────────────────────────────────────────

static mut obj2_tilepos:    u8  = 0;
static mut obj2_x:          u16 = 0;
static mut obj2_y:          u8  = 0;
static mut obj2_direction:  i8  = 0;
static mut obj2_id:         u8  = 0;
static mut obj2_chtab:      u8  = 0;
static mut obj2_clip_top:   i16 = 0;
static mut obj2_clip_bottom:i16 = 0;
static mut obj2_clip_left:  i16 = 0;
static mut obj2_clip_right: i16 = 0;

// ── Functions (ported from seg006.c) ─────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn get_tile(room: c_int, col: c_int, row: c_int) -> c_int {
    curr_room = room as i16;
    tile_col  = col as i16;
    tile_row  = row as i16;
    curr_room = find_room_of_tile() as i16;
    if curr_room > 0 {
        get_room_address(curr_room as c_int);
        curr_tilepos = (tbl_line_at(tile_row as usize) as i32 + tile_col as i32) as u8;
        curr_tile2   = *curr_room_tiles.add(curr_tilepos as usize) & 0x1F;
    } else {
        curr_tile2 = (*custom).level_edge_hit_tile;
    }
    curr_tile2 as c_int
}

#[no_mangle]
pub unsafe extern "C" fn find_room_of_tile() -> c_int {
    loop {
        // FIX_CORNER_GRAB: check tile_row < 0 first
        if tile_row < 0 {
            tile_row += 3;
            if curr_room > 0 {
                curr_room = level.roomlinks[(curr_room - 1) as usize].up as i16;
            } else {
                curr_room = 0;
            }
            continue;
        }
        if tile_col < 0 {
            tile_col += 10;
            if curr_room > 0 {
                curr_room = level.roomlinks[(curr_room - 1) as usize].left as i16;
            } else {
                curr_room = 0;
            }
            continue;
        }
        if tile_col >= 10 {
            tile_col -= 10;
            if curr_room > 0 {
                curr_room = level.roomlinks[(curr_room - 1) as usize].right as i16;
            } else {
                curr_room = 0;
            }
            continue;
        }
        if tile_row >= 3 {
            tile_row -= 3;
            if curr_room > 0 {
                curr_room = level.roomlinks[(curr_room - 1) as usize].down as i16;
            } else {
                curr_room = 0;
            }
            continue;
        }
        return curr_room as c_int;
    }
}

#[no_mangle]
pub unsafe extern "C" fn get_tilepos(tcol: c_int, trow: c_int) -> c_int {
    if trow < 0 {
        -(tcol + 1)
    } else if trow >= 3 || tcol >= 10 || tcol < 0 {
        30
    } else {
        tbl_line_at(trow as usize) as c_int + tcol
    }
}

#[no_mangle]
pub unsafe extern "C" fn get_tilepos_nominus(tcol: c_int, trow: c_int) -> c_int {
    let tp = get_tilepos(tcol, trow);
    if tp < 0 { 30 } else { tp }
}

#[no_mangle]
pub unsafe extern "C" fn load_fram_det_col() {
    load_frame();
    determine_col();
}

#[no_mangle]
pub unsafe extern "C" fn determine_col() {
    Char.curr_col = get_tile_div_mod_m7(dx_weight()) as i8;
}

unsafe fn get_frame_internal(frame_table: &[frame_type], frame: c_int) {
    if frame >= 0 && frame < frame_table.len() as c_int {
        cur_frame = frame_table[frame as usize];
    } else {
        cur_frame = frame_type { image: 255, sword: 0, dx: 0, dy: 0, flags: 0 };
    }
}

#[no_mangle]
pub unsafe extern "C" fn load_frame() {
    let frame = Char.frame as c_int;
    let mut add_frame: c_int = 0;
    match Char.charid {
        c if c == charids_charid_0_kid as u8 || c == charids_charid_24_mouse as u8 => {
            get_frame_internal(&FRAME_TABLE_KID, frame);
        }
        c if c == charids_charid_2_guard as u8 || c == charids_charid_4_skeleton as u8 => {
            if frame >= 102 && frame < 107 { add_frame = 70; }
            get_frame_internal(&FRAME_TBL_GUARD, frame + add_frame - 149);
        }
        c if c == charids_charid_1_shadow as u8 => {
            if frame < 150 || frame >= 190 {
                get_frame_internal(&FRAME_TABLE_KID, frame);
            } else {
                get_frame_internal(&FRAME_TBL_GUARD, frame + add_frame - 149);
            }
        }
        c if c == charids_charid_5_princess as u8 || c == charids_charid_6_vizier as u8 => {
            get_frame_internal(&FRAME_TBL_CUTS, frame);
        }
        _ => {}
    }
}

#[no_mangle]
pub unsafe extern "C" fn dx_weight() -> c_int {
    let offset = cur_frame.dx as i32 - (cur_frame.flags & frame_flags_FRAME_WEIGHT_X as u8) as i32;
    char_dx_forward(offset)
}

#[no_mangle]
pub unsafe extern "C" fn char_dx_forward(mut delta_x: c_int) -> c_int {
    if (Char.direction as i32) < directions_dir_0_right as i32 {
        delta_x = -delta_x;
    }
    delta_x + Char.x as i32
}

#[no_mangle]
pub unsafe extern "C" fn obj_dx_forward(mut delta_x: c_int) -> c_int {
    if (obj_direction as i32) < directions_dir_0_right as i32 {
        delta_x = -delta_x;
    }
    obj_x = (obj_x as i32 + delta_x) as i16;
    obj_x as c_int
}

#[no_mangle]
pub unsafe extern "C" fn play_seq() {
    loop {
        let seq_idx = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
        let command = seqtbl_byte(seq_idx);
        Char.curr_seq = Char.curr_seq.wrapping_add(1);
        match command {
            SEQ_DX => {
                let idx = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
                let val = seqtbl_byte(idx) as i32;
                Char.x = char_dx_forward(val) as u8;
                Char.curr_seq = Char.curr_seq.wrapping_add(1);
            }
            SEQ_DY => {
                let idx = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
                Char.y = Char.y.wrapping_add(seqtbl_byte(idx));
                Char.curr_seq = Char.curr_seq.wrapping_add(1);
            }
            SEQ_FLIP => {
                Char.direction = !Char.direction;
            }
            SEQ_JMP_IF_FEATHER => {
                if is_feather_fall == 0 {
                    Char.curr_seq = Char.curr_seq.wrapping_add(2);
                    // do NOT fall through: break and continue the outer loop
                } else {
                    // feather fall active: do the jump (same logic as SEQ_JMP)
                    let idx = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
                    let lo = seqtbl_byte(idx);
                    let hi = seqtbl_byte(idx + 1);
                    Char.curr_seq = u16::from_le_bytes([lo, hi]);
                }
            }
            SEQ_JMP => {
                let idx = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
                let lo = seqtbl_byte(idx);
                let hi = seqtbl_byte(idx + 1);
                Char.curr_seq = u16::from_le_bytes([lo, hi]);
            }
            SEQ_UP => {
                Char.curr_row -= 1;
                start_chompers();
            }
            SEQ_DOWN => {
                inc_curr_row();
                start_chompers();
            }
            SEQ_ACTION => {
                let idx = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
                Char.action = seqtbl_byte(idx);
                Char.curr_seq = Char.curr_seq.wrapping_add(1);
            }
            SEQ_SET_FALL => {
                let idx = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
                Char.fall_x = seqtbl_byte(idx) as i8;
                Char.curr_seq = Char.curr_seq.wrapping_add(1);
                let idx2 = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
                Char.fall_y = seqtbl_byte(idx2) as i8;
                Char.curr_seq = Char.curr_seq.wrapping_add(1);
            }
            SEQ_KNOCK_UP => {
                knock = 1;
            }
            SEQ_KNOCK_DOWN => {
                knock = -1;
            }
            SEQ_SOUND => {
                let idx = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
                let which_sound = seqtbl_byte(idx);
                Char.curr_seq = Char.curr_seq.wrapping_add(1);
                match which_sound {
                    SND_SILENT => {
                        is_guard_notice = 1;
                    }
                    SND_FOOTSTEP => {
                        play_sound(soundids_sound_23_footstep as c_int);
                        is_guard_notice = 1;
                    }
                    SND_BUMP => {
                        play_sound(soundids_sound_8_bumped as c_int);
                        is_guard_notice = 1;
                    }
                    SND_DRINK => {
                        play_sound(soundids_sound_18_drink as c_int);
                    }
                    SND_LEVEL => {
                        // USE_REPLAY: don't do end level music in replays
                        if recording != 0 || replaying != 0 { /* skip */ }
                        else if is_sound_on != 0 {
                            if current_level == (*custom).mirror_level as u16 {
                                play_sound(soundids_sound_32_shadow_music as c_int);
                            } else if current_level != 13 && current_level != 15 {
                                play_sound(soundids_sound_41_end_level_music as c_int);
                            }
                        }
                    }
                    _ => {}
                }
            }
            SEQ_END_LEVEL => {
                next_level += 1;
                // USE_REPLAY
                keep_last_seed = 1;
                if replaying != 0 && skipping_replay != 0 { stop_sounds(); }
            }
            SEQ_GET_ITEM => {
                let idx = Char.curr_seq.wrapping_sub(SEQTBL_BASE) as usize;
                let which_item = seqtbl_byte(idx) as c_int;
                Char.curr_seq = Char.curr_seq.wrapping_add(1);
                if which_item == 1 {
                    proc_get_object();
                }
                // USE_TELEPORTS
                if which_item == 2 {
                    teleport();
                }
            }
            SEQ_DIE => { /* nop */ }
            _ => {
                Char.frame = command;
                return;
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn get_tile_div_mod_m7(xpos: c_int) -> c_int {
    get_tile_div_mod(xpos - 7)
}

#[no_mangle]
pub unsafe extern "C" fn get_tile_div_mod(xpos: c_int) -> c_int {
    let x = xpos - SCREENSPACE_X;
    let mut xl = x % TILE_SIZEX;
    let mut xh = x / TILE_SIZEX;
    if xl < 0 {
        xh -= 1;
        xl += TILE_SIZEX;
    }
    if xpos < 0 {
        let bogus_len = BOGUS_BEFORE.len() as i32;
        if bogus_len + xpos >= 0 {
            xh = BOGUS_BEFORE[(bogus_len + xpos) as usize] as i32;
            xl = TILE_DIV_TBL[(256 + xpos) as usize] as i32;
        }
    }
    let tbl_size: i32 = 256;
    if xpos >= tbl_size {
        let off = (xpos - tbl_size) as usize;
        if off < BOGUS_AFTER.len() {
            xh = TILE_MOD_TBL[(xpos - tbl_size) as usize] as i32;
            xl = BOGUS_AFTER[off] as i32;
        }
    }
    obj_xl = xl as u8;
    xh
}

#[no_mangle]
pub unsafe extern "C" fn y_to_row_mod4(ypos: c_int) -> c_int {
    (ypos + 60) / TILE_SIZEY % 4 - 1
}

#[no_mangle]
pub unsafe extern "C" fn loadkid() {
    Char = Kid;
}

#[no_mangle]
pub unsafe extern "C" fn savekid() {
    Kid = Char;
}

#[no_mangle]
pub unsafe extern "C" fn loadshad() {
    Char = Guard;
}

#[no_mangle]
pub unsafe extern "C" fn saveshad() {
    Guard = Char;
}

#[no_mangle]
pub unsafe extern "C" fn loadkid_and_opp() {
    loadkid();
    Opp = Guard;
}

#[no_mangle]
pub unsafe extern "C" fn savekid_and_opp() {
    savekid();
    Guard = Opp;
}

#[no_mangle]
pub unsafe extern "C" fn loadshad_and_opp() {
    loadshad();
    Opp = Kid;
}

#[no_mangle]
pub unsafe extern "C" fn saveshad_and_opp() {
    saveshad();
    Kid = Opp;
}

#[no_mangle]
pub unsafe extern "C" fn reset_obj_clip() {
    obj_clip_left   = 0;
    obj_clip_top    = 0;
    obj_clip_right  = 320;
    obj_clip_bottom = 192;
}

#[no_mangle]
pub unsafe extern "C" fn x_to_xh_and_xl(xpos: c_int, xh_addr: *mut i8, xl_addr: *mut i8) {
    // FIX_SPRITE_XPOS active
    *xh_addr = (xpos >> 3) as i8;
    *xl_addr = (xpos & 7) as i8;
}

#[no_mangle]
pub unsafe extern "C" fn fall_accel() {
    if Char.action == actions_actions_4_in_freefall as u8 {
        if is_feather_fall != 0
            // FIX_FEATHER_FALL_AFFECTS_GUARDS: only kid affected
            && ((*fixes).fix_feather_fall_affects_guards == 0 || Char.charid == charids_charid_0_kid as u8)
        {
            Char.fall_y += FALLING_SPEED_ACCEL_FEATHER;
            if Char.fall_y > FALLING_SPEED_MAX_FEATHER {
                Char.fall_y = FALLING_SPEED_MAX_FEATHER;
            }
        } else {
            Char.fall_y += FALLING_SPEED_ACCEL;
            if Char.fall_y > FALLING_SPEED_MAX {
                Char.fall_y = FALLING_SPEED_MAX;
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn fall_speed() {
    Char.y = Char.y.wrapping_add(Char.fall_y as u8);
    // USE_SUPER_HIGH_JUMP
    if Char.action == actions_actions_4_in_freefall as u8
        && ((*fixes).enable_super_high_jump == 0 || super_jump_fall == 0)
    {
        Char.x = char_dx_forward(Char.fall_x as i32) as u8;
        load_fram_det_col();
    }
}

#[no_mangle]
pub unsafe extern "C" fn check_action() {
    let action = Char.action;
    let frame  = Char.frame;
    // USE_JUMP_GRAB
    if (*fixes).enable_jump_grab != 0
        && action == actions_actions_1_run_jump as u8
        && control_shift == CONTROL_HELD as i8
        && check_grab_run_jump() != 0
    {
        return;
    }
    if action == actions_actions_6_hang_straight as u8 || action == actions_actions_5_bumped as u8 {
        if frame == frameids_frame_109_crouch as u8
            || ((*fixes).fix_stand_on_thin_air != 0
                && frame >= frameids_frame_110_stand_up_from_crouch_1 as u8
                && frame <= frameids_frame_119_stand_up_from_crouch_10 as u8)
            || ((*fixes).fix_dead_floating_in_air != 0
                && frame >= frameids_frame_177_spiked as u8
                && frame <= frameids_frame_185_dead as u8)
        {
            check_on_floor();
        }
    } else if action == actions_actions_4_in_freefall as u8 {
        do_fall();
    } else if action == actions_actions_3_in_midair as u8 {
        if frame >= frameids_frame_102_start_fall_1 as u8 && frame < frameids_frame_106_fall as u8 {
            check_grab();
        }
    } else if action != actions_actions_2_hang_climb as u8 {
        check_on_floor();
    }
}

#[no_mangle]
pub unsafe extern "C" fn tile_is_floor(tiletype: c_int) -> c_int {
    match tiletype as u32 {
        x if x == tiles_tiles_0_empty as u32
          || x == tiles_tiles_9_bigpillar_top as u32
          || x == tiles_tiles_12_doortop as u32
          || x == tiles_tiles_20_wall as u32
          || x == tiles_tiles_26_lattice_down as u32
          || x == tiles_tiles_27_lattice_small as u32
          || x == tiles_tiles_28_lattice_left as u32
          || x == tiles_tiles_29_lattice_right as u32 => 0,
        _ => 1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn check_spiked() {
    let frame = Char.frame;
    if get_tile(Char.room as c_int, Char.curr_col as c_int, Char.curr_row as c_int) == tiles_tiles_2_spike as c_int {
        let harmful = is_spike_harmful();
        if (harmful >= 2
                && ((frame >= frameids_frame_7_run as u8 && frame < 15)
                    || (frame >= frameids_frame_34_start_run_jump_1 as u8 && frame < 40)))
            || ((frame == frameids_frame_43_running_jump_4 as u8
                    || frame == frameids_frame_26_standing_jump_11 as u8)
                && harmful != 0)
        {
            spiked();
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn take_hp(count: c_int) -> c_int {
    let mut dead: u16 = 0;
    if Char.charid == charids_charid_0_kid as u8 {
        if count >= hitp_curr as i32 {
            hitp_delta = -(hitp_curr as i32) as i16;
            dead = 1;
        } else {
            hitp_delta = -(count as i16);
        }
    } else {
        if count >= guardhp_curr as i32 {
            guardhp_delta = -(guardhp_curr as i32) as i16;
            dead = 1;
        } else {
            guardhp_delta = -(count as i16);
        }
    }
    dead as c_int
}

#[no_mangle]
pub unsafe extern "C" fn get_tile_at_char() -> c_int {
    get_tile(Char.room as c_int, Char.curr_col as c_int, Char.curr_row as c_int)
}

#[no_mangle]
pub unsafe extern "C" fn set_char_collision() {
    let image = get_image(obj_chtab as c_short, obj_id as c_int);
    if image.is_null() {
        char_width_half = 0;
        char_height     = 0;
    } else {
        char_width_half = (((*image).w as i32 + 1) / 2) as u16;
        char_height     = (*image).h as u16;
    }
    char_x_left = (obj_x as i32 / 2 + 58) as i16;
    if Char.direction >= directions_dir_0_right as i8 {
        char_x_left -= char_width_half as i16;
    }
    char_x_left_coll = char_x_left;
    char_x_right     = (char_x_left as i32 + char_width_half as i32) as i16;
    char_x_right_coll = char_x_right;
    char_top_y = (obj_y as i32 - char_height as i32 + 1) as i16;
    if char_top_y >= 192 {
        char_top_y = 0;
    }
    char_top_row    = y_to_row_mod4(char_top_y as c_int) as i16;
    char_bottom_row = y_to_row_mod4(obj_y as c_int) as i16;
    if char_bottom_row == -1 {
        char_bottom_row = 3;
    }
    char_col_left  = get_tile_div_mod(char_x_left as c_int).max(0) as i16;
    char_col_right = get_tile_div_mod(char_x_right as c_int).min(9) as i16;
    if cur_frame.flags & frame_flags_FRAME_THIN as u8 != 0 {
        char_x_left_coll  += 4;
        char_x_right_coll -= 4;
    }
}

#[no_mangle]
pub unsafe extern "C" fn check_on_floor() {
    if cur_frame.flags & frame_flags_FRAME_NEEDS_FLOOR as u8 != 0 {
        // FIX_FALLING_THROUGH_FLOOR_DURING_SWORD_STRIKE
        if (*fixes).fix_falling_through_floor_during_sword_strike != 0
            && Char.frame == frameids_frame_153_strike_3 as u8
        {
            return;
        }
        if get_tile_at_char() == tiles_tiles_20_wall as c_int {
            in_wall();
        }
        if tile_is_floor(curr_tile2 as c_int) == 0 {
            // Special event: floors appear (level 12)
            if current_level == 12
                && (united_with_shadow < 0
                    || ((*fixes).fix_hidden_floors_during_flashing != 0 && united_with_shadow > 0))
                && Char.curr_row == 0
                && (Char.room == 2 || (Char.room == 13 && tile_col >= 6))
            {
                *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
                set_wipe(curr_tilepos as c_short, 1);
                set_redraw_full(curr_tilepos as c_short, 1);
                curr_tilepos += 1;
                set_wipe(curr_tilepos as c_short, 1);
                set_redraw_full(curr_tilepos as c_short, 1);
            } else {
                // FIX_STAND_ON_THIN_AIR
                if (*fixes).fix_stand_on_thin_air != 0
                    && Char.frame >= frameids_frame_110_stand_up_from_crouch_1 as u8
                    && Char.frame <= frameids_frame_119_stand_up_from_crouch_10 as u8
                {
                    let col = get_tile_div_mod_m7(dx_weight() + back_delta_x(2));
                    if tile_is_floor(get_tile(Char.room as c_int, col, Char.curr_row as c_int)) != 0 {
                        return;
                    }
                }
                start_fall();
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn start_fall() {
    let frame = Char.frame;
    Char.sword = sword_status_sword_0_sheathed as u8;
    inc_curr_row();
    start_chompers();
    fall_frame = frame;
    let seq_id: u16;
    if frame == frameids_frame_9_run as u8 {
        seq_id = seqids_seq_7_fall as u16;
    } else if frame == frameids_frame_13_run as u8 {
        seq_id = seqids_seq_19_fall as u16;
    } else if frame == frameids_frame_26_standing_jump_11 as u8 {
        seq_id = seqids_seq_18_fall_after_standing_jump as u16;
    } else if frame == frameids_frame_44_running_jump_5 as u8 {
        seq_id = seqids_seq_21_fall_after_running_jump as u16;
    } else if frame >= frameids_frame_81_hangdrop_1 as u8 && frame < 86 {
        seq_id = seqids_seq_19_fall as u16;
        Char.x = char_dx_forward(5) as u8;
        load_fram_det_col();
    } else if frame >= 150 && frame < 180 {
        if Char.charid == charids_charid_2_guard as u8 {
            if Char.curr_row == 3 && Char.curr_col == 10 {
                clear_char();
                return;
            }
            if (Char.fall_x as i32) < 0 {
                seq_id = seqids_seq_82_guard_pushed_off_ledge as u16;
                if Char.direction < directions_dir_0_right as i8 && distance_to_edge_weight() <= 7 {
                    Char.x = char_dx_forward(-5) as u8;
                }
            } else {
                droppedout = 0;
                seq_id = seqids_seq_83_guard_fall as u16;
            }
        } else {
            droppedout = 1;
            if Char.direction < directions_dir_0_right as i8 && distance_to_edge_weight() <= 7 {
                Char.x = char_dx_forward(-5) as u8;
            }
            seq_id = seqids_seq_81_kid_pushed_off_ledge as u16;
        }
    } else {
        seq_id = seqids_seq_7_fall as u16;
    }
    seqtbl_offset_char(seq_id as c_short);
    play_seq();
    load_fram_det_col();
    if get_tile_at_char() == tiles_tiles_20_wall as c_int {
        in_wall();
        return;
    }
    let tile = get_tile_infrontof_char();
    if tile == tiles_tiles_20_wall as c_int
        || ((*fixes).fix_running_jump_through_tapestry != 0
            && Char.direction == directions_dir_FF_left as i8
            && (tile == tiles_tiles_12_doortop as c_int
                || tile == tiles_tiles_7_doortop_with_floor as c_int))
    {
        if fall_frame != frameids_frame_44_running_jump_5 as u8
            || distance_to_edge_weight() >= 6
        {
            Char.x = char_dx_forward(-1) as u8;
        } else {
            seqtbl_offset_char(seqids_seq_104_start_fall_in_front_of_wall as c_short);
            play_seq();
        }
        load_fram_det_col();
    }
}

#[no_mangle]
pub unsafe extern "C" fn check_grab() {
    // FIX_GRAB_FALLING_SPEED: max = 30 if fix enabled, else 32
    let max_grab_falling_speed: i8 = if (*fixes).fix_grab_falling_speed != 0 { 30 } else { 32 };
    if (control_shift == CONTROL_HELD as i8
            // USE_SUPER_HIGH_JUMP: also allow grabbing with up arrow during super jump
            || ((*fixes).enable_super_high_jump != 0
                && super_jump_fall != 0
                && control_y == CONTROL_HELD_UP as i8))
        && Char.fall_y < max_grab_falling_speed
        && Char.alive < 0
        && (y_land_at((Char.curr_row + 1) as usize) as u16) <= (Char.y as i32 + 25) as u16
    {
        let old_x = Char.x;
        let super_delta: i32 = if (*fixes).enable_super_high_jump != 0 && super_jump_fall != 0 {
            if Char.direction == directions_dir_FF_left as i8 { 3 } else { 4 }
        } else { 0 };
        Char.x = char_dx_forward(-8 + super_delta) as u8;
        load_fram_det_col();
        if can_grab_front_above() == 0 {
            Char.x = old_x;
        } else {
            Char.x = char_dx_forward(distance_to_edge_weight() - super_delta) as u8;
            Char.y = y_land_at((Char.curr_row + 1) as usize) as u8;
            Char.fall_y = 0;
            seqtbl_offset_char(seqids_seq_15_grab_ledge_midair as c_short);
            play_seq();
            grab_timer = 12;
            play_sound(soundids_sound_9_grab as c_int);
            is_screaming = 0;
            // FIX_CHOMPERS_NOT_STARTING
            if (*fixes).fix_chompers_not_starting != 0 { start_chompers(); }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn check_grab_run_jump() -> c_int {
    let frame = Char.frame as u32;
    let is_jump = frame >= frameids_frame_22_standing_jump_7 as u32
               && frame <= frameids_frame_23_standing_jump_8 as u32;
    let is_running_jump = frame >= frameids_frame_39_start_run_jump_6 as u32
                       && frame <= frameids_frame_41_running_jump_2 as u32;
    let char_room_m1 = Char.room - 1;
    if Char.action == actions_actions_1_run_jump as u8
        && (is_jump || is_running_jump)
        && control_x == CONTROL_RELEASED as i8
        && control_y == CONTROL_HELD_UP as i8
    {
        if can_grab_front_above() != 0 {
            let grab_tile = curr_tile2;
            let mut grab_col = tile_col;
            if curr_room != Char.room as i16 {
                let left_room  = level.roomlinks[char_room_m1 as usize].left;
                let right_room = level.roomlinks[char_room_m1 as usize].right;
                let up_room    = level.roomlinks[char_room_m1 as usize].up;
                if curr_room == right_room as i16 {
                    grab_col += 10;
                } else if curr_room == left_room as i16 {
                    grab_col -= 10;
                } else if right_room != 0 && curr_room == level.roomlinks[(right_room - 1) as usize].up as i16 {
                    grab_col += 10;
                } else if left_room != 0 && curr_room == level.roomlinks[(left_room - 1) as usize].up as i16 {
                    grab_col -= 10;
                } else if up_room != 0 && curr_room == level.roomlinks[(up_room - 1) as usize].right as i16 {
                    grab_col += 10;
                } else if up_room != 0 && curr_room == level.roomlinks[(up_room - 1) as usize].left as i16 {
                    grab_col -= 10;
                }
            }
            Char.x = (x_bump_at((grab_col + FIRST_ONSCREEN_COLUMN as i16) as usize) as i32 + TILE_MIDX) as u8;
            let dir_delta: i32 = if Char.direction == directions_dir_FF_left as i8 { -12 } else { 2 };
            Char.x = char_dx_forward(dir_delta) as u8;
            Char.y = y_land_at((Char.curr_row + 1) as usize) as u8;
            seqtbl_offset_char(seqids_seq_9_grab_while_jumping as c_short);
            play_seq();
            grab_timer = 12;
            play_sound(soundids_sound_9_grab as c_int);
            if grab_tile == tiles_tiles_15_opener as u8 || grab_tile == tiles_tiles_6_closer as u8 {
                trigger_button(1, 0, -1);
            } else if grab_tile == tiles_tiles_11_loose as u8 {
                is_guard_notice = 1;
                make_loose_fall(1);
            }
            return 1;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn can_grab_front_above() -> c_int {
    through_tile = get_tile_above_char() as u8;
    get_tile_front_above_char();
    can_grab()
}

#[no_mangle]
pub unsafe extern "C" fn in_wall() {
    let mut delta_x = distance_to_edge_weight();
    if delta_x >= 8 || get_tile_infrontof_char() == tiles_tiles_20_wall as c_int {
        delta_x = 6 - delta_x;
    } else {
        delta_x += 4;
    }
    Char.x = char_dx_forward(delta_x) as u8;
    load_fram_det_col();
    get_tile_at_char();
}

#[no_mangle]
pub unsafe extern "C" fn get_tile_infrontof_char() -> c_int {
    infrontx = (dir_front_at((Char.direction as i8 + 1) as usize) as i32 + Char.curr_col as i32) as i8;
    get_tile(Char.room as c_int, infrontx as c_int, Char.curr_row as c_int)
}

#[no_mangle]
pub unsafe extern "C" fn get_tile_infrontof2_char() -> c_int {
    let direction = dir_front_at((Char.direction as i8 + 1) as usize);
    infrontx = ((direction as i32 * 2) + Char.curr_col as i32) as i8;
    get_tile(Char.room as c_int, infrontx as c_int, Char.curr_row as c_int)
}

#[no_mangle]
pub unsafe extern "C" fn get_tile_behind_char() -> c_int {
    get_tile(
        Char.room as c_int,
        (dir_behind_at((Char.direction as i8 + 1) as usize) as i32 + Char.curr_col as i32) as c_int,
        Char.curr_row as c_int,
    )
}

#[no_mangle]
pub unsafe extern "C" fn distance_to_edge_weight() -> c_int {
    distance_to_edge(dx_weight())
}

#[no_mangle]
pub unsafe extern "C" fn distance_to_edge(xpos: c_int) -> c_int {
    get_tile_div_mod_m7(xpos);
    let mut distance = obj_xl as c_int;
    if Char.direction == directions_dir_0_right as i8 {
        distance = TILE_RIGHTX - distance;
    }
    distance
}

#[no_mangle]
pub unsafe extern "C" fn fell_out() {
    if Char.alive < 0 && Char.room == 0 {
        take_hp(100);
        Char.alive = 0;
        erase_bottom_text(1);
        Char.frame = frameids_frame_185_dead as u8;
    }
}

#[no_mangle]
pub unsafe extern "C" fn play_kid() {
    fell_out();
    control_kid();
    if Char.alive >= 0 && is_dead() != 0 {
        if resurrect_time != 0 {
            stop_sounds();
            loadkid();
            hitp_delta = hitp_max as i16;
            seqtbl_offset_char(seqids_seq_2_stand as c_short);
            Char.x = Char.x.wrapping_add(8);
            play_seq();
            load_fram_det_col();
            set_start_pos();
        }
        if check_sound_playing() != 0 && current_sound != 5 {
            return;
        }
        is_show_time = 0;
        if Char.alive < 0 || Char.alive >= 6 {
            if Char.alive == 6 {
                if is_sound_on != 0
                    && current_level != 0
                    && current_level != 15
                {
                    play_death_music();
                }
            } else {
                if Char.alive != 7 || check_sound_playing() != 0 { return; }
                if rem_min == 0 {
                    expired();
                }
                if current_level != 0 && current_level != 15 {
                    text_time_remaining = 288;
                    text_time_total = 288;
                    display_text_bottom(b"Press Button to Continue\0".as_ptr() as *const _);
                } else {
                    text_time_remaining = 36;
                    text_time_total = 36;
                }
            }
        }
        Char.alive += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn control_kid() {
    if Char.alive < 0 && hitp_curr == 0 {
        Char.alive = 0;
        if (*fixes).fix_quicksave_during_feather != 0 && is_feather_fall > 0 {
            is_feather_fall = 0;
            if check_sound_playing() != 0 {
                stop_sounds();
            }
        }
    }
    if grab_timer != 0 {
        grab_timer -= 1;
    }
    // USE_REPLAY: demo level check
    if current_level == 0 && play_demo_level == 0 && replaying == 0 {
        do_demo();
        control();
        let key = key_test_quit();
        if key == (15i32 | key_modifiers_WITH_CTRL as i32) {
            if load_game() != 0 {
                start_game();
            }
        } else if key != 0 {
            start_level = (*custom).first_level as i16;
            start_game();
        }
    } else {
        rest_ctrl_1();
        do_paused();
        if recording != 0 { add_replay_move(); }
        if replaying != 0 { do_replay_move(); }
        read_user_control();
        user_control();
        save_ctrl_1();
    }
}

#[no_mangle]
pub unsafe extern "C" fn do_demo() {
    if checkpoint != 0 {
        control_shift2 = release_arrows() as i8;
        control_forward = CONTROL_HELD as i8;
        control_x = CONTROL_HELD_FORWARD as i8;
    } else if Char.sword != 0 {
        guard_skill = 10;
        autocontrol_opponent();
        guard_skill = 11;
    } else {
        do_auto_moves(core::ptr::addr_of!((*custom).demo_moves) as *const auto_move_type);
    }
}

#[no_mangle]
pub unsafe extern "C" fn play_guard() {
    if Char.charid == charids_charid_24_mouse as u8 {
        autocontrol_opponent();
    } else {
        let mut skip_shadow_check = false;
        if Char.alive < 0 {
            if guardhp_curr == 0 {
                Char.alive = 0;
                on_guard_killed();
            } else {
                skip_shadow_check = true;
            }
        }
        if !skip_shadow_check {
            if Char.charid == charids_charid_1_shadow as u8 {
                clear_char();
            }
        }
        autocontrol_opponent();
        control();
    }
}

#[no_mangle]
pub unsafe extern "C" fn user_control() {
    if Char.direction >= directions_dir_0_right as i8 {
        flip_control_x();
        control();
        flip_control_x();
    } else {
        control();
    }
}

#[no_mangle]
pub unsafe extern "C" fn flip_control_x() {
    control_x = -control_x;
    let temp = control_forward;
    control_forward = control_backward;
    control_backward = temp;
}

#[no_mangle]
pub unsafe extern "C" fn release_arrows() -> c_int {
    control_backward = CONTROL_RELEASED as i8;
    control_forward  = CONTROL_RELEASED as i8;
    control_up       = CONTROL_RELEASED as i8;
    control_down     = CONTROL_RELEASED as i8;
    1
}

#[no_mangle]
pub unsafe extern "C" fn save_ctrl_1() {
    ctrl1_forward  = control_forward;
    ctrl1_backward = control_backward;
    ctrl1_up       = control_up;
    ctrl1_down     = control_down;
    ctrl1_shift2   = control_shift2;
}

#[no_mangle]
pub unsafe extern "C" fn rest_ctrl_1() {
    control_forward  = ctrl1_forward;
    control_backward = ctrl1_backward;
    control_up       = ctrl1_up;
    control_down     = ctrl1_down;
    control_shift2   = ctrl1_shift2;
}

#[no_mangle]
pub unsafe extern "C" fn clear_saved_ctrl() {
    ctrl1_forward  = CONTROL_RELEASED as i8;
    ctrl1_backward = CONTROL_RELEASED as i8;
    ctrl1_up       = CONTROL_RELEASED as i8;
    ctrl1_down     = CONTROL_RELEASED as i8;
    ctrl1_shift2   = CONTROL_RELEASED as i8;
}

#[no_mangle]
pub unsafe extern "C" fn read_user_control() {
    if control_forward >= CONTROL_RELEASED as i8 {
        if control_x == CONTROL_HELD_FORWARD as i8 {
            if control_forward == CONTROL_RELEASED as i8 {
                control_forward = CONTROL_HELD as i8;
            }
        } else {
            control_forward = CONTROL_RELEASED as i8;
        }
    }
    if control_backward >= CONTROL_RELEASED as i8 {
        if control_x == CONTROL_HELD_BACKWARD as i8 {
            if control_backward == CONTROL_RELEASED as i8 {
                control_backward = CONTROL_HELD as i8;
            }
        } else {
            control_backward = CONTROL_RELEASED as i8;
        }
    }
    if control_up >= CONTROL_RELEASED as i8 {
        if control_y == CONTROL_HELD_UP as i8 {
            if control_up == CONTROL_RELEASED as i8 {
                control_up = CONTROL_HELD as i8;
            }
        } else {
            control_up = CONTROL_RELEASED as i8;
        }
    }
    if control_down >= CONTROL_RELEASED as i8 {
        if control_y == CONTROL_HELD_DOWN as i8 {
            if control_down == CONTROL_RELEASED as i8 {
                control_down = CONTROL_HELD as i8;
            }
        } else {
            control_down = CONTROL_RELEASED as i8;
        }
    }
    if control_shift2 >= CONTROL_RELEASED as i8 {
        if control_shift == CONTROL_HELD as i8 {
            if control_shift2 == CONTROL_RELEASED as i8 {
                control_shift2 = CONTROL_HELD as i8;
            }
        } else {
            control_shift2 = CONTROL_RELEASED as i8;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn can_grab() -> c_int {
    let modifier = *curr_room_modif.add(curr_tilepos as usize);
    if through_tile == tiles_tiles_20_wall as u8 { return 0; }
    if through_tile == tiles_tiles_12_doortop as u8
        && Char.direction >= directions_dir_0_right as i8
    {
        return 0;
    }
    if tile_is_floor(through_tile as c_int) != 0 { return 0; }
    if curr_tile2 == tiles_tiles_11_loose as u8
        && modifier != 0
        && !((*custom).loose_floor_delay > 11)
    {
        return 0;
    }
    if curr_tile2 == tiles_tiles_7_doortop_with_floor as u8
        && Char.direction < directions_dir_0_right as i8
    {
        return 0;
    }
    if tile_is_floor(curr_tile2 as c_int) == 0 { return 0; }
    1
}

#[no_mangle]
pub unsafe extern "C" fn wall_type(tiletype: u8) -> c_int {
    match tiletype as u32 {
        x if x == tiles_tiles_4_gate as u32
          || x == tiles_tiles_7_doortop_with_floor as u32
          || x == tiles_tiles_12_doortop as u32 => 1,
        x if x == tiles_tiles_13_mirror as u32 => 2,
        x if x == tiles_tiles_18_chomper as u32 => 3,
        x if x == tiles_tiles_20_wall as u32 => 4,
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn get_tile_above_char() -> c_int {
    get_tile(Char.room as c_int, Char.curr_col as c_int, Char.curr_row as c_int - 1)
}

#[no_mangle]
pub unsafe extern "C" fn get_tile_behind_above_char() -> c_int {
    get_tile(
        Char.room as c_int,
        (dir_behind_at((Char.direction as i8 + 1) as usize) as i32 + Char.curr_col as i32) as c_int,
        Char.curr_row as c_int - 1,
    )
}

#[no_mangle]
pub unsafe extern "C" fn get_tile_front_above_char() -> c_int {
    infrontx = (dir_front_at((Char.direction as i8 + 1) as usize) as i32 + Char.curr_col as i32) as i8;
    get_tile(Char.room as c_int, infrontx as c_int, Char.curr_row as c_int - 1)
}

#[no_mangle]
pub unsafe extern "C" fn back_delta_x(delta_x: c_int) -> c_int {
    if Char.direction < directions_dir_0_right as i8 {
        delta_x
    } else {
        -delta_x
    }
}

#[no_mangle]
pub unsafe extern "C" fn do_pickup(obj_type: c_int) {
    pickup_obj_type = obj_type as i16;
    control_shift2 = CONTROL_IGNORE as i8;
    *curr_room_tiles.add(curr_tilepos as usize) = tiles_tiles_1_floor as u8;
    *curr_room_modif.add(curr_tilepos as usize) = 0;
    redraw_height = 35;
    set_wipe(curr_tilepos as c_short, 1);
    set_redraw_full(curr_tilepos as c_short, 1);
}

#[no_mangle]
pub unsafe extern "C" fn check_press() {
    let frame  = Char.frame;
    let action = Char.action;
    if (frame >= frameids_frame_87_hanging_1 as u8 && frame < 100)
        || (frame >= frameids_frame_135_climbing_1 as u8 && frame < frameids_frame_141_climbing_7 as u8)
    {
        get_tile_above_char();
    } else if action == actions_actions_7_turn as u8
        || action == actions_actions_5_bumped as u8
        || (action as u8) < actions_actions_2_hang_climb as u8
    {
        if frame == frameids_frame_79_jumphang as u8 && get_tile_above_char() == tiles_tiles_11_loose as c_int {
            make_loose_fall(1);
        } else {
            if cur_frame.flags & frame_flags_FRAME_NEEDS_FLOOR as u8 == 0 { return; }
            // FIX_PRESS_THROUGH_CLOSED_GATES
            if (*fixes).fix_press_through_closed_gates != 0 { determine_col(); }
            get_tile_at_char();
        }
    } else {
        return;
    }
    if curr_tile2 == tiles_tiles_15_opener as u8 || curr_tile2 == tiles_tiles_6_closer as u8 {
        if Char.alive < 0 {
            trigger_button(1, 0, -1);
        } else {
            died_on_button();
        }
    } else if curr_tile2 == tiles_tiles_11_loose as u8 {
        is_guard_notice = 1;
        make_loose_fall(1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn check_spike_below() {
    let right_col = get_tile_div_mod_m7(char_x_right as c_int);
    if right_col < 0 { return; }
    let room = Char.room;
    let mut col = get_tile_div_mod_m7(char_x_left as c_int);
    while col <= right_col {
        let mut row = Char.curr_row;
        loop {
            let not_finished;
            if get_tile(room as c_int, col, row as c_int) == tiles_tiles_2_spike as c_int {
                start_anim_spike(curr_room, curr_tilepos as c_short);
                not_finished = false;
            } else if tile_is_floor(curr_tile2 as c_int) == 0
                && curr_room != 0
                && if (*fixes).fix_infinite_down_bug != 0 { row <= 2 } else { room as i16 == curr_room }
            {
                row += 1;
                not_finished = true;
            } else {
                not_finished = false;
            }
            if !not_finished { break; }
        }
        col += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn clip_char() {
    let frame  = Char.frame;
    let action = Char.action;
    let room   = Char.room;
    let row    = Char.curr_row;
    reset_obj_clip();
    // USE_SUPER_HIGH_JUMP: clip during super jump
    if (*fixes).enable_super_high_jump != 0
        && (frame == frameids_frame_79_jumphang as u8 || frame == frameids_frame_106_fall as u8)
    {
        let top_left_tile = get_tile(
            room as c_int,
            char_col_left as c_int - 1,
            y_to_row_mod4(char_top_y as c_int + 10),
        );
        if top_left_tile == tiles_tiles_12_doortop as c_int
            && *curr_room_modif.add(curr_tilepos as usize) == 0
        {
            obj_clip_top = y_clip_at((tile_row + 1) as usize) - 22;
            return;
        }
    }
    if frame >= frameids_frame_224_exit_stairs_8 as u8 && frame < 229 {
        obj_clip_top   = leveldoor_ybottom as i16 + 1;
        obj_clip_right = leveldoor_right as i16;
    } else {
        if get_tile(room as c_int, char_col_left as c_int, char_top_row as c_int) == tiles_tiles_20_wall as c_int
            || tile_is_floor(curr_tile2 as c_int) != 0
        {
            if (action == actions_actions_0_stand as u8
                    && (frame == frameids_frame_79_jumphang as u8
                        || frame == frameids_frame_81_hangdrop_1 as u8))
                || get_tile(room as c_int, char_col_right as c_int, char_top_row as c_int) == tiles_tiles_20_wall as c_int
                || tile_is_floor(curr_tile2 as c_int) != 0
            {
                let clip_row = row + 1;
                let clip_y = y_clip_at(clip_row as usize);
                if clip_row == 1 || (clip_y < obj_y as i16 && clip_y - 15 < char_top_y) {
                    char_top_y = clip_y;
                    obj_clip_top = clip_y;
                }
            }
        }
        let col = get_tile_div_mod(char_x_left_coll as c_int - 4);
        if get_tile(room as c_int, col + 1, row as c_int) == tiles_tiles_7_doortop_with_floor as c_int
            || curr_tile2 == tiles_tiles_12_doortop as u8
        {
            obj_clip_right = (tile_col << 5) + 32;
        } else if (get_tile(room as c_int, col, row as c_int) != tiles_tiles_7_doortop_with_floor as c_int
                && curr_tile2 != tiles_tiles_12_doortop as u8)
            || action == actions_actions_3_in_midair as u8
            || (action == actions_actions_4_in_freefall as u8 && frame == frameids_frame_106_fall as u8)
            || (action == actions_actions_5_bumped as u8 && frame == frameids_frame_107_fall_land_1 as u8)
            || (Char.direction < directions_dir_0_right as i8 && (
                action == actions_actions_2_hang_climb as u8
                || action == actions_actions_6_hang_straight as u8
                || (action == actions_actions_1_run_jump as u8
                    && frame >= frameids_frame_137_climbing_3 as u8
                    && frame < frameids_frame_140_climbing_6 as u8)
            ))
        {
            let col2 = get_tile_div_mod(char_x_right_coll as c_int);
            if (get_tile(room as c_int, col2, row as c_int) == tiles_tiles_20_wall as c_int
                    || (curr_tile2 == tiles_tiles_13_mirror as u8
                        && Char.direction == directions_dir_0_right as i8))
                && (get_tile(room as c_int, col2, char_top_row as c_int) == tiles_tiles_20_wall as c_int
                    || curr_tile2 == tiles_tiles_13_mirror as u8)
                && room as i16 == curr_room
            {
                obj_clip_right = tile_col << 5;
            }
        } else {
            obj_clip_right = (tile_col << 5) + 32;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn stuck_lower() {
    if get_tile_at_char() == tiles_tiles_5_stuck as c_int {
        Char.y = Char.y.wrapping_add(1);
    }
}

#[no_mangle]
pub unsafe extern "C" fn set_objtile_at_char() {
    let char_frame  = Char.frame;
    let char_action = Char.action;
    if char_action == actions_actions_1_run_jump as u8 {
        tile_row = char_bottom_row;
        tile_col = char_col_left;
    } else {
        tile_row = Char.curr_row as i16;
        tile_col = Char.curr_col as i16;
    }
    if (char_frame >= frameids_frame_135_climbing_1 as u8 && char_frame < 149)
        || char_action == actions_actions_2_hang_climb as u8
        || char_action == actions_actions_3_in_midair as u8
        || char_action == actions_actions_4_in_freefall as u8
        || char_action == actions_actions_6_hang_straight as u8
    {
        tile_col -= 1;
    }
    obj_tilepos = get_tilepos_nominus(tile_col as c_int, tile_row as c_int) as u8;
}

#[no_mangle]
pub unsafe extern "C" fn proc_get_object() {
    if Char.charid != charids_charid_0_kid as u8 || pickup_obj_type == 0 { return; }
    if pickup_obj_type == -1 {
        have_sword = u16::MAX;
        play_sound(soundids_sound_37_victory as c_int);
        flash_color = colorids_color_14_brightyellow as u16;
        flash_time = 8;
    } else {
        match pickup_obj_type {
            1 => { // health
                if hitp_curr != hitp_max {
                    stop_sounds();
                    play_sound(soundids_sound_33_small_potion as c_int);
                    hitp_delta = 1;
                    flash_color = colorids_color_4_red as u16;
                    flash_time = 2;
                }
            }
            2 => { // life
                stop_sounds();
                play_sound(soundids_sound_30_big_potion as c_int);
                flash_color = colorids_color_4_red as u16;
                flash_time = 4;
                add_life();
            }
            3 => { // feather
                feather_fall();
            }
            4 => { // invert
                toggle_upside();
            }
            6 => { // open
                get_tile(8, 0, 0);
                trigger_button(0, 0, -1);
            }
            5 => { // hurt
                stop_sounds();
                play_sound(soundids_sound_13_kid_hurt as c_int);
                if current_level == 15 {
                    hitp_delta = -((hitp_max as i32 + 1) >> 1) as i16;
                } else {
                    hitp_delta = -1;
                }
            }
            _ => {}
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn is_dead() -> c_int {
    (Char.frame >= frameids_frame_177_spiked as u8
        && (Char.frame <= frameids_frame_178_chomped as u8
            || Char.frame == frameids_frame_185_dead as u8)) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn play_death_music() {
    let sound_id: u32;
    if Guard.charid == charids_charid_1_shadow as u8 {
        sound_id = soundids_sound_32_shadow_music;
    } else if holding_sword != 0 {
        sound_id = soundids_sound_28_death_in_fight;
    } else {
        sound_id = soundids_sound_24_death_regular;
    }
    play_sound(sound_id as c_int);
}

#[no_mangle]
pub unsafe extern "C" fn on_guard_killed() {
    if current_level == 0 {
        checkpoint = 1;
        demo_index = 0;
        demo_time = 0;
    } else if current_level == (*custom).jaffar_victory_level as u16 {
        flash_color = colorids_color_15_brightwhite as u16;
        flash_time = (*custom).jaffar_victory_flash_time as u16;
        is_show_time = 1;
        leveldoor_open = 2;
        play_sound(soundids_sound_43_victory_Jaffar as c_int);
    } else if Char.charid != charids_charid_1_shadow as u8 {
        play_sound(soundids_sound_37_victory as c_int);
    }
}

#[no_mangle]
pub unsafe extern "C" fn clear_char() {
    Char.direction = directions_dir_56_none as i8;
    Char.alive     = 0;
    Char.action    = 0;
    draw_guard_hp(0, guardhp_curr as c_short);
    guardhp_curr = 0;
}

#[no_mangle]
pub unsafe extern "C" fn save_obj() {
    obj2_tilepos    = obj_tilepos;
    obj2_x          = obj_x as u16;
    obj2_y          = obj_y;
    obj2_direction  = obj_direction;
    obj2_id         = obj_id;
    obj2_chtab      = obj_chtab;
    obj2_clip_top    = obj_clip_top;
    obj2_clip_bottom = obj_clip_bottom;
    obj2_clip_left   = obj_clip_left;
    obj2_clip_right  = obj_clip_right;
}

#[no_mangle]
pub unsafe extern "C" fn load_obj() {
    obj_tilepos    = obj2_tilepos;
    obj_x          = obj2_x as i16;
    obj_y          = obj2_y;
    obj_direction  = obj2_direction;
    obj_id         = obj2_id;
    obj_chtab      = obj2_chtab;
    obj_clip_top    = obj2_clip_top;
    obj_clip_bottom = obj2_clip_bottom;
    obj_clip_left   = obj2_clip_left;
    obj_clip_right  = obj2_clip_right;
}

#[no_mangle]
pub unsafe extern "C" fn draw_hurt_splash() {
    let frame = Char.frame;
    if frame != frameids_frame_178_chomped as u8 {
        save_obj();
        obj_tilepos = u8::MAX; // -1 as byte
        if frame == frameids_frame_185_dead as u8
            || (frame >= frameids_frame_106_fall as u8 && frame < 111)
        {
            obj_y = obj_y.wrapping_add(4);
            obj_dx_forward(5);
        } else if frame == frameids_frame_177_spiked as u8 {
            obj_dx_forward(-5);
        } else {
            obj_y = (obj_y as i32 - ((Char.charid == charids_charid_0_kid as u8) as i32 * 4) - 11) as u8;
            obj_dx_forward(5);
        }
        if Char.charid == charids_charid_0_kid as u8 {
            obj_chtab = chtabs_id_chtab_2_kid as u8;
            obj_id = 218;
        } else {
            obj_chtab = chtabs_id_chtab_5_guard as u8;
            obj_id = 1;
        }
        reset_obj_clip();
        add_objtable(5);
        load_obj();
    }
}

#[no_mangle]
pub unsafe extern "C" fn check_killed_shadow() {
    if current_level == 12 {
        if (Char.charid | Opp.charid) == charids_charid_1_shadow as u8
            && Char.alive < 0
            && Opp.alive >= 0
        {
            flash_color = colorids_color_15_brightwhite as u16;
            flash_time = 5;
            take_hp(100);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn add_sword_to_objtable() {
    let frame = Char.frame;
    if (frame >= frameids_frame_229_found_sword as u8 && frame < 238)
        || Char.sword != sword_status_sword_0_sheathed as u8
        || (Char.charid == charids_charid_2_guard as u8 && Char.alive < 0)
    {
        let sword_frame = (cur_frame.sword & 0x3F) as usize;
        if sword_frame != 0 {
            obj_id = SWORD_TBL[sword_frame].id;
            if obj_id != 0xFF {
                obj_x = calc_screen_x_coord(obj_x);
                obj_dx_forward(SWORD_TBL[sword_frame].x as c_int);
                obj_y = (obj_y as i32 + SWORD_TBL[sword_frame].y as i32) as u8;
                obj_chtab = chtabs_id_chtab_0_sword as u8;
                add_objtable(3);
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn control_guard_inactive() {
    if Char.frame == frameids_frame_166_stand_inactive as u8
        && control_down == CONTROL_HELD as i8
    {
        if control_forward == CONTROL_HELD as i8 {
            draw_sword();
        } else {
            control_down = CONTROL_IGNORE as i8;
            seqtbl_offset_char(seqids_seq_80_stand_flipped as c_short);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn char_opp_dist() -> c_int {
    if Char.room != Opp.room {
        return 999;
    }
    let mut distance = Opp.x as i16 - Char.x as i16;
    if Char.direction < directions_dir_0_right as i8 {
        distance = -distance;
    }
    if distance >= 0 && Char.direction != Opp.direction {
        distance += 13;
    }
    distance as c_int
}

#[no_mangle]
pub unsafe extern "C" fn inc_curr_row() {
    Char.curr_row += 1;
}

#[cfg(test)]
#[allow(static_mut_refs)]
mod tests {
    use super::*;
    use std::os::raw::c_int;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // TILE_DIV_TBL and TILE_MOD_TBL each cover the full 0-255 byte range used
    // by the DOS version's tile_div_tbl/tile_mod_tbl.  The frame tables are
    // indexed directly by frame number so wrong sizes silently truncate
    // animations.  SWORD_TBL has one entry per sword frame (0-50).
    #[test]
    fn table_sizes_are_correct() {
        assert_eq!(TILE_DIV_TBL.len(), 256);
        assert_eq!(TILE_MOD_TBL.len(), 256);
        assert_eq!(FRAME_TABLE_KID.len(), 241);
        assert_eq!(FRAME_TBL_GUARD.len(),  41);
        assert_eq!(SWORD_TBL.len(),        51);
    }

    // get_tile_div_mod converts a pixel x-position into a tile column (return
    // value) and a sub-tile pixel offset stored in obj_xl (0..13).
    // The screen coordinate origin is SCREENSPACE_X=58; tiles are 14 px wide.
    #[test]
    fn get_tile_div_mod_column_and_offset() {
        // (xpos, expected_col, expected_obj_xl)
        let cases: &[(c_int, c_int, u8)] = &[
            (58,   0,  0),  // leftmost pixel of column 0
            (65,   0,  7),  // mid-column 0 (65-58=7)
            (71,   0, 13),  // rightmost pixel of column 0
            (72,   1,  0),  // leftmost pixel of column 1
            (100,  3,  0),  // (100-58)=42, 42/14=3 rem 0
            (101,  3,  1),  // offset 1 within column 3
            (226, 12,  0),  // (226-58)=168, 168/14=12
            (44,  -1,  0),  // (44-58)=-14, column -1 (off-screen left)
            (30,  -2,  0),  // (30-58)=-28, column -2
        ];
        unsafe {
            for &(xpos, want_col, want_xl) in cases {
                let col = get_tile_div_mod(xpos);
                assert_eq!(col,    want_col, "xpos={xpos}: column");
                assert_eq!(obj_xl, want_xl,  "xpos={xpos}: obj_xl");
            }
        }
    }

    // y_to_row_mod4 maps a pixel y-position to a tile row in 0..2, or -1 for
    // positions above the room.  Anchored at the exact y_land[] floor values
    // { -8, 55, 118, 181 } which correspond to rows -1, 0, 1, 2.
    #[test]
    fn y_to_row_mod4_at_floor_positions() {
        // (ypos, expected_row)
        let cases: &[(c_int, c_int)] = &[
            ( -8, -1),  // above row 0 (y_land[0])
            ( 55,  0),  // row 0 floor (y_land[1])
            (118,  1),  // row 1 floor (y_land[2])
            (181,  2),  // row 2 floor (y_land[3])
        ];
        unsafe {
            for &(ypos, want) in cases {
                assert_eq!(y_to_row_mod4(ypos), want, "ypos={ypos}");
            }
        }
    }

    // tile_is_floor returns 0 for the eight tile types that have no walkable
    // surface (empty, big-pillar top, door top, wall, four lattice variants)
    // and 1 for everything else.
    #[test]
    fn tile_is_floor_classification() {
        unsafe {
            let non_floor = [
                (tiles_tiles_0_empty         as c_int, "empty"),
                (tiles_tiles_9_bigpillar_top as c_int, "bigpillar_top"),
                (tiles_tiles_12_doortop      as c_int, "doortop"),
                (tiles_tiles_20_wall         as c_int, "wall"),
                (tiles_tiles_26_lattice_down  as c_int, "lattice_down"),
                (tiles_tiles_27_lattice_small as c_int, "lattice_small"),
                (tiles_tiles_28_lattice_left  as c_int, "lattice_left"),
                (tiles_tiles_29_lattice_right as c_int, "lattice_right"),
            ];
            for (t, name) in non_floor {
                assert_eq!(tile_is_floor(t), 0, "{name}");
            }
            let floor = [
                (tiles_tiles_1_floor  as c_int, "floor"),
                (tiles_tiles_2_spike  as c_int, "spike"),
                (tiles_tiles_3_pillar as c_int, "pillar"),
            ];
            for (t, name) in floor {
                assert_eq!(tile_is_floor(t), 1, "{name}");
            }
        }
    }

    // get_tilepos maps (col, row) to a flat tile index in 0..29.
    // Row r begins at r*10 (tbl_line = {0, 10, 20}).
    // Negative rows return -(col+1) as an "above room" sentinel.
    // Any out-of-bounds coord (col<0, col>=10, row>=3) returns 30.
    #[test]
    fn get_tilepos_normal_and_boundary() {
        // (col, row, expected)
        let cases: &[(c_int, c_int, c_int)] = &[
            ( 0,  0,  0),   // top-left
            ( 9,  0,  9),   // top-right
            ( 0,  1, 10),   // row 1 start
            ( 5,  1, 15),
            ( 0,  2, 20),   // row 2 start
            ( 9,  2, 29),   // bottom-right
            ( 0, -1, -1),   // above row 0: -(0+1)
            ( 5, -1, -6),   // above row 0: -(5+1)
            (-1,  0, 30),   // left OOB
            (10,  0, 30),   // right OOB
            ( 0,  3, 30),   // below last row
        ];
        unsafe {
            for &(col, row, want) in cases {
                assert_eq!(get_tilepos(col, row), want, "col={col} row={row}");
            }
        }
    }

    // char_dx_forward adds delta_x to Char.x, negating when facing left.
    // The result is an i32 pixel position (not wrapped to u8).
    #[test]
    fn char_dx_forward_right_and_left() {
        // (direction, char_x, delta, expected)
        let cases: &[(i8, u8, c_int, c_int)] = &[
            (directions_dir_0_right as i8, 100,  5, 105),
            (directions_dir_0_right as i8, 100, -3,  97),
            (directions_dir_FF_left as i8, 100,  5,  95),
            (directions_dir_FF_left as i8, 100, -3, 103),
        ];
        unsafe {
            for &(dir, x, delta, want) in cases {
                Char.direction = dir;
                Char.x = x;
                assert_eq!(char_dx_forward(delta), want, "dir={dir} x={x} delta={delta}");
            }
        }
    }

    // load_frame dispatches to FRAME_TABLE_KID for kid/mouse/shadow and to
    // FRAME_TBL_GUARD for guard/skeleton (with frame -= 149).
    // Kid frame 7 is the first running step: FRAME_TABLE_KID[7] = ft(6,0,0,0,0x4A).
    // Guard frame 150 → FRAME_TBL_GUARD[1]                       = ft(12,0xCD,2,1,0).
    #[test]
    fn load_frame_dispatches_by_charid() {
        unsafe {
            // Kid frame 7
            Char.charid = charids_charid_0_kid as u8;
            Char.frame  = 7;
            load_frame();
            assert_eq!(cur_frame.image,  6,    "kid7: image");
            assert_eq!(cur_frame.dx,     0,    "kid7: dx");
            assert_eq!(cur_frame.flags, 0x4A,  "kid7: flags");

            // Guard frame 150 → index 1 in FRAME_TBL_GUARD
            Char.charid = charids_charid_2_guard as u8;
            Char.frame  = 150;
            load_frame();
            assert_eq!(cur_frame.image, 12,    "guard150: image");
            assert_eq!(cur_frame.dx,     2,    "guard150: dx");
            assert_eq!(cur_frame.dy,     1,    "guard150: dy");

            // Out-of-bounds frame → sentinel image=255
            Char.charid = charids_charid_0_kid as u8;
            Char.frame  = 255;
            load_frame();
            assert_eq!(cur_frame.image, 255, "oob frame → sentinel");
        }
    }

    // fall_accel increments Char.fall_y by FALLING_SPEED_ACCEL (3) each tick
    // while in freefall, capping at FALLING_SPEED_MAX (33).  With feather fall
    // active the increment is 1 and the cap is 4.  Outside freefall: no change.
    #[test]
    fn fall_accel_normal_and_feather() {
        setup();
        unsafe {
            Char.charid = charids_charid_0_kid as u8;

            // Not in freefall → no change
            Char.action = actions_actions_0_stand as u8;
            Char.fall_y = 5;
            fall_accel();
            assert_eq!(Char.fall_y, 5, "stand: fall_y unchanged");

            // Normal freefall: +3 per tick
            Char.action = actions_actions_4_in_freefall as u8;
            is_feather_fall = 0;
            Char.fall_y = 0;
            fall_accel();
            assert_eq!(Char.fall_y, 3, "normal: +3 on first tick");

            // Clamp at 33 (31+3=34 → 33)
            Char.fall_y = 31;
            fall_accel();
            assert_eq!(Char.fall_y, 33, "normal: clamped at 33");
            fall_accel();
            assert_eq!(Char.fall_y, 33, "normal: stays at 33");

            // Feather fall: +1 per tick, cap 4
            is_feather_fall = 1;
            Char.fall_y = 0;
            fall_accel();
            assert_eq!(Char.fall_y, 1, "feather: +1 on first tick");
            Char.fall_y = 4;
            fall_accel();
            assert_eq!(Char.fall_y, 4, "feather: clamped at 4");
        }
    }
}
