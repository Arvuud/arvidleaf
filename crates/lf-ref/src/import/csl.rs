//! CSL-JSON → BibLaTeX.
//!
//! CSL-JSON is what doi.org returns for Crossref, DataCite and mEDRA DOIs, and what Zotero
//! exports ("CSL JSON"). Values may be strings, numbers or arrays depending on the source;
//! all three are accepted. Text with HTML/JATS markup is converted to LaTeX.

use super::{html_to_latex, protect_case};
use crate::bib::{Entry, Piece, Value};
use crate::latex::encode;
use crate::names::Name;
use serde_json::Value as J;

/// One object or an array of objects.
pub fn from_json(text: &str) -> Result<Vec<Entry>, String> {
    let v: J = serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    match v {
        J::Array(items) => Ok(items.iter().filter(|i| i.is_object()).map(to_entry).collect()),
        J::Object(_) => Ok(vec![to_entry(&v)]),
        _ => Err("expected a CSL-JSON object or array".into()),
    }
}

/// A string from a string, number or the first non-empty element of an array.
fn text(v: &J, name: &str) -> Option<String> {
    let s = match v.get(name)? {
        J::String(s) => s.clone(),
        J::Number(n) => n.to_string(),
        J::Array(a) => a.iter().find_map(|x| match x {
            J::String(s) if !s.trim().is_empty() => Some(s.clone()),
            J::Number(n) => Some(n.to_string()),
            _ => None,
        })?,
        _ => return None,
    };
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// BibLaTeX entry type and where the container title goes.
fn kind(csl_type: &str) -> (&'static str, Option<&'static str>) {
    match csl_type {
        "article-journal" | "journal-article" | "article-magazine" | "article-newspaper" | "review" | "review-book" => ("article", Some("journaltitle")),
        "book" | "monograph" | "edited-book" | "reference-book" | "book-set" => ("book", None),
        "chapter" | "book-chapter" | "book-section" | "book-part" | "entry" | "entry-dictionary" | "entry-encyclopedia" | "reference-entry" => ("incollection", Some("booktitle")),
        "paper-conference" | "proceedings-article" => ("inproceedings", Some("booktitle")),
        "proceedings" | "proceedings-series" => ("proceedings", None),
        "thesis" | "dissertation" => ("thesis", None),
        "report" | "report-component" => ("report", None),
        "dataset" | "database" => ("dataset", None),
        "software" => ("software", None),
        "webpage" | "post" | "post-weblog" => ("online", Some("organization")),
        "manuscript" => ("unpublished", None),
        "patent" => ("patent", None),
        "standard" => ("standard", None),
        // preprints and working papers ("article" in CSL, "posted-content" in Crossref)
        "article" | "posted-content" => ("online", None),
        _ => ("misc", Some("howpublished")),
    }
}

fn name(n: &J) -> Option<Name> {
    if let Some(lit) = text(n, "literal") {
        return Some(Name { last: format!("{{{}}}", encode(&lit)), ..Name::default() });
    }
    let family = text(n, "family")?;
    let given = text(n, "given").unwrap_or_default();
    // a family name with spaces and no given name is an organization ("The pandas team")
    if given.is_empty() && family.contains(' ') {
        return Some(Name { last: format!("{{{}}}", encode(&family)), ..Name::default() });
    }
    let von = [text(n, "dropping-particle"), text(n, "non-dropping-particle")].into_iter().flatten().collect::<Vec<_>>().join(" ");
    // a family name that would be split by BibTeX (two words) is protected
    let last = if family.contains(' ') { format!("{{{}}}", encode(&family)) } else { encode(&family) };
    Some(Name { first: encode(&given), von: encode(&von), last, jr: text(n, "suffix").map(|s| encode(&s)).unwrap_or_default() })
}

fn names(v: &J, role: &str) -> Option<String> {
    let list: Vec<Name> = v.get(role)?.as_array()?.iter().filter_map(name).collect();
    (!list.is_empty()).then(|| crate::names::format_names(&list))
}

/// `issued` → `2017`, `2017-06` or `2017-06-12`.
fn date(v: &J) -> Option<String> {
    for f in ["issued", "published-print", "published-online", "published"] {
        let Some(d) = v.get(f) else { continue };
        if let Some(parts) = d.get("date-parts").and_then(|p| p.get(0)).and_then(J::as_array) {
            let nums: Vec<i64> = parts.iter().filter_map(|x| x.as_i64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))).collect();
            match nums.as_slice() {
                [y, m, d, ..] if (1..=12).contains(m) && (1..=31).contains(d) => return Some(format!("{y:04}-{m:02}-{d:02}")),
                [y, m, ..] if (1..=12).contains(m) => return Some(format!("{y:04}-{m:02}")),
                [y, ..] => return Some(format!("{y:04}")),
                [] => {}
            }
        }
        for k in ["raw", "literal"] {
            if let Some(s) = text(d, k) {
                return Some(edtf_or_year(&s));
            }
        }
    }
    None
}

/// biblatex needs ISO dates: `2017`, `2017-06`, `2017-06-12` stay, free text such as
/// `Spring 2017` becomes its year (or stays if it has none).
fn edtf_or_year(s: &str) -> String {
    let iso = |t: &str| {
        let parts: Vec<&str> = t.split('-').collect();
        !parts.is_empty() && parts.len() <= 3 && parts[0].len() == 4 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) && parts[1..].iter().all(|p| p.len() == 2)
    };
    let t = s.trim();
    if iso(t) {
        return t.to_string();
    }
    let b = t.as_bytes();
    (0..b.len().saturating_sub(3))
        .find(|&i| b[i..i + 4].iter().all(u8::is_ascii_digit) && (i == 0 || !b[i - 1].is_ascii_digit()) && b.get(i + 4).is_none_or(|c| !c.is_ascii_digit()))
        .map_or_else(|| t.to_string(), |i| t[i..i + 4].to_string())
}

const LANGUAGES: &[(&str, &str)] = &[("en", "english"), ("de", "ngerman"), ("fr", "french"), ("es", "spanish"), ("it", "italian"), ("nl", "dutch"), ("pt", "portuguese"), ("ru", "russian"), ("pl", "polish"), ("sv", "swedish"), ("da", "danish"), ("fi", "finnish"), ("no", "norsk"), ("nb", "norsk"), ("cs", "czech"), ("tr", "turkish"), ("el", "greek"), ("la", "latin"), ("ja", "japanese"), ("zh", "chinese")];

pub fn to_entry(v: &J) -> Entry {
    let csl_type = text(v, "type").unwrap_or_default();
    let (k, container_field) = kind(&csl_type);
    let mut e = Entry::new(k, "");
    let set = |e: &mut Entry, f: &str, s: String| e.set(f, Value::text(s));

    // Zotero/Better BibTeX use the citation key as id; URLs and numbers are not keys
    if let Some(id) = text(v, "id").or_else(|| text(v, "citation-key")).or_else(|| text(v, "citationKey"))
        && !id.contains("://")
        && !id.chars().all(|c| c.is_ascii_digit())
        && id.chars().any(|c| c.is_ascii_alphabetic())
        && !id.starts_with("10.")
    {
        e.key = crate::key::sanitize(&id);
    }
    if let Some(a) = names(v, "author") {
        set(&mut e, "author", a);
    }
    if let Some(a) = names(v, "editor") {
        set(&mut e, "editor", a);
    }
    if let Some(a) = names(v, "translator") {
        set(&mut e, "translator", a);
    }
    if let Some(t) = text(v, "title") {
        // "Title: Subtitle" from a source with a separate subtitle stays split
        set(&mut e, "title", protect_case(&html_to_latex(&t)));
    }
    if let Some(t) = text(v, "subtitle") {
        set(&mut e, "subtitle", protect_case(&html_to_latex(&t)));
    }
    if let Some(t) = text(v, "title-short").or_else(|| text(v, "shortTitle")) {
        set(&mut e, "shorttitle", protect_case(&html_to_latex(&t)));
    }
    if let (Some(f), Some(c)) = (container_field, text(v, "container-title")) {
        let c = html_to_latex(&c);
        set(&mut e, f, if f == "organization" { c } else { protect_case(&c) });
    }
    if k == "article"
        && let Some(s) = text(v, "container-title-short").or_else(|| text(v, "journalAbbreviation"))
    {
        set(&mut e, "shortjournal", html_to_latex(&s));
    }
    if let Some(s) = text(v, "collection-title") {
        set(&mut e, "series", html_to_latex(&s));
    }
    if (k == "inproceedings" || k == "proceedings")
        && let Some(s) = text(v, "event-title").or_else(|| text(v, "event").filter(|_| v.get("event").is_some_and(J::is_string))).or_else(|| v.get("event").and_then(|ev| text(ev, "name")))
    {
        set(&mut e, "eventtitle", html_to_latex(&s));
    }
    if let Some(d) = date(v) {
        set(&mut e, "date", d);
    }
    for (csl, bib) in [("volume", "volume"), ("edition", "edition"), ("version", "version"), ("number-of-pages", "pagetotal")] {
        if let Some(s) = text(v, csl) {
            set(&mut e, bib, encode(&s));
        }
    }
    if let Some(s) = text(v, "issue").or_else(|| text(v, "number")) {
        set(&mut e, "number", encode(&s));
    }
    if let Some(p) = text(v, "page") {
        let p = p.replace(['–', '—'], "-");
        let p = if p.contains("--") { p } else { p.replace('-', "--") };
        set(&mut e, "pages", encode(&p));
    }
    if let Some(p) = text(v, "publisher") {
        let field = match k {
            "thesis" | "report" => "institution",
            _ => "publisher",
        };
        // journals: the publisher adds nothing to a citation
        if k != "article" {
            set(&mut e, field, encode(&p));
        }
    }
    if k != "article"
        && let Some(s) = text(v, "publisher-place").or_else(|| text(v, "publisher-location")).or_else(|| text(v, "event-place"))
    {
        set(&mut e, "location", encode(&s));
    }
    if let Some(g) = text(v, "genre") {
        if matches!(k, "thesis" | "report") {
            set(&mut e, "type", encode(&g));
        }
    } else if k == "report" {
        set(&mut e, "type", "techreport".into());
    }
    if let Some(d) = text(v, "DOI") {
        let d = d.to_lowercase();
        // arXiv DOIs (DataCite): add the eprint fields biblatex styles print
        if let Some(id) = d.strip_prefix("10.48550/arxiv.").and_then(crate::library::normalize_arxiv) {
            set(&mut e, "eprint", id);
            set(&mut e, "eprinttype", "arxiv".into());
            e.remove("publisher");
        }
        set(&mut e, "doi", d);
    }
    if let Some(s) = text(v, "ISBN") {
        set(&mut e, "isbn", s);
    }
    if let Some(s) = text(v, "ISSN") {
        set(&mut e, "issn", s);
    }
    if let Some(u) = text(v, "URL") {
        // the DOI link itself is redundant
        let l = u.to_lowercase();
        if !(l.contains("doi.org/") && e.get("doi").is_some()) {
            set(&mut e, "url", u);
        }
    }
    if let Some(s) = text(v, "accessed").or_else(|| v.get("accessed").and_then(|a| date(&serde_json::json!({ "issued": a })))) {
        set(&mut e, "urldate", s);
    }
    if let Some(a) = text(v, "abstract") {
        set(&mut e, "abstract", html_to_latex(&a));
    }
    if let Some(n) = text(v, "note") {
        set(&mut e, "note", html_to_latex(&n));
    }
    if let Some(l) = text(v, "language") {
        let code = l.to_lowercase();
        let code = code.split(['-', '_']).next().unwrap_or("");
        if let Some((_, lang)) = LANGUAGES.iter().find(|(c, n)| *c == code || *n == code) {
            set(&mut e, "langid", (*lang).to_string());
        }
    }
    if let Some(kw) = text(v, "keyword") {
        let tags: Vec<String> = kw.split([',', ';']).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        crate::library::set_tags(&mut e, &tags);
    }
    // a CSL number that is purely numeric is written as a number token
    for f in ["volume", "number", "edition"] {
        if let Some(Value(p)) = e.get(f)
            && let [Piece::Braced(s)] = p.as_slice()
            && !s.is_empty()
            && s.chars().all(|c| c.is_ascii_digit())
        {
            let n = s.clone();
            e.set(f, Value(vec![Piece::Number(n)]));
        }
    }
    e
}
