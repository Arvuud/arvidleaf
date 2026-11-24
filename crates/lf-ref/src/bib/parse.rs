//! Fault-tolerant, lossless BibLaTeX parser.
//!
//! Rules follow BibTeX/biber:
//! - Text outside `@type{…}` is ignored (kept as [`Kind::Text`]).
//! - Entries use `{…}` or `(…)`. Values are `{…}`, `"…"`, numbers or `@string` names,
//!   joined with `#`. Braces must balance (also `\{`, as in BibTeX); in `"…"` a quote at
//!   brace depth 0 ends the value.
//! - `%` comments are allowed between fields (biber accepts them).
//!
//! A malformed entry becomes [`Kind::Broken`] up to the next line that starts a new entry,
//! so one bad entry never hides the rest of the file. As an extra safeguard against a
//! missing closing brace swallowing the following entries, a line that starts with
//! `@type{` inside a value counts as an error.

use super::{Bib, Block, Entry, Field, Kind, Piece, Value};

pub fn parse(src: &str) -> Bib {
    let mut p = P { s: src, b: src.as_bytes(), i: 0 };
    let mut lines = LineCounter { pos: 0, line: 1 };
    let mut blocks = Vec::new();
    let mut text_start = 0;
    let mut pos = 0;
    while let Some(off) = src[pos..].find('@') {
        let at = pos + off;
        let Some((kind, open)) = entry_start(p.b, at) else {
            pos = at + 1;
            continue;
        };
        if text_start < at {
            blocks.push(Block { raw: Some(src[text_start..at].to_string()), line: lines.at(src, text_start), kind: Kind::Text });
        }
        let line = lines.at(src, at);
        p.i = open + 1;
        let close = if p.b[open] == b'{' { b'}' } else { b')' };
        let parsed = match kind.to_ascii_lowercase().as_str() {
            "comment" => p.comment(open),
            "preamble" => p.preamble(close),
            "string" => p.string(close),
            _ => p.entry(kind, close),
        };
        let end = match parsed {
            Ok(k) => {
                blocks.push(Block { raw: Some(src[at..p.i].to_string()), line, kind: k });
                p.i
            }
            Err(message) => {
                let end = recover(p.b, at);
                blocks.push(Block { raw: Some(src[at..end].to_string()), line, kind: Kind::Broken { message } });
                end
            }
        };
        text_start = end;
        pos = end;
    }
    if text_start < src.len() {
        blocks.push(Block { raw: Some(src[text_start..].to_string()), line: lines.at(src, text_start), kind: Kind::Text });
    }
    Bib { blocks }
}

/// Counts lines incrementally (positions are requested in increasing order).
struct LineCounter {
    pos: usize,
    line: usize,
}

impl LineCounter {
    fn at(&mut self, src: &str, pos: usize) -> usize {
        if pos >= self.pos {
            self.line += src.as_bytes()[self.pos..pos].iter().filter(|&&c| c == b'\n').count();
            self.pos = pos;
        }
        self.line
    }
}

/// `@type{` or `@type (` at `at`: returns the type and the position of the delimiter.
fn entry_start(b: &[u8], at: usize) -> Option<(&str, usize)> {
    let mut j = at + 1;
    while j < b.len() && matches!(b[j], b' ' | b'\t') {
        j += 1;
    }
    let ts = j;
    if j >= b.len() || !b[j].is_ascii_alphabetic() {
        return None;
    }
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
        j += 1;
    }
    let te = j;
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    if j < b.len() && (b[j] == b'{' || b[j] == b'(') {
        // the type is ASCII, so this slice is on char boundaries
        Some((std::str::from_utf8(&b[ts..te]).unwrap_or(""), j))
    } else {
        None
    }
}

/// After a newline at `nl`: does the next line start a new entry?
fn entry_line_after(b: &[u8], nl: usize) -> bool {
    let mut j = nl + 1;
    while j < b.len() && matches!(b[j], b' ' | b'\t') {
        j += 1;
    }
    j < b.len() && b[j] == b'@' && entry_start(b, j).is_some()
}

/// Where a broken block ends: the start of the next line that begins a new entry.
fn recover(b: &[u8], at: usize) -> usize {
    let mut j = at + 1;
    while j < b.len() {
        if b[j] == b'\n' && entry_line_after(b, j) {
            return j + 1;
        }
        j += 1;
    }
    b.len()
}

/// Characters of field names, `@string` names and macros.
fn is_ident(c: u8) -> bool {
    !(c.is_ascii_whitespace() || matches!(c, b'=' | b',' | b'{' | b'}' | b'(' | b')' | b'"' | b'#' | b'%'))
}

struct P<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    /// Skips whitespace and `%` comments.
    fn ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.i += 1;
            } else if c == b'%' {
                while self.peek().is_some_and(|c| c != b'\n') {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
    }

    fn ident(&mut self) -> &str {
        let s = self.i;
        while self.peek().is_some_and(is_ident) {
            self.i += 1;
        }
        &self.s[s..self.i]
    }

    fn expect_close(&mut self, close: u8) -> Result<(), String> {
        self.ws();
        match self.peek() {
            Some(c) if c == close => {
                self.i += 1;
                Ok(())
            }
            Some(c) => Err(format!("expected '{}' but found '{}'", close as char, self.char_at(c))),
            None => Err(format!("missing closing '{}'", close as char)),
        }
    }

    fn char_at(&self, c: u8) -> char {
        if c.is_ascii() { c as char } else { self.s[self.i..].chars().next().unwrap_or('?') }
    }

    /// `@comment{…}`: balanced braces (or parentheses).
    fn comment(&mut self, open: usize) -> Result<Kind, String> {
        let (o, c) = if self.b[open] == b'{' { (b'{', b'}') } else { (b'(', b')') };
        let mut depth = 1usize;
        while let Some(ch) = self.peek() {
            self.i += 1;
            if ch == o {
                depth += 1;
            } else if ch == c {
                depth -= 1;
                if depth == 0 {
                    return Ok(Kind::Comment);
                }
            }
        }
        Err("unterminated @comment".into())
    }

    fn preamble(&mut self, close: u8) -> Result<Kind, String> {
        let v = self.value()?;
        self.expect_close(close)?;
        Ok(Kind::Preamble(v))
    }

    fn string(&mut self, close: u8) -> Result<Kind, String> {
        self.ws();
        let name = self.ident().to_string();
        if name.is_empty() {
            return Err("@string without a name".into());
        }
        self.ws();
        if self.peek() != Some(b'=') {
            return Err(format!("expected '=' after @string name \"{name}\""));
        }
        self.i += 1;
        let value = self.value()?;
        self.expect_close(close)?;
        Ok(Kind::String { name, value })
    }

    fn entry(&mut self, kind: &str, close: u8) -> Result<Kind, String> {
        self.ws();
        let ks = self.i;
        while self.peek().is_some_and(|c| !(c.is_ascii_whitespace() || c == b',' || c == close || c == b'{' || c == b'}')) {
            self.i += 1;
        }
        let key = self.s[ks..self.i].to_string();
        if key.is_empty() {
            return Err(format!("@{kind} without a key"));
        }
        let mut e = Entry { kind: kind.to_string(), key, fields: Vec::new() };
        self.ws();
        match self.peek() {
            Some(c) if c == close => {
                self.i += 1;
                return Ok(Kind::Entry(e));
            }
            Some(b',') => self.i += 1,
            Some(c) => return Err(format!("{}: expected ',' after the key but found '{}'", e.key, self.char_at(c))),
            None => return Err(format!("{}: missing closing '{}'", e.key, close as char)),
        }
        loop {
            self.ws();
            match self.peek() {
                None => return Err(format!("{}: missing closing '{}'", e.key, close as char)),
                Some(c) if c == close => {
                    self.i += 1;
                    return Ok(Kind::Entry(e));
                }
                _ => {}
            }
            let name = self.ident().to_string();
            if name.is_empty() {
                let c = self.peek().map(|c| self.char_at(c)).unwrap_or(' ');
                return Err(format!("{}: expected a field name but found '{c}'", e.key));
            }
            self.ws();
            if self.peek() != Some(b'=') {
                return Err(format!("{}: expected '=' after field \"{name}\"", e.key));
            }
            self.i += 1;
            let value = self.value().map_err(|m| format!("{}, field \"{name}\": {m}", e.key))?;
            e.fields.push(Field { name, value });
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(c) if c == close => {
                    self.i += 1;
                    return Ok(Kind::Entry(e));
                }
                Some(c) => return Err(format!("{}: expected ',' or '{}' after field \"{}\" but found '{}'", e.key, close as char, e.fields.last().map_or("", |f| &f.name), self.char_at(c))),
                None => return Err(format!("{}: missing closing '{}'", e.key, close as char)),
            }
        }
    }

    /// One or more pieces joined with `#`.
    fn value(&mut self) -> Result<Value, String> {
        let mut pieces = Vec::new();
        loop {
            self.ws();
            let piece = match self.peek() {
                Some(b'{') => Piece::Braced(self.delimited(false)?),
                Some(b'"') => Piece::Quoted(self.delimited(true)?),
                Some(c) if c.is_ascii_digit() => {
                    let s = self.i;
                    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.i += 1;
                    }
                    Piece::Number(self.s[s..self.i].to_string())
                }
                Some(c) if is_ident(c) => Piece::Macro(self.ident().to_string()),
                Some(c) => return Err(format!("expected a value but found '{}'", self.char_at(c))),
                None => return Err("missing value".into()),
            };
            pieces.push(piece);
            self.ws();
            if self.peek() == Some(b'#') {
                self.i += 1;
            } else {
                return Ok(Value(pieces));
            }
        }
    }

    /// `{…}` or `"…"` starting at the current position; returns the content.
    fn delimited(&mut self, quoted: bool) -> Result<String, String> {
        let start = self.i + 1;
        let mut j = start;
        let mut depth = if quoted { 0usize } else { 1 };
        while j < self.b.len() {
            match self.b[j] {
                b'{' => depth += 1,
                b'}' => {
                    if depth == 0 {
                        return Err("unbalanced '}' in quoted value".into());
                    }
                    depth -= 1;
                    if depth == 0 && !quoted {
                        self.i = j + 1;
                        return Ok(self.s[start..j].to_string());
                    }
                }
                b'"' if quoted && depth == 0 => {
                    self.i = j + 1;
                    return Ok(self.s[start..j].to_string());
                }
                b'\n' if entry_line_after(self.b, j) => {
                    return Err(if quoted { "missing closing '\"'".into() } else { "unbalanced braces (missing '}')".into() });
                }
                _ => {}
            }
            j += 1;
        }
        Err(if quoted { "missing closing '\"'".into() } else { "unbalanced braces (missing '}')".into() })
    }
}
