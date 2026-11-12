//! Hints for common mistakes behind cryptic TeX errors, mostly a package
//! that is used but not loaded (`\toprule` without booktabs, list options
//! without enumitem, …).

use crate::log::{Level, LogEntry};

/// Commands and the package that defines them.
const COMMANDS: &[(&str, &str)] = &[
    ("includegraphics", "graphicx"),
    ("toprule", "booktabs"),
    ("midrule", "booktabs"),
    ("bottomrule", "booktabs"),
    ("cmidrule", "booktabs"),
    ("href", "hyperref"),
    ("url", "url"),
    ("eqref", "amsmath"),
    ("text", "amsmath"),
    ("DeclareMathOperator", "amsmath"),
    ("mathbb", "amssymb"),
    ("mathfrak", "amssymb"),
    ("SI", "siunitx"),
    ("si", "siunitx"),
    ("qty", "siunitx"),
    ("unit", "siunitx"),
    ("num", "siunitx"),
    ("cref", "cleveref"),
    ("Cref", "cleveref"),
    ("textcolor", "xcolor"),
    ("color", "xcolor"),
    ("colorbox", "xcolor"),
    ("lipsum", "lipsum"),
    ("blindtext", "blindtext"),
    ("multirow", "multirow"),
    ("subcaption", "subcaption"),
    ("captionof", "caption"),
    ("FloatBarrier", "placeins"),
    ("autoref", "hyperref"),
    ("tikz", "tikz"),
    ("lstinline", "listings"),
    ("mintinline", "minted"),
    ("enquote", "csquotes"),
    ("todo", "todonotes"),
    ("addbibresource", "biblatex"),
    ("printbibliography", "biblatex"),
    ("textcite", "biblatex"),
    ("parencite", "biblatex"),
    ("citep", "natbib"),
    ("citet", "natbib"),
];

/// Environments and the package that defines them.
const ENVIRONMENTS: &[(&str, &str)] = &[
    ("align", "amsmath"),
    ("align*", "amsmath"),
    ("gather", "amsmath"),
    ("gather*", "amsmath"),
    ("multline", "amsmath"),
    ("multline*", "amsmath"),
    ("pmatrix", "amsmath"),
    ("bmatrix", "amsmath"),
    ("cases", "amsmath"),
    ("equation*", "amsmath"),
    ("tikzpicture", "tikz"),
    ("lstlisting", "listings"),
    ("minted", "minted"),
    ("subfigure", "subcaption"),
    ("multicols", "multicol"),
    ("longtable", "longtable"),
    ("tabularx", "tabularx"),
    ("algorithm", "algorithm"),
    ("algorithmic", "algpseudocode"),
    ("proof", "amsthm"),
    ("wrapfigure", "wrapfig"),
    ("landscape", "pdflscape"),
];

/// Packages loaded anywhere in the project (`\usepackage[…]{a,b}`).
fn loaded(sources: &[(String, String)]) -> Vec<String> {
    let mut v = Vec::new();
    for (_, src) in sources {
        for line in src.lines() {
            let code = crate::outline::strip_comment(line);
            for cmd in ["\\usepackage", "\\RequirePackage", "\\documentclass"] {
                let mut rest = code;
                while let Some(p) = rest.find(cmd) {
                    rest = &rest[p + cmd.len()..];
                    let Some(open) = rest.find('{') else { break };
                    let Some(close) = rest[open..].find('}') else { break };
                    v.extend(rest[open + 1..open + close].split(',').map(|s| s.trim().to_string()));
                    rest = &rest[open + close..];
                }
            }
        }
    }
    v
}

fn is_loaded(loaded: &[String], pkg: &str) -> bool {
    let first = pkg.split_whitespace().next().unwrap_or(pkg);
    // packages that load others
    let implied: &[(&str, &[&str])] = &[
        ("tikz", &["tikz", "pgf"]),
        ("xcolor", &["xcolor", "color", "tikz", "pgf", "beamer"]),
        ("graphicx", &["graphicx", "beamer", "tikz"]),
        ("url", &["url", "hyperref"]),
        ("hyperref", &["hyperref", "beamer"]),
        ("amsmath", &["amsmath", "mathtools", "beamer"]),
        ("amssymb", &["amssymb", "amsfonts"]),
        ("caption", &["caption", "subcaption"]),
        ("algorithm", &["algorithm", "algorithm2e"]),
        ("algpseudocode", &["algpseudocode", "algorithmic", "algorithmicx"]),
    ];
    let names = implied.iter().find(|(p, _)| *p == first).map(|(_, n)| *n).unwrap_or(std::slice::from_ref(&first));
    loaded.iter().any(|l| names.contains(&l.as_str()))
}

/// Source line `line` (1-based) of `file` and up to `before` lines above it.
fn lines_before<'a>(sources: &'a [(String, String)], file: &str, line: u32, before: usize) -> Vec<&'a str> {
    let Some((_, src)) = sources.iter().find(|(f, _)| f == file) else { return Vec::new() };
    let all: Vec<&str> = src.lines().collect();
    let end = (line as usize).min(all.len());
    all[end.saturating_sub(before + 1)..end].to_vec()
}

/// The undefined command from the context line `l.12 some text \foo`.
fn undefined_command(context: &str) -> Option<String> {
    let l = context.lines().find(|l| l.starts_with("l."))?;
    let p = l.rfind('\\')?;
    let name: String = l[p + 1..].chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    (!name.is_empty()).then_some(name)
}

fn hint_for(e: &LogEntry, sources: &[(String, String)], loaded: &[String]) -> Option<String> {
    let need = |pkg: &str, what: &str| (!is_loaded(loaded, pkg)).then(|| format!("Hint: {what} requires \\usepackage{{{pkg}}} in the preamble."));
    let m = &e.message;
    if m.contains("Undefined control sequence") {
        let cmd = undefined_command(&e.context)?;
        let (_, pkg) = COMMANDS.iter().find(|(c, _)| *c == cmd)?;
        return need(pkg, &format!("\\{cmd}"));
    }
    if let Some(p) = m.find("Environment ")
        && let Some(rest) = m[p + 12..].strip_suffix(" undefined.").or_else(|| m[p + 12..].strip_suffix(" undefined"))
    {
        let (_, pkg) = ENVIRONMENTS.iter().find(|(n, _)| *n == rest)?;
        return need(pkg, &format!("the environment {rest}"));
    }
    if m.starts_with("Missing $ inserted") {
        return Some("Hint: a math command (^, _, \\alpha, \\frac …) outside $…$, or a $ that is not closed earlier in this paragraph.".into());
    }
    if m.contains("perhaps a missing \\item") || m.contains("Missing number, treated as zero") {
        // list options like [label=\alph*)] without enumitem
        let file = e.file.as_deref()?;
        let near = lines_before(sources, file, e.line?, 3);
        let list_opts = near.iter().any(|l| {
            let c = crate::outline::strip_comment(l);
            ["enumerate", "itemize", "description"].iter().any(|env| c.contains(&format!("\\begin{{{env}}}[")))
        });
        if list_opts {
            return need("enumitem", "a list option like [label=…]");
        }
    }
    None
}

/// Appends hints to the messages of errors whose cause is known.
pub fn add(sources: &[(String, String)], entries: &mut [LogEntry]) {
    if !entries.iter().any(|e| e.level == Level::Error) {
        return;
    }
    let loaded = loaded(sources);
    for e in entries.iter_mut().filter(|e| e.level == Level::Error) {
        if let Some(h) = hint_for(e, sources, &loaded) {
            e.message.push(' ');
            e.message.push_str(&h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(line: u32, message: &str, context: &str) -> LogEntry {
        LogEntry { level: Level::Error, file: Some("main.tex".into()), line: Some(line), message: message.into(), context: context.into() }
    }

    fn src(s: &str) -> Vec<(String, String)> {
        vec![("main.tex".into(), s.into())]
    }

    #[test]
    fn list_options_without_enumitem() {
        let s = src("\\documentclass{article}\n\\begin{document}\n\\begin{enumerate}[label=\\alph*)]\n\t\\item a\n\\end{enumerate}\n\\end{document}\n");
        let mut e = vec![err(4, "LaTeX Error: Something's wrong--perhaps a missing \\item.", "")];
        add(&s, &mut e);
        assert!(e[0].message.contains("\\usepackage{enumitem}"), "{}", e[0].message);
        // with enumitem: no hint
        let s = src("\\documentclass{article}\n\\usepackage[shortlabels]{enumitem}\n\\begin{document}\n\\begin{enumerate}[a)]\n\\item a\n");
        let mut e = vec![err(5, "LaTeX Error: Something's wrong--perhaps a missing \\item.", "")];
        add(&s, &mut e);
        assert!(!e[0].message.contains("Hint"));
    }

    #[test]
    fn undefined_commands_and_environments() {
        let s = src("\\documentclass{article}\n\\usepackage{amsmath,hyperref}\n");
        let mut e = vec![
            err(5, "Undefined control sequence.", "l.5     \\toprule"),
            err(6, "Undefined control sequence.", "l.6 see \\url"),
            err(7, "LaTeX Error: Environment tikzpicture undefined.", ""),
            err(8, "LaTeX Error: Environment align undefined.", ""),
            err(9, "Undefined control sequence.", "l.9 \\myownmacro"),
        ];
        add(&s, &mut e);
        assert!(e[0].message.contains("\\usepackage{booktabs}"));
        assert!(!e[1].message.contains("Hint"), "hyperref loads url");
        assert!(e[2].message.contains("\\usepackage{tikz}"));
        assert!(!e[3].message.contains("Hint"));
        assert!(!e[4].message.contains("Hint"));
        let mut e = vec![err(3, "Missing $ inserted.", "")];
        add(&s, &mut e);
        assert!(e[0].message.contains("not closed"));
    }
}
