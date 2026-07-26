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
//! not stubs -- since they're simple enough to not need an actual libc. Three groups are
//! deliberately left as stubs, documented at each definition, because they need a real
//! design decision rather than a mechanical shim:
//! - `setjmp`/`longjmp`: wasm32 has no non-local jump primitive; `seg000.rs`'s restart-game
//!   mechanism needs restructuring (e.g. a retry loop wrapping `catch_unwind`) to work at
//!   all here, which is a real, separate change to core control flow, not a shim.
//! - POSIX directory listing (`opendir`/`readdir`/`closedir`) and file stats
//!   (`stat`/`fstat`/`access`/`chdir`/`mkdir`/`fileno`): these exist to scan `mods/`/`data/`
//!   on a real filesystem. A browser has no such thing -- this needs to route through the
//!   `FileSystem` trait (fetch/IndexedDB-backed), not a libc-level shim. Stubbed to fail
//!   gracefully (matching what these calls already do on a real filesystem when the path
//!   doesn't exist) so linking succeeds; real behavior is a `platform::wasm` task.

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

#[no_mangle]
pub unsafe extern "C" fn exit(_code: c_int) -> ! {
    std::process::abort()
}

// ============================================================================
// Deferred: non-local jump. `seg000.rs`'s restart-game loop calls these unconditionally
// on every `start_game()` invocation, so the symbols must exist to link at all, but true
// setjmp/longjmp semantics (a single call site returning twice, popping arbitrary stack
// frames) have no wasm32 equivalent without restructuring that loop into something like a
// `catch_unwind`-wrapped retry loop -- a real change to core control flow, not a shim.
// These panic if actually reached at runtime; not yet exercised by a real boot attempt.
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn setjmp(_env: *mut u8) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn longjmp(_env: *mut u8, _val: c_int) -> ! {
    panic!("longjmp: not implemented for wasm32 -- seg000.rs's restart-game loop needs restructuring for this target, see rust/src/wasm_libc.rs")
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
pub unsafe extern "C" fn access(_path: *const c_char, _mode: c_int) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn chdir(_path: *const c_char) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn mkdir(_path: *const c_char, _mode: c_uint) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn stat(_path: *const c_char, _buf: *mut c_void) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn fstat(_fd: c_int, _buf: *mut c_void) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn fileno(_stream: *mut c_void) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn __errno_location() -> *mut c_int {
    static mut ERRNO: c_int = 0;
    std::ptr::addr_of_mut!(ERRNO)
}

// ============================================================================
// Deferred: `FILE*` I/O. Same story as the POSIX filesystem functions above -- real file
// access in a browser needs to route through the `FileSystem` trait (fetch for reads,
// `localStorage`/IndexedDB for quicksave writes), not a libc-level shim. Stubbed to fail
// gracefully (`fopen` always returns null, matching what every caller already checks for)
// so linking succeeds.
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn fopen(_path: *const c_char, _mode: *const c_char) -> *mut c_void {
    std::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn fseek(_stream: *mut c_void, _offset: c_long, _whence: c_int) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn ftell(_stream: *mut c_void) -> c_long {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn remove(_path: *const c_char) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn getenv(_name: *const c_char) -> *mut c_char {
    std::ptr::null_mut()
}

#[no_mangle]
pub unsafe extern "C" fn fread(_ptr: *mut c_void, _size: usize, _count: usize, _stream: *mut c_void) -> usize {
    0
}

#[no_mangle]
pub unsafe extern "C" fn fwrite(_ptr: *const c_void, _size: usize, _count: usize, _stream: *mut c_void) -> usize {
    0
}

#[no_mangle]
pub unsafe extern "C" fn fclose(_stream: *mut c_void) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn perror(_s: *const c_char) {}

#[no_mangle]
pub unsafe extern "C" fn rewind(_stream: *mut c_void) {}

#[no_mangle]
pub unsafe extern "C" fn feof(_stream: *mut c_void) -> c_int {
    1
}

#[no_mangle]
pub unsafe extern "C" fn fflush(_stream: *mut c_void) -> c_int {
    0
}

#[no_mangle]
pub unsafe extern "C" fn fgetc(_stream: *mut c_void) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn fputc(_c: c_int, _stream: *mut c_void) -> c_int {
    -1
}

#[no_mangle]
pub unsafe extern "C" fn fputs(_s: *const c_char, _stream: *mut c_void) -> c_int {
    -1
}

/// `seg009.rs` declares `static mut stderr: *mut FILE` (the real glibc global, on
/// native). No real stream exists here; the print-family call sites route through
/// `fprintf(stderr, ...)`-style calls, all destined to be replaced by idiomatic Rust
/// logging (see the wasm plan notes) rather than kept as real C stdio.
#[no_mangle]
pub static mut stderr: *mut c_void = std::ptr::null_mut();
