//! ISBN → `@book`: OpenLibrary first, then Crossref (which knows many academic books).

use super::ImportError;
use crate::bib::{Entry, Value};
use crate::latex::encode;
use crate::names::{format_names, parse_name};
use crate::net::{self, NetError, NetOptions};
use serde_json::Value as J;
use std::sync::atomic::AtomicBool;

pub fn fetch(isbn: &str, opts: &NetOptions, cancel: &AtomicBool) -> Result<Option<Entry>, ImportError> {
    let ol = net::get(&format!("https://openlibrary.org/api/books?bibkeys=ISBN:{isbn}&format=json&jscmd=data"), Some("application/json"), opts, cancel);
    let first_error = match ol {
        Ok(json) => match from_openlibrary(&json, isbn).map_err(ImportError::Format)? {
            Some(e) => return Ok(Some(e)),
            None => None,
        },
        Err(NetError::Cancelled) => return Err(NetError::Cancelled.into()),
        Err(e) => Some(e),
    };
    let search = format!("https://api.crossref.org/works?filter=isbn:{isbn},type:book,type:monograph,type:edited-book,type:reference-book&rows=1&select=DOI");
    let doi = match net::get(&search, Some("application/json"), opts, cancel) {
        Ok(json) => crossref_book_doi(&json),
        // both sources failed: report the first problem (usually "offline")
        Err(e) => return Err(first_error.unwrap_or(e).into()),
    };
    match doi {
        Some(d) => super::fetch(&super::Identifier::Doi(d), opts, cancel).map(|mut e| {
            if e.get("isbn").is_none() {
                e.set("isbn", Value::text(isbn));
            }
            Some(e)
        }),
        None => match first_error {
            Some(e) => Err(e.into()),
            None => Ok(None),
        },
    }
}

/// DOI of the first book in a Crossref `works` search answer.
pub fn crossref_book_doi(json: &str) -> Option<String> {
    let v: J = serde_json::from_str(json).ok()?;
    Some(v.get("message")?.get("items")?.get(0)?.get("DOI")?.as_str()?.to_lowercase())
}

fn names_of(v: &J, field: &str) -> Vec<String> {
    v.get(field).and_then(J::as_array).map(|a| a.iter().filter_map(|x| x.get("name").and_then(J::as_str)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
}

/// OpenLibrary `api/books?jscmd=data` answer → entry. `Ok(None)` if the ISBN is unknown.
pub fn from_openlibrary(json: &str, isbn: &str) -> Result<Option<Entry>, String> {
    let v: J = serde_json::from_str(json).map_err(|e| format!("not valid JSON: {e}"))?;
    let Some(b) = v.as_object().and_then(|o| o.values().next()) else { return Ok(None) };
    let mut e = Entry::new("book", "");
    let authors: Vec<_> = names_of(b, "authors").iter().map(|n| parse_name(&encode(n))).collect();
    if !authors.is_empty() {
        e.set("author", Value::text(format_names(&authors)));
    }
    if let Some(t) = b.get("title").and_then(J::as_str) {
        e.set("title", Value::text(super::protect_case(&encode(&fix_caps(t)))));
    }
    if let Some(t) = b.get("subtitle").and_then(J::as_str) {
        e.set("subtitle", Value::text(super::protect_case(&encode(&upper_first(t)))));
    }
    if let Some(p) = names_of(b, "publishers").first() {
        e.set("publisher", Value::text(encode(p)));
    }
    if let Some(p) = names_of(b, "publish_places").first() {
        e.set("location", Value::text(encode(p)));
    }
    if let Some(d) = b.get("publish_date").and_then(J::as_str) {
        let y: String = d.split(|c: char| !c.is_ascii_digit()).find(|w| w.len() == 4).unwrap_or("").to_string();
        if !y.is_empty() {
            e.set("date", Value::text(y));
        }
    }
    if let Some(n) = b.get("number_of_pages").and_then(J::as_u64) {
        e.set("pagetotal", Value(vec![crate::bib::Piece::Number(n.to_string())]));
    }
    e.set("isbn", Value::text(isbn));
    Ok(Some(e))
}

/// OpenLibrary sometimes has titles in capitals ("LATEX"): only the first letter stays
/// upper case then; words that are fully upper case and short (acronyms) are kept.
fn fix_caps(t: &str) -> String {
    let letters: Vec<char> = t.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() < 6 || letters.iter().any(|c| c.is_lowercase()) {
        return t.to_string();
    }
    upper_first(&t.to_lowercase())
}

fn upper_first(t: &str) -> String {
    let mut c = t.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps() {
        assert_eq!(fix_caps("LATEX"), "LATEX", "short: probably an acronym or a name");
        assert_eq!(fix_caps("A HISTORY OF MATHEMATICS"), "A history of mathematics");
        assert_eq!(fix_caps("The TeXbook"), "The TeXbook");
        assert_eq!(from_openlibrary("{}", "9780000000002"), Ok(None));
    }
}
