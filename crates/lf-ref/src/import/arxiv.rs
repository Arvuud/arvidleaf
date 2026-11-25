//! arXiv API (Atom) → `@online` entry with eprint fields.
//!
//! Only the few elements arXiv returns are read, with plain string search (no XML library
//! needed). Titles and abstracts on arXiv are TeX-like (`$N$`, `${\cal N}=4$`), so math is
//! kept and only characters that break outside math are escaped.

use super::decode_entities;
use crate::bib::{Entry, Value};
use crate::library::normalize_arxiv;
use crate::names::{format_names, parse_name};

/// Inner text of the first `<tag …>…</tag>` in `s`.
fn element<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let mut from = 0;
    loop {
        let p = from + s[from..].find(&open)?;
        let after = &s[p + open.len()..];
        // `<title>` must not match `<titles>`
        if after.starts_with(['>', ' ', '\n', '\t', '/']) {
            let gt = p + open.len() + after.find('>')?;
            if s[..gt].ends_with('/') {
                return Some("");
            }
            let close = format!("</{tag}>");
            let end = gt + 1 + s[gt + 1..].find(&close)?;
            return Some(&s[gt + 1..end]);
        }
        from = p + open.len();
    }
}

/// Value of an attribute in the first `<tag …>`.
fn attribute(s: &str, tag: &str, attr: &str) -> Option<String> {
    let p = s.find(&format!("<{tag} "))?;
    let end = p + s[p..].find('>')?;
    let t = &s[p..end];
    let key = format!("{attr}=\"");
    let a = t.find(&key)? + key.len();
    let b = a + t[a..].find('"')?;
    Some(decode_entities(&t[a..b]))
}

/// TeX-like text: escape `& % #` everywhere and `_ ^ ~` outside math.
fn tex_text(s: &str) -> String {
    let s = decode_entities(s);
    let mut out = String::with_capacity(s.len());
    let mut math = false;
    let mut prev = ' ';
    for c in s.chars() {
        match c {
            '$' if prev != '\\' => {
                math = !math;
                out.push(c);
            }
            '&' | '%' | '#' if prev != '\\' => {
                out.push('\\');
                out.push(c);
            }
            '_' if !math && prev != '\\' => out.push_str("\\_"),
            '^' if !math => out.push_str("\\textasciicircum{}"),
            '~' if !math => out.push_str("\\textasciitilde{}"),
            c if c.is_whitespace() => {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            c => out.push(c),
        }
        prev = c;
    }
    // unbalanced math (a lone `$`) would break the entry
    if math {
        out = out.replace('$', "\\$");
    }
    out.trim().to_string()
}

/// The first entry of an arXiv API answer. `Ok(None)` if arXiv knows no such paper.
pub fn parse(xml: &str) -> Result<Option<Entry>, String> {
    if !xml.contains("<feed") {
        return Err("not an arXiv API answer".into());
    }
    let Some(entry) = element(xml, "entry") else { return Ok(None) };
    let id_url = element(entry, "id").unwrap_or("");
    if id_url.contains("/api/errors") {
        return Ok(None);
    }
    let id = normalize_arxiv(id_url).ok_or_else(|| format!("no arXiv ID in \"{id_url}\""))?;
    let mut e = Entry::new("online", "");
    let mut names = Vec::new();
    let mut rest = entry;
    while let Some(a) = element(rest, "author") {
        if let Some(n) = element(a, "name") {
            let n = decode_entities(n.trim());
            if !n.is_empty() {
                names.push(parse_name(&crate::latex::encode(&n)));
            }
        }
        let p = rest.find("</author>").map_or(rest.len(), |p| p + "</author>".len());
        rest = &rest[p..];
    }
    if !names.is_empty() {
        e.set("author", Value::text(format_names(&names)));
    }
    if let Some(t) = element(entry, "title") {
        e.set("title", Value::text(super::protect_case(&tex_text(t))));
    }
    if let Some(p) = element(entry, "published") {
        e.set("date", Value::text(p.get(..10).unwrap_or(p)));
    }
    e.set("eprint", Value::text(id.clone()));
    e.set("eprinttype", Value::text("arxiv"));
    if let Some(c) = attribute(entry, "arxiv:primary_category", "term") {
        e.set("eprintclass", Value::text(c));
    }
    e.set("url", Value::text(format!("https://arxiv.org/abs/{id}")));
    if let Some(d) = element(entry, "arxiv:doi") {
        e.set("doi", Value::text(d.trim().to_lowercase()));
    }
    if let Some(j) = element(entry, "arxiv:journal_ref") {
        e.set("note", Value::text(tex_text(j)));
    }
    if let Some(s) = element(entry, "summary") {
        e.set("abstract", Value::text(tex_text(s)));
    }
    Ok(Some(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tex_like_text() {
        assert_eq!(tex_text("large $N$ limit of ${\\cal N} =4$ &amp; 5% R&amp;D a_b"), r"large $N$ limit of ${\cal N} =4$ \& 5\% R\&D a\_b");
        assert_eq!(tex_text("costs $5"), r"costs \$5");
        assert_eq!(tex_text("  two\n   lines "), "two lines");
    }

    #[test]
    fn elements() {
        let x = "<feed><entry><titles>no</titles><title>Yes</title><link href=\"x\"/><arxiv:primary_category term=\"hep-th\"/></entry></feed>";
        assert_eq!(element(x, "title"), Some("Yes"));
        assert_eq!(attribute(x, "arxiv:primary_category", "term").as_deref(), Some("hep-th"));
        assert_eq!(parse("<feed><opensearch:totalResults>0</opensearch:totalResults></feed>"), Ok(None));
        assert!(parse("<html>").is_err());
    }
}
