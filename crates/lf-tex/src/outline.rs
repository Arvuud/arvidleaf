//! Document structure: headings, includes, labels and bibliography keys.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    /// 0 = part, 1 = chapter, 2 = section, … 5 = paragraph
    pub level: u8,
    pub title: String,
    /// File relative to the project root.
    pub file: PathBuf,
    /// 1-based line
    pub line: u32,
}

const LEVELS: &[(&str, u8)] = &[
    ("part", 0),
    ("chapter", 1),
    ("section", 2),
    ("subsection", 3),
    ("subsubsection", 4),
    ("paragraph", 5),
    ("frametitle", 3),
];

/// The line with comments removed (an unescaped `%` starts a comment).
pub fn strip_comment(line: &str) -> &str {
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

/// Content of the balanced `{…}` group starting at or after `from` (skipping
/// spaces and one optional `[…]`).
fn brace_arg(s: &str, from: usize) -> Option<(String, usize)> {
    let b = s.as_bytes();
    let mut i = from;
    if i < b.len() && b[i] == b'*' {
        i += 1;
    }
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    if i < b.len() && b[i] == b'[' {
        let mut d = 0;
        while i < b.len() {
            if b[i] == b'[' {
                d += 1;
            } else if b[i] == b']' {
                d -= 1;
                if d == 0 {
                    i += 1;
                    break;
                }
            }
            i += 1;
        }
    }
    while i < b.len() && b[i] == b' ' {
        i += 1;
    }
    if i >= b.len() || b[i] != b'{' {
        return None;
    }
    let start = i + 1;
    let mut d = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'{' => d += 1,
            b'}' => {
                d -= 1;
                if d == 0 {
                    return Some((s[start..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Finds `\name` (not followed by another letter) and returns the byte after the name.
fn find_cmds<'a>(line: &'a str, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    let pat = format!("\\{name}");
    let mut from = 0;
    std::iter::from_fn(move || {
        while let Some(p) = line[from..].find(&pat) {
            let at = from + p + pat.len();
            from = at;
            if !line[at..].starts_with(|c: char| c.is_ascii_alphabetic()) {
                return Some(at);
            }
        }
        None
    })
}

/// Headings of one file: (level, title, line).
pub fn headings(src: &str) -> Vec<(u8, String, u32)> {
    let mut out = Vec::new();
    for (n, raw) in src.lines().enumerate() {
        let line = strip_comment(raw);
        if !line.contains('\\') {
            continue;
        }
        for (name, level) in LEVELS {
            for at in find_cmds(line, name) {
                if let Some((title, _)) = brace_arg(line, at) {
                    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
                    out.push((*level, title, n as u32 + 1));
                }
            }
        }
        // beamer: \begin{frame}{Title}
        for at in find_cmds(line, "begin") {
            if let Some((env, after)) = brace_arg(line, at)
                && env == "frame"
                    && let Some((title, _)) = brace_arg(line, after) {
                        out.push((3, title, n as u32 + 1));
                    }
        }
    }
    out.sort_by_key(|h| h.2);
    out
}

/// Files pulled in by `\input`, `\include`, `\subfile` (as written, without extension handling).
pub fn includes(src: &str) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    for (n, raw) in src.lines().enumerate() {
        let line = strip_comment(raw);
        for name in ["input", "include", "subfile"] {
            for at in find_cmds(line, name) {
                if let Some((f, _)) = brace_arg(line, at) {
                    out.push((f.trim().to_string(), n as u32 + 1));
                }
            }
        }
    }
    out
}

/// `\label{…}` keys.
pub fn labels(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = strip_comment(raw);
        for at in find_cmds(line, "label") {
            if let Some((k, _)) = brace_arg(line, at) {
                out.push(k);
            }
        }
    }
    out
}

/// Entry keys of a `.bib` file (`@article{key,`).
pub fn bib_keys(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    while let Some(p) = src[i..].find('@') {
        let at = i + p + 1;
        let mut j = at;
        while j < b.len() && b[j].is_ascii_alphabetic() {
            j += 1;
        }
        let kind = src[at..j].to_ascii_lowercase();
        while j < b.len() && b[j] == b' ' {
            j += 1;
        }
        if j < b.len() && (b[j] == b'{' || b[j] == b'(') && !matches!(kind.as_str(), "comment" | "string" | "preamble" | "") {
            let ks = j + 1;
            let ke = src[ks..].find([',', '\n', '}']).map_or(src.len(), |e| ks + e);
            let key = src[ks..ke].trim();
            if !key.is_empty() {
                out.push(key.to_string());
            }
            i = ke;
        } else {
            i = j.max(at);
        }
    }
    out
}

/// Resolves an include as TeX would (`chapters/intro` → `chapters/intro.tex`).
pub fn resolve_include(root: &Path, name: &str) -> Option<PathBuf> {
    let p = PathBuf::from(name);
    let candidates = [p.with_extension("tex"), p.clone()];
    let rel = if p.extension().is_some_and(|e| e == "tex") { vec![p] } else { candidates.to_vec() };
    rel.into_iter().find(|r| root.join(r).is_file())
}

/// Outline of the whole project, following includes from the main file.
/// `read` returns the current text of a file (editor buffer or disk).
pub fn project_outline(root: &Path, main: &Path, read: &dyn Fn(&Path) -> Option<String>) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut visited = Vec::new();
    walk(root, main, read, &mut out, &mut visited, 0);
    out
}

fn walk(root: &Path, file: &Path, read: &dyn Fn(&Path) -> Option<String>, out: &mut Vec<Heading>, visited: &mut Vec<PathBuf>, depth: usize) {
    if depth > 16 || visited.iter().any(|v| v == file) {
        return;
    }
    visited.push(file.to_path_buf());
    let Some(src) = read(file) else { return };
    let hs = headings(&src);
    let inc = includes(&src);
    // merge headings and includes by line
    let (mut a, mut b) = (0, 0);
    while a < hs.len() || b < inc.len() {
        if b >= inc.len() || (a < hs.len() && hs[a].2 <= inc[b].1) {
            let (level, title, line) = hs[a].clone();
            out.push(Heading { level, title, file: file.to_path_buf(), line });
            a += 1;
        } else {
            if let Some(p) = resolve_include(root, &inc[b].0) {
                walk(root, &p, read, out, visited, depth + 1);
            }
            b += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_headings() {
        let src = "\\chapter{Intro}\n% \\section{Hidden}\n\\section*[short]{Long \\emph{title}}\n\\subsection{A}\\label{sec:a}\n\\sectionmark{x}\n";
        let h = headings(src);
        assert_eq!(h, vec![(1, "Intro".into(), 1), (2, "Long \\emph{title}".into(), 3), (3, "A".into(), 4)]);
        assert_eq!(labels(src), vec!["sec:a"]);
    }

    #[test]
    fn bib_and_includes() {
        assert_eq!(bib_keys("@article{knuth84,\n title={x}}\n@comment{no}\n@Book{lamport, }"), vec!["knuth84", "lamport"]);
        assert_eq!(includes("\\input{a}\n%\\input{b}\n\\include{ch/c}"), vec![("a".to_string(), 1), ("ch/c".to_string(), 3)]);
    }

    #[test]
    fn escaped_percent_is_not_comment() {
        assert_eq!(strip_comment("50\\% done % rest"), "50\\% done ");
    }
}
