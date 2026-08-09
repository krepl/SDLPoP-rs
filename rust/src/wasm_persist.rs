//! Persistent storage for the wasm build's quicksave/save/hall-of-fame/config files, via the
//! Origin Private File System (OPFS) -- Phase 2 feature work, `docs/plans/13-platform-
//! architecture-unification.md`'s deferred "wire quicksave to IndexedDB" item.
//!
//! **Design choice (flagged for review): OPFS, not IndexedDB.** `FileSystemSyncAccessHandle`'s
//! `read`/`write`/`truncate`/`flush` are genuinely *synchronous* browser calls once opened,
//! unlike IndexedDB (always async/Promise-based). That's the deciding factor: this codebase's
//! blocking call chain (`fclose` is an ordinary C ABI function, called from deep inside the
//! game's synchronous loop, which cannot `.await` anything) has no way to use an async API at
//! all without a second Worker + `Atomics.wait`/`SharedArrayBuffer` handshake (the
//! `absurd-sql`/sql.js pattern) -- real, working, but substantially more machinery than this
//! needs. OPFS sync access handles are Worker-only (matches this game's architecture exactly:
//! it already always runs inside one dedicated Worker) and give real synchronous file I/O with
//! no extra plumbing. Supported in Chromium (this project's test environment) and other
//! current browsers; if that ever turns out to matter, the IndexedDB+Atomics path is the
//! fallback design, not attempted here.
//!
//! **Scope (also flagged for review):** only the four known user-writable filenames are
//! persisted -- `QUICKSAVE.SAV`, `PRINCE.SAV`, `PRINCE.HOF`, `SDLPoP.cfg` (confirmed via grep:
//! `get_writable_file_path`/`locate_save_file_` resolve to these bare names on wasm, since
//! neither `SDLPOP_SAVE_PATH` nor `HOME` is ever set there). Anything else written to the VFS
//! (e.g. a recorded replay under `replays_folder`) behaves as before -- in-memory only, lost on
//! reload. Extending the list is a one-line change (`PERSISTENT_FILENAMES` below) if that ever
//! matters.
//!
//! Flow: `init_persistent_storage()` (called once from `worker.js`, awaited before
//! `run_game()`, same shape as the existing asset-preload step) opens a sync access handle per
//! filename and reads any existing content into the VFS (`wasm_vfs`) up front, mirroring how
//! assets are preloaded. From then on, `wasm_libc.rs`'s `fclose`/`fflush` call
//! [`persist_if_tracked`] after every VFS write, which synchronously writes through to OPFS if
//! the path is one of the tracked ones -- no other call site changes.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{FileSystemGetFileOptions, FileSystemSyncAccessHandle};

const PERSISTENT_FILENAMES: &[&str] = &["QUICKSAVE.SAV", "PRINCE.SAV", "PRINCE.HOF", "SDLPoP.cfg"];

fn handles() -> &'static mut std::collections::HashMap<String, FileSystemSyncAccessHandle> {
    static mut HANDLES: Option<std::collections::HashMap<String, FileSystemSyncAccessHandle>> = None;
    unsafe {
        #[allow(static_mut_refs)]
        HANDLES.get_or_insert_with(std::collections::HashMap::new)
    }
}

/// Opens (creating if needed) an OPFS sync access handle for each of `PERSISTENT_FILENAMES`,
/// and preloads any existing content into the VFS -- same effect as `preload_file` for a
/// regular asset, just sourced from OPFS instead of a `fetch()`. A no-op, not an error, if
/// OPFS isn't available (older browser, or a test/headless context with no real storage
/// permission): the game still works, it just falls back to the pre-existing in-memory-only
/// behavior (lost on reload) for these files, same as before this feature existed.
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn init_persistent_storage() {
    let Some(root) = get_opfs_root().await else {
        web_sys::console::warn_1(&"OPFS not available -- quicksave/save/HOF/config will not persist across reloads".into());
        return;
    };

    for &name in PERSISTENT_FILENAMES {
        let Some(handle) = open_sync_handle(&root, name).await else {
            web_sys::console::warn_1(&format!("could not open persistent storage for {name}").into());
            continue;
        };
        if let Some(bytes) = read_all(&handle) {
            if !bytes.is_empty() {
                crate::wasm_vfs::vfs_write(name, bytes);
            }
        }
        handles().insert(name.to_string(), handle);
    }
}

async fn get_opfs_root() -> Option<web_sys::FileSystemDirectoryHandle> {
    let global = js_sys::global();
    let worker_scope: web_sys::WorkerGlobalScope = global.dyn_into().ok()?;
    let storage = worker_scope.navigator().storage();
    let promise = storage.get_directory();
    let value = JsFuture::from(promise).await.ok()?;
    value.dyn_into().ok()
}

async fn open_sync_handle(root: &web_sys::FileSystemDirectoryHandle, name: &str) -> Option<FileSystemSyncAccessHandle> {
    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let file_handle_promise = root.get_file_handle_with_options(name, &options);
    let file_handle_value = match JsFuture::from(file_handle_promise).await {
        Ok(v) => v,
        Err(e) => { web_sys::console::error_2(&"get_file_handle failed:".into(), &e); return None; }
    };
    let file_handle: web_sys::FileSystemFileHandle = file_handle_value.dyn_into().ok()?;
    let access_promise = file_handle.create_sync_access_handle();
    let access_value = match JsFuture::from(access_promise).await {
        Ok(v) => v,
        Err(e) => { web_sys::console::error_2(&"create_sync_access_handle failed:".into(), &e); return None; }
    };
    access_value.dyn_into().ok()
}

fn read_all(handle: &FileSystemSyncAccessHandle) -> Option<Vec<u8>> {
    let size = handle.get_size().ok()? as usize;
    if size == 0 {
        return Some(Vec::new());
    }
    let mut buf = vec![0u8; size];
    let read = handle.read_with_u8_array(&mut buf).ok()? as usize;
    buf.truncate(read);
    Some(buf)
}

/// Called from `wasm_libc.rs`'s `fclose`/`fflush` after every VFS write. A synchronous OPFS
/// write-through if `path` is one of `PERSISTENT_FILENAMES` and its handle was opened
/// successfully at startup; otherwise a no-op (matches the pre-existing in-memory-only
/// behavior for every other path).
pub(crate) fn persist_if_tracked(path: &str, data: &[u8]) {
    let Some(handle) = handles().get(path) else { return };
    // Errors here (quota exceeded, handle closed unexpectedly, ...) are logged, not fatal --
    // losing persistence for one save is far better than crashing the game over it. The
    // in-memory VFS (wasm_vfs::vfs_write, already called by the caller before this) is
    // unaffected either way, so gameplay continues normally even if this fails.
    if handle.truncate_with_f64(0.0).is_err() {
        web_sys::console::warn_1(&format!("failed to truncate persistent storage for {path}").into());
        return;
    }
    if handle.write_with_u8_array(data).is_err() {
        web_sys::console::warn_1(&format!("failed to write persistent storage for {path}").into());
        return;
    }
    if handle.flush().is_err() {
        web_sys::console::warn_1(&format!("failed to flush persistent storage for {path}").into());
    }
}
