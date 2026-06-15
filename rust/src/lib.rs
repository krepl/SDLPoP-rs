#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// x_bump and y_land are extern const incomplete arrays; bindgen emits [T; 0].
// Index via raw pointer to avoid the zero-length slice panic.
pub(crate) unsafe fn x_bump_at(idx: usize) -> u8 {
    *core::ptr::addr_of!(x_bump).cast::<u8>().add(idx)
}
pub(crate) unsafe fn y_land_at(idx: usize) -> i16 {
    *core::ptr::addr_of!(y_land).cast::<i16>().add(idx)
}

pub mod state;
use state::State;

pub(crate) unsafe fn dir_front_at(idx: usize) -> i8 {
    *core::ptr::addr_of!(dir_front).cast::<i8>().add(idx)
}
pub(crate) unsafe fn dir_behind_at(idx: usize) -> i8 {
    *core::ptr::addr_of!(dir_behind).cast::<i8>().add(idx)
}
pub(crate) unsafe fn tbl_line_at(idx: usize) -> u8 {
    *core::ptr::addr_of!(tbl_line).cast::<u8>().add(idx)
}
pub(crate) unsafe fn y_clip_at(idx: usize) -> i16 {
    *core::ptr::addr_of!(y_clip).cast::<i16>().add(idx)
}

pub mod seg004;

/// Single global State instance bridging C interop and Rust internals.
/// #[no_mangle] wrapper functions delegate to inner fns via &mut STATE.
pub(crate) static mut STATE: State = unsafe { std::mem::zeroed() };

#[cfg(test)]
#[allow(static_mut_refs)] // all C globals are static mut; reading them in tests is safe here
mod tests {
    use super::*;

    // y_land is extern const short y_land[] — incomplete array, bindgen emits [c_short; 0].
    // Values are the y pixel positions for each row floor: { -8, 55, 118, 181, 244 }.
    #[test]
    fn y_land_readable_via_raw_pointer() {
        unsafe {
            assert_eq!(y_land_at(0), -8);   // ceiling / above row 0
            assert_eq!(y_land_at(1),  55);  // row 0 floor
            assert_eq!(y_land_at(2), 118);  // row 1 floor
            assert_eq!(y_land_at(3), 181);  // row 2 floor
            assert_eq!(y_land_at(4), 244);  // row 3 floor
        }
    }

    // prandom is a linear congruential generator (LCG):
    //   seed = seed * 214013 + 2531011
    //   return (seed >> 16) % (max + 1)
    // It drives all in-game randomness: guard reactions, event timing, etc.
    // These expected values anchor the sequence so a future Rust port can be
    // verified against the original C behaviour.
    #[test]
    fn prandom_rng_sequence() {
        unsafe {
            random_seed = 0;
            seed_was_init = 1;
            assert_eq!(prandom(255), 38);  // seed -> 2531011;          (2531011 >> 16) % 256
            assert_eq!(prandom(255), 39);  // seed -> 505908858;        (505908858 >> 16) % 256
        }
    }

    // x_to_xh_and_xl_splits_xpos — restore this test when seg006.c is ported.

    // set_options_to_default_initializes_known_values — restore this test when options.c is ported.
}
