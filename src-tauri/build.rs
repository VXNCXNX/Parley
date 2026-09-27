fn main() {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    build_apple_intelligence_bridge();

    generate_tray_translations();

    // Linux loads libtranscribe from the package. Windows loads DLLs from the
    // executable directory. macOS links the native runtime statically.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib/Parley:$ORIGIN/../lib");
    }
    stage_native_runtime();

    tauri_build::build()
}

fn stage_native_runtime() {
    use std::path::PathBuf;
    use std::process::Command;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let runtime_dir = manifest_dir.join("native-runtime");
    println!("cargo:rerun-if-changed={}", runtime_dir.join("src/lib.rs").display());
    println!("cargo:rerun-if-changed={}", runtime_dir.join("Cargo.toml").display());
    let profile = if std::env::var("PROFILE").as_deref() == Ok("release") { "release" } else { "release" };
    let status = Command::new("cargo")
        .args(["build", "--manifest-path"])
        .arg(runtime_dir.join("Cargo.toml"))
        .args(["--release", "--locked"])
        .status()
        .expect("build native runtime library");
    if !status.success() {
        panic!("native runtime library build failed");
    }
    let filename = if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        "parley_native_runtime.dll"
    } else if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        "libparley_native_runtime.dylib"
    } else {
        "libparley_native_runtime.so"
    };
    let source = runtime_dir.join("target").join(profile).join(filename);
    let dest_dir = manifest_dir.join("native-runtime-libs");
    std::fs::create_dir_all(&dest_dir).expect("create native runtime staging dir");
    std::fs::copy(&source, dest_dir.join(filename)).unwrap_or_else(|error| panic!("copy {}: {error}", source.display()));
    println!("cargo:rustc-env=PARLEY_NATIVE_RUNTIME_PATH={}", source.display());
}

