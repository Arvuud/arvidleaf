//! Import from saved real answers of Crossref, DataCite, arXiv and OpenLibrary, plus
//! Zotero exports and PDFs. Online tests are `#[ignore]`d (run with `--ignored`).

use lf_ref::bib::{self, Entry, format_entry};
use lf_ref::import::{self, Identifier, Input, arxiv, csl, isbn, pdf};
use lf_ref::Library;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

fn f(e: &Entry, name: &str) -> String {
    e.raw_field(name, &Default::default()).unwrap_or_else(|| panic!("no field {name} in\n{}", format_entry(e)))
}

fn one(name: &str) -> Entry {
    let mut v = csl::from_json(&fixture(name)).unwrap();
    assert_eq!(v.len(), 1);
    v.pop().unwrap()
}

/// Every imported entry must be valid BibLaTeX that parses back to itself.
fn valid(e: &Entry) {
    let mut e = e.clone();
    if e.key.is_empty() {
        e.key = "k".into(); // the library assigns keys
    }
    let e = &e;
    let text = format_entry(e);
    let b = bib::parse(&text);
    assert!(b.problems().is_empty(), "{:?}\n{text}", b.problems());
    assert_eq!(b.entries().next().unwrap(), e, "{text}");
}

#[test]
fn crossref_journal_article() {
    let e = one("net/crossref-article.json");
    valid(&e);
    assert!(e.is("article"));
    assert_eq!(f(&e, "author"), "Sigfridsson, Emma and Ryde, Ulf");
    assert_eq!(f(&e, "title"), "Comparison of methods for deriving atomic charges from the electrostatic potential and moments");
    assert_eq!(f(&e, "journaltitle"), "Journal of Computational Chemistry");
    assert_eq!(f(&e, "shortjournal"), "J. Comput. Chem.");
    assert_eq!((f(&e, "volume"), f(&e, "number"), f(&e, "pages")), ("19".into(), "4".into(), "377--395".into()));
    assert_eq!(f(&e, "date"), "1998-03");
    assert_eq!(f(&e, "doi"), "10.1002/(sici)1096-987x(199803)19:4<377::aid-jcc1>3.0.co;2-p");
    assert_eq!(f(&e, "langid"), "english");
    assert!(e.get("url").is_none(), "DOI link is redundant");
    assert!(e.get("publisher").is_none());
}

#[test]
fn crossref_chapter_book_proceedings() {
    let c = one("net/crossref-chapter.json");
    valid(&c);
    assert!(c.is("incollection"));
    assert_eq!(f(&c, "booktitle"), "The Philosophy of Quantum Mechanics");
    assert_eq!(f(&c, "publisher"), "Springer Netherlands");
    assert_eq!(f(&c, "location"), "Dordrecht");
    assert_eq!(f(&c, "pages"), "19--25");
    assert_eq!(f(&c, "author"), "Blokhintsev, D. I.");

    let b = one("net/crossref-book.json");
    valid(&b);
    assert!(b.is("book"));
    assert_eq!(f(&b, "isbn"), "9789048183357");
    assert!(b.get("booktitle").is_none());

    let p = one("net/crossref-proceedings.json");
    valid(&p);
    assert!(p.is("inproceedings"));
    let abs = f(&p, "abstract");
    assert!(abs.starts_with("The paper presents the state feedback") && !abs.contains('<'), "{abs}");
    assert!(p.get("booktitle").is_some());
}

#[test]
fn datacite() {
    let a = one("net/datacite-arxiv.json");
    valid(&a);
    assert!(a.is("online"));
    assert_eq!(f(&a, "title"), "Attention Is All You Need");
    assert!(f(&a, "author").starts_with("Vaswani, Ashish and Shazeer, Noam"));
    assert_eq!((f(&a, "eprint"), f(&a, "eprinttype"), f(&a, "doi")), ("1706.03762".into(), "arxiv".into(), "10.48550/arxiv.1706.03762".into()));
    assert!(a.get("publisher").is_none());
    let z = one("net/datacite-zenodo.json");
    valid(&z);
    assert!(z.is("software"));
    assert_eq!(f(&z, "author"), "{The pandas development team}");
    assert_eq!(f(&z, "version"), "v3.1.1");
    assert_eq!(f(&z, "date"), "2026-11-04");
}

#[test]
fn arxiv_answers() {
    let e = arxiv::parse(&fixture("net/arxiv-hep-th-9711200.xml")).unwrap().unwrap();
    valid(&e);
    assert_eq!((f(&e, "eprint"), f(&e, "eprinttype"), f(&e, "eprintclass")), ("hep-th/9711200".into(), "arxiv".into(), "hep-th".into()));
    assert_eq!(f(&e, "author"), "Maldacena, Juan M.");
    assert_eq!(f(&e, "date"), "1997-11-27");
    assert_eq!(f(&e, "doi"), "10.1023/a:1026654312961");
    assert_eq!(f(&e, "note"), "Adv.Theor.Math.Phys.2:231-252,1998");
    assert!(f(&e, "abstract").contains(r"${\cal N} =4$"));
    let v = arxiv::parse(&fixture("net/arxiv-1706.03762.xml")).unwrap().unwrap();
    valid(&v);
    assert_eq!(f(&v, "eprint"), "1706.03762");
    assert_eq!(f(&v, "url"), "https://arxiv.org/abs/1706.03762");
    // the same paper from DataCite is recognized as a duplicate
    let dir = tempdir("arxivdup");
    let mut lib = Library::open(&dir).unwrap();
    lib.add(v).unwrap();
    assert!(lib.add(one("net/datacite-arxiv.json")).is_err());
}

#[test]
fn isbn_answers() {
    let e = isbn::from_openlibrary(&fixture("net/openlibrary-9780201529838.json"), "9780201529838").unwrap().unwrap();
    valid(&e);
    assert!(e.is("book"));
    assert_eq!(f(&e, "author"), "Lamport, Leslie");
    assert_eq!(f(&e, "publisher"), "Addison-Wesley Pub. Co.");
    assert_eq!(f(&e, "location"), "Reading, Mass");
    assert_eq!(f(&e, "date"), "1994");
    assert_eq!(f(&e, "pagetotal"), "272");
    assert!(f(&e, "subtitle").starts_with("A document preparation system"));
    assert_eq!(isbn::from_openlibrary(&fixture("net/openlibrary-notfound.json"), "9783000000000").unwrap(), None);
    assert_eq!(isbn::crossref_book_doi(&fixture("net/crossref-isbn-search.json")).as_deref(), Some("10.1007/978-94-017-3427-1"));
}

#[test]
fn zotero_csl_export() {
    let es = csl::from_json(&fixture("zotero-csl.json")).unwrap();
    assert_eq!(es.len(), 3);
    es.iter().for_each(valid);
    assert_eq!(es[0].key, "lamportLaTeXDocumentPreparation1994");
    assert_eq!(f(&es[0], "title"), "{LaTeX}: a document preparation system");
    assert_eq!(f(&es[0], "date"), "1994");
    assert_eq!(f(&es[0], "isbn"), "978-0-201-52983-8");
    assert_eq!(es[1].key, "", "a Zotero URI is not a key");
    assert_eq!(f(&es[1], "title"), r"Über \emph{Drosophila} \& Co.");
    assert_eq!(f(&es[1], "author"), "Müller, Jürgen and van der Waals, Johannes and {Max Planck Society}");
    assert_eq!(f(&es[1], "urldate"), "2026-11-01");
    assert_eq!(f(&es[1], "date"), "2020-05-17");
    assert_eq!(f(&es[1], "keywords"), "genetics, flies");
    assert_eq!(f(&es[1], "url"), "https://example.org/article");
    assert!(es[2].is("online"));
    assert_eq!(f(&es[2], "organization"), "arvidleaf docs");
    assert_eq!(f(&es[2], "date"), "2026-11");
}

#[test]
fn pasted_text_into_library() {
    let dir = tempdir("paste");
    let mut lib = Library::open(&dir).unwrap();
    let Input::Entries(es, problems) = import::read_text(&fixture("zotero-bbt.bib")) else { panic!() };
    assert!(problems.is_empty());
    let r = import::add_all(&mut lib, es);
    assert_eq!(r.added, ["vaswaniAttentionAllYou2017", "lamportLaTeXDocumentPreparation1994", "grafStrukturUndFunktion2019"]);
    let Input::Entries(es, _) = import::read_text(&fixture("zotero-csl.json")) else { panic!() };
    let r = import::add_all(&mut lib, es);
    assert_eq!(r.duplicates.len(), 1, "Lamport is already there: {r:?}");
    assert_eq!(r.added, ["mueller2020drosophila", "web1"], "\"Über\" is a stop word: {r:?}");
    // @string macros of the source are resolved, months stay
    let Input::Entries(es, _) = import::read_text("@string{jcc = {J. Comput. Chem.}}\n@article{x, journal = jcc # { (online)}, month = mar, title={T}}") else { panic!() };
    assert_eq!(f(&es[0], "journal"), "J. Comput. Chem. (online)");
    assert!(format_entry(&es[0]).contains("month = mar,"));
    assert_eq!(import::read_text("10.1000/abc1\narXiv:2101.00001"), Input::Identifiers(vec![Identifier::Doi("10.1000/abc1".into()), Identifier::Arxiv("2101.00001".into())]));
    assert_eq!(import::read_text("just words"), Input::Nothing);
}

/// A real PDF made with pdflatex: the DOI on page 1 is found.
#[test]
fn pdf_with_doi() {
    if Command::new("pdflatex").arg("--version").output().is_err() || Command::new("pdftotext").arg("-v").output().is_err() {
        eprintln!("skipped: pdflatex or pdftotext missing");
        return;
    }
    let dir = tempdir("pdf");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.tex"), "\\documentclass{article}\\begin{document}\\title{A paper}\\maketitle\nJ. Comput. Chem. 19 (1998). DOI: 10.1002/jcc.540190402\n\nSee also 10.1000/other.\\end{document}\n").unwrap();
    let ok = Command::new("pdflatex").args(["-interaction=nonstopmode", "p.tex"]).current_dir(&dir).output().unwrap().status.success();
    assert!(ok);
    assert_eq!(pdf::identify(&dir.join("p.pdf")).unwrap(), Some(Identifier::Doi("10.1002/jcc.540190402".into())));
    assert!(pdf::identify(&dir.join("missing.pdf")).is_err());
}

use std::process::Command;

fn tempdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("lf-ref-import-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// Real lookups. `cargo test -p lf-ref --test import -- --ignored`
#[test]
#[ignore]
fn online() {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let o = lf_ref::net::NetOptions::default();
    let doi = import::fetch(&Identifier::Doi("10.1002/(sici)1096-987x(199803)19:4<377::aid-jcc1>3.0.co;2-p".into()), &o, &cancel).unwrap();
    assert_eq!(f(&doi, "journaltitle"), "Journal of Computational Chemistry");
    // arXiv with journal DOI: journal record plus eprint
    let m = import::fetch(&Identifier::Arxiv("hep-th/9711200".into()), &o, &cancel).unwrap();
    assert!(m.is("article"), "{}", format_entry(&m));
    assert_eq!(f(&m, "eprint"), "hep-th/9711200");
    let b = import::fetch(&Identifier::Isbn("9789401734271".into()), &o, &cancel).unwrap();
    assert!(b.is("book"), "{}", format_entry(&b));
    let missing = import::fetch(&Identifier::Doi("10.9999/does-not-exist".into()), &o, &cancel);
    assert!(matches!(missing, Err(import::ImportError::NotFound(_))), "{missing:?}");
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(import::fetch(&Identifier::Doi("10.1002/jcc.540190402".into()), &o, &cancel), Err(import::ImportError::Net(lf_ref::net::NetError::Cancelled))));
    if let Ok(dump) = std::env::var("ARVIDREF_DUMP") {
        let all = [doi, m, b].iter().map(format_entry).collect::<Vec<_>>().join("\n\n");
        std::fs::write(dump, all).unwrap();
    }
}

/// Writes every fixture import plus `online.bib` into a library in `ARVIDREF_DUMP_DIR`
/// (for compiling them with biber). `cargo test -p lf-ref --test import dump -- --ignored`
#[test]
#[ignore]
fn dump_library() {
    let Ok(dir) = std::env::var("ARVIDREF_DUMP_DIR") else { return };
    let dir = PathBuf::from(dir);
    let _ = std::fs::remove_file(dir.join("library.bib"));
    let mut lib = Library::open(&dir).unwrap();
    let mut all: Vec<Entry> = ["net/crossref-article.json", "net/crossref-chapter.json", "net/crossref-book.json", "net/crossref-proceedings.json", "net/datacite-zenodo.json", "zotero-csl.json"].iter().flat_map(|n| csl::from_json(&fixture(n)).unwrap()).collect();
    all.push(arxiv::parse(&fixture("net/arxiv-1706.03762.xml")).unwrap().unwrap());
    all.push(arxiv::parse(&fixture("net/arxiv-hep-th-9711200.xml")).unwrap().unwrap());
    all.push(isbn::from_openlibrary(&fixture("net/openlibrary-9780201529838.json"), "9780201529838").unwrap().unwrap());
    if let Ok(online) = std::fs::read_to_string(dir.join("online.bib")) {
        all.extend(bib::parse(&online.replace("{,\n", "{tmp,\n")).entries().cloned().map(|mut e| {
            e.key.clear();
            e
        }));
    }
    for mut e in all {
        lib.add_anyway(&mut e);
    }
    lib.save(true).unwrap();
    eprintln!("{} entries", lib.len());
}
