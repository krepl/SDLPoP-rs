#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub mod options;
pub mod seqtbl;
pub mod seg004;

#[cfg(test)]
#[allow(static_mut_refs)] // all C globals are static mut; reading them in tests is safe here
mod tests {
    use super::*;
    use std::os::raw::c_int;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

    // prandom is a linear congruential generator (LCG):
    //   seed = seed * 214013 + 2531011
    //   return (seed >> 16) % (max + 1)
    // It drives all in-game randomness: guard reactions, event timing, etc.
    // These expected values anchor the sequence so a future Rust port can be
    // verified against the original C behaviour.
    #[test]
    fn prandom_rng_sequence() {
        setup();
        unsafe {
            random_seed = 0;
            seed_was_init = 1;
            assert_eq!(prandom(255), 38);  // seed -> 2531011;          (2531011 >> 16) % 256
            assert_eq!(prandom(255), 39);  // seed -> 505908858;        (505908858 >> 16) % 256
        }
    }

    // x_to_xh_and_xl decomposes an x pixel position into:
    //   xh = xpos >> 3  (tile column index)
    //   xl = xpos & 7   (pixel offset within the tile, 0–7)
    // (FIX_SPRITE_XPOS is compiled in, enabling the clean bitwise form.)
    // Used throughout collision detection and sprite positioning.
    #[test]
    fn x_to_xh_and_xl_splits_xpos() {
        let cases: &[(c_int, i8, i8)] = &[
            (0,    0,   0),  // origin
            (8,    1,   0),  // exact tile boundary
            (15,   1,   7),  // last pixel before next tile
            (16,   2,   0),
            (100,  12,  4),  // 100 = 12*8 + 4
            (-1,  -1,   7),  // -1 in arithmetic right-shift: -1>>3 = -1, -1&7 = 7
            (-8,  -1,   0),  // -8 = -1 * 8 + 0
        ];
        unsafe {
            for &(xpos, want_xh, want_xl) in cases {
                let (mut xh, mut xl) = (0i8, 0i8);
                x_to_xh_and_xl(xpos, &mut xh, &mut xl);
                assert_eq!((xh, xl), (want_xh, want_xl), "xpos={xpos}");
            }
        }
    }

    // Verify that set_options_to_default puts well-known globals in their expected
    // starting state. Useful as a fixture assertion and as a regression check when
    // options.c is ported to Rust.
    #[test]
    fn set_options_to_default_initializes_known_values() {
        unsafe {
            set_options_to_default();
            assert_eq!(enable_music,       1);
            assert_eq!(enable_fade,        1);
            assert_eq!(enable_flash,       1);
            assert_eq!(enable_text,        1);
            assert_eq!(start_fullscreen,   0);
            assert_eq!(enable_lighting,    0); // off by default; requires opt-in in SDLPoP.ini
        }
    }
}
