//! BibTeX/BibLaTeX text (pasted or from a file, e.g. a Zotero or JabRef export) → entries.
//!
//! `@string` macros of the source are resolved, because the library does not have them.
//! Month macros (`jan` … `dec`) stay as they are; every BibTeX style knows them.

use crate::bib::{self, Entry, Piece, Value};

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// Entries of the text and its malformed parts (line, message).
pub fn entries(text: &str) -> (Vec<Entry>, Vec<(usize, String)>) {
    let b = bib::parse(text);
    let strings = b.strings();
    let entries = b
        .entries()
        .map(|e| {
            let mut e = e.clone();
            for f in &mut e.fields {
                let only_month = matches!(f.value.0.as_slice(), [Piece::Macro(m)] if MONTHS.contains(&m.to_ascii_lowercase().as_str()));
                let has_macro = f.value.0.iter().any(|p| matches!(p, Piece::Macro(m) if strings.contains_key(&m.to_ascii_lowercase())));
                if has_macro && !only_month {
                    f.value = Value::text(f.value.resolve(&strings));
                }
            }
            e
        })
        .collect();
    (entries, b.problems())
}
