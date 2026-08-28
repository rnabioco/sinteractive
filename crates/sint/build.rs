//! Bundles zellij and the status plugin into the `sinteractive` binary.
//!
//! - zellij: the pinned official `x86_64-unknown-linux-musl` tarball is
//!   downloaded into `OUT_DIR` (sha256 verified) and embedded as-is
//!   (`include_bytes!`); the binary extracts it on first use. Set
//!   `ZELLIJ_TARBALL=/path/to/zellij-x86_64-unknown-linux-musl.tar.gz` to
//!   build offline (still verified).
//! - plugin: `crates/sint-zellij` is built for `wasm32-wasip1` with a nested
//!   cargo into `OUT_DIR/plugin-target` (a separate target dir, so it never
//!   contends with the outer build's lock). `SINT_PLUGIN_WASM=/path` skips
//!   that and embeds the given file.
//! - `SINT_SKIP_BUNDLE=1` embeds empty placeholders for fast local iteration;
//!   the binary then requires `SINTERACTIVE_ZELLIJ`.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

const ZELLIJ_VERSION: &str = "0.45.1";
const ZELLIJ_SHA256: &str = "40bcc2e03f5d5ae8e054e39f676081fe12ab70871506996ba595834c3718eefc";
const ZELLIJ_ASSET: &str = "zellij-x86_64-unknown-linux-musl.tar.gz";

fn main() {
    println!("cargo:rerun-if-env-changed=ZELLIJ_TARBALL");
    println!("cargo:rerun-if-env-changed=SINT_PLUGIN_WASM");
    println!("cargo:rerun-if-env-changed=SINT_SKIP_BUNDLE");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../sint-zellij/src");
    println!("cargo:rerun-if-changed=../sint-zellij/Cargo.toml");
    println!("cargo:rerun-if-changed=../sint-proto/src");
    println!("cargo:rerun-if-changed=../../assets/zellij");

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let tarball = out.join("zellij.tar.gz");
    let wasm = out.join("sint-zellij.wasm");
    println!("cargo:rustc-env=SINT_ZELLIJ_VERSION={ZELLIJ_VERSION}");
    println!("cargo:rustc-env=SINT_ZELLIJ_SHA256={ZELLIJ_SHA256}");

    if env::var_os("SINT_SKIP_BUNDLE").is_some() {
        fs::write(&tarball, b"").unwrap();
        fs::write(&wasm, b"").unwrap();
        println!("cargo:warning=SINT_SKIP_BUNDLE set: zellij and the plugin are not embedded");
        return;
    }

    fetch_zellij(&tarball);
    build_plugin(&wasm);
}

fn fetch_zellij(dest: &Path) {
    if dest.exists() && sha256_file(dest) == ZELLIJ_SHA256 {
        return;
    }
    let src: PathBuf = match env::var_os("ZELLIJ_TARBALL") {
        Some(p) => PathBuf::from(p),
        None => {
            let url = format!(
                "https://github.com/zellij-org/zellij/releases/download/v{ZELLIJ_VERSION}/{ZELLIJ_ASSET}"
            );
            let tmp = dest.with_extension("part");
            let status = Command::new("curl")
                .args(["-fsSL", "--retry", "3", "-o"])
                .arg(&tmp)
                .arg(&url)
                .status()
                .expect("curl is required to fetch the zellij tarball (or set ZELLIJ_TARBALL)");
            assert!(status.success(), "downloading {url} failed");
            tmp
        }
    };
    let got = sha256_file(&src);
    assert_eq!(
        got,
        ZELLIJ_SHA256,
        "zellij tarball {} has sha256 {got}, expected {ZELLIJ_SHA256}",
        src.display()
    );
    if src != dest {
        fs::copy(&src, dest).expect("copy zellij tarball into OUT_DIR");
        if src.extension().is_some_and(|e| e == "part") {
            let _ = fs::remove_file(&src);
        }
    }
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
    let built = target_dir.join("wasm32-wasip1/release/sint_zellij.wasm");
    fs::copy(&built, dest).expect("copy built plugin");
}

fn sha256_file(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut f = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let mut h = Sha256::new();
    let mut buf = [0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    format!("{:x}", h.finalize())
}
