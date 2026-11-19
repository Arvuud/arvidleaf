//! Which package an environment or command needs, whether a preamble already
//! loads it, and where to add `\usepackage{…}`.

use std::collections::HashSet;

use crate::outline::strip_comment;

/// Package that should be loaded when `\begin{env}` is used.
pub fn for_env(env: &str) -> Option<&'static str> {
    Some(match env {
        // enumitem: `\begin{enumerate}[label=(\alph*)]` etc.
        "enumerate" => "enumitem",
        "align" | "align*" | "gather" | "gather*" | "multline" | "multline*" | "split" | "cases" | "matrix" | "pmatrix" | "bmatrix"
        | "vmatrix" | "Vmatrix" | "Bmatrix" | "equation*" | "aligned" | "alignat" | "alignat*" | "flalign" | "flalign*" => "amsmath",
        "proof" => "amsthm",
        "tabularx" => "tabularx",
        "longtable" => "longtable",
        "lstlisting" => "listings",
        "minted" => "minted",
        "tikzpicture" => "tikz",
        "axis" => "pgfplots",
        "subfigure" | "subtable" => "subcaption",
        "wrapfigure" => "wrapfig",
        "appendices" => "appendix",
        "multicols" | "multicols*" => "multicol",
        _ => return None,
    })
}

/// Package that should be loaded when `\cmd` is used (name without backslash).
pub fn for_command(cmd: &str) -> Option<&'static str> {
    Some(match cmd {
        "includegraphics" | "graphicspath" => "graphicx",
        "toprule" | "midrule" | "bottomrule" | "cmidrule" => "booktabs",
        "mathbb" | "mathfrak" | "varnothing" | "leqslant" | "geqslant" => "amssymb",
        "text" | "eqref" | "dfrac" | "tfrac" | "binom" | "operatorname" | "boldsymbol" | "iint" | "iiint" | "DeclareMathOperator" => "amsmath",
        "href" | "url" | "autoref" | "hypersetup" | "phantomsection" => "hyperref",
        "cref" | "Cref" => "cleveref",
        "textcolor" | "color" | "colorbox" | "definecolor" => "xcolor",
        "si" | "SI" | "qty" | "unit" | "num" => "siunitx",
        "multirow" => "multirow",
        "lstinline" | "lstinputlisting" => "listings",
        "mintinline" => "minted",
        "usetikzlibrary" => "tikz",
        "geometry" => "geometry",
        "sout" => "ulem",
        _ => return None,
    })
}

/// Packages a document class or package loads by itself.
fn implied_by(name: &str) -> &'static [&'static str] {
    match name {
        "beamer" => &["amsmath", "amsthm", "hyperref", "xcolor", "graphicx", "tikz"],
        "mathtools" => &["amsmath"],
        "amssymb" => &["amsfonts"],
        "pgfplots" => &["tikz", "xcolor", "graphicx"],
        "tikz" => &["xcolor", "graphicx"],
        "cleveref" => &[],
        "minted" => &["xcolor"],
        _ => &[],
    }
}

/// Packages that clash with a class (beamer has its own list styles).
fn conflicts(class: &str, pkg: &str) -> bool {
    matches!((class, pkg), ("beamer", "enumitem" | "geometry" | "hyperref"))
}

/// The document class and all loaded packages (`\usepackage`, `\RequirePackage`)
/// in the given sources, including what they load implicitly.
pub fn loaded(sources: &[&str]) -> (Option<String>, HashSet<String>) {
    let mut class = None;
    let mut set = HashSet::new();
    for src in sources {
        let code: String = src.lines().map(strip_comment).collect::<Vec<_>>().join("\n");
        for (cmd, is_class) in [("\\usepackage", false), ("\\RequirePackage", false), ("\\documentclass", true)] {
            let mut from = 0;
            while let Some(p) = code[from..].find(cmd) {
                let start = from + p + cmd.len();
                from = start;
                let Some((names, _)) = argument(&code, start) else { continue };
                for n in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                    if is_class {
                        class.get_or_insert_with(|| n.to_string());
                    }
                    set.insert(n.to_string());
                }
            }
        }
    }
    // close over implied packages (two levels are enough for the table above)
    for _ in 0..2 {
        let extra: Vec<String> = set.iter().flat_map(|n| implied_by(n).iter().map(|s| s.to_string())).collect();
        set.extend(extra);
    }
    (class, set)
}

/// The `{…}` argument after a command (skipping whitespace and one `[…]`),
/// with the byte offset just past its closing brace.
fn argument(code: &str, mut i: usize) -> Option<(&str, usize)> {
    let b = code.as_bytes();
    let skip_ws = |i: &mut usize| {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
    };
    skip_ws(&mut i);
    if b.get(i) == Some(&b'[') {
        i += code[i..].find(']')? + 1;
        skip_ws(&mut i);
    }
    if b.get(i) != Some(&b'{') {
        return None;
    }
    let close = i + code[i..].find('}')?;
    Some((&code[i + 1..close], close + 1))
}

/// Whether `pkg` should be added to a document whose sources are `sources`.
pub fn needed(sources: &[&str], pkg: &str) -> bool {
    let (class, set) = loaded(sources);
    !set.contains(pkg) && !class.is_some_and(|c| conflicts(&c, pkg))
}

/// Where to insert `\usepackage{pkg}` in `src`: after the last `\usepackage`
/// line of the preamble, otherwise after `\documentclass`. Returns the byte
/// offset and the text to insert, or `None` if `src` has no preamble.
pub fn insertion(src: &str, pkg: &str) -> Option<(usize, String)> {
    let end = src.find("\\begin{document}").unwrap_or(src.len());
    let pre = &src[..end];
    let mut after = None;
    let mut offset = 0;
    for line in pre.split_inclusive('\n') {
        let code = strip_comment(line);
        let t = code.trim_start();
        for cmd in ["\\usepackage", "\\RequirePackage", "\\documentclass"] {
            if t.starts_with(cmd) {
                let at = offset + (code.len() - t.len()) + cmd.len();
                // options may span several lines
                let stop = argument(src, at).map_or(offset + line.len(), |(_, e)| e);
                let eol = src[stop..].find('\n').map_or(src.len(), |p| stop + p + 1);
                if cmd != "\\documentclass" || after.is_none() {
                    after = Some(eol);
                }
            }
        }
        offset += line.len();
    }
    let at = after?.min(end);
    let nl = if at > 0 && !src[..at].ends_with('\n') { "\n" } else { "" };
    Some((at, format!("{nl}\\usepackage{{{pkg}}}\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_loaded_packages() {
        let src = "\\documentclass[11pt]{article}\n\\usepackage[shortlabels]{enumitem}\n\\usepackage{amsmath, amssymb}\n% \\usepackage{tikz}\n";
        assert!(!needed(&[src], "enumitem"));
        assert!(!needed(&[src], "amssymb"));
        assert!(!needed(&[src], "amsfonts"));
        assert!(needed(&[src], "tikz"), "commented-out packages do not count");
        assert!(!needed(&["\\usepackage{mathtools}"], "amsmath"));
        assert!(!needed(&["\\documentclass{beamer}"], "enumitem"), "enumitem breaks beamer lists");
        assert!(!needed(&["\\documentclass{beamer}"], "xcolor"));
        // a package loaded in a separate preamble file counts too
        assert!(!needed(&["\\documentclass{article}\\input{pre}", "\\usepackage{tikz}"], "xcolor"));
    }

    #[test]
    fn inserts_after_last_usepackage() {
        let src = "\\documentclass{article}\n\\usepackage{a}\n\\usepackage[\n  x=1\n]{b}\n\\title{T}\n\\begin{document}\n\\usepackage{late}\n";
        let (at, s) = insertion(src, "enumitem").unwrap();
        let mut out = src.to_string();
        out.insert_str(at, &s);
        assert!(out.contains("]{b}\n\\usepackage{enumitem}\n\\title{T}"), "{out}");
    }

    #[test]
    fn inserts_after_documentclass_without_packages() {
        let src = "\\documentclass[a4paper]{article}\n\\begin{document}\nx\n\\end{document}";
        let (at, s) = insertion(src, "enumitem").unwrap();
        let mut out = src.to_string();
        out.insert_str(at, &s);
        assert!(out.starts_with("\\documentclass[a4paper]{article}\n\\usepackage{enumitem}\n\\begin{document}"), "{out}");
        assert_eq!(insertion("just a chapter", "x"), None);
        // file ending right after the last package line
        let (at, s) = insertion("\\usepackage{a}", "b").unwrap();
        assert_eq!((at, s.as_str()), (14, "\n\\usepackage{b}\n"));
    }

    #[test]
    fn maps() {
        assert_eq!(for_env("enumerate"), Some("enumitem"));
        assert_eq!(for_env("align*"), Some("amsmath"));
        assert_eq!(for_env("itemize"), None);
        assert_eq!(for_command("toprule"), Some("booktabs"));
        assert_eq!(for_command("section"), None);
    }
}
