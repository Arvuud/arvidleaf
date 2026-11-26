//! Main window: start screen, project workspace, menus, compile orchestration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Key, RichText, TextureHandle, TextureOptions, vec2};
use lf_core::config::Config;
use lf_core::history::History;
use lf_core::project::{self, FileNode, Project};
use lf_tex::Engine;
use lf_tex::compile::{Cancel, Outcome};
use lf_tex::log::{Level, LogEntry};
use lf_tex::outline::Heading;

use crate::editor::{EditorAction, EditorDoc, EditorEnv, Link};
use crate::history_ui::{HistoryAction, HistoryView};
use crate::pdfview::{PdfAction, PdfView};
use crate::refs_ui::RefsView;
use crate::templates_ui::{Gallery, GalleryResult, Manager, ManagerResult, Previews};
use crate::theme::{self, Palette};

/// Auto-save after this much idle time.
const AUTOSAVE: Duration = Duration::from_millis(1000);
/// Periodic snapshot interval while editing.
const SNAPSHOT_EVERY: Duration = Duration::from_secs(600);

enum Msg {
    Compiled(u64, Box<Outcome>),
    Forward(Option<lf_tex::synctex::Hit>),
    Inverse(Option<(PathBuf, u32)>),
}

struct ImageDoc {
    rel: PathBuf,
    abs: PathBuf,
    tex: Option<Result<TextureHandle, String>>,
}

#[allow(clippy::large_enum_variant)] // few open documents, mostly text
enum Doc {
    Text(EditorDoc),
    Image(ImageDoc),
}

impl Doc {
    fn rel(&self) -> &Path {
        match self {
            Doc::Text(d) => &d.rel,
            Doc::Image(d) => &d.rel,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Split,
    Editor,
    Pdf,
}

enum Dialog {
    NewEntry { dir: PathBuf, name: String, folder: bool, error: Option<String> },
    Rename { from: PathBuf, name: String, error: Option<String> },
    Delete { path: PathBuf },
    SaveTemplate { name: String, category: String, description: String, error: Option<String> },
    QuickOpen { query: String, selected: usize },
    Search { query: String, results: Vec<(PathBuf, u32, String)>, searched: bool },
    Shortcuts { query: String },
    SystemCheck,
    About,
    /// Insert citations from the library.
    Cite { query: String, searched: Option<String>, results: Vec<String>, selected: usize, checked: Vec<String>, cmd: String },
    /// The document has no bibliography yet; entries went to `bib`.
    BibSetup { bib: PathBuf },
}

struct Workspace {
    project: Project,
    tree: Vec<FileNode>,
    files: Vec<PathBuf>,
    scanned: Instant,
    docs: Vec<Doc>,
    active: Option<usize>,
    pdf: PdfView,
    compiling: Option<(u64, Cancel, Instant)>,
    compile_again: bool,
    last: Option<Outcome>,
    compile_due: Option<Instant>,
    outline: Vec<Heading>,
    labels: Vec<String>,
    bib_keys: Vec<String>,
    /// Project `.bib` entries: lower-case key → `Author (Year): Title`.
    bib_info: HashMap<String, String>,
    /// Changes when `bib_info` changes.
    bib_gen: u64,
    refs_at: Instant,
    history: Arc<History>,
    history_view: Option<HistoryView>,
    last_snapshot: Instant,
    selected_dir: PathBuf,
    problems_open: bool,
    show: [bool; 3],
    raw_log: bool,
    view: View,
}

pub struct App {
    cfg: Config,
    /// An input method is composing text (see `drop_idle_preedits`).
    ime_composing: bool,
    ws: Option<Workspace>,
    gallery: Option<Gallery>,
    manager: Option<Manager>,
    previews: Previews,
    dialog: Option<Dialog>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    status: String,
    tools: Vec<lf_tex::tools::Tool>,
    next_id: u64,
    ctx: egui::Context,
    title: String,
    /// Completion picks across sessions (ranks suggestions).
    usage: lf_core::usage::Usage,
    /// The reference library, loaded when first shown.
    refs: Option<RefsView>,
    /// The References view replaces the start screen or workspace.
    refs_open: bool,
    /// Library keys and descriptions of library and project entries for the editor,
    /// rebuilt when either changes.
    lib_keys: Vec<String>,
    cite_info: HashMap<String, String>,
    cite_stamp: (u64, u64),
}

/// Opens a file or folder with the desktop's default application.
pub fn open_external(path: &Path) {
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("open");
    #[cfg(windows)]
    let mut c = std::process::Command::new("explorer");
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut c = std::process::Command::new("xdg-open");
    let _ = c.arg(path).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

fn file_icon(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "tex" => "📄",
        "bib" => "📚",
        "sty" | "cls" => "⚙",
        "png" | "jpg" | "jpeg" | "pdf" | "eps" | "svg" => "🖼",
        _ => "🗋",
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Vec<PathBuf>) -> App {
        App::with_ctx(&cc.egui_ctx, open)
    }

    pub fn with_ctx(ctx: &egui::Context, open: Vec<PathBuf>) -> App {
        let cfg = Config::load();
        theme::install_fonts(ctx);
        theme::apply(ctx, cfg.dark);
        // Ctrl +/- zooms the PDF, not the whole UI
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let (tx, rx) = channel();
        let mut app = App {
            ime_composing: false,
            previews: Previews::new(ctx),
            cfg,
            ws: None,
            gallery: None,
            manager: None,
            dialog: None,
            tx,
            rx,
            status: "Ready".into(),
            tools: lf_tex::tools::check(),
            next_id: 1,
            ctx: ctx.clone(),
            title: String::new(),
            usage: lf_core::usage::Usage::load(),
            refs: None,
            refs_open: false,
            lib_keys: Vec::new(),
            cite_info: HashMap::new(),
            cite_stamp: (u64::MAX, u64::MAX),
        };
        let missing: Vec<&str> = app.tools.iter().filter(|t| t.required && !t.found).map(|t| t.name).collect();
        if !missing.is_empty() {
            app.status = format!("Missing tools: {} (Help › System Check)", missing.join(", "));
        }
        for p in open {
            app.open_path(&p);
        }
        app
    }

    /// Something is still running (compile, page rendering) – used by screenshots.
    pub fn is_busy(&self) -> bool {
        self.ws.as_ref().is_some_and(|w| w.compiling.is_some() || w.pdf.busy()) || self.refs.as_ref().is_some_and(|r| r.busy())
    }

    fn toggle_refs(&mut self) {
        self.refs_open = !self.refs_open;
        if self.refs_open && self.refs.is_none() {
            self.refs = Some(RefsView::new(&self.ctx, &self.cfg));
        }
    }

    fn open_path(&mut self, p: &Path) {
        if p.is_dir() {
            self.open_project(p);
        } else if p.extension().is_some_and(|e| e == "zip") {
            self.import_zip(p);
        } else if p.is_file() {
            // a file: open its folder as project and the file in it
            if let Some(dir) = p.parent() {
                self.open_project(dir);
                if let (Some(ws), Ok(abs)) = (&self.ws, p.canonicalize())
                    && let Ok(rel) = abs.strip_prefix(&ws.project.root) {
                        let rel = rel.to_path_buf();
                        self.open_file(&rel);
                    }
            }
        }
    }

    // ------------------------------------------------------------ projects

    fn open_project(&mut self, root: &Path) {
        self.close_project();
        let project = match Project::open(root) {
            Ok(p) => p,
            Err(e) => {
                self.status = format!("Cannot open {}: {e}", root.display());
                return;
            }
        };
        self.cfg.add_recent(&project.root);
        let _ = self.cfg.save();
        let history = Arc::new(History::new(&project.root));
        {
            let h = history.clone();
            std::thread::spawn(move || {
                let _ = h.prune(lf_core::history::now_ms());
            });
        }
        let mut ws = Workspace {
            tree: project.tree(),
            files: project.files(),
            scanned: Instant::now(),
            docs: Vec::new(),
            active: None,
            pdf: PdfView::new(&self.ctx),
            compiling: None,
            compile_again: false,
            last: None,
            compile_due: None,
            outline: Vec::new(),
            labels: Vec::new(),
            bib_keys: Vec::new(),
            bib_info: HashMap::new(),
            bib_gen: 0,
            refs_at: Instant::now() - Duration::from_secs(60),
            history,
            history_view: None,
            last_snapshot: Instant::now(),
            selected_dir: PathBuf::new(),
            problems_open: false,
            show: [true, true, false],
            raw_log: false,
            view: View::Split,
            project,
        };
        // show the last build right away
        if let Some(job) = ws.project.compile_job() {
            let pdf = job.pdf_path();
            if pdf.is_file() {
                ws.pdf.load(&pdf);
            }
        }
        let mut open: Vec<String> = ws.project.meta.open_files.clone();
        if open.is_empty() {
            open.extend(ws.project.meta.main.clone());
        }
        self.status = format!("Opened {}", ws.project.root.display());
        self.ws = Some(ws);
        for f in open {
            if self.ws.as_ref().is_some_and(|w| w.project.root.join(&f).is_file()) {
                self.open_file(Path::new(&f));
            }
        }
        if let Some(ws) = &mut self.ws {
            ws.active = ws.docs.iter().position(|d| Some(d.rel().to_string_lossy().to_string()) == ws.project.meta.main).or(ws.active);
        }
        self.refresh_refs(true);
        // the library is needed for \cite completion and the cite dialog
        self.ensure_refs();
        if self.ws.as_ref().is_some_and(|w| w.project.meta.auto_compile) {
            self.compile();
        }
    }

    fn close_project(&mut self) {
        self.save_all();
        if let Some(mut ws) = self.ws.take() {
            if let Some((_, c, _)) = &ws.compiling {
                c.cancel();
            }
            ws.project.meta.open_files = ws.docs.iter().map(|d| d.rel().to_string_lossy().to_string()).collect();
            let _ = ws.project.save_meta();
            let _ = ws.history.snapshot(None, true, "close");
        }
    }

    fn import_zip(&mut self, zip: &Path) {
        let parent = self.cfg.projects_dir();
        let _ = std::fs::create_dir_all(&parent);
        match lf_core::archive::import_zip(zip, &parent) {
            Ok(root) => {
                self.open_project(&root);
                self.status = format!("Imported {} into {}", zip.display(), root.display());
            }
            Err(e) => self.status = format!("Import failed: {e}"),
        }
    }

    fn rescan(&mut self) {
        if let Some(ws) = &mut self.ws {
            ws.tree = ws.project.tree();
            ws.files = ws.project.files();
            ws.scanned = Instant::now();
        }
    }

    // ------------------------------------------------------------ documents

    fn open_file(&mut self, rel: &Path) {
        let Some(ws) = &mut self.ws else { return };
        if let Some(i) = ws.docs.iter().position(|d| d.rel() == rel) {
            ws.active = Some(i);
            ws.history_view = None;
            return;
        }
        let abs = ws.project.root.join(rel);
        let doc = if project::is_text_file(rel) || std::fs::read(&abs).is_ok_and(|d| lf_core::history::is_text(&d[..d.len().min(8192)])) {
            match std::fs::read(&abs) {
                Ok(d) if lf_core::history::is_text(&d) => {
                    self.next_id += 1;
                    Doc::Text(EditorDoc::new(rel.to_path_buf(), abs, String::from_utf8(d).unwrap_or_default(), self.next_id))
                }
                Ok(_) => Doc::Image(ImageDoc { rel: rel.to_path_buf(), abs, tex: None }),
                Err(e) => {
                    self.status = format!("Cannot open {}: {e}", rel.display());
                    return;
                }
            }
        } else {
            Doc::Image(ImageDoc { rel: rel.to_path_buf(), abs, tex: None })
        };
        ws.docs.push(doc);
        ws.active = Some(ws.docs.len() - 1);
        ws.history_view = None;
    }

    fn open_at(&mut self, rel: &Path, line: u32) {
        self.open_file(rel);
        if let Some(ws) = &mut self.ws
            && let Some(Doc::Text(d)) = ws.active.and_then(|a| ws.docs.get_mut(a)) {
                d.goto_line(line);
            }
    }

    fn close_doc(&mut self, i: usize) {
        let Some(ws) = &mut self.ws else { return };
        if let Some(Doc::Text(d)) = ws.docs.get_mut(i)
            && d.dirty() {
                let _ = d.save();
            }
        if i < ws.docs.len() {
            ws.docs.remove(i);
        }
        ws.active = match ws.active {
            Some(a) if a > i => Some(a - 1),
            Some(a) if a == i => (!ws.docs.is_empty()).then(|| i.min(ws.docs.len() - 1)),
            a => a,
        };
    }

    /// Saves all modified documents; returns how many were saved.
    fn save_all(&mut self) -> usize {
        let Some(ws) = &mut self.ws else { return 0 };
        let mut n = 0;
        for d in &mut ws.docs {
            if let Doc::Text(d) = d
                && d.dirty() {
                    match d.save() {
                        Ok(()) => n += 1,
                        Err(e) => self.status = format!("Could not save {}: {e}", d.rel.display()),
                    }
                }
        }
        let _ = self.usage.save();
        n
    }

    /// Takes over what the editor collected this frame: completion picks and
    /// packages the text now needs.
    fn after_editor(&mut self) {
        let Some(ws) = &mut self.ws else { return };
        let Some(Doc::Text(d)) = ws.active.and_then(|a| ws.docs.get_mut(a)) else { return };
        let picked = std::mem::take(&mut d.picked);
        let packages = std::mem::take(&mut d.needs_packages);
        let entries = std::mem::take(&mut d.needs_entries);
        for k in picked {
            self.usage.record(&k);
        }
        for p in packages {
            self.ensure_package(&p);
        }
        if !entries.is_empty() {
            self.add_to_project_bib(&entries);
        }
    }

    // ------------------------------------------------------------ citations

    /// Loads the reference library (without showing it). False if it cannot be opened.
    fn ensure_refs(&mut self) -> bool {
        if self.refs.is_none() {
            self.refs = Some(RefsView::new(&self.ctx, &self.cfg));
        }
        self.refs.as_ref().and_then(|r| r.library()).is_some()
    }

    /// Library keys and entry descriptions for completion and hover.
    fn update_cite_cache(&mut self) {
        let stamp = (self.refs.as_ref().map_or(0, |r| r.version() + 1), self.ws.as_ref().map_or(0, |w| w.bib_gen + 1));
        if stamp == self.cite_stamp {
            return;
        }
        self.cite_stamp = stamp;
        let mut keys = Vec::new();
        let mut info = HashMap::new();
        if let Some(lib) = self.refs.as_ref().and_then(|r| r.library()) {
            for v in lib.views() {
                keys.push(v.key.clone());
                info.insert(v.key.to_lowercase(), v.describe());
            }
        }
        // the project's version of an entry is what gets cited
        if let Some(ws) = &self.ws {
            info.extend(ws.bib_info.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        self.lib_keys = keys;
        self.cite_info = info;
    }

    /// A project file from its editor buffer, else from disk.
    fn project_text(&self, rel: &Path) -> Option<String> {
        let ws = self.ws.as_ref()?;
        ws.docs
            .iter()
            .find_map(|d| match d {
                Doc::Text(t) if t.rel == rel => Some(t.text.clone()),
                _ => None,
            })
            .or_else(|| std::fs::read_to_string(ws.project.root.join(rel)).ok())
    }

    /// Inserts texts (byte offsets ascending) into a project file: into the open buffer
    /// (the cursor stays) or on disk (the file is created if needed).
    fn insert_into(&mut self, rel: &Path, ins: &[(usize, String)]) -> Result<(), String> {
        let ctx = self.ctx.clone();
        let current = self.project_text(rel).unwrap_or_default();
        let ws = self.ws.as_mut().ok_or("no project")?;
        if let Some(doc) = ws.docs.iter_mut().find_map(|d| match d {
            Doc::Text(t) if t.rel == rel => Some(t),
            _ => None,
        }) {
            doc.insert_all(&ctx, ins);
            return Ok(());
        }
        let mut t = current;
        for (at, s) in ins.iter().rev() {
            t.insert_str((*at).min(t.len()), s);
        }
        let abs = ws.project.root.join(rel);
        if let Some(dir) = abs.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        lf_core::write_atomic(&abs, t.as_bytes()).map_err(|e| e.to_string())?;
        if ws.project.meta.auto_compile {
            ws.compile_due = Some(Instant::now());
        }
        self.rescan();
        Ok(())
    }

    /// The `.bib` file new entries go to (relative to the project) and whether the document
    /// already uses it. Without one, `refs.bib` next to the main file.
    fn project_bib(&self) -> Option<(PathBuf, bool)> {
        let main = PathBuf::from(self.ws.as_ref()?.project.meta.main.clone()?);
        let text = self.project_text(&main)?;
        let resources: Vec<String> = if main.extension().is_some_and(|e| e == "md") {
            lf_md::front_matter(&text).bibliography.into_iter().collect()
        } else {
            lf_ref::project::bib_resources(&text)
        };
        let dir = main.parent().map(Path::to_path_buf).unwrap_or_default();
        Some(match resources.first() {
            Some(r) => (dir.join(r.trim_start_matches("./")), true),
            None => (dir.join("refs.bib"), false),
        })
    }

    /// Copies library entries the project does not have yet into its `.bib` file.
    fn add_to_project_bib(&mut self, keys: &[String]) {
        if !self.ensure_refs() {
            self.status = "The reference library could not be opened".into();
            return;
        }
        let Some((bib, configured)) = self.project_bib() else {
            self.status = "Set a main file first (right-click it in the file panel)".into();
            return;
        };
        let lib = self.refs.as_ref().and_then(|r| r.library()).unwrap();
        let entries: Vec<lf_ref::Entry> = keys.iter().filter_map(|k| lib.entry(k).cloned()).collect();
        let text = self.project_text(&bib).unwrap_or_default();
        let (add, added) = lf_ref::project::entries_to_append(&text, &entries);
        if !added.is_empty() {
            match self.insert_into(&bib, &[(text.len(), add)]) {
                Ok(()) => self.status = format!("Added {} to {}", added.join(", "), bib.display()),
                Err(e) => {
                    self.status = format!("Could not write {}: {e}", bib.display());
                    return;
                }
            }
            self.refresh_refs(true);
        }
        if !configured && !added.is_empty() {
            self.dialog = Some(Dialog::BibSetup { bib });
        }
    }

    /// Makes the main file use `bib` (biblatex setup, or the Markdown front matter).
    fn set_up_bibliography(&mut self, bib: &Path) {
        let Some(main) = self.ws.as_ref().and_then(|w| w.project.meta.main.clone()).map(PathBuf::from) else { return };
        let Some(text) = self.project_text(&main) else { return };
        let dir = main.parent().unwrap_or(Path::new(""));
        let file = bib.strip_prefix(dir).unwrap_or(bib).to_string_lossy().replace('\\', "/");
        let ins = if main.extension().is_some_and(|e| e == "md") {
            if text.starts_with("---\n") { vec![(4, format!("bibliography: {file}\n"))] } else { vec![(0, format!("---\nbibliography: {file}\n---\n\n"))] }
        } else {
            lf_ref::project::add_bib_resource(&text, &file)
        };
        if ins.is_empty() {
            self.status = format!("No \\begin{{document}} in {} – add \\addbibresource{{{file}}} yourself", main.display());
            return;
        }
        match self.insert_into(&main, &ins) {
            Ok(()) => {
                self.status = format!("{} now uses {file}", main.display());
                self.compile();
            }
            Err(e) => self.status = format!("Could not change {}: {e}", main.display()),
        }
    }

    fn open_cite_dialog(&mut self) {
        let Some(ws) = &self.ws else { return };
        if !matches!(ws.active.and_then(|a| ws.docs.get(a)), Some(Doc::Text(_))) {
            self.status = "Open a .tex or .md file to cite into".into();
            return;
        }
        let cmd = ws.project.meta.cite_command.clone().unwrap_or_else(|| "cite".into());
        self.ensure_refs();
        self.dialog = Some(Dialog::Cite { query: String::new(), searched: None, results: Vec::new(), selected: 0, checked: Vec::new(), cmd });
    }

    /// Keys for the cite dialog: library matches, then project-only entries that match.
    fn cite_search(&self, query: &str) -> Vec<String> {
        let mut out: Vec<String> = self.refs.as_ref().and_then(|r| r.library()).map(|l| l.search(query).into_iter().take(300).map(String::from).collect()).unwrap_or_default();
        if let Some(ws) = &self.ws {
            let words: Vec<String> = query.split_whitespace().map(|w| lf_ref::latex::strip_accents(w).to_lowercase()).collect();
            for k in &ws.bib_keys {
                if out.iter().any(|o| o.eq_ignore_ascii_case(k)) {
                    continue;
                }
                let hay = lf_ref::latex::strip_accents(&format!("{k} {}", ws.bib_info.get(&k.to_lowercase()).map_or("", |s| s.as_str()))).to_lowercase();
                if words.iter().all(|w| hay.contains(w.as_str())) {
                    out.push(k.clone());
                }
            }
        }
        out
    }

    /// Inserts citations into the active document and copies the entries into the project.
    fn cite(&mut self, keys: &[String], cmd: &str) {
        let ctx = self.ctx.clone();
        let Some(ws) = &mut self.ws else { return };
        if ws.project.meta.cite_command.as_deref() != Some(cmd) {
            ws.project.meta.cite_command = Some(cmd.to_string());
            let _ = ws.project.save_meta();
        }
        let Some(Doc::Text(d)) = ws.active.and_then(|a| ws.docs.get_mut(a)) else { return };
        if !d.insert_citation(&ctx, cmd, keys) {
            self.status = "Place the cursor in the editor first".into();
            return;
        }
        self.add_to_project_bib(keys);
    }

    /// Adds `\usepackage{pkg}` to the preamble unless a file of the project
    /// already loads it (or the class does). Works on the open buffer if the
    /// target file is open, otherwise on disk.
    fn ensure_package(&mut self, pkg: &str) {
        let ctx = self.ctx.clone();
        let Some(ws) = &mut self.ws else { return };
        let Some(active) = ws.active.and_then(|a| ws.docs.get(a)).map(|d| d.rel().to_path_buf()) else { return };
        let is_tex = |p: &Path| p.extension().is_some_and(|e| e == "tex" || e == "sty");
        if !is_tex(&active) {
            return;
        }
        let mut paths: Vec<PathBuf> = ws.files.iter().filter(|f| is_tex(f)).cloned().collect();
        if !paths.contains(&active) {
            paths.push(active.clone());
        }
        let texts: Vec<(PathBuf, String)> = paths
            .into_iter()
            .filter_map(|p| {
                let open = ws.docs.iter().find_map(|d| match d {
                    Doc::Text(t) if t.rel == p => Some(t.text.clone()),
                    _ => None,
                });
                let text = open.or_else(|| std::fs::read_to_string(ws.project.root.join(&p)).ok())?;
                Some((p, text))
            })
            .collect();
        let sources: Vec<&str> = texts.iter().map(|t| t.1.as_str()).collect();
        if !lf_tex::packages::needed(&sources, pkg) {
            return;
        }
        // the file with \documentclass being edited, the main file, then a
        // separate preamble file with \usepackage lines
        let main = ws.project.meta.main.clone().map(PathBuf::from);
        let text_of = |p: &Path| texts.iter().find(|t| t.0 == p).map(|t| t.1.as_str());
        let mut order: Vec<&Path> = Vec::new();
        if text_of(&active).is_some_and(|t| t.contains("\\documentclass")) {
            order.push(&active);
        }
        order.extend(main.as_deref());
        order.extend(texts.iter().filter(|t| t.1.contains("\\usepackage")).map(|t| t.0.as_path()));
        let Some((target, at, ins)) =
            order.into_iter().find_map(|p| text_of(p).and_then(|t| lf_tex::packages::insertion(t, pkg)).map(|(at, ins)| (p.to_path_buf(), at, ins)))
        else {
            return;
        };
        let line = format!("\\usepackage{{{pkg}}}");
        let open = ws.docs.iter_mut().find_map(|d| match d {
            Doc::Text(t) if t.rel == target => Some(t),
            _ => None,
        });
        let result = match open {
            Some(doc) => {
                doc.insert_at(&ctx, at, &ins);
                Ok(())
            }
            None => {
                let mut t = text_of(&target).unwrap_or_default().to_string();
                t.insert_str(at, &ins);
                lf_core::write_atomic(&ws.project.root.join(&target), t.as_bytes())
            }
        };
        self.status = match result {
            Ok(()) => format!("Added {line} to {}", target.display()),
            Err(e) => format!("Could not add {line} to {}: {e}", target.display()),
        };
    }

    fn autosave(&mut self) {
        let Some(ws) = &mut self.ws else { return };
        let mut saved = false;
        let mut pending = false;
        for d in &mut ws.docs {
            if let Doc::Text(d) = d
                && d.dirty() && !d.conflict {
                    match d.last_edit {
                        Some(t) if t.elapsed() < AUTOSAVE => pending = true,
                        // never overwrite what another program just wrote
                        _ if d.changed_on_disk() => d.conflict = true,
                        _ => {
                            if d.save().is_ok() {
                                saved = true;
                            }
                        }
                    }
                }
        }
        if saved && ws.project.meta.auto_compile {
            ws.compile_due = Some(Instant::now());
        }
        if pending {
            self.ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    /// Reloads open documents changed by other programs; rescans the tree.
    fn watch(&mut self) {
        let Some(ws) = &mut self.ws else { return };
        if ws.scanned.elapsed() < Duration::from_secs(2) {
            return;
        }
        for d in &mut ws.docs {
            if let Doc::Text(d) = d
                && !d.conflict && d.changed_on_disk() {
                    if d.dirty() {
                        d.conflict = true;
                    } else {
                        let _ = d.reload();
                    }
                }
        }
        self.rescan();
        self.ctx.request_repaint_after(Duration::from_secs(2));
    }

    /// Labels, bibliography keys and outline for completion and the side panel.
    fn refresh_refs(&mut self, force: bool) {
        let Some(ws) = &mut self.ws else { return };
        if !force && ws.refs_at.elapsed() < Duration::from_millis(1500) {
            return;
        }
        ws.refs_at = Instant::now();
        let buffers: Vec<(PathBuf, String)> = ws.docs.iter().filter_map(|d| if let Doc::Text(t) = d { Some((t.rel.clone(), t.text.clone())) } else { None }).collect();
        let root = ws.project.root.clone();
        let read = |p: &Path| -> Option<String> { buffers.iter().find(|(r, _)| r == p).map(|(_, t)| t.clone()).or_else(|| std::fs::read_to_string(root.join(p)).ok()) };
        let mut labels = Vec::new();
        let mut keys = Vec::new();
        let mut info = HashMap::new();
        for f in &ws.files {
            match f.extension().and_then(|e| e.to_str()) {
                Some("tex") => {
                    if let Some(s) = read(f) {
                        labels.extend(lf_tex::outline::labels(&s));
                    }
                }
                Some("bib") => {
                    if let Some(s) = read(f) {
                        let bib = lf_ref::bib::parse(&s);
                        let strings = bib.strings();
                        for e in bib.entries() {
                            info.entry(e.key.to_lowercase()).or_insert_with(|| lf_ref::library::view_of(e, &strings).describe());
                        }
                        keys.extend(bib.keys());
                    }
                }
                _ => {}
            }
        }
        labels.sort();
        labels.dedup();
        keys.sort();
        keys.dedup();
        ws.labels = labels;
        ws.bib_keys = keys;
        if info != ws.bib_info {
            ws.bib_info = info;
            ws.bib_gen += 1;
        }
        ws.outline = match ws.project.main_rel() {
            Some(m) => lf_tex::outline::project_outline(&root, &m, &read),
            None => Vec::new(),
        };
    }

    // ------------------------------------------------------------ compiling

    fn compile(&mut self) {
        self.save_all();
        let Some(ws) = &mut self.ws else { return };
        ws.compile_due = None;
        if ws.compiling.is_some() {
            ws.compile_again = true;
            return;
        }
        let Some(job) = ws.project.compile_job() else {
            self.status = "No main file. Choose one under Build › Main File.".into();
            return;
        };
        // ids are unique across projects: a late message from a closed
        // project can never be taken for the current compile
        self.next_id += 1;
        let seq = self.next_id;
        let cancel = Cancel::default();
        ws.compiling = Some((seq, cancel.clone(), Instant::now()));
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let history = ws.history.clone();
        std::thread::spawn(move || {
            let out = lf_tex::compile::run(&job, &cancel);
            if out.ok {
                let _ = history.snapshot(None, true, "compile");
            }
            let _ = tx.send(Msg::Compiled(seq, Box::new(out)));
            ctx.request_repaint();
        });
        self.ctx.request_repaint();
    }

    fn stop(&mut self) {
        if let Some(ws) = &mut self.ws {
            if let Some((_, c, _)) = &ws.compiling {
                c.cancel();
            }
            ws.compile_again = false;
        }
    }

    fn handle_messages(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Compiled(seq, out) => {
                    let Some(ws) = &mut self.ws else { continue };
                    if ws.compiling.as_ref().is_none_or(|c| c.0 != seq) {
                        continue;
                    }
                    ws.compiling = None;
                    let out = *out;
                    if let Some(pdf) = &out.pdf
                        && !out.cancelled {
                            ws.pdf.load(pdf);
                        }
                    let had_errors = ws.last.as_ref().is_some_and(|l| l.errors() > 0);
                    self.status = if let Some(e) = &out.launch_error {
                        e.clone()
                    } else if out.cancelled {
                        "Compilation stopped.".into()
                    } else if out.pdf.is_none() || out.pages.is_none() {
                        format!("Compilation failed after {:.1} s – see Problems", out.duration.as_secs_f32())
                    } else {
                        let pages = out.pages.map(|p| format!(" · {p} page{}", if p == 1 { "" } else { "s" })).unwrap_or_default();
                        format!("Compiled in {:.1} s{pages}", out.duration.as_secs_f32())
                    };
                    if (out.errors() > 0 && !had_errors) || !out.missing.is_empty() {
                        ws.problems_open = true;
                    }
                    if out.ok {
                        ws.last_snapshot = Instant::now();
                    }
                    ws.last = Some(out);
                    if ws.compile_again {
                        ws.compile_again = false;
                        self.compile();
                    }
                }
                Msg::Forward(hit) => {
                    if let (Some(ws), Some(hit)) = (&mut self.ws, hit) {
                        if ws.view == View::Editor {
                            ws.view = View::Split;
                        }
                        ws.pdf.show_hit(hit);
                    } else {
                        self.status = "No PDF position for this line (compile first).".into();
                    }
                }
                Msg::Inverse(hit) => {
                    let Some((path, line)) = hit else {
                        self.status = "No source position here.".into();
                        continue;
                    };
                    let Some(ws) = &self.ws else { continue };
                    let root = ws.project.root.clone();
                    let abs = if path.is_absolute() { path } else { root.join(path) };
                    let abs = abs.canonicalize().unwrap_or(abs);
                    match abs.strip_prefix(&root) {
                        Ok(rel) => {
                            let rel = rel.to_path_buf();
                            self.open_at(&rel, line);
                        }
                        Err(_) => self.status = format!("{}:{line} is outside the project", abs.display()),
                    }
                }
            }
        }
    }

    fn forward_sync(&mut self, line: u32, col: u32) {
        let Some(ws) = &self.ws else { return };
        let (Some(pdf), Some(Doc::Text(d))) = (ws.pdf.source.clone(), ws.active.and_then(|a| ws.docs.get(a))) else {
            self.status = "Compile first to jump into the PDF.".into();
            return;
        };
        let input = d.abs.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Forward(lf_tex::synctex::forward(&pdf, &input, line, col.saturating_sub(1))));
            ctx.request_repaint();
        });
    }

    fn inverse_sync(&mut self, page: u32, x: f32, y: f32) {
        let Some(pdf) = self.ws.as_ref().and_then(|w| w.pdf.source.clone()) else { return };
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Inverse(lf_tex::synctex::inverse(&pdf, page, x, y)));
            ctx.request_repaint();
        });
    }

    // ------------------------------------------------------------ frame

    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.handle_messages();
        self.autosave();
        self.watch();
        self.refresh_refs(false);
        if let Some(r) = &mut self.refs
            && !self.refs_open
        {
            r.poll();
            if let Some(s) = r.status.take() {
                self.status = s;
            }
        }
        self.update_cite_cache();
        if let Some(ws) = &mut self.ws
            && ws.compile_due.is_some_and(|t| t.elapsed() > Duration::from_millis(300)) {
                self.compile();
            }
        self.periodic_snapshot();
        let pal = theme::palette(&ctx);
        self.shortcuts(&ctx);
        self.dropped_files(&ctx);
        self.update_title(&ctx);

        egui::Panel::top("top").frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(10, 6))).show(ui, |ui| self.top_bar(ui, &pal));
        egui::Panel::bottom("status").frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::symmetric(10, 3))).show(ui, |ui| self.status_bar(ui, &pal));
        if self.refs_open
            && let Some(r) = &mut self.refs
        {
            egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.bg)).show(ui, |ui| r.ui(ui, &pal));
            if let Some(s) = r.status.take() {
                self.status = s;
            }
        } else if self.ws.is_some() {
            self.workspace(ui, &pal);
        } else {
            egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.bg).inner_margin(20)).show(ui, |ui| self.dashboard(ui, &pal));
        }
        self.dialogs(&ctx, &pal);
        if let Some(g) = &mut self.gallery {
            match g.ui(&ctx, &pal, &mut self.previews) {
                Some(GalleryResult::Created(p)) => {
                    self.cfg.author = g.author.clone();
                    self.gallery = None;
                    self.open_project(&p);
                }
                Some(GalleryResult::Closed) => self.gallery = None,
                None => {}
            }
        }
        if let Some(m) = &mut self.manager {
            match m.ui(&ctx, &pal, &mut self.previews) {
                Some(ManagerResult::Close) => self.manager = None,
                Some(ManagerResult::Hidden(h)) => {
                    self.cfg.hidden_templates = h;
                    let _ = self.cfg.save();
                }
                None => {}
            }
        }
        if self.ws.as_ref().is_some_and(|w| w.compiling.is_some()) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn periodic_snapshot(&mut self) {
        let Some(ws) = &mut self.ws else { return };
        if ws.last_snapshot.elapsed() < SNAPSHOT_EVERY {
            return;
        }
        ws.last_snapshot = Instant::now();
        let h = ws.history.clone();
        std::thread::spawn(move || {
            let _ = h.snapshot(None, true, "timer");
        });
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let t = match &self.ws {
            Some(ws) => format!("{} — arvidleaf", ws.project.name()),
            None => "arvidleaf".into(),
        };
        if t != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(t.clone()));
            self.title = t;
        }
    }

    fn dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if dropped.is_empty() {
            return;
        }
        if self.refs_open
            && let Some(r) = &mut self.refs
        {
            r.import_paths(&dropped);
            return;
        }
        if self.ws.is_none() {
            if let Some(p) = dropped.first() {
                self.open_path(p);
            }
            return;
        }
        let ws = self.ws.as_mut().unwrap();
        let dest = ws.project.root.join(&ws.selected_dir);
        let mut copied = Vec::new();
        for p in dropped {
            if p.starts_with(&ws.project.root) {
                continue;
            }
            match project::copy_into(&p, &dest) {
                Ok(n) => copied.push(n),
                Err(e) => self.status = format!("Could not add {}: {e}", p.display()),
            }
        }
        if copied.is_empty() {
            return;
        }
        self.status = format!("Added {} file{} to the project", copied.len(), if copied.len() == 1 { "" } else { "s" });
        // one image dropped while editing: insert a figure at the cursor
        let ws = self.ws.as_mut().unwrap();
        if copied.len() == 1 && project::is_image(&copied[0]) {
            let rel = copied[0].strip_prefix(&ws.project.root).unwrap_or(&copied[0]).to_string_lossy().to_string();
            ctx.copy_text(format!(
                "\\begin{{figure}}[htbp]\n  \\centering\n  \\includegraphics[width=0.8\\linewidth]{{{rel}}}\n  \\caption{{}}\n  \\label{{fig:}}\n\\end{{figure}}"
            ));
            self.status = format!("Added {rel} – a figure snippet is on the clipboard (Ctrl+V)");
        }
        self.rescan();
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let i = ctx.input(|i| {
            let c = i.modifiers.command;
            let s = i.modifiers.shift;
            (
                c && !s && i.key_pressed(Key::S),
                c && i.key_pressed(Key::Enter),
                c && i.key_pressed(Key::Period),
                c && !s && i.key_pressed(Key::P),
                c && s && i.key_pressed(Key::F),
                c && s && i.key_pressed(Key::N),
                c && !s && i.key_pressed(Key::O),
                c && i.key_pressed(Key::H),
                c && i.key_pressed(Key::W),
            )
        });
        let (save, build, stop, quick, search, new_project, open, history, close) = i;
        let help = ctx.input(|i| i.key_pressed(Key::F1) || i.modifiers.command && i.key_pressed(Key::Questionmark));
        if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::R)) {
            self.toggle_refs();
        }
        if help {
            self.dialog = match self.dialog {
                Some(Dialog::Shortcuts { .. }) => None,
                _ => Some(Dialog::Shortcuts { query: String::new() }),
            };
        }
        if new_project {
            self.gallery = Some(Gallery::new(&self.cfg));
        }
        if open {
            self.open_dialog();
        }
        if self.ws.is_none() {
            return;
        }
        if save {
            let n = self.save_all();
            self.status = format!("Saved {n} file{}", if n == 1 { "" } else { "s" });
            self.compile();
        }
        if build {
            self.compile();
        }
        if stop {
            self.stop();
        }
        if quick {
            self.dialog = Some(Dialog::QuickOpen { query: String::new(), selected: 0 });
        }
        if search {
            self.dialog = Some(Dialog::Search { query: String::new(), results: Vec::new(), searched: false });
        }
        if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::C)) {
            self.open_cite_dialog();
        }
        if history {
            self.toggle_history();
        }
        if close
            && let Some(a) = self.ws.as_ref().and_then(|w| w.active) {
                self.close_doc(a);
            }
        // zoom the PDF with Ctrl +/- when the editor does not have focus
        let (zin, zout) = ctx.input(|i| (i.modifiers.command && i.key_pressed(Key::Plus) || i.modifiers.command && i.key_pressed(Key::Equals), i.modifiers.command && i.key_pressed(Key::Minus)));
        if let Some(ws) = &mut self.ws {
            if zin {
                ws.pdf.zoom_by(1.15);
            }
            if zout {
                ws.pdf.zoom_by(1.0 / 1.15);
            }
        }
    }

    /// Ctrl+Click in the editor: opens the label, bibliography entry or file.
    fn follow_link(&mut self, link: Link) {
        let Some(ws) = &self.ws else { return };
        let current_dir = ws.active.and_then(|a| ws.docs.get(a)).and_then(|d| d.rel().parent().map(Path::to_path_buf)).unwrap_or_default();
        match link {
            Link::Label(k) => {
                let needle = format!("\\label{{{k}}}");
                match self.find_line(&["tex", "md"], |l| lf_tex::outline::strip_comment(l).contains(&needle)) {
                    Some((rel, line)) => self.open_at(&rel, line),
                    None => self.status = format!("No \\label{{{k}}} in the project"),
                }
            }
            Link::Citation(k) => match self.find_in(&["bib"], |text| lf_ref::bib::parse(text).entry_line(&k)) {
                Some((rel, line)) => self.open_at(&rel, line),
                None => self.status = format!("No entry \"{k}\" in the project's .bib files"),
            },
            Link::File(f) => {
                let f = f.trim_start_matches("./");
                let exts = ["", ".tex", ".bib", ".md", ".png", ".jpg", ".jpeg", ".pdf"];
                let found = [current_dir.join(f), PathBuf::from(f)]
                    .into_iter()
                    .flat_map(|base| exts.iter().map(move |e| PathBuf::from(format!("{}{e}", base.display()))))
                    .find(|p| ws.files.iter().any(|x| x == p));
                match found {
                    Some(rel) => self.open_file(&rel),
                    None => self.status = format!("File not found: {f}"),
                }
            }
        }
    }

    /// First line (1-based) in a project file with one of `exts` for which
    /// `hit` is true. Open files are searched in their editor buffer.
    fn find_line(&self, exts: &[&str], hit: impl Fn(&str) -> bool) -> Option<(PathBuf, u32)> {
        self.find_in(exts, |text| text.lines().position(&hit).map(|i| i + 1))
    }

    /// First project file (editor buffer or disk) with one of `exts` where `find` returns a
    /// 1-based line.
    fn find_in(&self, exts: &[&str], find: impl Fn(&str) -> Option<usize>) -> Option<(PathBuf, u32)> {
        let ws = self.ws.as_ref()?;
        for rel in &ws.files {
            if !rel.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(&e)) {
                continue;
            }
            let open = ws.docs.iter().find_map(|d| match d {
                Doc::Text(t) if &t.rel == rel => Some(t.text.clone()),
                _ => None,
            });
            let Some(text) = open.or_else(|| std::fs::read_to_string(ws.project.root.join(rel)).ok()) else { continue };
            if let Some(line) = find(&text) {
                return Some((rel.clone(), line as u32));
            }
        }
        None
    }

    fn toggle_history(&mut self) {
        if let Some(ws) = &mut self.ws {
            ws.history_view = if ws.history_view.is_some() { None } else { Some(HistoryView::new(&ws.history)) };
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().set_directory(self.cfg.projects_dir()).pick_folder() {
            self.open_project(&p);
        }
    }

    // ------------------------------------------------------------ bars

    fn top_bar(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        egui::MenuBar::new().ui(ui, |ui| {
            let (r, _) = ui.allocate_exact_size(vec2(20.0, 20.0), egui::Sense::hover());
            theme::logo(ui.painter(), r);
            ui.label(RichText::new("arvidleaf").strong().color(pal.accent2).size(15.0));
            ui.add_space(8.0);
            self.file_menu(ui);
            let has = self.ws.is_some();
            ui.add_enabled_ui(has, |ui| {
                ui.menu_button("Edit", |ui| {
                    if ui.button("Find in Project…    Ctrl+Shift+F").clicked() {
                        self.dialog = Some(Dialog::Search { query: String::new(), results: Vec::new(), searched: false });
                        ui.close();
                    }
                    if ui.button("Go to File…    Ctrl+P").clicked() {
                        self.dialog = Some(Dialog::QuickOpen { query: String::new(), selected: 0 });
                        ui.close();
                    }
                    if ui.button("Insert Citation…    Ctrl+Shift+C").clicked() {
                        self.open_cite_dialog();
                        ui.close();
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.cfg.vim, "Vim Mode").on_hover_text("Vim key bindings in the editor (normal, insert, visual mode; :w saves)").changed() {
                        let _ = self.cfg.save();
                        self.status = if self.cfg.vim { "Vim mode on – Esc for normal mode, i to insert".into() } else { "Vim mode off".into() };
                        ui.close();
                    }
                    if ui.button("Keyboard Shortcuts…    F1").clicked() {
                        self.dialog = Some(Dialog::Shortcuts { query: String::new() });
                        ui.close();
                    }
                });
            });
            ui.menu_button("View", |ui| {
                if ui.checkbox(&mut self.cfg.dark, "Dark theme").changed() {
                    theme::apply(ui.ctx(), self.cfg.dark);
                    let _ = self.cfg.save();
                }
                if ui.selectable_label(self.refs_open, "References    Ctrl+Shift+R").clicked() {
                    self.toggle_refs();
                    ui.close();
                }
                ui.separator();
                ui.checkbox(&mut self.cfg.show_outline, "Files & outline panel");
                if let Some(ws) = &mut self.ws {
                    ui.checkbox(&mut ws.problems_open, "Problems panel");
                    ui.separator();
                    ui.radio_value(&mut ws.view, View::Split, "Editor + PDF");
                    ui.radio_value(&mut ws.view, View::Editor, "Editor only");
                    ui.radio_value(&mut ws.view, View::Pdf, "PDF only");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Editor font size");
                    if ui.add(egui::DragValue::new(&mut self.cfg.font_size).range(9.0..=28.0).speed(0.1)).changed() {
                        let _ = self.cfg.save();
                    }
                });
            });
            ui.menu_button("Templates", |ui| {
                if ui.button("New Project from Template…    Ctrl+Shift+N").clicked() {
                    self.gallery = Some(Gallery::new(&self.cfg));
                    ui.close();
                }
                if ui.button("Manage Templates…").clicked() {
                    self.manager = Some(Manager::new(&self.cfg, self.ws.as_ref().map(|w| w.project.root.clone())));
                    ui.close();
                }
                if ui.add_enabled(has, egui::Button::new("Save Project as Template…")).clicked() {
                    let name = self.ws.as_ref().map(|w| w.project.name()).unwrap_or_default();
                    self.dialog = Some(Dialog::SaveTemplate { name, category: "My Templates".into(), description: String::new(), error: None });
                    ui.close();
                }
                if ui.button("Open Templates Folder").clicked() {
                    let d = lf_core::templates::user_dir();
                    let _ = std::fs::create_dir_all(&d);
                    open_external(&d);
                    ui.close();
                }
            });
            self.references_menu(ui);
            ui.add_enabled_ui(has, |ui| self.build_menu(ui));
            ui.add_enabled_ui(has, |ui| {
                ui.menu_button("History", |ui| {
                    if ui.button("Show History    Ctrl+H").clicked() {
                        self.toggle_history();
                        ui.close();
                    }
                    if ui.button("Take Snapshot Now").clicked() {
                        if let Some(ws) = &self.ws {
                            self.status = match ws.history.snapshot(None, false, "manual") {
                                Ok(Some(_)) => "Snapshot saved.".into(),
                                Ok(None) => "Nothing changed since the last snapshot.".into(),
                                Err(e) => format!("Snapshot failed: {e}"),
                            };
                        }
                        ui.close();
                    }
                });
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Keyboard Shortcuts    F1").clicked() {
                    self.dialog = Some(Dialog::Shortcuts { query: String::new() });
                    ui.close();
                }
                if ui.button("System Check").clicked() {
                    self.tools = lf_tex::tools::check();
                    self.dialog = Some(Dialog::SystemCheck);
                    ui.close();
                }
                if ui.button("About arvidleaf").clicked() {
                    self.dialog = Some(Dialog::About);
                    ui.close();
                }
            });

            // right side: compile controls
            if let Some(ws) = &mut self.ws {
                ui.separator();
                let mut do_compile = false;
                let mut do_stop = false;
                if ws.compiling.is_some() {
                    if ui.button(RichText::new("⏹ Stop").color(pal.error)).on_hover_text("Ctrl+.").clicked() {
                        do_stop = true;
                    }
                } else if ui.button(RichText::new("▶ Compile").color(pal.ok).strong()).on_hover_text("Ctrl+Enter (Ctrl+S saves and compiles)").clicked() {
                    do_compile = true;
                }
                let mut engine = ws.project.meta.engine;
                egui::ComboBox::from_id_salt("engine").selected_text(engine.label()).width(90.0).show_ui(ui, |ui| {
                    for e in Engine::ALL {
                        ui.selectable_value(&mut engine, e, e.label());
                    }
                });
                if engine != ws.project.meta.engine {
                    ws.project.meta.engine = engine;
                    let _ = ws.project.save_meta();
                    do_compile = true;
                }
                if ui.selectable_label(ws.project.meta.auto_compile, "⟳ Auto").on_hover_text("Compile automatically after each save").clicked() {
                    ws.project.meta.auto_compile = !ws.project.meta.auto_compile;
                    let _ = ws.project.save_meta();
                }
                ui.separator();
                for (v, sym, tip) in [(View::Editor, "Code", "Editor only"), (View::Split, "Split", "Editor + PDF"), (View::Pdf, "PDF", "PDF only")] {
                    if ui.selectable_label(ws.view == v, sym).on_hover_text(tip).clicked() {
                        ws.view = v;
                    }
                }
                if ui.selectable_label(ws.history_view.is_some(), "🕘 History").on_hover_text("Ctrl+H").clicked() {
                    ws.history_view = if ws.history_view.is_some() { None } else { Some(HistoryView::new(&ws.history)) };
                }
                if do_compile {
                    self.compile();
                }
                if do_stop {
                    self.stop();
                }
            }
        });
    }

    fn references_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("References", |ui| {
            let label = if self.refs_open { "Hide Library    Ctrl+Shift+R" } else { "Show Library    Ctrl+Shift+R" };
            if ui.button(label).clicked() {
                self.toggle_refs();
                ui.close();
            }
            if ui.add_enabled(self.ws.is_some(), egui::Button::new("Insert Citation…    Ctrl+Shift+C")).clicked() {
                self.refs_open = false;
                self.open_cite_dialog();
                ui.close();
            }
            if ui.button("Import from File…").on_hover_text("PDFs, .bib or CSL-JSON (e.g. a Zotero export)").clicked() {
                ui.close();
                if let Some(files) = rfd::FileDialog::new().add_filter("PDF, BibTeX, CSL-JSON", &["pdf", "bib", "json", "txt"]).pick_files() {
                    if !self.refs_open {
                        self.toggle_refs();
                    }
                    if let Some(r) = &mut self.refs {
                        r.import_paths(&files);
                    }
                }
            }
            ui.separator();
            let dir = self.cfg.library_dir();
            ui.label(RichText::new(format!("Library: {}", dir.display())).small().weak());
            if ui.button("Open Library Folder").clicked() {
                let _ = std::fs::create_dir_all(&dir);
                open_external(&dir);
                ui.close();
            }
            if ui.button("Change Library Location…").on_hover_text("A folder with library.bib – e.g. in a synced or git folder").clicked() {
                ui.close();
                if let Some(p) = rfd::FileDialog::new().set_directory(&dir).pick_folder() {
                    self.cfg.library_dir = Some(p.clone());
                    let _ = self.cfg.save();
                    if let Some(r) = &mut self.refs {
                        r.open(&p);
                    }
                    self.status = format!("Library: {}", p.display());
                }
            }
            ui.separator();
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Key pattern");
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut self.cfg.key_pattern).hint_text(lf_ref::key::DEFAULT_PATTERN).desired_width(170.0))
                    .on_hover_text("[auth] [Auth] [authors2] [year] [title] [Title] [shorttitle], other text is copied")
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Crossref contact");
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut self.cfg.ref_mailto).hint_text("optional e-mail").desired_width(150.0))
                    .on_hover_text("Sent with DOI lookups (Crossref's \"polite pool\"). Empty: nothing is sent.")
                    .changed();
            });
            if changed {
                let _ = self.cfg.save();
                if let Some(r) = &mut self.refs {
                    r.configure(&self.cfg);
                }
            }
        });
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui.button("New Project…    Ctrl+Shift+N").clicked() {
                self.gallery = Some(Gallery::new(&self.cfg));
                ui.close();
            }
            if ui.button("Open Project Folder…    Ctrl+O").clicked() {
                ui.close();
                self.open_dialog();
            }
            if ui.button("Import ZIP…").on_hover_text("e.g. an Overleaf \"Download Source\" archive").clicked() {
                ui.close();
                if let Some(p) = rfd::FileDialog::new().add_filter("ZIP archive", &["zip"]).pick_file() {
                    self.import_zip(&p);
                }
            }
            ui.menu_button("Recent Projects", |ui| {
                let recent = self.cfg.recent.clone();
                if recent.is_empty() {
                    ui.label("none");
                }
                for p in recent {
                    if ui.button(p.display().to_string()).clicked() {
                        ui.close();
                        self.open_project(&p);
                    }
                }
            });
            let has = self.ws.is_some();
            ui.separator();
            if ui.add_enabled(has, egui::Button::new("New File…")).clicked() {
                let dir = self.ws.as_ref().map(|w| w.selected_dir.clone()).unwrap_or_default();
                self.dialog = Some(Dialog::NewEntry { dir, name: String::new(), folder: false, error: None });
                ui.close();
            }
            if ui.add_enabled(has, egui::Button::new("New Folder…")).clicked() {
                let dir = self.ws.as_ref().map(|w| w.selected_dir.clone()).unwrap_or_default();
                self.dialog = Some(Dialog::NewEntry { dir, name: String::new(), folder: true, error: None });
                ui.close();
            }
            if ui.add_enabled(has, egui::Button::new("Add Files…")).on_hover_text("Copy images, .bib, .sty … into the project (or drag them onto the window)").clicked() {
                ui.close();
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    if let Some(ws) = &self.ws {
                        let dest = ws.project.root.join(&ws.selected_dir);
                        for f in files {
                            if let Err(e) = project::copy_into(&f, &dest) {
                                self.status = format!("Could not add {}: {e}", f.display());
                            }
                        }
                    }
                    self.rescan();
                }
            }
            if ui.add_enabled(has, egui::Button::new("Save All    Ctrl+S")).clicked() {
                self.save_all();
                ui.close();
            }
            ui.separator();
            let pdf = self.ws.as_ref().and_then(|w| w.pdf.source.clone());
            if ui.add_enabled(pdf.is_some(), egui::Button::new("Export PDF…")).clicked() {
                ui.close();
                let name = self.ws.as_ref().map(|w| format!("{}.pdf", w.project.name())).unwrap_or_default();
                if let (Some(src), Some(dst)) = (pdf, rfd::FileDialog::new().set_file_name(&name).add_filter("PDF", &["pdf"]).save_file()) {
                    self.status = match std::fs::copy(&src, &dst) {
                        Ok(_) => format!("Saved {}", dst.display()),
                        Err(e) => format!("Export failed: {e}"),
                    };
                }
            }
            if ui.add_enabled(has, egui::Button::new("Export ZIP…")).clicked() {
                ui.close();
                if let Some(ws) = &self.ws {
                    let name = format!("{}.zip", ws.project.name());
                    if let Some(dst) = rfd::FileDialog::new().set_file_name(&name).add_filter("ZIP", &["zip"]).save_file() {
                        self.status = match lf_core::archive::export_zip(&ws.project.root, &dst) {
                            Ok(n) => format!("Exported {n} files to {}", dst.display()),
                            Err(e) => format!("Export failed: {e}"),
                        };
                    }
                }
            }
            if ui.add_enabled(has, egui::Button::new("Show in File Manager")).clicked() {
                if let Some(ws) = &self.ws {
                    open_external(&ws.project.root);
                }
                ui.close();
            }
            ui.separator();
            if ui.add_enabled(has, egui::Button::new("Close Project")).clicked() {
                self.close_project();
                ui.close();
            }
            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn build_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Build", |ui| {
            if ui.button("Compile    Ctrl+Enter").clicked() {
                self.compile();
                ui.close();
            }
            if ui.button("Stop    Ctrl+.").clicked() {
                self.stop();
                ui.close();
            }
            let Some(ws) = &mut self.ws else { return };
            ui.separator();
            let mut changed = false;
            changed |= ui.checkbox(&mut ws.project.meta.auto_compile, "Compile after saving").changed();
            ui.menu_button("Engine", |ui| {
                for e in Engine::ALL {
                    changed |= ui.radio_value(&mut ws.project.meta.engine, e, e.label()).changed();
                }
            });
            ui.menu_button("Main File", |ui| {
                let texs: Vec<String> = ws.files.iter().filter(|f| f.extension().is_some_and(|e| e == "tex")).map(|f| f.to_string_lossy().to_string()).collect();
                for t in texs {
                    let sel = ws.project.meta.main.as_ref() == Some(&t);
                    if ui.radio(sel, &t).clicked() {
                        ws.project.meta.main = Some(t.clone());
                        changed = true;
                    }
                }
            });
            changed |= ui
                .checkbox(&mut ws.project.meta.shell_escape, "Allow shell escape")
                .on_hover_text("Needed by minted, svg, gnuplottex … Only enable for projects you trust.")
                .changed();
            ui.separator();
            if ui.button("Clean Build Files").on_hover_text("Delete .arvidleaf/build (aux, log, pdf)").clicked() {
                let _ = lf_tex::compile::clean(&ws.project.build_dir());
                ws.pdf.clear();
                self.status = "Build files removed.".into();
                ui.close();
            }
            if ui.button("Open Build Folder").clicked() {
                open_external(&ws.project.build_dir());
                ui.close();
            }
            if changed {
                let _ = ws.project.save_meta();
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.horizontal(|ui| {
            let (dot, txt) = match &self.ws {
                Some(ws) if ws.compiling.is_some() => {
                    let t = ws.compiling.as_ref().unwrap().2.elapsed().as_secs_f32();
                    let pulse = 0.55 + 0.45 * (t * 4.0).sin().abs();
                    (pal.accent.gamma_multiply(pulse), format!("Compiling… {t:.0} s"))
                }
                Some(ws) => match &ws.last {
                    Some(l) if l.errors() > 0 => (pal.error, format!("{} error{}", l.errors(), if l.errors() == 1 { "" } else { "s" })),
                    Some(l) if l.warnings() > 0 => (pal.warn, format!("{} warning{}", l.warnings(), if l.warnings() == 1 { "" } else { "s" })),
                    Some(l) if l.ok => (pal.ok, "OK".into()),
                    Some(_) => (pal.warn, "Not compiled".into()),
                    None => (pal.dim, "Not compiled yet".into()),
                },
                None => (pal.ok, "Ready".into()),
            };
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, dot);
            let lbl = ui.add(egui::Label::new(RichText::new(txt).small()).sense(egui::Sense::click()));
            if let Some(ws) = &mut self.ws
                && lbl.on_hover_text("Show problems").clicked() {
                    ws.problems_open = !ws.problems_open;
                }
            ui.separator();
            ui.label(RichText::new(&self.status).small().color(pal.dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(ws) = &self.ws {
                    if let Some(Doc::Text(d)) = ws.active.and_then(|a| ws.docs.get(a)) {
                        ui.label(RichText::new(format!("Ln {}, Col {}", d.cursor_line, d.cursor_col)).small().color(pal.dim));
                        ui.separator();
                    }
                    let main = ws.project.meta.main.clone().unwrap_or_else(|| "no main file".into());
                    ui.label(RichText::new(format!("{} · {}", main, ws.project.meta.engine.label())).small().color(pal.dim));
                    if let Some(ms) = ws.pdf.first_page_ms {
                        ui.separator();
                        ui.label(RichText::new(format!("preview {ms} ms")).small().color(pal.dim)).on_hover_text("Time from finished compile to the first visible page");
                    }
                }
            });
        });
    }

    // ------------------------------------------------------------ start screen

    fn dashboard(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.08).min(60.0));
            let (r, _) = ui.allocate_exact_size(vec2(72.0, 72.0), egui::Sense::hover());
            theme::logo(ui.painter(), r);
            ui.label(RichText::new("arvidleaf").size(30.0).strong().color(pal.text));
            ui.label(RichText::new("Local LaTeX editor").color(pal.dim));
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let w = 3.0 * 170.0 + 2.0 * 10.0;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                let big = |t: &str| egui::Button::new(RichText::new(t).size(15.0)).min_size(vec2(170.0, 40.0));
                if ui.add(big("➕ New Project")).on_hover_text("From a template (Ctrl+Shift+N)").clicked() {
                    self.gallery = Some(Gallery::new(&self.cfg));
                }
                if ui.add(big("📂 Open Folder")).on_hover_text("Ctrl+O").clicked() {
                    self.open_dialog();
                }
                if ui.add(big("📦 Import ZIP")).on_hover_text("e.g. an Overleaf download").clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter("ZIP archive", &["zip"]).pick_file() {
                        self.import_zip(&p);
                    }
            });
            ui.add_space(24.0);
            let missing: Vec<&lf_tex::tools::Tool> = self.tools.iter().filter(|t| !t.found).collect();
            if !missing.is_empty() {
                egui::Frame::new().fill(pal.warn.gamma_multiply(0.12)).corner_radius(8).inner_margin(10).show(ui, |ui| {
                    ui.set_max_width(520.0);
                    for t in missing {
                        ui.label(RichText::new(format!("⚠ {} not found – {}", t.name, t.purpose)).color(if t.required { pal.error } else { pal.warn }));
                    }
                });
                ui.add_space(12.0);
            }
            ui.label(RichText::new("Recent projects").strong());
            ui.add_space(4.0);
            let recent: Vec<PathBuf> = self.cfg.recent.iter().filter(|p| p.is_dir()).cloned().collect();
            if recent.is_empty() {
                ui.label(RichText::new("No projects yet – create one from a template or drop a folder or ZIP onto this window.").color(pal.dim));
            }
            let mut open = None;
            let mut forget = None;
            egui::Frame::new().fill(pal.panel).corner_radius(8).inner_margin(6).show(ui, |ui| {
                ui.set_max_width(560.0);
                for p in recent.iter().take(12) {
                    ui.horizontal(|ui| {
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let r = ui.add(egui::Button::new(RichText::new(format!("📁 {name}")).strong()).frame(false).min_size(vec2(160.0, 22.0)));
                        if r.clicked() {
                            open = Some(p.clone());
                        }
                        ui.label(RichText::new(p.display().to_string()).small().color(pal.dim));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text("Remove from list").clicked() {
                                forget = Some(p.clone());
                            }
                        });
                    });
                }
            });
            if let Some(p) = open {
                self.open_project(&p);
            }
            if let Some(p) = forget {
                self.cfg.recent.retain(|r| r != &p);
                let _ = self.cfg.save();
            }
        });
    }

    // ------------------------------------------------------------ workspace

    fn workspace(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let show_left = self.cfg.show_outline;
        if show_left {
            egui::Panel::left("left").resizable(true).default_size(250.0).frame(egui::Frame::new().fill(pal.panel).inner_margin(8)).show(ui, |ui| self.left_panel(ui, pal));
        }
        let problems_open = self.ws.as_ref().is_some_and(|w| w.problems_open);
        if problems_open {
            egui::Panel::bottom("problems").resizable(true).default_size(180.0).min_size(80.0).frame(egui::Frame::new().fill(pal.panel).inner_margin(8)).show(ui, |ui| self.problems_panel(ui, pal));
        }
        let ws = self.ws.as_mut().unwrap();
        if ws.history_view.is_some() {
            let h = ws.history.clone();
            let mut act = None;
            egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.bg).inner_margin(10)).show(ui, |ui| {
                act = ws.history_view.as_mut().unwrap().ui(ui, pal, &h);
            });
            match act {
                Some(HistoryAction::Close) => ws.history_view = None,
                Some(HistoryAction::Restored) => {
                    for d in &mut ws.docs {
                        if let Doc::Text(d) = d
                            && d.abs.is_file() {
                                let _ = d.reload();
                            }
                    }
                    ws.docs.retain(|d| match d {
                        Doc::Text(t) => t.abs.is_file(),
                        Doc::Image(i) => i.abs.is_file(),
                    });
                    ws.active = (!ws.docs.is_empty()).then_some(0).and(ws.active.map(|a| a.min(ws.docs.len().saturating_sub(1))));
                    self.rescan();
                    self.compile();
                }
                None => {}
            }
            return;
        }
        let view = ws.view;
        let mut editor_action = None;
        let mut pdf_action = None;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.bg).inner_margin(6)).show(ui, |ui| {
            if view == View::Split {
                let w = ui.available_width();
                egui::Panel::right("pdf")
                    .resizable(true)
                    .default_size(w * 0.5)
                    .min_size(200.0)
                    .max_size(w - 200.0)
                    .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin { left: 6, ..Default::default() }))
                    .show(ui, |ui| pdf_action = self.pdf_pane(ui, pal));
            }
            if view == View::Pdf {
                pdf_action = self.pdf_pane(ui, pal);
            } else {
                editor_action = self.editor_pane(ui, pal);
                self.after_editor();
            }
        });
        match editor_action {
            Some(EditorAction::SyncToPdf(l, c)) => self.forward_sync(l, c),
            Some(EditorAction::Follow(link)) => self.follow_link(link),
            Some(EditorAction::Save) => {
                // `:w` behaves like Ctrl+S
                let n = self.save_all();
                self.status = format!("Saved {n} file{}", if n == 1 { "" } else { "s" });
                self.compile();
            }
            None => {}
        }
        if let Some(PdfAction::Inverse(p, x, y)) = pdf_action {
            self.inverse_sync(p, x, y);
        }
    }

    fn pdf_pane(&mut self, ui: &mut egui::Ui, pal: &Palette) -> Option<PdfAction> {
        let ws = self.ws.as_mut()?;
        let compiling = ws.compiling.is_some();
        ws.pdf.toolbar(ui, pal);
        egui::Frame::new().fill(pal.canvas).corner_radius(6).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ws.pdf.ui(ui, pal, compiling)
        })
        .inner
    }

    fn editor_pane(&mut self, ui: &mut egui::Ui, pal: &Palette) -> Option<EditorAction> {
        let font_size = self.cfg.font_size;
        let vim = self.cfg.vim;
        let ws = self.ws.as_mut()?;
        // tabs
        let mut close = None;
        let mut activate = None;
        ui.horizontal_wrapped(|ui| {
            for (i, d) in ws.docs.iter().enumerate() {
                let active = ws.active == Some(i);
                let (title, dirty) = match d {
                    Doc::Text(t) => (t.title(), t.dirty()),
                    Doc::Image(im) => (im.rel.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), false),
                };
                let is_main = ws.project.meta.main.as_deref() == Some(&d.rel().to_string_lossy());
                let frame = egui::Frame::new()
                    .fill(if active { pal.panel } else { pal.bg })
                    .stroke(egui::Stroke::new(1.0, if active { pal.border } else { Color32::TRANSPARENT }))
                    .corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 })
                    .inner_margin(egui::Margin::symmetric(8, 4));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut t = RichText::new(format!("{}{}", if is_main { "★ " } else { "" }, title));
                        if active {
                            t = t.strong();
                        } else {
                            t = t.color(pal.dim);
                        }
                        let r = ui.add(egui::Label::new(t).sense(egui::Sense::click()));
                        if r.clicked() {
                            activate = Some(i);
                        }
                        if r.middle_clicked() {
                            close = Some(i);
                        }
                        r.on_hover_text(d.rel().display().to_string());
                        let x = if dirty { "•" } else { "×" };
                        if ui.add(egui::Button::new(RichText::new(x).small()).frame(false)).on_hover_text("Close (Ctrl+W)").clicked() {
                            close = Some(i);
                        }
                    });
                });
            }
        });
        if let Some(i) = activate {
            ws.active = Some(i);
        }
        if let Some(i) = close {
            self.close_doc(i);
            return None;
        }
        let ws = self.ws.as_mut()?;
        let Some(a) = ws.active else {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new("Open a file from the project panel (Ctrl+P)").color(pal.dim));
            });
            return None;
        };
        let main = ws.project.meta.main.clone();
        let entries: Vec<LogEntry> = ws.last.as_ref().map(|l| l.entries.clone()).unwrap_or_default();
        let files: Vec<String> = ws.files.iter().map(|f| f.to_string_lossy().to_string()).collect();
        let ctx = ui.ctx().clone();
        let usage = &self.usage;
        match &mut ws.docs[a] {
            Doc::Text(d) => {
                let rel = d.rel.to_string_lossy().to_string();
                let problems = entries
                    .iter()
                    .filter(|e| e.line.is_some())
                    .filter(|e| match &e.file {
                        Some(f) => f == &rel || f.ends_with(&format!("/{rel}")),
                        None => main.as_deref() == Some(rel.as_str()),
                    })
                    .map(|e| (e.line.unwrap(), e.level, e.message.clone()))
                    .collect();
                let env = EditorEnv { problems, labels: &ws.labels, bib_keys: &ws.bib_keys, lib_keys: &self.lib_keys, cite_info: &self.cite_info, files: &files, font_size, vim, usage };
                d.ui(ui, pal, &env)
            }
            Doc::Image(im) => {
                image_view(ui, &ctx, im, pal);
                None
            }
        }
    }

    fn left_panel(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let Some(ws) = &mut self.ws else { return };
        let mut action: Option<TreeAction> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(ws.project.name().to_uppercase()).small().strong().color(pal.dim)).on_hover_text(ws.project.root.display().to_string());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⟳").on_hover_text("Refresh").clicked() {
                    action = Some(TreeAction::Refresh);
                }
                if ui.small_button("🗀").on_hover_text("New folder").clicked() {
                    action = Some(TreeAction::NewEntry(ws.selected_dir.clone(), true));
                }
                if ui.small_button("🗋").on_hover_text("New file").clicked() {
                    action = Some(TreeAction::NewEntry(ws.selected_dir.clone(), false));
                }
            });
        });
        let outline_h = if ws.outline.is_empty() { 0.0 } else { (ui.available_height() * 0.42).min(320.0) };
        let tree_h = ui.available_height() - outline_h - 30.0;
        let active_rel = ws.active.and_then(|a| ws.docs.get(a)).map(|d| d.rel().to_path_buf());
        egui::ScrollArea::vertical().id_salt("tree").max_height(tree_h).auto_shrink([false, false]).show(ui, |ui| {
            let main = ws.project.meta.main.clone();
            tree_ui(ui, pal, &ws.tree, &main, &active_rel, &ws.selected_dir, &mut action);
            // empty space: drop target / root selection
            let r = ui.allocate_response(vec2(ui.available_width(), 24.0), egui::Sense::click());
            if r.clicked() {
                action = Some(TreeAction::SelectDir(PathBuf::new()));
            }
            r.context_menu(|ui| {
                if ui.button("New File…").clicked() {
                    action = Some(TreeAction::NewEntry(PathBuf::new(), false));
                    ui.close();
                }
                if ui.button("New Folder…").clicked() {
                    action = Some(TreeAction::NewEntry(PathBuf::new(), true));
                    ui.close();
                }
            });
        });
        if !ws.outline.is_empty() {
            ui.separator();
            ui.label(RichText::new("OUTLINE").small().strong().color(pal.dim));
            egui::ScrollArea::vertical().id_salt("outline").auto_shrink([false, false]).show(ui, |ui| {
                let min_level = ws.outline.iter().map(|h| h.level).min().unwrap_or(0);
                for h in &ws.outline {
                    ui.horizontal(|ui| {
                        ui.add_space(((h.level - min_level) as f32) * 12.0);
                        let t = if h.level <= min_level { RichText::new(&h.title).strong() } else { RichText::new(&h.title) };
                        if ui.add(egui::Label::new(t).truncate().sense(egui::Sense::click())).on_hover_text(format!("{}:{}", h.file.display(), h.line)).clicked() {
                            action = Some(TreeAction::Goto(h.file.clone(), h.line));
                        }
                    });
                }
            });
        }
        if let Some(a) = action {
            self.tree_action(a);
        }
    }

    fn tree_action(&mut self, a: TreeAction) {
        let Some(ws) = &mut self.ws else { return };
        match a {
            TreeAction::Open(rel) => {
                if let Some(p) = rel.parent() {
                    ws.selected_dir = p.to_path_buf();
                }
                self.open_file(&rel);
            }
            TreeAction::Goto(rel, line) => self.open_at(&rel, line),
            TreeAction::SelectDir(d) => ws.selected_dir = d,
            TreeAction::NewEntry(dir, folder) => self.dialog = Some(Dialog::NewEntry { dir, name: String::new(), folder, error: None }),
            TreeAction::Rename(p) => {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.dialog = Some(Dialog::Rename { from: p, name, error: None });
            }
            TreeAction::Delete(p) => self.dialog = Some(Dialog::Delete { path: p }),
            TreeAction::SetMain(p) => {
                ws.project.meta.main = Some(p.to_string_lossy().to_string());
                let _ = ws.project.save_meta();
                self.refresh_refs(true);
                self.compile();
            }
            TreeAction::Reveal(p) => {
                let abs = ws.project.root.join(&p);
                open_external(if abs.is_dir() { &abs } else { abs.parent().unwrap_or(&abs) });
            }
            TreeAction::Refresh => self.rescan(),
        }
    }

    fn problems_panel(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let lib = self.refs.as_ref().and_then(|r| r.library());
        let Some(ws) = &mut self.ws else { return };
        let mut goto = None;
        let mut from_library: Vec<String> = Vec::new();
        // undefined citations that the library can fill
        let fillable = |k: &str| lib.is_some_and(|l| l.contains(k)) && !ws.bib_keys.iter().any(|b| b.eq_ignore_ascii_case(k));
        let mut missing_keys: Vec<String> = ws.last.as_ref().map(|l| l.entries.iter().filter_map(|e| lf_ref::project::undefined_citation(&e.message)).collect()).unwrap_or_default();
        missing_keys.sort();
        missing_keys.dedup();
        missing_keys.retain(|k| fillable(k));
        ui.horizontal(|ui| {
            ui.label(RichText::new("Problems").strong());
            if let Some(l) = &ws.last {
                let n = |lv: Level| l.entries.iter().filter(|e| e.level == lv).count();
                ui.toggle_value(&mut ws.show[0], RichText::new(format!("✖ {}", n(Level::Error))).color(pal.error));
                ui.toggle_value(&mut ws.show[1], RichText::new(format!("⚠ {}", n(Level::Warning))).color(pal.warn));
                ui.toggle_value(&mut ws.show[2], RichText::new(format!("⊟ {} bad boxes", n(Level::BadBox))).color(pal.dim));
            }
            ui.toggle_value(&mut ws.raw_log, "Raw log");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("×").clicked() {
                    ws.problems_open = false;
                }
            });
        });
        let Some(l) = &ws.last else {
            ui.label(RichText::new("Not compiled yet.").color(pal.dim));
            return;
        };
        if !missing_keys.is_empty() {
            egui::Frame::new().fill(pal.accent.gamma_multiply(0.12)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let n = missing_keys.len();
                    ui.label(format!("{n} undefined citation{} {} in your library: {}", if n == 1 { "" } else { "s" }, if n == 1 { "is" } else { "are" }, missing_keys.join(", ")));
                    if ui.button("Add from library").on_hover_text("Copies the entries into the project's .bib and recompiles").clicked() {
                        from_library = missing_keys.clone();
                    }
                });
            });
        }
        for m in &l.missing {
            egui::Frame::new().fill(pal.error.gamma_multiply(0.12)).corner_radius(6).inner_margin(6).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Missing TeX file `{}`.", m.file)).color(pal.error).strong());
                    match &m.provider {
                        Some(p) => {
                            let pkg = p.rsplit('/').next().unwrap_or(p);
                            let cmd = format!("sudo pacman -S {pkg}");
                            ui.label("Install it with");
                            ui.label(RichText::new(&cmd).monospace());
                            if ui.small_button("Copy").clicked() {
                                ui.ctx().copy_text(cmd);
                            }
                        }
                        None => {
                            ui.label("Install the TeX Live package that contains it (e.g. `tlmgr search --global --file`).");
                        }
                    }
                });
            });
        }
        if let Some(e) = &l.launch_error {
            ui.colored_label(pal.error, e);
        }
        if ws.raw_log {
            let mut log = if l.log.is_empty() { l.output.clone() } else { l.log.clone() };
            egui::ScrollArea::both().id_salt("raw-log").auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut log).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).interactive(true));
            });
            if !from_library.is_empty() {
                self.add_to_project_bib(&from_library);
                self.compile();
            }
            return;
        }
        egui::ScrollArea::vertical().id_salt("problems").auto_shrink([false, false]).show(ui, |ui| {
            let mut any = false;
            for e in &l.entries {
                let idx = match e.level {
                    Level::Error => 0,
                    Level::Warning => 1,
                    Level::BadBox => 2,
                };
                if !ws.show[idx] {
                    continue;
                }
                any = true;
                let (sym, col) = match e.level {
                    Level::Error => ("✖", pal.error),
                    Level::Warning => ("⚠", pal.warn),
                    Level::BadBox => ("⊟", pal.dim),
                };
                let loc = match (&e.file, e.line) {
                    (Some(f), Some(n)) => format!("{f}:{n}"),
                    (Some(f), None) => f.clone(),
                    (None, Some(n)) => format!("line {n}"),
                    _ => String::new(),
                };
                let r = ui
                    .horizontal(|ui| {
                        ui.label(RichText::new(sym).color(col));
                        ui.label(RichText::new(&loc).monospace().small().color(pal.accent));
                        if let Some(k) = lf_ref::project::undefined_citation(&e.message).filter(|k| missing_keys.contains(k))
                            && ui.small_button("Add from library").clicked()
                        {
                            from_library.push(k);
                        }
                        ui.add(egui::Label::new(&e.message).truncate());
                    })
                    .response;
                let r = ui.interact(r.rect, ui.scope_id().with(("problem", &e.message, e.line)), egui::Sense::click());
                let r = if e.context.is_empty() { r } else { r.on_hover_text(RichText::new(&e.context).monospace()) };
                if r.clicked()
                    && let Some(line) = e.line {
                        let file = e.file.clone().or_else(|| ws.project.meta.main.clone());
                        if let Some(f) = file {
                            let f = Path::new(&f);
                            let rel = if f.is_absolute() { f.strip_prefix(&ws.project.root).map(Path::to_path_buf).ok() } else { Some(f.to_path_buf()) };
                            if let Some(rel) = rel.filter(|r| ws.project.root.join(r).is_file()) {
                                goto = Some((rel, line));
                            }
                        }
                    }
            }
            if !any {
                ui.label(RichText::new(if l.ok { "No problems. 🎉" } else { "Nothing to show with the current filters." }).color(pal.dim));
            }
        });
        if let Some((rel, line)) = goto {
            self.open_at(&rel, line);
        }
        if !from_library.is_empty() {
            self.add_to_project_bib(&from_library);
            self.compile();
        }
    }

    // ------------------------------------------------------------ dialogs

    fn dialogs(&mut self, ctx: &egui::Context, pal: &Palette) {
        let Some(mut d) = self.dialog.take() else { return };
        let mut keep = true;
        let root = self.ws.as_ref().map(|w| w.project.root.clone());
        let enter = ctx.input(|i| i.key_pressed(Key::Enter));
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        let modal = egui::Modal::new(egui::Id::new("dialog")).show(ctx, |ui| match &mut d {
            Dialog::NewEntry { dir, name, folder, error } => {
                ui.set_width(380.0);
                ui.heading(if *folder { "New Folder" } else { "New File" });
                ui.label(RichText::new(format!("in /{}", dir.display())).small().color(pal.dim));
                let r = ui.add(egui::TextEdit::singleline(name).hint_text(if *folder { "figures" } else { "chapter.tex" }).desired_width(f32::INFINITY));
                r.request_focus();
                if let Some(e) = error {
                    ui.colored_label(pal.error, e.as_str());
                }
                ui.horizontal(|ui| {
                    if (ui.button("Create").clicked() || enter) && !name.trim().is_empty()
                        && let Some(root) = &root {
                            let rel = dir.join(name.trim());
                            let res = if *folder { project::create_dir(root, &rel) } else { project::create_file(root, &rel, b"") };
                            match res {
                                Ok(()) => {
                                    keep = false;
                                    self.rescan();
                                    if !*folder {
                                        self.open_file(&rel);
                                    }
                                }
                                Err(e) => *error = Some(e.to_string()),
                            }
                        }
                    if ui.button("Cancel").clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::Rename { from, name, error } => {
                ui.set_width(380.0);
                ui.heading("Rename");
                ui.label(RichText::new(from.display().to_string()).small().color(pal.dim));
                let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                r.request_focus();
                if let Some(e) = error {
                    ui.colored_label(pal.error, e.as_str());
                }
                ui.horizontal(|ui| {
                    if (ui.button("Rename").clicked() || enter) && !name.trim().is_empty()
                        && let Some(root) = &root {
                            let to = from.parent().map(|p| p.join(name.trim())).unwrap_or_else(|| PathBuf::from(name.trim()));
                            self.save_all();
                            match project::rename(root, from, &to) {
                                Ok(()) => {
                                    keep = false;
                                    if let Some(ws) = &mut self.ws {
                                        for doc in &mut ws.docs {
                                            let r = doc.rel().to_path_buf();
                                            if let Ok(rest) = r.strip_prefix(&*from) {
                                                let new_rel = if rest.as_os_str().is_empty() { to.clone() } else { to.join(rest) };
                                                match doc {
                                                    Doc::Text(t) => {
                                                        t.abs = root.join(&new_rel);
                                                        t.rel = new_rel;
                                                    }
                                                    Doc::Image(i) => {
                                                        i.abs = root.join(&new_rel);
                                                        i.rel = new_rel;
                                                    }
                                                }
                                            }
                                        }
                                        if ws.project.meta.main.as_deref() == Some(&from.to_string_lossy()) {
                                            ws.project.meta.main = Some(to.to_string_lossy().to_string());
                                            let _ = ws.project.save_meta();
                                        }
                                    }
                                    self.rescan();
                                }
                                Err(e) => *error = Some(e.to_string()),
                            }
                        }
                    if ui.button("Cancel").clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::Delete { path } => {
                ui.set_width(380.0);
                ui.heading("Move to Trash?");
                ui.label(format!("{} will be moved to the trash. It can also be restored from the project history.", path.display()));
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Move to Trash").color(pal.error)).clicked() {
                        if let Some(root) = &root {
                            if let Some(ws) = &self.ws {
                                let _ = ws.history.snapshot(None, true, "before delete");
                            }
                            match project::trash(&root.join(&*path)) {
                                Ok(()) => {
                                    if let Some(ws) = &mut self.ws {
                                        let gone: Vec<usize> = ws.docs.iter().enumerate().filter(|(_, d)| d.rel().starts_with(&*path)).map(|(i, _)| i).collect();
                                        for i in gone.into_iter().rev() {
                                            ws.docs.remove(i);
                                        }
                                        ws.active = (!ws.docs.is_empty()).then_some(0);
                                    }
                                    self.rescan();
                                }
                                Err(e) => self.status = format!("Delete failed: {e}"),
                            }
                        }
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::SaveTemplate { name, category, description, error } => {
                ui.set_width(420.0);
                ui.heading("Save Project as Template");
                egui::Grid::new("save-tpl").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(name);
                    ui.end_row();
                    ui.label("Category");
                    ui.text_edit_singleline(category);
                    ui.end_row();
                    ui.label("Description");
                    ui.add(egui::TextEdit::multiline(description).desired_rows(3));
                    ui.end_row();
                });
                ui.label(RichText::new("All project files are copied (no build files, no history).").small().color(pal.dim));
                if let Some(e) = error {
                    ui.colored_label(pal.error, e.as_str());
                }
                ui.horizontal(|ui| {
                    if ui.button("Save Template").clicked() && !name.trim().is_empty() {
                        self.save_all();
                        if let Some(ws) = &self.ws {
                            let meta = lf_core::templates::TemplateMeta {
                                name: name.trim().to_string(),
                                category: if category.trim().is_empty() { "My Templates".into() } else { category.trim().to_string() },
                                description: description.clone(),
                                main: ws.project.meta.main.clone().unwrap_or_else(|| "main.tex".into()),
                                engine: ws.project.meta.engine,
                                order: 0,
                            };
                            match lf_core::templates::create_from_folder(&ws.project.root, &meta) {
                                Ok(t) => {
                                    self.status = format!("Template \"{}\" saved.", t.meta.name);
                                    keep = false;
                                }
                                Err(e) => *error = Some(e.to_string()),
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::Cite { query, searched, results, selected, checked, cmd } => {
                ui.set_width(720.0);
                let markdown = self.ws.as_ref().and_then(|w| w.active.and_then(|a| w.docs.get(a))).is_some_and(|d| d.rel().extension().is_some_and(|e| e == "md"));
                ui.horizontal(|ui| {
                    ui.heading("Insert Citation");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if markdown {
                            ui.label(RichText::new("Markdown: [@key]").small().color(pal.dim));
                        } else {
                            egui::ComboBox::from_id_salt("cite-cmd").selected_text(format!("\\{cmd}")).width(120.0).show_ui(ui, |ui| {
                                for c in lf_ref::project::CITE_COMMANDS {
                                    ui.selectable_value(cmd, c.to_string(), format!("\\{c}"));
                                }
                            });
                        }
                    });
                });
                let r = ui.add(egui::TextEdit::singleline(query).hint_text("Search author, title, year, key …").desired_width(f32::INFINITY));
                r.request_focus();
                if searched.as_deref() != Some(query.as_str()) {
                    *results = self.cite_search(query);
                    *searched = Some(query.clone());
                    *selected = 0;
                }
                if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
                    *selected = (*selected + 1).min(results.len().saturating_sub(1));
                }
                if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
                    *selected = selected.saturating_sub(1);
                }
                // Ctrl+Space marks the highlighted entry for a multiple citation
                if ui.input(|i| i.modifiers.command && i.key_pressed(Key::Space))
                    && let Some(k) = results.get(*selected)
                {
                    match checked.iter().position(|c| c == k) {
                        Some(p) => {
                            checked.remove(p);
                        }
                        None => checked.push(k.clone()),
                    }
                }
                let in_project: Vec<String> = self.ws.as_ref().map(|w| w.bib_keys.iter().map(|k| k.to_lowercase()).collect()).unwrap_or_default();
                let mut pick: Option<Vec<String>> = None;
                let row_h = 22.0;
                egui::ScrollArea::vertical().max_height(380.0).auto_shrink([false, true]).show_rows(ui, row_h, results.len(), |ui, range| {
                    for i in range {
                        let k = &results[i];
                        ui.horizontal(|ui| {
                            ui.set_height(row_h);
                            let mut on = checked.contains(k);
                            if ui.checkbox(&mut on, "").on_hover_text("Cite several (Ctrl+Space)").changed() {
                                if on {
                                    checked.push(k.clone());
                                } else {
                                    checked.retain(|c| c != k);
                                }
                            }
                            let info = self.cite_info.get(&k.to_lowercase()).cloned().unwrap_or_default();
                            let mut job = egui::text::LayoutJob::default();
                            job.append(k, 0.0, egui::TextFormat::simple(egui::FontId::monospace(12.5), pal.text));
                            job.append(&info, 12.0, egui::TextFormat::simple(egui::FontId::proportional(13.0), pal.dim));
                            if in_project.contains(&k.to_lowercase()) {
                                job.append("in project", 12.0, egui::TextFormat::simple(egui::FontId::proportional(11.0), pal.accent2));
                            }
                            job.wrap = egui::text::TextWrapping { max_width: ui.available_width(), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
                            let r = ui.selectable_label(i == *selected, job);
                            if r.clicked() {
                                *selected = i;
                            }
                            if r.double_clicked() {
                                pick = Some(vec![k.clone()]);
                            }
                        });
                    }
                });
                if results.is_empty() {
                    let empty = self.refs.as_ref().and_then(|r| r.library()).is_none_or(|l| l.is_empty());
                    ui.label(RichText::new(if empty { "The library is empty – add references in View › References (Ctrl+Shift+R)." } else { "No matching entries." }).color(pal.dim));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let n = if checked.is_empty() { usize::from(!results.is_empty()) } else { checked.len() };
                    let label = if n > 1 { format!("Cite {n} entries") } else { "Cite".to_string() };
                    if ui.add_enabled(n > 0, egui::Button::new(label)).clicked() || (enter && n > 0) {
                        pick = Some(if checked.is_empty() { results.get(*selected).cloned().into_iter().collect() } else { checked.clone() });
                    }
                    if ui.button("Cancel").clicked() || esc {
                        keep = false;
                    }
                    ui.label(RichText::new("Enter cites · Ctrl+Space or the boxes: several · entries are copied into the project's .bib").small().color(pal.dim));
                });
                if let Some(keys) = pick.filter(|k| !k.is_empty()) {
                    let c = if markdown { "cite".to_string() } else { cmd.clone() };
                    keep = false;
                    self.cite(&keys, &c);
                }
            }
            Dialog::BibSetup { bib } => {
                ui.set_width(460.0);
                ui.heading("Set Up the Bibliography");
                let main = self.ws.as_ref().and_then(|w| w.project.meta.main.clone()).unwrap_or_default();
                let md = main.ends_with(".md");
                ui.label(format!("{main} does not use a bibliography file yet. The entries were written to {}.", bib.display()));
                ui.add_space(4.0);
                ui.label(RichText::new(if md { "Add `bibliography:` to the front matter?" } else { "Add \\addbibresource and \\printbibliography (biblatex with biber) to the main file?" }).color(pal.dim));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("Add to {main}")).clicked() || enter {
                        let b = bib.clone();
                        keep = false;
                        self.set_up_bibliography(&b);
                    }
                    if ui.button("Not now").clicked() || esc {
                        keep = false;
                    }
                });
            }
            Dialog::QuickOpen { query, selected } => {
                ui.set_width(460.0);
                let r = ui.add(egui::TextEdit::singleline(query).hint_text("Go to file…").desired_width(f32::INFINITY));
                r.request_focus();
                let files: Vec<PathBuf> = self.ws.as_ref().map(|w| w.files.clone()).unwrap_or_default();
                let q = query.to_lowercase();
                let hits: Vec<&PathBuf> = files.iter().filter(|f| fuzzy(&f.to_string_lossy().to_lowercase(), &q)).take(14).collect();
                if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
                    *selected = (*selected + 1).min(hits.len().saturating_sub(1));
                }
                if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
                    *selected = selected.saturating_sub(1);
                }
                let mut pick = None;
                for (i, f) in hits.iter().enumerate() {
                    if ui.selectable_label(i == *selected, format!("{} {}", file_icon(&f.to_string_lossy()), f.display())).clicked() {
                        pick = Some((*f).clone());
                    }
                }
                if enter {
                    pick = hits.get(*selected).map(|f| (*f).clone());
                }
                if let Some(f) = pick {
                    self.open_file(&f);
                    keep = false;
                }
                if esc {
                    keep = false;
                }
            }
            Dialog::Search { query, results, searched } => {
                ui.set_width(640.0);
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(query).hint_text("Search in all project files").desired_width(480.0));
                    if !*searched {
                        r.request_focus();
                    }
                    if (ui.button("Search").clicked() || enter) && !query.is_empty() {
                        *results = self.search(query);
                        *searched = true;
                    }
                    if ui.button("Close").clicked() || esc {
                        keep = false;
                    }
                });
                if *searched {
                    ui.label(RichText::new(format!("{} matches", results.len())).small().color(pal.dim));
                }
                let mut pick = None;
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (f, line, text) in results.iter() {
                        let r = ui.add(egui::Label::new(RichText::new(format!("{}:{line}  {}", f.display(), text.trim())).monospace().small()).truncate().sense(egui::Sense::click()));
                        if r.clicked() {
                            pick = Some((f.clone(), *line));
                        }
                    }
                });
                if let Some((f, l)) = pick {
                    self.open_at(&f, l);
                    keep = false;
                }
            }
            Dialog::Shortcuts { query } => {
                ui.set_width(560.0);
                ui.horizontal(|ui| {
                    ui.heading("Keyboard Shortcuts");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(egui::TextEdit::singleline(query).hint_text("Filter…").desired_width(180.0)).request_focus();
                    });
                });
                ui.add_space(4.0);
                let groups = crate::shortcuts::filtered(query);
                egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.7).auto_shrink([false, true]).show(ui, |ui| {
                    if groups.is_empty() {
                        ui.label(RichText::new("No shortcut matches.").color(pal.dim));
                    }
                    for (g, items) in &groups {
                        ui.label(RichText::new(*g).strong().color(pal.accent2));
                        egui::Grid::new(("keys", *g)).num_columns(2).spacing([24.0, 4.0]).min_col_width(210.0).striped(true).show(ui, |ui| {
                            for (k, v) in items {
                                ui.label(RichText::new(*k).monospace());
                                ui.label(*v);
                                ui.end_row();
                            }
                        });
                        ui.add_space(8.0);
                    }
                });
                ui.label(RichText::new("Editing shortcuts apply in the editor; in Vim mode in insert mode.").small().color(pal.dim));
                if ui.button("Close").clicked() || esc {
                    keep = false;
                }
            }
            Dialog::SystemCheck => {
                ui.set_width(520.0);
                ui.heading("System Check");
                egui::Grid::new("tools").num_columns(3).spacing([16.0, 4.0]).show(ui, |ui| {
                    for t in &self.tools {
                        ui.label(if t.found { RichText::new("✔").color(pal.ok) } else if t.required { RichText::new("✖").color(pal.error) } else { RichText::new("⚠").color(pal.warn) });
                        ui.label(RichText::new(t.name).monospace());
                        ui.label(RichText::new(t.purpose).color(pal.dim));
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                let german = lf_tex::tools::kpsewhich("ngerman.ldf").is_some();
                ui.label(if german {
                    RichText::new("✔ German babel support (ngerman) installed").color(pal.ok)
                } else {
                    RichText::new("⚠ German babel support missing – install texlive-langgerman").color(pal.warn)
                });
                if ui.button("Close").clicked() || esc || enter {
                    keep = false;
                }
            }
            Dialog::About => {
                ui.set_width(380.0);
                ui.vertical_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), egui::Sense::hover());
                    theme::logo(ui.painter(), r);
                    ui.heading("arvidleaf");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.label(RichText::new("A local LaTeX editor in Rust. Compiles with your TeX Live via latexmk – no time or page limits.").color(pal.dim));
                    if ui.button("Close").clicked() || esc || enter {
                        keep = false;
                    }
                });
            }
        });
        if modal.should_close() {
            keep = false;
        }
        if keep && self.dialog.is_none() {
            self.dialog = Some(d);
        }
    }

    fn search(&self, q: &str) -> Vec<(PathBuf, u32, String)> {
        let Some(ws) = &self.ws else { return Vec::new() };
        let ql = q.to_lowercase();
        let mut out = Vec::new();
        for f in &ws.files {
            if !project::is_text_file(f) {
                continue;
            }
            let text = ws
                .docs
                .iter()
                .find_map(|d| match d {
                    Doc::Text(t) if &t.rel == f => Some(t.text.clone()),
                    _ => None,
                })
                .or_else(|| std::fs::read_to_string(ws.project.root.join(f)).ok());
            let Some(text) = text else { continue };
            for (i, l) in text.lines().enumerate() {
                if l.to_lowercase().contains(&ql) {
                    out.push((f.clone(), i as u32 + 1, l.chars().take(200).collect()));
                    if out.len() >= 1000 {
                        return out;
                    }
                }
            }
        }
        out
    }
}

/// Subsequence match ("chin" matches "chapters/intro.tex").
fn fuzzy(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

enum TreeAction {
    Open(PathBuf),
    Goto(PathBuf, u32),
    SelectDir(PathBuf),
    NewEntry(PathBuf, bool),
    Rename(PathBuf),
    Delete(PathBuf),
    SetMain(PathBuf),
    Reveal(PathBuf),
    Refresh,
}

fn tree_ui(ui: &mut egui::Ui, pal: &Palette, nodes: &[FileNode], main: &Option<String>, active: &Option<PathBuf>, sel_dir: &Path, action: &mut Option<TreeAction>) {
    for n in nodes {
        if n.is_dir {
            let id = ui.make_persistent_id(("dir", &n.rel));
            let hdr = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
            let selected = sel_dir == n.rel;
            let (_, resp, _) = hdr
                .show_header(ui, |ui| {
                    let t = RichText::new(format!("📁 {}", n.name));
                    let r = ui.add(egui::Label::new(if selected { t.color(pal.accent) } else { t }).sense(egui::Sense::click()));
                    if r.clicked() {
                        *action = Some(TreeAction::SelectDir(n.rel.clone()));
                    }
                    r.context_menu(|ui| {
                        if ui.button("New File…").clicked() {
                            *action = Some(TreeAction::NewEntry(n.rel.clone(), false));
                            ui.close();
                        }
                        if ui.button("New Folder…").clicked() {
                            *action = Some(TreeAction::NewEntry(n.rel.clone(), true));
                            ui.close();
                        }
                        if ui.button("Rename…").clicked() {
                            *action = Some(TreeAction::Rename(n.rel.clone()));
                            ui.close();
                        }
                        if ui.button("Show in File Manager").clicked() {
                            *action = Some(TreeAction::Reveal(n.rel.clone()));
                            ui.close();
                        }
                        if ui.button(RichText::new("Delete…").color(pal.error)).clicked() {
                            *action = Some(TreeAction::Delete(n.rel.clone()));
                            ui.close();
                        }
                    });
                })
                .body(|ui| tree_ui(ui, pal, &n.children, main, active, sel_dir, action));
            let _ = resp;
        } else {
            let rel_s = n.rel.to_string_lossy().to_string();
            let is_main = main.as_deref() == Some(rel_s.as_str());
            let is_active = active.as_deref() == Some(n.rel.as_path());
            let label = format!("{} {}{}", file_icon(&n.name), n.name, if is_main { "  ★" } else { "" });
            let r = ui.selectable_label(is_active, label);
            if r.clicked() {
                *action = Some(TreeAction::Open(n.rel.clone()));
            }
            r.context_menu(|ui| {
                if ui.button("Open").clicked() {
                    *action = Some(TreeAction::Open(n.rel.clone()));
                    ui.close();
                }
                if n.name.ends_with(".tex") && !is_main && ui.button("★ Set as Main File").clicked() {
                    *action = Some(TreeAction::SetMain(n.rel.clone()));
                    ui.close();
                }
                if ui.button("Rename…").clicked() {
                    *action = Some(TreeAction::Rename(n.rel.clone()));
                    ui.close();
                }
                if ui.button("Copy Path").clicked() {
                    ui.ctx().copy_text(rel_s.clone());
                    ui.close();
                }
                if ui.button("Show in File Manager").clicked() {
                    *action = Some(TreeAction::Reveal(n.rel.clone()));
                    ui.close();
                }
                if ui.button(RichText::new("Delete…").color(pal.error)).clicked() {
                    *action = Some(TreeAction::Delete(n.rel.clone()));
                    ui.close();
                }
            });
        }
    }
}

fn image_view(ui: &mut egui::Ui, ctx: &egui::Context, im: &mut ImageDoc, pal: &Palette) {
    if im.tex.is_none() {
        let ext = im.abs.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let r: Result<lf_pdf::Image, String> = match ext.as_str() {
            "pdf" => lf_pdf::render_page(&im.abs, 1, 110.0),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" => image::open(&im.abs).map_err(|e| e.to_string()).map(|i| {
                let rgb = i.to_rgb8();
                lf_pdf::Image { width: rgb.width() as usize, height: rgb.height() as usize, rgb: rgb.into_raw() }
            }),
            _ => Err("No preview for this file type.".into()),
        };
        im.tex = Some(r.map(|img| ctx.load_texture(im.rel.to_string_lossy(), egui::ColorImage::from_rgb([img.width, img.height], &img.rgb), TextureOptions::LINEAR)));
    }
    let size = std::fs::metadata(&im.abs).map(|m| m.len()).unwrap_or(0);
    ui.label(RichText::new(format!("{} · {:.1} KB", im.rel.display(), size as f64 / 1024.0)).small().color(pal.dim));
    match im.tex.as_ref().unwrap() {
        Ok(t) => {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                let avail = ui.available_width();
                let s = t.size_vec2();
                let scale = (avail / s.x).min(1.0);
                ui.image((t.id(), s * scale));
            });
        }
        Err(e) => {
            ui.label(RichText::new(e).color(pal.dim));
            if ui.button("Open with default application").clicked() {
                open_external(&im.abs);
            }
        }
    }
}

/// Removes the empty IME preedits winit sends on Wayland whenever the
/// compositor confirms the cursor area (`done`) while nothing is being
/// composed. egui answers every event with a new cursor area, so KWin and the
/// app would ping-pong forever: constant repaints, and each preedit used to
/// hide the next keys from the editor. An empty preedit that ends a real
/// composition stays.
pub fn drop_idle_preedits(events: &mut Vec<egui::Event>, composing: &mut bool) {
    events.retain(|e| match e {
        egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if text.is_empty() => std::mem::replace(composing, false),
        egui::Event::Ime(egui::ImeEvent::Preedit { .. }) => {
            *composing = true;
            true
        }
        egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
            *composing = false;
            true
        }
        _ => true,
    });
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        drop_idle_preedits(&mut raw_input.events, &mut self.ime_composing);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(r) = &mut self.refs {
            r.flush();
        }
        self.close_project();
        let _ = self.cfg.save();
    }
}

#[cfg(test)]
mod ime_tests {
    use super::drop_idle_preedits;
    use egui::{Event, ImeEvent};

    fn pre(t: &str) -> Event {
        Event::Ime(ImeEvent::Preedit { text: t.into(), active_range_chars: None })
    }

    #[test]
    fn idle_preedits_are_dropped_but_end_of_composition_stays() {
        let mut composing = false;
        let mut ev = vec![pre(""), Event::Text("(".into()), pre("")];
        drop_idle_preedits(&mut ev, &mut composing);
        assert_eq!(ev, vec![Event::Text("(".into())]);
        let mut ev = vec![pre("ä"), pre(""), pre("")];
        drop_idle_preedits(&mut ev, &mut composing);
        assert_eq!(ev, vec![pre("ä"), pre("")], "the first empty preedit clears the composition");
        assert!(!composing);
    }
}
