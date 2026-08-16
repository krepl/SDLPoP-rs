//! Project-local dev tooling, invoked as `cargo xtask <subcommand>` (see the alias in
//! `.cargo/config.toml`). Run `cargo xtask --help` for the full subcommand list, or
//! `cargo xtask <subcommand> --help` for one command's details.
//!
//! Some subcommands are real Rust implementations (`wasm-build`/`wasm-serve`); others are
//! thin wrappers around the existing scripts under `scripts/` (the differential harness and
//! its Python trace-format parser are real, nontrivial pieces of logic not worth re-deriving
//! here yet -- see the plan doc, docs/plans, for the explicit follow-up).

mod harness;
mod wasm;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "Project-local dev tooling for SDLPoP-rs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build the wasm32 target and regenerate web/pkg/ via wasm-bindgen (always rebuilds).
    WasmBuild,
    /// Serve web/ over HTTP with the COOP/COEP headers SharedArrayBuffer requires, rebuilding
    /// first only if the wasm bundle is stale.
    WasmServe {
        /// Port to listen on.
        #[arg(long, default_value_t = 8642)]
        port: u16,
    },
    /// Run the differential harness (smoke test + gameplay smoke test + menu smoke test + all
    /// replay/golden-trace comparisons). With no subcommand, runs the full default suite.
    Harness {
        #[command(subcommand)]
        command: Option<HarnessCommand>,
    },
    /// Launch the game via its normal interactive startup and confirm it runs briefly without
    /// crashing (does not exercise replay/state correctness -- see `harness`).
    SmokeTest {
        /// How long to let it run before considering it a pass, in seconds.
        #[arg(default_value_t = 5)]
        duration_seconds: u32,
    },
    /// Run scripted-input scenarios (walk right, walk-then-stop) and assert the Kid's position
    /// moved the way real keyboard input should drive it.
    GameplaySmokeTest,
    /// Open the pause menu via scripted input and confirm the native build doesn't crash.
    /// Already part of `harness`/`verify`; standalone mainly for fast iteration. See
    /// `wasm-menu-smoke-test` for the wasm counterpart, which is the side that actually
    /// caught the real Esc-menu crash bug (commit d20c68e).
    MenuSmokeTest,
    /// Open the pause menu in the wasm build via scripted input and confirm the Worker
    /// doesn't crash -- requires `npm install` (Playwright) and a current `cargo xtask
    /// wasm-build`. Part of `wasm-verify` (and so of `verify`); standalone mainly for fast
    /// iteration.
    WasmMenuSmokeTest,
    /// Navigate the pause menu with scripted mouse input (open, click Settings, back out,
    /// click Quit Game, click OK) and confirm it actually works -- not just "doesn't crash"
    /// (that's menu-smoke-test's job). Already part of `harness`/`verify`; standalone mainly
    /// for fast iteration. See `wasm-menu-mouse-navigation-test` for the wasm counterpart.
    MenuMouseNavigationTest,
    /// Same mouse-driven menu navigation as `menu-mouse-navigation-test`, in the wasm build
    /// -- requires `npm install` (Playwright) and a current `cargo xtask wasm-build`. Part of
    /// `wasm-verify` (and so of `verify`); standalone mainly for fast iteration.
    WasmMenuMouseNavigationTest,
    /// Check Node/Playwright are installed, rebuild the wasm bundle, then run the wasm test
    /// suite (`wasm-menu-smoke-test` and `wasm-menu-mouse-navigation-test`). Part of
    /// `verify`. Requires `npm install` -- fails with a clear message naming the fix if that
    /// hasn't been run.
    WasmVerify,
    /// Diff the *live* surface (scripted keyboard input, not recorded replays) between the C
    /// oracle and the Rust build: state trace plus pixel hashes. Covers what the replay
    /// harness structurally cannot -- save/load and the live input path. Requires the C
    /// oracle binary at the project root; skips with a clear message if it isn't built,
    /// since `verify` never builds it.
    LiveDiff,
    /// Compile the standalone C oracle binary and capture a fresh quicksave fixture.
    QuicksaveFixture,
    /// Run everything: cargo build, cargo test --lib, cargo check --target
    /// wasm32-unknown-unknown, the full harness suite, then wasm-verify (wasm test suite).
    /// The one command to run before considering a change done.
    Verify,
}

#[derive(Subcommand)]
enum HarnessCommand {
    /// Regenerate all golden traces from the C oracle binary.
    Regen,
    /// Diff two arbitrary trace files.
    Compare {
        a: PathBuf,
        b: PathBuf,
        /// Extra arguments passed through to compare_traces.py (e.g. --all, --tick N).
        #[arg(trailing_var_arg = true)]
        extra: Vec<String>,
    },
    /// Run a single replay against a single golden trace.
    One { replay: PathBuf, golden: PathBuf },
    /// Just build the Rust binary under test (cargo build).
    Build,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let root = project_root();

    let result = match cli.command {
        Command::WasmBuild => wasm::build(&root),
        Command::WasmServe { port } => wasm::serve(&root, port),
        Command::Harness { command: None } => harness::run_full(&root),
        Command::Harness { command: Some(HarnessCommand::Regen) } => harness::regen(&root),
        Command::Harness { command: Some(HarnessCommand::Compare { a, b, extra }) } => {
            harness::compare(&root, &a, &b, &extra)
        }
        Command::Harness { command: Some(HarnessCommand::One { replay, golden }) } => {
            harness::one(&root, &replay, &golden)
        }
        Command::Harness { command: Some(HarnessCommand::Build) } => harness::build_binary(&root),
        Command::SmokeTest { duration_seconds } => harness::smoke_test(&root, duration_seconds),
        Command::GameplaySmokeTest => harness::gameplay_smoke_test(&root),
        Command::MenuSmokeTest => harness::menu_smoke_test(&root),
        Command::WasmMenuSmokeTest => harness::wasm_menu_smoke_test(&root),
        Command::MenuMouseNavigationTest => harness::menu_mouse_navigation_test(&root),
        Command::WasmMenuMouseNavigationTest => harness::wasm_menu_mouse_navigation_test(&root),
        Command::WasmVerify => harness::wasm_verify(&root),
        Command::LiveDiff => harness::live_diff(&root),
        Command::QuicksaveFixture => harness::quicksave_fixture(&root),
        Command::Verify => verify(&root),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask: {err}");
            ExitCode::FAILURE
        }
    }
}

/// The project root, one level above this crate's own directory (`xtask/`).
fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask crate has no parent directory")
        .to_path_buf()
}

fn verify(root: &Path) -> Result<(), String> {
    run_status(std::process::Command::new("cargo").arg("build").current_dir(root), "cargo build")?;
    run_status(
        std::process::Command::new("cargo").args(["test", "--lib"]).current_dir(root),
        "cargo test --lib",
    )?;
    run_status(
        std::process::Command::new("cargo")
            .args(["check", "--target", "wasm32-unknown-unknown"])
            .current_dir(root),
        "cargo check --target wasm32-unknown-unknown",
    )?;
    harness::run_full(root)?;
    // Skips itself when the C oracle isn't built (see harness::live_diff) -- `verify` has
    // never required a cmake build and shouldn't start now. CI builds the oracle, so it
    // gets the real signal there.
    harness::live_diff(root)?;
    harness::wasm_verify(root)
}

/// Runs a `Command`, mapping a nonzero exit or spawn failure into a plain `Err` with the
/// given label -- shared by every subcommand here so a failure partway through `verify`
/// (or any wrapper) reports which step failed instead of a bare exit code.
pub(crate) fn run_status(cmd: &mut std::process::Command, label: &str) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("failed to run {label}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed: {status}"))
    }
}
