//! BibLaTeX files as a list of blocks.
//!
//! A file is split into [`Block`]s: entries, `@string`, `@preamble`, `@comment`, the free
//! text between them, and entries that could not be parsed. Every block keeps the text it
//! was parsed from (`raw`). Writing a file concatenates the raw texts, so an unchanged file
//! is reproduced exactly. Only blocks that were changed or added (`raw == None`) are
//! formatted anew by [`write`].

mod parse;
mod write;

use std::collections::HashMap;

pub use parse::parse;
pub use write::{format_entry, format_value};

/// A parsed `.bib` file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bib {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Original text of the block. `None` once the block was changed or newly added.
    pub raw: Option<String>,
    /// 1-based line in the parsed file where the block starts (0 for new blocks).
    pub line: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Text outside of entries (whitespace, `%` comments, notes). Ignored by BibTeX/biber.
    Text,
    /// `@comment{…}`
    Comment,
    /// `@preamble{…}`
    Preamble(Value),
    /// `@string{name = value}`
    String { name: String, value: Value },
    Entry(Entry),
    /// Starts like an entry but is malformed. Kept as raw text so nothing is lost.
    Broken { message: String },
}

/// A reference entry, e.g. `@article{key, author = {…}, …}`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Entry {
    /// Entry type as written (`article`, `Book`, …). Compare with [`Entry::is`].
    pub kind: String,
    pub key: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Field name as written. Lookups ignore case.
    pub name: String,
    pub value: Value,
}

/// A field value: one or more pieces joined with `#`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Value(pub Vec<Piece>);

#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    /// `{…}` (content without the outer braces)
    Braced(String),
    /// `"…"` (content without the quotes)
    Quoted(String),
    /// `1984`
    Number(String),
    /// `jan`, `cup` – a `@string` name
    Macro(String),
}

/// Strips spaces/tabs and one line break from the start (only if the line is blank).
fn strip_line_end(s: &str) -> &str {
    let t = s.trim_start_matches([' ', '\t']);
    t.strip_prefix("\r\n").or_else(|| t.strip_prefix('\n')).unwrap_or(if t.is_empty() { t } else { s })
}

/// The month macros predefined by BibTeX and biber.
const MONTHS: [(&str, &str); 12] = [
    ("jan", "1"),
    ("feb", "2"),
    ("mar", "3"),
    ("apr", "4"),
    ("may", "5"),
    ("jun", "6"),
    ("jul", "7"),
    ("aug", "8"),
    ("sep", "9"),
    ("oct", "10"),
    ("nov", "11"),
    ("dec", "12"),
];

/// `@string` definitions, names in lower case.
pub type Strings = HashMap<String, String>;

impl Value {
    /// A single braced piece – the usual form for new values.
    pub fn text(s: impl Into<String>) -> Value {
        Value(vec![Piece::Braced(s.into())])
    }

    /// The value as LaTeX source, with `@string` macros replaced. Unknown macros are kept
    /// by name (BibTeX would warn and use an empty string; showing the name is more useful).
    pub fn resolve(&self, strings: &Strings) -> String {
        let mut out = String::new();
        for p in &self.0 {
            match p {
                Piece::Braced(s) | Piece::Quoted(s) | Piece::Number(s) => out.push_str(s),
                Piece::Macro(m) => {
                    let l = m.to_ascii_lowercase();
                    match strings.get(&l) {
                        Some(v) => out.push_str(v),
                        None => match MONTHS.iter().find(|(n, _)| *n == l) {
                            Some((_, n)) => out.push_str(n),
                            None => out.push_str(m),
                        },
                    }
                }
            }
        }
        out
    }
}

impl Entry {
    pub fn new(kind: impl Into<String>, key: impl Into<String>) -> Entry {
        Entry { kind: kind.into(), key: key.into(), fields: Vec::new() }
    }

    /// Case-insensitive comparison of the entry type.
    pub fn is(&self, kind: &str) -> bool {
        self.kind.eq_ignore_ascii_case(kind)
    }

    /// First field with this name (case-insensitive). Duplicates are kept in the file, but
    /// like biber the first one counts.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|f| f.name.eq_ignore_ascii_case(name)).map(|f| &f.value)
    }

    /// Field as LaTeX source with macros resolved.
    pub fn raw_field(&self, name: &str, strings: &Strings) -> Option<String> {
        self.get(name).map(|v| v.resolve(strings))
    }

    /// Field as plain Unicode text (for display and search).
    pub fn text_field(&self, name: &str, strings: &Strings) -> Option<String> {
        self.raw_field(name, strings).map(|s| crate::latex::decode(&s))
    }

    /// Sets a field, replacing the first one with this name or appending a new one.
    pub fn set(&mut self, name: &str, value: Value) {
        match self.fields.iter_mut().find(|f| f.name.eq_ignore_ascii_case(name)) {
            Some(f) => f.value = value,
            None => self.fields.push(Field { name: name.to_ascii_lowercase(), value }),
        }
    }

    /// Removes all fields with this name. Returns whether one existed.
    pub fn remove(&mut self, name: &str) -> bool {
        let n = self.fields.len();
        self.fields.retain(|f| !f.name.eq_ignore_ascii_case(name));
        n != self.fields.len()
    }
}

impl Bib {
    /// Writes the file: raw text for unchanged blocks, formatted text for changed ones.
    pub fn to_bib_string(&self) -> String {
        let mut out = String::new();
        for b in &self.blocks {
            match &b.raw {
                Some(r) => out.push_str(r),
                None => write::format_block(&b.kind, &mut out),
            }
        }
        out
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.blocks.iter().filter_map(|b| if let Kind::Entry(e) = &b.kind { Some(e) } else { None })
    }

    /// Entry keys in file order.
    pub fn keys(&self) -> Vec<String> {
        self.entries().map(|e| e.key.clone()).collect()
    }

    /// Index of the block holding the entry with this key (keys compare case-insensitively,
    /// as in biber).
    pub fn find(&self, key: &str) -> Option<usize> {
        self.blocks.iter().position(|b| matches!(&b.kind, Kind::Entry(e) if e.key.eq_ignore_ascii_case(key)))
    }

    pub fn entry(&self, key: &str) -> Option<&Entry> {
        self.find(key).and_then(|i| if let Kind::Entry(e) = &self.blocks[i].kind { Some(e) } else { None })
    }

    /// The entry as source: as written in the file if unchanged, else formatted.
    pub fn source(&self, key: &str) -> Option<String> {
        let b = &self.blocks[self.find(key)?];
        match (&b.raw, &b.kind) {
            (Some(raw), _) => Some(raw.trim().to_string()),
            (None, Kind::Entry(e)) => Some(format_entry(e).trim_end().to_string()),
            _ => None,
        }
    }

    /// 1-based line where the entry starts in the parsed file.
    pub fn entry_line(&self, key: &str) -> Option<usize> {
        self.find(key).map(|i| self.blocks[i].line)
    }

    /// `@string` definitions of this file (later definitions win, as in BibTeX).
    pub fn strings(&self) -> Strings {
        let mut map = Strings::new();
        for b in &self.blocks {
            if let Kind::String { name, value } = &b.kind {
                let v = value.resolve(&map);
                map.insert(name.to_ascii_lowercase(), v);
            }
        }
        map
    }

    /// Malformed blocks: (line, message).
    pub fn problems(&self) -> Vec<(usize, String)> {
        self.blocks.iter().filter_map(|b| if let Kind::Broken { message } = &b.kind { Some((b.line, message.clone())) } else { None }).collect()
    }

    /// Replaces the entry in block `idx`. The block is written formatted from now on.
    pub fn replace_entry(&mut self, idx: usize, entry: Entry) {
        let b = &mut self.blocks[idx];
        b.kind = Kind::Entry(entry);
        b.raw = None;
    }

    /// Changes an entry in place; only that entry is re-formatted.
    pub fn edit_entry(&mut self, key: &str, f: impl FnOnce(&mut Entry)) -> bool {
        let Some(i) = self.find(key) else { return false };
        if let Kind::Entry(e) = &mut self.blocks[i].kind {
            f(e);
            self.blocks[i].raw = None;
        }
        true
    }

    /// Appends an entry at the end, separated by a blank line.
    pub fn push_entry(&mut self, entry: Entry) {
        let text = self.to_tail();
        let sep = if text.is_empty() || text.ends_with("\n\n") {
            ""
        } else if text.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        if !sep.is_empty() {
            self.blocks.push(Block { raw: Some(sep.to_string()), line: 0, kind: Kind::Text });
        }
        self.blocks.push(Block { raw: None, line: 0, kind: Kind::Entry(entry) });
        self.blocks.push(Block { raw: Some("\n".to_string()), line: 0, kind: Kind::Text });
    }

    /// Removes the entry with this key. The rest of its line goes too, and so does one blank
    /// line, so the remaining entries keep their spacing.
    pub fn remove_entry(&mut self, key: &str) -> Option<Entry> {
        let i = self.find(key)?;
        let b = self.blocks.remove(i);
        // merge the text blocks that followed the entry into one
        let mut after = String::new();
        while i < self.blocks.len() && self.blocks[i].kind == Kind::Text {
            after.push_str(self.blocks.remove(i).raw.as_deref().unwrap_or(""));
        }
        let before_blank = match i.checked_sub(1).map(|p| &self.blocks[p]) {
            None => true,
            Some(Block { kind: Kind::Text, raw: Some(r), .. }) => r.ends_with("\n\n") || r.ends_with("\n\r\n") || r.trim().is_empty(),
            Some(_) => false,
        };
        // the rest of the entry's line (spaces, a trailing `% comment`)
        let mut rest = after.find('\n').map_or("", |p| &after[p + 1..]);
        if before_blank {
            rest = strip_line_end(rest);
        }
        let rest = rest.to_string();
        if !rest.is_empty() {
            self.blocks.insert(i, Block { raw: Some(rest), line: 0, kind: Kind::Text });
        } else if i == self.blocks.len()
            && let Some(Block { kind: Kind::Text, raw: Some(r), .. }) = self.blocks.last_mut()
        {
            // entry was the last one: leave a single line break at the end
            while r.ends_with("\n\n") || r.ends_with("\n\r\n") {
                let cut = if r.ends_with("\r\n") { 2 } else { 1 };
                r.truncate(r.len() - cut);
            }
        }
        if let Some(Block { kind: Kind::Text, raw: Some(r), .. }) = self.blocks.get(i)
            && r.is_empty()
        {
            self.blocks.remove(i);
        }
        if let Kind::Entry(e) = b.kind { Some(e) } else { None }
    }

    /// The last two characters of the written file (enough to decide on separators).
    fn to_tail(&self) -> String {
        let mut tail = String::new();
        for b in self.blocks.iter().rev() {
            let s = match &b.raw {
                Some(r) => r.clone(),
                None => {
                    let mut s = String::new();
                    write::format_block(&b.kind, &mut s);
                    s
                }
            };
            tail.insert_str(0, &s);
            if tail.len() >= 2 {
                break;
            }
        }
        tail
    }
}
