//! Compilation through `latexmk` (no time or page limits; cancellable).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::log::{self, Level, LogEntry};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Pdflatex,
    Xelatex,
    Lualatex,
}

impl Engine {
    pub const ALL: [Engine; 3] = [Engine::Pdflatex, Engine::Xelatex, Engine::Lualatex];

    pub fn label(self) -> &'static str {
        match self {
            Engine::Pdflatex => "pdfLaTeX",
            Engine::Xelatex => "XeLaTeX",
            Engine::Lualatex => "LuaLaTeX",
        }
    }

    pub fn latexmk_flag(self) -> &'static str {
        match self {
            Engine::Pdflatex => "-pdf",
            Engine::Xelatex => "-pdfxe",
            Engine::Lualatex => "-pdflua",
        }
    }

    pub fn from_name(s: &str) -> Option<Engine> {
        match s.to_ascii_lowercase().as_str() {
            "pdflatex" => Some(Engine::Pdflatex),
            "xelatex" => Some(Engine::Xelatex),
            "lualatex" => Some(Engine::Lualatex),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Job {
    /// Project root; also the working directory.
    pub root: PathBuf,
    /// Main file relative to `root`.
    pub main: PathBuf,
    pub engine: Engine,
    pub shell_escape: bool,
    /// Absolute output directory (aux, log, pdf, synctex).
    pub out_dir: PathBuf,
}

impl Job {
    pub fn pdf_path(&self) -> PathBuf {
        self.out_dir.join(self.main.with_extension("pdf").file_name().unwrap_or_default())
    }

    pub fn log_path(&self) -> PathBuf {
        self.out_dir.join(self.main.with_extension("log").file_name().unwrap_or_default())
    }
}

#[derive(Clone, Debug)]
pub struct MissingPackage {
    pub file: String,
    /// Distribution package providing the file, if known (e.g. `extra/texlive-langgerman`).
    pub provider: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// PDF produced and no errors.
    pub ok: bool,
    pub cancelled: bool,
    pub pdf: Option<PathBuf>,
    pub entries: Vec<LogEntry>,
    pub log: String,
    /// latexmk's own output (stdout + stderr).
    pub output: String,
    pub pages: Option<u32>,
    pub duration: Duration,
    pub missing: Vec<MissingPackage>,
    /// Set when the compiler could not be started at all.
    pub launch_error: Option<String>,
}

impl Outcome {
    pub fn errors(&self) -> usize {
        self.entries.iter().filter(|e| e.level == Level::Error).count()
    }

    pub fn warnings(&self) -> usize {
        self.entries.iter().filter(|e| e.level == Level::Warning).count()
    }
}

/// Handle to cancel a running compilation from another thread.
#[derive(Clone, Default)]
pub struct Cancel {
    pid: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
}

impl Cancel {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let pid = self.pid.load(Ordering::SeqCst);
        if pid != 0 {
            // latexmk runs in its own process group: stop it and all children
            let _ = Command::new("kill").arg("-TERM").arg(format!("-{pid}")).stderr(Stdio::null()).status();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

fn mirror_dirs(root: &Path, dir: &Path, out: &Path, depth: usize) {
    if depth > 12 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') || !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let p = e.path();
        if let Ok(rel) = p.strip_prefix(root) {
            let _ = std::fs::create_dir_all(out.join(rel));
        }
        mirror_dirs(root, &p, out, depth + 1);
    }
}

fn search_path(root: &Path) -> String {
    // `//` searches subdirectories; the trailing `:` appends the system default
    format!("{}:{}//:", root.display(), root.display())
}

/// Runs latexmk and parses the result. Blocks until done (run it in a thread).
pub fn run(job: &Job, cancel: &Cancel) -> Outcome {
    let start = Instant::now();
    let _ = std::fs::create_dir_all(&job.out_dir);
    // \include{chapters/x} writes chapters/x.aux into the output directory:
    // TeX cannot create folders, so mirror the project's folder structure
    mirror_dirs(&job.root, &job.root, &job.out_dir, 0);
    let mut cmd = Command::new("latexmk");
    cmd.arg(job.engine.latexmk_flag())
        .arg("-synctex=1")
        .arg("-interaction=nonstopmode")
        .arg("-file-line-error")
        .arg("-f")
        .arg(format!("-outdir={}", job.out_dir.display()));
    if job.shell_escape {
        cmd.arg("-shell-escape");
    }
    // latexmk remembers failed runs and reports "nothing to do" until a source
    // changes – but the cause may have been outside (a package was installed,
    // a folder created). After a failed run, always run again.
    let previous = std::fs::read(job.log_path()).map(|d| String::from_utf8_lossy(&d).into_owned()).unwrap_or_default();
    if !previous.is_empty() && log::parse(&previous).iter().any(|e| e.level == Level::Error) {
        cmd.arg("-g");
    }
    cmd.arg(&job.main)
        .current_dir(&job.root)
        .env("TEXINPUTS", search_path(&job.root))
        .env("BIBINPUTS", search_path(&job.root))
        .env("BSTINPUTS", search_path(&job.root))
        // long log lines keep file names and messages unbroken; BibTeX
        // rejects values above 10000 ("3 is a bad bad") and also reads
        // error_line/half_error_line, so those stay at their defaults
        .env("max_print_line", "10000")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Outcome { launch_error: Some(format!("could not start latexmk: {e}")), duration: start.elapsed(), ..Default::default() };
        }
    };
    cancel.pid.store(child.id(), Ordering::SeqCst);
    let output = child.wait_with_output();
    cancel.pid.store(0, Ordering::SeqCst);
    let mut out = Outcome { duration: start.elapsed(), cancelled: cancel.is_cancelled(), ..Default::default() };
    if let Ok(o) = output {
        out.output = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    }
    let log_bytes = std::fs::read(job.log_path()).unwrap_or_default();
    out.log = String::from_utf8_lossy(&log_bytes).into_owned();
    out.entries = log::parse(&out.log);
    // project files as relative paths, like the editor shows them
    let root = job.root.canonicalize().unwrap_or_else(|_| job.root.clone());
    for e in &mut out.entries {
        if let Some(f) = &e.file
            && let Ok(rel) = Path::new(f).strip_prefix(&root).or_else(|_| Path::new(f).strip_prefix(&job.root)) {
                e.file = Some(rel.to_string_lossy().to_string());
            }
    }
    out.pages = log::page_count(&out.log);
    let pdf = job.pdf_path();
    if pdf.exists() {
        out.pdf = Some(pdf);
    }
    locate_missing(&job.root, &mut out.entries);
    for e in out.entries.iter_mut().filter(|e| e.file.is_none() && e.message.starts_with("Missing \\end{document}")) {
        let lines = std::fs::read_to_string(job.root.join(&job.main)).map(|s| s.lines().count()).unwrap_or(1);
        e.file = Some(job.main.to_string_lossy().to_string());
        e.line = Some(lines.max(1) as u32);
    }
    out.missing = log::missing_files(&out.entries).into_iter().map(|f| MissingPackage { provider: provider_of(&f), file: f }).collect();
    out.ok = !out.cancelled && out.pdf.is_some() && out.errors() == 0;
    out
}

/// Project source files (`.tex`, `.sty`, `.cls`), hidden folders skipped.
fn project_sources(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut ents: Vec<_> = rd.filter_map(|x| x.ok()).collect();
        ents.sort_by_key(|e| e.file_name());
        for ent in ents {
            let p = ent.path();
            let name = ent.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if (name.ends_with(".tex") || name.ends_with(".sty") || name.ends_with(".cls"))
                && let Ok(src) = std::fs::read_to_string(&p) {
                    out.push((p.strip_prefix(root).unwrap_or(&p).to_string_lossy().to_string(), src));
                }
        }
    }
    out
}

/// Line that loads `name` (`\usepackage{name}`, `\input{name}`, …).
fn find_load(sources: &[(String, String)], name: &str) -> Option<(String, u32)> {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    for (f, src) in sources {
        for (n, line) in src.lines().enumerate() {
            let code = crate::outline::strip_comment(line);
            let wants = ["usepackage", "RequirePackage", "documentclass", "input", "include", "babel"].iter().any(|c| code.contains(&format!("\\{c}")))
                || code.contains("\\selectlanguage");
            let names_it = code.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')).any(|w| w == stem || w == name);
            if wants && names_it {
                return Some((f.clone(), n as u32 + 1));
            }
        }
    }
    None
}

/// Line where a runaway argument starts: its first words, as TeX printed them.
fn find_runaway(sources: &[(String, String)], text: &str) -> Option<(String, u32)> {
    let t = text.trim_start_matches(['{', '[', ' ']);
    let end = t.find(['\\', '{', '}']).unwrap_or(t.len());
    let snippet = t[..end].trim();
    if snippet.len() < 3 {
        return None;
    }
    let mut hits = sources.iter().flat_map(|(f, src)| {
        src.lines().enumerate().filter(move |(_, l)| crate::outline::strip_comment(l).contains(snippet)).map(move |(n, _)| (f.clone(), n as u32 + 1))
    });
    let first = hits.next()?;
    // ambiguous text: better no line than a wrong one
    hits.next().is_none().then_some(first)
}

/// TeX reports some errors without a line, or inside a system file
/// (babel.sty, fontspec.sty). Point to the line in the project's sources:
/// the `\usepackage` that loaded the file, or the start of a runaway argument.
fn locate_missing(root: &Path, entries: &mut [LogEntry]) {
    let outside = |e: &LogEntry| e.line.is_none() || e.file.as_ref().is_some_and(|f| Path::new(f).is_absolute());
    if !entries.iter().any(|e| e.level == Level::Error && outside(e)) {
        return;
    }
    let sources = project_sources(root);
    for e in entries.iter_mut().filter(|e| e.level == Level::Error && outside(e)) {
        let found = if let Some(missing) = log::missing_files(std::slice::from_ref(e)).into_iter().next() {
            find_load(&sources, &missing)
        } else if let Some(r) = e.context.lines().find_map(|l| l.strip_prefix("Runaway argument: ")) {
            find_runaway(&sources, r)
        } else if let Some(sys) = e.file.clone().filter(|f| Path::new(f).is_absolute()) {
            let name = Path::new(&sys).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let hit = find_load(&sources, &name);
            if hit.is_some() {
                let at = e.line.map(|l| format!("{name}:{l}")).unwrap_or(name);
                e.context = format!("(reported in {at})\n{}", e.context).trim_end().to_string();
            }
            hit
        } else {
            None
        };
        if let Some((f, l)) = found {
            e.file = Some(f);
            e.line = Some(l);
        }
        if e.message.contains("requires either XeTeX or LuaTeX") || e.message.contains("requires XeTeX or LuaTeX") {
            e.message.push_str(" Hint: switch the engine to XeLaTeX or LuaLaTeX (Build › Engine).");
        }
    }
}

/// Asks the package manager which package ships a TeX file (Arch: `pacman -F`,
/// which needs a synced file database; babel languages are also known without it).
pub fn provider_of(file: &str) -> Option<String> {
    crate::tools::find("pacman")?;
    if let Ok(o) = Command::new("pacman").arg("-Fq").arg(file).stderr(Stdio::null()).output() {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(pick) = s.lines().find(|l| l.contains("texlive")).or_else(|| s.lines().next()) {
            return Some(pick.trim().to_string());
        }
    }
    file.strip_suffix(".ldf").map(|lang| babel_package(lang).to_string())
}

/// Arch Linux package with the babel support for a language.
pub fn babel_package(lang: &str) -> &'static str {
    match lang.to_ascii_lowercase().as_str() {
        "german" | "ngerman" | "austrian" | "naustrian" | "swissgerman" | "nswissgerman" => "texlive-langgerman",
        "english" | "british" | "american" | "ukenglish" | "usenglish" | "canadian" | "australian" | "newzealand" => "texlive-langenglish",
        "french" | "francais" | "acadian" | "canadien" => "texlive-langfrench",
        "spanish" | "spanishmx" => "texlive-langspanish",
        "italian" => "texlive-langitalian",
        "polish" => "texlive-langpolish",
        "portuguese" | "portuges" | "brazil" | "brazilian" => "texlive-langportuguese",
        "russian" | "ukrainian" | "bulgarian" | "serbianc" | "belarusian" => "texlive-langcyrillic",
        "greek" | "polutonikogreek" => "texlive-langgreek",
        "czech" | "slovak" => "texlive-langczechslovak",
        "dutch" | "danish" | "swedish" | "norsk" | "nynorsk" | "finnish" | "estonian" | "icelandic" | "irish" | "welsh" | "croatian" | "slovene"
        | "hungarian" | "magyar" | "romanian" | "albanian" | "catalan" | "basque" | "galician" | "latvian" | "lithuanian" | "turkish" => {
            "texlive-langeuropean"
        }
        _ => "texlive-langother",
    }
}

/// Removes the build directory (like `latexmk -C`).
pub fn clean(out_dir: &Path) -> std::io::Result<()> {
    if out_dir.exists() { std::fs::remove_dir_all(out_dir) } else { Ok(()) }
}
