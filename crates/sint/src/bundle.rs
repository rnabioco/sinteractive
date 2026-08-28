//! The embedded status plugin and zellij config, laid out on disk.
//!
//! zellij itself is compiled into this binary (`zellij_embed`), so the only
//! things that must exist as files are what zellij loads by path: the
//! plugin `.wasm`, `config.kdl`, and the layout. They go under
//! `<cache>/bin/<sha12>/` (the cache dir is on the shared filesystem, so one
//! extraction serves every node):
//!
//! ```text
//! sint-zellij.wasm        the status plugin
//! config.kdl              assets/zellij/config.kdl with paths substituted
//! layouts/sint.kdl        assets/zellij/layouts/sint.kdl, likewise
//! layouts/sint-panel.kdl  assets/zellij/layouts/sint-panel.kdl (monitor panel open)
//! .complete               marker written last; presence = extraction done
//! ```
//!
//! Extraction is write-to-temp-dir-then-rename, so concurrent first runs on
//! different nodes cannot see a half-written tree.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use sint_core::config::Config;

static PLUGIN_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sint-zellij.wasm"));
static CONFIG_KDL: &str = include_str!("../../../assets/zellij/config.kdl");
static LAYOUT_KDL: &str = include_str!("../../../assets/zellij/layouts/sint.kdl");
static PANEL_LAYOUT_KDL: &str = include_str!("../../../assets/zellij/layouts/sint-panel.kdl");

/// The zellij version compiled in.
pub const ZELLIJ_VERSION: &str = zellij_utils::consts::VERSION;

/// Paths of an extracted bundle.
#[derive(Debug, Clone)]
pub struct Bundle {
    pub dir: PathBuf,
    pub plugin: PathBuf,
    pub config: PathBuf,
    pub layouts: PathBuf,
}

/// Short id of this build's bundle: the first 12 hex chars of the sha256 of
/// (zellij version, plugin bytes, config text, layout text, mouse flag).
fn bundle_id(mouse: bool) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(ZELLIJ_VERSION.as_bytes());
    h.update(PLUGIN_WASM);
    h.update(CONFIG_KDL.as_bytes());
    h.update(LAYOUT_KDL.as_bytes());
    h.update(PANEL_LAYOUT_KDL.as_bytes());
    h.update(if mouse { "mouse" } else { "nomouse" }.as_bytes());
    h.update(exe_path().as_bytes());
    let hex = format!("{:x}", h.finalize());
    hex[..12].to_string()
}

/// This binary's path, for the keybindings that run `sinteractive` in a
/// floating pane (it is usually not on the job's PATH).
fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "sinteractive".to_string())
}

/// Whether this binary carries the plugin (built without `SINT_SKIP_BUNDLE`).
pub fn has_plugin() -> bool {
    !PLUGIN_WASM.is_empty()
}

/// Ensure the bundle is extracted for `cfg` and return its paths.
pub fn ensure(cfg: &Config, mouse: bool) -> Result<Bundle> {
    let dir = cfg.cache_dir.join("bin").join(bundle_id(mouse));
    let bundle = Bundle {
        plugin: dir.join("sint-zellij.wasm"),
        config: dir.join("config.kdl"),
        layouts: dir.join("layouts"),
        dir: dir.clone(),
    };
    if dir.join(".complete").exists() {
        return Ok(bundle);
    }
    fs::create_dir_all(dir.parent().unwrap())?;
    let tmp = tempfile::Builder::new()
        .prefix(".extract-")
        .tempdir_in(dir.parent().unwrap())
        .context("create extraction dir")?;
    let t = tmp.path();
    fs::write(t.join("sint-zellij.wasm"), PLUGIN_WASM)?;
    fs::create_dir_all(t.join("layouts"))?;
    let sub = |s: &str| {
        s.replace("__PLUGIN__", &bundle.plugin.to_string_lossy())
            .replace("__LAYOUTS__", &bundle.layouts.to_string_lossy())
            .replace("__MOUSE__", if mouse { "true" } else { "false" })
            .replace("__EXE__", &exe_path())
    };
    fs::write(t.join("config.kdl"), sub(CONFIG_KDL))?;
    fs::write(t.join("layouts/sint.kdl"), sub(LAYOUT_KDL))?;
    fs::write(t.join("layouts/sint-panel.kdl"), sub(PANEL_LAYOUT_KDL))?;
    fs::write(t.join(".complete"), ZELLIJ_VERSION)?;
    match fs::rename(t, &dir) {
        Ok(()) => {
            std::mem::forget(tmp);
        }
        Err(_) if dir.join(".complete").exists() => {
            // Another process won the race; ours is discarded by `tmp`.
        }
        Err(e) => return Err(e).context("install extracted bundle"),
    }
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_and_layout_placeholders_are_substituted() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::defaults();
        cfg.cache_dir = dir.path().to_path_buf();
        let b = ensure(&cfg, true).unwrap();
        let conf = fs::read_to_string(&b.config).unwrap();
        assert!(!conf.contains("__PLUGIN__") && !conf.contains("__LAYOUTS__"));
        assert!(conf.contains("mouse_mode true"));
        assert!(!conf.contains("__EXE__"));
        assert!(conf.contains(&format!("Run \"{}\"", exe_path())));
        assert!(conf.contains(&format!("layout_dir \"{}\"", b.layouts.display())));
        let lay = fs::read_to_string(b.layouts.join("sint.kdl")).unwrap();
        let panel = fs::read_to_string(b.layouts.join("sint-panel.kdl")).unwrap();
        assert!(panel.contains("view \"monitor\"") && !panel.contains("__PLUGIN__"));
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
