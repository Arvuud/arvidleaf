//! References view (arvidref): collections and tags, the library table, the entry editor,
//! import (DOI/arXiv/ISBN, BibTeX, CSL-JSON, PDFs) and a reader for attached PDFs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, FontId, Galley, Key, RichText, Sense, text::LayoutJob, vec2};
use lf_core::config::Config;
use lf_ref::bib::{Entry, Piece, Value};
use lf_ref::import::{self, Identifier, Input};
use lf_ref::library::{self, SaveError};
use lf_ref::net::{NetError, NetOptions};
use lf_ref::{EntryView, Library, types};

use crate::app::open_external;
use crate::pdfview::PdfView;
use crate::theme::Palette;

/// Changes in the editor are written after this much idle time.
const EDIT_DELAY: Duration = Duration::from_millis(800);
const ROW_H: f32 = 22.0;
/// Width of the field names in the editor.
const LABEL_W: f32 = 84.0;

#[derive(Clone, PartialEq, Debug)]
enum Filter {
    All,
    Unfiled,
    Collection(String),
    Tag(String),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Col {
    /// File order, newest last – shown newest first.
    Added,
    Author,
    Year,
    Title,
    Container,
}

enum Job {
    Fetch(Identifier),
    Pdf(PathBuf),
}

enum Done {
    Found { entry: Box<Entry>, what: String, attach: Option<PathBuf> },
    /// `id`: what a PDF was identified as, kept in the fallback entry.
    Failed { what: String, error: String, cancelled: bool, attach: Option<PathBuf>, id: Option<Identifier> },
}

/// Results of the running import batch, summarized when it ends.
#[derive(Default)]
struct Batch {
    added: Vec<String>,
    duplicates: Vec<String>,
    failed: Vec<String>,
}

/// One field in the editor.
struct FieldEdit {
    name: String,
    text: String,
    orig: String,
    /// Concatenations and `@string` macros (`jan # "~1"`) are only editable as source.
    composite: bool,
}

/// The entry being edited. Changes are applied to the library after [`EDIT_DELAY`].
struct Draft {
    key: String,
    new_key: String,
    kind: String,
    fields: Vec<FieldEdit>,
    removed: Vec<String>,
    tags: String,
    tags_orig: String,
    notes: String,
    notes_orig: String,
    changed: Option<Instant>,
    error: Option<String>,
    raw: String,
    raw_orig: String,
    custom_field: String,
}

fn field_text(v: &Value) -> (String, bool) {
    match v.0.as_slice() {
        [Piece::Braced(s)] | [Piece::Quoted(s)] | [Piece::Number(s)] => (s.clone(), false),
        _ => (lf_ref::bib::format_value(v), true),
    }
}

impl Draft {
    fn new(lib: &Library, key: &str) -> Option<Draft> {
        let e = lib.entry(key)?;
        let mut fields: Vec<FieldEdit> = Vec::new();
        // required groups first; an empty one gets a placeholder for its first alternative
        for group in types::required(&e.kind) {
            let alts: Vec<&str> = group.split('|').collect();
            match alts.iter().find(|f| e.get(f).is_some()) {
                Some(f) => {
                    let (text, composite) = field_text(e.get(f).unwrap());
                    let name = e.fields.iter().find(|x| x.name.eq_ignore_ascii_case(f)).map(|x| x.name.clone()).unwrap_or_else(|| f.to_string());
                    fields.push(FieldEdit { name, orig: text.clone(), text, composite });
                }
                None => fields.push(FieldEdit { name: alts[0].to_string(), text: String::new(), orig: String::new(), composite: false }),
            }
        }
        for f in &e.fields {
            if types::SPECIAL.iter().any(|s| s.eq_ignore_ascii_case(&f.name)) || fields.iter().any(|x| x.name.eq_ignore_ascii_case(&f.name)) {
                continue;
            }
            let (text, composite) = field_text(&f.value);
            fields.push(FieldEdit { name: f.name.clone(), orig: text.clone(), text, composite });
        }
        let tags = library::tags(e, lib.strings()).join(", ");
        let notes = e.get("annotation").map(|v| field_text(v).0).unwrap_or_default();
        let raw = lib.source(key).unwrap_or_default();
        Some(Draft {
            key: e.key.clone(),
            new_key: e.key.clone(),
            kind: e.kind.to_lowercase(),
            fields,
            removed: Vec::new(),
            tags_orig: tags.clone(),
            tags,
            notes_orig: notes.clone(),
            notes,
            changed: None,
            error: None,
            raw_orig: raw.clone(),
            raw,
            custom_field: String::new(),
        })
    }

    fn build(&self, base: &Entry) -> Entry {
        let mut e = base.clone();
        e.kind = self.kind.clone();
        e.key = self.new_key.trim().to_string();
        for r in &self.removed {
            e.remove(r);
        }
        for f in &self.fields {
            if f.composite || f.text == f.orig {
                continue;
            }
            let t = f.text.trim();
            if t.is_empty() {
                e.remove(&f.name);
            } else if t.chars().all(|c| c.is_ascii_digit()) {
                e.set(&f.name, Value(vec![Piece::Number(t.to_string())]));
            } else {
                e.set(&f.name, Value::text(t));
            }
        }
        if self.tags != self.tags_orig {
            let tags: Vec<String> = self.tags.split([',', ';']).map(|t| t.trim().to_string()).collect();
            library::set_tags(&mut e, &tags);
        }
        if self.notes != self.notes_orig {
            if self.notes.trim().is_empty() {
                e.remove("annotation");
            } else {
                e.set("annotation", Value::text(self.notes.trim_end()));
            }
        }
        e
    }

    fn touch(&mut self) {
        self.changed = Some(Instant::now());
        self.error = None;
    }
}

/// Counts for the side panel, rebuilt with the rows.
#[derive(Default)]
struct Side {
    total: usize,
    unfiled: usize,
    /// Collections in tree order with their (recursive) entry counts.
    collections: Vec<(String, usize)>,
    tags: Vec<(String, usize)>,
}

enum Tab {
    Fields,
    Source,
}

pub struct RefsView {
    lib: Result<Library, String>,
    dir: PathBuf,
    query: String,
    filter: Filter,
    sort: (Col, bool),
    rows: Vec<EntryView>,
    rows_dirty: bool,
    side: Side,
    selected: Vec<String>,
    anchor: Option<usize>,
    scroll_to_selected: bool,
    draft: Option<Draft>,
    tab: Tab,
    add_input: String,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    cancel: Arc<AtomicBool>,
    pending: usize,
    batch: Batch,
    net: NetOptions,
    undo: Vec<Vec<(Entry, Vec<String>)>>,
    pdf: Option<PdfView>,
    reading: Option<PathBuf>,
    conflict: bool,
    checked: Instant,
    collapsed: HashSet<String>,
    /// Collection name being created or renamed: (old path or parent prefix, text, is_rename).
    naming: Option<(String, String, bool)>,
    ctx: egui::Context,
    /// Counts changes to the library (for caches outside the view).
    version: u64,
    /// Message for the status bar, taken by the app.
    pub status: Option<String>,
}

impl RefsView {
    pub fn new(ctx: &egui::Context, cfg: &Config) -> RefsView {
        let (tx, rx) = channel();
        let dir = cfg.library_dir();
        let mut v = RefsView {
            lib: Err(String::new()),
            dir: dir.clone(),
            query: String::new(),
            filter: Filter::All,
            sort: (Col::Added, false),
            rows: Vec::new(),
            rows_dirty: true,
            side: Side::default(),
            selected: Vec::new(),
            anchor: None,
            scroll_to_selected: false,
            draft: None,
            tab: Tab::Fields,
            add_input: String::new(),
            tx,
            rx,
            cancel: Arc::new(AtomicBool::new(false)),
            pending: 0,
            batch: Batch::default(),
            net: NetOptions::default(),
            undo: Vec::new(),
            pdf: None,
            reading: None,
            conflict: false,
            checked: Instant::now(),
            collapsed: HashSet::new(),
            naming: None,
            ctx: ctx.clone(),
            version: 0,
            status: None,
        };
        v.configure(cfg);
        v.open(&dir);
        v
    }

    /// Applies key pattern and contact address from the settings.
    pub fn configure(&mut self, cfg: &Config) {
        self.net.mailto = cfg.ref_mailto.clone();
        if let Ok(lib) = &mut self.lib {
            lib.key_pattern = if cfg.key_pattern.trim().is_empty() { lf_ref::key::DEFAULT_PATTERN.into() } else { cfg.key_pattern.trim().into() };
        }
    }

    /// Opens the library in `dir` (after a change of location in the settings).
    pub fn open(&mut self, dir: &Path) {
        self.apply_draft();
        let pattern = self.lib.as_ref().map(|l| l.key_pattern.clone()).unwrap_or_else(|_| lf_ref::key::DEFAULT_PATTERN.into());
        self.dir = dir.to_path_buf();
        self.lib = Library::open(dir).map_err(|e| format!("Cannot open the library in {}: {e}", dir.display()));
        if let Ok(lib) = &mut self.lib {
            lib.key_pattern = pattern;
            let problems = lib.problems();
            if !problems.is_empty() {
                self.status = Some(format!("library.bib: {} malformed entr{} kept as text (first at line {})", problems.len(), if problems.len() == 1 { "y" } else { "ies" }, problems[0].0));
            }
        }
        self.version += 1;
        self.selected.clear();
        self.draft = None;
        self.undo.clear();
        self.filter = Filter::All;
        self.rows_dirty = true;
    }

    /// The library, if it could be opened.
    pub fn library(&self) -> Option<&Library> {
        self.lib.as_ref().ok()
    }

    /// Changes whenever the library changes.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Background work while the view is hidden: import results, changes on disk.
    pub fn poll(&mut self) {
        self.receive();
        self.watch();
    }

    /// Writes pending edits (on exit).
    pub fn flush(&mut self) {
        self.apply_draft();
    }

    pub fn busy(&self) -> bool {
        self.pending > 0 || self.pdf.as_ref().is_some_and(|p| p.busy())
    }

    fn say(&mut self, s: impl Into<String>) {
        self.status = Some(s.into());
    }

    // ------------------------------------------------------------ library changes

    fn persist(&mut self) {
        self.rows_dirty = true;
        self.version += 1;
        let Ok(lib) = &mut self.lib else { return };
        match lib.save(false) {
            Ok(()) => {}
            Err(SaveError::Conflict) => self.conflict = true,
            Err(SaveError::Io(e)) => self.status = Some(format!("Could not save the library: {e}")),
        }
    }

    /// Writes pending edits of the open entry.
    fn apply_draft(&mut self) {
        let (Ok(lib), Some(d)) = (&mut self.lib, &mut self.draft) else { return };
        if d.changed.is_none() {
            return;
        }
        d.changed = None;
        let Some(base) = lib.entry(&d.key).cloned() else { return };
        let e = d.build(&base);
        match lib.update(&d.key, e) {
            Ok(k) => {
                let old = std::mem::replace(&mut d.key, k.clone());
                d.new_key = k.clone();
                for f in &mut d.fields {
                    f.orig = f.text.clone();
                }
                d.removed.clear();
                d.tags_orig = d.tags.clone();
                d.notes_orig = d.notes.clone();
                d.raw = lib.source(&k).unwrap_or_default();
                d.raw_orig = d.raw.clone();
                for s in &mut self.selected {
                    if *s == old {
                        *s = k.clone();
                    }
                }
                self.persist();
            }
            Err(msg) => d.error = Some(msg),
        }
    }

    fn select(&mut self, keys: Vec<String>) {
        self.apply_draft();
        self.selected = keys;
        self.load_draft();
    }

    fn load_draft(&mut self) {
        self.draft = match (&self.lib, self.selected.last()) {
            (Ok(lib), Some(k)) if self.selected.len() == 1 => Draft::new(lib, k),
            _ => None,
        };
    }

    fn delete(&mut self, keys: &[String]) {
        self.apply_draft();
        let Ok(lib) = &mut self.lib else { return };
        let mut removed = Vec::new();
        for k in keys {
            let cols = lib.collections.of(k);
            if let Some(e) = lib.remove(k) {
                removed.push((e, cols));
            }
        }
        if removed.is_empty() {
            return;
        }
        let n = removed.len();
        self.undo.push(removed);
        self.selected.clear();
        self.draft = None;
        self.persist();
        self.say(format!("Deleted {n} entr{} – Undo with Ctrl+Z (attachments stay on disk)", if n == 1 { "y" } else { "ies" }));
    }

    fn undo_delete(&mut self) {
        let Some(batch) = self.undo.pop() else { return };
        let Ok(lib) = &mut self.lib else { return };
        let mut keys = Vec::new();
        for (mut e, cols) in batch {
            let k = lib.add_anyway(&mut e);
            for c in cols {
                lib.collections.add(&c, &k);
            }
            keys.push(k);
        }
        self.persist();
        self.say(format!("Restored {} entr{}", keys.len(), if keys.len() == 1 { "y" } else { "ies" }));
        self.select(keys);
        self.scroll_to_selected = true;
    }

    /// Reloads after a change by another program, or marks a conflict.
    fn watch(&mut self) {
        if self.checked.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.checked = Instant::now();
        self.ctx.request_repaint_after(Duration::from_secs(2));
        let Ok(lib) = &mut self.lib else { return };
        if self.conflict || !lib.changed_on_disk() {
            return;
        }
        let editing = self.draft.as_ref().is_some_and(|d| d.changed.is_some());
        if lib.is_dirty() || editing {
            self.conflict = true;
            return;
        }
        match lib.reload() {
            Ok(()) => {
                self.status = Some("Library reloaded – library.bib was changed by another program".into());
                self.version += 1;
                self.selected.retain(|k| lib.contains(k));
                self.rows_dirty = true;
                self.load_draft();
            }
            Err(e) => self.status = Some(format!("Cannot reload the library: {e}")),
        }
    }

    // ------------------------------------------------------------ import

    /// Pasted or typed text: BibTeX, CSL-JSON or identifiers.
    pub fn add_text(&mut self, text: &str) {
        match import::read_text(text) {
            Input::Entries(entries, problems) => {
                let Ok(lib) = &mut self.lib else { return };
                let r = import::add_all(lib, entries);
                let mut msg = format!("Imported {} entr{}", r.added.len(), if r.added.len() == 1 { "y" } else { "ies" });
                if !r.duplicates.is_empty() {
                    msg.push_str(&format!(", {} already in the library ({})", r.duplicates.len(), r.duplicates.iter().map(|d| d.1.as_str()).take(3).collect::<Vec<_>>().join(", ")));
                }
                if !problems.is_empty() {
                    msg.push_str(&format!(", {} malformed (line {})", problems.len(), problems[0].0));
                }
                let keys = if r.added.is_empty() { r.duplicates.iter().map(|d| d.1.clone()).collect() } else { r.added };
                self.persist();
                self.say(msg);
                self.select(keys);
                self.scroll_to_selected = true;
            }
            Input::Identifiers(ids) => self.start(ids.into_iter().map(Job::Fetch).collect()),
            Input::Nothing => self.say("Nothing to import – enter a DOI, arXiv ID, ISBN or BibTeX"),
        }
    }

    /// Dropped or chosen files: PDFs (DOI from the text), `.bib`, `.json`, text lists.
    pub fn import_paths(&mut self, paths: &[PathBuf]) {
        let mut jobs = Vec::new();
        for p in paths {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if ext == "pdf" {
                jobs.push(Job::Pdf(p.clone()));
            } else if matches!(ext.as_str(), "bib" | "json" | "txt" | "bibtex" | "biblatex") {
                match std::fs::read_to_string(p) {
                    Ok(t) => self.add_text(&t),
                    Err(e) => self.say(format!("Cannot read {}: {e}", p.display())),
                }
            } else {
                self.say(format!("Cannot import {} – PDFs, .bib and CSL-JSON files are supported", p.display()));
            }
        }
        if !jobs.is_empty() {
            self.start(jobs);
        }
    }

    fn start(&mut self, jobs: Vec<Job>) {
        if self.pending == 0 {
            self.cancel.store(false, Ordering::Relaxed);
            self.batch = Batch::default();
        }
        self.pending += jobs.len();
        let n = jobs.len();
        self.say(if n == 1 { "Looking up…".to_string() } else { format!("Looking up {n} items…") });
        let (tx, cancel, opts, ctx) = (self.tx.clone(), self.cancel.clone(), self.net.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            for job in jobs {
                let done = run_job(job, &opts, &cancel);
                if tx.send(done).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
    }

    fn receive(&mut self) {
        while let Ok(done) = self.rx.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            let Ok(lib) = &mut self.lib else { continue };
            match done {
                Done::Found { entry, what, attach } => match lib.add(*entry) {
                    Ok(k) => {
                        if let Some(p) = &attach
                            && let Err(e) = lib.attach(&k, p)
                        {
                            self.batch.failed.push(format!("{}: {e}", p.display()));
                        }
                        self.batch.added.push(k);
                    }
                    Err(library::AddError::Duplicate(k)) => {
                        // a PDF for an entry without one is attached to it
                        if let Some(p) = &attach
                            && lib.files(&k).is_empty()
                        {
                            let _ = lib.attach(&k, p);
                        }
                        self.batch.duplicates.push(format!("{what} as {k}"));
                        self.batch.added.push(k);
                    }
                },
                Done::Failed { what, error, cancelled, attach, id } => {
                    // a PDF that cannot be identified is kept with its file name as title
                    if let (Some(p), false) = (&attach, cancelled) {
                        let mut e = Entry::new("misc", "");
                        let stem = p.file_stem().map(|s| s.to_string_lossy().replace(['_', '-'], " ")).unwrap_or_default();
                        e.set("title", Value::text(lf_ref::latex::encode(stem.trim())));
                        match id {
                            Some(Identifier::Doi(d)) => e.set("doi", Value::text(d)),
                            Some(Identifier::Arxiv(a)) => {
                                e.set("eprint", Value::text(a));
                                e.set("eprinttype", Value::text("arxiv"));
                            }
                            Some(Identifier::Isbn(i)) => e.set("isbn", Value::text(i)),
                            None => {}
                        }
                        let k = lib.add_anyway(&mut e);
                        let _ = lib.attach(&k, p);
                        self.batch.added.push(k);
                        self.batch.failed.push(format!("{what}: {error} – added with the file name as title"));
                    } else {
                        self.batch.failed.push(format!("{what}: {error}"));
                    }
                }
            }
            self.persist();
            if self.pending == 0 {
                self.finish_batch();
            }
        }
    }

    fn finish_batch(&mut self) {
        let b = std::mem::take(&mut self.batch);
        let new = b.added.len() - b.duplicates.len().min(b.added.len());
        let mut parts = Vec::new();
        if new > 0 {
            parts.push(format!("Added {new} entr{}", if new == 1 { "y" } else { "ies" }));
        }
        if !b.duplicates.is_empty() {
            parts.push(format!("already in the library: {}", b.duplicates.join(", ")));
        }
        if !b.failed.is_empty() {
            parts.push(b.failed.join("; "));
        }
        if parts.is_empty() {
            parts.push("Nothing imported".into());
        }
        self.say(parts.join(" · "));
        if !b.added.is_empty() {
            self.select(b.added);
            self.scroll_to_selected = true;
        }
    }

    // ------------------------------------------------------------ rows

    fn rebuild_rows(&mut self) {
        self.rows_dirty = false;
        let Ok(lib) = &self.lib else {
            self.rows.clear();
            return;
        };
        let by_key: HashMap<&str, &EntryView> = lib.views().map(|v| (v.key.as_str(), v)).collect();
        let filed: HashSet<String> = lib.collections.collections.values().flatten().map(|k| k.to_lowercase()).collect();
        let mut paths: Vec<String> = lib.collections.collections.keys().cloned().collect();
        paths.sort_by(|a, b| a.split('/').cmp(b.split('/')));
        self.side = Side {
            total: lib.len(),
            unfiled: lib.views().filter(|v| !filed.contains(&v.key.to_lowercase())).count(),
            collections: paths.into_iter().map(|p| {
                let n = lib.collections.keys(&p, true).len();
                (p, n)
            }).collect(),
            tags: lib.tags(),
        };
        let allowed: Option<HashSet<String>> = match &self.filter {
            Filter::All | Filter::Tag(_) => None,
            Filter::Collection(c) => Some(lib.collections.keys(c, true).into_iter().map(|k| k.to_lowercase()).collect()),
            Filter::Unfiled => Some(lib.views().map(|v| v.key.to_lowercase()).filter(|k| !filed.contains(k)).collect()),
        };
        let query = match &self.filter {
            Filter::Tag(t) if !t.contains(' ') => format!("tag:{t} {}", self.query),
            _ => self.query.clone(),
        };
        let mut rows: Vec<EntryView> = lib
            .search(&query)
            .into_iter()
            .filter(|k| allowed.as_ref().is_none_or(|a| a.contains(&k.to_lowercase())))
            .filter(|k| match &self.filter {
                Filter::Tag(t) if t.contains(' ') => by_key.get(k).is_some_and(|v| v.tags.iter().any(|x| x.eq_ignore_ascii_case(t))),
                _ => true,
            })
            .filter_map(|k| by_key.get(k).map(|v| (*v).clone()))
            .collect();
        let (col, asc) = self.sort;
        let searching = !self.query.trim().is_empty();
        match col {
            // relevance while searching, else newest first
            Col::Added if searching => {}
            Col::Added => {
                if !asc {
                    rows.reverse();
                }
            }
            _ => {
                let lower = |s: &str| lf_ref::latex::strip_accents(s).to_lowercase();
                rows.sort_by_cached_key(|v| match col {
                    Col::Author => lower(&v.authors),
                    Col::Year => v.year.clone(),
                    Col::Title => lower(v.title.trim_start_matches(['"', '\'', '(', '['])),
                    _ => lower(&v.container),
                });
                if !asc {
                    rows.reverse();
                }
            }
        }
        self.rows = rows;
    }

    // ------------------------------------------------------------ frame

    pub fn ui(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        self.receive();
        self.watch();
        if self.draft.as_ref().and_then(|d| d.changed).is_some_and(|t| t.elapsed() >= EDIT_DELAY) {
            self.apply_draft();
        } else if self.draft.as_ref().is_some_and(|d| d.changed.is_some()) {
            self.ctx.request_repaint_after(EDIT_DELAY);
        }
        if self.rows_dirty {
            self.rebuild_rows();
        }
        if self.pending > 0 {
            self.ctx.request_repaint_after(Duration::from_millis(100));
        }
        let lib_err = self.lib.as_ref().err().cloned();
        if let Some(e) = lib_err {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(RichText::new(e).color(pal.error));
                ui.add_space(8.0);
                if ui.button("Try again").clicked() {
                    let d = self.dir.clone();
                    self.open(&d);
                }
            });
            return;
        }
        self.keys(ui.ctx());
        egui::Panel::left("refs-side").resizable(true).default_size(210.0).min_size(150.0).max_size(360.0).frame(egui::Frame::new().fill(pal.panel).inner_margin(8)).show(ui, |ui| self.sidebar(ui, pal));
        let max_detail = (ui.available_width() * 0.5).max(300.0);
        egui::Panel::right("refs-detail").resizable(true).default_size(420.0).min_size(280.0).max_size(max_detail).frame(egui::Frame::new().fill(pal.panel).inner_margin(10)).show(ui, |ui| self.detail(ui, pal));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.bg).inner_margin(8)).show(ui, |ui| {
            if self.conflict {
                self.conflict_banner(ui, pal);
            }
            if self.reading.is_some() {
                self.reader(ui, pal);
            } else {
                self.toolbar(ui, pal);
                ui.add_space(4.0);
                self.table(ui, pal);
            }
            // dropping files: show what will happen
            let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
            if hovering {
                let r = ui.max_rect();
                ui.painter().rect_filled(r, 8, pal.accent.gamma_multiply(0.12));
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Drop PDFs, .bib or CSL-JSON files to add them to the library", FontId::proportional(16.0), pal.text);
            }
        });
        // dragged entries follow the pointer
        if let Some(keys) = egui::DragAndDrop::payload::<Vec<String>>(ui.ctx())
            && let Some(p) = ui.ctx().pointer_latest_pos()
        {
            let t = if keys.len() == 1 { keys[0].clone() } else { format!("{} entries", keys.len()) };
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("refs-drag")));
            let g = painter.layout_no_wrap(t, FontId::proportional(13.0), pal.text);
            let r = egui::Rect::from_min_size(p + vec2(14.0, 8.0), g.size() + vec2(12.0, 6.0));
            painter.rect_filled(r, 4, pal.panel2);
            painter.galley(r.min + vec2(6.0, 3.0), g, pal.text);
        }
    }

    /// List keys while no text field has focus.
    fn keys(&mut self, ctx: &egui::Context) {
        if ctx.memory(|m| m.focused().is_some()) || self.reading.is_some() {
            if self.reading.is_some() && ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.reading = None;
            }
            return;
        }
        let (up, down, del, enter, undo, all, paste) = ctx.input(|i| {
            let paste = i.events.iter().find_map(|e| if let egui::Event::Paste(s) = e { Some(s.clone()) } else { None });
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Delete),
                i.key_pressed(Key::Enter),
                i.modifiers.command && i.key_pressed(Key::Z),
                i.modifiers.command && i.key_pressed(Key::A),
                paste,
            )
        });
        if let Some(t) = paste.filter(|t| !t.trim().is_empty()) {
            self.add_text(&t);
            return;
        }
        if up || down {
            let cur = self.selected.last().and_then(|k| self.rows.iter().position(|r| &r.key == k));
            let next = match (cur, up) {
                (None, _) => 0,
                (Some(i), true) => i.saturating_sub(1),
                (Some(i), false) => (i + 1).min(self.rows.len().saturating_sub(1)),
            };
            if let Some(r) = self.rows.get(next) {
                let k = r.key.clone();
                self.anchor = Some(next);
                self.select(vec![k]);
                self.scroll_to_selected = true;
            }
        }
        if del && !self.selected.is_empty() {
            let keys = self.selected.clone();
            self.delete(&keys);
        }
        if enter
            && let Some(k) = self.selected.last().cloned()
        {
            self.open_first_file(&k);
        }
        if undo {
            self.undo_delete();
        }
        if all {
            let keys = self.rows.iter().map(|r| r.key.clone()).collect();
            self.select(keys);
        }
    }

    fn open_first_file(&mut self, key: &str) {
        let Ok(lib) = &self.lib else { return };
        match lib.files(key).into_iter().next() {
            Some(f) => self.read(&f),
            None => self.say(format!("{key} has no attachment")),
        }
    }

    /// Opens an attachment in the built-in viewer (other files with the desktop).
    fn read(&mut self, path: &Path) {
        if !path.is_file() {
            self.say(format!("File not found: {}", path.display()));
            return;
        }
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            open_external(path);
            return;
        }
        self.pdf.get_or_insert_with(|| PdfView::new(&self.ctx)).open(path);
        self.reading = Some(path.to_path_buf());
    }

    fn conflict_banner(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        egui::Frame::new().fill(pal.warn.gamma_multiply(0.15)).corner_radius(6).inner_margin(8).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("⚠ library.bib was changed by another program while you had unsaved changes.").color(pal.warn));
                if ui.button("Reload from disk").on_hover_text("Discards your unsaved changes").clicked()
                    && let Ok(lib) = &mut self.lib
                {
                    match lib.reload() {
                        Ok(()) => {
                            self.conflict = false;
                            self.selected.retain(|k| lib.contains(k));
                            self.rows_dirty = true;
                            self.draft = None;
                            self.load_draft();
                            self.status = Some("Library reloaded".into());
                            self.version += 1;
                        }
                        Err(e) => self.status = Some(format!("Cannot reload: {e}")),
                    }
                }
                if ui.button("Keep mine").on_hover_text("Overwrites the file (yesterday's version stays in backups/)").clicked() {
                    self.apply_draft();
                    if let Ok(lib) = &mut self.lib {
                        match lib.save(true) {
                            Ok(()) => {
                                self.conflict = false;
                                self.status = Some("Library saved".into());
                            }
                            Err(e) => self.status = Some(format!("Could not save the library: {e}")),
                        }
                    }
                }
            });
        });
        ui.add_space(6.0);
    }

    // ------------------------------------------------------------ side panel

    fn sidebar(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let total = self.side.total;
        let unfiled = self.side.unfiled;
        let paths: Vec<String> = self.side.collections.iter().map(|c| c.0.clone()).collect();
        let counts: Vec<usize> = self.side.collections.iter().map(|c| c.1).collect();
        let tags = std::mem::take(&mut self.side.tags);
        let mut pick = None;
        let mut drop_into: Option<(String, Vec<String>)> = None;
        let mut menu_action: Option<(&'static str, String)> = None;
        let mut naming_done = None;

        ui.label(RichText::new("LIBRARY").small().strong().color(pal.dim));
        egui::ScrollArea::vertical().id_salt("refs-side-scroll").auto_shrink([false, false]).show(ui, |ui| {
            if side_item(ui, pal, "📚 All entries", total, self.filter == Filter::All, 0.0).clicked() {
                pick = Some(Filter::All);
            }
            if side_item(ui, pal, "Unfiled", unfiled, self.filter == Filter::Unfiled, 0.0).clicked() {
                pick = Some(Filter::Unfiled);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("COLLECTIONS").small().strong().color(pal.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("+").on_hover_text("New collection").clicked() {
                        self.naming = Some((String::new(), String::new(), false));
                    }
                });
            });
            let mut hidden_below: Option<String> = None;
            for (p, n) in paths.iter().zip(&counts) {
                if let Some(h) = &hidden_below {
                    if p.starts_with(&format!("{h}/")) {
                        continue;
                    }
                    hidden_below = None;
                }
                let depth = p.matches('/').count() as f32;
                let name = p.rsplit('/').next().unwrap_or(p);
                let has_children = paths.iter().any(|c| c.starts_with(&format!("{p}/")));
                let collapsed = self.collapsed.contains(p);
                if collapsed {
                    hidden_below = Some(p.clone());
                }
                let r = side_item(ui, pal, name, *n, self.filter == Filter::Collection(p.clone()), depth * 12.0 + 14.0);
                if has_children {
                    // fold arrow
                    let c = egui::pos2(r.rect.min.x + depth * 12.0 + 12.0, r.rect.center().y);
                    let pts = if collapsed { vec![c + vec2(-2.5, -4.0), c + vec2(3.0, 0.0), c + vec2(-2.5, 4.0)] } else { vec![c + vec2(-4.0, -2.5), c + vec2(4.0, -2.5), c + vec2(0.0, 3.0)] };
                    ui.painter().add(egui::Shape::convex_polygon(pts, pal.dim, egui::Stroke::NONE));
                }
                if let Some(keys) = r.dnd_release_payload::<Vec<String>>() {
                    drop_into = Some((p.clone(), (*keys).clone()));
                }
                if r.dnd_hover_payload::<Vec<String>>().is_some() {
                    ui.painter().rect_stroke(r.rect, 4, egui::Stroke::new(1.5, pal.accent), egui::StrokeKind::Inside);
                }
                if r.clicked() {
                    // clicking the arrow folds, clicking the name filters
                    let arrow = r.rect.min.x + depth * 12.0 + 20.0;
                    if has_children && r.interact_pointer_pos().is_some_and(|q| q.x < arrow) {
                        if collapsed {
                            self.collapsed.remove(p);
                        } else {
                            self.collapsed.insert(p.clone());
                        }
                    } else {
                        pick = Some(Filter::Collection(p.clone()));
                    }
                }
                r.context_menu(|ui| {
                    if ui.button("New Sub-collection…").clicked() {
                        menu_action = Some(("sub", p.clone()));
                        ui.close();
                    }
                    if ui.button("Rename…").clicked() {
                        menu_action = Some(("rename", p.clone()));
                        ui.close();
                    }
                    if ui.button("Delete Collection").on_hover_text("The entries stay in the library").clicked() {
                        menu_action = Some(("delete", p.clone()));
                        ui.close();
                    }
                });
            }
            if paths.is_empty() && self.naming.is_none() {
                ui.label(RichText::new("Drag entries onto a collection to file them.").small().color(pal.dim));
            }
            if let Some(ok) = naming_field(ui, pal, &mut self.naming) {
                naming_done = Some(ok);
            }
            if !tags.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new("TAGS").small().strong().color(pal.dim));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for (t, n) in &tags {
                        let on = self.filter == Filter::Tag(t.clone());
                        let r = ui.selectable_label(on, RichText::new(format!("#{t}")).small()).on_hover_text(format!("{n} entr{}", if *n == 1 { "y" } else { "ies" }));
                        if r.clicked() {
                            pick = Some(if on { Filter::All } else { Filter::Tag(t.clone()) });
                        }
                    }
                });
            }
        });
        self.side.tags = tags;
        if let Some(f) = pick {
            self.filter = f;
            self.rows_dirty = true;
        }
        if let Some((col, keys)) = drop_into
            && let Ok(lib) = &mut self.lib
        {
            let n = keys.iter().filter(|k| lib.collections.add(&col, k)).count();
            self.persist();
            self.say(format!("Added {n} entr{} to {col}", if n == 1 { "y" } else { "ies" }));
        }
        if let Some(ok) = naming_done {
            self.finish_naming(ok);
        }
        match menu_action {
            Some(("sub", p)) => self.naming = Some((format!("{p}/"), String::new(), false)),
            Some(("rename", p)) => {
                let name = p.rsplit('/').next().unwrap_or(&p).to_string();
                self.naming = Some((p, name, true));
            }
            Some(("delete", p)) => {
                if let Ok(lib) = &mut self.lib {
                    lib.collections.delete(&p);
                }
                if matches!(&self.filter, Filter::Collection(c) if c == &p || c.starts_with(&format!("{p}/"))) {
                    self.filter = Filter::All;
                }
                self.persist();
                self.say(format!("Deleted collection {p}"));
            }
            _ => {}
        }
    }

    /// Creates or renames the collection named in the inline field.
    fn finish_naming(&mut self, ok: bool) {
        let Some((base, text, rename)) = self.naming.take() else { return };
        let name = text.trim().replace('/', "-");
        if !ok || name.is_empty() {
            return;
        }
        let Ok(lib) = &mut self.lib else { return };
        if rename {
            let target = match base.rsplit_once('/') {
                Some((parent, _)) => format!("{parent}/{name}"),
                None => name,
            };
            match lib.collections.rename(&base, &target) {
                Ok(()) => {
                    if self.filter == Filter::Collection(base.clone()) {
                        self.filter = Filter::Collection(target);
                    }
                    self.persist();
                }
                Err(e) => self.status = Some(format!("Cannot rename: {e}")),
            }
        } else if let Some(p) = lib.collections.create(&format!("{base}{name}")) {
            self.filter = Filter::Collection(p);
            self.persist();
        }
    }

    // ------------------------------------------------------------ toolbar and table

    fn toolbar(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.horizontal(|ui| {
            let w = (ui.available_width() * 0.45).clamp(220.0, 520.0);
            let r = ui.add(egui::TextEdit::singleline(&mut self.add_input).hint_text("DOI, arXiv ID, ISBN or BibTeX – Enter adds").desired_width(w));
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if (ui.add_enabled(!self.add_input.trim().is_empty(), egui::Button::new("➕ Add")).clicked() || enter) && !self.add_input.trim().is_empty() {
                let t = std::mem::take(&mut self.add_input);
                self.add_text(&t);
            }
            if ui.button("📂 Import…").on_hover_text("PDFs, .bib or CSL-JSON files").clicked()
                && let Some(files) = rfd::FileDialog::new().add_filter("PDF, BibTeX, CSL-JSON", &["pdf", "bib", "json", "txt"]).pick_files()
            {
                self.import_paths(&files);
            }
            if self.pending > 0 {
                ui.spinner();
                ui.label(RichText::new(format!("{} pending", self.pending)).small().color(pal.dim));
                if ui.small_button("Cancel").clicked() {
                    self.cancel.store(true, Ordering::Relaxed);
                }
            }
            if !self.undo.is_empty() && ui.button("⟲ Undo delete").on_hover_text("Ctrl+Z").clicked() {
                self.undo_delete();
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("🔍 Search author, title, year, key, journal …  (tag:x  type:article)").desired_width(ui.available_width() - 150.0));
            if r.changed() {
                self.rows_dirty = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let total = self.lib.as_ref().map_or(0, |l| l.len());
                let shown = self.rows.len();
                let t = if shown == total { format!("{total} entries") } else { format!("{shown} of {total}") };
                ui.label(RichText::new(t).small().color(pal.dim)).on_hover_text(self.dir.join(lf_ref::library::LIBRARY_FILE).display().to_string());
            });
        });
    }

    fn table(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let w = ui.available_width();
        let year_w = 46.0;
        let clip_w = 22.0;
        let author_w = (w * 0.2).clamp(90.0, 220.0);
        let cont_w = (w * 0.22).clamp(80.0, 260.0);
        let title_w = (w - author_w - year_w - cont_w - clip_w - 4.0 * 8.0).max(80.0);
        let cols = [(Col::Author, "Author", author_w), (Col::Year, "Year", year_w), (Col::Title, "Title", title_w), (Col::Container, "Journal / Publisher", cont_w)];
        let font = FontId::proportional(13.0);
        // header
        let (hr, _) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::hover());
        let mut x = hr.min.x + 4.0;
        let mut resort = None;
        for (c, name, cw) in cols {
            let rect = egui::Rect::from_min_size(egui::pos2(x, hr.min.y), vec2(cw, ROW_H));
            let resp = ui.interact(rect, ui.id().with(("refs-head", name)), Sense::click());
            let color = if resp.hovered() || self.sort.0 == c { pal.text } else { pal.dim };
            let tr = ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, name, FontId::proportional(12.0), color);
            if self.sort.0 == c {
                let m = egui::pos2(tr.max.x + 8.0, rect.center().y);
                let pts = if self.sort.1 { vec![m + vec2(-4.0, 2.5), m + vec2(4.0, 2.5), m + vec2(0.0, -3.0)] } else { vec![m + vec2(-4.0, -2.5), m + vec2(4.0, -2.5), m + vec2(0.0, 3.0)] };
                ui.painter().add(egui::Shape::convex_polygon(pts, pal.accent, egui::Stroke::NONE));
            }
            if resp.clicked() {
                resort = Some(c);
            }
            x += cw + 8.0;
        }
        ui.painter().text(egui::pos2(x, hr.center().y), egui::Align2::LEFT_CENTER, "📎", FontId::proportional(12.0), pal.dim);
        ui.painter().hline(hr.x_range(), hr.max.y, egui::Stroke::new(1.0, pal.border));
        if let Some(c) = resort {
            self.sort = match self.sort {
                (old, asc) if old == c => (c, !asc),
                _ => (c, c != Col::Year),
            };
            self.rows_dirty = true;
        }
        if self.rows.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                let empty = self.lib.as_ref().is_ok_and(|l| l.is_empty());
                let msg = if empty {
                    "The library is empty.\n\nPaste a DOI, arXiv ID, ISBN or BibTeX into the field above (or press Ctrl+V here),\nor drop PDFs and .bib files onto this window."
                } else {
                    "No matching entries."
                };
                ui.label(RichText::new(msg).color(pal.dim));
            });
            return;
        }
        let mut clicked: Option<(usize, egui::Modifiers)> = None;
        let mut double = None;
        let mut menu: Option<(usize, RowMenu)> = None;
        let selected: HashSet<&str> = self.selected.iter().map(String::as_str).collect();
        let scroll_row = if std::mem::take(&mut self.scroll_to_selected) { self.selected.last().and_then(|k| self.rows.iter().position(|r| &r.key == k)) } else { None };
        let mut sa = egui::ScrollArea::vertical().id_salt("refs-table").auto_shrink([false, false]);
        if let Some(i) = scroll_row {
            let row_h = ROW_H + ui.spacing().item_spacing.y;
            let view_h = ui.available_height();
            sa = sa.vertical_scroll_offset((i as f32 * row_h - view_h * 0.4).max(0.0));
        }
        let collections: Vec<String> = self.lib.as_ref().map(|l| l.collections.collections.keys().cloned().collect()).unwrap_or_default();
        let in_collection = match &self.filter {
            Filter::Collection(c) => Some(c.clone()),
            _ => None,
        };
        let drag_keys = |i: usize, rows: &[EntryView]| -> Vec<String> { if selected.contains(rows[i].key.as_str()) { self.selected.clone() } else { vec![rows[i].key.clone()] } };
        sa.show_rows(ui, ROW_H, self.rows.len(), |ui, range| {
            for i in range {
                let v = &self.rows[i];
                let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click_and_drag());
                let sel = selected.contains(v.key.as_str());
                if sel {
                    ui.painter().rect_filled(rect, 4, pal.accent.gamma_multiply(0.25));
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 4, pal.panel2);
                } else if i % 2 == 1 {
                    ui.painter().rect_filled(rect, 0, pal.panel.gamma_multiply(0.5));
                }
                let mut x = rect.min.x + 4.0;
                let texts = [&v.authors, &v.year, &v.title, &v.container];
                for (k, (_, _, cw)) in cols.iter().enumerate() {
                    let color = if k == 2 { pal.text } else { pal.dim };
                    let g = cell(ui, texts[k], *cw, &font, color);
                    ui.painter().galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, color);
                    x += cw + 8.0;
                }
                if v.has_file {
                    ui.painter().text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, "📎", FontId::proportional(12.0), pal.accent2);
                }
                if resp.clicked() {
                    clicked = Some((i, ui.input(|i| i.modifiers)));
                }
                if resp.double_clicked() {
                    double = Some(i);
                }
                if resp.dragged() && egui::DragAndDrop::payload::<Vec<String>>(ui.ctx()).is_none() {
                    resp.dnd_set_drag_payload(drag_keys(i, &self.rows));
                }
                resp.context_menu(|ui| {
                    if let Some(a) = row_menu(ui, v, &collections, in_collection.as_deref()) {
                        menu = Some((i, a));
                    }
                });
            }
        });
        if let Some((i, m)) = clicked {
            let key = self.rows[i].key.clone();
            if m.command {
                let mut s = self.selected.clone();
                if let Some(p) = s.iter().position(|k| *k == key) {
                    s.remove(p);
                } else {
                    s.push(key);
                }
                self.select(s);
                self.anchor = Some(i);
            } else if m.shift
                && let Some(a) = self.anchor
            {
                let (lo, hi) = (a.min(i), a.max(i));
                let mut s: Vec<String> = self.rows[lo..=hi].iter().map(|r| r.key.clone()).collect();
                // the clicked row is the one shown in the editor
                if let Some(p) = s.iter().position(|k| *k == key) {
                    let k = s.remove(p);
                    s.push(k);
                }
                self.select(s);
            } else {
                self.select(vec![key]);
                self.anchor = Some(i);
            }
        }
        if let Some(i) = double {
            let k = self.rows[i].key.clone();
            self.open_first_file(&k);
        }
        if let Some((i, a)) = menu {
            // the menu acts on the selection if the row belongs to it
            let key = self.rows[i].key.clone();
            let keys = if self.selected.contains(&key) { self.selected.clone() } else { vec![key.clone()] };
            self.row_action(&keys, a, ui.ctx());
        }
    }

    fn row_action(&mut self, keys: &[String], a: RowMenu, ctx: &egui::Context) {
        let Ok(lib) = &mut self.lib else { return };
        match a {
            RowMenu::CopyKey => ctx.copy_text(keys.join(",")),
            RowMenu::CopyCite => ctx.copy_text(format!("\\cite{{{}}}", keys.join(","))),
            RowMenu::CopyBib => {
                let src: Vec<String> = keys.iter().filter_map(|k| lib.source(k)).collect();
                ctx.copy_text(src.join("\n\n") + "\n");
                self.say(format!("Copied {} entr{} as BibLaTeX", src.len(), if src.len() == 1 { "y" } else { "ies" }));
            }
            RowMenu::OpenPdf => {
                let k = keys[0].clone();
                self.open_first_file(&k);
            }
            RowMenu::OpenDoi(doi) => open_external(Path::new(&format!("https://doi.org/{doi}"))),
            RowMenu::AddTo(c) => {
                let n = keys.iter().filter(|k| lib.collections.add(&c, k)).count();
                self.persist();
                self.say(format!("Added {n} entr{} to {c}", if n == 1 { "y" } else { "ies" }));
            }
            RowMenu::NewCollection => {
                self.naming = Some((String::new(), String::new(), false));
                self.say("Name the collection, then drag the entries onto it");
            }
            RowMenu::RemoveFrom(c) => {
                for k in keys {
                    lib.collections.remove(&c, k);
                }
                self.persist();
            }
            RowMenu::Delete => self.delete(keys),
        }
    }

    // ------------------------------------------------------------ reader

    fn reader(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let path = self.reading.clone().unwrap_or_default();
        ui.horizontal(|ui| {
            if ui.button("⬅ Library").on_hover_text("Esc").clicked() {
                self.reading = None;
            }
            ui.label(RichText::new(path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).strong());
        });
        let Some(pdf) = &mut self.pdf else { return };
        pdf.toolbar(ui, pal);
        egui::Frame::new().fill(pal.canvas).corner_radius(6).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            pdf.ui(ui, pal, false);
        });
    }

    // ------------------------------------------------------------ detail editor

    fn detail(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        if self.selected.len() > 1 {
            ui.label(RichText::new(format!("{} entries selected", self.selected.len())).strong());
            ui.add_space(8.0);
            ui.label(RichText::new("Right-click for copy, collections and delete; drag them onto a collection.").color(pal.dim));
            return;
        }
        let Some(d) = &mut self.draft else {
            ui.add_space(20.0);
            ui.label(RichText::new("Select an entry to see and edit it.").color(pal.dim));
            return;
        };
        let Ok(lib) = &self.lib else { return };
        let mut action: Option<DetailAction> = None;
        // widths come from the panel, not from the content (the panel would grow with it)
        let full = ui.available_width();
        let field_w = (full - LABEL_W - 48.0).max(80.0);
        // header: type and key
        ui.horizontal(|ui| {
            let before = d.kind.clone();
            egui::ComboBox::from_id_salt("refs-kind").selected_text(RichText::new(&d.kind).strong()).width(130.0).show_ui(ui, |ui| {
                if !types::KINDS.contains(&d.kind.as_str()) {
                    let k = d.kind.clone();
                    ui.selectable_value(&mut d.kind, k.clone(), k);
                }
                for k in types::KINDS {
                    ui.selectable_value(&mut d.kind, k.to_string(), *k);
                }
            });
            if d.kind != before {
                d.touch();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.selectable_label(matches!(self.tab, Tab::Source), "BibLaTeX").on_hover_text("Edit the entry as source").clicked() {
                    self.tab = Tab::Source;
                }
                if ui.selectable_label(matches!(self.tab, Tab::Fields), "Fields").clicked() {
                    self.tab = Tab::Fields;
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Key").color(pal.dim));
            let r = ui.add(egui::TextEdit::singleline(&mut d.new_key).font(egui::TextStyle::Monospace).desired_width((full - 110.0).max(80.0)));
            if r.changed() {
                d.touch();
            }
            if ui.small_button("📋").on_hover_text("Copy key").clicked() {
                ui.ctx().copy_text(d.key.clone());
            }
            if ui.small_button("↻").on_hover_text("Generate the key from the key pattern").clicked() {
                let mut probe = lib.entry(&d.key).cloned().unwrap_or_default();
                probe.key = String::new();
                let k = lf_ref::key::base_key(&d.build(&probe), lib.strings(), &lib.key_pattern);
                d.new_key = lf_ref::key::unique(&k, |x| !x.eq_ignore_ascii_case(&d.key) && lib.contains(x));
                d.touch();
            }
        });
        if let Some(e) = &d.error {
            ui.label(RichText::new(e).color(pal.error));
        }
        ui.separator();
        match self.tab {
            Tab::Source => {
                let font = FontId::monospace(12.5);
                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| -> Arc<Galley> {
                    let mut job = crate::editor::highlight(buf.as_str(), &font, pal, &[]);
                    job.wrap.max_width = wrap;
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                egui::ScrollArea::vertical().id_salt("refs-src").max_height(ui.available_height() - 40.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut d.raw).code_editor().desired_width(full - 24.0).desired_rows(16).layouter(&mut layouter));
                });
                ui.horizontal(|ui| {
                    let changed = d.raw != d.raw_orig;
                    if ui.add_enabled(changed, egui::Button::new("Apply")).clicked() {
                        action = Some(DetailAction::ApplySource);
                    }
                    if ui.add_enabled(changed, egui::Button::new("Revert")).clicked() {
                        d.raw = d.raw_orig.clone();
                    }
                });
            }
            Tab::Fields => {
                let files = lib.files(&d.key);
                let doi = lib.view(&d.key).and_then(|v| v.doi.clone());
                let missing: Vec<&str> = types::required(&d.kind).iter().filter(|g| !g.split('|').any(|f| d.fields.iter().any(|x| x.name.eq_ignore_ascii_case(f) && !x.text.trim().is_empty()))).map(|g| g.split('|').next().unwrap_or("")).collect();
                egui::ScrollArea::vertical().id_salt("refs-fields").auto_shrink([false, false]).show(ui, |ui| {
                    let mut remove = None;
                    for (i, f) in d.fields.iter_mut().enumerate() {
                        let miss = missing.iter().any(|m| m.eq_ignore_ascii_case(&f.name));
                        ui.horizontal_top(|ui| {
                            let (lr, _) = ui.allocate_exact_size(vec2(LABEL_W, 20.0), Sense::hover());
                            let lg = cell(ui, &f.name, LABEL_W - 4.0, &FontId::proportional(12.5), if miss { pal.warn } else { pal.dim });
                            ui.painter().galley(egui::pos2(lr.min.x, lr.center().y - lg.size().y / 2.0), lg, pal.dim);
                            let width = field_w;
                            if f.composite {
                                ui.add_sized([width, 20.0], egui::Label::new(RichText::new(&f.text).monospace()).wrap()).on_hover_text("Uses @string macros or #: edit it in the BibLaTeX tab");
                            } else {
                                let long = matches!(f.name.as_str(), "abstract" | "title" | "author" | "editor") || f.text.len() > 50;
                                let boxed = f.name == "abstract" || f.text.len() > 400;
                                let edit = if long { egui::TextEdit::multiline(&mut f.text).desired_rows(1) } else { egui::TextEdit::singleline(&mut f.text) };
                                // long texts (abstracts) scroll inside a box of limited height
                                let r = if boxed {
                                    egui::ScrollArea::vertical().id_salt(("refs-long", i)).max_height(110.0).show(ui, |ui| ui.add(edit.desired_width(width - 12.0))).inner
                                } else {
                                    ui.add(edit.desired_width(width))
                                };
                                if miss {
                                    r.clone().on_hover_text("Required for this entry type");
                                }
                                if r.changed() {
                                    d.changed = Some(Instant::now());
                                    d.error = None;
                                }
                            }
                            if ui.small_button("×").on_hover_text("Remove field").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        let f = d.fields.remove(i);
                        d.removed.push(f.name);
                        d.touch();
                    }
                    ui.horizontal(|ui| {
                        ui.menu_button("+ Field", |ui| {
                            for name in types::optional(&d.kind) {
                                if d.fields.iter().any(|f| f.name.eq_ignore_ascii_case(name)) {
                                    continue;
                                }
                                if ui.button(*name).clicked() {
                                    d.removed.retain(|r| !r.eq_ignore_ascii_case(name));
                                    d.fields.push(FieldEdit { name: name.to_string(), text: String::new(), orig: String::new(), composite: false });
                                    ui.close();
                                }
                            }
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut d.custom_field).hint_text("other field").desired_width(110.0));
                                let name = d.custom_field.trim().to_lowercase();
                                let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:+".contains(c)) && !d.fields.iter().any(|f| f.name.eq_ignore_ascii_case(&name));
                                if ui.add_enabled(ok, egui::Button::new("Add")).clicked() {
                                    d.removed.retain(|r| !r.eq_ignore_ascii_case(&name));
                                    d.fields.push(FieldEdit { name, text: String::new(), orig: String::new(), composite: false });
                                    d.custom_field.clear();
                                    ui.close();
                                }
                            });
                        });
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Tags").color(pal.dim));
                    if ui.add(egui::TextEdit::singleline(&mut d.tags).hint_text("comma-separated").desired_width(full - 20.0)).changed() {
                        d.touch();
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Files").color(pal.dim));
                        if ui.small_button("+ Attach…").clicked() {
                            action = Some(DetailAction::Attach);
                        }
                    });
                    if files.is_empty() {
                        ui.label(RichText::new("No attachments – drop a PDF on the window or attach one.").small().color(pal.dim));
                    }
                    for f in &files {
                        ui.horizontal(|ui| {
                            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            let exists = f.is_file();
                            let t = RichText::new(format!("📎 {name}")).color(if exists { pal.accent } else { pal.error });
                            if ui.add(egui::Label::new(t).sense(Sense::click())).on_hover_text(if exists { f.display().to_string() } else { format!("missing: {}", f.display()) }).clicked() {
                                action = Some(DetailAction::Read(f.clone()));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.menu_button("…", |ui| {
                                    if ui.button("Open with system viewer").clicked() {
                                        open_external(f);
                                        ui.close();
                                    }
                                    if ui.button("Show in folder").clicked() {
                                        if let Some(p) = f.parent() {
                                            open_external(p);
                                        }
                                        ui.close();
                                    }
                                    if ui.button("Remove link").clicked() {
                                        action = Some(DetailAction::Detach(f.clone(), false));
                                        ui.close();
                                    }
                                    if ui.button("Remove and delete file").on_hover_text("Only files inside the library folder are deleted").clicked() {
                                        action = Some(DetailAction::Detach(f.clone(), true));
                                        ui.close();
                                    }
                                });
                            });
                        });
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Notes").color(pal.dim));
                    if ui.add(egui::TextEdit::multiline(&mut d.notes).hint_text("annotation").desired_rows(3).desired_width(full - 20.0)).changed() {
                        d.touch();
                    }
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Copy \\cite").clicked() {
                            ui.ctx().copy_text(format!("\\cite{{{}}}", d.key));
                        }
                        if ui.button("Copy BibLaTeX").clicked() {
                            action = Some(DetailAction::CopyBib);
                        }
                        if let Some(doi) = &doi
                            && ui.button("DOI ↗").on_hover_text(format!("https://doi.org/{doi}")).clicked()
                        {
                            open_external(Path::new(&format!("https://doi.org/{doi}")));
                        }
                        if ui.button(RichText::new("Delete").color(pal.error)).on_hover_text("Del – undo with Ctrl+Z").clicked() {
                            action = Some(DetailAction::Delete);
                        }
                    });
                    if let Some(v) = lib.entry(&d.key).and_then(|e| e.raw_field(library::ADDED, lib.strings())) {
                        ui.add_space(6.0);
                        ui.label(RichText::new(format!("Added {}", v.replace('T', " ").trim_end_matches('Z'))).small().color(pal.dim));
                    }
                });
            }
        }
        let key = self.draft.as_ref().map(|d| d.key.clone()).unwrap_or_default();
        match action {
            Some(DetailAction::ApplySource) => self.apply_source(),
            Some(DetailAction::Attach) => {
                if let Some(files) = rfd::FileDialog::new().pick_files() {
                    self.apply_draft();
                    let key = self.draft.as_ref().map(|d| d.key.clone()).unwrap_or(key);
                    if let Ok(lib) = &mut self.lib {
                        for f in files {
                            if let Err(e) = lib.attach(&key, &f) {
                                self.status = Some(format!("Cannot attach {}: {e}", f.display()));
                            }
                        }
                    }
                    self.persist();
                }
            }
            Some(DetailAction::Detach(f, delete)) => {
                self.apply_draft();
                if let Ok(lib) = &mut self.lib
                    && let Err(e) = lib.detach(&key, &f, delete)
                {
                    self.status = Some(e);
                }
                self.persist();
            }
            Some(DetailAction::Read(f)) => self.read(&f),
            Some(DetailAction::CopyBib) => {
                if let Ok(lib) = &self.lib
                    && let Some(s) = lib.source(&key)
                {
                    ui.ctx().copy_text(s + "\n");
                }
            }
            Some(DetailAction::Delete) => self.delete(&[key]),
            None => {}
        }
    }

    fn apply_source(&mut self) {
        let (Ok(lib), Some(d)) = (&mut self.lib, &mut self.draft) else { return };
        let parsed = lf_ref::bib::parse(&d.raw);
        let entries: Vec<Entry> = parsed.entries().cloned().collect();
        let problems = parsed.problems();
        let e = match (entries.len(), problems.first()) {
            (_, Some((line, msg))) => {
                d.error = Some(format!("line {line}: {msg}"));
                return;
            }
            (1, None) => entries.into_iter().next().unwrap(),
            (n, None) => {
                d.error = Some(format!("expected one entry, found {n}"));
                return;
            }
        };
        match lib.update(&d.key.clone(), e) {
            Ok(k) => {
                self.selected = vec![k];
                self.persist();
                self.load_draft();
                self.tab = Tab::Source;
            }
            Err(msg) => d.error = Some(msg),
        }
    }
}

enum DetailAction {
    ApplySource,
    Attach,
    Detach(PathBuf, bool),
    Read(PathBuf),
    CopyBib,
    Delete,
}

enum RowMenu {
    CopyKey,
    CopyCite,
    CopyBib,
    OpenPdf,
    OpenDoi(String),
    AddTo(String),
    NewCollection,
    RemoveFrom(String),
    Delete,
}

fn row_menu(ui: &mut egui::Ui, v: &EntryView, collections: &[String], current: Option<&str>) -> Option<RowMenu> {
    let mut a = None;
    let mut item = |ui: &mut egui::Ui, label: &str, m: RowMenu| {
        if ui.button(label).clicked() {
            a = Some(m);
            ui.close();
        }
    };
    item(ui, "Copy \\cite{…}", RowMenu::CopyCite);
    item(ui, "Copy Key", RowMenu::CopyKey);
    item(ui, "Copy BibLaTeX", RowMenu::CopyBib);
    ui.separator();
    if v.has_file {
        item(ui, "Open PDF", RowMenu::OpenPdf);
    }
    if let Some(d) = &v.doi {
        item(ui, "Open DOI ↗", RowMenu::OpenDoi(d.clone()));
    }
    ui.menu_button("Add to Collection", |ui| {
        for c in collections {
            item(ui, c, RowMenu::AddTo(c.clone()));
        }
        if !collections.is_empty() {
            ui.separator();
        }
        item(ui, "New Collection…", RowMenu::NewCollection);
    });
    if let Some(c) = current {
        item(ui, &format!("Remove from \"{c}\""), RowMenu::RemoveFrom(c.to_string()));
    }
    ui.separator();
    item(ui, "Delete", RowMenu::Delete);
    a
}

fn run_job(job: Job, opts: &NetOptions, cancel: &AtomicBool) -> Done {
    let cancelled = |e: &str| e == NetError::Cancelled.to_string();
    if cancel.load(Ordering::Relaxed) {
        let what = match &job {
            Job::Fetch(id) => id.to_string(),
            Job::Pdf(p) => p.display().to_string(),
        };
        return Done::Failed { what, error: "cancelled".into(), cancelled: true, attach: None, id: None };
    }
    match job {
        Job::Fetch(id) => match import::fetch(&id, opts, cancel) {
            Ok(e) => Done::Found { entry: Box::new(e), what: id.to_string(), attach: None },
            Err(e) => {
                let error = e.to_string();
                Done::Failed { what: id.to_string(), cancelled: cancelled(&error), error, attach: None, id: None }
            }
        },
        Job::Pdf(p) => {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            match import::pdf::identify(&p) {
                Ok(Some(id)) => match import::fetch(&id, opts, cancel) {
                    Ok(e) => Done::Found { entry: Box::new(e), what: format!("{name} ({id})"), attach: Some(p) },
                    Err(e) => {
                        let error = e.to_string();
                        Done::Failed { what: format!("{name} ({id})"), cancelled: cancelled(&error), error, attach: Some(p), id: Some(id) }
                    }
                },
                Ok(None) => Done::Failed { what: name, error: "no DOI or arXiv ID found".into(), cancelled: false, attach: Some(p), id: None },
                Err(e) => Done::Failed { what: name, error: e, cancelled: false, attach: Some(p), id: None },
            }
        }
    }
}

/// Inline name field for a new or renamed collection: `Some(true)` on Enter, `Some(false)`
/// when cancelled.
fn naming_field(ui: &mut egui::Ui, pal: &Palette, naming: &mut Option<(String, String, bool)>) -> Option<bool> {
    let (base, text, rename) = naming.as_mut()?;
    let mut done = None;
    ui.horizontal(|ui| {
        let hint = if *rename { "New name" } else if base.is_empty() { "Collection name" } else { "Sub-collection name" };
        let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(ui.available_width() - 30.0));
        if !r.has_focus() && !r.lost_focus() {
            r.request_focus();
        }
        if r.lost_focus() {
            done = Some(ui.input(|i| i.key_pressed(Key::Enter)));
        }
        if ui.small_button("×").clicked() {
            done = Some(false);
        }
    });
    if !base.is_empty() && !*rename {
        ui.label(RichText::new(format!("in {}", base.trim_end_matches('/'))).small().color(pal.dim));
    }
    done
}

fn side_item(ui: &mut egui::Ui, pal: &Palette, label: &str, count: usize, selected: bool, indent: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 4, pal.accent.gamma_multiply(0.22));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4, pal.panel2);
    }
    let font = FontId::proportional(13.0);
    let color = if selected { pal.text } else { pal.text.gamma_multiply(0.9) };
    let g = cell(ui, label, rect.width() - indent - 44.0, &font, color);
    ui.painter().galley(egui::pos2(rect.min.x + 6.0 + indent, rect.center().y - g.size().y / 2.0), g, color);
    ui.painter().text(egui::pos2(rect.max.x - 6.0, rect.center().y), egui::Align2::RIGHT_CENTER, count.to_string(), FontId::proportional(11.5), pal.dim);
    resp
}

/// Single-line text cut to `width` with an ellipsis.
fn cell(ui: &egui::Ui, text: &str, width: f32, font: &FontId, color: Color32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font.clone(), color);
    job.wrap = egui::text::TextWrapping { max_width: width.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.fonts_mut(|f| f.layout_job(job))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(tag: &str) -> (RefsView, PathBuf) {
        let dir = std::env::temp_dir().join(format!("lf-refs-ui-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cfg = Config { library_dir: Some(dir.clone()), ..Config::default() };
        (RefsView::new(&egui::Context::default(), &cfg), dir)
    }

    fn lib(v: &RefsView) -> &Library {
        v.lib.as_ref().unwrap()
    }

    /// Waits for the import worker.
    fn wait(v: &mut RefsView) {
        let t = Instant::now();
        while v.pending > 0 && t.elapsed() < Duration::from_secs(20) {
            v.receive();
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(v.pending, 0, "import did not finish");
    }

    #[test]
    fn paste_edit_rename_delete_undo() {
        let (mut v, dir) = view("edit");
        v.add_text("@article{x, author={Doe, Jane}, title={Kernels for Everyone}, journal={J. Test}, year=2025}\n@book{y, author={Roe, Richard}, title={Another Book}, year=2020}");
        assert_eq!(lib(&v).len(), 2);
        assert_eq!(v.selected, ["x", "y"], "imported entries are selected");
        v.select(vec!["x".into()]);

        // edit a field and the key; written after the delay (here: directly)
        let d = v.draft.as_mut().unwrap();
        d.fields.iter_mut().find(|f| f.name == "title").unwrap().text = "Kernels for {Everyone}".into();
        d.new_key = "doe2025kernels".into();
        d.tags = "ml, survey".into();
        d.notes = "read chapter 2".into();
        d.touch();
        v.apply_draft();
        assert_eq!(v.selected, ["doe2025kernels"]);
        let e = lib(&v).entry("doe2025kernels").unwrap();
        assert_eq!(e.raw_field("title", lib(&v).strings()).unwrap(), "Kernels for {Everyone}");
        assert_eq!(e.raw_field("annotation", lib(&v).strings()).unwrap(), "read chapter 2");
        assert_eq!(library::tags(e, lib(&v).strings()), ["ml", "survey"]);
        assert!(matches!(e.get("year").unwrap().0.as_slice(), [Piece::Number(_)]), "untouched fields keep their form");
        let saved = std::fs::read_to_string(dir.join("library.bib")).unwrap();
        assert!(saved.contains("@article{doe2025kernels,") && saved.contains("keywords = {ml, survey}"));

        // the key of another entry is refused
        let d = v.draft.as_mut().unwrap();
        d.new_key = "Y".into();
        d.touch();
        v.apply_draft();
        assert!(v.draft.as_ref().unwrap().error.as_deref().unwrap().contains("already used"));
        assert!(lib(&v).contains("doe2025kernels"));
        v.load_draft();

        // collection, delete, undo
        v.naming = Some((String::new(), "Thesis".into(), false));
        v.finish_naming(true);
        assert_eq!(v.filter, Filter::Collection("Thesis".into()));
        v.row_action(&["doe2025kernels".into()], RowMenu::AddTo("Thesis".into()), &egui::Context::default());
        v.delete(&["doe2025kernels".into()]);
        assert_eq!(lib(&v).len(), 1);
        assert!(lib(&v).collections.keys("Thesis", true).is_empty());
        v.undo_delete();
        assert_eq!(lib(&v).len(), 2);
        assert_eq!(lib(&v).collections.keys("Thesis", true), ["doe2025kernels"]);
        v.rebuild_rows();
        assert_eq!(v.rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["doe2025kernels"], "filtered by the collection");

        // pasting the same entry again is reported, not duplicated
        v.add_text("@article{z, author={Doe, Jane}, title={Kernels for Everyone}, year=2025}");
        assert_eq!(lib(&v).len(), 2);
        assert!(v.status.as_deref().unwrap().contains("already in the library (doe2025kernels)"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn source_tab_applies_one_entry() {
        let (mut v, dir) = view("source");
        v.add_text("@misc{a, title={One}}");
        v.select(vec!["a".into()]);
        v.draft.as_mut().unwrap().raw = "@misc{a, title={Two}} @misc{b, title={x}}".into();
        v.apply_source();
        assert!(v.draft.as_ref().unwrap().error.as_deref().unwrap().contains("found 2"));
        v.draft.as_mut().unwrap().raw = "@online{a2,\n  title = {Two},\n  url = {https://example.org},\n}".into();
        v.apply_source();
        assert_eq!(v.selected, ["a2"]);
        assert!(lib(&v).entry("a2").unwrap().is("online"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pdf_without_identifier_is_kept_with_its_name() {
        if std::process::Command::new("pdftotext").arg("-v").output().is_err() {
            return; // poppler not installed
        }
        let (mut v, dir) = view("pdf");
        let src = dir.join("in");
        std::fs::create_dir_all(&src).unwrap();
        let pdf = src.join("my_old-notes.pdf");
        std::fs::write(&pdf, blank_pdf()).unwrap();
        v.import_paths(std::slice::from_ref(&pdf));
        wait(&mut v);
        let key = v.selected.last().cloned().expect("an entry was created");
        let e = lib(&v).entry(&key).unwrap();
        assert!(e.is("misc"));
        assert_eq!(e.raw_field("title", lib(&v).strings()).unwrap(), "my old notes");
        let files = lib(&v).files(&key);
        assert_eq!(files.len(), 1);
        assert!(files[0].starts_with(dir.join("files")) && files[0].is_file());
        assert!(v.status.as_deref().unwrap().contains("no DOI or arXiv ID found"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ten_thousand_rows() {
        let (v, dir) = view("big");
        drop(v);
        let mut src = String::new();
        for i in 0..10_000 {
            src.push_str(&format!("@article{{k{i},\n  author = {{M{{\\\"u}}ller{}, J. and Smith, A.}},\n  title = {{Study number {i} of {} things}},\n  journaltitle = {{Journal {}}},\n  date = {{{}}},\n  keywords = {{t{}}},\n}}\n\n", i % 97, i % 13, i % 50, 1950 + i % 70, i % 20));
        }
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("library.bib"), src).unwrap();
        std::fs::write(dir.join("collections.toml"), "[collections]\n\"A\" = [\"k1\", \"k2\"]\n\"A/B\" = [\"k3\"]\n").unwrap();
        let cfg = Config { library_dir: Some(dir.clone()), ..Config::default() };
        let mut v = RefsView::new(&egui::Context::default(), &cfg);
        let t = Instant::now();
        v.rebuild_rows();
        let all = t.elapsed();
        assert_eq!(v.rows.len(), 10_000);
        assert_eq!(v.side.unfiled, 9_997);
        v.query = "mueller study 1999".into();
        let t = Instant::now();
        v.rebuild_rows();
        let search = t.elapsed();
        assert!(!v.rows.is_empty());
        v.sort = (Col::Title, true);
        v.query.clear();
        let t = Instant::now();
        v.rebuild_rows();
        let sorted = t.elapsed();
        eprintln!("10k rows: all {all:?}, search {search:?}, sorted by title {sorted:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A valid one-page PDF without text.
    fn blank_pdf() -> Vec<u8> {
        let objs = ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>", "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>"];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
        for off in offsets {
            out.extend(format!("{off:010} 00000 n \n").bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).bytes());
        out
    }
}
