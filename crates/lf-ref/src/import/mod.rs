//! Getting entries into the library.
//!
//! - Offline: BibTeX/BibLaTeX ([`bibtex`]), CSL-JSON such as Zotero exports ([`csl`]),
//!   PDFs (DOI or arXiv ID read from the first page, [`pdf`]).
//! - Online (on request only): DOI via doi.org (CSL-JSON for Crossref, DataCite, mEDRA …),
//!   arXiv via the arXiv API ([`arxiv`]), ISBN via OpenLibrary with Crossref as fallback
//!   ([`isbn`]).
//!
//! [`detect`] recognizes what the user pasted; [`fetch`] resolves an identifier.

pub mod arxiv;
pub mod bibtex;
pub mod csl;
pub mod isbn;
pub mod pdf;

use crate::bib::{Entry, Value};
use crate::library::{AddError, Library, normalize_arxiv, normalize_doi};
use crate::net::{self, NetError, NetOptions};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identifier {
    /// Lower-case DOI without resolver prefix.
    Doi(String),
    /// arXiv ID without version.
    Arxiv(String),
    /// ISBN-13 without hyphens.
    Isbn(String),
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Identifier::Doi(d) => write!(f, "DOI {d}"),
            Identifier::Arxiv(a) => write!(f, "arXiv:{a}"),
            Identifier::Isbn(i) => write!(f, "ISBN {i}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportError {
    Net(NetError),
    /// The source answered but has no record for the identifier.
    NotFound(Identifier),
    /// The answer could not be read.
    Format(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Net(e) => write!(f, "{e}"),
            ImportError::NotFound(id) => write!(f, "{id} was not found"),
            ImportError::Format(m) => write!(f, "unexpected answer: {m}"),
        }
    }
}

impl From<NetError> for ImportError {
    fn from(e: NetError) -> Self {
        ImportError::Net(e)
    }
}

/// Recognizes a DOI (also as URL or `doi:`), an arXiv ID (also as URL, `arXiv:` or the
/// arXiv DOI `10.48550/arXiv.…`) or an ISBN-10/13 with a valid check digit.
pub fn detect(input: &str) -> Option<Identifier> {
    let s = input.trim().trim_matches(['<', '>', '"', '\'']);
    if s.is_empty() || s.contains(char::is_whitespace) && !s.to_lowercase().starts_with("isbn") {
        return None;
    }
    let l = s.to_lowercase();
    if l.contains("arxiv.org/") || l.starts_with("arxiv:") {
        return normalize_arxiv(s).map(Identifier::Arxiv);
    }
    // the whole input is the DOI (it may contain characters such as `<>` that `find_doi`
    // stops at in running text)
    if let Some(d) = normalize_doi(s).filter(|d| (l.starts_with("10.") || l.contains("doi")) && find_doi(d).is_some_and(|f| d.starts_with(&f.to_lowercase()))) {
        if let Some(a) = d.strip_prefix("10.48550/arxiv.").and_then(normalize_arxiv) {
            return Some(Identifier::Arxiv(a));
        }
        return Some(Identifier::Doi(d));
    }
    if let Some(i) = normalize_isbn(s) {
        return Some(Identifier::Isbn(i));
    }
    // bare new-style arXiv ID (2101.00001, 1706.03762v5)
    normalize_arxiv(s).filter(|a| a.contains('.')).map(Identifier::Arxiv)
}

/// All identifiers in a pasted text, one per line or separated by spaces/commas.
pub fn detect_all(text: &str) -> Vec<Identifier> {
    let mut out: Vec<Identifier> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let found: Vec<Identifier> = match detect(line) {
            Some(id) => vec![id],
            None => line.split([' ', '\t', ',', ';']).filter_map(detect).collect(),
        };
        for id in found {
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }
    out
}

/// ISBN-10 or ISBN-13 (hyphens/spaces, optional `ISBN` prefix) → ISBN-13 if the check digit
/// is right.
pub fn normalize_isbn(s: &str) -> Option<String> {
    let l = s.trim();
    let l = if l.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("isbn")) { &l[4..] } else { l };
    let l = l.strip_prefix("-13").or_else(|| l.strip_prefix("-10")).unwrap_or(l);
    let l = l.trim_start_matches([':', ' ']);
    if l.chars().any(|c| !(c.is_ascii_digit() || matches!(c, '-' | ' ' | 'x' | 'X'))) {
        return None;
    }
    let d: Vec<char> = l.chars().filter(|c| !matches!(c, '-' | ' ')).collect();
    match d.len() {
        13 if d.iter().all(char::is_ascii_digit) => {
            let sum: u32 = d.iter().enumerate().map(|(i, c)| c.to_digit(10).unwrap_or(0) * if i % 2 == 0 { 1 } else { 3 }).sum();
            (sum.is_multiple_of(10) && (d[..3] == ['9', '7', '8'] || d[..3] == ['9', '7', '9'])).then(|| d.iter().collect())
        }
        10 if d[..9].iter().all(char::is_ascii_digit) => {
            let sum: u32 = d.iter().enumerate().map(|(i, c)| (10 - i as u32) * if matches!(c, 'x' | 'X') { 10 } else { c.to_digit(10).unwrap_or(99) }).sum();
            if !sum.is_multiple_of(11) || d[9..].iter().any(|c| !c.is_ascii_digit() && !matches!(c, 'x' | 'X')) {
                return None;
            }
            let body: String = std::iter::once("978".to_string()).chain(d[..9].iter().map(|c| c.to_string())).collect();
            let sum: u32 = body.chars().enumerate().map(|(i, c)| c.to_digit(10).unwrap_or(0) * if i % 2 == 0 { 1 } else { 3 }).sum();
            Some(format!("{body}{}", (10 - sum % 10) % 10))
        }
        _ => None,
    }
}

/// The first DOI in a text (`10.<4–9 digits>/<suffix>`), trailing punctuation removed.
pub fn find_doi(text: &str) -> Option<String> {
    find_dois(text).into_iter().next()
}

/// All DOIs in a text, in order. DOIs after the word `doi` or in a doi.org link come first.
pub fn find_dois(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut marked = Vec::new();
    let mut plain = Vec::new();
    let mut i = 0;
    while let Some(p) = text[i..].find("10.") {
        let s = i + p;
        i = s + 3;
        let mut j = s + 3;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if !(4..=9).contains(&(j - s - 3)) || b.get(j) != Some(&b'/') || (s > 0 && b[s - 1].is_ascii_alphanumeric()) {
            continue;
        }
        let mut e = j + 1;
        while e < b.len() && !(b[e].is_ascii_whitespace() || matches!(b[e], b'"' | b'<' | b'>' | b'{' | b'}')) {
            e += 1;
        }
        let mut d = &text[s..e];
        // trailing punctuation, and closing brackets that were not opened inside the DOI
        loop {
            let t = d.trim_end_matches(['.', ',', ';', ':']);
            let t = if (t.ends_with(')') && t.matches(')').count() > t.matches('(').count()) || (t.ends_with(']') && t.matches(']').count() > t.matches('[').count()) { &t[..t.len() - 1] } else { t };
            if t.len() == d.len() {
                break;
            }
            d = t;
        }
        if d.len() <= j + 1 - s {
            continue;
        }
        i = s + d.len();
        let before = text[..s].to_lowercase();
        let before = before.trim_end_matches([' ', ':', '/', '\u{a0}']);
        let is_marked = before.ends_with("doi") || before.ends_with("doi.org") || before.ends_with("dx.doi.org");
        let d = d.to_string();
        if is_marked { marked.push(d) } else { plain.push(d) }
    }
    let mut out = marked;
    for d in plain {
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

/// Looks up an identifier online and returns the entry (without key; the library assigns
/// one). arXiv preprints with a journal DOI get the journal record plus the eprint fields.
pub fn fetch(id: &Identifier, opts: &NetOptions, cancel: &AtomicBool) -> Result<Entry, ImportError> {
    match id {
        Identifier::Doi(d) => fetch_doi(d, opts, cancel),
        Identifier::Arxiv(a) => {
            let url = format!("https://export.arxiv.org/api/query?id_list={}", net::encode_path(a));
            let xml = net::get(&url, Some("application/atom+xml"), opts, cancel)?;
            let pre = arxiv::parse(&xml).map_err(ImportError::Format)?.ok_or_else(|| ImportError::NotFound(id.clone()))?;
            let journal_doi = pre.get("doi").map(|v| v.resolve(&Default::default()));
            match journal_doi.map(|d| fetch_doi(&d, opts, cancel)) {
                Some(Ok(mut e)) => {
                    for f in ["eprint", "eprinttype", "eprintclass"] {
                        if let Some(v) = pre.get(f) {
                            e.set(f, v.clone());
                        }
                    }
                    if e.get("abstract").is_none()
                        && let Some(v) = pre.get("abstract")
                    {
                        e.set("abstract", v.clone());
                    }
                    Ok(e)
                }
                Some(Err(ImportError::Net(NetError::Cancelled))) => Err(ImportError::Net(NetError::Cancelled)),
                // journal record unavailable: the preprint is still useful
                _ => Ok(pre),
            }
        }
        Identifier::Isbn(i) => isbn::fetch(i, opts, cancel)?.ok_or_else(|| ImportError::NotFound(id.clone())),
    }
}

fn fetch_doi(doi: &str, opts: &NetOptions, cancel: &AtomicBool) -> Result<Entry, ImportError> {
    let url = format!("https://doi.org/{}", net::encode_path(doi));
    let json = match net::get(&url, Some("application/vnd.citationstyles.csl+json"), opts, cancel) {
        Err(NetError::NotFound) => return Err(ImportError::NotFound(Identifier::Doi(doi.to_string()))),
        r => r?,
    };
    let mut entries = csl::from_json(&json).map_err(ImportError::Format)?;
    let mut e = entries.pop().ok_or_else(|| ImportError::NotFound(Identifier::Doi(doi.to_string())))?;
    if e.get("doi").is_none() {
        e.set("doi", Value::text(doi));
    }
    Ok(e)
}

/// Result of importing several entries.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub added: Vec<String>,
    /// (key or title of the imported entry, key in the library)
    pub duplicates: Vec<(String, String)>,
    /// Malformed parts of the input: (line, message).
    pub problems: Vec<(usize, String)>,
}

/// Adds entries to the library, skipping duplicates.
pub fn add_all(lib: &mut Library, entries: Vec<Entry>) -> Report {
    let mut r = Report::default();
    for e in entries {
        let label = if e.key.is_empty() { e.text_field("title", lib.strings()).unwrap_or_default() } else { e.key.clone() };
        match lib.add(e) {
            Ok(k) => r.added.push(k),
            Err(AddError::Duplicate(k)) => r.duplicates.push((label, k)),
        }
    }
    r
}

/// What a pasted or dropped text contains.
#[derive(Debug, PartialEq)]
pub enum Input {
    Entries(Vec<Entry>, Vec<(usize, String)>),
    Identifiers(Vec<Identifier>),
    Nothing,
}

/// Classifies pasted text: CSL-JSON, BibTeX, or a list of identifiers.
pub fn read_text(text: &str) -> Input {
    let t = text.trim_start();
    if (t.starts_with('{') || t.starts_with('['))
        && let Ok(es) = csl::from_json(t)
    {
        return Input::Entries(es, Vec::new());
    }
    let (entries, problems) = bibtex::entries(text);
    if !entries.is_empty() || !problems.is_empty() {
        return Input::Entries(entries, problems);
    }
    let ids = detect_all(text);
    if ids.is_empty() { Input::Nothing } else { Input::Identifiers(ids) }
}

/// HTML/JATS markup in metadata → LaTeX (`<i>` → `\emph`, `<sub>` → `\textsubscript`,
/// entities decoded, text escaped). `<jats:title>` headings are dropped.
pub fn html_to_latex(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    let mut skip = 0usize; // inside <jats:title>
    while !rest.is_empty() {
        match rest.find('<') {
            Some(p) => {
                if skip == 0 {
                    out.push_str(&crate::latex::encode(&decode_entities(&rest[..p])));
                }
                let Some(e) = rest[p..].find('>') else {
                    if skip == 0 {
                        out.push_str(&crate::latex::encode(&decode_entities(&rest[p..])));
                    }
                    break;
                };
                let tag = rest[p + 1..p + e].trim();
                rest = &rest[p + e + 1..];
                let closing = tag.starts_with('/');
                let name = tag.trim_start_matches('/').split([' ', '/']).next().unwrap_or("").to_lowercase();
                let name = name.strip_prefix("jats:").or_else(|| name.strip_prefix("mml:")).unwrap_or(&name);
                match (name, closing) {
                    ("title", false) => skip += 1,
                    ("title", true) => skip = skip.saturating_sub(1),
                    _ if skip > 0 => {}
                    ("i" | "em" | "italic", false) => out.push_str("\\emph{"),
                    ("b" | "strong" | "bold", false) => out.push_str("\\textbf{"),
                    ("sub", false) => out.push_str("\\textsubscript{"),
                    ("sup", false) => out.push_str("\\textsuperscript{"),
                    ("sc" | "small-caps", false) => out.push_str("\\textsc{"),
                    ("i" | "em" | "italic" | "b" | "strong" | "bold" | "sub" | "sup" | "sc" | "small-caps", true) => out.push('}'),
                    ("p" | "br" | "sec", _) => out.push(' '),
                    _ => {}
                }
            }
            None => {
                if skip == 0 {
                    out.push_str(&crate::latex::encode(&decode_entities(rest)));
                }
                break;
            }
        }
    }
    balance(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Closes or drops braces left open/unmatched by broken markup.
fn balance(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    let mut prev = ' ';
    for c in s.chars() {
        match c {
            '{' if prev != '\\' => {
                depth += 1;
                out.push(c);
            }
            '}' if prev != '\\' => {
                if depth > 0 {
                    depth -= 1;
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
        prev = c;
    }
    out.extend(std::iter::repeat_n('}', depth));
    out
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let end = rest.find(';').filter(|&e| e <= 10);
        let decoded = end.and_then(|e| {
            let ent = &rest[1..e];
            let c = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => ent
                    .strip_prefix("#x")
                    .or_else(|| ent.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            c.map(|c| (c, e))
        });
        match decoded {
            Some((c, e)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Protects words with inner capitals (`LaTeX`, `DNA`, `iPhone`) with braces, so styles
/// that change the case of titles keep them (like Zotero/Better BibTeX do).
pub fn protect_case(latex: &str) -> String {
    let mut out = String::with_capacity(latex.len() + 8);
    for (i, w) in latex.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let letters: Vec<char> = w.chars().filter(|c| c.is_alphabetic()).collect();
        let inner_cap = letters.iter().skip(1).any(|c| c.is_uppercase());
        if inner_cap && !w.contains(['\\', '{', '}', '$']) {
            // keep punctuation outside the braces: "(DNA)," → "({DNA}),"
            let start = w.find(|c: char| c.is_alphanumeric()).unwrap_or(0);
            let end = w.rfind(|c: char| c.is_alphanumeric()).map_or(w.len(), |e| e + w[e..].chars().next().map_or(1, char::len_utf8));
            out.push_str(&w[..start]);
            out.push('{');
            out.push_str(&w[start..end]);
            out.push('}');
            out.push_str(&w[end..]);
        } else {
            out.push_str(w);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert_eq!(detect("10.1002/jcc.540190402"), Some(Identifier::Doi("10.1002/jcc.540190402".into())));
        assert_eq!(detect(" https://doi.org/10.1007/978-94-017-3427-1_4 "), Some(Identifier::Doi("10.1007/978-94-017-3427-1_4".into())));
        assert_eq!(detect("doi:10.1023/A:1026654312961"), Some(Identifier::Doi("10.1023/a:1026654312961".into())));
        assert_eq!(detect("10.1002/(SICI)1096-987X(199803)19:4<377::AID-JCC1>3.0.CO;2-P"), Some(Identifier::Doi("10.1002/(sici)1096-987x(199803)19:4<377::aid-jcc1>3.0.co;2-p".into())));
        assert_eq!(detect("https://doi.org/10.48550/arXiv.1706.03762"), Some(Identifier::Arxiv("1706.03762".into())));
        assert_eq!(detect("arXiv:1706.03762v5"), Some(Identifier::Arxiv("1706.03762".into())));
        assert_eq!(detect("https://arxiv.org/abs/hep-th/9711200"), Some(Identifier::Arxiv("hep-th/9711200".into())));
        assert_eq!(detect("2101.00001"), Some(Identifier::Arxiv("2101.00001".into())));
        assert_eq!(detect("978-0-201-52983-8"), Some(Identifier::Isbn("9780201529838".into())));
        assert_eq!(detect("ISBN 0-201-52983-1"), Some(Identifier::Isbn("9780201529838".into())));
        assert_eq!(detect("ISBN-10: 080442957X"), Some(Identifier::Isbn("9780804429573".into())));
        assert_eq!(detect("978-0-201-52983-9"), None, "wrong check digit");
        assert_eq!(detect("hello world"), None);
        assert_eq!(detect("1234"), None);
        assert_eq!(detect_all("10.1000/a1\narXiv:2101.00001, 9780201529838\n10.1000/a1"), [Identifier::Doi("10.1000/a1".into()), Identifier::Arxiv("2101.00001".into()), Identifier::Isbn("9780201529838".into())]);
    }

    #[test]
    fn dois_in_text() {
        let t = "See (Smith 2020, https://doi.org/10.1000/xyz123). Also cited: 10.5555/other.\nDOI: 10.1002/(SICI)1096-987X(199803)19:4<377::AID-JCC1>3.0.CO;2-P\n";
        assert_eq!(find_dois(t), ["10.1000/xyz123", "10.1002/(SICI)1096-987X(199803)19:4", "10.5555/other"]);
        assert_eq!(find_doi("version 10.5 / nothing"), None);
        assert_eq!(find_doi("[doi:10.1145/3290605.3300857]"), Some("10.1145/3290605.3300857".into()));
        assert_eq!(find_doi("x210.1000/abc"), None);
    }

    #[test]
    fn markup() {
        assert_eq!(html_to_latex("H<sub>2</sub>O &amp; <i>E. coli</i> at 5%"), r"H\textsubscript{2}O \& \emph{E. coli} at 5\%");
        assert_eq!(html_to_latex("<jats:title>Abstract</jats:title><jats:p>First.</jats:p><jats:p>Second &lt;x&gt;</jats:p>"), "First. Second <x>");
        assert_eq!(html_to_latex("broken <i>open"), r"broken \emph{open}");
        assert_eq!(html_to_latex("a </i> b"), "a b");
        assert_eq!(decode_entities("&#233;t&#xE9; &unknown; & done"), "été &unknown; & done");
        assert_eq!(protect_case("Using LaTeX for DNA (RNA), an iPhone App"), "Using {LaTeX} for {DNA} ({RNA}), an {iPhone} App");
        assert_eq!(protect_case(r"The \emph{TeX} way"), r"The \emph{TeX} way");
    }
}
