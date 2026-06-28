use std::env;
use std::path::PathBuf;

fn main() {
    // Only re-run this script when C sources or headers change, not on every Rust edit.
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=rust/src/seqtbl.rs");
    println!("cargo:rerun-if-changed=rust/src/seg004.rs");
    println!("cargo:rerun-if-changed=rust/src/seg005.rs");
    println!("cargo:rerun-if-changed=rust/src/seg006.rs");
    println!("cargo:rerun-if-changed=rust/src/seg007.rs");
    println!("cargo:rerun-if-changed=rust/src/seg003.rs");
    println!("cargo:rerun-if-changed=rust/src/seg002.rs");
    println!("cargo:rerun-if-changed=rust/src/seg001.rs");
    println!("cargo:rerun-if-changed=rust/src/seg008.rs");
    println!("cargo:rerun-if-changed=rust/src/seg000.rs");
    println!("cargo:rerun-if-changed=rust/src/seg009.rs");
    println!("cargo:rerun-if-changed=rust/src/sdl_rw_wrappers.rs");
    println!("cargo:rerun-if-changed=rust/src/lighting.rs");
    println!("cargo:rerun-if-changed=rust/src/state_dump.rs");
    println!("cargo:rerun-if-changed=rust/src/options.rs");
    println!("cargo:rerun-if-changed=rust/src/screenshot.rs");
    println!("cargo:rerun-if-changed=rust/src/replay.rs");
    println!("cargo:rerun-if-changed=rust/src/opl3.rs");
    println!("cargo:rerun-if-changed=rust/src/midi.rs");

    // Probe SDL2 (auto-emits cargo:rustc-link-* directives)
    let sdl2 = pkg_config::Config::new()
        .probe("sdl2")
        .expect("sdl2 not found via pkg-config; install libsdl2-dev");
    let sdl2_image = pkg_config::Config::new()
        .probe("SDL2_image")
        .expect("SDL2_image not found via pkg-config; install libsdl2-image-dev");

    let include_paths: Vec<PathBuf> = sdl2
        .include_paths
        .iter()
        .chain(sdl2_image.include_paths.iter())
        .cloned()
        .collect();

    // Compile all C sources except main.c (Rust provides main)
    // Ported to Rust: seg004
    let sources = [
        "src/data.c",
        // seg000.c ported to Rust
        // seg008.c ported to Rust
        // seg009.c ported to Rust
        // seqtbl.c ported to Rust
        // options.c ported to Rust
        // replay.c ported to Rust
        // sdl_rw_wrappers.c ported to Rust
        // lighting.c ported to Rust
        // screenshot.c ported to Rust
        "src/menu.c",
        "src/midi.c",
        // opl3.c ported to Rust
        // midi.c ported to Rust
        "src/opl3.c",
        "src/stb_vorbis.c",
        // state_dump.c ported to Rust
    ];

    let mut build = cc::Build::new();
    build
        .std("c99")
        .define("_GNU_SOURCE", "1")
        .flag("-O2")
        .flag("-w");

    for path in &include_paths {
        build.include(path);
    }
    for source in &sources {
        build.file(source);
    }
    build.compile("sdlpop");

    println!("cargo:rustc-link-lib=m");

    // Generate bindings from common.h
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut builder = bindgen::Builder::default()
        .header("src/common.h")
        .clang_arg("-std=c99")
        .clang_arg("-D_GNU_SOURCE=1")
        .allowlist_file(r".*src/.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    for path in &include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    builder
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings");
}
