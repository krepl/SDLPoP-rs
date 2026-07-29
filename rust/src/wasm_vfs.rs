//! The wasm32 virtual filesystem's shared storage (Phase C,
//! `docs/plans/13-platform-architecture-unification.md`).
//!
//! Split out from `wasm_libc.rs` (which owns the actual `fopen`-family libc shim functions,
//! wasm32-only) because `platform::wasm::WasmRenderer::rw_from_file` needs the exact same
//! backing store -- `SDLPoP.cfg` is the one real caller of `rw_from_file`, and it must see
//! the same files `fopen`-based code writes, not a separate store. `platform::wasm` is also
//! compiled on native under `cargo test` (Phase A), which `wasm_libc.rs` itself can't be
//! (it depends on `js_sys`, a wasm32-only crate) -- so this module holds only the
//! dependency-free storage both sides need, not the libc shim functions themselves.

use std::collections::HashMap;

fn vfs_store() -> &'static mut HashMap<String, Vec<u8>> {
    static mut VFS_STORE: Option<HashMap<String, Vec<u8>>> = None;
    unsafe {
        #[allow(static_mut_refs)]
        VFS_STORE.get_or_insert_with(HashMap::new)
    }
}

pub(crate) fn vfs_read(path: &str) -> Option<Vec<u8>> {
    vfs_store().get(path).cloned()
}

pub(crate) fn vfs_write(path: &str, data: Vec<u8>) {
    vfs_store().insert(path.to_string(), data);
}

pub(crate) fn vfs_contains(path: &str) -> bool {
    vfs_store().contains_key(path)
}

pub(crate) fn vfs_remove(path: &str) -> bool {
    vfs_store().remove(path).is_some()
}
