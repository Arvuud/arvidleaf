//! BibTeX name lists: `author = {Knuth, Donald E. and van der Waals, Johannes and others}`.
//!
//! Each name has the parts *First*, *von*, *Last* and *Jr*, written as
//! `First von Last`, `von Last, First` or `von Last, Jr, First`. Parts are kept as LaTeX
//! source; use [`crate::latex::decode`] for display. Rules as in BibTeX:
//! - `and` separates names only at brace depth 0 (`{Barnes and Noble}` is one name).
//! - *von* words start with a lower-case letter; `{van}` in braces does not count,
//!   but `{\"u}ber` does (special characters keep their case).
//! - A name in braces (`{World Health Organization}`) is a single last name.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Name {
    pub first: String,
    pub von: String,
    pub last: String,
    pub jr: String,
}

impl Name {
    /// `and others` at the end of a list.
    pub fn is_others(&self) -> bool {
        self.first.is_empty() && self.von.is_empty() && self.jr.is_empty() && self.last == "others"
    }

    /// `von Last, Jr, First` – the unambiguous BibTeX form.
    pub fn to_bib(&self) -> String {
        let mut s = String::new();
        if !self.von.is_empty() {
            s.push_str(&self.von);
            s.push(' ');
        }
        s.push_str(&self.last);
        if !self.jr.is_empty() {
            s.push_str(", ");
            s.push_str(&self.jr);
        }
        if !self.first.is_empty() || !self.jr.is_empty() {
            s.push_str(", ");
            s.push_str(&self.first);
        }
        s
    }

    /// Display form `First von Last, Jr` as plain text.
    pub fn display(&self) -> String {
        let parts = [&self.first, &self.von, &self.last].into_iter().filter(|p| !p.is_empty()).map(|p| crate::latex::decode(p)).collect::<Vec<_>>();
        let mut s = parts.join(" ");
        if !self.jr.is_empty() {
            s.push_str(", ");
            s.push_str(&crate::latex::decode(&self.jr));
        }
        s
    }

    /// `von Last` as plain text (for lists and citation keys).
    pub fn family(&self) -> String {
        let s = if self.von.is_empty() { self.last.clone() } else { format!("{} {}", self.von, self.last) };
        crate::latex::decode(&s)
    }
}

/// Splits a name list at ` and ` (case-insensitive, brace depth 0) and parses each name.
pub fn parse_names(list: &str) -> Vec<Name> {
    split_and(list).into_iter().map(|n| parse_name(&n)).collect()
}

/// Formats names as a BibTeX list (`A and B and others`).
pub fn format_names(names: &[Name]) -> String {
    names.iter().map(Name::to_bib).collect::<Vec<_>>().join(" and ")
}

fn split_and(list: &str) -> Vec<String> {
    let words = words(list, true);
    let mut out = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for w in &words {
        if w.eq_ignore_ascii_case("and") {
            if !cur.is_empty() {
                out.push(cur.join(" "));
            }
            cur.clear();
        } else {
            cur.push(w);
        }
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    out
}

/// Splits at whitespace (and `~` if `tilde`) on brace depth 0.
fn words(s: &str, keep_tilde: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '{' => {
                depth += 1;
                cur.push(ch);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                cur.push(ch);
            }
            c if depth == 0 && (c.is_whitespace() || (!keep_tilde && c == '~')) => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Splits at commas on brace depth 0.
fn comma_parts(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    out.push(cur);
    out.into_iter().map(|p| p.trim().to_string()).collect()
}

/// Case of a word for the *von* rule: `Some(true)` = lower case.
fn lower_case(word: &str) -> Option<bool> {
    let c: Vec<char> = word.chars().collect();
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            '{' => {
                // special character `{\"u}` keeps its case; any other group is caseless
                if c.get(i + 1) != Some(&'\\') {
                    return None;
                }
                let mut j = i + 2;
                // skip the command name (`\"`, `\v`, `\ss` …): a symbol command, or the
                // letters of a word command that is an accent
                if c.get(j).is_some_and(|ch| !ch.is_ascii_alphabetic()) {
                    j += 1;
                } else {
                    let s = j;
                    while j < c.len() && c[j].is_ascii_alphabetic() {
                        j += 1;
                    }
                    let name: String = c[s..j].iter().collect();
                    if !matches!(name.as_str(), "u" | "v" | "H" | "c" | "k" | "r" | "d" | "b" | "t") {
                        // a symbol such as \ss, \o, \AA: its own case
                        return name.chars().next().map(|ch| ch.is_lowercase());
                    }
                }
                return c[j..].iter().find(|ch| ch.is_alphabetic()).map(|ch| ch.is_lowercase());
            }
            ch if ch.is_alphabetic() => return Some(ch.is_lowercase()),
            _ => i += 1,
        }
    }
    None
}

pub fn parse_name(s: &str) -> Name {
    let parts = comma_parts(s);
    match parts.len() {
        1 => {
            let w = words(&parts[0], false);
            if w.is_empty() {
                return Name::default();
            }
            if w.len() == 1 {
                return Name { last: w[0].clone(), ..Name::default() };
            }
            // von = from the first lower-case word to the last lower-case word before Last
            let n = w.len();
            let start = (0..n - 1).find(|&i| lower_case(&w[i]) == Some(true));
            match start {
                Some(s) => {
                    let end = (s..n - 1).rev().find(|&i| lower_case(&w[i]) == Some(true)).unwrap_or(s);
                    Name { first: w[..s].join(" "), von: w[s..=end].join(" "), last: w[end + 1..].join(" "), jr: String::new() }
                }
                None => Name { first: w[..n - 1].join(" "), von: String::new(), last: w[n - 1].clone(), jr: String::new() },
            }
        }
        k => {
            let (von, last) = von_last(&parts[0]);
            let (jr, first) = if k == 2 { (String::new(), parts[1].clone()) } else { (parts[1].clone(), parts[2..].join(", ")) };
            Name { first, von, last, jr }
        }
    }
}

/// `von Last` part before the first comma: leading lower-case words are *von*, but at least
/// the last word stays the last name.
fn von_last(s: &str) -> (String, String) {
    let w = words(s, false);
    if w.is_empty() {
        return (String::new(), String::new());
    }
    let n = w.len();
    let end = (0..n - 1).rev().find(|&i| lower_case(&w[i]) == Some(true));
    match end {
        Some(e) => (w[..=e].join(" "), w[e + 1..].join(" ")),
        None => (String::new(), w.join(" ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(first: &str, von: &str, last: &str, jr: &str) -> Name {
        Name { first: first.into(), von: von.into(), last: last.into(), jr: jr.into() }
    }

    #[test]
    fn three_forms() {
        assert_eq!(parse_name("Donald E. Knuth"), n("Donald E.", "", "Knuth", ""));
        assert_eq!(parse_name("Knuth, Donald E."), n("Donald E.", "", "Knuth", ""));
        assert_eq!(parse_name("Johannes Diderik van der Waals"), n("Johannes Diderik", "van der", "Waals", ""));
        assert_eq!(parse_name("van der Waals, Johannes Diderik"), n("Johannes Diderik", "van der", "Waals", ""));
        assert_eq!(parse_name("Ford, Jr., Henry"), n("Henry", "", "Ford", "Jr."));
        assert_eq!(parse_name("de la Fontaine, Jr., Jean"), n("Jean", "de la", "Fontaine", "Jr."));
        assert_eq!(parse_name("Plato"), n("", "", "Plato", ""));
        assert_eq!(parse_name("Ludwig van Beethoven"), n("Ludwig", "van", "Beethoven", ""));
    }

    #[test]
    fn braces_and_special_characters() {
        assert_eq!(parse_name("{World Health Organization}"), n("", "", "{World Health Organization}", ""));
        assert_eq!(parse_name("{van} Gogh, Vincent"), n("Vincent", "", "{van} Gogh", ""));
        assert_eq!(parse_name(r#"{\"O}zkal, Erhan"#), n("Erhan", "", r#"{\"O}zkal"#, ""));
        assert_eq!(parse_name(r#"Jean {\"u}ber Dupont"#), n("Jean", r#"{\"u}ber"#, "Dupont", ""));
        assert_eq!(parse_name("Charles Louis Xavier Joseph de la Vall{\\'e}e Poussin"), n("Charles Louis Xavier Joseph", "de la", "Vall{\\'e}e Poussin", ""));
    }

    #[test]
    fn lists() {
        let l = parse_names("Sigfridsson, Emma and Ryde, Ulf");
        assert_eq!(l.len(), 2);
        assert_eq!(l[1].last, "Ryde");
        let l = parse_names("{Barnes and Noble} AND Smith, J.\n  and others");
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].last, "{Barnes and Noble}");
        assert!(l[2].is_others());
        assert!(parse_names("   ").is_empty());
        assert_eq!(parse_names("Alexander Anderson")[0].last, "Anderson"); // "and" inside a word
    }

    #[test]
    fn display_and_round_trip() {
        let l = parse_names(r#"van der Waals, Jr., Johannes and Aks{\i}n, {\"O}zge"#);
        assert_eq!(l[0].display(), "Johannes van der Waals, Jr.");
        assert_eq!(l[0].family(), "van der Waals");
        assert_eq!(l[1].display(), "Özge Aksın");
        let s = format_names(&l);
        assert_eq!(s, r#"van der Waals, Jr., Johannes and Aks{\i}n, {\"O}zge"#);
        assert_eq!(parse_names(&s), l);
    }
}
