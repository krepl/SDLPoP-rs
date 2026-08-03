//! Minimal libc shim for `wasm32-unknown-unknown`, which has no real libc at all.
//!
//! Every `extern "C" { fn foo(...); }` declaration scattered across the ported modules
//! (mirroring the original C source's includes) expects *some* symbol named `foo` to
//! exist somewhere in the final link. On native targets that's real glibc, linked
//! automatically. Here, this module *is* that symbol -- each function below is
//! `#[no_mangle] pub unsafe extern "C" fn`, so the linker resolves the existing `extern`
//! declarations to these definitions instead of failing with "undefined symbol". Gated
//! entirely behind `target_arch = "wasm32"`; native builds are completely unaffected
//! (they still link real glibc, unchanged).
//!
//! Scope: the memory/string/parsing functions below are real, correct implementations --
//! not stubs -- since they're simple enough to not need an actual libc. `setjmp`/`longjmp`
//! are declared `extern "C"` in `seg000.rs` for the *native* target only -- wasm32 has no
//! non-local jump primitive, so `seg000.rs`'s `start_game` uses a `catch_unwind`-based retry
//! loop for wasm32 instead of calling them at all (Phase B,
//! `docs/plans/13-platform-architecture-unification.md`); no shim for them exists or is
//! needed here. Two groups are deliberately left as stubs, documented at each definition,
//! because they need a real design decision rather than a mechanical shim:
//! - POSIX directory listing (`opendir`/`readdir`/`closedir`) and file stats
//!   (`stat`/`fstat`/`access`/`chdir`/`mkdir`/`fileno`): these exist to scan `mods/`/`data/`
//!   on a real filesystem. A browser has no such thing -- this needs to route through the
//!   `FileSystem` trait (fetch/IndexedDB-backed), not a libc-level shim. Stubbed to fail
//!   gracefully (matching what these calls already do on a real filesystem when the path
//!   doesn't exist) so linking succeeds; real behavior is a `platform::wasm` task.

use std::collections::HashMap;
use std::os::raw::{c_char, c_int, c_long, c_uint, c_void};

// ============================================================================
// Memory allocation -- backed by Rust's own global allocator, not a real libc heap.
// Each block is prefixed with its `Layout`'s size (stored at a fixed 16-byte-aligned
// offset before the pointer returned to the caller) so free/realloc can recover it.
// ============================================================================

const HEADER_ALIGN: usize = 16;

unsafe fn alloc_with_header(size: usize) -> *mut c_void {
    if size == 0 {
        return std::ptr::null_mut();
    }
    let total = HEADER_ALIGN + size;
    let layout = match std::alloc::Layout::from_size_align(total, HEADER_ALIGN) {
        Ok(l) => l,
        Err(_) => return std::ptr::null_mut(),
    };
    let base = std::alloc::alloc(layout);
    if base.is_null() {
        return std::ptr::null_mut();
    }
    *(base as *mut usize) = size;
    base.add(HEADER_ALIGN) as *mut c_void
}

unsafe fn header_size(ptr: *mut c_void) -> usize {
    *(ptr.cast::<u8>().sub(HEADER_ALIGN) as *mut usize)
}

unsafe fn dealloc_with_header(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let size = header_size(ptr);
    let base = ptr.cast::<u8>().sub(HEADER_ALIGN);
    let total = HEADER_ALIGN + size;
    let layout = std::alloc::Layout::from_size_align_unchecked(total, HEADER_ALIGN);
    std::alloc::dealloc(base, layout);
}

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    alloc_with_header(size)
}

#[no_mangle]
pub unsafe extern "C" fn calloc(nmemb: usize, size: usize) -> *mut c_void {
    let Some(total) = nmemb.checked_mul(size) else {
        return std::ptr::null_mut();
    };
    let ptr = alloc_with_header(total);
    if !ptr.is_null() {
        std::ptr::write_bytes(ptr as *mut u8, 0, total);
    }
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, new_size: usize) -> *mut c_void {
    if ptr.is_null() {
        return alloc_with_header(new_size);
    }
    if new_size == 0 {
        dealloc_with_header(ptr);
        return std::ptr::null_mut();
    }
    let old_size = header_size(ptr);
    let new_ptr = alloc_with_header(new_size);
    if !new_ptr.is_null() {
        let copy_len = old_size.min(new_size);
        std::ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, copy_len);
        dealloc_with_header(ptr);
    }
    new_ptr
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    dealloc_with_header(ptr);
}

// ============================================================================
// mem*/str* -- straightforward pointer-walking reimplementations, C semantics.
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn memcpy(dst: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    std::ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, n);
    dst
}

#[no_mangle]
pub unsafe extern "C" fn memset(s: *mut c_void, c: c_int, n: usize) -> *mut c_void {
    std::ptr::write_bytes(s as *mut u8, c as u8, n);
    s
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> c_int {
    let a = std::slice::from_raw_parts(a as *const u8, n);
    let b = std::slice::from_raw_parts(b as *const u8, n);
    for i in 0..n {
        if a[i] != b[i] {
            return a[i] as c_int - b[i] as c_int;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const c_char) -> usize {
    let mut n = 0usize;
    while *s.add(n) != 0 {
        n += 1;
    }
    n
}

#[no_mangle]
pub unsafe extern "C" fn strnlen(s: *const c_char, maxlen: usize) -> usize {
    let mut n = 0usize;
    while n < maxlen && *s.add(n) != 0 {
        n += 1;
    }
    n
}

unsafe fn cmp_bytes(a: *const c_char, b: *const c_char, fold_case: bool) -> c_int {
    let mut i = 0isize;
    loop {
        let mut ca = *a.offset(i) as u8;
        let mut cb = *b.offset(i) as u8;
        if fold_case {
            ca = ca.to_ascii_lowercase();
            cb = cb.to_ascii_lowercase();
        }
        if ca != cb {
            return ca as c_int - cb as c_int;
        }
        if ca == 0 {
            return 0;
        }
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn strcmp(a: *const c_char, b: *const c_char) -> c_int {
    cmp_bytes(a, b, false)
}

#[no_mangle]
pub unsafe extern "C" fn strcasecmp(a: *const c_char, b: *const c_char) -> c_int {
    cmp_bytes(a, b, true)
}

unsafe fn cmp_bytes_n(a: *const c_char, b: *const c_char, n: usize, fold_case: bool) -> c_int {
    for i in 0..n {
        let mut ca = *a.add(i) as u8;
        let mut cb = *b.add(i) as u8;
        if fold_case {
            ca = ca.to_ascii_lowercase();
            cb = cb.to_ascii_lowercase();
        }
        if ca != cb {
            return ca as c_int - cb as c_int;
        }
        if ca == 0 {
            return 0;
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn strncmp(a: *const c_char, b: *const c_char, n: usize) -> c_int {
    cmp_bytes_n(a, b, n, false)
}

#[no_mangle]
pub unsafe extern "C" fn strncasecmp(a: *const c_char, b: *const c_char, n: usize) -> c_int {
    cmp_bytes_n(a, b, n, true)
}

#[no_mangle]
pub unsafe extern "C" fn strcpy(dst: *mut c_char, src: *const c_char) -> *mut c_char {
    let n = strlen(src);
    std::ptr::copy_nonoverlapping(src, dst, n + 1);
    dst
}

#[no_mangle]
pub unsafe extern "C" fn strncpy(dst: *mut c_char, src: *const c_char, n: usize) -> *mut c_char {
    let src_len = strnlen(src, n);
    std::ptr::copy_nonoverlapping(src, dst, src_len);
    // C's strncpy zero-fills the remainder of `n` when src is shorter.
    if src_len < n {
        std::ptr::write_bytes(dst.add(src_len), 0, n - src_len);
    }
    dst
}

#[no_mangle]
pub unsafe extern "C" fn strdup(s: *const c_char) -> *mut c_char {
    let n = strlen(s);
    let ptr = alloc_with_header(n + 1) as *mut c_char;
    if !ptr.is_null() {
        std::ptr::copy_nonoverlapping(s, ptr, n + 1);
    }
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn strchr(s: *const c_char, c: c_int) -> *mut c_char {
    let target = c as u8;
    let mut i = 0isize;
    loop {
        let ch = *s.offset(i) as u8;
        if ch == target {
            return s.offset(i) as *mut c_char;
        }
        if ch == 0 {
            return std::ptr::null_mut();
        }
        i += 1;
    }
}

#[no_mangle]
pub unsafe extern "C" fn strrchr(s: *const c_char, c: c_int) -> *mut c_char {
    let target = c as u8;
    let n = strlen(s) as isize;
    let mut i = n;
    while i >= 0 {
        if *s.offset(i) as u8 == target {
            return s.offset(i) as *mut c_char;
        }
        i -= 1;
    }
    std::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn strerror(_errnum: c_int) -> *mut c_char {
    // Only ever logged (via fprintf(stderr, ...)); a generic message is fine here since
    // real errno semantics don't exist in this environment anyway.
    static MSG: &[u8] = b"error\0";
    MSG.as_ptr() as *mut c_char
}

// ============================================================================
// ctype / numeric parsing
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn isspace(c: c_int) -> c_int {
    matches!(c as u8, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r') as c_int
}

#[no_mangle]
pub unsafe extern "C" fn atoi(s: *const c_char) -> c_int {
    c_strtoll(s, std::ptr::null_mut(), 10) as c_int
}

#[no_mangle]
pub unsafe extern "C" fn strtol(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_long {
    c_strtoll(nptr, endptr, base) as c_long
}

#[no_mangle]
pub unsafe extern "C" fn strtoimax(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> i64 {
    c_strtoll(nptr, endptr, base)
}

/// Shared parser for `atoi`/`strtol`/`strtoimax` -- C semantics: skip leading whitespace,
/// optional `+`/`-`, then digits in `base` (base 0 auto-detects `0x`/`0`-octal/decimal,
/// same as real `strtol`), stopping at the first non-digit. `endptr`, if non-null, is set
/// to point at that first unparsed character (or `nptr` itself if nothing was parsed).
unsafe fn c_strtoll(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> i64 {
    let mut p = nptr;
    while isspace(*p as c_int) != 0 {
        p = p.add(1);
    }
    let negative = match *p as u8 {
        b'-' => { p = p.add(1); true }
        b'+' => { p = p.add(1); false }
        _ => false,
    };

    let mut base = base as u32;
    if (base == 16 || base == 0) && *p as u8 == b'0' && matches!(*p.add(1) as u8, b'x' | b'X') {
        p = p.add(2);
        base = 16;
    } else if base == 0 {
        base = if *p as u8 == b'0' { 8 } else { 10 };
    }

    let start = p;
    let mut value: i64 = 0;
    while let Some(digit) = (*p as u8 as char).to_digit(base) {
        value = value.wrapping_mul(base as i64).wrapping_add(digit as i64);
        p = p.add(1);
    }

    if !endptr.is_null() {
        *endptr = if p == start { nptr as *mut c_char } else { p as *mut c_char };
    }
    if negative { -value } else { value }
}

// ============================================================================
// Time -- routed through the JS `Date` object (no real wall clock otherwise).
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn time(t: *mut c_long) -> c_long {
    let seconds = (js_sys::Date::now() / 1000.0) as c_long;
    if !t.is_null() {
        *t = seconds;
    }
    seconds
}

#[no_mangle]
pub unsafe extern "C" fn difftime(time1: i64, time0: i64) -> f64 {
    (time1 - time0) as f64
}

// ============================================================================
// Process control
// ============================================================================

/// The JS `Error.message` a clean C-level `exit()` (validate mode ending a replay,
/// `replay.rs`'s `end_replay`/error paths) is thrown with. Same "throw across the wasm/JS
/// boundary" mechanism `seg000.rs`'s `RESTART_SIGNAL` uses and for the same reason --
/// `std::process::abort()`'s `unreachable` trap poisons the wasm instance, but this replay-
/// harness use of `exit()` is a *normal* "the run is over, come read the results back out of
/// the VFS" signal, not a crash, so it must unwind cleanly instead. A driver script catches
/// this specific string to tell "run finished" apart from a real panic, which must still
/// propagate.
pub(crate) const EXIT_SIGNAL: &str = "SDLPOP_EXIT";

#[no_mangle]
pub unsafe extern "C" fn exit(_code: c_int) -> ! {
    wasm_bindgen::throw_str(EXIT_SIGNAL)
}

// ============================================================================
// Deferred: POSIX filesystem. A browser has no real filesystem -- these exist to scan
// mods/data directories, which needs a `FileSystem`-trait-backed reimplementation
// (fetch/IndexedDB), not a libc shim. Stubbed to fail gracefully so linking succeeds.
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn opendir(_name: *const c_char) -> *mut c_void {
    std::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn readdir(_dirp: *mut c_void) -> *mut c_void {
    std::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn closedir(_dirp: *mut c_void) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn access(path: *const c_char, _mode: c_int) -> c_int {
    // Real access() also checks read/write/execute permission bits (the `mode` argument);
    // every real caller in this codebase only ever checks F_OK (existence), so that's all
    // this needs to implement. Also recognizes directory prefixes (see `vfs_contains_dir`)
    // -- `locate_file_`'s first probe is `file_exists(a_loose_resource_folder_path)`, e.g.
    // `"data/IBM_SND1"`, which is never a literal VFS key itself, only a prefix of the real
    // per-resource file keys stored under it.
    let path_str = c_str_to_string(path);
    if vfs_contains(&path_str) || vfs_contains_dir(&path_str) { 0 } else { -1 }
}

#[no_mangle]
pub unsafe extern "C" fn chdir(_path: *const c_char) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn mkdir(_path: *const c_char, _mode: c_uint) -> c_int {
    -1
}

/// Glibc x86-64 `struct stat` field offsets this codebase actually reads (see the matching
/// comment on [`fstat`] below): `st_mode` at byte 24, `st_size` at byte 48. The VFS models no
/// real directories, so a path is reported as a directory if anything is stored *under* it
/// (see [`vfs_contains_dir`]) and as a regular file if it matches exactly -- covering both
/// real callers in this codebase (data-folder-exists checks, and file-size/mtime probing;
/// `st_mtime` is left zeroed, since the VFS keeps no timestamps).
#[no_mangle]
pub unsafe extern "C" fn stat(path: *const c_char, buf: *mut c_void) -> c_int {
    const S_IFDIR: u32 = 0o040000;
    const S_IFREG: u32 = 0o100000;
    let path_str = c_str_to_string(path);
    let (mode, size) = if vfs_contains(&path_str) {
        (S_IFREG, vfs_read(&path_str).map_or(0, |d| d.len() as i64))
    } else if vfs_contains_dir(&path_str) {
        (S_IFDIR, 0)
    } else {
        return -1;
    };
    std::ptr::write_bytes(buf as *mut u8, 0, 144);
    std::ptr::write_unaligned((buf as *mut u8).add(24) as *mut u32, mode);
    std::ptr::write_unaligned((buf as *mut u8).add(48) as *mut i64, size);
    0
}

/// `fd` here is really the VFS file id `fopen` disguised as a pointer (see [`fileno`]) --
/// there's no real file descriptor table on wasm32 to consult. `st_size` is the only field
/// any caller in this codebase reads off an `fstat` result (`load_from_opendats_metadata`,
/// to size a loose-file resource after opening it via the directory fallback), so that's the
/// only one populated; everything else in the buffer is zeroed. Layout matches seg009.rs's
/// local `stat_t` (glibc x86-64 `struct stat`, 144 bytes, `st_size` at byte offset 48) --
/// duplicated here rather than shared, since that struct is private to each file that needs
/// it, the same way it's separately duplicated in replay.rs/options.rs/menu.rs.
#[no_mangle]
pub unsafe extern "C" fn fstat(fd: c_int, buf: *mut c_void) -> c_int {
    let Some(f) = open_files().get(&(fd as usize)) else { return -1 };
    std::ptr::write_bytes(buf as *mut u8, 0, 144);
    std::ptr::write_unaligned((buf as *mut u8).add(48) as *mut i64, f.data.len() as i64);
    0
}

/// Real callers only ever pass this straight into `fstat`, so returning the same id `fopen`
/// handed out (rather than a real OS file descriptor, which doesn't exist here) is enough.
#[no_mangle]
pub unsafe extern "C" fn fileno(stream: *mut c_void) -> c_int {
    if open_files().contains_key(&(stream as usize)) {
        stream as usize as c_int
    } else {
        -1
    }
}

/// Generic in-place sort driven by a C comparator callback, operating on raw
/// `size`-byte elements (insertion sort -- simple and correct; `qsort`'s only caller in
/// this codebase sorts a small in-memory list, so this isn't a hot path).
#[no_mangle]
pub unsafe extern "C" fn qsort(
    base: *mut c_void,
    nmemb: usize,
    size: usize,
    compar: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> c_int>,
) {
    let Some(compar) = compar else { return };
    let base = base as *mut u8;
    let mut tmp = vec![0u8; size];
    for i in 1..nmemb {
        let item = base.add(i * size);
        std::ptr::copy_nonoverlapping(item, tmp.as_mut_ptr(), size);
        let mut j = i;
        while j > 0 {
            let prev = base.add((j - 1) * size);
            if compar(prev as *const c_void, tmp.as_ptr() as *const c_void) <= 0 {
                break;
            }
            std::ptr::copy_nonoverlapping(prev, base.add(j * size), size);
            j -= 1;
        }
        std::ptr::copy_nonoverlapping(tmp.as_ptr(), base.add(j * size), size);
    }
}

#[no_mangle]
pub unsafe extern "C" fn __errno_location() -> *mut c_int {
    static mut ERRNO: c_int = 0;
    std::ptr::addr_of_mut!(ERRNO)
}

// ============================================================================
// `FILE*` I/O -- a real in-memory virtual filesystem (Phase C,
// docs/plans/13-platform-architecture-unification.md), not a stub. Every DAT/asset/INI/
// quicksave/HOF/replay file access in this codebase already goes through plain `fopen`/
// `fread`/`fwrite`/`fseek`/`ftell`/`fclose` (confirmed by a full audit -- the `FileSystem`
// trait turned out to be unused dead code, so this does NOT route through it), so making
// these functions real makes every existing call site work with zero changes anywhere else
// in the crate. `preload_file` is the JS-facing way to populate it -- call it for every
// asset path the game might open, before starting the game.
//
// Semantics: read-mode opens look up the exact path string in `VFS_STORE`; nothing is
// fetched lazily here (a synchronous-XHR-inside-a-Worker fallback is a reasonable future
// refinement, not implemented yet). Write-mode opens start with an empty buffer; on
// `fclose`, the buffer is copied back into `VFS_STORE` under the same path, so a
// quicksave-then-quickload round-trip works within one session even with no real backing
// storage yet -- it just doesn't survive a page reload (another deferred refinement, e.g.
// wiring to IndexedDB).
// ============================================================================

struct VfsFile {
    path: String,
    data: Vec<u8>,
    pos: usize,
    writable: bool,
}

// The actual storage lives in `wasm_vfs` (shared with `platform::wasm::WasmRenderer::
// rw_from_file` -- see that module's doc comment for why it's split out). Re-exported here
// under their old names so the rest of this file's `fopen`-family code doesn't change.
use crate::wasm_vfs::{vfs_contains, vfs_contains_dir, vfs_read, vfs_remove, vfs_write};

fn open_files() -> &'static mut HashMap<usize, VfsFile> {
    static mut OPEN_FILES: Option<HashMap<usize, VfsFile>> = None;
    unsafe {
        #[allow(static_mut_refs)]
        OPEN_FILES.get_or_insert_with(HashMap::new)
    }
}

fn next_file_id() -> usize {
    static mut NEXT_ID: usize = 1;
    unsafe {
        let id = NEXT_ID;
        NEXT_ID += 1;
        id
    }
}

unsafe fn c_str_to_string(s: *const c_char) -> String {
    std::ffi::CStr::from_ptr(s).to_string_lossy().into_owned()
}

/// JS-facing: populate the virtual filesystem with one file's contents, keyed by the exact
/// path the game will `fopen` (relative, matching the native build's own path resolution --
/// e.g. `"data/PRINCE.DAT"`, `"SDLPoP.ini"`). Call before starting the game; there is no
/// on-demand fetch fallback yet, so anything not preloaded will simply fail to open, exactly
/// like a missing file does natively.
#[no_mangle]
pub extern "C" fn wasm_vfs_preload(path: *const c_char, data: *const u8, len: usize) {
    unsafe {
        let path = c_str_to_string(path);
        let bytes = std::slice::from_raw_parts(data, len).to_vec();
        vfs_write(&path, bytes);
    }
}

#[no_mangle]
pub unsafe extern "C" fn fopen(path: *const c_char, mode: *const c_char) -> *mut c_void {
    let path_str = c_str_to_string(path);
    let mode_str = c_str_to_string(mode);
    let writable = mode_str.starts_with('w') || mode_str.starts_with('a');

    let file = if writable {
        // "a" (append) would need seeding pos from any existing content; this codebase
        // never opens a file in append mode (confirmed by the Phase C file-I/O audit), so
        // treating both "w" and "a" as "start empty" is exact, not an approximation.
        VfsFile { path: path_str, data: Vec::new(), pos: 0, writable: true }
    } else {
        match vfs_read(&path_str) {
            Some(bytes) => VfsFile { path: path_str, data: bytes, pos: 0, writable: false },
            None => return std::ptr::null_mut(),
        }
    };
    let id = next_file_id();
    open_files().insert(id, file);
    id as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn fseek(stream: *mut c_void, offset: c_long, whence: c_int) -> c_int {
    let Some(f) = open_files().get_mut(&(stream as usize)) else { return -1 };
    let base = match whence {
        0 => 0i64,                    // SEEK_SET
        1 => f.pos as i64,            // SEEK_CUR
        2 => f.data.len() as i64,     // SEEK_END
        _ => return -1,
    };
    let new_pos = base + offset as i64;
    if new_pos < 0 { return -1; }
    f.pos = new_pos as usize;
    0
}

#[no_mangle]
pub unsafe extern "C" fn ftell(stream: *mut c_void) -> c_long {
    match open_files().get(&(stream as usize)) {
        Some(f) => f.pos as c_long,
        None => -1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn remove(path: *const c_char) -> c_int {
    let path_str = c_str_to_string(path);
    if vfs_remove(&path_str) { 0 } else { -1 }
}

// A browser has no real process environment; this in-memory map lets JS (via
// `wasm_setenv`, `lib.rs`) inject the handful of env vars the harness diagnostics
// (`POPTRACE_OUT`, `POPPIXELS_OUT`, `POPTRACE_TICKS`) already key off of, with zero
// changes needed to that code -- it just calls `getenv` like the native build does.
fn env_store() -> &'static mut HashMap<String, std::ffi::CString> {
    static mut ENV_STORE: Option<HashMap<String, std::ffi::CString>> = None;
    unsafe {
        #[allow(static_mut_refs)]
        ENV_STORE.get_or_insert_with(HashMap::new)
    }
}

/// Sets an env var `getenv` will subsequently see. Called from `lib.rs`'s JS-facing
/// `wasm_setenv` export (that one owns the `#[wasm_bindgen]`/`#[no_mangle]` surface; this
/// one is a plain internal helper so the two don't collide as the same link symbol).
pub(crate) fn wasm_setenv(name: *const c_char, value: *const c_char) {
    unsafe {
        let name = c_str_to_string(name);
        let value = c_str_to_string(value);
        env_store().insert(name, std::ffi::CString::new(value).unwrap_or_default());
    }
}

#[no_mangle]
pub unsafe extern "C" fn getenv(name: *const c_char) -> *mut c_char {
    let name = c_str_to_string(name);
    match env_store().get(&name) {
        Some(v) => v.as_ptr() as *mut c_char,
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn fread(ptr: *mut c_void, size: usize, count: usize, stream: *mut c_void) -> usize {
    let Some(f) = open_files().get_mut(&(stream as usize)) else { return 0 };
    if size == 0 { return 0; }
    let want = size * count;
    let avail = f.data.len().saturating_sub(f.pos);
    let n = want.min(avail);
    if n > 0 {
        std::ptr::copy_nonoverlapping(f.data[f.pos..f.pos + n].as_ptr(), ptr as *mut u8, n);
        f.pos += n;
    }
    n / size
}

#[no_mangle]
pub unsafe extern "C" fn fwrite(ptr: *const c_void, size: usize, count: usize, stream: *mut c_void) -> usize {
    let Some(f) = open_files().get_mut(&(stream as usize)) else { return 0 };
    if size == 0 { return 0; }
    let n = size * count;
    let end = f.pos + n;
    if f.data.len() < end {
        f.data.resize(end, 0);
    }
    let src = std::slice::from_raw_parts(ptr as *const u8, n);
    f.data[f.pos..end].copy_from_slice(src);
    f.pos = end;
    count
}

#[no_mangle]
pub unsafe extern "C" fn fclose(stream: *mut c_void) -> c_int {
    if let Some(f) = open_files().remove(&(stream as usize)) {
        if f.writable {
            vfs_write(&f.path, f.data);
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn perror(_s: *const c_char) {}

#[no_mangle]
pub unsafe extern "C" fn rewind(stream: *mut c_void) {
    if let Some(f) = open_files().get_mut(&(stream as usize)) {
        f.pos = 0;
    }
}

#[no_mangle]
pub unsafe extern "C" fn feof(stream: *mut c_void) -> c_int {
    match open_files().get(&(stream as usize)) {
        Some(f) => (f.pos >= f.data.len()) as c_int,
        None => 1,
    }
}

/// Syncs a writable file's current buffer into `VFS_STORE` without closing it -- unlike
/// `fclose`'s one-time write-back, this can run many times over a file's life. Needed for
/// long-lived diagnostic dumps (`dump_frame_state`/`dump_frame_pixels`, `state_dump.rs`)
/// that `fflush()` after every tick but rely on process-exit-flushes-stdio semantics
/// (true natively) rather than ever calling `fclose` themselves -- on wasm, `exit()`
/// throws instead of running that native cleanup (see `EXIT_SIGNAL`'s doc comment), so
/// without this, everything written after the last explicit `fflush` (in practice,
/// everything, since there is no explicit close) would be silently lost.
#[no_mangle]
pub unsafe extern "C" fn fflush(stream: *mut c_void) -> c_int {
    if let Some(f) = open_files().get(&(stream as usize)) {
        if f.writable {
            vfs_write(&f.path, f.data.clone());
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn fgetc(stream: *mut c_void) -> c_int {
    let Some(f) = open_files().get_mut(&(stream as usize)) else { return -1 };
    if f.pos >= f.data.len() { return -1 }; // EOF
    let c = f.data[f.pos];
    f.pos += 1;
    c as c_int
}

#[no_mangle]
pub unsafe extern "C" fn fputc(c: c_int, stream: *mut c_void) -> c_int {
    let Some(f) = open_files().get_mut(&(stream as usize)) else { return -1 };
    let byte = c as u8;
    if f.pos >= f.data.len() {
        f.data.push(byte);
    } else {
        f.data[f.pos] = byte;
    }
    f.pos += 1;
    c
}

#[no_mangle]
pub unsafe extern "C" fn fputs(s: *const c_char, stream: *mut c_void) -> c_int {
    let bytes = std::ffi::CStr::from_ptr(s).to_bytes();
    let Some(f) = open_files().get_mut(&(stream as usize)) else { return -1 };
    let end = f.pos + bytes.len();
    if f.data.len() < end {
        f.data.resize(end, 0);
    }
    f.data[f.pos..end].copy_from_slice(bytes);
    f.pos = end;
    0
}

/// `seg009.rs` declares `static mut stderr: *mut FILE` (the real glibc global, on
/// native). No real stream exists here; the print-family call sites route through
/// `fprintf(stderr, ...)`-style calls, all destined to be replaced by idiomatic Rust
/// logging (see the wasm plan notes) rather than kept as real C stdio.
#[no_mangle]
pub static mut stderr: *mut c_void = std::ptr::null_mut();
