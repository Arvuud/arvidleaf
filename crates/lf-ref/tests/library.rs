//! Library on disk: add, duplicates, save/reload, conflicts, backups, renames, search.

use lf_ref::bib::{Entry, Value};
use lf_ref::library::{AddError, SaveError, set_tags, tags};
use lf_ref::Library;
use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("lf-ref-test-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn entry(kind: &str, fields: &[(&str, &str)]) -> Entry {
    let mut e = Entry::new(kind, "");
    for (k, v) in fields {
        e.set(k, Value::text(*v));
    }
    e
}

fn knuth() -> Entry {
    entry("book", &[("author", "Knuth, Donald E."), ("title", "The {\\TeX book}"), ("date", "1984"), ("publisher", "Addison-Wesley")])
}

fn mueller() -> Entry {
    entry("article", &[("author", r#"M{\"u}ller, J{\"u}rgen and Smith, Anna"#), ("title", "Neuronale Netze für Einsteiger"), ("journaltitle", "Zeitschrift für Informatik"), ("date", "2021-03"), ("doi", "10.1000/ABC.1")])
}

#[test]
fn add_save_reload() {
    let dir = tempdir("basic");
    let mut lib = Library::open(&dir).unwrap();
    assert!(lib.is_empty());
    assert_eq!(lib.add(knuth()).unwrap(), "knuth1984texbook");
    assert_eq!(lib.add(mueller()).unwrap(), "mueller2021neuronale");
    assert!(lib.is_dirty());
    lib.save(false).unwrap();
    assert!(!lib.is_dirty());
    let text = std::fs::read_to_string(dir.join("library.bib")).unwrap();
    assert!(text.starts_with("@book{knuth1984texbook,\n  author = {Knuth, Donald E.},"), "{text}");
    assert!(text.contains("x-arvidref-added = {"));

    let lib2 = Library::open(&dir).unwrap();
    assert_eq!(lib2.len(), 2);
    let v = lib2.view("mueller2021neuronale").unwrap();
    assert_eq!(v.authors, "Müller & Smith");
    assert_eq!(v.year, "2021");
    assert_eq!(v.title, "Neuronale Netze für Einsteiger");
    assert_eq!(v.container, "Zeitschrift für Informatik");
    assert_eq!(v.doi.as_deref(), Some("10.1000/abc.1"));
    assert_eq!(lib2.view("knuth1984texbook").unwrap().title, "The TeXbook");
}

#[test]
fn keys_and_duplicates() {
    let dir = tempdir("dup");
    let mut lib = Library::open(&dir).unwrap();
    lib.add(mueller()).unwrap();
    // same DOI in another spelling
    let mut same_doi = entry("article", &[("title", "Something else entirely"), ("doi", "https://doi.org/10.1000/abc.1")]);
    assert_eq!(lib.add(same_doi.clone()), Err(AddError::Duplicate("mueller2021neuronale".into())));
    // same title and year
    let same_title = entry("misc", &[("author", "Other, A."), ("title", "Neuronale  Netze fuer Einsteiger"), ("year", "2021")]);
    assert!(matches!(lib.add(same_title), Err(AddError::Duplicate(_))));
    // arXiv: eprint vs. DOI vs. URL
    lib.add(entry("online", &[("title", "Attention Is All You Need"), ("eprint", "1706.03762v5"), ("eprinttype", "arxiv")])).unwrap();
    assert!(matches!(lib.add(entry("article", &[("title", "Attention (journal version)"), ("doi", "10.48550/arXiv.1706.03762")])), Err(AddError::Duplicate(_))));
    assert!(matches!(lib.add(entry("misc", &[("title", "x"), ("url", "https://arxiv.org/abs/1706.03762")])), Err(AddError::Duplicate(_))));
    // forced duplicate gets a suffixed key
    same_doi.set("author", Value::text("Müller, J."));
    same_doi.set("title", Value::text("Neuronale Ideen"));
    same_doi.set("year", Value::text("2021"));
    assert_eq!(lib.add_anyway(&mut same_doi), "mueller2021neuronalea");
    // a given key is kept if free and sanitized
    let mut k = knuth();
    k.key = "Knuth TeX{book}".into();
    assert_eq!(lib.add(k).unwrap(), "KnuthTeXbook");
    assert!(lib.contains("knuthtexbook"));
}

#[test]
fn conflict_with_other_program() {
    let dir = tempdir("conflict");
    let mut lib = Library::open(&dir).unwrap();
    lib.add(knuth()).unwrap();
    lib.save(false).unwrap();
    // another program appends an entry
    std::thread::sleep(std::time::Duration::from_millis(20));
    let path = dir.join("library.bib");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\n@misc{external, title = {Added in a text editor}}\n");
    std::fs::write(&path, &text).unwrap();
    assert!(lib.changed_on_disk());
    lib.add(mueller()).unwrap();
    assert!(matches!(lib.save(false), Err(SaveError::Conflict)));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "nothing overwritten");
    lib.reload().unwrap();
    assert!(lib.contains("external") && !lib.contains("mueller2021neuronale"));
    lib.add(mueller()).unwrap();
    lib.save(false).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.starts_with(&text), "external text kept byte for byte");
    // one backup per day, holding the version before the first save of the day
    let backups: Vec<_> = std::fs::read_dir(dir.join("backups")).unwrap().flatten().collect();
    assert_eq!(backups.len(), 1);
}

#[test]
fn rename_update_remove() {
    let dir = tempdir("rename");
    let mut lib = Library::open(&dir).unwrap();
    let key = lib.add(knuth()).unwrap();
    lib.add(mueller()).unwrap();
    lib.collections.add("Thesis/Chapter 1", &key);
    let files = lib.files_dir(&key);
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(files.join("book.pdf"), b"%PDF").unwrap();
    let mut e = lib.entry(&key).unwrap().clone();
    e.set("file", Value::text(format!(":{}:application/pdf", files.join("book.pdf").display())));
    lib.update(&key, e.clone()).unwrap();

    // key taken → error, nothing changed
    let mut clash = e.clone();
    clash.key = "mueller2021neuronale".into();
    assert!(lib.update(&key, clash).is_err());

    let mut renamed = lib.entry(&key).unwrap().clone();
    renamed.key = "knuth:texbook".into();
    set_tags(&mut renamed, &["typesetting".into(), " classic ".into()]);
    assert_eq!(lib.update(&key, renamed).unwrap(), "knuth:texbook");
    assert!(!lib.contains(&key));
    assert_eq!(lib.collections.keys("Thesis", true), ["knuth:texbook"]);
    let new_files = lib.files_dir("knuth:texbook");
    assert!(new_files.join("book.pdf").is_file() && !files.exists());
    let e = lib.entry("knuth:texbook").unwrap();
    assert!(lib::field(e, "file").contains(&new_files.display().to_string()));
    assert_eq!(tags(e, lib.strings()), ["typesetting", "classic"]);
    assert_eq!(lib.tags(), [("classic".to_string(), 1), ("typesetting".to_string(), 1)]);

    assert!(lib.remove("knuth:texbook").is_some());
    assert!(lib.collections.keys("Thesis", true).is_empty());
    assert_eq!(lib.len(), 1);
    lib.save(false).unwrap();
    let text = std::fs::read_to_string(dir.join("library.bib")).unwrap();
    assert!(!text.contains("knuth") && text.contains("mueller2021neuronale"));
    assert!(text.starts_with("@article{mueller2021neuronale"), "no blank line left behind: {text:?}");
    let col = std::fs::read_to_string(dir.join("collections.toml")).unwrap();
    assert!(col.contains("Thesis/Chapter 1"));
}

mod lib {
    pub fn field(e: &lf_ref::Entry, name: &str) -> String {
        e.raw_field(name, &Default::default()).unwrap_or_default()
    }
}

#[test]
fn search() {
    let dir = tempdir("search");
    let mut lib = Library::open(&dir).unwrap();
    lib.add(knuth()).unwrap();
    lib.add(mueller()).unwrap();
    let mut t = entry("article", &[("author", "Smithson, Bob"), ("title", "On Knuth's algorithms"), ("year", "1999"), ("keywords", "algorithms, classic")]);
    lib.add_anyway(&mut t);
    assert_eq!(lib.search("").len(), 3);
    assert_eq!(lib.search("muller"), ["mueller2021neuronale"]);
    assert_eq!(lib.search("Mueller"), ["mueller2021neuronale"]);
    assert_eq!(lib.search("MÜLLER 2021"), ["mueller2021neuronale"]);
    assert_eq!(lib.search("smith"), ["smithson1999knuths", "mueller2021neuronale"]);
    assert_eq!(lib.search("knuth"), ["knuth1984texbook", "smithson1999knuths"], "author/key before title");
    assert_eq!(lib.search("texbook"), ["knuth1984texbook"]);
    assert_eq!(lib.search("informatik"), ["mueller2021neuronale"]);
    assert_eq!(lib.search("tag:classic"), ["smithson1999knuths"]);
    assert_eq!(lib.search("type:book"), ["knuth1984texbook"]);
    assert_eq!(lib.search("10.1000/abc"), ["mueller2021neuronale"]);
    assert!(lib.search("knuth 2021").is_empty());
}

#[test]
fn broken_collections_file_is_not_replaced() {
    let dir = tempdir("broken");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("collections.toml"), "[collections\n").unwrap();
    assert!(Library::open(&dir).is_err());
}

/// 10 000 entries: open (parse + index) and search must stay interactive.
#[test]
fn ten_thousand_entries() {
    let dir = tempdir("big");
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = String::new();
    for i in 0..10_000 {
        s.push_str(&format!(
            "@article{{key{i},\n  author = {{M{{\\\"u}}ller{i}, J{{\\\"u}}rgen and Smith, Anna and Doe, John}},\n  title = {{A study of topic number {i} in {{LaTeX}}}},\n  journaltitle = {{Journal {}}},\n  date = {{{}}},\n  doi = {{10.1000/{i}}},\n  keywords = {{tag{}, common}},\n}}\n\n",
            i % 50,
            1950 + i % 75,
            i % 20
        ));
    }
    std::fs::write(dir.join("library.bib"), &s).unwrap();
    let t = std::time::Instant::now();
    let lib = Library::open(&dir).unwrap();
    let open = t.elapsed();
    assert_eq!(lib.len(), 10_000);
    let t = std::time::Instant::now();
    let hits = lib.search("muller4242 latex");
    let search = t.elapsed();
    assert_eq!(hits[0], "key4242");
    let t = std::time::Instant::now();
    let many = lib.search("smith");
    let search_all = t.elapsed();
    assert_eq!(many.len(), 10_000);
    eprintln!("open {open:?}, search {search:?}, broad search {search_all:?}");
    let limit = if cfg!(debug_assertions) { 10 } else { 1 };
    assert!(open.as_millis() < 100 * limit, "open {open:?}");
    assert!(search_all.as_millis() < 50 * limit, "search {search_all:?}");
}

/// A real file as library: unchanged save is byte-identical; search and duplicates work.
#[test]
fn biblatex_examples_as_library() {
    let Ok(src) = std::fs::read_to_string("/usr/share/texmf-dist/bibtex/bib/biblatex/biblatex/biblatex-examples.bib") else { return };
    let dir = tempdir("examples");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("library.bib"), &src).unwrap();
    let mut lib = Library::open(&dir).unwrap();
    assert_eq!(lib.len(), 92);
    lib.save(false).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("library.bib")).unwrap(), src);
    let knuth = lib.search("knuth");
    assert!(knuth.len() >= 5 && knuth.iter().all(|k| k.starts_with("knuth")), "{knuth:?}");
    assert_eq!(lib.search("ozge"), ["aksin"]);
    assert_eq!(lib.view("sigfridsson").unwrap().authors, "Sigfridsson & Ryde");
    assert_eq!(lib.view("sigfridsson").unwrap().container, "Journal of Computational Chemistry");
    // every entry is found as its own duplicate (crossref children have no title+year of their own)
    let e = lib.entry("sigfridsson").unwrap().clone();
    assert_eq!(lib.find_duplicate(&e).as_deref(), Some("sigfridsson"));
    let mut k = lib.entry("knuth:ct:a").unwrap().clone();
    k.set("note", Value::text("edited"));
    lib.update("knuth:ct:a", k).unwrap();
    lib.save(false).unwrap();
    let after = std::fs::read_to_string(dir.join("library.bib")).unwrap();
    let start = src.find("@book{knuth:ct:a,").unwrap();
    assert_eq!(&after[..start], &src[..start], "entries before the edited one are untouched");
}
