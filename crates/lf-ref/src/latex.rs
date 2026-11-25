//! LaTeX in field values ↔ Unicode text.
//!
//! - [`decode`]: LaTeX source → readable text for display and search
//!   (`M{\"u}ller` → `Müller`, `The {\TeX book}` → `The TeXbook`, `--` → `–`).
//! - [`encode`]: plain text → safe LaTeX for writing a field (escapes `& % $ # _ { } \ ~ ^`,
//!   keeps Unicode – biber reads UTF-8).
//! - [`fold_ascii`]: text → ASCII for keys and search (`Müller` → `Mueller`, `Øre` → `Ore`).

/// Accent commands: (command, combining mark, base → precomposed pairs).
const ACCENTS: &[(&str, char, &str)] = &[
    ("`", '\u{300}', "aàeèiìoòuùAÀEÈIÌOÒUÙnǹNǸ"),
    ("'", '\u{301}', "aáeéiíoóuúyýcćnńsśzźlĺrŕgǵAÁEÉIÍOÓUÚYÝCĆNŃSŚZŹLĹRŔGǴ"),
    ("^", '\u{302}', "aâeêiîoôuûcĉgĝhĥjĵsŝwŵyŷAÂEÊIÎOÔUÛCĈGĜHĤJĴSŜWŴYŶ"),
    ("~", '\u{303}', "aãnñoõiĩuũAÃNÑOÕIĨUŨ"),
    ("=", '\u{304}', "aāeēiīoōuūAĀEĒIĪOŌUŪ"),
    ("u", '\u{306}', "aăgğuŭeĕiĭoŏAĂGĞUŬEĔIĬOŎ"),
    (".", '\u{307}', "zżcċeėgġIİZŻCĊEĖGĠ"),
    ("\"", '\u{308}', "aäeëiïoöuüyÿAÄEËIÏOÖUÜYŸ"),
    ("r", '\u{30A}', "aåuůAÅUŮ"),
    ("H", '\u{30B}', "oőuűOŐUŰ"),
    ("v", '\u{30C}', "cčsšzžrřeěnňdďtťlľCČSŠZŽRŘEĚNŇDĎTŤLĽ"),
    ("d", '\u{323}', ""),
    ("c", '\u{327}', "cçsştţgģkķlļnņrŗCÇSŞTŢGĢKĶLĻNŅRŖ"),
    ("k", '\u{328}', "aąeęiįuųAĄEĘIĮUŲ"),
    ("b", '\u{331}', ""),
];

/// Commands that stand for a character.
const SYMBOLS: &[(&str, &str)] = &[
    ("ss", "ß"),
    ("SS", "SS"),
    ("o", "ø"),
    ("O", "Ø"),
    ("ae", "æ"),
    ("AE", "Æ"),
    ("oe", "œ"),
    ("OE", "Œ"),
    ("aa", "å"),
    ("AA", "Å"),
    ("l", "ł"),
    ("L", "Ł"),
    ("i", "ı"),
    ("j", "ȷ"),
    ("dh", "ð"),
    ("DH", "Ð"),
    ("th", "þ"),
    ("TH", "Þ"),
    ("ng", "ŋ"),
    ("NG", "Ŋ"),
    ("dj", "đ"),
    ("DJ", "Đ"),
    ("&", "&"),
    ("%", "%"),
    ("$", "$"),
    ("#", "#"),
    ("_", "_"),
    ("{", "{"),
    ("}", "}"),
    (" ", " "),
    (",", " "),
    (";", " "),
    ("\\", " "),
    ("/", ""),
    ("-", ""),
    ("textendash", "–"),
    ("textemdash", "—"),
    ("ldots", "…"),
    ("dots", "…"),
    ("textellipsis", "…"),
    ("textquoteleft", "‘"),
    ("textquoteright", "’"),
    ("textquotedblleft", "“"),
    ("textquotedblright", "”"),
    ("guillemotleft", "«"),
    ("guillemotright", "»"),
    ("S", "§"),
    ("P", "¶"),
    ("copyright", "©"),
    ("textregistered", "®"),
    ("texttrademark", "™"),
    ("pounds", "£"),
    ("euro", "€"),
    ("texteuro", "€"),
    ("textdegree", "°"),
    ("textbackslash", "\\"),
    ("textasciitilde", "~"),
    ("textasciicircum", "^"),
    ("textbar", "|"),
    ("textless", "<"),
    ("textgreater", ">"),
    ("textunderscore", "_"),
    ("TeX", "TeX"),
    ("LaTeX", "LaTeX"),
    ("LaTeXe", "LaTeX2e"),
    ("BibTeX", "BibTeX"),
    ("XeTeX", "XeTeX"),
    ("LuaTeX", "LuaTeX"),
    ("ConTeXt", "ConTeXt"),
    ("protect", ""),
    ("relax", ""),
    ("nobreakspace", " "),
    ("space", " "),
];

/// LaTeX source → readable Unicode text. Braces disappear, accents and symbols become
/// characters, formatting commands (`\emph`, `\textbf`, `{\em …}`, …) keep only their text,
/// unknown commands are dropped. Math (`$…$`) is kept as written. Whitespace is collapsed.
pub fn decode(src: &str) -> String {
    // fast path: nothing to translate
    if !src.bytes().any(|b| matches!(b, b'\\' | b'{' | b'}' | b'$' | b'-' | b'`' | b'\'' | b'~')) {
        return collapse_ws(src);
    }
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    decode_into(&chars, &mut out);
    collapse_ws(&out)
}

fn decode_into(c: &[char], out: &mut String) {
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            '\\' => i = command(c, i, out),
            '{' | '}' => i += 1,
            '$' => {
                // keep math verbatim, including the dollars
                let s = i;
                i += 1;
                if c.get(i) == Some(&'$') {
                    i += 1;
                }
                while i < c.len() && c[i] != '$' {
                    if c[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
                while i < c.len() && c[i] == '$' {
                    i += 1;
                }
                out.extend(&c[s..i.min(c.len())]);
            }
            '-' if c.get(i + 1) == Some(&'-') => {
                if c.get(i + 2) == Some(&'-') {
                    out.push('—');
                    i += 3;
                } else {
                    out.push('–');
                    i += 2;
                }
            }
            '`' if c.get(i + 1) == Some(&'`') => {
                out.push('“');
                i += 2;
            }
            '\'' if c.get(i + 1) == Some(&'\'') => {
                out.push('”');
                i += 2;
            }
            '`' => {
                out.push('‘');
                i += 1;
            }
            '~' => {
                out.push(' ');
                i += 1;
            }
            ch => {
                out.push(ch);
                i += 1;
            }
        }
    }
}

/// Handles the command starting with the backslash at `i`; returns the next position.
fn command(c: &[char], i: usize, out: &mut String) -> usize {
    let mut j = i + 1;
    let Some(&first) = c.get(j) else { return j };
    let name: String = if first.is_ascii_alphabetic() {
        while j < c.len() && c[j].is_ascii_alphabetic() {
            j += 1;
        }
        c[i + 1..j].iter().collect()
    } else {
        j += 1;
        first.to_string()
    };
    let word = first.is_ascii_alphabetic();
    if word {
        // spaces after a control word are not part of the text
        while j < c.len() && c[j].is_whitespace() {
            j += 1;
        }
    }
    if let Some((_, mark, table)) = ACCENTS.iter().find(|(n, _, _)| *n == name) {
        let (arg, next) = accent_arg(c, j);
        let mut inner = String::new();
        decode_into(&arg, &mut inner);
        let mut it = inner.chars();
        if let Some(base) = it.next() {
            let base = match base {
                'ı' => 'i',
                'ȷ' => 'j',
                b => b,
            };
            match lookup_pair(table, base) {
                Some(p) => out.push(p),
                None => {
                    out.push(base);
                    out.push(*mark);
                }
            }
            out.extend(it);
        }
        return next;
    }
    if let Some((_, s)) = SYMBOLS.iter().find(|(n, _)| *n == name) {
        out.push_str(s);
        // `\ss{}` / `{\o}`: an empty group after a symbol is just a terminator
        if c.get(j) == Some(&'{') && c.get(j + 1) == Some(&'}') {
            return j + 2;
        }
        return j;
    }
    if matches!(name.as_str(), "url" | "path") {
        // keep the URL literally
        if c.get(j) == Some(&'{') {
            let mut k = j + 1;
            let mut depth = 1;
            while k < c.len() {
                match c[k] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            out.extend(&c[j + 1..k.min(c.len())]);
            return (k + 1).min(c.len());
        }
        return j;
    }
    // formatting and unknown commands: drop the name, the argument text (in braces) stays
    j
}

/// Argument of an accent: `{x}`, a single character, or a command such as `\i`.
fn accent_arg(c: &[char], j: usize) -> (Vec<char>, usize) {
    match c.get(j) {
        Some('{') => {
            let mut k = j + 1;
            let mut depth = 1;
            while k < c.len() {
                match c[k] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            (c[j + 1..k.min(c.len())].to_vec(), (k + 1).min(c.len()))
        }
        Some('\\') => {
            let mut k = j + 1;
            while k < c.len() && c[k].is_ascii_alphabetic() {
                k += 1;
            }
            if k == j + 1 && k < c.len() {
                k += 1;
            }
            (c[j..k].to_vec(), k)
        }
        Some(_) => (vec![c[j]], j + 1),
        None => (Vec::new(), j),
    }
}

/// Pairs `(base, accented)` of an accent table, without allocating.
fn pairs(table: &str) -> impl Iterator<Item = (char, char)> + '_ {
    let mut it = table.chars();
    std::iter::from_fn(move || Some((it.next()?, it.next()?)))
}

fn lookup_pair(table: &str, base: char) -> Option<char> {
    pairs(table).find(|p| p.0 == base).map(|p| p.1)
}

fn collapse_ws(s: &str) -> String {
    let b = s.as_bytes();
    let clean = !b.first().is_some_and(u8::is_ascii_whitespace)
        && !b.last().is_some_and(u8::is_ascii_whitespace)
        && !b.windows(2).any(|w| w[0].is_ascii_whitespace() && (w[1].is_ascii_whitespace() || w[0] != b' '))
        && !s.chars().any(|c| !c.is_ascii() && c.is_whitespace());
    if clean {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        }
    }
    out
}

/// Plain text → LaTeX for a braced field value. Unicode stays as it is.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' | '}' | '&' | '%' | '$' | '#' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '\u{a0}' => out.push('~'),
            c if c.is_whitespace() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Unicode → ASCII: German umlauts as `ae oe ue ss`, other letters without accents,
/// characters without an ASCII form are dropped.
pub fn fold_ascii(text: &str) -> String {
    fold(text, true)
}

/// Like [`fold_ascii`], but every accented letter becomes its base letter (`ü` → `u`).
/// Used for search, so `muller` and `mueller` both find `Müller`.
pub fn strip_accents(text: &str) -> String {
    fold(text, false)
}

fn fold(text: &str, german: bool) -> String {
    if text.is_ascii() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii() {
            out.push(ch);
            continue;
        }
        let s: &str = match ch {
            'ä' if german => "ae",
            'ö' if german => "oe",
            'ü' if german => "ue",
            'Ä' if german => "Ae",
            'Ö' if german => "Oe",
            'Ü' if german => "Ue",
            'ß' => "ss",
            'ẞ' => "SS",
            'æ' => "ae",
            'Æ' => "AE",
            'œ' => "oe",
            'Œ' => "OE",
            'ø' => "o",
            'Ø' => "O",
            'ł' => "l",
            'Ł' => "L",
            'ı' => "i",
            'ȷ' => "j",
            'ð' | 'đ' => "d",
            'Ð' | 'Đ' => "D",
            'þ' => "th",
            'Þ' => "Th",
            'ŋ' => "ng",
            'Ŋ' => "Ng",
            '–' | '—' => "-",
            '‘' | '’' => "'",
            '“' | '”' | '«' | '»' => "\"",
            '…' => "...",
            c if c.is_whitespace() => " ",
            c => {
                // precomposed accented letter: find its base in the accent tables
                match base_letter(c) {
                    Some(b) => {
                        out.push(b);
                        continue;
                    }
                    None => "",
                }
            }
        };
        out.push_str(s);
    }
    out
}

fn base_letter(c: char) -> Option<char> {
    if ('\u{300}'..='\u{36f}').contains(&c) {
        return None; // combining mark: drop
    }
    ACCENTS.iter().find_map(|(_, _, table)| pairs(table).find(|p| p.1 == c).map(|p| p.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_in_all_spellings() {
        assert_eq!(decode(r#"M{\"u}ller"#), "Müller");
        assert_eq!(decode(r#"M\"uller"#), "Müller");
        assert_eq!(decode(r#"M\"{u}ller"#), "Müller");
        assert_eq!(decode(r#"Aks{\i}n, {\"O}zge"#), "Aksın, Özge");
        assert_eq!(decode(r"Erd\H{o}s"), "Erdős");
        assert_eq!(decode(r"Ho\v{c}evar"), "Hočevar");
        assert_eq!(decode(r"Fran\c{c}ois"), "François");
        assert_eq!(decode(r"Fran\c cois"), "François");
        assert_eq!(decode(r"{\'\i}ndice"), "índice");
        assert_eq!(decode(r"\'{\i}"), "í");
        assert_eq!(decode(r"Stra{\ss}e"), "Straße");
        assert_eq!(decode(r"\ss{}e"), "ße");
        assert_eq!(decode(r#"{\o}re {\AA}ngstr{\"o}m"#), "øre Ångström");
        assert_eq!(decode(r"\L{}ukasiewicz"), "Łukasiewicz");
        assert_eq!(decode(r"\~{n}"), "ñ");
        // accent without a precomposed form → combining mark
        assert_eq!(decode(r"\d{s}"), "s\u{323}");
    }

    #[test]
    fn commands_braces_and_ligatures() {
        assert_eq!(decode(r"The {\TeX book}"), "The TeXbook");
        assert_eq!(decode(r"\emph{Rust} and {\em C}"), "Rust and C");
        assert_eq!(decode(r#"\mkbibquote{Intention} und Intentionalit{\"a}t"#), "Intention und Intentionalität");
        assert_eq!(decode(r"Computers \& Typesetting"), "Computers & Typesetting");
        assert_eq!(decode("pages 55--65 --- see ``this''"), "pages 55–65 — see “this”");
        assert_eq!(decode("Angew.~Chem.\n   Int.~Ed."), "Angew. Chem. Int. Ed.");
        assert_eq!(decode(r"Energy $E=mc^2$ \& $\alpha$"), r"Energy $E=mc^2$ & $\alpha$");
        assert_eq!(decode(r"\url{https://x.org/a_b%20c}"), "https://x.org/a_b%20c");
        assert_eq!(decode(r"\unknown{Text}"), "Text");
        assert_eq!(decode("  {}  "), "");
    }

    #[test]
    fn decode_never_panics_on_truncated_input() {
        for s in [r"\", r"\'", r#"\"{"#, r"{\i", "$", "$$x", r"\url{", r"\c", "--", r#"\v{\"}"#] {
            let _ = decode(s);
        }
    }

    #[test]
    fn encode_escapes_and_round_trips() {
        assert_eq!(encode("R&D 100% $5 #1 a_b {x}"), r"R\&D 100\% \$5 \#1 a\_b \{x\}");
        assert_eq!(encode(r"C:\x ~ ^"), r"C:\textbackslash{}x \textasciitilde{} \textasciicircum{}");
        for s in ["Müller & Søn – 100% {sure}", "a_b\\c ~^ #1 $x$", "Ünïcödé “quoted”"] {
            assert_eq!(decode(&encode(s)), s);
        }
    }

    #[test]
    fn ascii_folding() {
        assert_eq!(fold_ascii("Müller Straße"), "Mueller Strasse");
        assert_eq!(fold_ascii("Erdős Ångström Łukasiewicz Øre"), "Erdos Angstroem Lukasiewicz Ore");
        assert_eq!(fold_ascii("François Hočevar ñ"), "Francois Hocevar n");
        assert_eq!(fold_ascii("s\u{323}"), "s");
        assert_eq!(fold_ascii("日本"), "");
        assert_eq!(strip_accents("Müller Straße Ångström"), "Muller Strasse Angstrom");
    }
}
