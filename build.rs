use std::env;
use std::path::PathBuf;

fn main() {
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
    let sources = [
        "src/data.c",
        "src/seg000.c",
        "src/seg001.c",
        "src/seg002.c",
        "src/seg003.c",
        "src/seg004.c",
        "src/seg005.c",
        "src/seg006.c",
        "src/seg007.c",
        "src/seg008.c",
        "src/seg009.c",
        "src/seqtbl.c",
        "src/replay.c",
        "src/options.c",
        "src/lighting.c",
        "src/screenshot.c",
        "src/menu.c",
        "src/midi.c",
        "src/opl3.c",
        "src/stb_vorbis.c",
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
