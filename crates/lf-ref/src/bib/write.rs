//! Formatting of changed or new blocks.
//!
//! Style (as biblatex-examples.bib, without column alignment so diffs stay small):
//! ```text
//! @article{key,
//!   author = {Knuth, Donald E.},
//!   title = {Literate Programming},
//! }
//! ```

use super::{Entry, Kind, Piece, Value};

pub fn format_value(v: &Value) -> String {
    let mut out = String::new();
    for (i, p) in v.0.iter().enumerate() {
        if i > 0 {
            out.push_str(" # ");
        }
        match p {
            Piece::Braced(s) => {
                out.push('{');
                out.push_str(s);
                out.push('}');
            }
            Piece::Quoted(s) => {
                out.push('"');
                out.push_str(s);
                out.push('"');
            }
            Piece::Number(s) | Piece::Macro(s) => out.push_str(s),
        }
    }
    out
}

/// The entry as BibLaTeX source (no trailing newline).
pub fn format_entry(e: &Entry) -> String {
    let mut out = format!("@{}{{{},\n", e.kind, e.key);
    for f in &e.fields {
        out.push_str("  ");
        out.push_str(&f.name);
        out.push_str(" = ");
        out.push_str(&format_value(&f.value));
        out.push_str(",\n");
    }
    out.push('}');
    out
}

pub(super) fn format_block(k: &Kind, out: &mut String) {
    match k {
        Kind::Entry(e) => out.push_str(&format_entry(e)),
        Kind::String { name, value } => out.push_str(&format!("@string{{{name} = {}}}", format_value(value))),
        Kind::Preamble(v) => out.push_str(&format!("@preamble{{{}}}", format_value(v))),
        // these always keep their raw text
        Kind::Text | Kind::Comment | Kind::Broken { .. } => {}
    }
}
