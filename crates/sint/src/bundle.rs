//! The embedded zellij + plugin bundle and its on-disk extraction.
//!
//! Layout under `<cache>/bin/<sha12>/` (the cache dir is on the shared
//! filesystem, so one extraction serves every node):
//!
//! ```text
//! zellij                  the static binary
//! sint-zellij.wasm        the status plugin
//! config.kdl              assets/zellij/config.kdl with paths substituted
//! layouts/sint.kdl        assets/zellij/layouts/sint.kdl, likewise
//! .complete               marker written last; presence = extraction done
//! ```
//!
//! Extraction is write-to-temp-dir-then-rename, so concurrent first runs on
//! different nodes cannot see a half-written tree. `SINTERACTIVE_ZELLIJ`
//! bypasses the embedded zellij but the plugin and config are still laid out.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use sint_core::config::Config;

static ZELLIJ_TARBALL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/zellij.tar.gz"));
static PLUGIN_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sint-zellij.wasm"));
static CONFIG_KDL: &str = include_str!("../../../assets/zellij/config.kdl");
static LAYOUT_KDL: &str = include_str!("../../../assets/zellij/layouts/sint.kdl");

pub const ZELLIJ_VERSION: &str = env!("SINT_ZELLIJ_VERSION");

/// Paths of an extracted bundle.
#[derive(Debug, Clone)]
pub struct Bundle {
    pub dir: PathBuf,
    pub zellij: PathBuf,
    pub plugin: PathBuf,
    pub config: PathBuf,
    pub layouts: PathBuf,
}

/// Short id of this build's bundle: the first 12 hex chars of the sha256 of
/// (zellij tarball sha, plugin bytes, config text, layout text, mouse flag).
fn bundle_id(mouse: bool) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(env!("SINT_ZELLIJ_SHA256").as_bytes());
    h.update(PLUGIN_WASM);
    h.update(CONFIG_KDL.as_bytes());
    h.update(LAYOUT_KDL.as_bytes());
    h.update(if mouse { "mouse" } else { "nomouse" }.as_bytes());
    let hex = format!("{:x}", h.finalize());
    hex[..12].to_string()
}

/// Whether this binary carries an embedded zellij (built without
/// `SINT_SKIP_BUNDLE`).
pub fn has_embedded_zellij() -> bool {
    !ZELLIJ_TARBALL.is_empty()
}

/// Ensure the bundle is extracted for `cfg` and return its paths.
pub fn ensure(cfg: &Config, mouse: bool) -> Result<Bundle> {
    let dir = cfg.cache_dir.join("bin").join(bundle_id(mouse));
    let bundle = Bundle {
        zellij: match &cfg.zellij {
            Some(p) => p.clone(),
            None => dir.join("zellij"),
        },
        plugin: dir.join("sint-zellij.wasm"),
        config: dir.join("config.kdl"),
        layouts: dir.join("layouts"),
        dir: dir.clone(),
    };
    if dir.join(".complete").exists() {
        return Ok(bundle);
    }
    if cfg.zellij.is_none() && !has_embedded_zellij() {
        return Err(anyhow!(
            "this build has no embedded zellij (SINT_SKIP_BUNDLE); set SINTERACTIVE_ZELLIJ"
        ));
    }
    fs::create_dir_all(dir.parent().unwrap())?;
    let tmp = tempfile::Builder::new()
        .prefix(".extract-")
        .tempdir_in(dir.parent().unwrap())
        .context("create extraction dir")?;
    let t = tmp.path();
    if cfg.zellij.is_none() {
        extract_zellij(t.join("zellij").as_path())?;
    }
    fs::write(t.join("sint-zellij.wasm"), PLUGIN_WASM)?;
    fs::create_dir_all(t.join("layouts"))?;
    let plugin_path = dir.join("sint-zellij.wasm");
    let layouts_path = dir.join("layouts");
    let sub = |s: &str| {
        s.replace("__PLUGIN__", &plugin_path.to_string_lossy())
            .replace("__LAYOUTS__", &layouts_path.to_string_lossy())
            .replace("__MOUSE__", if mouse { "true" } else { "false" })
    };
    fs::write(t.join("config.kdl"), sub(CONFIG_KDL))?;
    fs::write(t.join("layouts/sint.kdl"), sub(LAYOUT_KDL))?;
    fs::write(t.join(".complete"), ZELLIJ_VERSION)?;
    match fs::rename(t, &dir) {
        Ok(()) => {
            std::mem::forget(tmp);
        }
        Err(e) if dir.join(".complete").exists() => {
            // Another process won the race; ours is discarded by `tmp`.
            let _ = e;
        }
        Err(e) => return Err(e).context("install extracted bundle"),
    }
    Ok(bundle)
}

fn extract_zellij(dest: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let gz = flate2::read::GzDecoder::new(ZELLIJ_TARBALL);
    let mut ar = tar::Archive::new(gz);
    for entry in ar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        if path.file_name().is_some_and(|n| n == "zellij") {
            let mut buf = Vec::with_capacity(60 << 20);
            entry.read_to_end(&mut buf)?;
            fs::write(dest, &buf)?;
            fs::set_permissions(dest, fs::Permissions::from_mode(0o755))?;
            return Ok(());
        }
    }
    Err(anyhow!("embedded zellij tarball has no `zellij` entry"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_and_layout_placeholders_are_substituted() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::defaults();
        cfg.cache_dir = dir.path().to_path_buf();
        // Use a fake zellij so the test does not depend on the embedded tarball.
        let fake = dir.path().join("fake-zellij");
        fs::write(&fake, "#!/bin/sh\n").unwrap();
        cfg.zellij = Some(fake.clone());
        let b = ensure(&cfg, true).unwrap();
        assert_eq!(b.zellij, fake);
        let conf = fs::read_to_string(&b.config).unwrap();
        assert!(!conf.contains("__PLUGIN__") && !conf.contains("__LAYOUTS__"));
        assert!(conf.contains("mouse_mode true"));
        assert!(conf.contains(&format!("layout_dir \"{}\"", b.layouts.display())));
        let lay = fs::read_to_string(b.layouts.join("sint.kdl")).unwrap();
        assert!(lay.contains(&format!("file:{}", b.plugin.display())));
        assert!(b.dir.join(".complete").exists());
        // Second call is a no-op returning the same paths.
        let b2 = ensure(&cfg, true).unwrap();
        assert_eq!(b2.dir, b.dir);
        // Mouse off is a different bundle id.
        let b3 = ensure(&cfg, false).unwrap();
        assert_ne!(b3.dir, b.dir);
        assert!(fs::read_to_string(&b3.config)
            .unwrap()
            .contains("mouse_mode false"));
    }
}
