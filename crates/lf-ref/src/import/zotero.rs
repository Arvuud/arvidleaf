//! Import straight from a Zotero data directory (`~/Zotero/zotero.sqlite`): items with
//! their creators, tags, child notes and attached files, plus the collection tree.
//!
//! The database is copied first and only the copy is read, through the `sqlite3` program
//! (like curl and poppler, no C library in the build). Zotero may keep running.
//! Items are converted to CSL-JSON with Zotero's own field mapping and then go through the
//! same conversion as CSL-JSON imports ([`super::csl`]). Only the personal library is read;
//! items in the trash are skipped.

use super::csl;
use crate::bib::{Entry, Value};
use crate::library::{AddError, Library};
use serde_json::{Map, Value as J, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One Zotero item, converted.
#[derive(Debug, Clone)]
pub struct ZoteroItem {
    /// Zotero's item key (`ABCD2345`).
    pub zotero_key: String,
    pub entry: Entry,
    /// Attached files that exist on disk.
    pub files: Vec<PathBuf>,
    /// Collection paths (`Thesis/Chapter 2`).
    pub collections: Vec<String>,
}

#[derive(Debug, Default)]
pub struct ZoteroData {
    pub items: Vec<ZoteroItem>,
    /// All collection paths, also empty ones.
    pub collections: Vec<String>,
    /// Attachments whose file is missing (not synced, or linked from elsewhere).
    pub missing_files: Vec<String>,
    /// Files and notes without a parent item (not imported).
    pub standalone: usize,
}

/// Result of adding Zotero items to the library.
#[derive(Debug, Default)]
pub struct ZoteroReport {
    pub added: Vec<String>,
    /// (title, key in the library)
    pub duplicates: Vec<(String, String)>,
    /// Files to copy into the library: (entry key, source).
    pub files: Vec<(String, PathBuf)>,
    pub collections: usize,
}

/// Zotero's default data directory (`~/Zotero` on all systems).
pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
    home.join("Zotero")
}

/// Zotero item type → CSL type.
fn csl_type(t: &str) -> &'static str {
    match t {
        "preprint" => "article",
        "journalArticle" => "article-journal",
        "magazineArticle" => "article-magazine",
        "newspaperArticle" => "article-newspaper",
        "book" => "book",
        "bookSection" => "chapter",
        "dataset" => "dataset",
        "dictionaryEntry" => "entry-dictionary",
        "encyclopediaArticle" => "entry-encyclopedia",
        "manuscript" => "manuscript",
        "conferencePaper" => "paper-conference",
        "patent" => "patent",
        "forumPost" => "post",
        "blogPost" => "post-weblog",
        "report" => "report",
        "computerProgram" => "software",
        "presentation" => "speech",
        "standard" => "standard",
        "thesis" => "thesis",
        "webpage" => "webpage",
        _ => "document",
    }
}

/// Type-specific Zotero fields → their base field (`bookTitle` → `publicationTitle`).
fn base_field(f: &str) -> &str {
    match f {
        "blogTitle" | "bookTitle" | "dictionaryTitle" | "encyclopediaTitle" | "forumTitle" | "proceedingsTitle" | "programTitle" | "sessionTitle" | "websiteTitle" => "publicationTitle",
        "company" | "distributor" | "institution" | "label" | "network" | "repository" | "studio" | "university" => "publisher",
        "archiveID" | "billNumber" | "docketNumber" | "documentNumber" | "episodeNumber" | "identifier" | "patentNumber" | "publicLawNumber" | "reportNumber" => "number",
        "genre" | "letterType" | "manuscriptType" | "mapType" | "postType" | "presentationType" | "reportType" | "thesisType" | "websiteType" => "type",
        "caseName" | "nameOfAct" | "subject" => "title",
        "codePages" | "firstPage" => "pages",
        "codeVolume" | "reporterVolume" => "volume",
        "dateDecided" | "dateEnacted" | "issueDate" => "date",
        "repositoryLocation" => "place",
        other => other,
    }
}

/// Zotero base field → CSL variable (Zotero's schema, `csl.fields.text`).
fn csl_field(f: &str) -> Option<&'static str> {
    Some(match f {
        "abstractNote" => "abstract",
        "citationKey" => "citation-key",
        "seriesNumber" => "collection-number",
        "seriesTitle" | "series" => "collection-title",
        "publicationTitle" => "container-title",
        "DOI" => "DOI",
        "edition" => "edition",
        "eventPlace" => "event-place",
        "meetingName" | "conferenceName" => "event-title",
        "type" => "genre",
        "ISBN" => "ISBN",
        "ISSN" => "ISSN",
        "issue" => "issue",
        "journalAbbreviation" => "journalAbbreviation",
        "language" => "language",
        "extra" => "note",
        "number" => "number",
        "numPages" => "number-of-pages",
        "pages" => "page",
        "publisher" => "publisher",
        "place" => "publisher-place",
        "shortTitle" => "title-short",
        "title" => "title",
        "url" => "URL",
        "versionNumber" => "version",
        "volume" => "volume",
        _ => return None,
    })
}

/// Zotero creator type → CSL name variable (only those BibLaTeX conversion uses).
fn csl_creator(t: &str) -> Option<&'static str> {
    match t {
        "author" | "creator" | "podcaster" => Some("author"),
        "editor" => Some("editor"),
        "translator" => Some("translator"),
        _ => None,
    }
}

/// Zotero's stored date (`2017-06-12 2017-06-12`, `2017-00-00 2017`,
/// `0000-00-00 Spring 2017`, `2024-01-05 10:22:11`) → CSL date.
pub fn date(stored: &str) -> Option<J> {
    let s = stored.trim();
    let iso = s.get(..10).filter(|d| d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-');
    if let Some(d) = iso {
        let parts: Vec<i64> = d.split('-').filter_map(|p| p.parse().ok()).collect();
        if let [y, m, day] = parts[..] {
            if y > 0 {
                let mut dp = vec![y];
                if m > 0 {
                    dp.push(m);
                    if day > 0 {
                        dp.push(day);
                    }
                }
                return Some(json!({ "date-parts": [dp] }));
            }
            let rest = s[10..].trim();
            return (!rest.is_empty()).then(|| json!({ "literal": rest }));
        }
    }
    (!s.is_empty()).then(|| json!({ "literal": s }))
}

/// `Citation Key: x` (Better BibTeX, older Zotero) from the Extra field; the rest of Extra.
fn split_extra(extra: &str) -> (Option<String>, String) {
    let mut key = None;
    let mut rest = Vec::new();
    for line in extra.lines() {
        match line.split_once(':') {
            Some((k, v)) if k.trim().eq_ignore_ascii_case("citation key") && !v.trim().is_empty() => key = Some(v.trim().to_string()),
            _ => rest.push(line),
        }
    }
    (key, rest.join("\n").trim().to_string())
}

/// A Zotero item as CSL-JSON. `fields`: (field name, value); `creators`: (type, first,
/// last, single-field) in order; `tags`: names.
pub fn item_to_csl(item_type: &str, fields: &[(String, String)], creators: &[(String, String, String, bool)], tags: &[String]) -> J {
    let mut m = Map::new();
    m.insert("type".into(), json!(csl_type(item_type)));
    for (f, v) in fields {
        // `arXiv:1706.03762` as archive ID: the arXiv DOI gives the eprint fields
        if f == "archiveID" && v.to_lowercase().starts_with("arxiv:") {
            continue;
        }
        let base = base_field(f);
        match base {
            "date" => {
                if let Some(d) = date(v) {
                    m.insert("issued".into(), d);
                }
            }
            "accessDate" => {
                if let Some(d) = date(v) {
                    m.insert("accessed".into(), d);
                }
            }
            "extra" => {
                let (key, rest) = split_extra(v);
                if let Some(k) = key {
                    m.entry("citation-key").or_insert(json!(k));
                }
                if !rest.is_empty() {
                    m.insert("note".into(), json!(rest));
                }
            }
            _ => {
                if let Some(var) = csl_field(base) {
                    m.insert(var.into(), json!(v));
                }
            }
        }
    }
    // the citation key wins over the Extra field
    if let Some((_, k)) = fields.iter().find(|(f, v)| f == "citationKey" && !v.trim().is_empty()) {
        m.insert("citation-key".into(), json!(k.trim()));
    }
    let mut names: HashMap<&str, Vec<J>> = HashMap::new();
    for (t, first, last, single) in creators {
        let Some(var) = csl_creator(t) else { continue };
        let n = if *single || first.is_empty() && last.contains(' ') { json!({ "literal": last }) } else { json!({ "family": last, "given": first }) };
        names.entry(var).or_default().push(n);
    }
    for (var, list) in names {
        m.insert(var.into(), J::Array(list));
    }
    if !tags.is_empty() {
        m.insert("keyword".into(), json!(tags.iter().map(|t| t.replace([',', ';'], " ")).collect::<Vec<_>>().join(", ")));
    }
    J::Object(m)
}

/// A Zotero note (HTML) as LaTeX text with its paragraphs kept.
fn note_text(html: &str) -> String {
    let mut s = html.to_string();
    for end in ["</p>", "</div>", "</li>", "</h1>", "</h2>", "</h3>", "</h4>", "<br>", "<br/>", "<br />"] {
        s = s.replace(end, &format!("{end}\u{1}"));
    }
    s.split('\u{1}').map(super::html_to_latex).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")
}

/// Runs a query on a database file and returns the rows.
fn query(db: &Path, sql: &str) -> Result<Vec<Map<String, J>>, String> {
    let mut c = Command::new("sqlite3");
    c.arg("-readonly").arg("-json").arg(db).arg(sql);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = c.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound { "sqlite3 is not installed – the Zotero import needs it".to_string() } else { e.to_string() }
    })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("-json") {
            return Err("sqlite3 is too old – the Zotero import needs version 3.33 or newer".into());
        }
        return Err(format!("sqlite3: {}", err.trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<J> = serde_json::from_str(&text).map_err(|e| format!("unexpected sqlite3 output: {e}"))?;
    Ok(rows.into_iter().filter_map(|r| if let J::Object(o) = r { Some(o) } else { None }).collect())
}

fn int(r: &Map<String, J>, k: &str) -> i64 {
    r.get(k).and_then(J::as_i64).unwrap_or(0)
}

fn string(r: &Map<String, J>, k: &str) -> String {
    match r.get(k) {
        Some(J::String(s)) => s.clone(),
        Some(J::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// Reads a Zotero data directory (with `zotero.sqlite` and `storage/`).
pub fn read(dir: &Path) -> Result<ZoteroData, String> {
    let src = dir.join("zotero.sqlite");
    if !src.is_file() {
        return Err(format!("{} not found – choose the Zotero data directory (Zotero: Settings › Advanced › Files and Folders)", src.display()));
    }
    static RUN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("arvidref-zotero-{}-{run}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let db = tmp.join("zotero.sqlite");
    let copy = || -> std::io::Result<()> {
        std::fs::copy(&src, &db)?;
        let wal = dir.join("zotero.sqlite-wal");
        if wal.is_file() {
            std::fs::copy(&wal, tmp.join("zotero.sqlite-wal"))?;
        }
        Ok(())
    };
    let result = copy().map_err(|e| format!("copying {}: {e}", src.display())).and_then(|()| read_db(&db, dir));
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

fn read_db(db: &Path, dir: &Path) -> Result<ZoteroData, String> {
    const USER: &str = "(SELECT libraryID FROM libraries WHERE type = 'user')";
    const LIVE: &str = "itemID NOT IN (SELECT itemID FROM deletedItems)";
    let items = query(
        db,
        &format!("SELECT i.itemID AS id, i.key AS key, t.typeName AS type, i.dateAdded AS added FROM items i JOIN itemTypes t ON t.itemTypeID = i.itemTypeID WHERE i.libraryID = {USER} AND i.{LIVE} AND t.typeName NOT IN ('note', 'attachment', 'annotation') ORDER BY i.itemID"),
    )?;
    let data = query(db, "SELECT d.itemID AS id, f.fieldName AS field, v.value AS value FROM itemData d JOIN fields f ON f.fieldID = d.fieldID JOIN itemDataValues v ON v.valueID = d.valueID")?;
    let creators = query(
        db,
        "SELECT ic.itemID AS id, c.firstName AS first, c.lastName AS last, c.fieldMode AS mode, ct.creatorType AS type FROM itemCreators ic JOIN creators c ON c.creatorID = ic.creatorID JOIN creatorTypes ct ON ct.creatorTypeID = ic.creatorTypeID ORDER BY ic.itemID, ic.orderIndex",
    )?;
    let tags = query(db, "SELECT it.itemID AS id, t.name AS name FROM itemTags it JOIN tags t ON t.tagID = it.tagID ORDER BY t.name")?;
    let notes = query(db, &format!("SELECT n.parentItemID AS id, n.note AS note FROM itemNotes n WHERE n.{LIVE}"))?;
    let files = query(db, &format!("SELECT a.parentItemID AS id, i.key AS key, a.linkMode AS mode, a.contentType AS type, a.path AS path FROM itemAttachments a JOIN items i ON i.itemID = a.itemID WHERE a.{LIVE} AND i.libraryID = {USER}"))?;
    let cols = query(db, &format!("SELECT collectionID AS id, collectionName AS name, parentCollectionID AS parent FROM collections WHERE libraryID = {USER} AND collectionID NOT IN (SELECT collectionID FROM deletedCollections)"))?;
    let members = query(db, "SELECT collectionID AS cid, itemID AS id FROM collectionItems")?;

    // collection paths
    let by_id: HashMap<i64, (String, i64)> = cols.iter().map(|c| (int(c, "id"), (string(c, "name").replace('/', "-"), int(c, "parent")))).collect();
    let path_of = |mut id: i64| -> Option<String> {
        let mut parts = Vec::new();
        for _ in 0..64 {
            let (name, parent) = by_id.get(&id)?;
            parts.push(name.trim().to_string());
            if *parent == 0 {
                parts.reverse();
                return Some(parts.join("/"));
            }
            id = *parent;
        }
        None
    };
    let mut collections: Vec<String> = by_id.keys().filter_map(|id| path_of(*id)).collect();
    collections.sort();

    let mut out = ZoteroData { collections, ..Default::default() };
    let group = |rows: &[Map<String, J>]| -> HashMap<i64, Vec<usize>> {
        let mut m: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, r) in rows.iter().enumerate() {
            m.entry(int(r, "id")).or_default().push(i);
        }
        m
    };
    let (data_by, creators_by, tags_by, notes_by, files_by) = (group(&data), group(&creators), group(&tags), group(&notes), group(&files));
    let item_ids: std::collections::HashSet<i64> = items.iter().map(|i| int(i, "id")).collect();
    out.standalone = notes.iter().chain(files.iter()).filter(|r| r.get("id").is_none_or(J::is_null) || !item_ids.contains(&int(r, "id"))).count();
    let mut cols_by: HashMap<i64, Vec<String>> = HashMap::new();
    for m in &members {
        if let Some(p) = path_of(int(m, "cid")) {
            cols_by.entry(int(m, "id")).or_default().push(p);
        }
    }

    for it in &items {
        let id = int(it, "id");
        let pick = |m: &HashMap<i64, Vec<usize>>| m.get(&id).cloned().unwrap_or_default();
        let fields: Vec<(String, String)> = pick(&data_by).into_iter().map(|i| (string(&data[i], "field"), string(&data[i], "value"))).collect();
        let cr: Vec<(String, String, String, bool)> = pick(&creators_by).into_iter().map(|i| (string(&creators[i], "type"), string(&creators[i], "first"), string(&creators[i], "last"), int(&creators[i], "mode") == 1)).collect();
        let tg: Vec<String> = pick(&tags_by).into_iter().map(|i| string(&tags[i], "name")).collect();
        let mut entry = csl::to_entry(&item_to_csl(&string(it, "type"), &fields, &cr, &tg));
        // child notes → annotation
        let ns: Vec<String> = pick(&notes_by).into_iter().map(|i| note_text(&string(&notes[i], "note"))).filter(|n| !n.is_empty()).collect();
        if !ns.is_empty() {
            entry.set("annotation", Value::text(ns.join("\n\n")));
        }
        // Zotero's "date added" (UTC, `2024-01-05 10:22:11`)
        let added = string(it, "added");
        if added.len() >= 19 {
            entry.set(crate::library::ADDED, Value::text(format!("{}T{}Z", &added[..10], &added[11..19])));
        }
        let mut paths = Vec::new();
        for i in pick(&files_by) {
            let f = &files[i];
            if string(f, "type") == "text/html" {
                continue; // web page snapshots
            }
            let path = string(f, "path");
            let p = match int(f, "mode") {
                0 | 1 => path.strip_prefix("storage:").map(|name| dir.join("storage").join(string(f, "key")).join(name)),
                2 => Some(PathBuf::from(&path)).filter(|p| p.is_absolute()),
                _ => None, // linked URLs
            };
            match p {
                Some(p) if p.is_file() => paths.push(p),
                Some(p) => out.missing_files.push(p.display().to_string()),
                None if !path.is_empty() => out.missing_files.push(path),
                None => {}
            }
        }
        let mut cs = cols_by.remove(&id).unwrap_or_default();
        cs.sort();
        cs.dedup();
        out.items.push(ZoteroItem { zotero_key: string(it, "key"), entry, files: paths, collections: cs });
    }
    Ok(out)
}

/// Adds the items to the library: duplicates are not added again but still get their
/// collections (and files, if the library entry has none). Returns the files to copy.
pub fn add_to_library(lib: &mut Library, data: &ZoteroData) -> ZoteroReport {
    let mut r = ZoteroReport::default();
    for c in &data.collections {
        if !lib.collections.collections.contains_key(c) {
            lib.collections.create(c);
            r.collections += 1;
        }
    }
    for it in &data.items {
        let key = match lib.add(it.entry.clone()) {
            Ok(k) => {
                r.added.push(k.clone());
                r.files.extend(it.files.iter().map(|f| (k.clone(), f.clone())));
                k
            }
            Err(AddError::Duplicate(k)) => {
                r.duplicates.push((it.entry.text_field("title", lib.strings()).unwrap_or_default(), k.clone()));
                if lib.files(&k).is_empty() {
                    r.files.extend(it.files.iter().map(|f| (k.clone(), f.clone())));
                }
                k
            }
        };
        for c in &it.collections {
            lib.collections.add(c, &key);
        }
    }
    r
}

/// Copies an attachment into the entry's folder (call on a worker thread); then
/// [`Library::attach`] with the returned path only links it.
pub fn copy_file(lib_dir: &Path, key: &str, src: &Path) -> Result<PathBuf, String> {
    let dir = crate::library::files_dir_in(lib_dir, key);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
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
    std::fs::copy(src, &t).map_err(|e| format!("copying {}: {e}", src.display()))?;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date("2017-06-12 2017-06-12"), Some(json!({"date-parts": [[2017, 6, 12]]})));
        assert_eq!(date("2017-00-00 2017"), Some(json!({"date-parts": [[2017]]})));
        assert_eq!(date("2019-03-00 March 2019"), Some(json!({"date-parts": [[2019, 3]]})));
        assert_eq!(date("0000-00-00 Spring 2017"), Some(json!({"literal": "Spring 2017"})));
        assert_eq!(date("2024-01-05 10:22:11"), Some(json!({"date-parts": [[2024, 1, 5]]})));
        assert_eq!(date("in press"), Some(json!({"literal": "in press"})));
        assert_eq!(date(""), None);
    }

    #[test]
    fn items_become_biblatex() {
        let f = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let csl = item_to_csl(
            "bookSection",
            &f(&[("title", "Ein Kapitel über Dinge"), ("bookTitle", "Das große Buch"), ("publisher", "Springer"), ("date", "2020-00-00 2020"), ("pages", "10-20"), ("extra", "Citation Key: mueller2020kapitel\nSome note")]),
            &[("author".into(), "Jürgen".into(), "Müller".into(), false), ("editor".into(), "".into(), "World Health Organization".into(), true), ("contributor".into(), "A".into(), "B".into(), false)],
            &["ml".into(), "a,b".into()],
        );
        let e = csl::to_entry(&csl);
        let s = |n: &str| e.raw_field(n, &Default::default()).unwrap_or_default();
        assert!(e.is("incollection"));
        assert_eq!(e.key, "mueller2020kapitel");
        assert_eq!(s("booktitle"), "Das große Buch");
        assert_eq!(s("author"), "Müller, Jürgen");
        assert_eq!(s("editor"), "{World Health Organization}");
        assert_eq!(s("date"), "2020");
        assert_eq!(s("pages"), "10--20");
        assert_eq!(s("note"), "Some note");
        assert_eq!(s("keywords"), "ml, a b");
        let thesis = csl::to_entry(&item_to_csl("thesis", &f(&[("title", "T"), ("thesisType", "PhD thesis"), ("university", "TU Berlin"), ("citationKey", "x2020"), ("extra", "Citation Key: ignored")]), &[], &[]));
        assert!(thesis.is("thesis"));
        assert_eq!(thesis.key, "x2020");
        assert_eq!(thesis.raw_field("institution", &Default::default()).unwrap(), "TU Berlin");
        assert_eq!(thesis.raw_field("type", &Default::default()).unwrap(), "PhD thesis");
    }
}
