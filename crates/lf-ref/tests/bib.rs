//! Parser/writer tests on real and deliberately messy `.bib` files.

use lf_ref::bib::{self, Kind, Piece, Value, format_entry};
use lf_ref::latex::decode;
use lf_ref::names::parse_names;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

const BIBLATEX_EXAMPLES: &str = "/usr/share/texmf-dist/bibtex/bib/biblatex/biblatex/biblatex-examples.bib";

#[test]
fn round_trip_is_byte_exact() {
    for name in ["messy.bib", "zotero-bbt.bib"] {
        let src = fixture(name);
        assert_eq!(bib::parse(&src).to_bib_string(), src, "{name}");
    }
    let crlf = fixture("messy.bib").replace('\n', "\r\n");
    assert_eq!(bib::parse(&crlf).to_bib_string(), crlf);
}

#[test]
fn messy_file_semantics() {
    let b = bib::parse(&fixture("messy.bib"));
    let keys = b.keys();
    assert_eq!(keys, ["quoted", "paren", "nofields", "trailing", "commented", "after_broken", "nested", "müller2021neuronale", "indented", "last"]);
    let problems = b.problems();
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems[0].1.contains("broken1"), "{problems:?}");
    assert!(problems[1].1.contains("broken2"), "{problems:?}");
    assert_eq!(b.entry_line("after_broken"), Some(44));

    let s = b.strings();
    assert_eq!(s["full"], "Journal of Computational Chemistry (Online)");
    let q = b.entry("QUOTED").unwrap();
    assert!(q.is("article"));
    assert_eq!(q.text_field("journal", &s).unwrap(), "Journal of Computational Chemistry");
    assert_eq!(q.raw_field("month", &s).unwrap(), "3");
    assert_eq!(q.text_field("title", &s).unwrap(), "A \"quoted\" title with Braces");
    assert_eq!(q.text_field("pages", &s).unwrap(), "1–10");
    let authors = parse_names(&q.raw_field("author", &s).unwrap());
    assert_eq!(authors.iter().map(|n| n.display()).collect::<Vec<_>>(), ["Jörg Müller", "Erhan Özkal"]);

    let p = b.entry("paren").unwrap();
    assert_eq!(p.text_field("publisher", &s).unwrap(), "Addison-Wesley and others");
    assert_eq!(p.text_field("title", &s).unwrap(), "The TeXbook");
    assert!(b.entry("nofields").unwrap().fields.is_empty());
    assert_eq!(b.entry("commented").unwrap().text_field("url", &s).unwrap(), "https://example.org/a_b%20c");
    assert_eq!(b.entry("nested").unwrap().text_field("title", &s).unwrap(), r"Deeply Nested Braces and $\{x\}$");
    assert!(b.blocks.iter().any(|bl| matches!(&bl.kind, Kind::Preamble(v) if v.0.len() == 2)));
    assert!(b.blocks.iter().any(|bl| bl.kind == Kind::Comment));
    assert_eq!(b.entry("müller2021neuronale").unwrap().text_field("title", &s).unwrap(), "Neuronale Netze für Einsteiger");
}

#[test]
fn zotero_export() {
    let b = bib::parse(&fixture("zotero-bbt.bib"));
    assert!(b.problems().is_empty());
    let s = b.strings();
    let v = b.entry("vaswaniAttentionAllYou2017").unwrap();
    let names = parse_names(&v.raw_field("author", &s).unwrap());
    assert_eq!(names.len(), 8);
    assert_eq!(names[6].display(), "Łukasz Kaiser");
    assert_eq!(v.raw_field("month", &s).unwrap(), "6");
    let l = b.entry("lamportLaTeXDocumentPreparation1994").unwrap();
    assert_eq!(l.text_field("title", &s).unwrap(), "LaTeX: A Document Preparation System");
    let g = b.entry("grafStrukturUndFunktion2019").unwrap();
    let names = parse_names(&g.raw_field("author", &s).unwrap());
    assert_eq!(names[1].family(), "von Bülow");
    assert_eq!(names[1].first, "Hans");
}

#[test]
fn biblatex_examples() {
    let Ok(src) = std::fs::read_to_string(BIBLATEX_EXAMPLES) else {
        eprintln!("skipped: {BIBLATEX_EXAMPLES} not installed");
        return;
    };
    let b = bib::parse(&src);
    assert_eq!(b.to_bib_string(), src);
    assert!(b.problems().is_empty(), "{:?}", b.problems());
    assert_eq!(b.entries().count(), 92);
    assert_eq!(b.strings().len(), 8);
    let s = b.strings();
    let k = b.entry("knuth:ct:a").unwrap();
    assert_eq!(k.text_field("maintitle", &s).unwrap(), "Computers & Typesetting");
    assert_eq!(k.raw_field("date", &s).unwrap(), "1984");
    let a = b.entry("aksin").unwrap();
    let names = parse_names(&a.raw_field("author", &s).unwrap());
    assert_eq!(names[0].display(), "Özge Aksın");
    assert_eq!(names.len(), 7);
    let w = b.entry("westfahl:space").unwrap();
    assert_eq!(w.text_field("subtitle", &s).unwrap(), "Confronting and Avoiding the Realities of Space in American Science Fiction Films");
}

/// Every `.bib` file of the TeX Live installation must survive a round trip, and parsing must
/// stay fast. Files with problems are listed (some are not BibTeX at all).
#[test]
fn texlive_corpus() {
    let root = Path::new("/usr/share/texmf-dist");
    if !root.is_dir() {
        eprintln!("skipped: no TeX Live");
        return;
    }
    let mut files = Vec::new();
    collect(root, &mut files);
    let (mut entries, mut bytes) = (0, 0);
    let t = std::time::Instant::now();
    for f in &files {
        let Ok(src) = std::fs::read(f) else { continue };
        let src = String::from_utf8_lossy(&src).into_owned();
        let b = bib::parse(&src);
        assert_eq!(b.to_bib_string(), src, "{}", f.display());
        entries += b.entries().count();
        bytes += src.len();
        let p = b.problems();
        if !p.is_empty() {
            eprintln!("{}: {} problem(s), first: line {} {}", f.display(), p.len(), p[0].0, p[0].1);
        }
    }
    let el = t.elapsed();
    eprintln!("{} files, {entries} entries, {} KB in {el:?}", files.len(), bytes / 1024);
    assert!(entries > 1000);
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "bib") {
            out.push(p);
        }
    }
}

#[test]
fn editing_changes_only_that_entry() {
    let src = fixture("messy.bib");
    let mut b = bib::parse(&src);
    assert!(b.edit_entry("paren", |e| e.set("year", Value::text("1986"))));
    let out = b.to_bib_string();
    let start = src.find("@book(paren").unwrap();
    let end = start + src[start..].find("\n)\n").unwrap() + 2;
    assert_eq!(&out[..start], &src[..start]);
    assert_eq!(&out[out.len() - (src.len() - end)..], &src[end..]);
    let again = bib::parse(&out);
    assert_eq!(again.entry("paren").unwrap().raw_field("year", &again.strings()).unwrap(), "1986");
    // macros and concatenations survive re-formatting
    assert!(out.contains("publisher = aw # { and } # \"others\","), "{out}");
}

#[test]
fn push_and_remove() {
    let mut b = bib::parse("@misc{a, title={A}}");
    let mut e = bib::Entry::new("article", "b");
    e.set("Title", Value::text("B"));
    e.set("year", Value(vec![Piece::Number("2020".into())]));
    b.push_entry(e.clone());
    assert_eq!(b.to_bib_string(), "@misc{a, title={A}}\n\n@article{b,\n  title = {B},\n  year = 2020,\n}\n");
    let parsed = bib::parse(&b.to_bib_string());
    assert_eq!(parsed.entry("b").unwrap(), &e);
    assert_eq!(b.remove_entry("a").unwrap().key, "a");
    assert_eq!(b.to_bib_string(), "@article{b,\n  title = {B},\n  year = 2020,\n}\n");
    let mut empty = bib::parse("");
    empty.push_entry(bib::Entry::new("misc", "x"));
    assert_eq!(empty.to_bib_string(), "@misc{x,\n}\n");
}

#[test]
fn removing_keeps_spacing() {
    let src = "% head\n\n@misc{a, t={1}}\n\n@misc{b, t={2}}  % note\n\n@misc{c, t={3}}\n";
    let cases = [
        ("a", "% head\n\n@misc{b, t={2}}  % note\n\n@misc{c, t={3}}\n"),
        ("b", "% head\n\n@misc{a, t={1}}\n\n@misc{c, t={3}}\n"),
        ("c", "% head\n\n@misc{a, t={1}}\n\n@misc{b, t={2}}  % note\n"),
    ];
    for (key, want) in cases {
        let mut b = bib::parse(src);
        b.remove_entry(key).unwrap();
        assert_eq!(b.to_bib_string(), want, "removing {key}");
    }
    let mut b = bib::parse("@misc{a,t={1}}\r\n\r\n@misc{b,t={2}}\r\n");
    b.remove_entry("a");
    assert_eq!(b.to_bib_string(), "@misc{b,t={2}}\r\n");
    let mut b = bib::parse("@misc{only,t={1}}");
    b.remove_entry("only");
    assert_eq!(b.to_bib_string(), "");
}

#[test]
fn formatted_entries_parse_back_identically() {
    let b = bib::parse(&fixture("zotero-bbt.bib"));
    for e in b.entries() {
        let text = format_entry(e);
        let again = bib::parse(&text);
        assert_eq!(again.entry(&e.key).unwrap(), e);
    }
}

/// Truncating a file anywhere must neither panic nor lose text.
#[test]
fn any_prefix_round_trips() {
    let src = fixture("messy.bib");
    for (i, _) in src.char_indices() {
        let part = &src[..i];
        assert_eq!(bib::parse(part).to_bib_string(), part, "prefix {i}");
    }
    // and arbitrary garbage
    for s in ["@", "@@@", "@a{", "@a{,", "@a{k,=}", "@a{k,f=#}", "@string{=}", "@a{k,f={}#", "@comment(", "@a (k)", "{}}}@x{y}"] {
        assert_eq!(bib::parse(s).to_bib_string(), s);
    }
    assert_eq!(decode(""), "");
}
