#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
// bindgen (0.70) emits `mem::transmute` for bitfield accessors where a newer rustc lint
// wants `cast_signed`/`cast_unsigned` instead; this is generated code we don't control.
#![allow(unnecessary_transmutes)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// Plain Rust translations of what used to be src/data.c's globals (Step B of the
// post-port refactor plan). Spliced at crate-root scope, same as bindings.rs above, so
// every existing `use super::*;` call site keeps resolving them exactly as before --
// zero call-site changes. See globals.rs's own header comment for details.
include!("globals.rs");

// Shared libc functions used across multiple modules.
// FILE comes from bindings.rs (pub type FILE = _IO_FILE).
// Declared once here; modules bring them in via `use super::*`.
use std::os::raw::{c_char, c_int, c_long, c_void};
extern "C" {
    pub fn fopen(path: *const c_char, mode: *const c_char) -> *mut FILE;
    pub fn fread(ptr: *mut c_void, size: usize, count: usize, stream: *mut FILE) -> usize;
    pub fn fwrite(ptr: *const c_void, size: usize, count: usize, stream: *mut FILE) -> usize;
    pub fn fclose(stream: *mut FILE) -> c_int;
    pub fn fseek(stream: *mut FILE, offset: c_long, whence: c_int) -> c_int;
    pub fn remove(path: *const c_char) -> c_int;
    pub fn perror(s: *const c_char);
    pub fn getenv(name: *const c_char) -> *mut c_char;
    pub fn setenv(name: *const c_char, value: *const c_char, overwrite: c_int) -> c_int;
    pub fn unsetenv(name: *const c_char) -> c_int;
}

// Shared replacements for the C variadic print/format family (printf/fprintf/snprintf).
// These are call sites we own (ported C source, not a real external API), and stable Rust
// can't *define* a variadic extern "C" function at all (that needs the nightly-only
// `c_variadic` feature) -- so rather than shim C's variadic ABI, each call site is
// rewritten to build a Rust `String` via `format!`/`write!` and go through one of these.
// Used on every target, not just wasm32: it's simpler and more idiomatic than the libc
// calls it replaces, and shrinks the crate's `unsafe extern "C"` footprint everywhere.

/// Diagnostic/warning output (`printf(...)`/`puts(...)` in the C source). Native: stdout.
/// wasm32: browser devtools console.
pub(crate) fn c_log(s: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    print!("{s}");
    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&s.into());
}

/// Error output (`fprintf(stderr, ...)` in the C source). Native: stderr. wasm32: browser
/// devtools console (as an error-level entry).
pub(crate) fn c_log_err(s: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    eprint!("{s}");
    #[cfg(target_arch = "wasm32")]
    web_sys::console::error_1(&s.into());
}

/// Replaces `snprintf(buf, size, fmt, ...)`: writes `s` (already formatted by the caller,
/// typically via `format!`) into a raw C buffer of `size` bytes, NUL-terminated,
/// truncating safely if it doesn't fit. Returns the *full* formatted length (not the
/// truncated copy length) -- matching real `snprintf`'s return value, since
/// `snprintf_check`-style callers compare it against `size` to detect truncation.
pub(crate) unsafe fn write_c_str_truncating(buf: *mut c_char, size: usize, s: &str) -> c_int {
    let bytes = s.as_bytes();
    if size > 0 {
        let copy_len = bytes.len().min(size - 1);
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, copy_len);
        *buf.add(copy_len) = 0;
    }
    bytes.len() as c_int
}

// x_bump and y_land are extern const incomplete arrays; bindgen emits [T; 0].
// Index via raw pointer to avoid the zero-length slice panic.
pub(crate) unsafe fn x_bump_at(idx: usize) -> u8 {
    *core::ptr::addr_of!(x_bump).cast::<u8>().add(idx)
}
pub(crate) unsafe fn y_land_at(idx: usize) -> i16 {
    *core::ptr::addr_of!(y_land).cast::<i16>().add(idx)
}

// Helper to access sound_interruptible — bindgen emits [byte; 0] for extern arrays
pub(crate) unsafe fn sound_interruptible_at(idx: usize) -> u8 {
    *core::ptr::addr_of!(sound_interruptible).cast::<u8>().add(idx)
}
pub(crate) unsafe fn sound_interruptible_set(idx: usize, val: u8) {
    *core::ptr::addr_of_mut!(sound_interruptible).cast::<u8>().add(idx) = val;
}
// doorlink1_ad and doorlink2_ad are extern byte arrays; bindgen emits [byte; 0]
pub(crate) unsafe fn doorlink1_ad_at(idx: usize) -> u8 {
    *doorlink1_ad.add(idx)
}
pub(crate) unsafe fn doorlink2_ad_at(idx: usize) -> u8 {
    *doorlink2_ad.add(idx)
}
pub mod seg004;
pub mod seg005;
pub mod seg006;
pub mod seg007;
pub mod seg002;
pub mod seg003;
pub mod seg001;
pub mod seg008;
pub mod seg000;
pub mod seg009;
pub mod sdl_rw_wrappers;
pub mod lighting;
pub mod state_dump;
pub mod seqtbl;
pub mod options;
pub mod screenshot;
pub mod replay;
pub mod opl3;
pub mod midi;
pub mod menu;
pub mod ogg_decode;
pub mod platform;
pub mod state;
#[cfg(target_arch = "wasm32")]
pub mod wasm_libc;
// Dependency-free VFS storage shared by wasm_libc.rs (wasm32-only, uses js_sys elsewhere in
// that file) and platform::wasm (also compiled on native under `cargo test`, see there).
#[cfg(any(target_arch = "wasm32", test))]
pub mod wasm_vfs;
// OPFS-backed persistence for quicksave/save/HOF/config (Phase 2 feature work) -- wasm32-only,
// uses web_sys types not available/needed on native.
#[cfg(target_arch = "wasm32")]
pub mod wasm_persist;

/// Browser entry point (Phase 2 exploratory milestone). `main()` in `main.rs` is the
/// wasm module's `main` export, but wasm-bindgen's JS glue doesn't call it automatically
/// -- `#[wasm_bindgen(start)]` is what the generated JS actually invokes right after
/// instantiation. Deliberately NOT calling `pop_main()` yet: that's real game-loop/asset-
/// loading logic that will hit real, unresolved architecture questions (a blocking C-style
/// game loop vs. the browser's non-blocking event loop; synchronous DAT-file loading vs.
/// `fetch`'s async API) -- this first entry point exists only to prove the wasm-bindgen /
/// canvas JS pipeline itself works, isolated from those questions.
#[cfg(target_arch = "wasm32")]
mod wasm_entry {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(start)]
    pub fn start() {
        console_error_panic_hook::set_once();
        web_sys::console::log_1(&"SDLPoP wasm module loaded".into());
    }

    /// Draws a small test pattern directly to the page's `<canvas id="screen">`, bypassing
    /// all game logic -- exercises the same `CanvasRenderingContext2d`/`ImageData` calls a
    /// real `WasmRenderer::present` will need, without needing any of the rest of the
    /// engine running yet.
    #[wasm_bindgen]
    pub fn draw_test_pattern() {
        let window = web_sys::window().expect("no global window");
        let document = window.document().expect("no document");
        let canvas = document
            .get_element_by_id("screen")
            .expect("no #screen canvas")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("#screen is not a canvas");
        let ctx = canvas
            .get_context("2d")
            .expect("no 2d context")
            .expect("no 2d context")
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .expect("not a 2d context");

        let (w, h) = (320u32, 200u32);
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                pixels[i] = (x * 255 / w) as u8; // R: horizontal gradient
                pixels[i + 1] = (y * 255 / h) as u8; // G: vertical gradient
                pixels[i + 2] = 128; // B: constant
                pixels[i + 3] = 255; // A: opaque
            }
        }
        let image_data =
            web_sys::ImageData::new_with_u8_clamped_array_and_sh(wasm_bindgen::Clamped(&pixels), w, h)
                .expect("failed to build ImageData");
        ctx.put_image_data(&image_data, 0.0, 0.0)
            .expect("put_image_data failed");
        web_sys::console::log_1(&"drew test pattern".into());
    }

    /// Populates the wasm32 virtual filesystem (`wasm_libc.rs`) with one file's bytes,
    /// keyed by the exact relative path the game will `fopen` (e.g. `"data/PRINCE.DAT"`,
    /// `"SDLPoP.ini"`). Call once per asset before `run_game()`/the real game entry point.
    #[wasm_bindgen]
    pub fn preload_file(path: String, data: &[u8]) {
        let cpath = std::ffi::CString::new(path).expect("path must not contain a NUL byte");
        crate::wasm_libc::wasm_vfs_preload(cpath.as_ptr(), data.as_ptr(), data.len());
    }

    /// Hands the wasm module the `SharedArrayBuffer` the main thread writes live keyboard/
    /// mouse state into (see `platform::wasm`'s "Live input" section for the full design and
    /// why a `SharedArrayBuffer` is needed at all). Call once, before `run_game()` --
    /// `worker.js` waits for an `'init'` message carrying this buffer before doing anything
    /// else, since that's the one message a Worker's `onmessage` handler *can* receive (it
    /// arrives before `pop_main()`'s blocking loop starts, unlike any message sent after).
    #[wasm_bindgen]
    pub fn set_shared_input_buffer(buf: js_sys::SharedArrayBuffer) {
        crate::platform::wasm::set_shared_input_buffer(buf);
    }

    /// Exploratory Phase B milestone: actually call `pop_main()`, with no real
    /// `WasmRenderer`/`WasmInput`/`WasmAudio`/`WasmFiles` implementation behind it yet --
    /// used to empirically discover exactly what real platform surface `pop_main`'s startup
    /// path needs, one `unimplemented!()` panic at a time, rather than guessing the full
    /// Worker/message-protocol design up front. Not part of the real game-loading path.
    #[wasm_bindgen]
    pub fn run_game() {
        console_error_panic_hook::set_once();
        unsafe {
            crate::g_argc = 1;
            static mut ARGV0: [std::os::raw::c_char; 7] = [
                b'p' as _, b'r' as _, b'i' as _, b'n' as _, b'c' as _, b'e' as _, 0,
            ];
            static mut ARGV: [*mut std::os::raw::c_char; 1] = [std::ptr::null_mut()];
            #[allow(static_mut_refs)]
            {
                ARGV[0] = ARGV0.as_mut_ptr();
                crate::g_argv = ARGV.as_mut_ptr();
            }
            crate::pop_main();
        }
    }

    /// Like [`run_game`], but with caller-chosen `argv` instead of the hardcoded
    /// `["prince"]` -- lets a headless driver pass `["prince", "validate", "<replay path>"]`
    /// the same way the native harness invokes `prince validate <replay>` (see
    /// `check_param`, `seg009.rs`), so the wasm build can run one of the harness's existing
    /// golden replays instead of only ever taking live keyboard input.
    ///
    /// A validated replay run ends by calling C's `exit()`, which on wasm32
    /// (`wasm_libc::exit`) throws [`crate::wasm_libc::EXIT_SIGNAL`] rather than aborting --
    /// callers must catch that specific string as "run finished cleanly," same as
    /// `resume_game_after_restart`'s callers already do for `RESTART_SIGNAL`.
    #[wasm_bindgen]
    pub fn run_game_with_args(args: Vec<String>) {
        console_error_panic_hook::set_once();
        unsafe {
            // Leaked deliberately: argv must outlive this call (g_argv is read for the
            // process's entire lifetime, e.g. by check_param on every start_game/restart),
            // and a wasm module instance only ever runs one game session.
            let cargs: Vec<std::ffi::CString> = args
                .into_iter()
                .map(|s| std::ffi::CString::new(s).expect("arg must not contain a NUL byte"))
                .collect();
            let mut argv: Vec<*mut std::os::raw::c_char> =
                cargs.iter().map(|s| s.as_ptr() as *mut _).collect();
            std::mem::forget(cargs);
            crate::g_argc = argv.len() as _;
            crate::g_argv = argv.as_mut_ptr();
            std::mem::forget(argv);
            crate::pop_main();
        }
    }

    /// JS-facing: set an env var the game's `getenv` calls will see (`POPTRACE_OUT`,
    /// `POPPIXELS_OUT`, `POPTRACE_TICKS`, ...). Call before `run_game`/`run_game_with_args`.
    #[wasm_bindgen]
    pub fn wasm_setenv(name: String, value: String) {
        let cname = std::ffi::CString::new(name).expect("name must not contain a NUL byte");
        let cvalue = std::ffi::CString::new(value).expect("value must not contain a NUL byte");
        crate::wasm_libc::wasm_setenv(cname.as_ptr(), cvalue.as_ptr());
    }

    /// JS-facing: read back a file the game wrote into the virtual filesystem (e.g. a
    /// `POPTRACE_OUT`/`POPPIXELS_OUT` diagnostic dump) -- the write-mode-`fopen` half of the
    /// same VFS `preload_file` populates for reads. Returns an empty `Vec` if the path was
    /// never written (indistinguishable from "written but empty"; callers needing to tell
    /// those apart should check the file's existence some other way first).
    #[wasm_bindgen]
    pub fn read_vfs_file(path: String) -> Vec<u8> {
        crate::wasm_vfs::vfs_read(&path).unwrap_or_default()
    }

    /// The JS-facing re-entry point a restart request unwinds to. `seg000.rs`'s wasm32
    /// `start_game` throws a JS `Error` (`seg000::RESTART_SIGNAL`) to signal a restart --
    /// the only non-local-control mechanism that actually works on this target (`catch_unwind`
    /// does not; see that function's doc comment for why). That throw necessarily unwinds all
    /// the way back to whatever JS call is currently running, discarding `pop_main()`'s own
    /// frame along with everything below it -- harmless, since `pop_main()`/`init_game_main()`
    /// do only one-time setup (asset loading, `SDL_Init`, ...) before their single call into
    /// `start_game`, and none of it should re-run on a restart. So the retry loop (`worker.js`)
    /// calls `run_game()` exactly once, and calls this instead for every restart after that --
    /// it re-enters directly at `start_game_body()`, skipping straight past that setup.
    #[wasm_bindgen]
    pub fn resume_game_after_restart() {
        unsafe { crate::seg000::start_game_body() };
    }
}

// Shared support for tests that touch real files on disk (quicksave, hall-of-fame,
// long-term save). `getenv`/`setenv` are process-global, not thread-local, and `cargo
// test` runs tests in parallel threads by default, so any test that sets SDLPOP_SAVE_PATH
// must hold ENV_LOCK for its whole body.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

    static SCRATCH_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A uniquely-named temp directory, removed on drop (even if a test panics).
    pub(crate) struct ScratchDir(pub PathBuf);

    impl ScratchDir {
        pub(crate) fn new(tag: &str) -> Self {
            let n = SCRATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sdlpop-test-{tag}-{}-{n}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create scratch dir");
            ScratchDir(path)
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Points `SDLPOP_SAVE_PATH` (read by `get_writable_file_path`, seg000.rs) at `path`, via
    /// the shared `setenv` (not `std::env::set_var`, which has no wasm32-unknown-unknown
    /// backing) so this exercises the exact same env-var mechanism the real "headless" flag
    /// startup path now uses too, not a native-only shortcut.
    pub(crate) unsafe fn set_save_path_env(path: &std::path::Path) {
        let value = std::ffi::CString::new(path.to_str().expect("scratch path must be UTF-8")).unwrap();
        crate::setenv(c"SDLPOP_SAVE_PATH".as_ptr(), value.as_ptr(), 1);
    }

    pub(crate) unsafe fn remove_save_path_env() {
        crate::unsetenv(c"SDLPOP_SAVE_PATH".as_ptr());
    }
}

#[cfg(test)]
#[allow(static_mut_refs)] // all C globals are static mut; reading them in tests is safe here
mod tests {
    use super::*;
    use std::os::raw::c_int;

    fn setup() {
        unsafe { set_options_to_default(); }
    }

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
