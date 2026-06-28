#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
#![allow(static_mut_refs)]

use std::os::raw::{c_char, c_int, c_void};
use super::*;

extern "C" {
    fn SDL_RWwrite(
        context: *mut SDL_RWops,
        ptr: *const c_void,
        size: usize,
        num: usize,
    ) -> usize;
    fn SDL_RWread(
        context: *mut SDL_RWops,
        ptr: *mut c_void,
        size: usize,
        maxnum: usize,
    ) -> usize;
}

#[no_mangle]
pub unsafe extern "C" fn process_rw_write(
    rw: *mut SDL_RWops,
    data: *mut c_void,
    data_size: usize,
) -> c_int {
    SDL_RWwrite(rw, data, data_size, 1) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn process_rw_read(
    rw: *mut SDL_RWops,
    data: *mut c_void,
    data_size: usize,
) -> c_int {
    SDL_RWread(rw, data, data_size, 1) as c_int
}

// KEY_VALUE_LIST(never_is_16, {{"Never", 16}}); expands to:
//   const key_value_type never_is_16[] = {{"Never", 16}};
//   names_list_type never_is_16_list = {.type=1, .kv_pairs={(key_value_type*)&never_is_16, 1}};
// Defined here (not in the Rust port of options.c) because menu.c holds an
// extern reference to it for the in-game settings UI.
static never_is_16: [key_value_type; 1] = [key_value_type {
    key: [
        b'N' as c_char,
        b'e' as c_char,
        b'v' as c_char,
        b'e' as c_char,
        b'r' as c_char,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ],
    value: 16,
}];

#[no_mangle]
pub static mut never_is_16_list: names_list_type = names_list_type {
    type_: 1,
    __bindgen_anon_1: names_list_type__bindgen_ty_1 {
        kv_pairs: names_list_type__bindgen_ty_1__bindgen_ty_2 {
            data: &never_is_16 as *const [key_value_type; 1] as *mut key_value_type,
            count: 1,
        },
    },
};
