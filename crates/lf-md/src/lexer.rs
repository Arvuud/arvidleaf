//! Markdown tokens for the editor's syntax colors. Line based and forgiving:
//! it only has to look right, not to parse exactly. The spans cover the
//! whole text without gaps.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdTok {
    Text,
    /// `# Heading` (the whole line)
    Heading,
    /// `*`, `**`, `_`, `~~`, list bullets, `>`, `[ ]`, `---`
    Marker,
    /// `` `code` `` and fenced code blocks
    Code,
    /// `[text](url)`: brackets and target
    Link,
    /// `$…$`, `$$…$$` and raw `\begin…\end` blocks: highlight as LaTeX
    Math,
    /// `\command` in running text
    Command,
    /// front matter and HTML comments
    Meta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdSpan {
    pub range: Range<usize>,
    pub tok: MdTok,
}

struct Out {
    spans: Vec<MdSpan>,
}

impl Out {
    fn push(&mut self, a: usize, b: usize, tok: MdTok) {
        if a >= b {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.tok == tok
            && last.range.end == a
        {
            last.range.end = b;
            return;
        }
        self.spans.push(MdSpan { range: a..b, tok });
    }
}

pub fn tokenize(src: &str) -> Vec<MdSpan> {
    let mut out = Out { spans: Vec::with_capacity(src.len() / 8 + 4) };
    let mut fence: Option<&str> = None;
    let mut front = src.starts_with("---\n") || src.starts_with("---\r\n");
    let mut first = true;
    let mut display_math = false;
    let mut raw_env: Option<String> = None;
    let mut off = 0;
    for line in src.split_inclusive('\n') {
        let start = off;
        off += line.len();
        let end = off;
        let content = line.trim_end_matches(['\n', '\r']);
        let t = content.trim_start();
        let indent = content.len() - t.len();
        if front {
            out.push(start, end, MdTok::Meta);
            if !first && matches!(content.trim_end(), "---" | "...") {
                front = false;
            }
            first = false;
            continue;
        }
        first = false;
        if let Some(f) = fence {
            out.push(start, end, MdTok::Code);
            if t.starts_with(f) {
                fence = None;
            }
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(&t[..3]);
            out.push(start, end, MdTok::Code);
            continue;
        }
        if let Some(env) = &raw_env {
            out.push(start, end, MdTok::Math);
            if content.contains(&format!("\\end{{{env}}}")) {
                raw_env = None;
            }
            continue;
        }
        if display_math {
            match content.find("$$") {
                Some(p) => {
                    out.push(start, start + p + 2, MdTok::Math);
                    display_math = false;
                    inline(&mut out, src, start + p + 2, end);
                }
                None => out.push(start, end, MdTok::Math),
            }
            continue;
        }
        if indent <= 3
            && let Some(rest) = t.strip_prefix("\\begin{")
            && let Some(close) = rest.find('}')
        {
            let env = rest[..close].to_string();
            out.push(start, end, MdTok::Math);
            if !content.contains(&format!("\\end{{{env}}}")) {
                raw_env = Some(env);
            }
            continue;
        }
        if t.starts_with('#') {
            let hashes = t.chars().take_while(|c| *c == '#').count();
            if hashes <= 6 && t[hashes..].chars().next().is_none_or(|c| c == ' ') {
                out.push(start, end, MdTok::Heading);
                continue;
            }
        }
        if t.starts_with("<!--") {
            out.push(start, end, MdTok::Meta);
            continue;
        }
        let trimmed = t.trim_end();
        if trimmed.len() >= 3 && (trimmed.chars().all(|c| c == '-' || c == ' ') || trimmed.chars().all(|c| c == '*' || c == ' ') || trimmed.chars().all(|c| c == '=')) {
            out.push(start, end, MdTok::Marker);
            continue;
        }
        // block markers: quotes, bullets, numbers, task boxes
        let mut p = start + indent;
        out.push(start, p, MdTok::Text);
        loop {
            let rest = &src[p..end];
            if rest.starts_with("> ") || rest.starts_with(">\n") || rest == ">" {
                out.push(p, p + 1, MdTok::Marker);
                p += 1;
                let sp = src[p..end].len() - src[p..end].trim_start_matches(' ').len();
                out.push(p, p + sp, MdTok::Text);
                p += sp;
                continue;
            }
            break;
        }
        let rest = &src[p..end];
        let bullet = if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
            1
        } else {
            let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            if digits > 0 && (rest[digits..].starts_with(". ") || rest[digits..].starts_with(") ")) { digits + 1 } else { 0 }
        };
        if bullet > 0 {
            out.push(p, p + bullet, MdTok::Marker);
            p += bullet;
            let r = &src[p..end];
            if r.starts_with(" [ ] ") || r.starts_with(" [x] ") || r.starts_with(" [X] ") {
                out.push(p, p + 1, MdTok::Text);
                out.push(p + 1, p + 4, MdTok::Marker);
                p += 4;
            }
        }
        // a `$$` that does not close on this line opens display math
        let opens = src[p..end].matches("$$").count() % 2 == 1;
        if opens {
            let q = p + src[p..end].rfind("$$").unwrap_or(0);
            inline(&mut out, src, p, q);
            out.push(q, end, MdTok::Math);
            display_math = true;
        } else {
            inline(&mut out, src, p, end);
        }
    }
    // gaps (should not happen) are plain text
    let mut spans = Vec::with_capacity(out.spans.len());
    let mut pos = 0;
    for s in out.spans {
        if s.range.start > pos {
            spans.push(MdSpan { range: pos..s.range.start, tok: MdTok::Text });
        }
        pos = s.range.end;
        spans.push(s);
    }
    if pos < src.len() {
        spans.push(MdSpan { range: pos..src.len(), tok: MdTok::Text });
    }
    spans
}

/// Inline markup between byte offsets `a` and `b` (inside one line).
fn inline(out: &mut Out, src: &str, a: usize, b: usize) {
    let s = &src[..b];
    let bytes = s.as_bytes();
    let mut i = a;
    let mut text_from = a;
    while i < b {
        let c = bytes[i];
        let emit = |out: &mut Out, from: usize, to: usize, tok: MdTok, text_from: &mut usize| {
            out.push(*text_from, from, MdTok::Text);
            out.push(from, to, tok);
            *text_from = to;
        };
        match c {
            b'\\' => {
                let name = s[i + 1..].bytes().take_while(|c| c.is_ascii_alphabetic()).count();
                if name > 0 {
                    emit(out, i, i + 1 + name, MdTok::Command, &mut text_from);
                    i += 1 + name;
                } else {
                    // escaped character (may be multi-byte)
                    i += 1 + bytes.get(i + 1).map_or(1, |c| utf8_len(*c));
                }
                continue;
            }
            b'`' => {
                let ticks = s[i..].bytes().take_while(|c| *c == b'`').count();
                let fence = &s[i..i + ticks];
                if let Some(close) = s[i + ticks..].find(fence) {
                    let to = i + ticks + close + ticks;
                    emit(out, i, to, MdTok::Code, &mut text_from);
                    i = to;
                    continue;
                }
                i += ticks;
                continue;
            }
            b'$' => {
                let dd = s[i..].starts_with("$$");
                let delim = if dd { "$$" } else { "$" };
                let from = i + delim.len();
                if let Some(close) = find_unescaped(&s[from..], delim)
                    && (dd || close > 0)
                {
                    let to = from + close + delim.len();
                    emit(out, i, to, MdTok::Math, &mut text_from);
                    i = to;
                    continue;
                }
                i += delim.len();
                continue;
            }
            b'*' | b'_' | b'~' => {
                let run = s[i..].bytes().take_while(|x| *x == c).count();
                // `_` inside a word (snake_case) is no emphasis; one `~` is a LaTeX space
                let in_word = c == b'_' && i > 0 && bytes[i - 1].is_ascii_alphanumeric() && bytes.get(i + run).is_some_and(|x| x.is_ascii_alphanumeric());
                if !in_word && !(c == b'~' && run < 2) {
                    emit(out, i, i + run, MdTok::Marker, &mut text_from);
                }
                i += run;
                continue;
            }
            b'[' | b'!' if c == b'[' || s[i..].starts_with("![") => {
                let open = if c == b'!' { i + 1 } else { i };
                if let Some(close) = s[open..].find("](").map(|p| open + p)
                    && let Some(end) = s[close..].find(')').map(|p| close + p + 1)
                {
                    emit(out, i, open + 1, MdTok::Link, &mut text_from);
                    // link text keeps inline markup
                    inline(out, src, open + 1, close);
                    text_from = close;
                    emit(out, close, end, MdTok::Link, &mut text_from);
                    i = end;
                    continue;
                }
                i += 1;
                continue;
            }
            b'<' if s[i..].starts_with("<!--") => {
                let to = s[i..].find("-->").map_or(b, |p| i + p + 3);
                emit(out, i, to, MdTok::Meta, &mut text_from);
                i = to;
                continue;
            }
            _ => {}
        }
        // skip a whole UTF-8 character
        i += utf8_len(c);
    }
    out.push(text_from, b, MdTok::Text);
}

fn utf8_len(first: u8) -> usize {
    match first {
        0xF0..=0xFF => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}

fn find_unescaped(s: &str, pat: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(p) = s[from..].find(pat) {
        let at = from + p;
        if at == 0 || s.as_bytes()[at - 1] != b'\\' {
            return Some(at);
        }
        from = at + pat.len();
    }
    None
}
