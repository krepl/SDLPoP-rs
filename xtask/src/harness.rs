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

pub fn quicksave_fixture(root: &Path) -> Result<(), String> {
    run_status(
        Command::new(script(root, "gen_quicksave_fixture.sh")).current_dir(root),
        "scripts/gen_quicksave_fixture.sh",
    )
}
