fn main() {
    generate_tray_translations();

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
    let target_dir = runtime_dir.join("target");
    let target = std::env::var("TARGET").expect("Cargo target triple");
    let host = std::env::var("HOST").expect("Cargo host triple");
    println!(
        "cargo:rerun-if-changed={}",
        runtime_dir.join("src/lib.rs").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        runtime_dir.join("Cargo.toml").display()
    );
    let mut command = Command::new("cargo");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // Rust's strip pass can produce a LINKEDIT string table that macOS 27 refuses to load.
        command.env("CARGO_PROFILE_RELEASE_STRIP", "none");
    }
    command
        .env("CARGO_TARGET_DIR", &target_dir)
        .args(["build", "--manifest-path"])
        .arg(runtime_dir.join("Cargo.toml"))
        .args(["--release", "--locked"]);
    if target != host {
        command.args(["--target", &target]);
    }
    let status = command.status().expect("build native runtime library");
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
    let source_dir = if target == host {
        target_dir.join("release")
    } else {
        target_dir.join(&target).join("release")
    };
    let source = source_dir.join(filename);
    let dest_dir = manifest_dir.join("native-runtime-libs");
    std::fs::create_dir_all(&dest_dir).expect("create native runtime staging dir");
    let staged = dest_dir.join(filename);
    std::fs::copy(&source, &staged)
        .unwrap_or_else(|error| panic!("copy {}: {error}", source.display()));
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let install_name = format!("@rpath/{filename}");
        let _ = std::process::Command::new("install_name_tool")
            .args(["-id", &install_name])
            .arg(&staged)
            .status();
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }
}

/// Generate tray menu translations from frontend locale files.
///
/// Source of truth: src/i18n/locales/*/translation.json
/// The English "tray" section defines the struct fields.
fn generate_tray_translations() {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let locales_dir = Path::new("../src/i18n/locales");

    println!("cargo:rerun-if-changed=../src/i18n/locales");

    // Collect all locale translations
    let mut translations: BTreeMap<String, serde_json::Value> = BTreeMap::new();

    for entry in fs::read_dir(locales_dir).unwrap().flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let lang = path.file_name().unwrap().to_str().unwrap().to_string();
        let json_path = path.join("translation.json");

        println!("cargo:rerun-if-changed={}", json_path.display());

        let content = fs::read_to_string(&json_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        if let Some(tray) = parsed.get("tray").cloned() {
            translations.insert(lang, tray);
        }
    }

    // English defines the schema
    let english = translations.get("en").unwrap().as_object().unwrap();
    let fields: Vec<_> = english
        .keys()
        .map(|k| (camel_to_snake(k), k.clone()))
        .collect();

    // Generate code
    let mut out = String::from(
        "// Auto-generated from src/i18n/locales/*/translation.json - do not edit\n\n",
    );

    // Struct
    out.push_str("#[derive(Debug, Clone)]\npub struct TrayStrings {\n");
    for (rust_field, _) in &fields {
        out.push_str(&format!("    pub {rust_field}: String,\n"));
    }
    out.push_str("}\n\n");

    // Static map
    out.push_str(
        "pub static TRANSLATIONS: Lazy<HashMap<&'static str, TrayStrings>> = Lazy::new(|| {\n",
    );
    out.push_str("    let mut m = HashMap::new();\n");

    for (lang, tray) in &translations {
        out.push_str(&format!("    m.insert(\"{lang}\", TrayStrings {{\n"));
        for (rust_field, json_key) in &fields {
            let val = tray.get(json_key).and_then(|v| v.as_str()).unwrap_or("");
            out.push_str(&format!(
                "        {rust_field}: \"{}\".to_string(),\n",
                escape_string(val)
            ));
        }
        out.push_str("    });\n");
    }

    out.push_str("    m\n});\n");

    fs::write(Path::new(&out_dir).join("tray_translations.rs"), out).unwrap();

    println!(
        "cargo:warning=Generated tray translations: {} languages, {} fields",
        translations.len(),
        fields.len()
    );
}

fn camel_to_snake(s: &str) -> String {
    s.chars()
        .enumerate()
        .fold(String::new(), |mut acc, (i, c)| {
            if c.is_uppercase() && i > 0 {
                acc.push('_');
            }
            acc.push(c.to_lowercase().next().unwrap());
            acc
        })
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
