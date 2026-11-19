//! A project is a plain folder; settings live in `.arvidleaf/project.toml`.

use std::io;
use std::path::{Path, PathBuf};

use lf_tex::Engine;
use serde::{Deserialize, Serialize};

/// Name of the per-project metadata folder (build output, history, settings).
pub const META_DIR: &str = ".arvidleaf";

/// Extensions of TeX by-products: hidden in the tree, left out of history and ZIP export.
pub const AUX_EXTS: &[&str] = &[
    "aux", "log", "fls", "fdb_latexmk", "synctex", "synctex.gz", "out", "toc", "lof", "lot", "bbl", "blg", "nav", "snm", "vrb", "xdv", "bcf", "run.xml",
    "idx", "ilg", "ind", "glo", "gls", "glg", "ist", "acn", "acr", "alg", "lol", "thm", "dvi",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectMeta {
    /// Main file relative to the root.
    pub main: Option<String>,
    pub engine: Engine,
    pub shell_escape: bool,
    pub auto_compile: bool,
    pub open_files: Vec<String>,
}

impl Default for ProjectMeta {
    fn default() -> Self {
        ProjectMeta { main: None, engine: Engine::Pdflatex, shell_escape: false, auto_compile: true, open_files: Vec::new() }
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    pub meta: ProjectMeta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileNode {
    pub name: String,
    /// Path relative to the project root.
    pub rel: PathBuf,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
}

pub fn is_aux(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    AUX_EXTS.iter().any(|e| lower.ends_with(&format!(".{e}")))
}

fn hidden(name: &str) -> bool {
    name.starts_with('.') || name.ends_with('~') || is_aux(name)
}

impl Project {
    /// Opens a folder as project (creates no files until settings are saved).
    pub fn open(root: &Path) -> io::Result<Project> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "not a folder"));
        }
        let meta: ProjectMeta =
            std::fs::read_to_string(root.join(META_DIR).join("project.toml")).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default();
        let mut p = Project { root, meta };
        if p.meta.main.as_ref().is_none_or(|m| !p.root.join(m).is_file()) {
            p.meta.main = detect_main(&p.root);
        }
        Ok(p)
    }

    pub fn name(&self) -> String {
        self.root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "project".into())
    }

    pub fn meta_dir(&self) -> PathBuf {
        self.root.join(META_DIR)
    }

    pub fn build_dir(&self) -> PathBuf {
        self.meta_dir().join("build")
    }

    pub fn save_meta(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.meta_dir())?;
        let s = toml::to_string_pretty(&self.meta).map_err(io::Error::other)?;
        crate::write_atomic(&self.meta_dir().join("project.toml"), s.as_bytes())
    }

    pub fn main_rel(&self) -> Option<PathBuf> {
        self.meta.main.as_ref().map(PathBuf::from)
    }

    pub fn compile_job(&self) -> Option<lf_tex::compile::Job> {
        Some(lf_tex::compile::Job {
            root: self.root.clone(),
            main: self.main_rel()?,
            engine: self.meta.engine,
            shell_escape: self.meta.shell_escape,
            out_dir: self.build_dir(),
        })
    }

    pub fn tree(&self) -> Vec<FileNode> {
        scan_tree(&self.root, Path::new(""))
    }

    /// All visible files (relative paths), depth-first.
    pub fn files(&self) -> Vec<PathBuf> {
        list_files(&self.root)
    }
}

pub fn scan_tree(root: &Path, rel: &Path) -> Vec<FileNode> {
    let Ok(rd) = std::fs::read_dir(root.join(rel)) else { return Vec::new() };
    let mut v: Vec<FileNode> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if hidden(&name) {
                return None;
            }
            let is_dir = e.file_type().ok()?.is_dir();
            let r = rel.join(&name);
            let children = if is_dir { scan_tree(root, &r) } else { Vec::new() };
            Some(FileNode { name, rel: r, is_dir, children })
        })
        .collect();
    v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

pub fn list_files(root: &Path) -> Vec<PathBuf> {
    fn walk(nodes: &[FileNode], out: &mut Vec<PathBuf>) {
        for n in nodes {
            if n.is_dir {
                walk(&n.children, out);
            } else {
                out.push(n.rel.clone());
            }
        }
    }
    let mut out = Vec::new();
    walk(&scan_tree(root, Path::new("")), &mut out);
    out
}

/// Whether the (uncommented) source declares a document class.
pub fn has_documentclass(src: &str) -> bool {
    src.lines().any(|l| lf_tex::outline::strip_comment(l).contains("\\documentclass"))
}

/// Finds the main file: `main.tex` if it is a document, else the first `.tex`
/// with `\documentclass` (shallow files first).
pub fn detect_main(root: &Path) -> Option<String> {
    let mut texs: Vec<PathBuf> = list_files(root).into_iter().filter(|p| p.extension().is_some_and(|e| e == "tex")).collect();
    texs.sort_by_key(|p| (p.components().count(), p.file_name() != Some("main.tex".as_ref()), p.clone()));
    texs.into_iter()
        .find(|p| std::fs::read_to_string(root.join(p)).is_ok_and(|s| has_documentclass(&s)))
        .map(|p| p.to_string_lossy().to_string())
}

/// Rejects paths that would leave the project (`..`, absolute).
pub fn safe_rel(rel: &Path) -> io::Result<()> {
    use std::path::Component;
    if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid name"));
    }
    Ok(())
}

pub fn create_file(root: &Path, rel: &Path, content: &[u8]) -> io::Result<()> {
    safe_rel(rel)?;
    let p = root.join(rel);
    if p.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "a file with this name exists"));
    }
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, content)
}

pub fn create_dir(root: &Path, rel: &Path) -> io::Result<()> {
    safe_rel(rel)?;
    std::fs::create_dir_all(root.join(rel))
}

pub fn rename(root: &Path, from: &Path, to: &Path) -> io::Result<()> {
    safe_rel(from)?;
    safe_rel(to)?;
    let dst = root.join(to);
    if dst.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "target exists"));
    }
    if let Some(d) = dst.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::rename(root.join(from), dst)
}

/// Moves a file or folder to the trash: `~/.Trash` on macOS, arvidleaf's own
/// `trash` folder on Windows (the Recycle Bin needs the shell API).
#[cfg(any(windows, target_os = "macos"))]
pub fn trash(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let dir = crate::paths::home_dir().join(".Trash");
    #[cfg(windows)]
    let dir = crate::paths::data_dir().join("trash");
    std::fs::create_dir_all(&dir)?;
    let name = path.file_name().ok_or_else(|| io::Error::other("no file name"))?.to_string_lossy().to_string();
    let mut target = dir.join(&name);
    let mut i = 1;
    while target.exists() {
        i += 1;
        target = dir.join(format!("{name}.{i}"));
    }
    std::fs::rename(path, target)
}

/// Moves a file or folder to the freedesktop trash (`~/.local/share/Trash`).
#[cfg(not(any(windows, target_os = "macos")))]
pub fn trash(path: &Path) -> io::Result<()> {
    let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| crate::paths::home_dir().join(".local/share")).join("Trash");
    let files = base.join("files");
    let info = base.join("info");
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&info)?;
    let name = path.file_name().ok_or_else(|| io::Error::other("no file name"))?.to_string_lossy().to_string();
    let mut target = name.clone();
    let mut i = 1;
    while files.join(&target).exists() || info.join(format!("{target}.trashinfo")).exists() {
        i += 1;
        target = format!("{name}.{i}");
    }
    let abs = path.canonicalize()?;
    let when = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    std::fs::write(info.join(format!("{target}.trashinfo")), format!("[Trash Info]\nPath={}\nDeletionDate={when}\n", abs.display()))?;
    match std::fs::rename(&abs, files.join(&target)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(info.join(format!("{target}.trashinfo")));
            Err(e)
        }
    }
}

/// Copies an outside file or folder into `dest_dir` (inside the project),
/// adding a number if the name is taken. Returns the new path.
pub fn copy_into(src: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    let name = src.file_name().ok_or_else(|| io::Error::other("no file name"))?;
    let stem = Path::new(name).file_stem().unwrap_or(name).to_string_lossy().to_string();
    let ext = Path::new(name).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut dst = dest_dir.join(name);
    let mut i = 1;
    while dst.exists() {
        i += 1;
        dst = dest_dir.join(format!("{stem}-{i}{ext}"));
    }
    copy_recursive(src, &dst)?;
    Ok(dst)
}

pub fn copy_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_recursive(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::copy(src, dst).map(|_| ())
    }
}

/// Text files the editor opens; everything else is shown as preview or info.
pub fn is_text_file(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(
        ext.as_str(),
        "tex" | "sty" | "cls" | "bib" | "bst" | "txt" | "md" | "ltx" | "dtx" | "ins" | "cfg" | "def" | "clo" | "tikz" | "pgf" | "csv" | "dat" | "lua"
            | "py" | "m" | "json" | "toml" | "yaml" | "yml" | "latexmkrc" | "bbx" | "cbx" | "lbx" | "gnuplot" | "gp" | "svg" | "xml" | "sh" | "c"
            | "h" | "cpp" | "rs" | "java" | "js"
    ) || path.file_name().is_some_and(|n| n == "latexmkrc" || n == ".latexmkrc" || n == "Makefile" || n == "README")
}

pub fn is_image(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "pdf" | "eps")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_main_and_hides_aux() {
        let d = crate::testutil::tempdir("detect");
        std::fs::create_dir_all(d.join("chapters")).unwrap();
        std::fs::write(d.join("chapters/intro.tex"), "\\section{x}").unwrap();
        std::fs::write(d.join("thesis.tex"), "% \\documentclass{x}\n\\documentclass{report}\n").unwrap();
        std::fs::write(d.join("thesis.aux"), "").unwrap();
        std::fs::create_dir_all(d.join(".arvidleaf")).unwrap();
        let p = Project::open(&d).unwrap();
        assert_eq!(p.meta.main.as_deref(), Some("thesis.tex"));
        let files = p.files();
        assert_eq!(files, vec![PathBuf::from("chapters/intro.tex"), PathBuf::from("thesis.tex")]);
        assert!(safe_rel(Path::new("../x")).is_err());
        assert!(safe_rel(Path::new("a/b.tex")).is_ok());
    }
}
