//! Citation keys.
//!
//! Keys are built from a pattern (default [`DEFAULT_PATTERN`]) with these tokens:
//!
//! | token | meaning | example |
//! |---|---|---|
//! | `[auth]` / `[Auth]` | family name of the first author (else editor), ASCII | `mueller` / `Mueller` |
//! | `[authors2]` | first two family names, `etal` for three or more | `knuthlamport` |
//! | `[year]` | year from `date` or `year` | `2021` |
//! | `[title]` / `[Title]` | first word of the title that is not a stop word | `neuronale` |
//! | `[shorttitle]` | first three such words | `neuronalenetzeeinsteiger` |
//!
//! Everything else in the pattern is copied. Characters that are not allowed in keys are
//! dropped. If the key is taken, `a`, `b`, … `z`, `aa`, … are appended.

use crate::bib::{Entry, Strings};
use crate::latex::fold_ascii;
use crate::names::{Name, parse_names};

pub const DEFAULT_PATTERN: &str = "[auth][year][title]";

/// Words skipped for `[title]` (English, German, French, Spanish articles and prepositions).
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "of", "on", "in", "at", "to", "for", "and", "or", "with", "from", "by", "is", "are", "der", "die", "das", "den", "dem", "des", "ein", "eine", "einer", "eines", "einem", "einen", "und", "oder", "zu", "zur", "zum", "im", "am",
    "von", "vom", "mit", "fur", "fuer", "uber", "ueber", "auf", "le", "la", "les", "un", "une", "des", "du", "de", "et", "el", "los", "las", "y",
];

/// Characters that may appear in a key besides ASCII letters and digits.
fn key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.' | '+' | '/')
}

/// Removes characters that are not allowed in keys.
pub fn sanitize(key: &str) -> String {
    fold_ascii(key).chars().filter(|&c| key_char(c)).collect()
}

/// Year of an entry: the first four-digit number in `date`, else `year`.
pub fn year(e: &Entry, strings: &Strings) -> Option<String> {
    ["date", "year"].iter().find_map(|f| {
        let v = e.raw_field(f, strings)?;
        let b = v.as_bytes();
        (0..b.len().saturating_sub(3)).find(|&i| b[i..i + 4].iter().all(u8::is_ascii_digit)).map(|i| v[i..i + 4].to_string())
    })
}

/// Family names of the authors, or of the editors if there are no authors (plain text).
pub fn families(e: &Entry, strings: &Strings) -> Vec<String> {
    families_of(&creators(e, strings))
}

/// Authors, or editors if there are no authors.
pub fn creators(e: &Entry, strings: &Strings) -> Vec<Name> {
    ["author", "editor"].iter().map(|f| e.raw_field(f, strings).map(|l| parse_names(&l)).unwrap_or_default()).find(|n| !n.is_empty()).unwrap_or_default()
}

pub fn families_of(names: &[Name]) -> Vec<String> {
    names.iter().map(|n| if n.is_others() { "others".to_string() } else { n.family() }).collect()
}

fn word(s: &str) -> String {
    fold_ascii(s).chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

fn title_words(e: &Entry, strings: &Strings) -> Vec<String> {
    let t = e.text_field("title", strings).unwrap_or_default();
    t.split(|c: char| c.is_whitespace() || c == '-' || c == '/').map(word).filter(|w| !w.is_empty() && !STOP_WORDS.contains(&w.as_str())).collect()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// The key for an entry from `pattern`, without checking for collisions.
pub fn base_key(e: &Entry, strings: &Strings, pattern: &str) -> String {
    let fam = families(e, strings);
    let auth = fam.first().map(|f| word(f)).unwrap_or_else(|| "anon".into());
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find(']') else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let tok = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        match tok {
            "auth" => out.push_str(&auth),
            "Auth" => out.push_str(&capitalize(&auth)),
            "authors2" => match fam.len() {
                0 => out.push_str("anon"),
                1 | 2 => fam.iter().for_each(|f| out.push_str(&word(f))),
                _ => {
                    out.push_str(&auth);
                    out.push_str("etal");
                }
            },
            "year" => out.push_str(&year(e, strings).unwrap_or_default()),
            "title" => out.push_str(title_words(e, strings).first().map_or("", |w| w.as_str())),
            "Title" => out.push_str(&capitalize(title_words(e, strings).first().map_or("", |w| w.as_str()))),
            "shorttitle" => out.push_str(&title_words(e, strings).into_iter().take(3).collect::<String>()),
            other => {
                out.push('[');
                out.push_str(other);
                out.push(']');
            }
        }
    }
    out.push_str(rest);
    let k = sanitize(&out);
    if k.is_empty() { "ref".into() } else { k }
}

/// Suffix for the n-th collision: 0 → `a`, 25 → `z`, 26 → `aa`.
fn suffix(mut n: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'a' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// A free key: `base` itself, or `base` with the first free suffix. `taken` compares
/// case-insensitively in the library (biber treats keys that way).
pub fn unique(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    (0..).map(|n| format!("{base}{}", suffix(n))).find(|k| !taken(k)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::Value;

    fn entry(fields: &[(&str, &str)]) -> Entry {
        let mut e = Entry::new("article", "");
        for (k, v) in fields {
            e.set(k, Value::text(*v));
        }
        e
    }

    #[test]
    fn default_pattern() {
        let s = Strings::new();
        let e = entry(&[("author", r#"M{\"u}ller, J{\"u}rgen and Smith, A."#), ("title", "Neuronale Netze für Einsteiger"), ("date", "2021-03")]);
        assert_eq!(base_key(&e, &s, DEFAULT_PATTERN), "mueller2021neuronale");
        let e = entry(&[("author", "van der Waals, J."), ("title", "On the Continuity of the Gaseous and Liquid States"), ("year", "1873")]);
        assert_eq!(base_key(&e, &s, DEFAULT_PATTERN), "vanderwaals1873continuity");
        let e = entry(&[("editor", "{World Health Organization}"), ("title", "The {\\TeX book}")]);
        assert_eq!(base_key(&e, &s, DEFAULT_PATTERN), "worldhealthorganizationtexbook");
        assert_eq!(base_key(&entry(&[]), &s, DEFAULT_PATTERN), "anon");
    }

    #[test]
    fn other_patterns() {
        let s = Strings::new();
        let e = entry(&[("author", "Knuth, Donald and Lamport, Leslie and Others, X"), ("title", "A Short Title of Things Here"), ("year", "1990")]);
        assert_eq!(base_key(&e, &s, "[Auth][year]"), "Knuth1990");
        assert_eq!(base_key(&e, &s, "[authors2]_[year]"), "knuthetal_1990");
        assert_eq!(base_key(&e, &s, "[auth]:[shorttitle]"), "knuth:shorttitlethings");
        assert_eq!(base_key(&e, &s, "x[unknown]y"), "xunknowny");
        let two = entry(&[("author", "Knuth, Donald and Lamport, Leslie")]);
        assert_eq!(base_key(&two, &s, "[authors2]"), "knuthlamport");
    }

    #[test]
    fn collisions() {
        assert_eq!(suffix(0), "a");
        assert_eq!(suffix(25), "z");
        assert_eq!(suffix(26), "aa");
        assert_eq!(suffix(27), "ab");
        let taken = ["knuth1984", "knuth1984a"];
        assert_eq!(unique("knuth1984", |k| taken.iter().any(|t| t.eq_ignore_ascii_case(k))), "knuth1984b");
        assert_eq!(unique("free", |_| false), "free");
        assert_eq!(sanitize("Mü ller{,}2021"), "Mueller2021");
    }
}
