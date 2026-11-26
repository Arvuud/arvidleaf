//! The central reference library.
//!
//! ```text
//! <dir>/
//! ├── library.bib          all entries (source of truth, plain BibLaTeX)
//! ├── collections.toml     collections, see [`crate::collections`]
//! ├── files/<key>/…        attachments
//! └── backups/             one copy of library.bib per day, the last 30 days
//! ```
//!
//! Changes are kept in memory until [`Library::save`]. Saving is atomic (temporary file +
//! rename) and refuses to overwrite a file that was changed by another program since it
//! was loaded ([`SaveError::Conflict`]); the caller then reloads or saves with `force`.

use crate::bib::{self, Bib, Entry, Strings, Value};
use crate::collections::Collections;
use crate::key;
use crate::latex::{fold_ascii, strip_accents};
use crate::names::parse_names;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const LIBRARY_FILE: &str = "library.bib";
pub const COLLECTIONS_FILE: &str = "collections.toml";
const BACKUPS_KEPT: usize = 30;

/// What a list row shows (plain text).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntryView {
    pub key: String,
    pub kind: String,
    /// `Knuth`, `Knuth & Lamport`, `Knuth et al.`
    pub authors: String,
    pub year: String,
    pub title: String,
    /// Journal, book title or publisher.
    pub container: String,
    pub doi: Option<String>,
    pub tags: Vec<String>,
    pub has_file: bool,
}

/// Normalized text for search and duplicate detection.
#[derive(Debug, Clone, Default)]
struct Indexed {
    view: EntryView,
    key: String,
    authors: String,
    title: String,
    container: String,
    other: String,
    doi: Option<String>,
    arxiv: Option<String>,
    title_year: Option<String>,
}

#[derive(Debug)]
pub enum SaveError {
    /// The file on disk changed since it was loaded or last saved.
    Conflict,
    Io(io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Conflict => write!(f, "the library was changed by another program"),
            SaveError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for SaveError {
    fn from(e: io::Error) -> Self {
        SaveError::Io(e)
    }
}

#[derive(Debug, PartialEq)]
pub enum AddError {
    /// The entry is already in the library under this key.
    Duplicate(String),
}

/// Modification time and size of a file, to notice changes by other programs.
type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let md = std::fs::metadata(path).ok()?;
    Some((md.modified().ok()?, md.len()))
}

pub struct Library {
    dir: PathBuf,
    bib: Bib,
    strings: Strings,
    index: Vec<Indexed>,
    pub collections: Collections,
    stamp: Stamp,
    dirty: bool,
    /// Key pattern for new entries, see [`crate::key`].
    pub key_pattern: String,
}

impl Library {
    /// Opens (or creates) the library in `dir`. A broken `collections.toml` is an error rather
    /// than being replaced by an empty one.
    pub fn open(dir: &Path) -> io::Result<Library> {
        std::fs::create_dir_all(dir)?;
        let mut lib = Library { dir: dir.to_path_buf(), bib: Bib::default(), strings: Strings::new(), index: Vec::new(), collections: Collections::default(), stamp: None, dirty: false, key_pattern: key::DEFAULT_PATTERN.into() };
        lib.reload()?;
        Ok(lib)
    }

    /// Reads library and collections from disk again; unsaved changes are lost.
    pub fn reload(&mut self) -> io::Result<()> {
        let path = self.path();
        let src = match std::fs::read(&path) {
            Ok(b) => String::from_utf8(b).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("{} is not UTF-8", path.display())))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let cpath = self.dir.join(COLLECTIONS_FILE);
        let collections = match std::fs::read_to_string(&cpath) {
            Ok(s) => Collections::parse(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", cpath.display())))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Collections::default(),
            Err(e) => return Err(e),
        };
        self.stamp = stamp(&path);
        self.bib = bib::parse(&src);
        self.collections = collections;
        self.rebuild();
        self.dirty = false;
        Ok(())
    }

    fn rebuild(&mut self) {
        self.strings = self.bib.strings();
        let strings = &self.strings;
        let entries: Vec<&Entry> = self.bib.entries().collect();
        // indexing is independent per entry: split large libraries across threads
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8);
        self.index = if entries.len() < 2000 || threads < 2 {
            entries.iter().map(|e| index_entry(e, strings)).collect()
        } else {
            let chunk = entries.len().div_ceil(threads);
            std::thread::scope(|sc| {
                let handles: Vec<_> = entries.chunks(chunk).map(|part| sc.spawn(move || part.iter().map(|e| index_entry(e, strings)).collect::<Vec<_>>())).collect();
                handles.into_iter().flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
            })
        };
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(LIBRARY_FILE)
    }

    /// Folder for the attachments of an entry.
    pub fn files_dir(&self, key: &str) -> PathBuf {
        self.dir.join("files").join(key)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether another program changed `library.bib` since it was loaded or saved.
    pub fn changed_on_disk(&self) -> bool {
        stamp(&self.path()) != self.stamp
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn strings(&self) -> &Strings {
        &self.strings
    }

    /// Malformed entries in `library.bib`: (line, message).
    pub fn problems(&self) -> Vec<(usize, String)> {
        self.bib.problems()
    }

    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.bib.entry(key)
    }

    /// The entry as BibLaTeX source: as written in the file if unchanged, else formatted.
    pub fn source(&self, key: &str) -> Option<String> {
        let b = &self.bib.blocks[self.bib.find(key)?];
        match (&b.raw, &b.kind) {
            (Some(raw), _) => Some(raw.trim().to_string()),
            (None, bib::Kind::Entry(e)) => Some(bib::format_entry(e).trim_end().to_string()),
            _ => None,
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        self.index.iter().any(|i| i.view.key.eq_ignore_ascii_case(key))
    }

    /// All entries for list display, in file order.
    pub fn views(&self) -> impl Iterator<Item = &EntryView> {
        self.index.iter().map(|i| &i.view)
    }

    pub fn view(&self, key: &str) -> Option<&EntryView> {
        self.index.iter().find(|i| i.view.key.eq_ignore_ascii_case(key)).map(|i| &i.view)
    }

    /// Writes `library.bib` and `collections.toml`. Without `force`, a file changed by another
    /// program is not overwritten. Keeps one backup of the previous file per day.
    pub fn save(&mut self, force: bool) -> Result<(), SaveError> {
        let path = self.path();
        if !force && self.changed_on_disk() {
            return Err(SaveError::Conflict);
        }
        if path.exists() {
            self.backup()?;
        }
        write_atomic(&path, self.bib.to_bib_string().as_bytes())?;
        self.stamp = stamp(&path);
        let cpath = self.dir.join(COLLECTIONS_FILE);
        if !self.collections.collections.is_empty() || cpath.exists() {
            write_atomic(&cpath, self.collections.to_toml().as_bytes())?;
        }
        self.dirty = false;
        Ok(())
    }

    fn backup(&self) -> io::Result<()> {
        let dir = self.dir.join("backups");
        std::fs::create_dir_all(&dir)?;
        let (y, m, d) = civil(unix_secs() / 86_400);
        let target = dir.join(format!("library-{y:04}-{m:02}-{d:02}.bib"));
        if !target.exists() {
            std::fs::copy(self.path(), &target)?;
        }
        let mut all: Vec<PathBuf> = std::fs::read_dir(&dir)?.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("library-"))).collect();
        all.sort();
        if all.len() > BACKUPS_KEPT {
            for p in &all[..all.len() - BACKUPS_KEPT] {
                let _ = std::fs::remove_file(p);
            }
        }
        Ok(())
    }

    /// Key of an entry in the library that describes the same work: same DOI, same arXiv
    /// ID, or same title and year.
    pub fn find_duplicate(&self, e: &Entry) -> Option<String> {
        let probe = index_entry(e, &self.strings);
        let same = |i: &&Indexed| {
            (probe.doi.is_some() && i.doi == probe.doi) || (probe.arxiv.is_some() && i.arxiv == probe.arxiv) || (probe.title_year.is_some() && i.title_year == probe.title_year)
        };
        self.index.iter().find(same).map(|i| i.view.key.clone())
    }

    /// A free key for the entry: its own key if set and free, else one from the key pattern.
    pub fn free_key(&self, e: &Entry) -> String {
        let base = if e.key.trim().is_empty() { key::base_key(e, &self.strings, &self.key_pattern) } else { key::sanitize(&e.key) };
        let base = if base.is_empty() { "ref".to_string() } else { base };
        key::unique(&base, |k| self.contains(k))
    }

    /// Adds an entry. The key is kept if it is free; otherwise (or if empty) a key is
    /// generated. Returns the key used.
    pub fn add(&mut self, mut e: Entry) -> Result<String, AddError> {
        if let Some(k) = self.find_duplicate(&e) {
            return Err(AddError::Duplicate(k));
        }
        Ok(self.add_anyway(&mut e))
    }

    /// Adds an entry even if it looks like a duplicate.
    pub fn add_anyway(&mut self, e: &mut Entry) -> String {
        e.key = self.free_key(e);
        let now = timestamp();
        if e.get(ADDED).is_none() {
            e.set(ADDED, Value::text(now.clone()));
        }
        e.set(MODIFIED, Value::text(now));
        self.index.push(index_entry(e, &self.strings));
        self.bib.push_entry(e.clone());
        self.dirty = true;
        e.key.clone()
    }

    /// Replaces the entry `key` with `e`. If the key changes, collections and the attachment
    /// folder follow. Fails if the new key belongs to another entry.
    pub fn update(&mut self, key: &str, mut e: Entry) -> Result<String, String> {
        let Some(pos) = self.bib.find(key) else { return Err(format!("no entry \"{key}\"")) };
        e.key = key::sanitize(&e.key);
        if e.key.is_empty() {
            return Err("the key must not be empty".into());
        }
        let renamed = !e.key.eq_ignore_ascii_case(key);
        if renamed && self.contains(&e.key) {
            return Err(format!("the key \"{}\" is already used", e.key));
        }
        e.set(MODIFIED, Value::text(timestamp()));
        if renamed || e.key != key {
            self.collections.rename_key(key, &e.key);
            let (old, new) = (self.files_dir(key), self.files_dir(&e.key));
            if old.is_dir() && !new.exists() {
                std::fs::rename(&old, &new).map_err(|err| format!("moving attachments: {err}"))?;
                let new_key = e.key.clone();
                rewrite_file_paths(&mut e, key, &new_key, &old, &new);
            }
        }
        let new_key = e.key.clone();
        let idx = index_entry(&e, &self.strings);
        if let Some(i) = self.index.iter().position(|i| i.view.key.eq_ignore_ascii_case(key)) {
            self.index[i] = idx;
        }
        self.bib.replace_entry(pos, e);
        self.dirty = true;
        Ok(new_key)
    }

    /// Removes an entry (also from all collections). Attachments stay on disk.
    pub fn remove(&mut self, key: &str) -> Option<Entry> {
        let e = self.bib.remove_entry(key)?;
        self.index.retain(|i| !i.view.key.eq_ignore_ascii_case(key));
        self.collections.forget_key(key);
        self.dirty = true;
        Some(e)
    }

    /// All tags with the number of entries, sorted by name.
    pub fn tags(&self) -> Vec<(String, usize)> {
        let mut map: std::collections::BTreeMap<String, (String, usize)> = std::collections::BTreeMap::new();
        for i in &self.index {
            for t in &i.view.tags {
                let slot = map.entry(t.to_lowercase()).or_insert_with(|| (t.clone(), 0));
                slot.1 += 1;
            }
        }
        map.into_values().collect()
    }

    /// Keys of the entries matching `query`, best first. Every word of the query must occur
    /// (in key, authors, title, year, journal, tags, notes or DOI); accents and case are
    /// ignored and `mueller`/`muller` both find `Müller`. An empty query returns all keys in
    /// file order. `tag:x` restricts to a tag, `type:article` to an entry type.
    pub fn search(&self, query: &str) -> Vec<&str> {
        let mut terms = Vec::new();
        let mut tag = None;
        let mut kind = None;
        for w in query.split_whitespace() {
            if let Some(t) = w.strip_prefix("tag:") {
                tag = Some(t.to_lowercase());
            } else if let Some(t) = w.strip_prefix("type:") {
                kind = Some(t.to_lowercase());
            } else {
                terms.push(strip_accents(w).to_lowercase());
            }
        }
        let mut hits: Vec<(u32, &Indexed)> = Vec::new();
        'entries: for i in &self.index {
            if tag.as_ref().is_some_and(|t| !i.view.tags.iter().any(|x| x.to_lowercase() == *t)) || kind.as_ref().is_some_and(|k| !i.view.kind.eq_ignore_ascii_case(k)) {
                continue;
            }
            let mut score = 0;
            for t in &terms {
                let fields: [(&str, u32); 6] = [(&i.key, 6), (&i.authors, 5), (&i.view.year, 4), (&i.title, 3), (&i.container, 2), (&i.other, 1)];
                let best = fields.iter().filter_map(|(f, w)| match_score(f, t).map(|m| m * w)).max();
                match best {
                    Some(s) => score += s,
                    None => continue 'entries,
                }
            }
            hits.push((score, i));
        }
        if !terms.is_empty() {
            hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.view.year.cmp(&a.1.view.year)));
        }
        hits.into_iter().map(|(_, i)| i.view.key.as_str()).collect()
    }
}

/// 3 = the field starts with the term, 2 = a word starts with it, 1 = it occurs inside.
fn match_score(field: &str, term: &str) -> Option<u32> {
    let pos = field.find(term)?;
    if pos == 0 {
        return Some(3);
    }
    let word_start = field.match_indices(term).any(|(p, _)| p == 0 || !field.as_bytes()[p - 1].is_ascii_alphanumeric());
    Some(if word_start { 2 } else { 1 })
}

/// The list row of an entry (for entries outside the library, e.g. in a project `.bib`).
pub fn view_of(e: &Entry, strings: &Strings) -> EntryView {
    index_entry(e, strings).view
}

impl EntryView {
    /// `Knuth (1984): The TeXbook` – for tooltips and completion.
    pub fn describe(&self) -> String {
        let who = if self.authors.is_empty() { "Anonymous" } else { &self.authors };
        let year = if self.year.is_empty() { String::new() } else { format!(" ({})", self.year) };
        format!("{who}{year}: {}", self.title)
    }
}

pub const ADDED: &str = "x-arvidref-added";
pub const MODIFIED: &str = "x-arvidref-modified";

/// Tags of an entry (the `keywords` field, comma-separated).
pub fn tags(e: &Entry, strings: &Strings) -> Vec<String> {
    e.text_field("keywords", strings).map(|k| k.split([',', ';']).map(str::trim).filter(|t| !t.is_empty()).map(String::from).collect()).unwrap_or_default()
}

/// Sets the tags of an entry (removes `keywords` if empty).
pub fn set_tags(e: &mut Entry, tags: &[String]) {
    let tags: Vec<String> = tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    if tags.is_empty() {
        e.remove("keywords");
    } else {
        e.set("keywords", Value::text(tags.iter().map(|t| crate::latex::encode(t)).collect::<Vec<_>>().join(", ")));
    }
}

/// `10.1000/ABC`, `https://doi.org/10.1000/abc`, `doi:10.1000/abc` → `10.1000/abc`.
pub fn normalize_doi(s: &str) -> Option<String> {
    let l = s.trim().to_lowercase();
    let start = l.find("10.")?;
    let d = l[start..].trim_end_matches(['.', ',', ';']).to_string();
    d.contains('/').then_some(d)
}

/// `2101.00001v2`, `arXiv:2101.00001`, `https://arxiv.org/abs/2101.00001`, `hep-th/9901001`
/// → ID without version.
pub fn normalize_arxiv(s: &str) -> Option<String> {
    let l = s.trim().to_lowercase();
    let l = l.rsplit_once("arxiv.org/abs/").or_else(|| l.rsplit_once("arxiv.org/pdf/")).map(|(_, r)| r.to_string()).unwrap_or(l);
    let l = l.strip_prefix("arxiv:").unwrap_or(&l).trim_end_matches(".pdf");
    let id = match l.rfind('v') {
        Some(p) if p > 0 && l[p + 1..].chars().all(|c| c.is_ascii_digit()) && !l[p + 1..].is_empty() && l.as_bytes()[p - 1].is_ascii_digit() => &l[..p],
        _ => l,
    };
    let new_style = id.len() >= 9 && id.as_bytes()[4] == b'.' && id.chars().enumerate().all(|(i, c)| if i == 4 { c == '.' } else { c.is_ascii_digit() });
    let old_style = id.split_once('/').is_some_and(|(a, b)| !a.is_empty() && a.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '.') && b.len() == 7 && b.chars().all(|c| c.is_ascii_digit()));
    (new_style || old_style).then(|| id.to_string())
}

fn arxiv_of(e: &Entry, s: &Strings) -> Option<String> {
    let typed = ["eprinttype", "archiveprefix"].iter().any(|f| e.raw_field(f, s).is_some_and(|v| v.eq_ignore_ascii_case("arxiv")));
    if let Some(ep) = e.raw_field("eprint", s)
        && let Some(id) = normalize_arxiv(&ep)
        && (typed || ep.to_lowercase().starts_with("arxiv:") || id.contains('.'))
    {
        return Some(id);
    }
    if let Some(d) = e.raw_field("doi", s).and_then(|d| normalize_doi(&d))
        && let Some(id) = d.strip_prefix("10.48550/arxiv.")
    {
        return normalize_arxiv(id);
    }
    e.raw_field("url", s).filter(|u| u.contains("arxiv.org/")).and_then(|u| normalize_arxiv(&u))
}

/// Lower case, accents stripped, plus the German spelling (`ü` → `ue`) if it differs.
fn norm(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    let a = strip_accents(s).to_lowercase();
    let b = fold_ascii(s).to_lowercase();
    if a == b { a } else { format!("{a} {b}") }
}

fn index_entry(e: &Entry, s: &Strings) -> Indexed {
    let creators = key::creators(e, s);
    let fam = key::families_of(&creators);
    let authors = match fam.len() {
        0 => String::new(),
        1 => fam[0].clone(),
        2 if fam[1] != "others" => format!("{} & {}", fam[0], fam[1]),
        _ => format!("{} et al.", fam[0]),
    };
    let year = key::year(e, s).unwrap_or_default();
    let title = e.text_field("title", s).unwrap_or_default();
    let container = ["journaltitle", "journal", "booktitle", "maintitle", "publisher", "institution", "school", "organization", "howpublished"].iter().find_map(|f| e.text_field(f, s)).unwrap_or_default();
    let doi = e.raw_field("doi", s).and_then(|d| normalize_doi(&d));
    let tags = tags(e, s);
    let has_file = e.raw_field("file", s).is_some_and(|f| !f.trim().is_empty());
    // authors and editors are both searchable; the editor list is only parsed if it is not
    // already the creator list
    let mut all_names: Vec<String> = creators.iter().map(|n| n.display()).collect();
    if e.get("author").is_some()
        && let Some(ed) = e.raw_field("editor", s)
    {
        all_names.extend(parse_names(&ed).iter().map(|n| n.display()));
    }
    let all_names = all_names.join(" ");
    let other = [tags.join(" "), e.text_field("annotation", s).unwrap_or_default(), e.text_field("note", s).unwrap_or_default(), doi.clone().unwrap_or_default(), e.text_field("subtitle", s).unwrap_or_default()].join(" ");
    let title_key: String = fold_ascii(&title).chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    Indexed {
        key: norm(&e.key),
        authors: norm(&all_names),
        title: norm(&title),
        container: norm(&container),
        other: norm(&other),
        arxiv: arxiv_of(e, s),
        title_year: (title_key.len() >= 8).then(|| format!("{title_key}|{year}")),
        doi: doi.clone(),
        view: EntryView { key: e.key.clone(), kind: e.kind.to_lowercase(), authors, year, title, container, doi, tags, has_file },
    }
}

/// After moving the attachment folder: fix paths in the `file` field (absolute paths and
/// paths relative to the library, `files/<key>/…`).
fn rewrite_file_paths(e: &mut Entry, old_key: &str, new_key: &str, old: &Path, new: &Path) {
    let items: Vec<String> = attachments(e)
        .into_iter()
        .map(|f| {
            let rel_old = format!("files/{old_key}/");
            if let Some(rest) = f.strip_prefix(&rel_old) {
                format!("files/{new_key}/{rest}")
            } else if let Ok(rest) = Path::new(&f).strip_prefix(old) {
                new.join(rest).to_string_lossy().to_string()
            } else {
                f
            }
        })
        .collect();
    set_attachments(e, &items);
}

/// Paths in the `file` field. Understands plain lists (`a.pdf;b.pdf`) and the JabRef/Zotero
/// form `description:path:type` (with `\:` for colons in the path).
pub fn attachments(e: &Entry) -> Vec<String> {
    let Some(v) = e.get("file") else { return Vec::new() };
    let raw = v.resolve(&Strings::new());
    split_unescaped(&raw, ';')
        .into_iter()
        .map(|item| {
            let parts = split_unescaped(&item, ':');
            // `C:\x.pdf` has one colon and is a plain path
            let path = if parts.len() == 3 { parts[1].clone() } else { item.clone() };
            path.replace("\\:", ":").replace("\\;", ";").replace("\\\\", "\\").trim().to_string()
        })
        .filter(|p| !p.is_empty())
        .collect()
}

fn split_unescaped(s: &str, sep: char) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut esc = false;
    for c in s.chars() {
        if c == sep && !esc {
            out.push(String::new());
        } else {
            out.last_mut().unwrap().push(c);
        }
        esc = c == '\\' && !esc;
    }
    out
}

/// Writes the `file` field as a plain `;`-separated list (removes it if empty). `:` and `;`
/// in paths are escaped as JabRef expects.
pub fn set_attachments(e: &mut Entry, items: &[String]) {
    if items.is_empty() {
        e.remove("file");
    } else {
        e.set("file", Value::text(items.iter().map(|p| p.replace('\\', "\\\\").replace(':', "\\:").replace(';', "\\;")).collect::<Vec<_>>().join(";")));
    }
}

impl Library {
    /// Attachments of an entry as absolute paths (relative ones are relative to the library).
    pub fn files(&self, key: &str) -> Vec<PathBuf> {
        let Some(e) = self.entry(key) else { return Vec::new() };
        attachments(e).iter().map(|p| self.dir.join(p)).collect()
    }

    /// Copies `src` into the entry's attachment folder and adds it to the `file` field.
    /// A file already in that folder is only added. Returns the stored path.
    pub fn attach(&mut self, key: &str, src: &Path) -> Result<PathBuf, String> {
        let mut e = self.entry(key).cloned().ok_or_else(|| format!("no entry \"{key}\""))?;
        let dir = self.files_dir(&e.key);
        let target = if src.parent().is_some_and(|p| p == dir) {
            src.to_path_buf()
        } else {
            std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
            let name = src.file_name().ok_or("not a file")?.to_string_lossy().to_string();
            let (stem, ext) = match name.rsplit_once('.') {
                Some((s, x)) if !s.is_empty() => (s.to_string(), format!(".{x}")),
                _ => (name.clone(), String::new()),
            };
            let mut t = dir.join(&name);
            let mut n = 2;
            while t.exists() {
                t = dir.join(format!("{stem}-{n}{ext}"));
                n += 1;
            }
            std::fs::copy(src, &t).map_err(|err| format!("copying {}: {err}", src.display()))?;
            t
        };
        let rel = target.strip_prefix(&self.dir).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| target.to_string_lossy().to_string());
        let mut items = attachments(&e);
        if !items.contains(&rel) {
            items.push(rel);
        }
        set_attachments(&mut e, &items);
        self.update(key, e)?;
        Ok(target)
    }

    /// Removes an attachment from the entry; with `delete`, a file inside the library's
    /// attachment folder is deleted too (files elsewhere are never deleted).
    pub fn detach(&mut self, key: &str, path: &Path, delete: bool) -> Result<(), String> {
        let mut e = self.entry(key).cloned().ok_or_else(|| format!("no entry \"{key}\""))?;
        let items: Vec<String> = attachments(&e).into_iter().filter(|p| self.dir.join(p) != path).collect();
        set_attachments(&mut e, &items);
        self.update(key, e)?;
        if delete && path.starts_with(self.dir.join("files")) {
            std::fs::remove_file(path).map_err(|err| format!("{}: {err}", path.display()))?;
            if let Some(d) = path.parent() {
                let _ = std::fs::remove_dir(d); // only if empty
            }
        }
        Ok(())
    }
}

fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}-{n}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        if let Ok(md) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(&tmp, md.permissions());
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn unix_secs() -> i64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// `2026-11-25T10:42:07Z`
pub fn timestamp() -> String {
    let s = unix_secs();
    let (y, m, d) = civil(s.div_euclid(86_400));
    let t = s.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
}

/// Days since 1970-01-01 → (year, month, day) (proleptic Gregorian, H. Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_781), (2026, 11, 24));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(-1), (1969, 12, 31));
        assert_eq!(timestamp().len(), 20);
    }

    #[test]
    fn identifiers() {
        assert_eq!(normalize_doi("https://doi.org/10.1002/ABC.123."), Some("10.1002/abc.123".into()));
        assert_eq!(normalize_doi("doi:10.1000/x"), Some("10.1000/x".into()));
        assert_eq!(normalize_doi("no doi"), None);
        assert_eq!(normalize_arxiv("arXiv:1706.03762v5"), Some("1706.03762".into()));
        assert_eq!(normalize_arxiv("https://arxiv.org/abs/2101.00001v2"), Some("2101.00001".into()));
        assert_eq!(normalize_arxiv("https://arxiv.org/pdf/2101.00001.pdf"), Some("2101.00001".into()));
        assert_eq!(normalize_arxiv("hep-th/9901001v1"), Some("hep-th/9901001".into()));
        assert_eq!(normalize_arxiv("12345"), None);
        assert_eq!(normalize_arxiv("cs"), None);
    }

    #[test]
    fn scores() {
        assert_eq!(match_score("knuth donald", "knuth"), Some(3));
        assert_eq!(match_score("donald knuth", "knuth"), Some(2));
        assert_eq!(match_score("donaldknuth", "knuth"), Some(1));
        assert_eq!(match_score("lamport", "knuth"), None);
    }
}
