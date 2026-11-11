//! Detection of the external tools arvidleaf relies on.

use std::path::PathBuf;

/// Looks up an executable in `PATH`.
pub fn find(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

pub struct Tool {
    pub name: &'static str,
    pub purpose: &'static str,
    pub required: bool,
    pub found: bool,
}

/// All tools with their status.
pub fn check() -> Vec<Tool> {
    let list: [(&str, &str, bool); 10] = [
        ("latexmk", "runs the compilation", true),
        ("pdflatex", "pdfLaTeX engine", true),
        ("xelatex", "XeLaTeX engine", false),
        ("lualatex", "LuaLaTeX engine", false),
        ("bibtex", "bibliographies (BibTeX)", false),
        ("biber", "bibliographies (biblatex/biber)", false),
        ("synctex", "jump between PDF and source", false),
        ("pdftoppm", "PDF preview", true),
        ("pdfinfo", "PDF preview", true),
        ("kpsewhich", "package lookup", false),
    ];
    list.iter().map(|(n, p, r)| Tool { name: n, purpose: p, required: *r, found: find(n).is_some() }).collect()
}

/// Whether TeX can find a file (`kpsewhich`), e.g. `ngerman.ldf`.
pub fn kpsewhich(file: &str) -> Option<PathBuf> {
    let o = std::process::Command::new("kpsewhich").arg(file).output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(PathBuf::from(s)) }
}
