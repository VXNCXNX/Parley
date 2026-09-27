fn main() {
    if let Some(dir) = std::env::var_os("DEP_TRANSCRIBE_CPP_RUNTIME_DIR") {
        println!("cargo:rustc-env=PARLEY_TRANSCRIBE_RUNTIME_DIR={}", std::path::Path::new(&dir).display());
    }
    if let Some(dir) = std::env::var_os("DEP_TRANSCRIBE_CPP_MODULE_DIR") {
        println!("cargo:rustc-env=PARLEY_TRANSCRIBE_MODULE_DIR={}", std::path::Path::new(&dir).display());
    }
}
