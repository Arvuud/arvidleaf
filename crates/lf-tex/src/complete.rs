//! Completion candidates: commands, environments, and the context at the cursor.

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
    let lower = prefix.to_ascii_lowercase();
    let mut starts = Vec::new();
    let mut contains = Vec::new();
    for it in items {
        let name = it.split('{').next().unwrap_or(it);
        if name == prefix {
            continue;
        }
        let l = name.to_ascii_lowercase();
        if name.starts_with(prefix) {
            starts.push(it);
        } else if l.starts_with(&lower) || (lower.len() >= 2 && l.contains(&lower)) {
            contains.push(it);
        }
    }
    starts.sort_by_key(|s| s.len());
    let mut seen = std::collections::HashSet::new();
    starts.into_iter().chain(contains).filter(|s| seen.insert(*s)).take(limit).collect()
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
}
