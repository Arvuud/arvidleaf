//! Completion candidates: commands, environments, and the context at the cursor.

use std::collections::HashMap;

/// Common LaTeX commands (without backslash). `{}` marks where an argument goes.
pub const COMMANDS: &[&str] = &[
    "documentclass{}", "usepackage{}", "begin{}", "end{}", "title{}", "author{}", "date{}", "maketitle", "tableofcontents", "listoffigures",
    "listoftables", "part{}", "chapter{}", "section{}", "subsection{}", "subsubsection{}", "paragraph{}", "subparagraph{}", "label{}", "ref{}",
    "eqref{}", "pageref{}", "autoref{}", "cref{}", "Cref{}", "cite{}", "citep{}", "citet{}", "parencite{}", "textcite{}", "footnote{}", "textbf{}",
    "textit{}", "texttt{}", "textsc{}", "textsf{}", "textrm{}", "emph{}", "underline{}", "url{}", "href{}{}", "includegraphics{}", "caption{}",
    "centering", "item", "newpage", "clearpage", "cleardoublepage", "pagebreak", "linebreak", "noindent", "vspace{}", "hspace{}", "vfill", "hfill",
    "smallskip", "medskip", "bigskip", "input{}", "include{}", "bibliography{}", "bibliographystyle{}", "addbibresource{}", "printbibliography",
    "newcommand{}{}", "renewcommand{}{}", "newenvironment{}{}{}", "setlength{}{}", "textwidth", "linewidth", "columnwidth", "frac{}{}", "dfrac{}{}",
    "sqrt{}", "sum", "prod", "int", "iint", "oint", "lim", "infty", "partial", "nabla", "cdot", "times", "leq", "geq", "neq", "approx", "equiv",
    "leqslant", "geqslant", "ll", "gg", "le", "ge", "pm", "mp", "div", "circ", "ast", "star", "otimes", "oplus", "setminus", "emptyset", "varnothing",
    "mid", "parallel", "perp", "langle", "rangle", "lfloor", "rfloor", "lceil", "rceil", "sup", "inf", "max", "min", "log", "ln", "exp", "sin", "cos",
    "tan", "det", "dim", "ker", "arg", "deg", "mathfrak{}", "mathsf{}", "widehat{}", "widetilde{}", "underline{}", "cdotp", "colon", "coloneqq",
    "iff", "implies", "impliedby", "longrightarrow", "longmapsto", "hookrightarrow", "uparrow", "downarrow", "leftrightarrow", "ell", "hbar", "Re", "Im",
    "aleph", "wedge", "vee", "neg", "lnot", "top", "bot", "vdash", "models", "subsetneq", "supset", "supseteq", "bigcup", "bigcap", "coprod", "simeq",
    "cong", "prec", "succ", "preceq", "succeq", "tfrac{}{}", "cfrac{}{}", "stackrel{}{}", "overset{}{}", "underset{}{}", "xrightarrow{}", "boxed{}",
    "displaystyle", "textstyle", "nonumber", "notag", "tag{}",
    "sim", "propto", "in", "notin", "subset", "subseteq", "cup", "cap", "forall", "exists", "rightarrow", "leftarrow", "Rightarrow", "Leftarrow",
    "Leftrightarrow", "mapsto", "to", "alpha", "beta", "gamma", "delta", "epsilon", "varepsilon", "zeta", "eta", "theta", "vartheta", "iota",
    "kappa", "lambda", "mu", "nu", "xi", "pi", "rho", "sigma", "tau", "upsilon", "phi", "varphi", "chi", "psi", "omega", "Gamma", "Delta", "Theta",
    "Lambda", "Xi", "Pi", "Sigma", "Phi", "Psi", "Omega", "mathbb{}", "mathcal{}", "mathrm{}", "mathbf{}", "mathit{}", "boldsymbol{}", "text{}",
    "operatorname{}", "left", "right", "big", "Big", "bigg", "Bigg", "ldots", "cdots", "vdots", "ddots", "hat{}", "bar{}", "vec{}", "dot{}",
    "ddot{}", "tilde{}", "overline{}", "underbrace{}", "overbrace{}", "binom{}{}", "pmod{}", "quad", "qquad", "today", "LaTeX", "TeX", "hline",
    "toprule", "midrule", "bottomrule", "cline{}", "multicolumn{}{}{}", "multirow{}{}{}", "tiny", "scriptsize", "footnotesize", "small",
    "normalsize", "large", "Large", "LARGE", "huge", "Huge", "frametitle{}", "pause", "titlepage", "appendix", "frontmatter", "mainmatter",
    "backmatter", "maketitle", "thanks{}", "abstract", "tikz", "draw", "node", "fill", "usetikzlibrary{}", "si{}", "SI{}{}", "qty{}{}", "unit{}",
    "num{}", "textcolor{}{}", "color{}", "colorbox{}{}", "footnotemark", "footnotetext{}", "marginpar{}", "newline", "par", "verb", "lstinline",
    "mintinline{}{}", "graphicspath{}", "hypersetup{}", "geometry{}", "setcounter{}{}", "addcontentsline{}{}{}", "phantomsection", "nocite{}",
];

pub const ENVIRONMENTS: &[&str] = &[
    "document", "abstract", "figure", "figure*", "table", "table*", "tabular", "tabularx", "longtable", "itemize", "enumerate", "description",
    "equation", "equation*", "align", "align*", "gather", "gather*", "multline", "split", "cases", "matrix", "pmatrix", "bmatrix", "vmatrix",
    "array", "center", "flushleft", "flushright", "quote", "quotation", "verbatim", "lstlisting", "minted", "minipage", "frame", "block",
    "columns", "column", "tikzpicture", "axis", "theorem", "lemma", "proof", "definition", "corollary", "remark", "example", "thebibliography",
    "titlepage", "subfigure", "wrapfigure", "appendices",
];

/// How common a command is in LaTeX documents in general (0 = rare … 3 =
/// in almost every document). Users' own habits weigh far more, see
/// [`command_score`].
pub fn base_frequency(name: &str) -> u32 {
    const VERY_COMMON: &[&str] = &[
        "begin", "end", "section", "subsection", "label", "ref", "cite", "textbf", "emph", "item", "frac", "includegraphics", "usepackage",
        "caption", "documentclass", "textit", "left", "right", "mathbb", "alpha", "cdot", "sum", "int", "infty", "leq", "geq", "text", "mathrm",
        "eqref", "footnote", "centering", "hline", "sqrt", "lambda", "mu", "pi", "sigma", "partial", "in", "to",
    ];
    const COMMON: &[&str] = &[
        "chapter", "subsubsection", "maketitle", "title", "author", "date", "beta", "gamma", "delta", "theta", "varphi", "phi", "epsilon",
        "varepsilon", "omega", "rho", "tau", "nabla", "times", "approx", "neq", "rightarrow", "Rightarrow", "mathcal", "mathbf", "hat", "bar",
        "vec", "dot", "tilde", "overline", "ldots", "cdots", "quad", "qquad", "vspace", "hspace", "newcommand", "input", "tableofcontents", "url",
        "href", "texttt", "lim", "operatorname", "boldsymbol", "newpage", "noindent", "cref", "pm", "le", "ge", "forall", "exists", "subset",
        "subseteq", "cup", "cap", "mapsto", "equiv", "sim", "log", "exp", "sin", "cos", "max", "min", "sup", "inf", "langle", "rangle", "dfrac",
        "binom", "prod", "Delta", "Omega", "Sigma", "ell", "mid", "circ", "iff", "implies", "dots", "par", "textwidth", "linewidth", "small",
        "footnotesize", "large", "toprule", "midrule", "bottomrule", "multicolumn", "bibliography", "bibliographystyle", "printbibliography",
        "addbibresource", "SI", "si", "qty", "num", "big", "Big", "displaystyle", "nonumber", "tag", "setminus", "emptyset", "otimes", "kappa",
        "eta", "xi", "psi", "chi", "zeta", "nu", "Gamma", "Lambda", "Phi", "Psi", "Pi", "Theta",
    ];
    if VERY_COMMON.contains(&name) {
        3
    } else if COMMON.contains(&name) {
        2
    } else if COMMANDS.iter().any(|c| c.split('{').next() == Some(name)) {
        1
    } else {
        0
    }
}

/// Where a command belongs: `Some(true)` only in math, `Some(false)` only in
/// text, `None` anywhere (or unknown).
pub fn math_only(name: &str) -> Option<bool> {
    const TEXT: &[&str] = &[
        "documentclass", "usepackage", "begin", "title", "author", "date", "maketitle", "tableofcontents", "listoffigures", "listoftables",
        "part", "chapter", "section", "subsection", "subsubsection", "paragraph", "subparagraph", "label", "ref", "eqref", "pageref",
        "autoref", "cref", "Cref", "cite", "citep", "citet", "parencite", "textcite", "footnote", "textbf", "textit", "texttt", "textsc",
        "textsf", "textrm", "emph", "underline", "url", "href", "includegraphics", "caption", "centering", "item", "newpage", "clearpage",
        "cleardoublepage", "pagebreak", "linebreak", "noindent", "vspace", "vfill", "smallskip", "medskip", "bigskip", "input", "include",
        "bibliography", "bibliographystyle", "addbibresource", "printbibliography", "hline", "toprule", "midrule", "bottomrule", "multicolumn",
        "multirow", "frametitle", "pause", "appendix", "footnotemark", "footnotetext", "marginpar", "newline", "par", "today", "LaTeX", "TeX",
        "textcolor", "graphicspath", "hypersetup", "geometry", "newcommand", "renewcommand", "newenvironment", "tiny", "scriptsize",
        "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge", "end",
    ];
    if TEXT.contains(&name) {
        return Some(false);
    }
    const ANYWHERE: &[&str] = &[
        "hspace", "quad", "qquad", "ldots", "dots", "text", "textwidth", "linewidth", "columnwidth", "setlength", "si", "SI", "qty", "num",
        "unit", "color", "ensuremath", "verb", "hfill", "vdots",
    ];
    if ANYWHERE.contains(&name) {
        return None;
    }
    // the rest of the built-in list is math (Greek, operators, relations, accents …)
    COMMANDS.iter().any(|c| c.split('{').next() == Some(name)).then_some(true)
}

/// Whether the cursor at the end of `before` is in math mode: inside `$…$`,
/// `$$…$$`, `\(…\)`, `\[…\]` or a math environment, and not in a `\text{…}`
/// inside it. Looks at the last few thousand characters only.
pub fn in_math(before: &str) -> bool {
    let mut cut = before.len().saturating_sub(6000);
    while !before.is_char_boundary(cut) {
        cut += 1;
    }
    let s = &before[cut..];
    let b = s.as_bytes();
    let mut math = false;
    // depth of math environments; `$`-style math as a flag
    let mut envs: Vec<String> = Vec::new();
    // brace depth where a `\text{` (or similar) inside math started
    let mut text_at: Vec<usize> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                i = s[i..].find('\n').map_or(b.len(), |p| i + p);
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                if text_at.last() == Some(&depth) {
                    text_at.pop();
                }
                depth = depth.saturating_sub(1);
            }
            b'$' => {
                if b.get(i + 1) == Some(&b'$') {
                    i += 1;
                }
                math = !math;
            }
            b'\\' => {
                let start = i + 1;
                let mut j = start;
                while j < b.len() && b[j].is_ascii_alphabetic() {
                    j += 1;
                }
                if j == start {
                    match b.get(start) {
                        Some(b'(') | Some(b'[') => math = true,
                        Some(b')') | Some(b']') => math = false,
                        _ => {}
                    }
                    i = start + 1;
                    continue;
                }
                let name = &s[start..j];
                let arg = || s[j..].strip_prefix('{').and_then(|r| r.split_once('}')).map(|(a, _)| a);
                match name {
                    "begin" => {
                        if let Some(env) = arg().filter(|e| crate::lexer::MATH_ENVS.contains(e)) {
                            envs.push(env.to_string());
                        }
                    }
                    "end" => {
                        if let Some(env) = arg()
                            && envs.last().is_some_and(|e| e == env) {
                                envs.pop();
                            }
                    }
                    "text" | "textrm" | "textbf" | "textit" | "mbox" | "intertext" if (math || !envs.is_empty()) && b.get(j) == Some(&b'{') => {
                        text_at.push(depth + 1);
                    }
                    _ => {}
                }
                i = j;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    (math || !envs.is_empty()) && text_at.is_empty()
}

/// Ranking score of a command completion `item` (`frac{}{}`): what the user
/// writes across all projects (`user`: picks and hand-typed uses) weighs most,
/// then its uses in this document (`doc`), then how common it is in general
/// and whether it fits math or text mode at the cursor.
pub fn command_score(item: &str, user: u32, doc: u32, math: bool) -> u32 {
    let name = item.split('{').next().unwrap_or(item);
    let fits = match math_only(name) {
        Some(m) => m == math,
        None => true,
    };
    // a command of the other mode keeps only what the user did with it
    let base = if fits { 2 * base_frequency(name) + 1 } else { 0 };
    8 * user.min(500) + 2 * doc.min(50) + base
}

/// What the user is typing at the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Context {
    /// `\sec` → prefix "sec"
    Command(String),
    /// `\begin{fi` → prefix "fi"
    Environment(String),
    /// `\ref{fig:` → prefix "fig:"
    Label(String),
    /// `\cite{kn` → prefix "kn" (only the key after the last comma)
    Citation(String),
    /// `\input{ch` / `\includegraphics{im`
    File(String),
}

/// Determines the completion context from the text before the cursor.
pub fn context(before: &str) -> Option<Context> {
    let mut cut = before.len().saturating_sub(200);
    while !before.is_char_boundary(cut) {
        cut += 1;
    }
    let tail = &before[cut..];
    let line = tail.rsplit('\n').next().unwrap_or(tail);
    // inside an argument?
    if let Some(open) = line.rfind('{')
        && !line[open..].contains('}') {
            let arg = &line[open + 1..];
            let cmd_start = line[..open].rfind('\\')?;
            let cmd: String = line[cmd_start + 1..].chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            return match cmd.as_str() {
                "begin" | "end" => Some(Context::Environment(arg.to_string())),
                "ref" | "eqref" | "pageref" | "autoref" | "cref" | "Cref" | "nameref" | "vref" => Some(Context::Label(arg.to_string())),
                c if c.contains("cite") => Some(Context::Citation(arg.rsplit(',').next().unwrap_or("").trim().to_string())),
                "input" | "include" | "includegraphics" | "subfile" | "lstinputlisting" | "addbibresource" | "bibliography" => {
                    Some(Context::File(arg.to_string()))
                }
                _ => None,
            };
        }
    let bs = line.rfind('\\')?;
    let word = &line[bs + 1..];
    if word.chars().all(|c| c.is_ascii_alphabetic()) && !word.is_empty() {
        return Some(Context::Command(word.to_string()));
    }
    None
}

/// Candidates for a prefix: prefix matches first, then substring matches.
pub fn rank<'a>(prefix: &str, items: impl Iterator<Item = &'a str>, limit: usize) -> Vec<&'a str> {
    rank_by(prefix, items, |_| 0, limit)
}

/// Like [`rank`], but within each group items with a higher `score` (how often
/// the user picked or wrote them) come first. If the prefix is already a
/// complete name, only longer names starting with it are offered, so Enter
/// after a finished command stays a line break.
pub fn rank_by<'a>(prefix: &str, items: impl Iterator<Item = &'a str>, score: impl Fn(&str) -> u32, limit: usize) -> Vec<&'a str> {
    let lower = prefix.to_ascii_lowercase();
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    let mut exact = false;
    for it in items {
        let name = it.split('{').next().unwrap_or(it);
        if name == prefix {
            exact = true;
            continue;
        }
        let l = name.to_ascii_lowercase();
        if l.starts_with(&lower) {
            starts.push(it);
        } else if lower.len() >= 2 && l.contains(&lower) {
            contains.push(it);
        }
    }
    let key = |s: &&str| (std::cmp::Reverse(score(s)), !s.starts_with(prefix), s.len());
    starts.sort_by_key(key);
    contains.sort_by_key(key);
    if exact {
        contains.clear();
    }
    let mut seen = std::collections::HashSet::new();
    starts.into_iter().chain(contains).filter(|s| seen.insert(*s)).take(limit).collect()
}

/// Commands (as completion templates like `vect{}`) and environments used in
/// a document, with how often they occur.
#[derive(Default, Debug)]
pub struct Vocab {
    pub commands: HashMap<String, u32>,
    pub envs: HashMap<String, u32>,
}

/// Collects the commands and environments of `src`. The command name
/// starting at byte `skip` (the one being typed) is left out.
pub fn vocabulary(src: &str, skip: Option<usize>) -> Vocab {
    let known: HashMap<&str, &str> = COMMANDS.iter().map(|c| (c.split('{').next().unwrap_or(c), *c)).collect();
    // name → (uses, uses followed by `{`)
    let mut cmds: HashMap<&str, (u32, u32)> = HashMap::new();
    let mut envs: HashMap<String, u32> = HashMap::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && (i == 0 || b[i - 1] != b'\\') {
            // skip comments
            i = src[i..].find('\n').map_or(b.len(), |p| i + p);
            continue;
        }
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        while j < b.len() && b[j].is_ascii_alphabetic() {
            j += 1;
        }
        if j == start {
            i = start + 1; // `\\`, `\{` …
            continue;
        }
        let name = &src[start..j];
        if Some(start) != skip {
            let e = cmds.entry(name).or_default();
            e.0 += 1;
            if b.get(j) == Some(&b'{') {
                e.1 += 1;
            }
            if name == "begin"
                && b.get(j) == Some(&b'{')
                && let Some(close) = src[j..].find('}')
            {
                let env = &src[j + 1..j + close];
                if !env.is_empty() && !env.contains(['\n', '\\']) {
                    *envs.entry(env.to_string()).or_default() += 1;
                }
            }
        }
        i = j;
    }
    let commands = cmds
        .into_iter()
        .map(|(name, (n, braced))| {
            let template = match known.get(name) {
                Some(t) => t.to_string(),
                None if braced * 2 > n => format!("{name}{{}}"),
                None => name.to_string(),
            };
            (template, n)
        })
        .collect();
    Vocab { commands, envs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contexts() {
        assert_eq!(context("text \\sec"), Some(Context::Command("sec".into())));
        assert_eq!(context("\\begin{fi"), Some(Context::Environment("fi".into())));
        assert_eq!(context("see \\ref{fig:"), Some(Context::Label("fig:".into())));
        assert_eq!(context("\\cite{a, kn"), Some(Context::Citation("kn".into())));
        assert_eq!(context("\\includegraphics[width=3cm]{img/"), Some(Context::File("img/".into())));
        assert_eq!(context("\\textbf{bold"), None);
        assert_eq!(context("plain"), None);
    }

    #[test]
    fn ranking() {
        let r = rank("sec", COMMANDS.iter().copied(), 5);
        assert_eq!(r[0], "section{}");
        assert!(r.contains(&"subsection{}"));
    }

    #[test]
    fn usage_moves_items_up() {
        let r = rank_by("s", COMMANDS.iter().copied(), |s| if s == "subsection{}" { 5 } else { 0 }, 5);
        assert_eq!(r[0], "subsection{}");
        // `\int` is complete: no `\iint`/`\printbibliography`, so Enter stays a line break
        let r = rank("int", COMMANDS.iter().copied(), 12);
        assert!(r.is_empty(), "{r:?}");
        let r = rank("lef", COMMANDS.iter().copied(), 12);
        assert!(r.contains(&"left") && r.contains(&"leftarrow"));
    }

    #[test]
    fn math_mode_at_cursor() {
        assert!(!in_math("Some text \\be"));
        assert!(in_math("text $a + \\be"));
        assert!(!in_math("text $a$ and \\be"));
        assert!(in_math("\\begin{align}\n x &= \\fr"));
        assert!(!in_math("\\begin{align}x\\end{align}\n\\se"));
        assert!(in_math("\\[ x \\le"));
        assert!(!in_math("\\[ x \\] \\le"));
        assert!(!in_math("$x \\text{for all \\te"));
        assert!(in_math("$x \\text{a} + \\al"));
        assert!(!in_math("costs 5\\$ and \\se"));
        assert!(!in_math("% $ comment\n\\se"));
    }

    #[test]
    fn scores_follow_mode_and_habits() {
        let r = |prefix: &str, math: bool, user: &dyn Fn(&str) -> u32| {
            rank_by(prefix, COMMANDS.iter().copied(), |s| command_score(s, user(s), 0, math), 5).into_iter().map(String::from).collect::<Vec<_>>()
        };
        let none = |_: &str| 0;
        // in text `\\be` means \\begin, in math \\beta
        assert_eq!(r("be", false, &none)[0], "begin{}");
        assert_eq!(r("be", true, &none)[0], "beta");
        assert_eq!(r("fr", true, &none)[0], "frac{}{}");
        assert_eq!(r("se", false, &none)[0], "section{}");
        // someone who writes \\leqslant gets it before \\leq
        assert_eq!(r("le", true, &none)[0], "leq");
        let slant = |s: &str| if s == "leqslant" { 3 } else if s == "leq" { 1 } else { 0 };
        assert_eq!(r("le", true, &slant)[0], "leqslant");
        // \\ddot and \\dot used a lot
        let dots = |s: &str| match s { "ddot{}" => 6, "dot{}" => 4, _ => 0 };
        assert_eq!(r("d", true, &dots)[..2], ["ddot{}".to_string(), "dot{}".to_string()]);
    }

    #[test]
    fn vocabulary_counts_document() {
        let src = "\\newcommand{\\vect}[1]{\\mathbf{#1}}\n$\\vect{a} + \\vect{b} + \\R$ % \\ignored\n\\begin{myenv}\\end{myenv}\\begin{itemize}\\se";
        let skip = src.rfind("\\se").map(|p| p + 1);
        let v = vocabulary(src, skip);
        assert_eq!(v.commands.get("vect{}"), Some(&3));
        assert_eq!(v.commands.get("R"), Some(&1));
        assert_eq!(v.commands.get("mathbf{}"), Some(&1));
        assert!(!v.commands.contains_key("ignored"));
        assert!(!v.commands.contains_key("se"), "the word being typed is not vocabulary");
        assert_eq!(v.envs.get("myenv"), Some(&1));
        assert_eq!(v.envs.get("itemize"), Some(&1));
    }
}
