//! Detection of the external tools arvidleaf relies on.

use std::path::PathBuf;

/// Looks up an executable in `PATH` (on Windows also `name.exe`).
pub fn find(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: &[String] = if cfg!(windows) { &[format!("{name}.exe"), format!("{name}.bat"), name.to_string()] } else { &[] };
    std::env::split_paths(&path).find_map(|d| {
        if names.is_empty() {
            Some(d.join(name)).filter(|p| p.is_file())
        } else {
            names.iter().map(|n| d.join(n)).find(|p| p.is_file())
        }
    })
}

/// A command for an external tool. On Windows it runs without flashing a
/// console window.
pub fn command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

pub struct Tool {
    pub name: &'static str,
    pub purpose: &'static str,
    pub required: bool,
    pub found: bool,
}

/// All tools with their status.
pub fn check() -> Vec<Tool> {
    let list: [(&str, &str, bool); 12] = [
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
        ("pdftotext", "find the DOI of an imported PDF", false),
        ("curl", "online import (DOI, arXiv, ISBN)", false),
    ];
    list.iter().map(|(n, p, r)| Tool { name: n, purpose: p, required: *r, found: find(n).is_some() }).collect()
}

/// Whether TeX can find a file (`kpsewhich`), e.g. `ngerman.ldf`.
pub fn kpsewhich(file: &str) -> Option<PathBuf> {
    let o = command("kpsewhich").arg(file).output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(PathBuf::from(s)) }
}
