use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let runtime_dir = manifest_dir.join("../doria-rt");
    let runtime_source_dir = runtime_dir.join("src");
    let runtime_source = runtime_source_dir.join("lib.rs");
    let runtime_manifest = runtime_dir.join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", runtime_source_dir.display());
    println!("cargo:rerun-if-changed={}", runtime_manifest.display());

    let target = env::var("TARGET").expect("Cargo target triple");
    let filename = if target.ends_with("windows-msvc") {
        "doria_rt.lib"
    } else {
        "libdoria_rt.a"
    };
    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory")).join(filename);
    let dependency_dir = output
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .expect("Cargo build directory")
        .join("deps");
    let ryu = std::fs::read_dir(&dependency_dir)
        .expect("Cargo dependency directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("libryu-") && name.ends_with(".rlib"))
        })
        .expect("compiled ryu dependency");
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let mut command = Command::new(rustc);
    command
        .arg("--crate-name")
        .arg("doria_rt")
        .arg("--edition=2021")
        .arg("--crate-type=staticlib")
        .arg("--target")
        .arg(&target)
        .arg("-L")
        .arg(format!("dependency={}", dependency_dir.display()))
        .arg("--extern")
        .arg(format!("ryu={}", ryu.display()))
        .arg("-C")
        .arg(format!(
            "opt-level={}",
            env::var("OPT_LEVEL").unwrap_or_else(|_| "0".to_string())
        ));
    if let Some(encoded_flags) = env::var_os("CARGO_ENCODED_RUSTFLAGS") {
        for flag in encoded_flags.to_string_lossy().split('\u{1f}') {
            if !flag.is_empty() {
                command.arg(flag);
            }
        }
    }
    let status = command
        .arg("-C")
        .arg("panic=abort")
        .arg("-o")
        .arg(&output)
        .arg(&runtime_source)
        .status()
        .expect("failed to invoke rustc for doria-rt");
    assert!(status.success(), "failed to build doria-rt static library");

    println!("cargo:rustc-env=DORIA_RT_BUILT_PATH={}", output.display());
}
