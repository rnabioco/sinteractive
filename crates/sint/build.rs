//! Builds the status plugin into the `sinteractive` binary.
//!
//! `crates/sint-zellij` is compiled for `wasm32-wasip1` by a nested cargo
//! into `OUT_DIR/plugin-target` (a separate target dir, so it never contends
//! with the outer build's lock) and embedded with `include_bytes!`.
//! `SINT_PLUGIN_WASM=/path` skips the nested build and embeds that file;
//! `SINT_SKIP_BUNDLE=1` embeds an empty placeholder for quick local builds.
//!
//! zellij itself is compiled in as a library (see `src/zellij_embed/`), so
//! there is nothing else to fetch.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=SINT_PLUGIN_WASM");
    println!("cargo:rerun-if-env-changed=SINT_SKIP_BUNDLE");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../sint-zellij/src");
    println!("cargo:rerun-if-changed=../sint-zellij/Cargo.toml");
    println!("cargo:rerun-if-changed=../sint-proto/src");
    println!("cargo:rerun-if-changed=../../assets/zellij");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let wasm = out.join("sint-zellij.wasm");
    if env::var_os("SINT_SKIP_BUNDLE").is_some() {
        fs::write(&wasm, b"").unwrap();
        println!("cargo:warning=SINT_SKIP_BUNDLE set: the status plugin is not embedded");
        return;
    }
    build_plugin(&wasm);
}

fn build_plugin(dest: &Path) {
    if let Some(p) = env::var_os("SINT_PLUGIN_WASM") {
        fs::copy(&p, dest).expect("copy SINT_PLUGIN_WASM");
        return;
    }
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let plugin_manifest = manifest_dir.join("../sint-zellij/Cargo.toml");
    let target_dir = dest.parent().unwrap().join("plugin-target");
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-wasip1",
            "--manifest-path",
        ])
        .arg(&plugin_manifest)
        .arg("--target-dir")
        .arg(&target_dir)
        // The nested build must not inherit the outer job's flags/target.
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .expect("nested cargo for the wasm plugin");
    assert!(
        status.success(),
        "building sint-zellij for wasm32-wasip1 failed"
    );
    let built = target_dir.join("wasm32-wasip1/release/sint-zellij.wasm");
    fs::copy(&built, dest).expect("copy built plugin");
}
