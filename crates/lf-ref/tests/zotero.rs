//! Import from a Zotero data directory built from `fixtures/zotero/zotero.sql` (needs the
//! `sqlite3` program; skipped without it).

use lf_ref::Library;
use lf_ref::import::zotero;
use std::path::{Path, PathBuf};

fn have_sqlite() -> bool {
    std::process::Command::new("sqlite3").arg("-version").output().is_ok_and(|o| o.status.success())
}

/// A Zotero data directory with `zotero.sqlite` and the stored PDF of item 1.
fn zotero_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lf-ref-zotero-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("storage/ATTA0001")).unwrap();
    std::fs::write(dir.join("storage/ATTA0001/Müller et al. - 2021 - Lorem.pdf"), b"%PDF-1.4 lorem").unwrap();
    std::fs::create_dir_all(dir.join("storage/ATTA0002")).unwrap();
    std::fs::write(dir.join("storage/ATTA0002/snapshot.html"), b"<html>").unwrap();
    let sql = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/zotero/zotero.sql")).unwrap();
    let mut child = std::process::Command::new("sqlite3").arg(dir.join("zotero.sqlite")).stdin(std::process::Stdio::piped()).spawn().unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(&sql).unwrap();
    assert!(child.wait().unwrap().success());
    dir
}

fn field(e: &lf_ref::Entry, n: &str) -> String {
    e.raw_field(n, &Default::default()).unwrap_or_default()
}

#[test]
fn read_zotero_database() {
    if !have_sqlite() {
        eprintln!("sqlite3 not installed – skipped");
        return;
    }
    let dir = zotero_dir("read");
    let data = zotero::read(&dir).unwrap();

    // personal library only, trash skipped, notes/attachments are not items
    let keys: Vec<&str> = data.items.iter().map(|i| i.zotero_key.as_str()).collect();
    assert_eq!(keys, ["AAAA1111", "BBBB2222", "CCCC3333", "DDDD4444", "GGGG7777"]);
    assert_eq!(data.collections, ["Teaching-Old", "Thesis", "Thesis/Chapter 2", "Thesis/Empty"]);
    assert_eq!(data.standalone, 2, "standalone note and PDF");
    assert_eq!(data.missing_files.len(), 2, "linked file elsewhere and unsynced file: {:?}", data.missing_files);

    let a = &data.items[0];
    let e = &a.entry;
    assert!(e.is("article"));
    assert_eq!(field(e, "author"), "Müller, Jürgen and Smith, Anna");
    assert_eq!(field(e, "editor"), "Editor, Eve");
    assert_eq!(field(e, "title"), "Lorem ipsum dolor sit amet: \\emph{consectetur} adipiscing");
    assert_eq!(field(e, "journaltitle"), "Journal of Lorem Studies");
    assert_eq!(field(e, "shortjournal"), "J. Lorem Stud.");
    assert_eq!(field(e, "date"), "2021-03-15");
    assert_eq!(field(e, "pages"), "101--120");
    assert_eq!(field(e, "doi"), "10.1234/lorem.2021.001");
    assert_eq!(field(e, "langid"), "english");
    assert_eq!(field(e, "urldate"), "2024-01-05");
    assert_eq!(field(e, "keywords"), "lorem, methods");
    assert_eq!(field(e, "annotation"), "Read \\textbf{section 2} \\& check the proof.\n\nSecond paragraph.", "the deleted note is left out");
    assert_eq!(field(e, lf_ref::library::ADDED), "2024-01-05T10:22:11Z");
    assert!(e.get("url").is_none(), "the DOI link is redundant");
    assert_eq!(a.files, [dir.join("storage/ATTA0001/Müller et al. - 2021 - Lorem.pdf")], "the HTML snapshot is skipped");
    assert_eq!(a.collections, ["Thesis/Chapter 2"]);

    let book = &data.items[1].entry;
    assert_eq!(book.key, "ipsum2019book", "citation key from Extra");
    assert_eq!(field(book, "author"), "{Lorem Ipsum Consortium}");
    assert_eq!(field(book, "location"), "Berlin");
    assert_eq!(field(book, "edition"), "2");
    assert_eq!(field(book, "note"), "tex.note: kept");

    let thesis = &data.items[2].entry;
    assert_eq!(thesis.key, "amet2018dolor");
    assert_eq!(field(thesis, "institution"), "Technische Universität Amet");
    assert_eq!(field(thesis, "type"), "Dissertation");
    assert_eq!(field(thesis, "date"), "2018", "biblatex needs an ISO date");

    let chapter = &data.items[3].entry;
    assert!(chapter.is("incollection"));
    assert_eq!(field(chapter, "booktitle"), "Handbook of Amet");
    assert_eq!(field(chapter, "editor"), "Handbook, Hanna");
    assert_eq!(field(chapter, "date"), "2020-07");
    assert!(data.items[3].collections.is_empty(), "its collection is in the trash");

    let pre = &data.items[4].entry;
    assert_eq!(field(pre, "eprint"), "1706.03762");
    assert_eq!(field(pre, "eprinttype"), "arxiv");
    assert!(pre.get("number").is_none(), "the archive ID repeats the eprint");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn zotero_into_library() {
    if !have_sqlite() {
        return;
    }
    let zdir = zotero_dir("lib");
    let ldir = zdir.with_file_name(format!("{}-library", zdir.file_name().unwrap().to_string_lossy()));
    let _ = std::fs::remove_dir_all(&ldir);
    let mut lib = Library::open(&ldir).unwrap();
    // already in the library: the preprint (same arXiv ID)
    let mut pre = lf_ref::bib::parse("@online{vaswani2017attention, title={Attention Is All You Need}, eprint={1706.03762}, eprinttype={arxiv}}").entries().next().unwrap().clone();
    lib.add_anyway(&mut pre);

    let data = zotero::read(&zdir).unwrap();
    let r = zotero::add_to_library(&mut lib, &data);
    assert_eq!(r.added, ["mueller2021lorem", "ipsum2019book", "amet2018dolor", "sit2020chapter"]);
    assert_eq!(r.duplicates, [("Attention-like Lorem".to_string(), "vaswani2017attention".to_string())]);
    assert_eq!(r.collections, 4);
    assert_eq!(lib.collections.keys("Thesis", true), ["ipsum2019book", "mueller2021lorem", "vaswani2017attention"]);

    // files are copied separately (on a worker thread in the app), then linked
    assert_eq!(r.files.len(), 1);
    let (key, src) = &r.files[0];
    let copied = zotero::copy_file(lib.dir(), key, src).unwrap();
    lib.attach(key, &copied).unwrap();
    assert_eq!(lib.files(key), std::slice::from_ref(&copied));
    assert!(copied.starts_with(ldir.join("files/mueller2021lorem")) && src.is_file(), "the Zotero file stays");
    lib.save(false).unwrap();

    // importing again adds nothing
    let again = zotero::add_to_library(&mut lib, &data);
    assert!(again.added.is_empty() && again.files.is_empty() && again.collections == 0);
    assert_eq!(again.duplicates.len(), 5);
    let _ = std::fs::remove_dir_all(&zdir);
    let _ = std::fs::remove_dir_all(&ldir);
}

#[test]
fn missing_database_is_explained() {
    let err = zotero::read(Path::new("/nonexistent/zotero-dir")).unwrap_err();
    assert!(err.contains("zotero.sqlite not found"), "{err}");
}

/// Imports the fixture into the library in `$ARVIDREF_DUMP_DIR` (for compiling it with
/// biber). `cargo test -p lf-ref --test zotero dump -- --ignored`
#[test]
#[ignore]
fn dump_zotero_library() {
    let Ok(out) = std::env::var("ARVIDREF_DUMP_DIR") else { return };
    let out = std::path::PathBuf::from(out);
    let _ = std::fs::remove_dir_all(&out);
    let zdir = zotero_dir("dump");
    let mut lib = Library::open(&out).unwrap();
    let data = zotero::read(&zdir).unwrap();
    let r = zotero::add_to_library(&mut lib, &data);
    for (k, src) in &r.files {
        let p = zotero::copy_file(lib.dir(), k, src).unwrap();
        lib.attach(k, &p).unwrap();
    }
    lib.save(true).unwrap();
    eprintln!("{} entries: {:?}", lib.len(), r.added);
    let _ = std::fs::remove_dir_all(&zdir);
}
