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
    bib_setup(&[tex], file).into_iter().map(|(_, at, s)| (at, s)).collect()
}

/// Like [`add_bib_resource`] for a document spread over several files: `files[0]` is the
/// main file, the others are included files (a preamble that loads biblatex is used
/// instead of loading it again). Returns (file index, byte offset, text), offsets
/// ascending per file.
pub fn bib_setup(files: &[&str], file: &str) -> Vec<(usize, usize, String)> {
    let Some(&main) = files.first() else { return Vec::new() };
    let begin = commands(main, "begin").into_iter().find(|(_, a)| *a == "document").map(|(p, _)| p);
    let end = commands(main, "end").into_iter().find(|(_, a)| *a == "document").map(|(p, _)| p);
    let (Some(begin), Some(end)) = (begin, end) else { return Vec::new() };
    let line_after = |tex: &str, p: usize| tex[p..].find('\n').map_or(tex.len(), |n| p + n + 1);
    let biblatex = files.iter().enumerate().find_map(|(i, t)| commands(t, "usepackage").into_iter().find(|(_, a)| a.split(',').any(|p| p.trim() == "biblatex")).map(|(p, _)| (i, line_after(t, p))));
    let bibtex = files.iter().any(|t| !commands(t, "bibliographystyle").is_empty() || t.contains("{natbib}"));
    let mut ins = Vec::new();
    match biblatex {
        Some((i, at)) => ins.push((i, at, format!("\\addbibresource{{{file}}}\n"))),
        None if bibtex => {
            let stem = file.strip_suffix(".bib").unwrap_or(file);
            ins.push((0, end, format!("\\bibliography{{{stem}}}\n")));
            return ins;
        }
        None => ins.push((0, begin, format!("\\usepackage[backend=biber]{{biblatex}}\n\\addbibresource{{{file}}}\n\n"))),
    }
    if !files.iter().any(|t| has_command(t, "printbibliography")) {
        ins.push((0, end, "\\printbibliography\n".into()));
    }
    ins.sort_by_key(|(i, at, _)| (*i, *at));
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

// ------------------------------------------------------------------ sync

/// Keys cited in a LaTeX file (`\cite[…]{a,b}`, `\parencite`, `\cites{a}{b}` … outside
/// comments) or a Markdown file (`[@a; @b]`, `@a`). `\nocite{*}` is skipped.
pub fn cited_keys(text: &str, markdown: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |k: &str| {
        let k = k.trim();
        if !k.is_empty() && k != "*" && !out.iter().any(|o| o.eq_ignore_ascii_case(k)) {
            out.push(k.to_string());
        }
    };
    for line in text.lines() {
        if markdown {
            let b = line.as_bytes();
            for (i, _) in line.match_indices('@') {
                // `name@host` is an e-mail address, not a citation
                if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'.') {
                    continue;
                }
                let key: String = line[i + 1..].chars().take_while(|c| c.is_alphanumeric() || "_:.-/+".contains(*c)).collect();
                push(key.trim_end_matches(['.', ':', '-']));
            }
            continue;
        }
        let code = strip_comment(line);
        let mut i = 0;
        while let Some(p) = code[i..].find('\\') {
            let at = i + p + 1;
            let name: String = code[at..].chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            i = at + name.len().max(1).min(code.len() - at);
            if !name.to_lowercase().contains("cite") {
                continue;
            }
            let mut j = at + name.len();
            if code[j..].starts_with('*') {
                j += 1;
            }
            // optional arguments, then one or more key groups (`\cites{a}[p]{b}`)
            loop {
                let rest = &code[j..];
                let t = rest.len() - rest.trim_start().len();
                j += t;
                if code[j..].starts_with('[') || code[j..].starts_with('(') {
                    let close = if code[j..].starts_with('[') { ']' } else { ')' };
                    match code[j..].find(close) {
                        Some(e) => j += e + 1,
                        None => break,
                    }
                } else if code[j..].starts_with('{') {
                    match code[j..].find('}') {
                        Some(e) => {
                            code[j + 1..j + e].split(',').for_each(&mut push);
                            j += e + 1;
                            if !name.ends_with('s') {
                                break;
                            }
                        }
                        None => break,
                    }
                } else {
                    break;
                }
            }
            i = j.max(i);
        }
    }
    out
}

/// How an entry of the project differs from the library.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncItem {
    /// Same work under the same key, but the fields differ. Texts for a diff.
    Changed { key: String, project: String, library: String },
    /// The key is used for different works in project and library.
    KeyConflict { key: String, project: String, library: String },
    /// Only in the project; can be added to the library.
    ProjectOnly { key: String },
    /// Only in the project under this key; the library has the work as `library_key`.
    SameWork { key: String, library_key: String },
    /// Cited in the document, missing in the project's `.bib`, but in the library.
    Missing { key: String },
}

impl SyncItem {
    pub fn key(&self) -> &str {
        match self {
            SyncItem::Changed { key, .. } | SyncItem::KeyConflict { key, .. } | SyncItem::ProjectOnly { key } | SyncItem::SameWork { key, .. } | SyncItem::Missing { key } => key,
        }
    }
}

/// Fields that matter for a comparison: lower-case names, resolved values with collapsed
/// whitespace, without the library's own fields.
fn comparable(e: &Entry, strings: &bib::Strings) -> (String, std::collections::BTreeMap<String, String>) {
    let fields = e
        .fields
        .iter()
        .filter(|f| !["file", "annotation", ADDED, MODIFIED].iter().any(|x| f.name.eq_ignore_ascii_case(x)))
        .map(|f| (f.name.to_lowercase(), f.value.resolve(strings).split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect();
    (e.kind.to_lowercase(), fields)
}

/// Compares the project's `.bib` with the library. `cited`: keys the document cites;
/// `known`: keys in other `.bib` files of the project (not reported as missing).
pub fn sync(lib: &crate::Library, bib_text: &str, cited: &[String], known: &[String]) -> Vec<SyncItem> {
    let bib = bib::parse(bib_text);
    let strings = bib.strings();
    let mut out = Vec::new();
    for e in bib.entries() {
        let key = e.key.clone();
        let source = bib.source(&key).unwrap_or_default();
        match lib.entry(&key) {
            Some(l) => {
                if comparable(e, &strings) == comparable(l, lib.strings()) {
                    continue;
                }
                let library = bib::format_entry(&export_entry(l)).trim_end().to_string();
                let same = lib.find_duplicate(e).is_some_and(|d| d.eq_ignore_ascii_case(&key)) || {
                    let t = |x: &Entry, s: &bib::Strings| x.text_field("title", s).map(|t| crate::latex::fold_ascii(&t).to_lowercase().split_whitespace().collect::<String>());
                    t(e, &strings).is_some() && t(e, &strings) == t(l, lib.strings())
                };
                out.push(match (same, lib.find_duplicate(e)) {
                    (true, _) => SyncItem::Changed { key, project: source, library },
                    // the project's work is in the library under another key (e.g. after a conflict)
                    (false, Some(other)) => SyncItem::SameWork { key, library_key: other },
                    (false, None) => SyncItem::KeyConflict { key, project: source, library },
                });
            }
            None => match lib.find_duplicate(e) {
                Some(k) => out.push(SyncItem::SameWork { key, library_key: k }),
                None => out.push(SyncItem::ProjectOnly { key }),
            },
        }
    }
    let have: Vec<String> = bib.keys().into_iter().chain(known.iter().cloned()).map(|k| k.to_lowercase()).collect();
    for k in cited {
        if !have.contains(&k.to_lowercase())
            && let Some(l) = lib.entry(k)
        {
            out.push(SyncItem::Missing { key: l.key.clone() });
        }
    }
    out
}

/// The `.bib` text with the entry `key` replaced by `e` (other entries stay byte for byte).
pub fn replace_entry(bib_text: &str, key: &str, e: &Entry) -> Option<String> {
    let mut bib = bib::parse(bib_text);
    let pos = bib.find(key)?;
    bib.replace_entry(pos, export_entry(e));
    Some(bib.to_bib_string())
}

/// The project's version of an entry for the library: its fields, plus what only the
/// library keeps (attachments, notes, timestamps).
pub fn merge_into_library(project: &Entry, library: &Entry) -> Entry {
    let mut e = project.clone();
    for f in ["file", "annotation", ADDED, MODIFIED] {
        if let Some(v) = library.get(f) {
            e.set(f, v.clone());
        }
    }
    e
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

        // biblatex loaded in an included preamble: \addbibresource goes there, not a second \usepackage
        let main = "\\documentclass{book}\n\\input{preamble}\n\\begin{document}\nHi\n\\end{document}\n";
        let pre = "\\usepackage[style=apa]{biblatex}\n\\usepackage{xcolor}\n";
        let ins = bib_setup(&[main, pre], "refs.bib");
        assert_eq!(ins, [(0, main.find("\\end{document}").unwrap(), "\\printbibliography\n".to_string()), (1, pre.find("\\usepackage{xcolor}").unwrap(), "\\addbibresource{refs.bib}\n".to_string())]);
        let chapter_prints = "\\printbibliography[heading=bibintoc]";
        assert_eq!(bib_setup(&[main, pre, chapter_prints], "refs.bib").len(), 1, "\\printbibliography in another file counts");
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

    #[test]
    fn cited_keys_in_documents() {
        let tex = "\\cite{a, b} \\parencite[p.~3][]{c} % \\cite{hidden}\n\\cites[1]{d}[2]{e} \\nocite{*} \\textcite{A}\n\\Citeauthor*{f}";
        assert_eq!(cited_keys(tex, false), ["a", "b", "c", "d", "e", "f"]);
        let md = "As [@knuth; @lamport, p. 3] said, @mueller2020 too. Mail a@b.de.";
        assert_eq!(cited_keys(md, true), ["knuth", "lamport", "mueller2020"]);
    }

    #[test]
    fn sync_cases() {
        let dir = std::env::temp_dir().join(format!("lf-ref-sync-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lib = crate::Library::open(&dir).unwrap();
        let add = |lib: &mut crate::Library, src: &str| {
            let e = bib::parse(src).entries().next().unwrap().clone();
            lib.add_anyway(&mut e.clone())
        };
        add(&mut lib, "@article{knuth84, author={Knuth, Donald}, title={Literate Programming}, journal={Comput. J.}, year=1984, file={files/knuth84/a.pdf}}");
        add(&mut lib, "@book{lamport94, author={Lamport, Leslie}, title={LaTeX}, year=1994}");
        add(&mut lib, "@book{same, author={X, Y}, title={A Book About Things}, year=2000}");
        add(&mut lib, "@misc{onlylib, title={Only in the Library}, year=2020}");
        add(&mut lib, "@misc{cited, title={Cited but Missing}, year=2021}");
        let project = "@article{knuth84,\n  author = {Knuth, Donald},\n  title = {Literate Programming},\n  journal = {The Computer Journal},\n  year = 1984,\n}\n\n@book{lamport94, author={Lamport, Leslie}, title = {LaTeX}, year = {1994}}\n\n@book{lamport94b, author={Other, A}, title={Something Else Entirely}, year=1999}\n\n@book{mine, author={X, Y}, title={A Book About Things}, year=2000}\n\n@misc{onlyproject, title={Only Here}, year=2022}\n";
        // lamport94 differs only in delimiters and the library's private field: in sync
        let items = sync(&lib, project, &["cited".into(), "knuth84".into(), "elsewhere".into()], &["elsewhere".into()]);
        let kinds: Vec<(&str, &str)> = items
            .iter()
            .map(|i| match i {
                SyncItem::Changed { key, .. } => ("changed", key.as_str()),
                SyncItem::KeyConflict { key, .. } => ("conflict", key.as_str()),
                SyncItem::ProjectOnly { key } => ("project", key.as_str()),
                SyncItem::SameWork { key, .. } => ("same", key.as_str()),
                SyncItem::Missing { key } => ("missing", key.as_str()),
            })
            .collect();
        assert_eq!(kinds, [("changed", "knuth84"), ("project", "lamport94b"), ("same", "mine"), ("project", "onlyproject"), ("missing", "cited")]);
        let SyncItem::Changed { library, .. } = &items[0] else { panic!() };
        assert!(library.contains("Comput. J.") && !library.contains("file"));

        // a different work under a library key
        let clash = "@book{lamport94, author={Someone, Else}, title={Unrelated Title}, year=2010}";
        assert!(matches!(sync(&lib, clash, &[], &[]).as_slice(), [SyncItem::KeyConflict { .. }]));
        // once the project's version is in the library under a new key, it is no conflict
        let mut p = bib::parse(clash).entries().next().unwrap().clone();
        let new_key = lib.add_anyway(&mut p);
        assert_eq!(new_key, "lamport94a");
        assert_eq!(sync(&lib, clash, &[], &[]), [SyncItem::SameWork { key: "lamport94".into(), library_key: "lamport94a".into() }]);

        // use the library version: only that entry changes
        let new = replace_entry(project, "knuth84", lib.entry("knuth84").unwrap()).unwrap();
        assert!(new.contains("journal = {Comput. J.}") && !new.contains("files/knuth84"));
        assert!(new.ends_with(&project[project.find("@book{lamport94,").unwrap()..]));
        // update the library from the project: private fields stay
        let p = bib::parse(project).entry("knuth84").unwrap().clone();
        let merged = merge_into_library(&p, lib.entry("knuth84").unwrap());
        assert_eq!(merged.raw_field("journal", &Default::default()).unwrap(), "The Computer Journal");
        assert!(merged.get("file").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
