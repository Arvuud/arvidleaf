//! Parser for TeX `.log` files (errors, warnings, bad boxes).
//!
//! Expects logs written with `-file-line-error` and a large `max_print_line`
//! (the compile runner sets both), but also understands the classic `! …`
//! error format.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Error,
    Warning,
    BadBox,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub level: Level,
    /// File as written in the log, `./` stripped.
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
    /// Extra lines (TeX context, `l.42 …`).
    pub context: String,
}

const FILE_EXTS: &[&str] = &[
    "tex", "sty", "cls", "bib", "bbl", "aux", "toc", "lof", "lot", "out", "def", "cfg", "clo", "fd", "ldf", "ltx", "dtx", "tikz", "pgf", "code.tex", "fdd", "lbx",
    "bbx", "cbx", "dict", "map", "enc", "pfb", "nav", "snm", "vrb", "lua", "otf", "ttf", "tfm", "ist", "ind", "gls", "glo", "acn", "xmpi", "mkii", "mkiv",
];

fn looks_like_file(tok: &str) -> bool {
    if tok.is_empty() {
        return false;
    }
    let name = tok.rsplit('/').next().unwrap_or(tok);
    let Some((_, ext)) = name.rsplit_once('.') else { return false };
    (tok.starts_with("./") || tok.starts_with('/') || tok.starts_with("../") || tok.chars().next().is_some_and(|c| c.is_alphanumeric()))
        && FILE_EXTS.iter().any(|e| e.eq_ignore_ascii_case(ext) || e.ends_with(ext))
}

fn clean_file(f: &str) -> String {
    f.strip_prefix("./").unwrap_or(f).to_string()
}

/// Tracks which file TeX is reading from the `(file … )` nesting in the log.
#[derive(Default)]
struct FileStack {
    stack: Vec<Option<String>>,
}

impl FileStack {
    fn scan(&mut self, line: &str) {
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'(' => {
                    let mut j = i + 1;
                    while j < b.len() && !matches!(b[j], b' ' | b'(' | b')' | b'\t' | b'"' | b'{' | b'[' | b'<') {
                        j += 1;
                    }
                    let tok = &line[i + 1..j];
                    self.stack.push(if looks_like_file(tok) { Some(clean_file(tok)) } else { None });
                    i = j;
                }
                b')' => {
                    self.stack.pop();
                    i += 1;
                }
                _ => i += 1,
            }
        }
    }

    fn current(&self) -> Option<String> {
        self.stack.iter().rev().flatten().next().cloned()
    }
}

/// `./main.tex:42: message` → (file, line, message)
fn split_file_line(line: &str) -> Option<(String, u32, String)> {
    let mut search = 0;
    while let Some(p) = line[search..].find(':') {
        let p = search + p;
        let rest = &line[p + 1..];
        let digits = rest.bytes().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && rest[digits..].starts_with(": ") {
            let file = &line[..p];
            if looks_like_file(file) {
                let n = rest[..digits].parse().ok()?;
                return Some((clean_file(file), n, rest[digits + 2..].to_string()));
            }
        }
        search = p + 1;
    }
    None
}

fn input_line(msg: &str) -> Option<u32> {
    for pat in ["on input line ", "at lines ", "at line ", "input line "] {
        if let Some(p) = msg.find(pat) {
            let n: String = msg[p + pat.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(v) = n.parse() {
                return Some(v);
            }
        }
    }
    None
}

fn is_warning_start(line: &str) -> bool {
    (line.starts_with("LaTeX ") || line.starts_with("Package ") || line.starts_with("Class ") || line.starts_with("pdfTeX warning") || line.contains(" Warning: "))
        && (line.contains("Warning:") || line.starts_with("pdfTeX warning"))
}

/// Collects the context after an error: up to the `l.N` line and the line after it.
fn error_context(lines: &[&str], start: usize) -> (String, Option<u32>, usize) {
    let mut ctx = Vec::new();
    let mut line_no = None;
    let mut k = start;
    while k < lines.len() && k < start + 12 {
        let l = lines[k];
        if let Some(rest) = l.strip_prefix("l.") {
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            line_no = n.parse().ok();
            ctx.push(l.to_string());
            if k + 1 < lines.len() && !lines[k + 1].trim().is_empty() {
                ctx.push(lines[k + 1].to_string());
                k += 1;
            }
            k += 1;
            break;
        }
        if l.trim().is_empty()
            || l.starts_with('!')
            || l.starts_with("Type X")
            || l.starts_with("Enter file name")
            || l.starts_with("Overfull \\")
            || l.starts_with("Underfull \\")
            || is_warning_start(l)
            || split_file_line(l).is_some()
        {
            break;
        }
        if !l.trim().is_empty() {
            ctx.push(l.to_string());
        }
        k += 1;
    }
    (ctx.join("\n"), line_no, k)
}

/// Continuation lines of a multi-line message: `(pkgname)  more text` or
/// deeply indented text. Returns the joined message and the next line index.
fn continued_message(lines: &[&str], first: &str, start: usize) -> (String, usize) {
    let mut msg = first.to_string();
    let mut k = start;
    while k < lines.len() {
        let l = lines[k];
        let t = l.trim_start();
        if t.is_empty() {
            break;
        }
        let cont = (t.starts_with('(') && t.find(')').is_some_and(|p| !t[1..p].contains(' ') && !looks_like_file(&t[1..p]))) || l.starts_with("               ");
        if !cont {
            break;
        }
        let body = if t.starts_with('(') { t[t.find(')').unwrap() + 1..].trim() } else { t };
        msg.push(' ');
        msg.push_str(body);
        k += 1;
    }
    (msg.split_whitespace().collect::<Vec<_>>().join(" "), k)
}

/// Follow-up messages TeX prints after the real error.
fn is_followup(msg: &str) -> bool {
    msg.starts_with("Emergency stop") || msg.starts_with("==> Fatal error occurred")
}

/// Parses a TeX log into entries (errors first in log order, then the rest).
pub fn parse(log: &str) -> Vec<LogEntry> {
    let lines: Vec<&str> = log.lines().collect();
    let mut files = FileStack::default();
    let mut out: Vec<LogEntry> = Vec::new();
    // "Runaway argument?" + the text TeX was reading, shown with the next error
    let mut runaway: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("Runaway ") && line.ends_with('?') {
            runaway = lines.get(i + 1).map(|l| l.trim().to_string());
            i += 2;
            continue;
        }
        let err = split_file_line(line).map(|(f, l, m)| (Some(f), Some(l), m)).or_else(|| line.strip_prefix("! ").map(|m| (None, None, m.to_string())));
        if let Some((file, ln, msg)) = err {
            let (msg, after) = continued_message(&lines, msg.trim(), i + 1);
            let (mut ctx, ctx_line, next) = error_context(&lines, after);
            let file = file.or_else(|| files.current());
            let ln = ln.or(ctx_line);
            let mut message = msg;
            if lines[after..(after + 6).min(lines.len())].iter().any(|l| l.contains("no legal \\end found")) {
                if out.iter().any(|e| e.level == Level::Error) {
                    // consequence of an earlier error (e.g. a runaway argument)
                    i = next;
                    continue;
                }
                message = "Missing \\end{document}: the file ended before the document was closed.".to_string();
            }
            if let Some(r) = runaway.take() {
                if message.contains("while scanning use of") || message.contains("ended before") {
                    message.push_str(" Probably a missing `}`.");
                }
                ctx = format!("Runaway argument: {r}\n{ctx}").trim_end().to_string();
            }
            out.push(LogEntry { level: Level::Error, file, line: ln, message, context: ctx });
            i = next;
            continue;
        }
        if is_warning_start(line) {
            let (msg, k) = continued_message(&lines, line, i + 1);
            let ln = input_line(&msg);
            out.push(LogEntry { level: Level::Warning, file: files.current(), line: ln, message: msg, context: String::new() });
            i = k;
            continue;
        }
        if line.starts_with("Overfull \\") || line.starts_with("Underfull \\") {
            out.push(LogEntry { level: Level::BadBox, file: files.current(), line: input_line(line), message: line.trim().to_string(), context: String::new() });
            i += 1;
            continue;
        }
        files.scan(line);
        i += 1;
    }
    // "Emergency stop" / "Fatal error occurred" only repeat the real error
    if out.iter().any(|e| e.level == Level::Error && !is_followup(&e.message)) {
        out.retain(|e| e.level != Level::Error || !is_followup(&e.message));
    }
    // TeX repeats some errors: keep the first
    let mut seen = std::collections::HashSet::new();
    out.retain(|e| seen.insert((e.level, e.file.clone(), e.line, e.message.clone())));
    out.sort_by_key(|e| e.level);
    out
}

/// Files TeX could not find (`File `x.sty' not found`). A babel language
/// that is not installed is reported as its `.ldf` file.
pub fn missing_files(entries: &[LogEntry]) -> Vec<String> {
    let mut v = Vec::new();
    for e in entries.iter().filter(|e| e.level == Level::Error) {
        if e.message.contains("babel Error: Unknown option") || e.message.contains("babel Error: You haven't loaded the language") {
            let lang = e.message.split(['\'', '`']).nth(1).unwrap_or("").trim();
            if !lang.is_empty() && lang.chars().all(|c| c.is_ascii_alphanumeric()) {
                let f = format!("{lang}.ldf");
                if !v.contains(&f) {
                    v.push(f);
                }
            }
            continue;
        }
        if let Some(p) = e.message.find("File `") {
            let rest = &e.message[p + 6..];
            if let Some(q) = rest.find('\'')
                && rest[q..].contains("not found") {
                    let f = rest[..q].to_string();
                    if !v.contains(&f) {
                        v.push(f);
                    }
                }
        }
    }
    v
}

/// Page count from `Output written on main.pdf (12 pages, 3456 bytes).`
pub fn page_count(log: &str) -> Option<u32> {
    let p = log.rfind("Output written on ")?;
    let rest = &log[p..];
    let open = rest.find(" (")?;
    rest[open + 2..].split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const LOG: &str = r#"This is pdfTeX, Version 3.141592653-2.6-1.40.29 (TeX Live 2026/Arch Linux) (preloaded format=pdflatex)
(./main.tex
LaTeX2e <2025-11-01>
(/usr/share/texmf-dist/tex/latex/base/article.cls
Document Class: article 2024/06/29 v1.4n Standard LaTeX document class
(/usr/share/texmf-dist/tex/latex/base/size10.clo))
(./chapters/intro.tex
./chapters/intro.tex:7: Undefined control sequence.
l.7 Hello \foo
               world
)
./main.tex:12: LaTeX Error: File `nonexistent.sty' not found.

Type X to quit or <RETURN> to proceed,
or enter new name. (Default extension: sty)

Enter file name:
LaTeX Warning: Reference `fig:x' on page 1 undefined on input line 15.

Package hyperref Warning: Token not allowed in a PDF string (Unicode):
(hyperref)                removing `math shift' on input line 20.

Overfull \hbox (12.0pt too wide) in paragraph at lines 30--31
[1] (./main.aux) )
Output written on main.pdf (3 pages, 12345 bytes).
"#;

    #[test]
    fn parses_errors_and_warnings() {
        let e = parse(LOG);
        let errs: Vec<_> = e.iter().filter(|e| e.level == Level::Error).collect();
        assert_eq!(errs.len(), 2, "{e:#?}");
        assert_eq!(errs[0].file.as_deref(), Some("chapters/intro.tex"));
        assert_eq!(errs[0].line, Some(7));
        assert!(errs[0].context.contains("l.7 Hello \\foo"));
        assert_eq!(errs[1].file.as_deref(), Some("main.tex"));
        let w: Vec<_> = e.iter().filter(|e| e.level == Level::Warning).collect();
        assert_eq!(w.len(), 2, "{w:#?}");
        assert_eq!(w[0].line, Some(15));
        assert_eq!(w[0].file.as_deref(), Some("main.tex"));
        assert!(w[1].message.contains("removing `math shift'"), "{}", w[1].message);
        assert_eq!(w[1].line, Some(20));
        let bb: Vec<_> = e.iter().filter(|e| e.level == Level::BadBox).collect();
        assert_eq!(bb[0].line, Some(30));
        assert_eq!(missing_files(&e), vec!["nonexistent.sty".to_string()]);
        let babel = parse("/usr/share/texmf-dist/tex/generic/babel/babel.sty:4330: Package babel Error: Unknown option 'ngerman'.\n");
        assert_eq!(missing_files(&babel), vec!["ngerman.ldf".to_string()]);
        assert_eq!(page_count(LOG), Some(3));
    }

    #[test]
    fn classic_error_format() {
        let log = "(./doc.tex\n! Missing $ inserted.\n<inserted text> \n                $\nl.5 a^\n      b\n)";
        let e = parse(log);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].file.as_deref(), Some("doc.tex"));
        assert_eq!(e[0].line, Some(5));
        assert_eq!(e[0].message, "Missing $ inserted.");
    }

    #[test]
    fn multi_line_errors_and_followups() {
        let log = "(./main.tex\n./main.tex:4: LaTeX Error: Unicode character \u{2211} (U+2211)\n               not set up for use with LaTeX.\n\nSee the LaTeX manual.\n\n! Emergency stop.\n<*> main.tex\n\n!  ==> Fatal error occurred, no output PDF file produced!\n)";
        let e = parse(log);
        assert_eq!(e.len(), 1, "{e:#?}");
        assert_eq!(e[0].message, "LaTeX Error: Unicode character \u{2211} (U+2211) not set up for use with LaTeX.");
        let pkg = parse("/usr/share/texmf-dist/tex/latex/fontspec/fontspec.sty:101: Fatal Package fontspec Error: The fontspec package requires either XeTeX or\n(fontspec)                      LuaTeX.\n(fontspec)                      \n(fontspec)                      You must change your typesetting engine.\n\n");
        assert!(pkg[0].message.ends_with("requires either XeTeX or LuaTeX. You must change your typesetting engine."), "{}", pkg[0].message);
    }

    #[test]
    fn missing_end_document_and_runaway() {
        let fatal = "(./main.tex\n! Emergency stop.\n<*> main.tex\n            \n*** (job aborted, no legal \\end found)\n\n!  ==> Fatal error occurred, no output PDF file produced!\n";
        let e = parse(fatal);
        assert_eq!(e.len(), 1, "{e:#?}");
        assert!(e[0].message.starts_with("Missing \\end{document}"), "{}", e[0].message);
        let runaway = "(./main.tex\nRunaway argument?\n{unclosed \\par \\section {x} \\end {document} \n! File ended while scanning use of \\textbf .\n<inserted text> \n                \\par \n<*> main.tex\n            \nI suspect you have forgotten a `}'.\n\n! Emergency stop.\n<*> main.tex\n            \n*** (job aborted, no legal \\end found)\n\n!  ==> Fatal error occurred, no output PDF file produced!\n";
        let e = parse(runaway);
        assert_eq!(e.len(), 1, "{e:#?}");
        assert!(e[0].message.contains("missing `}`"), "{}", e[0].message);
        assert!(e[0].context.starts_with("Runaway argument: {unclosed"), "{}", e[0].context);
    }
}
