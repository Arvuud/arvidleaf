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
