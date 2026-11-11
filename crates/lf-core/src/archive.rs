//! ZIP import (e.g. an Overleaf "Download Source" archive) and export.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Writes all project files (no hidden files, no TeX by-products) into a ZIP.
pub fn export_zip(root: &Path, out: &Path) -> io::Result<usize> {
    let f = std::fs::File::create(out)?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let files = crate::project::list_files(root);
    for rel in &files {
        let name = rel.to_string_lossy().replace('\\', "/");
        z.start_file(name, opts).map_err(io::Error::other)?;
        z.write_all(&std::fs::read(root.join(rel))?)?;
    }
    z.finish().map_err(io::Error::other)?;
    Ok(files.len())
}

/// Extracts a ZIP into a new folder `<parent>/<zip name>` and returns it.
/// A single top-level folder inside the archive is flattened away.
pub fn import_zip(zip_path: &Path, parent: &Path) -> io::Result<PathBuf> {
    let f = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f).map_err(io::Error::other)?;
    let mut entries: Vec<(PathBuf, usize)> = Vec::new();
    for i in 0..z.len() {
        let e = z.by_index(i).map_err(io::Error::other)?;
        if e.is_dir() {
            continue;
        }
        // enclosed_name rejects absolute paths and `..` (zip slip)
        let Some(p) = e.enclosed_name() else { continue };
        if p.components().any(|c| c.as_os_str() == "__MACOSX") {
            continue;
        }
        entries.push((p, i));
    }
    if entries.is_empty() {
        return Err(io::Error::other("the archive contains no files"));
    }
    let first: Vec<_> = entries.iter().map(|(p, _)| p.components().next().map(|c| c.as_os_str().to_owned())).collect();
    let strip = entries.iter().all(|(p, _)| p.components().count() > 1) && first.windows(2).all(|w| w[0] == w[1]);
    let stem = zip_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "project".into());
    let mut dest = parent.join(&stem);
    let mut n = 1;
    while dest.exists() {
        n += 1;
        dest = parent.join(format!("{stem}-{n}"));
    }
    std::fs::create_dir_all(&dest)?;
    for (p, i) in entries {
        let rel: PathBuf = if strip { p.components().skip(1).collect() } else { p };
        let out = dest.join(&rel);
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut e = z.by_index(i).map_err(io::Error::other)?;
        let mut buf = Vec::with_capacity(e.size() as usize);
        e.read_to_end(&mut buf)?;
        std::fs::write(out, buf)?;
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_roundtrip() {
        let d = crate::testutil::tempdir("zip");
        let src = d.join("src");
        std::fs::create_dir_all(src.join("fig")).unwrap();
        std::fs::write(src.join("main.tex"), "\\documentclass{article}").unwrap();
        std::fs::write(src.join("fig/a.png"), [1u8, 2, 3]).unwrap();
        std::fs::write(src.join("main.log"), "x").unwrap();
        let zip = d.join("Paper.zip");
        assert_eq!(export_zip(&src, &zip).unwrap(), 2);
        let out = import_zip(&zip, &d).unwrap();
        assert_eq!(out, d.join("Paper"));
        assert_eq!(std::fs::read(out.join("fig/a.png")).unwrap(), vec![1, 2, 3]);
        assert!(!out.join("main.log").exists());
        let again = import_zip(&zip, &d).unwrap();
        assert_eq!(again, d.join("Paper-2"));
    }
}
