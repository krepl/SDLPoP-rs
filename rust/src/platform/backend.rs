//! Compile-time backend selection.
//!
//! `platform::sdl::shared_renderer()`/`shared_audio()`/`shared_input()` (called from all
//! ~224 real call sites across the codebase) return `&'static mut ActiveRenderer`/
//! `ActiveAudio`/`ActiveInput` -- type aliases resolved here, not the concrete `Sdl*`
//! structs directly. Every call site only ever calls trait methods (`Renderer`/
//! `AudioBackend`/`InputSource`), so adding a second backend (a `WasmRenderer` for
//! `wasm32`, a `HeadlessRenderer` for tests) means adding a `#[cfg]` arm *here* and
//! nowhere else -- no call-site changes anywhere in the ~11 files that reach the
//! singletons today.
//!
//! This is deliberately *not* threaded dependency injection: the concrete backend is
//! chosen once, at compile time, for the whole binary. Two backends can't coexist in one
//! process. That tradeoff was a deliberate choice (see the project memory / plan entry on
//! this decision) -- true DI would mean threading a `Platform` parameter through close to
//! the entire ~645-function port, which is a much bigger and riskier change than the goals
//! motivating this (a WASM build, a headless test backend) actually require.
//!
//! No second backend exists yet, so the `not(target_arch = "wasm32")` guard below is
//! purely a seam: it makes an accidental `wasm32` build fail fast, right here, with a
//! clear "no ActiveRenderer for this target" error, instead of failing deep inside
//! `seg009.rs` on a missing `SDL_*` symbol.

#[cfg(not(target_arch = "wasm32"))]
pub type ActiveRenderer = crate::platform::sdl::SdlRenderer;
#[cfg(not(target_arch = "wasm32"))]
pub type ActiveAudio = crate::platform::sdl::SdlAudio;
#[cfg(not(target_arch = "wasm32"))]
pub type ActiveInput = crate::platform::sdl::SdlInput;
#[cfg(not(target_arch = "wasm32"))]
pub type ActiveFiles = crate::platform::sdl::SdlFiles;

// Future backends land here, e.g.:
//
// #[cfg(target_arch = "wasm32")]
// pub type ActiveRenderer = crate::platform::wasm::WasmRenderer;
// #[cfg(target_arch = "wasm32")]
// pub type ActiveAudio = crate::platform::wasm::WasmAudio;
// #[cfg(target_arch = "wasm32")]
// pub type ActiveInput = crate::platform::wasm::WasmInput;
// #[cfg(target_arch = "wasm32")]
// pub type ActiveFiles = crate::platform::wasm::WasmFiles;
