//! `cargo xtask wasm-build` / `cargo xtask wasm-serve` -- full Rust port of the former
//! `scripts/build_wasm.sh` / `scripts/serve_wasm.sh`. Serving still shells out to Python's
//! `http.server` for the actual HTTP handling (the one piece not ported to pure Rust yet --
//! see the follow-up note in docs/plans/13-platform-architecture-unification.md); everything
//! else (the wasm-bindgen version check, the build itself, the asset manifest, the symlinks)
//! is real Rust.

use std::path::Path;
use std::process::Command;

use crate::run_status;

/// `cargo xtask wasm-build`: always regenerates web/pkg/, regardless of staleness -- this is
/// the "I want it rebuilt" command.
pub fn build(root: &Path) -> Result<(), String> {
    ensure_wasm_build(root, true)?;
    println!("Built web/pkg/. Run `cargo xtask wasm-serve` to try it in a browser.");
    Ok(())
}

/// `cargo xtask wasm-serve`: rebuilds only if the wasm bundle is stale (or missing), then
/// serves web/ with the COOP/COEP headers SharedArrayBuffer requires -- plain
/// `python3 -m http.server` can't set custom headers, hence the inline server script below.
pub fn serve(root: &Path, port: u16) -> Result<(), String> {
    ensure_wasm_build(root, false)?;
    let web_dir = root.join("web");
    let script = format!(
        r#"
import http.server

class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()

http.server.test(HandlerClass=Handler, port={port})
"#
    );
    println!("Serving web/ on http://localhost:{port}/ (Ctrl+C to stop)...");
    run_status(
        Command::new("python3").arg("-c").arg(&script).current_dir(&web_dir),
        "python3 http.server",
    )
}

/// Runs `cargo build --target wasm32-unknown-unknown` (cheap/no-op if nothing changed --
/// reuses cargo's own incremental-build staleness detection rather than reinventing it), then
/// -- if `force` or the wasm-bindgen-generated bundle is missing/older than the freshly-built
/// `.wasm` -- regenerates `web/pkg/` via `wasm-bindgen`, the `web/data`/`web/SDLPoP.ini`
/// symlinks, and `web/data_manifest.txt`.
fn ensure_wasm_build(root: &Path, force: bool) -> Result<(), String> {
    run_status(
        Command::new("cargo")
            .args(["build", "--target", "wasm32-unknown-unknown"])
            .current_dir(root),
        "cargo build --target wasm32-unknown-unknown",
    )?;

    let wasm_artifact = root.join("target/wasm32-unknown-unknown/debug/prince.wasm");
    let pkg_wasm = root.join("web/pkg/sdlpop_bg.wasm");

    let stale = force || !pkg_wasm.exists() || is_older(&pkg_wasm, &wasm_artifact)?;
    if !stale {
        println!("web/pkg/ is up to date.");
        return Ok(());
    }

    check_wasm_bindgen_version(root)?;

    run_status(
        Command::new("wasm-bindgen")
            .args(["--target", "web", "--out-dir", "web/pkg", "--out-name", "sdlpop"])
            .arg(&wasm_artifact)
            .current_dir(root),
        "wasm-bindgen",
    )?;

    symlink_force(&root.join("data"), &root.join("web/data"))?;
    symlink_force(&root.join("SDLPoP.ini"), &root.join("web/SDLPoP.ini"))?;
    write_manifest(root)?;

    Ok(())
}

fn is_older(a: &Path, b: &Path) -> Result<bool, String> {
    let a_mtime = std::fs::metadata(a)
        .and_then(|m| m.modified())
        .map_err(|e| format!("reading mtime of {}: {e}", a.display()))?;
    let b_mtime = std::fs::metadata(b)
        .and_then(|m| m.modified())
        .map_err(|e| format!("reading mtime of {}: {e}", b.display()))?;
    Ok(a_mtime < b_mtime)
}

/// Confirms the installed `wasm-bindgen` CLI matches the version pinned in `Cargo.lock` --
/// the JS glue it generates must match the `wasm-bindgen` crate version compiled into the
/// `.wasm`, or the two silently disagree on the ABI between them.
fn check_wasm_bindgen_version(root: &Path) -> Result<(), String> {
    let pinned = pinned_wasm_bindgen_version(root)?;
    let installed = installed_wasm_bindgen_version();
    if installed.as_deref() != Some(pinned.as_str()) {
        let installed_desc = installed.as_deref().unwrap_or("(not found)");
        return Err(format!(
            "wasm-bindgen CLI version ({installed_desc}) doesn't match Cargo.lock ({pinned}).\n\
             Run: cargo install wasm-bindgen-cli --version {pinned} --locked"
        ));
    }
    Ok(())
}

fn pinned_wasm_bindgen_version(root: &Path) -> Result<String, String> {
    let lock = std::fs::read_to_string(root.join("Cargo.lock"))
        .map_err(|e| format!("reading Cargo.lock: {e}"))?;
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "name = \"wasm-bindgen\"" {
            if let Some(version_line) = lines.next() {
                if let Some(v) = version_line
                    .trim()
                    .strip_prefix("version = \"")
                    .and_then(|v| v.strip_suffix('"'))
                {
                    return Ok(v.to_string());
                }
            }
        }
    }
    Err("could not find a wasm-bindgen entry in Cargo.lock".to_string())
}

fn installed_wasm_bindgen_version() -> Option<String> {
    let output = Command::new("wasm-bindgen").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .map(|s| s.to_string())
}

#[cfg(unix)]
fn symlink_force(target: &Path, link: &Path) -> Result<(), String> {
    if link.symlink_metadata().is_ok() {
        std::fs::remove_file(link)
            .map_err(|e| format!("removing existing {}: {e}", link.display()))?;
    }
    std::os::unix::fs::symlink(target, link)
        .map_err(|e| format!("symlinking {} -> {}: {e}", link.display(), target.display()))
}

/// Regenerates `web/data_manifest.txt`: every file actually on disk under `data/` (not just
/// what's tracked by git -- `data/music/*.ogg` is gitignored but still needed at runtime if
/// present; a plain recursive walk naturally includes it, matching the old `fdfind
/// --no-ignore` behavior with no special-casing needed), plus `SDLPoP.ini`.
fn write_manifest(root: &Path) -> Result<(), String> {
    let mut paths = Vec::new();
    walk_files(&root.join("data"), root, &mut paths)?;
    paths.sort();

    let mut content = paths.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    content.push_str("SDLPoP.ini\n");

    std::fs::write(root.join("web/data_manifest.txt"), content)
        .map_err(|e| format!("writing web/data_manifest.txt: {e}"))
}

fn walk_files(dir: &Path, root: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("reading a dir entry under {}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, root, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| format!("{} is not under {}: {e}", path.display(), root.display()))?;
            // Forward slashes regardless of host platform -- the manifest is consumed by a
            // browser's fetch(), not the local filesystem.
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}
