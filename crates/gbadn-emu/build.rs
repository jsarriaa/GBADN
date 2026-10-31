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
        .profile("Release")
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
    // Visual Studio generators put libraries in a folder named after the profile.
    println!(
        "cargo:rustc-link-search=native={}",
        build.join("Release").display()
    );
    println!("cargo:rustc-link-lib=static=mgba");
    match env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
        "windows" => {
            println!("cargo:rustc-link-lib=ws2_32");
            println!("cargo:rustc-link-lib=shlwapi");
        }
        _ => {
            println!("cargo:rustc-link-lib=m");
            println!("cargo:rustc-link-lib=pthread");
        }
    }
}
