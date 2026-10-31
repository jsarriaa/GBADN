use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=shim/shim.c");
    println!("cargo:rerun-if-env-changed=GBADN_SKIP_MGBA_BUILD");

    // Lets `cargo check` and linters run without compiling the native core.
    if env::var_os("GBADN_SKIP_MGBA_BUILD").is_some() {
        return;
    }

    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mgba = root.join("../../vendor/mgba");
    assert!(
        mgba.join("CMakeLists.txt").exists(),
        "mGBA is missing, run: git submodule update --init"
    );

    let out = cmake::Config::new(&mgba)
        .define("LIBMGBA_ONLY", "ON")
        .define("M_CORE_GBA", "ON")
        .define("M_CORE_GB", "OFF")
        .build_target("mgba")
        .build();
    let build = out.join("build");

    cc::Build::new()
        .file("shim/shim.c")
        .include(mgba.join("include"))
        .include(build.join("include"))
        .define("BUILD_STATIC", None)
        .compile("gbadn_shim");

    println!("cargo:rustc-link-search=native={}", build.display());
    println!("cargo:rustc-link-lib=static=mgba");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rustc-link-lib=pthread");
}
