//! BibLaTeX entry types and their fields (biblatex manual, section 2.1), for the detail
//! editor: required fields are highlighted, common optional ones are offered.

/// Entry types offered in the editor, most common first.
pub const KINDS: &[&str] = &[
    "article", "book", "incollection", "inproceedings", "thesis", "report", "online", "misc", "collection", "proceedings", "inbook", "booklet", "manual", "unpublished",
    "dataset", "software", "patent", "standard",
];

/// Required fields. `a|b` means one of them is enough (`date|year`).
pub fn required(kind: &str) -> &'static [&'static str] {
    match kind.to_ascii_lowercase().as_str() {
        "article" => &["author", "title", "journaltitle|journal", "date|year"],
        "book" | "mvbook" => &["author", "title", "date|year"],
        "inbook" | "bookinbook" | "suppbook" => &["author", "title", "booktitle", "date|year"],
        "collection" | "mvcollection" | "proceedings" | "mvproceedings" | "reference" => &["editor", "title", "date|year"],
        "incollection" | "suppcollection" | "inreference" => &["author", "title", "booktitle", "date|year"],
        "inproceedings" | "conference" => &["author", "title", "booktitle", "date|year"],
        "thesis" | "phdthesis" | "mastersthesis" => &["author", "title", "type", "institution|school", "date|year"],
        "report" | "techreport" => &["author", "title", "type", "institution", "date|year"],
        "online" | "electronic" | "www" => &["author|editor", "title", "date|year|url"],
        "manual" | "booklet" | "misc" | "dataset" | "software" => &["author|editor", "title", "date|year"],
        "patent" => &["author", "title", "number", "date|year"],
        "unpublished" => &["author", "title", "date|year"],
        _ => &["author|editor", "title", "date|year"],
    }
}

/// Optional fields worth offering for a type (besides the required ones).
pub fn optional(kind: &str) -> &'static [&'static str] {
    match kind.to_ascii_lowercase().as_str() {
        "article" => &["subtitle", "volume", "number", "pages", "doi", "url", "eprint", "eprinttype", "shortjournal", "issn", "langid", "abstract"],
        "book" | "mvbook" | "collection" | "mvcollection" => &["editor", "subtitle", "edition", "volume", "series", "publisher", "location", "isbn", "doi", "url", "pagetotal", "langid"],
        "inbook" | "incollection" | "bookinbook" | "inreference" => &["editor", "subtitle", "edition", "volume", "series", "pages", "publisher", "location", "isbn", "doi", "url"],
        "inproceedings" | "proceedings" => &["editor", "eventtitle", "venue", "volume", "series", "pages", "publisher", "location", "isbn", "doi", "url"],
        "thesis" | "phdthesis" | "mastersthesis" => &["subtitle", "location", "doi", "url", "pagetotal"],
        "report" | "techreport" => &["number", "location", "doi", "url", "pagetotal"],
        "online" => &["subtitle", "organization", "urldate", "doi", "eprint", "eprinttype", "version"],
        "software" | "dataset" => &["version", "publisher", "doi", "url", "urldate"],
        _ => &["subtitle", "howpublished", "publisher", "location", "doi", "url", "urldate", "note"],
    }
}

/// Fields shown by the editor in their own place, not in the field list.
pub const SPECIAL: &[&str] = &["keywords", "annotation", "file", crate::library::ADDED, crate::library::MODIFIED];

/// Required groups (`date|year` …) that the entry does not fill.
pub fn missing(e: &crate::Entry) -> Vec<&'static str> {
    required(&e.kind).iter().copied().filter(|group| !group.split('|').any(|f| e.get(f).is_some_and(|v| !v.resolve(&Default::default()).trim().is_empty()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, Value};

    #[test]
    fn missing_fields() {
        let mut e = Entry::new("Article", "k");
        e.set("author", Value::text("Knuth, D."));
        e.set("year", Value::text("1984"));
        e.set("title", Value::text(""));
        assert_eq!(missing(&e), vec!["title", "journaltitle|journal"]);
        e.set("journal", Value::text("CACM"));
        e.set("title", Value::text("Literate Programming"));
        assert!(missing(&e).is_empty());
        assert!(required("unknowntype").contains(&"title"));
    }
}
