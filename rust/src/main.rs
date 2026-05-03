use std::ffi::CString;
use std::os::raw::c_char;

use sdlpop::*;

fn main() {
    let args: Vec<CString> = std::env::args()
        .map(|a| CString::new(a).unwrap())
        .collect();
    let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();

    unsafe {
        g_argc = args.len() as _;
        g_argv = argv.as_mut_ptr();
        pop_main();
    }
}
