//! LaTeX tokenizer for syntax highlighting.
//!
//! Produces byte ranges with a token class. Ranges always start and end on
//! ASCII positions (or the end of input), so they are valid `str` boundaries.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Text,
    /// `\section`, `\textbf`, `\%`
    Command,
    /// Environment name in `\begin{…}` / `\end{…}`
    Env,
    /// Everything inside `$…$`, `\[…\]`, `equation`, …
    Math,
    Comment,
    /// Argument of `\label`, `\ref`, `\cite`, `\usepackage`, …
    Arg,
    /// `[…]` directly after a command
    OptArg,
    /// `{ } [ ]`
    Brace,
    /// `&`, `~`, `\\`, `#`, `^`, `_`
    Special,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub tok: Tok,
}

/// Commands whose first `{…}` argument is a name/key/path rather than text.
pub const ARG_CMDS: &[&str] = &[
    "label", "ref", "eqref", "pageref", "autoref", "cref", "Cref", "nameref", "vref", "cite", "citep", "citet", "citeauthor", "citeyear", "parencite",
    "textcite", "autocite", "footcite", "fullcite", "nocite", "input", "include", "includeonly", "includegraphics", "usepackage", "RequirePackage",
    "documentclass", "bibliography", "bibliographystyle", "addbibresource", "includepdf", "subfile", "usetikzlibrary", "usetheme", "usecolortheme",
    "url", "href", "graphicspath", "newcommand", "renewcommand", "providecommand", "newenvironment", "renewenvironment", "setlength", "addtolength",
    "hypersetup", "lstinputlisting", "inputminted", "usepgfplotslibrary",
];

pub const MATH_ENVS: &[&str] = &[
    "equation", "equation*", "align", "align*", "gather", "gather*", "multline", "multline*", "flalign", "flalign*", "alignat", "alignat*", "eqnarray",
    "eqnarray*", "math", "displaymath", "dmath", "dmath*",
];

/// Environments whose body is shown literally (no commands, no comments).
pub const VERBATIM_ENVS: &[&str] = &["verbatim", "verbatim*", "Verbatim", "lstlisting", "minted", "comment", "filecontents", "filecontents*"];

enum Math {
    Dollar,
    DDollar,
    Paren,
    Bracket,
    Env(String),
}

struct Out {
    spans: Vec<Span>,
}

impl Out {
    fn push(&mut self, start: usize, end: usize, tok: Tok) {
        if end <= start {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.tok == tok && last.range.end == start {
                last.range.end = end;
                return;
            }
        self.spans.push(Span { range: start..end, tok });
    }
}

fn is_letter(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'@'
}

/// Length in bytes of the UTF-8 character starting at `b[i]`.
fn char_len(b: &[u8], i: usize) -> usize {
    match b[i] {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
    .min(b.len() - i)
}

/// Index of the byte after the group closing the bracket at `b[i]`
/// (`{`/`}` or `[`/`]`), or `None` if it is not closed within `limit` bytes.
fn match_group(b: &[u8], i: usize, open: u8, close: u8, limit: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut j = i;
    let end = b.len().min(i + limit);
    while j < end {
        match b[j] {
            b'\\' => {
                j += 2;
                continue;
            }
            b'%' if open == b'{' => {
                while j < end && b[j] != b'\n' {
                    j += 1;
                }
                continue;
            }
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn skip_inline_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    i
}

/// Tokenizes a complete LaTeX source.
pub fn tokenize(src: &str) -> Vec<Span> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Out { spans: Vec::with_capacity(n / 6 + 4) };
    let mut i = 0;
    let mut math: Option<Math> = None;
    while i < n {
        if let Some(m) = &math {
            // ---- inside math: everything is Math until the closing delimiter
            let start = i;
            let mut close: Option<(usize, usize)> = None; // (math end, resume)
            let mut env_end: Option<usize> = None;
            while i < n {
                let c = b[i];
                if c == b'%' {
                    break;
                }
                if c == b'\n' && matches!(m, Math::Dollar | Math::Paren) {
                    // a blank line ends a paragraph: inline math cannot continue
                    let k = skip_inline_ws(b, i + 1);
                    if k < n && b[k] == b'\n' {
                        close = Some((i, i));
                        break;
                    }
                }
                if c == b'\\' && i + 1 < n {
                    let d = b[i + 1];
                    if (d == b')' && matches!(m, Math::Paren)) || (d == b']' && matches!(m, Math::Bracket)) {
                        close = Some((i + 2, i + 2));
                        break;
                    }
                    if is_letter(d) {
                        let mut j = i + 1;
                        while j < n && is_letter(b[j]) {
                            j += 1;
                        }
                        if let Math::Env(e) = m
                            && &src[i + 1..j] == "end" {
                                let k = skip_inline_ws(b, j);
                                if k < n && b[k] == b'{'
                                    && let Some(e2) = match_group(b, k, b'{', b'}', 200)
                                        && &src[k + 1..e2 - 1] == e {
                                            env_end = Some(i);
                                            break;
                                        }
                            }
                        i = j;
                        continue;
                    }
                    i += 1 + char_len(b, i + 1);
                    continue;
                }
                if c == b'$' {
                    match m {
                        Math::Dollar => {
                            close = Some((i + 1, i + 1));
                            break;
                        }
                        Math::DDollar if i + 1 < n && b[i + 1] == b'$' => {
                            close = Some((i + 2, i + 2));
                            break;
                        }
                        _ => {}
                    }
                }
                i += 1;
            }
            if let Some((end, resume)) = close {
                out.push(start, end, Tok::Math);
                i = resume;
                math = None;
            } else if let Some(at) = env_end {
                out.push(start, at, Tok::Math);
                i = at;
                math = None;
            } else {
                out.push(start, i, Tok::Math);
                if i < n && b[i] == b'%' {
                    let e = src[i..].find('\n').map_or(n, |k| i + k);
                    out.push(i, e, Tok::Comment);
                    i = e;
                }
            }
            continue;
        }

        let c = b[i];
        match c {
            b'%' => {
                let e = src[i..].find('\n').map_or(n, |k| i + k);
                out.push(i, e, Tok::Comment);
                i = e;
            }
            b'$' => {
                if i + 1 < n && b[i + 1] == b'$' {
                    out.push(i, i + 2, Tok::Math);
                    i += 2;
                    math = Some(Math::DDollar);
                } else {
                    out.push(i, i + 1, Tok::Math);
                    i += 1;
                    math = Some(Math::Dollar);
                }
            }
            b'\\' if i + 1 < n => {
                let d = b[i + 1];
                if is_letter(d) {
                    let mut j = i + 1;
                    while j < n && is_letter(b[j]) {
                        j += 1;
                    }
                    let name = &src[i + 1..j];
                    if j < n && b[j] == b'*' {
                        j += 1;
                    }
                    out.push(i, j, Tok::Command);
                    i = j;
                    if name == "verb" || name == "lstinline" {
                        // \verb|…| (any delimiter, same line)
                        if i < n && b[i] != b'{' && !b[i].is_ascii_alphabetic() && b[i] != b'\n' {
                            let d = b[i];
                            if let Some(e) = src[i + 1..].find(|c: char| c as u32 == d as u32 || c == '\n') {
                                let end = i + 1 + e;
                                let end = if b[end] == d { end + 1 } else { end };
                                out.push(i, end, Tok::Text);
                                i = end;
                            }
                        }
                    } else if name == "begin" || name == "end" {
                        let k = skip_inline_ws(b, i);
                        if k < n && b[k] == b'{'
                            && let Some(e) = match_group(b, k, b'{', b'}', 200) {
                                out.push(i, k, Tok::Text);
                                out.push(k, k + 1, Tok::Brace);
                                out.push(k + 1, e - 1, Tok::Env);
                                out.push(e - 1, e, Tok::Brace);
                                let env = &src[k + 1..e - 1];
                                i = e;
                                if name == "begin" && VERBATIM_ENVS.contains(&env) {
                                    // options, then the literal body up to \end{env}
                                    let env = env.to_string();
                                    if i < n && b[i] == b'['
                                        && let Some(e) = match_group(b, i, b'[', b']', 4000) {
                                            out.push(i, e, Tok::OptArg);
                                            i = e;
                                        }
                                    if env == "minted" && i < n && b[i] == b'{'
                                        && let Some(e) = match_group(b, i, b'{', b'}', 200) {
                                            out.push(i, i + 1, Tok::Brace);
                                            out.push(i + 1, e - 1, Tok::Arg);
                                            out.push(e - 1, e, Tok::Brace);
                                            i = e;
                                        }
                                    let end_pat = format!("\\end{{{env}}}");
                                    let body_end = src[i..].find(&end_pat).map_or(n, |p| i + p);
                                    out.push(i, body_end, if env == "comment" { Tok::Comment } else { Tok::Text });
                                    i = body_end;
                                } else if name == "begin" && MATH_ENVS.contains(&env) {
                                    math = Some(Math::Env(env.to_string()));
                                } else if name == "begin" && i < n && b[i] == b'['
                                    && let Some(e) = match_group(b, i, b'[', b']', 4000) {
                                        out.push(i, e, Tok::OptArg);
                                        i = e;
                                    }
                            }
                    } else if ARG_CMDS.contains(&name) {
                        // [opt] {arg}: only the first brace group is a key/path
                        loop {
                            let k = skip_inline_ws(b, i);
                            if k < n && b[k] == b'['
                                && let Some(e) = match_group(b, k, b'[', b']', 4000) {
                                    out.push(i, k, Tok::Text);
                                    out.push(k, e, Tok::OptArg);
                                    i = e;
                                    continue;
                                }
                            if k < n && b[k] == b'{'
                                && let Some(e) = match_group(b, k, b'{', b'}', 4000) {
                                    out.push(i, k, Tok::Text);
                                    out.push(k, k + 1, Tok::Brace);
                                    out.push(k + 1, e - 1, Tok::Arg);
                                    out.push(e - 1, e, Tok::Brace);
                                    i = e;
                                }
                            break;
                        }
                    } else if i < n && b[i] == b'['
                        && let Some(e) = match_group(b, i, b'[', b']', 4000) {
                            out.push(i, e, Tok::OptArg);
                            i = e;
                        }
                } else if d == b'(' {
                    out.push(i, i + 2, Tok::Math);
                    i += 2;
                    math = Some(Math::Paren);
                } else if d == b'[' {
                    out.push(i, i + 2, Tok::Math);
                    i += 2;
                    math = Some(Math::Bracket);
                } else if d == b'\\' {
                    let mut e = i + 2;
                    if e < n && b[e] == b'*' {
                        e += 1;
                    }
                    out.push(i, e, Tok::Special);
                    i = e;
                    if i < n && b[i] == b'['
                        && let Some(e) = match_group(b, i, b'[', b']', 200) {
                            out.push(i, e, Tok::OptArg);
                            i = e;
                        }
                } else {
                    let e = i + 1 + char_len(b, i + 1);
                    out.push(i, e, Tok::Command);
                    i = e;
                }
            }
            b'{' | b'}' | b'[' | b']' => {
                out.push(i, i + 1, Tok::Brace);
                i += 1;
            }
            b'&' | b'~' | b'#' | b'^' | b'_' => {
                out.push(i, i + 1, Tok::Special);
                i += 1;
            }
            _ => {
                // run of plain text up to the next interesting byte
                let mut j = i + 1;
                while j < n && !matches!(b[j], b'%' | b'$' | b'\\' | b'{' | b'}' | b'[' | b']' | b'&' | b'~' | b'#' | b'^' | b'_') {
                    j += 1;
                }
                out.push(i, j, Tok::Text);
                i = j;
            }
        }
    }
    out.spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<(&str, Tok)> {
        tokenize(src).into_iter().map(|s| (&src[s.range], s.tok)).collect()
    }

    #[test]
    fn covers_input_exactly() {
        for src in ["", "a", "\\", "$", "\\begin{equation} x \\end{equation}", "ä\\ö $ü$ % ß", "{[}]", "\\begin{align*}a&=b\\\\c\\end{align*}"] {
            let spans = tokenize(src);
            let mut pos = 0;
            for s in &spans {
                assert_eq!(s.range.start, pos, "gap in {src:?}");
                pos = s.range.end;
            }
            assert_eq!(pos, src.len(), "{src:?}");
        }
    }

    #[test]
    fn commands_and_comments() {
        let t = toks("\\section{Intro} % note\n50\\% done");
        assert_eq!(t[0], ("\\section", Tok::Command));
        assert!(t.contains(&("% note", Tok::Comment)));
        assert!(t.contains(&("\\%", Tok::Command)), "escaped percent is not a comment: {t:?}");
        assert!(t.contains(&(" done", Tok::Text)));
    }

    #[test]
    fn environments() {
        let t = toks("\\begin{figure}[htbp]\\end{figure}");
        assert_eq!(t[0], ("\\begin", Tok::Command));
        assert_eq!(t[2], ("figure", Tok::Env));
        assert!(t.contains(&("[htbp]", Tok::OptArg)));
    }

    #[test]
    fn math_modes() {
        let t = toks("a $x^2 + \\alpha$ b");
        assert_eq!(t[1], ("$x^2 + \\alpha$", Tok::Math));
        assert_eq!(t[2], (" b", Tok::Text));
        let t = toks("\\[ a \\] c");
        assert_eq!(t.last().unwrap(), &(" c", Tok::Text));
        let t = toks("\\begin{align}\n a &= b \\\\\n\\end{align} z");
        assert!(t.iter().any(|(s, k)| *k == Tok::Math && s.contains("a &= b")));
        assert_eq!(t.last().unwrap(), &(" z", Tok::Text));
        assert!(t.contains(&("align", Tok::Env)));
    }

    #[test]
    fn escaped_dollar_and_blank_line_end_math() {
        let t = toks("cost \\$5 and $a\n\nnext");
        assert!(t.contains(&("\\$", Tok::Command)));
        assert_eq!(t.last().unwrap(), &("\n\nnext", Tok::Text), "{t:?}");
    }

    #[test]
    fn arguments() {
        let t = toks("\\usepackage[utf8]{inputenc}\\ref{fig:a}\\cite{knuth}");
        assert!(t.contains(&("[utf8]", Tok::OptArg)));
        assert!(t.contains(&("inputenc", Tok::Arg)));
        assert!(t.contains(&("fig:a", Tok::Arg)));
        assert!(t.contains(&("knuth", Tok::Arg)));
    }

    #[test]
    fn verbatim_is_literal() {
        let t = toks("\\begin{verbatim}50% \\x $\n\\end{verbatim} $a$");
        assert!(t.contains(&("50% \\x $\n", Tok::Text)), "{t:?}");
        assert!(t.contains(&("$a$", Tok::Math)), "{t:?}");
        let t = toks("\\verb|%$\\| after % c");
        assert!(t[1].0.starts_with("|%$\\|") && t[1].1 == Tok::Text, "{t:?}");
        assert!(t.contains(&("% c", Tok::Comment)), "{t:?}");
        let t = toks("\\begin{minted}{python}\n# x % y\n\\end{minted}");
        assert!(t.contains(&("python", Tok::Arg)), "{t:?}");
        assert!(t.contains(&("\n# x % y\n", Tok::Text)), "{t:?}");
    }

    #[test]
    fn comment_inside_math() {
        let t = toks("$a % c\n b$ x");
        assert!(t.contains(&("% c", Tok::Comment)));
        assert_eq!(t.last().unwrap(), &(" x", Tok::Text));
    }
}
