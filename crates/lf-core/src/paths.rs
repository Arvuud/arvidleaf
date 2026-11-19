//! Per-user directories: XDG on Linux, `~/Library` on macOS, `%APPDATA%` on
//! Windows. `ARVIDLEAF_HOME` overrides all of them (tests); set XDG variables
//! win on every system.

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[derive(Clone, Copy)]
enum Kind {
    Config,
    Data,
    Cache,
}

fn dir(var: &str, fallback: &str, kind: Kind) -> PathBuf {
    if let Some(h) = std::env::var_os("ARVIDLEAF_HOME") {
        return PathBuf::from(h).join(fallback.rsplit('/').next().unwrap_or(fallback));
    }
    if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
        return PathBuf::from(v).join("arvidleaf");
    }
    native(kind).unwrap_or_else(|| home().join(fallback).join("arvidleaf"))
}

#[cfg(target_os = "macos")]
fn native(kind: Kind) -> Option<PathBuf> {
    let lib = home().join("Library");
    Some(match kind {
        Kind::Config => lib.join("Application Support/arvidleaf"),
        Kind::Data => lib.join("Application Support/arvidleaf/data"),
        Kind::Cache => lib.join("Caches/arvidleaf"),
    })
}

#[cfg(windows)]
fn native(kind: Kind) -> Option<PathBuf> {
    let var = |v: &str| std::env::var_os(v).filter(|x| !x.is_empty()).map(PathBuf::from);
    Some(match kind {
        Kind::Config => var("APPDATA")?.join("arvidleaf"),
        Kind::Data => var("APPDATA")?.join("arvidleaf").join("data"),
        Kind::Cache => var("LOCALAPPDATA")?.join("arvidleaf").join("cache"),
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
fn native(_: Kind) -> Option<PathBuf> {
    None
}

/// `~/.config/arvidleaf`
pub fn config_dir() -> PathBuf {
    dir("XDG_CONFIG_HOME", ".config", Kind::Config)
}

/// `~/.local/share/arvidleaf` (user templates)
pub fn data_dir() -> PathBuf {
    dir("XDG_DATA_HOME", ".local/share", Kind::Data)
}

/// `~/.cache/arvidleaf` (template previews)
pub fn cache_dir() -> PathBuf {
    dir("XDG_CACHE_HOME", ".cache", Kind::Cache)
}

/// Default parent folder for new projects.
pub fn default_projects_dir() -> PathBuf {
    let docs = home().join("Documents");
    if docs.is_dir() { docs.join("arvidleaf") } else { home().join("arvidleaf") }
}

pub fn home_dir() -> PathBuf {
    home()
}
