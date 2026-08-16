//! `cargo xtask harness` / `smoke-test` / `gameplay-smoke-test` / `quicksave-fixture` / the
//! harness portion of `verify` -- thin wrappers around the existing scripts under `scripts/`.
//!
//! Not ported to Rust: `scripts/run_harness.sh`'s replay-diffing orchestration and
//! `scripts/compare_traces.py`'s binary trace-format parser (400+ lines of Python with a
//! custom binary format) are real, nontrivial pieces of logic, not simple shell glue --
//! left as an explicit future follow-up rather than done here.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::run_status;

fn script(root: &Path, name: &str) -> PathBuf {
    root.join("scripts").join(name)
}

/// The full default harness suite: smoke test + gameplay smoke test + all replay/golden-trace
/// comparisons (`scripts/run_harness.sh` with no arguments already does all three internally).
/// Shared by `cargo xtask harness` (no subcommand) and `cargo xtask verify`.
pub fn run_full(root: &Path) -> Result<(), String> {
    run_status(
        Command::new(script(root, "run_harness.sh")).current_dir(root),
        "scripts/run_harness.sh",
    )
}

pub fn regen(root: &Path) -> Result<(), String> {
    run_status(
        Command::new(script(root, "run_harness.sh")).arg("--regen").current_dir(root),
        "scripts/run_harness.sh --regen",
    )
}

pub fn compare(root: &Path, a: &Path, b: &Path, extra: &[String]) -> Result<(), String> {
    run_status(
        Command::new(script(root, "run_harness.sh"))
            .arg("--compare")
            .arg(a)
            .arg(b)
            .args(extra)
            .current_dir(root),
        "scripts/run_harness.sh --compare",
    )
}

pub fn one(root: &Path, replay: &Path, golden: &Path) -> Result<(), String> {
    run_status(
        Command::new(script(root, "run_harness.sh"))
            .arg("--one")
            .arg(replay)
            .arg(golden)
            .current_dir(root),
        "scripts/run_harness.sh --one",
    )
}

pub fn build_binary(root: &Path) -> Result<(), String> {
    run_status(
        Command::new(script(root, "run_harness.sh")).arg("--build").current_dir(root),
        "scripts/run_harness.sh --build",
    )
}

pub fn smoke_test(root: &Path, duration_seconds: u32) -> Result<(), String> {
    run_status(
        Command::new(script(root, "smoke_test.sh"))
            .arg(duration_seconds.to_string())
            .current_dir(root),
        "scripts/smoke_test.sh",
    )
}

pub fn gameplay_smoke_test(root: &Path) -> Result<(), String> {
    run_status(
        Command::new(script(root, "gameplay_smoke_test.sh")).current_dir(root),
        "scripts/gameplay_smoke_test.sh",
    )
}

pub fn menu_smoke_test(root: &Path) -> Result<(), String> {
    run_status(
        Command::new(script(root, "menu_smoke_test.sh")).current_dir(root),
        "scripts/menu_smoke_test.sh",
    )
}

pub fn wasm_menu_smoke_test(root: &Path) -> Result<(), String> {
    run_status(
        Command::new("node")
            .arg(script(root, "wasm_menu_smoke_test.mjs"))
            .current_dir(root),
        "scripts/wasm_menu_smoke_test.mjs",
    )
}

pub fn menu_mouse_navigation_test(root: &Path) -> Result<(), String> {
    run_status(
        Command::new(script(root, "menu_mouse_navigation_test.sh")).current_dir(root),
        "scripts/menu_mouse_navigation_test.sh",
    )
}

pub fn wasm_menu_mouse_navigation_test(root: &Path) -> Result<(), String> {
    run_status(
        Command::new("node")
            .arg(script(root, "wasm_menu_mouse_navigation_test.mjs"))
            .current_dir(root),
        "scripts/wasm_menu_mouse_navigation_test.mjs",
    )
}

/// Confirms `node` and the Playwright package are actually available before trying to run
/// anything that needs them, with an error message that names the exact fix -- the failure
/// mode without this check is a much less legible `Error: Cannot find module 'playwright'`
/// buried inside a Node stack trace.
fn check_wasm_test_deps(root: &Path) -> Result<(), String> {
    if Command::new("node").arg("--version").output().is_err() {
        return Err(
            "wasm tests require Node.js, which was not found on PATH. Install Node, then run \
             `npm install` in the project root.".to_string(),
        );
    }
    // stdout/stderr suppressed: on failure this prints a raw "Cannot find module" stack
    // trace, which would just be noise ahead of the actionable message below.
    let has_playwright = Command::new("node")
        .arg("-e")
        .arg("require.resolve('playwright')")
        .current_dir(root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !has_playwright {
        return Err(
            "wasm tests require the Playwright npm package, which isn't installed. Run \
             `npm install` in the project root.".to_string(),
        );
    }
    Ok(())
}

/// `cargo xtask wasm-verify`: dependency check, then a fresh wasm build (same as `wasm-build`
/// -- the wasm tests need current `web/pkg/`, not whatever was last built), then the wasm
/// test suite (`wasm_menu_smoke_test`, `wasm_menu_mouse_navigation_test`); add further
/// wasm-only regression tests here as they're built (see
/// docs/plans/13-platform-architecture-unification.md's Phase D).
/// Deliberately does NOT include the full native-vs-wasm pixel-hash sweep
/// (`scripts/wasm_pixel_harness.mjs` run across all golden replays) -- that takes minutes
/// (a headless Chromium launch + full asset preload per replay), appropriate for an
/// occasional deep check but not every `cargo xtask verify` run.
pub fn wasm_verify(root: &Path) -> Result<(), String> {
    check_wasm_test_deps(root)?;
    crate::wasm::build(root)?;
    wasm_menu_smoke_test(root)?;
    wasm_menu_mouse_navigation_test(root)
}

/// `cargo xtask live-diff`: run each live-surface scenario through both the C oracle and the
/// Rust build and diff state + pixels (`scripts/live_surface_diff.sh`).
///
/// Deliberately **skips rather than fails** when the C oracle binary is absent. `verify`
/// never builds the oracle -- it compares against already-committed golden traces -- so on a
/// working tree where nobody has run cmake, a hard failure here would just be noise telling
/// developers to build something they don't otherwise need. CI builds the oracle explicitly
/// and so gets the real signal.
///
/// Menu-resident scenarios are excluded on purpose: draw_menu's inner loop blocks the tick
/// loop so no trace is ever written. `menu_mouse_navigation_test` covers those instead.
pub fn live_diff(root: &Path) -> Result<(), String> {
    if !root.join("prince").is_file() {
        println!(
            "SKIP: live-diff needs the C oracle at {}/prince, which isn't built.\n      \
             Build it with: mkdir -p c/build && cd c/build && cmake -G Ninja .. && ninja",
            root.display()
        );
        return Ok(());
    }
    let scenarios = [
        "quickload.txt",
        "walk_right_then_stop.txt",
        "walk_right.txt",
        // Live-input-only paths: no recorded .p1r can contain a Ctrl+A or an F6/F9, so these
        // are unreachable by the replay harness no matter how many replays get added.
        "restart_level.txt",
        "save_then_load.txt",
    ];
    for scenario in scenarios {
        let path = root.join("scripts").join("scripted_inputs").join(scenario);
        run_status(
            Command::new(script(root, "live_surface_diff.sh"))
                .arg(&path)
                .current_dir(root),
            &format!("scripts/live_surface_diff.sh {scenario}"),
        )?;
    }
    Ok(())
}

pub fn quicksave_fixture(root: &Path) -> Result<(), String> {
    run_status(
        Command::new(script(root, "gen_quicksave_fixture.sh")).current_dir(root),
        "scripts/gen_quicksave_fixture.sh",
    )
}
