//! The bibliography of a project: which `.bib` files a document uses, how to add one,
//! copying library entries into it, and `\cite` commands.
//!
//! Functions return text to insert (byte offset, text) instead of whole new files, so the
//! editor can apply them to an open buffer without moving the cursor.

use crate::bib::{self, Entry};
use crate::library::{ADDED, MODIFIED};

/// Commands offered by the cite dialog.
pub const CITE_COMMANDS: &[&str] = &["cite", "parencite", "textcite", "autocite", "footcite", "citep", "citet"];

/// A line without its `%` comment (`\%` is kept).
fn strip_comment(line: &str) -> &str {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'%' => return &line[..i],
            _ => i += 1,
        }
    }
    line
}

/// `\name[…]{arg}` occurrences outside comments: (byte offset of the backslash, argument).
fn commands<'a>(text: &'a str, name: &str) -> Vec<(usize, &'a str)> {
    let mut out = Vec::new();
    let mut off = 0;
    for line in text.split_inclusive('\n') {
        let code = strip_comment(line);
        let mut i = 0;
        while let Some(p) = code[i..].find('\\') {
            let at = i + p;
            i = at + 1;
            let rest = &code[at + 1..];
            if !rest.starts_with(name) || rest[name.len()..].starts_with(|c: char| c.is_ascii_alphabetic()) {
                continue;
            }
            let mut j = at + 1 + name.len();
            while code[j..].starts_with('[') {
                match code[j..].find(']') {
                    Some(e) => j += e + 1,
                    None => break,
                }
            }
            let j = j + (code[j..].len() - code[j..].trim_start().len());
            if !code[j..].starts_with('{') {
                continue;
            }
            if let Some(e) = code[j..].find('}') {
                out.push((off + at, &code[j + 1..j + e]));
            }
        }
        off += line.len();
    }
    out
}

/// Whether `\name` occurs outside comments.
fn has_command(text: &str, name: &str) -> bool {
    text.lines().any(|l| strip_comment(l).match_indices(&format!("\\{name}")).any(|(i, m)| !l[i + m.len()..].starts_with(|c: char| c.is_ascii_alphabetic())))
}

/// `.bib` files of a LaTeX document: `\addbibresource{x.bib}`, `\addglobalbib{…}`,
/// `\bibliography{a,b}` (`.bib` added when missing). Paths as written.
pub fn bib_resources(tex: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in ["addbibresource", "addglobalbib"] {
        out.extend(commands(tex, name).into_iter().map(|(_, a)| a.trim().to_string()));
    }
    for (_, arg) in commands(tex, "bibliography") {
        for f in arg.split(',').map(str::trim).filter(|f| !f.is_empty()) {
            out.push(if f.ends_with(".bib") { f.to_string() } else { format!("{f}.bib") });
        }
    }
    out.retain(|f| !f.is_empty());
    out.dedup();
    out
}

/// Insertions that make a LaTeX document use `file` (a `.bib` path relative to the
/// document): with biblatex an `\addbibresource`, with BibTeX a `\bibliography`, otherwise
/// biblatex (biber) is set up. `\printbibliography` is added when missing. Empty if the
/// text has no `\begin{document}` (not a main file). Offsets ascend.
pub fn add_bib_resource(tex: &str, file: &str) -> Vec<(usize, String)> {
    let begin = commands(tex, "begin").into_iter().find(|(_, a)| *a == "document").map(|(p, _)| p);
    let end = commands(tex, "end").into_iter().find(|(_, a)| *a == "document").map(|(p, _)| p);
    let (Some(begin), Some(end)) = (begin, end) else { return Vec::new() };
    let line_after = |p: usize| tex[p..].find('\n').map_or(tex.len(), |n| p + n + 1);
    let biblatex = commands(tex, "usepackage").into_iter().find(|(_, a)| a.split(',').any(|p| p.trim() == "biblatex"));
    let bibtex = !commands(tex, "bibliographystyle").is_empty() || tex.contains("{natbib}");
    let mut ins = Vec::new();
    match biblatex {
        Some((p, _)) => ins.push((line_after(p), format!("\\addbibresource{{{file}}}\n"))),
        None if bibtex => {
            let stem = file.strip_suffix(".bib").unwrap_or(file);
            ins.push((end, format!("\\bibliography{{{stem}}}\n")));
            return ins;
        }
        None => ins.push((begin, format!("\\usepackage[backend=biber]{{biblatex}}\n\\addbibresource{{{file}}}\n\n"))),
    }
    if !has_command(tex, "printbibliography") {
        ins.push((end, "\\printbibliography\n".into()));
    }
    ins
}

/// A library entry as it goes into a project: without local attachment paths, private
/// notes and arvidref timestamps.
pub fn export_entry(e: &Entry) -> Entry {
    let mut e = e.clone();
    for f in ["file", "annotation", ADDED, MODIFIED] {
        e.remove(f);
    }
    e
}

/// Text to append to a `.bib` file for the entries whose keys it does not have yet, and
/// those keys. Keys are compared case-insensitively, as biber does.
pub fn entries_to_append(bib_text: &str, entries: &[Entry]) -> (String, Vec<String>) {
    let mut have: Vec<String> = bib::parse(bib_text).keys().into_iter().map(|k| k.to_lowercase()).collect();
    let mut out = String::new();
    let mut keys = Vec::new();
    for e in entries {
        if have.contains(&e.key.to_lowercase()) {
            continue;
        }
        have.push(e.key.to_lowercase());
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(bib::format_entry(&export_entry(e)).trim_end());
        out.push('\n');
        keys.push(e.key.clone());
    }
    if out.is_empty() {
        return (out, keys);
    }
    let sep = if bib_text.is_empty() || bib_text.ends_with("\n\n") {
        ""
    } else if bib_text.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    (format!("{sep}{out}"), keys)
}

/// `\cmd{a,b}`, or `[@a; @b]` in Markdown.
pub fn cite_text(cmd: &str, keys: &[String], markdown: bool) -> String {
    if markdown {
        format!("[{}]", keys.iter().map(|k| format!("@{k}")).collect::<Vec<_>>().join("; "))
    } else {
        format!("\\{cmd}{{{}}}", keys.join(","))
    }
}

/// A citation the cursor is in: where new keys go and which keys it has.
#[derive(Debug, PartialEq)]
pub struct CiteSpan {
    /// Byte offset of the closing `}` or `]`.
    pub close: usize,
    pub keys: Vec<String>,
    pub markdown: bool,
}

impl CiteSpan {
    /// Text to insert at `close` for the keys not cited yet (empty if none).
    pub fn addition(&self, keys: &[String]) -> String {
        let new: Vec<&String> = keys.iter().filter(|k| !self.keys.iter().any(|x| x.eq_ignore_ascii_case(k))).collect();
        if new.is_empty() {
            return String::new();
        }
        let sep = if self.keys.is_empty() { "" } else if self.markdown { "; " } else { "," };
        let list: Vec<String> = new.iter().map(|k| if self.markdown { format!("@{k}") } else { k.to_string() }).collect();
        format!("{sep}{}", list.join(if self.markdown { "; " } else { "," }))
    }
}

/// The `\…cite…{…}` key list or Markdown `[@…]` around byte offset `at` (same line).
pub fn cite_at(text: &str, at: usize) -> Option<CiteSpan> {
    let ls = text[..at].rfind('\n').map_or(0, |p| p + 1);
    let le = text[at..].find('\n').map_or(text.len(), |p| at + p);
    let line = &text[ls..le];
    let pos = at - ls;
    // LaTeX: the nearest `{` before the cursor belongs to a cite command
    if let Some(open) = line[..pos].rfind('{')
        && !line[open..pos].contains('}')
        && let Some(close) = line[pos..].find('}').map(|c| pos + c)
        && !line[open + 1..close].contains('{')
    {
        let head = line[..open].trim_end_matches(|c: char| c != '\\');
        let cmd_start = head.len().checked_sub(1)?;
        let mut cmd = &line[cmd_start + 1..open];
        // optional arguments: `\cite[p. 3]{`
        if let Some(b) = cmd.find(['[', '*']) {
            cmd = &cmd[..b];
        }
        if cmd.chars().all(|c| c.is_ascii_alphabetic()) && cmd.to_lowercase().contains("cite") {
            let keys = line[open + 1..close].split(',').map(str::trim).filter(|k| !k.is_empty()).map(String::from).collect();
            return Some(CiteSpan { close: ls + close, keys, markdown: false });
        }
    }
    // Markdown: `[@a; @b]`
    if let Some(open) = line[..pos].rfind('[')
        && !line[open..pos].contains(']')
        && let Some(close) = line[pos..].find(']').map(|c| pos + c)
        && line[open + 1..].trim_start().starts_with('@')
    {
        let keys = line[open + 1..close]
            .split(';')
            .filter_map(|p| p.trim().strip_prefix('@'))
            .map(|k| k.split([',', ' ']).next().unwrap_or("").to_string())
            .filter(|k| !k.is_empty())
            .collect();
        return Some(CiteSpan { close: ls + close, keys, markdown: true });
    }
    None
}

/// The key of an undefined-citation warning (LaTeX, natbib, biblatex), e.g.
/// ``Citation `knuth' on page 1 undefined on input line 5.``
pub fn undefined_citation(message: &str) -> Option<String> {
    let p = message.find("Citation")?;
    let rest = message[p + 8..].trim_start();
    let rest = rest.strip_prefix('`').or_else(|| rest.strip_prefix('\'')).or_else(|| rest.strip_prefix('"'))?;
    let end = rest.find(['\'', '"', '`'])?;
    let key = &rest[..end];
    (message[p..].contains("undefined") && !key.is_empty()).then(|| key.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;

    const DOC: &str = "\\documentclass{article}\n\\usepackage[style=authoryear]{biblatex}\n% \\addbibresource{old.bib}\n\\addbibresource{refs.bib}\n\\begin{document}\nText \\cite[p.~3]{knuth, lamport} and \\parencite{x}.\n\\printbibliography\n\\end{document}\n";

    fn apply(text: &str, ins: &[(usize, String)]) -> String {
        let mut t = text.to_string();
        for (at, s) in ins.iter().rev() {
            t.insert_str(*at, s);
        }
        t
    }

    #[test]
    fn resources() {
        assert_eq!(bib_resources(DOC), ["refs.bib"]);
        assert_eq!(bib_resources("\\bibliographystyle{plain}\n\\bibliography{a, lit/b.bib}"), ["a.bib", "lit/b.bib"]);
        assert_eq!(bib_resources("\\addbibresource[location=remote]{x.bib} \\bibliographystyle{x}"), ["x.bib"]);
        assert!(bib_resources("no bibliography % \\bibliography{x}").is_empty());
    }

    #[test]
    fn setting_up_a_bibliography() {
        let plain = "\\documentclass{article}\n\\begin{document}\nHi\n\\end{document}\n";
        let t = apply(plain, &add_bib_resource(plain, "refs.bib"));
        assert_eq!(t, "\\documentclass{article}\n\\usepackage[backend=biber]{biblatex}\n\\addbibresource{refs.bib}\n\n\\begin{document}\nHi\n\\printbibliography\n\\end{document}\n");
        assert_eq!(bib_resources(&t), ["refs.bib"]);

        let bl = "\\documentclass{article}\n\\usepackage{biblatex}\n\\begin{document}\n\\printbibliography\n\\end{document}";
        assert_eq!(apply(bl, &add_bib_resource(bl, "refs.bib")), "\\documentclass{article}\n\\usepackage{biblatex}\n\\addbibresource{refs.bib}\n\\begin{document}\n\\printbibliography\n\\end{document}");

        let bt = "\\documentclass{article}\n\\begin{document}\n\\bibliographystyle{plain}\n\\end{document}\n";
        assert!(apply(bt, &add_bib_resource(bt, "refs.bib")).contains("\\bibliography{refs}\n\\end{document}"));
        assert!(add_bib_resource("\\section{Only a chapter}", "refs.bib").is_empty());
    }

    #[test]
    fn appending_entries() {
        let mut a = Entry::new("article", "knuth84");
        a.set("title", Value::text("Literate Programming"));
        a.set("file", Value::text("files/knuth84/x.pdf"));
        a.set("annotation", Value::text("private"));
        a.set(ADDED, Value::text("2026"));
        let b = Entry::new("book", "Lamport94");
        let existing = "@book{lamport94,\n  title = {LaTeX},\n}\n";
        let (add, keys) = entries_to_append(existing, &[a.clone(), b]);
        assert_eq!(keys, ["knuth84"], "lamport94 is already there (case-insensitive)");
        assert_eq!(add, "\n@article{knuth84,\n  title = {Literate Programming},\n}\n");
        let joined = format!("{existing}{add}");
        assert_eq!(bib::parse(&joined).keys(), ["lamport94", "knuth84"]);
        assert_eq!(entries_to_append("", std::slice::from_ref(&a)).0, "@article{knuth84,\n  title = {Literate Programming},\n}\n");
        assert_eq!(entries_to_append("x}", &[a]).0.chars().take(3).collect::<String>(), "\n\n@");
    }

    #[test]
    fn citations_at_the_cursor() {
        let at = DOC.find("lamport").unwrap();
        let c = cite_at(DOC, at).unwrap();
        assert_eq!(c.keys, ["knuth", "lamport"]);
        assert_eq!(&DOC[c.close..c.close + 1], "}");
        assert_eq!(c.addition(&["Knuth".into(), "new".into()]), ",new");
        assert!(cite_at(DOC, DOC.find("Text").unwrap()).is_none());
        let p = DOC.find("parencite{x").unwrap() + 10;
        assert_eq!(cite_at(DOC, p).unwrap().keys, ["x"]);
        assert!(cite_at("\\textbf{bold}", 9).is_none());

        let md = "As shown [@knuth; @lamport, p. 3] here.";
        let c = cite_at(md, md.find("@knuth").unwrap() + 2).unwrap();
        assert!(c.markdown);
        assert_eq!(c.keys, ["knuth", "lamport"]);
        assert_eq!(c.addition(&["x".into()]), "; @x");
        assert_eq!(cite_text("parencite", &["a".into(), "b".into()], false), "\\parencite{a,b}");
        assert_eq!(cite_text("cite", &["a".into(), "b".into()], true), "[@a; @b]");
        assert_eq!(cite_at("\\cite{}", 6).unwrap().addition(&["k".into()]), "k");
    }

    #[test]
    fn undefined_citation_warnings() {
        assert_eq!(undefined_citation("Citation `knuth1984' on page 1 undefined on input line 5."), Some("knuth1984".into()));
        assert_eq!(undefined_citation("Package natbib Warning: Citation `a:b' on page 2 undefined"), Some("a:b".into()));
        assert_eq!(undefined_citation("LaTeX Warning: Citation 'mueller2020' on page 1 undefined on input line 9."), Some("mueller2020".into()));
        assert_eq!(undefined_citation("There were undefined references."), None);
        assert_eq!(undefined_citation("Reference `fig:x' on page 1 undefined"), None);
    }
}
