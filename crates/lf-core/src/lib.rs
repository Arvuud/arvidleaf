//! Projects, settings, templates, version history and archives. No GUI code.

pub mod archive;
pub mod config;
pub mod history;
pub mod paths;
pub mod project;
pub mod templates;

use std::io;
use std::path::Path;

/// Writes a file via a temporary file and rename, so a crash never leaves a
/// half-written file behind.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    // unique per call: two threads may write the same file at the same time
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.{}-{n}.arvidleaf-tmp", std::process::id()));
    std::fs::write(&tmp, data)?;
    // keep permissions of an existing file
    if let Ok(md) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, md.permissions());
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::PathBuf;

    /// Fresh empty directory under the system temp dir.
    pub fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("arvidleaf-test-{tag}-{}-{}", std::process::id(), crate::history::now_ms()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }
}
