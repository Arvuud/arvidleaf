//! Per-user directories (XDG). `ARVIDLEAF_HOME` overrides all of them (tests).

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    if let Some(h) = std::env::var_os("ARVIDLEAF_HOME") {
        return PathBuf::from(h).join(fallback.rsplit('/').next().unwrap_or(fallback));
    }
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v).join("arvidleaf"),
        _ => home().join(fallback).join("arvidleaf"),
    }
}

/// `~/.config/arvidleaf`
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

/// `~/.local/share/arvidleaf` (user templates)
pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

/// `~/.cache/arvidleaf` (template previews)
pub fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache")
}

/// Default parent folder for new projects.
pub fn default_projects_dir() -> PathBuf {
    let docs = home().join("Documents");
    if docs.is_dir() { docs.join("arvidleaf") } else { home().join("arvidleaf") }
}

pub fn home_dir() -> PathBuf {
    home()
}
