//! YAML-style front matter at the top of a Markdown file:
//!
//! ```yaml
//! ---
//! title: My notes
//! author: arvid
//! class: article
//! lang: ngerman
//! packages: [siunitx, tikz]
//! preamble: |
//!   \newcommand{\R}{\mathbb{R}}
//! bibliography: refs.bib
//! engine: pdflatex
//! toc: true
//! ---
//! ```
//!
//! Only this simple subset of YAML is read (scalars, `[a, b]` lists, `- item`
//! lists and `|` blocks); unknown keys are ignored.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub author: Option<String>,
    pub date: Option<String>,
    /// Document class (default `article`).
    pub class: String,
    pub classoptions: Option<String>,
    /// babel language, e.g. `ngerman`.
    pub lang: Option<String>,
    pub packages: Vec<String>,
    /// Raw LaTeX for the preamble.
    pub preamble: String,
    /// `.bib` file (biblatex + biber).
    pub bibliography: Option<String>,
    pub biblio_style: Option<String>,
    /// `pdflatex`, `xelatex` or `lualatex`; overrides the project setting.
    pub engine: Option<String>,
    pub toc: bool,
    pub numbersections: bool,
    /// Options for `geometry` (default `margin=2.5cm`).
    pub geometry: Option<String>,
}

impl Default for FrontMatter {
    fn default() -> Self {
        FrontMatter {
            title: None,
            author: None,
            date: None,
            class: "article".into(),
            classoptions: None,
            lang: None,
            packages: Vec::new(),
            preamble: String::new(),
            bibliography: None,
            biblio_style: None,
            engine: None,
            toc: false,
            numbersections: true,
            geometry: None,
        }
    }
}

impl FrontMatter {
    /// `#` becomes `\chapter` in these classes.
    pub fn uses_chapters(&self) -> bool {
        matches!(self.class.as_str(), "report" | "book" | "scrreprt" | "scrbook" | "memoir")
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

fn truthy(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "yes" | "on" | "1")
}

/// Parses the front matter; returns it and the number of lines it takes
/// (0 without front matter).
pub fn parse(src: &str) -> (FrontMatter, usize) {
    let lines: Vec<&str> = src.lines().collect();
    let mut f = FrontMatter::default();
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return (f, 0);
    }
    let Some(end) = lines.iter().skip(1).position(|l| matches!(l.trim_end(), "---" | "...")).map(|p| p + 1) else {
        return (f, 0);
    };
    let body = &lines[1..end];
    let mut i = 0;
    while i < body.len() {
        let line = body[i];
        i += 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') || line.starts_with([' ', '\t']) {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        // block (`|`) or list (`- a`) on the following, indented lines
        let mut block = Vec::new();
        if value.is_empty() || value == "|" || value == ">" {
            while i < body.len() && (body[i].starts_with([' ', '\t']) || body[i].trim().is_empty()) {
                block.push(body[i]);
                i += 1;
            }
        }
        let list: Vec<String> = if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            inner.split(',').map(unquote).filter(|s| !s.is_empty()).collect()
        } else if !block.is_empty() && value.is_empty() {
            block.iter().filter_map(|l| l.trim().strip_prefix("- ").map(unquote)).collect()
        } else if !value.is_empty() {
            vec![unquote(value)]
        } else {
            Vec::new()
        };
        let text = if value == "|" || value == ">" {
            let indent = block.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
            let joined: Vec<&str> = block.iter().map(|l| if l.len() >= indent { &l[indent..] } else { "" }).collect();
            joined.join(if value == "|" { "\n" } else { " " }).trim_end().to_string()
        } else {
            list.join("\n")
        };
        let opt = || (!text.is_empty()).then(|| text.clone());
        match key.as_str() {
            "title" => f.title = opt(),
            "author" => f.author = Some(list.join(" \\and ")).filter(|s| !s.is_empty()),
            "date" => f.date = opt(),
            "class" | "documentclass" => {
                if let Some(c) = opt() {
                    f.class = c;
                }
            }
            "classoption" | "classoptions" => f.classoptions = Some(list.join(",")).filter(|s| !s.is_empty()),
            "lang" | "language" | "babel" => f.lang = opt(),
            "packages" | "usepackage" => f.packages = list,
            "preamble" | "header-includes" => f.preamble = text,
            "bibliography" => f.bibliography = opt(),
            "biblio-style" | "bibstyle" | "biblio_style" => f.biblio_style = opt(),
            "engine" | "pdf-engine" => f.engine = opt(),
            "toc" => f.toc = truthy(&text),
            "numbersections" | "number-sections" | "numbered" => f.numbersections = truthy(&text),
            "geometry" => f.geometry = Some(list.join(",")).filter(|s| !s.is_empty()),
            _ => {}
        }
    }
    (f, end + 1)
}
