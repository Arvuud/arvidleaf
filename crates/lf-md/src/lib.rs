//! Markdown → LaTeX for arvidleaf.
//!
//! Markdown is parsed with `pulldown-cmark` and written as a complete LaTeX
//! document. Math (`$…$`, `$$…$$`), LaTeX commands in the text (`\cite{x}`,
//! `\newpage`) and LaTeX environments at the start of a line
//! (`\begin{align} … \end{align}`) are passed through unchanged, so anything
//! that compiles in LaTeX also compiles here.
//!
//! Every line of the generated LaTeX remembers the Markdown line it came
//! from ([`Output::map`]); errors and SyncTeX use it to point back into the
//! Markdown file.

mod front;
mod lexer;

use std::fmt::Write as _;
use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options as MdOptions, Parser, Tag, TagEnd};

pub use front::FrontMatter;
pub use lexer::{MdSpan, MdTok, tokenize};

/// How the document is converted.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Folder of the Markdown file relative to the directory TeX runs in
    /// (`""` when both are the same). Images and the bibliography are found there.
    pub base_dir: String,
    /// XeLaTeX/LuaLaTeX (fontspec) instead of pdfLaTeX (fontenc).
    pub unicode_engine: bool,
    /// Name of the Markdown file, for the header comment.
    pub source_name: String,
}

#[derive(Clone, Debug)]
pub struct Output {
    pub tex: String,
    /// Markdown line (1-based) for every line of `tex`.
    pub map: Vec<u32>,
    pub front: FrontMatter,
}

/// Placeholders for raw LaTeX blocks (private-use characters, kept by the parser).
const RAW_OPEN: char = '\u{E000}';
const RAW_CLOSE: char = '\u{E001}';

/// Converts a Markdown document into a complete LaTeX document.
pub fn convert(src: &str, opts: &Options) -> Output {
    let (front, front_lines) = front::parse(src);
    let (body, raws) = prepare(src, front_lines);
    let line_starts: Vec<usize> = std::iter::once(0).chain(body.match_indices('\n').map(|(i, _)| i + 1)).collect();
    let mut w = Writer { out: String::new(), map: Vec::new(), line: 1 };
    preamble(&mut w, &front, opts);
    let events = collect_events(&body);
    let mut r = Renderer { events: &events, line_starts: &line_starts, raws: &raws, front: &front, footnotes: Vec::new() };
    r.footnotes = footnote_definitions(&events);
    let last_line = line_starts.len() as u32;
    r.render(&mut w, 0..events.len());
    w.line = last_line;
    if !w.out.ends_with('\n') {
        w.push("\n");
    }
    if front.bibliography.is_some() {
        w.push("\n\\printbibliography\n");
    }
    w.push("\\end{document}\n");
    Output { tex: w.out, map: w.map, front }
}

/// Front matter only (cheap; for the engine and the outline).
pub fn front_matter(src: &str) -> FrontMatter {
    front::parse(src).0
}

/// Headings for the outline: (LaTeX level, title, line). The level follows
/// the document class like the conversion does (`#` = section, or chapter
/// in report/book).
pub fn headings(src: &str) -> Vec<(u8, String, u32)> {
    let (front, skip) = front::parse(src);
    let offset = if front.uses_chapters() { 0 } else { 1 };
    let mut out = Vec::new();
    let mut fence: Option<String> = None;
    let lines: Vec<&str> = src.lines().collect();
    for (i, l) in lines.iter().enumerate().skip(skip) {
        let t = l.trim_start();
        if let Some(f) = &fence {
            if t.starts_with(f.as_str()) {
                fence = None;
            }
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(t[..3].to_string());
            continue;
        }
        if l.starts_with('#') {
            let level = l.chars().take_while(|c| *c == '#').count();
            let rest = &l[level..];
            if (1..=6).contains(&level) && (rest.is_empty() || rest.starts_with(' ')) {
                let mut title = rest.trim().trim_end_matches('#').trim().to_string();
                if let Some(p) = title.rfind(" {")
                    && title.ends_with('}')
                {
                    title.truncate(p);
                }
                out.push(((level as u8 + offset).min(5), title, i as u32 + 1));
            }
            continue;
        }
        // setext: a text line underlined with === or ---
        if let Some(next) = lines.get(i + 1) {
            let n = next.trim();
            if !t.is_empty() && n.len() >= 2 && !t.starts_with(['-', '*', '+', '>', '|']) {
                if n.chars().all(|c| c == '=') {
                    out.push((1 + offset, t.trim().to_string(), i as u32 + 1));
                } else if n.chars().all(|c| c == '-') {
                    out.push((2 + offset, t.trim().to_string(), i as u32 + 1));
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------------ writer

struct Writer {
    out: String,
    map: Vec<u32>,
    /// Markdown line of what is written now.
    line: u32,
}

impl Writer {
    fn push(&mut self, s: &str) {
        for (i, part) in s.split('\n').enumerate() {
            if i > 0 {
                self.map.push(self.line);
                self.out.push('\n');
            }
            self.out.push_str(part);
        }
    }

    /// Multi-line text whose lines are consecutive Markdown lines from `first`.
    fn push_lines(&mut self, s: &str, first: u32) {
        for (i, part) in s.split('\n').enumerate() {
            if i > 0 {
                self.map.push(self.line);
                self.out.push('\n');
            }
            self.line = first + i as u32;
            self.out.push_str(part);
        }
    }

    fn newline_if_needed(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.push("\n");
        }
    }

    fn blank_line(&mut self) {
        self.newline_if_needed();
        if !self.out.ends_with("\n\n") {
            self.push("\n");
        }
    }
}

// ------------------------------------------------------------------ preparation

/// Blanks the front matter and replaces raw LaTeX environments that start a
/// line (`\begin{align}` … `\end{align}`) by placeholders, so the Markdown
/// parser leaves them alone (`\\` would otherwise become `\`). Line numbers
/// stay the same. Returns the body and the raw blocks (first line, text).
fn prepare(src: &str, front_lines: usize) -> (String, Vec<(u32, String)>) {
    let lines: Vec<&str> = src.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut raws = Vec::new();
    let mut fence: Option<String> = None;
    let mut display_math = false;
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        if i < front_lines {
            out.push(String::new());
            i += 1;
            continue;
        }
        let t = l.trim_start();
        if let Some(f) = &fence {
            if t.starts_with(f.as_str()) {
                fence = None;
            }
            out.push(l.to_string());
            i += 1;
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(t[..3].to_string());
            out.push(l.to_string());
            i += 1;
            continue;
        }
        if display_math || l.len() - t.len() > 3 {
            if l.matches("$$").count() % 2 == 1 {
                display_math = !display_math;
            }
            out.push(l.to_string());
            i += 1;
            continue;
        }
        if let Some(env) = begin_env(t) {
            // find the matching \end{env}
            let (open, close) = (format!("\\begin{{{env}}}"), format!("\\end{{{env}}}"));
            let mut depth = 0i32;
            let mut end = None;
            for (k, line) in lines.iter().enumerate().skip(i) {
                depth += line.matches(&open).count() as i32;
                depth -= line.matches(&close).count() as i32;
                if depth <= 0 {
                    end = Some(k);
                    break;
                }
            }
            if let Some(end) = end {
                raws.push((i as u32 + 1, lines[i..=end].join("\n")));
                out.push(format!("{RAW_OPEN}{}{RAW_CLOSE}", raws.len() - 1));
                out.extend(std::iter::repeat_n(String::new(), end - i));
                i = end + 1;
                continue;
            }
        }
        if l.matches("$$").count() % 2 == 1 {
            display_math = !display_math;
        }
        out.push(l.to_string());
        i += 1;
    }
    (out.join("\n"), raws)
}

/// `\begin{name}` at the start of a (trimmed) line.
fn begin_env(t: &str) -> Option<String> {
    let rest = t.strip_prefix("\\begin{")?;
    let end = rest.find('}')?;
    let name = &rest[..end];
    (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '*')).then(|| name.to_string())
}

fn collect_events(body: &str) -> Vec<(Event<'_>, Range<usize>)> {
    let opts = MdOptions::ENABLE_TABLES
        | MdOptions::ENABLE_FOOTNOTES
        | MdOptions::ENABLE_STRIKETHROUGH
        | MdOptions::ENABLE_TASKLISTS
        | MdOptions::ENABLE_MATH
        | MdOptions::ENABLE_HEADING_ATTRIBUTES;
    let mut events: Vec<(Event, Range<usize>)> = Vec::new();
    for (e, r) in Parser::new_ext(body, opts).into_offset_iter() {
        // the parser splits text at `[`, `\`, …: join it again
        if let (Event::Text(t), Some((Event::Text(prev), pr))) = (&e, events.last_mut()) {
            let joined = format!("{prev}{t}");
            *prev = joined.into();
            pr.end = r.end;
            continue;
        }
        events.push((e, r));
    }
    events
}

/// Footnote label → event range of its definition (without start/end).
fn footnote_definitions(events: &[(Event, Range<usize>)]) -> Vec<(String, Range<usize>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::FootnoteDefinition(label)) = &events[i].0 {
            let end = matching_end(events, i);
            out.push((label.to_string(), i + 1..end));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Index of the End event that closes the Start at `i`.
fn matching_end(events: &[(Event, Range<usize>)], i: usize) -> usize {
    let mut depth = 0;
    for (k, (e, _)) in events.iter().enumerate().skip(i) {
        match e {
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    events.len().saturating_sub(1)
}

// ------------------------------------------------------------------ preamble

fn preamble(w: &mut Writer, f: &FrontMatter, opts: &Options) {
    let mut p = String::new();
    let name = if opts.source_name.is_empty() { "Markdown" } else { &opts.source_name };
    let _ = writeln!(p, "% Generated by arvidleaf from {name}. Changes belong in the Markdown file.");
    let _ = writeln!(p, "\\documentclass[{}]{{{}}}", f.classoptions.as_deref().unwrap_or("11pt,a4paper"), f.class);
    if opts.unicode_engine {
        p.push_str("\\usepackage{fontspec}\n");
    } else {
        p.push_str("\\usepackage[T1]{fontenc}\n\\usepackage{lmodern}\n");
    }
    if let Some(lang) = &f.lang {
        let _ = writeln!(p, "\\usepackage[{lang}]{{babel}}");
    }
    p.push_str("\\usepackage{amsmath,amssymb}\n\\usepackage{graphicx}\n\\usepackage{booktabs}\n\\usepackage{enumitem}\n");
    p.push_str("\\usepackage{listings}\n\\usepackage{xcolor}\n\\usepackage[normalem]{ulem}\n\\usepackage{parskip}\n");
    let _ = writeln!(p, "\\usepackage[{}]{{geometry}}", f.geometry.as_deref().unwrap_or("margin=2.5cm"));
    if let Some(bib) = &f.bibliography {
        p.push_str("\\usepackage{csquotes}\n");
        let _ = writeln!(p, "\\usepackage[backend=biber,style={}]{{biblatex}}", f.biblio_style.as_deref().unwrap_or("numeric"));
        let _ = writeln!(p, "\\addbibresource{{{}}}", join_base(&opts.base_dir, bib));
    }
    p.push_str("\\usepackage[hidelinks]{hyperref}\n");
    for pkg in &f.packages {
        if pkg.contains('{') {
            let _ = writeln!(p, "\\usepackage{pkg}");
        } else {
            let _ = writeln!(p, "\\usepackage{{{pkg}}}");
        }
    }
    p.push_str(
        "\\lstset{basicstyle=\\ttfamily\\small,breaklines=true,columns=fullflexible,keepspaces=true,\n  \
         backgroundcolor=\\color{black!4},frame=single,framerule=0pt,showstringspaces=false,\n  \
         keywordstyle=\\color{blue!60!black}\\bfseries,commentstyle=\\color{black!55}\\itshape,stringstyle=\\color{red!50!black}}\n",
    );
    if !opts.unicode_engine {
        p.push_str("\\lstset{literate={ä}{{\\\"a}}1 {ö}{{\\\"o}}1 {ü}{{\\\"u}}1 {Ä}{{\\\"A}}1 {Ö}{{\\\"O}}1 {Ü}{{\\\"U}}1 {ß}{{\\ss}}1 {é}{{\\'e}}1 {è}{{\\`e}}1}\n");
    }
    if !opts.base_dir.is_empty() {
        let _ = writeln!(p, "\\graphicspath{{{{{}/}}}}", opts.base_dir.trim_end_matches('/'));
    }
    if !f.numbersections {
        p.push_str("\\setcounter{secnumdepth}{0}\n");
    }
    if !f.preamble.is_empty() {
        p.push_str(f.preamble.trim_end());
        p.push('\n');
    }
    if let Some(t) = &f.title {
        let _ = writeln!(p, "\\title{{{}}}", escape_text(t));
        let _ = writeln!(p, "\\author{{{}}}", escape_text(f.author.as_deref().unwrap_or("")));
        let _ = writeln!(p, "\\date{{{}}}", f.date.as_deref().map(escape_text).unwrap_or_else(|| "\\today".into()));
    }
    p.push_str("\n\\begin{document}\n");
    if f.title.is_some() {
        p.push_str("\\maketitle\n");
    }
    if f.toc {
        p.push_str("\\tableofcontents\n");
    }
    p.push('\n');
    w.line = 1;
    w.push(&p);
}

fn join_base(base: &str, path: &str) -> String {
    if base.is_empty() || path.starts_with('/') { path.to_string() } else { format!("{}/{path}", base.trim_end_matches('/')) }
}

// ------------------------------------------------------------------ rendering

struct Renderer<'a, 'e> {
    events: &'a [(Event<'e>, Range<usize>)],
    line_starts: &'a [usize],
    raws: &'a [(u32, String)],
    front: &'a FrontMatter,
    footnotes: Vec<(String, Range<usize>)>,
}

#[derive(Default)]
struct Table {
    cell: usize,
}

impl Renderer<'_, '_> {
    fn line_of(&self, offset: usize) -> u32 {
        self.line_starts.partition_point(|&s| s <= offset) as u32
    }

    fn render(&self, w: &mut Writer, range: Range<usize>) {
        let mut lists: Vec<&'static str> = Vec::new();
        let mut table = Table::default();
        // inside a code block: Some(true) for raw LaTeX (```{=latex})
        let mut code: Option<bool> = None;
        let mut i = range.start;
        while i < range.end {
            let (ev, r) = &self.events[i];
            // an end tag's range starts with its block: keep the line of the content
            if !matches!(ev, Event::End(_)) {
                w.line = self.line_of(r.start);
            }
            match ev {
                Event::Start(tag) => match tag {
                    Tag::Paragraph => {
                        if let Some(end) = self.figure_paragraph(i) {
                            self.figure(w, i + 1);
                            i = end + 1;
                            continue;
                        }
                    }
                    Tag::Heading { level, id, .. } => {
                        w.blank_line();
                        let cmd = heading_cmd(*level, self.front.uses_chapters());
                        w.push(&format!("\\{cmd}{{"));
                        let end = matching_end(self.events, i);
                        self.render(w, i + 1..end);
                        w.push("}");
                        if let Some(id) = id {
                            w.push(&format!("\\label{{{id}}}"));
                        }
                        w.push("\n\n");
                        i = end + 1;
                        continue;
                    }
                    Tag::BlockQuote(_) => {
                        w.newline_if_needed();
                        w.push("\\begin{quote}\n");
                    }
                    Tag::CodeBlock(kind) => {
                        w.newline_if_needed();
                        let lang = match kind {
                            CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_string(),
                            CodeBlockKind::Indented => String::new(),
                        };
                        let raw = lang == "{=latex}" || lang == "=latex";
                        code = Some(raw);
                        if !raw {
                            match listings_language(&lang) {
                                Some(l) => w.push(&format!("\\begin{{lstlisting}}[language={l}]\n")),
                                None => w.push("\\begin{lstlisting}\n"),
                            }
                        }
                    }
                    Tag::List(start) => {
                        w.newline_if_needed();
                        match start {
                            Some(n) if *n != 1 => w.push(&format!("\\begin{{enumerate}}[start={n}]\n")),
                            Some(_) => w.push("\\begin{enumerate}\n"),
                            None => w.push("\\begin{itemize}\n"),
                        }
                        lists.push(if start.is_some() { "enumerate" } else { "itemize" });
                    }
                    Tag::Item => {
                        w.newline_if_needed();
                        // task lists: the marker follows the item (or its paragraph)
                        let marker = self.events[i + 1..(i + 3).min(range.end)].iter().find_map(|(e, _)| match e {
                            Event::TaskListMarker(done) => Some(*done),
                            _ => None,
                        });
                        match marker {
                            Some(true) => w.push("\\item[$\\boxtimes$] "),
                            Some(false) => w.push("\\item[$\\square$] "),
                            None => w.push("\\item "),
                        }
                    }
                    Tag::FootnoteDefinition(_) => {
                        i = matching_end(self.events, i) + 1;
                        continue;
                    }
                    Tag::Table(aligns) => {
                        w.newline_if_needed();
                        let spec: String = aligns
                            .iter()
                            .map(|a| match a {
                                Alignment::Center => 'c',
                                Alignment::Right => 'r',
                                _ => 'l',
                            })
                            .collect();
                        w.push(&format!("\\begin{{center}}\n\\begin{{tabular}}{{{spec}}}\n\\toprule\n"));
                    }
                    Tag::TableHead | Tag::TableRow => table.cell = 0,
                    Tag::TableCell => {
                        if table.cell > 0 {
                            w.push(" & ");
                        }
                        table.cell += 1;
                    }
                    Tag::Emphasis => w.push("\\emph{"),
                    Tag::Strong => w.push("\\textbf{"),
                    Tag::Strikethrough => w.push("\\sout{"),
                    Tag::Link { dest_url, .. } => {
                        if let Some(target) = dest_url.strip_prefix('#') {
                            w.push(&format!("\\hyperref[{target}]{{"));
                        } else {
                            w.push(&format!("\\href{{{}}}{{", escape_url(dest_url)));
                        }
                    }
                    Tag::Image { dest_url, .. } => {
                        // inside running text: a small inline image
                        let end = matching_end(self.events, i);
                        w.push(&inline_image(dest_url));
                        i = end + 1;
                        continue;
                    }
                    _ => {}
                },
                Event::End(tag) => match tag {
                    TagEnd::Paragraph => w.blank_line(),
                    TagEnd::BlockQuote(_) => {
                        w.newline_if_needed();
                        w.push("\\end{quote}\n\n");
                    }
                    TagEnd::CodeBlock => {
                        w.newline_if_needed();
                        if code.take() == Some(true) {
                            w.push("\n");
                        } else {
                            w.push("\\end{lstlisting}\n\n");
                        }
                    }
                    TagEnd::List(_) => {
                        w.newline_if_needed();
                        let env = lists.pop().unwrap_or("itemize");
                        w.push(&format!("\\end{{{env}}}\n\n"));
                    }
                    TagEnd::Item => w.newline_if_needed(),
                    TagEnd::Table => w.push("\\bottomrule\n\\end{tabular}\n\\end{center}\n\n"),
                    TagEnd::TableHead => w.push(" \\\\\n\\midrule\n"),
                    TagEnd::TableRow => w.push(" \\\\\n"),
                    TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => w.push("}"),
                    _ => {}
                },
                Event::Text(t) => {
                    if code.is_some() {
                        w.push_lines(t.strip_suffix('\n').unwrap_or(t), self.line_of(r.start));
                    } else {
                        self.text(w, t);
                    }
                }
                Event::Code(t) => w.push(&format!("\\texttt{{{}}}", escape_literal(t))),
                Event::InlineMath(m) => w.push(&format!("\\({m}\\)")),
                Event::DisplayMath(m) => {
                    let first = self.line_of(r.start);
                    let t = m.trim();
                    if t.starts_with("\\begin{") {
                        w.newline_if_needed();
                        let lead = (m.len() - m.trim_start_matches('\n').len()) as u32;
                        w.push_lines(m.trim_matches('\n'), first + lead);
                        w.push("\n");
                    } else {
                        w.push("\\[");
                        w.push_lines(m, first);
                        w.push("\\]");
                    }
                }
                Event::Html(h) | Event::InlineHtml(h) => {
                    let t = h.trim();
                    if t.starts_with("<br") {
                        w.push("\\newline{}");
                    }
                    // other HTML (comments, tags) has no LaTeX meaning: dropped
                }
                Event::SoftBreak => w.push("\n"),
                Event::HardBreak => w.push("\\\\\n"),
                Event::Rule => {
                    w.blank_line();
                    w.push("\\noindent\\rule{\\linewidth}{0.4pt}\n\n");
                }
                Event::FootnoteReference(label) => {
                    if let Some((_, range)) = self.footnotes.iter().find(|(l, _)| l == label.as_ref()) {
                        let mut inner = Writer { out: String::new(), map: Vec::new(), line: w.line };
                        self.render(&mut inner, range.clone());
                        let text = inner.out.trim().replace("\n\n", "\\par ").replace('\n', " ");
                        w.push(&format!("\\footnote{{{text}}}"));
                    }
                }
                Event::TaskListMarker(_) => {}
            }
            i += 1;
        }
    }

    /// A paragraph that holds only an image becomes a figure: returns the
    /// index of the paragraph's end.
    fn figure_paragraph(&self, i: usize) -> Option<usize> {
        let end = matching_end(self.events, i);
        let Event::Start(Tag::Image { .. }) = self.events.get(i + 1)?.0 else { return None };
        let img_end = matching_end(self.events, i + 1);
        (img_end + 1 == end).then_some(end)
    }

    fn figure(&self, w: &mut Writer, img: usize) {
        let Event::Start(Tag::Image { dest_url, title, .. }) = &self.events[img].0 else { return };
        let end = matching_end(self.events, img);
        let mut caption = Writer { out: String::new(), map: Vec::new(), line: w.line };
        self.render(&mut caption, img + 1..end);
        let caption = caption.out.trim().replace('\n', " ");
        w.newline_if_needed();
        if is_remote(dest_url) {
            w.push(&format!("\\begin{{center}}\\href{{{}}}{{{}}}\\end{{center}}\n\n", escape_url(dest_url), if caption.is_empty() { escape_literal(dest_url) } else { caption }));
            return;
        }
        w.push("\\begin{figure}[htbp]\n  \\centering\n");
        w.push(&format!("  \\includegraphics[width=0.8\\linewidth,height=0.4\\textheight,keepaspectratio]{{{dest_url}}}\n"));
        if !caption.is_empty() {
            w.push(&format!("  \\caption{{{caption}}}\n"));
        }
        if !title.is_empty() {
            w.push(&format!("  \\label{{{title}}}\n"));
        }
        w.push("\\end{figure}\n\n");
    }

    /// Running text: escaped, with LaTeX commands, citations and raw blocks kept.
    fn text(&self, w: &mut Writer, t: &str) {
        let mut rest = t;
        while let Some(p) = rest.find(RAW_OPEN) {
            w.push(&escape_text(&rest[..p]));
            let after = &rest[p + RAW_OPEN.len_utf8()..];
            let Some(q) = after.find(RAW_CLOSE) else {
                rest = after;
                continue;
            };
            if let Some((first, raw)) = after[..q].parse::<usize>().ok().and_then(|n| self.raws.get(n)) {
                w.newline_if_needed();
                w.push_lines(raw, *first);
                w.push("\n");
            }
            rest = &after[q + RAW_CLOSE.len_utf8()..];
        }
        w.push(&escape_text(rest));
    }
}

fn heading_cmd(level: HeadingLevel, chapters: bool) -> &'static str {
    let n = level as usize; // H1 = 1
    let names = ["chapter", "section", "subsection", "subsubsection", "paragraph", "subparagraph", "subparagraph"];
    names[(n - 1 + usize::from(!chapters)).min(names.len() - 1)]
}

fn is_remote(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn inline_image(url: &str) -> String {
    if is_remote(url) { format!("\\href{{{}}}{{[image]}}", escape_url(url)) } else { format!("\\includegraphics[height=1.2em]{{{url}}}") }
}

/// Languages `listings` knows (anything else is shown without colors).
fn listings_language(lang: &str) -> Option<&'static str> {
    Some(match lang.to_ascii_lowercase().as_str() {
        "python" | "py" | "python3" => "Python",
        "c" | "h" => "C",
        "cpp" | "c++" | "cxx" | "hpp" => "C++",
        "java" => "Java",
        "bash" | "sh" | "shell" | "zsh" | "console" => "bash",
        "sql" => "SQL",
        "html" => "HTML",
        "xml" => "XML",
        "tex" | "latex" => "{[LaTeX]TeX}",
        "matlab" | "octave" => "Matlab",
        "r" => "R",
        "ruby" | "rb" => "Ruby",
        "perl" => "Perl",
        "haskell" | "hs" => "Haskell",
        "fortran" => "Fortran",
        "php" => "PHP",
        "csharp" | "cs" | "c#" => "{[Sharp]C}",
        "make" | "makefile" => "make",
        _ => return None,
    })
}

// ------------------------------------------------------------------ escaping

/// Text with LaTeX's special characters escaped. Kept as written:
/// LaTeX commands with their arguments (`\cite[p.~3]{knuth}`), `~` (a
/// non-breaking space, as in LaTeX) and citations `[@key]` → `\cite{key}`.
pub fn escape_text(s: &str) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 8);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '[' && b.get(i + 1) == Some(&'@')
            && let Some((cite, next)) = citation(&b, i)
        {
            out.push_str(&cite);
            i = next;
            continue;
        }
        match c {
            '\\' if b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) => {
                // a command and its arguments, unchanged
                let start = i;
                i += 1;
                while i < b.len() && b[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if b.get(i) == Some(&'*') {
                    i += 1;
                }
                loop {
                    match b.get(i) {
                        Some('{') => i = group_end(&b, i, '{', '}'),
                        Some('[') => i = group_end(&b, i, '[', ']'),
                        _ => break,
                    }
                }
                out.extend(&b[start..i]);
                continue;
            }
            '\\' => out.push_str("\\textbackslash{}"),
            '#' | '%' | '&' | '_' | '{' | '}' | '$' => {
                out.push('\\');
                out.push(c);
            }
            '^' => out.push_str("\\^{}"),
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// End (exclusive) of a group starting at `i` with `open`; nested groups
/// and escaped braces are respected. Unbalanced: up to the end.
fn group_end(b: &[char], i: usize, open: char, close: char) -> usize {
    let mut depth = 0;
    let mut k = i;
    while k < b.len() {
        match b[k] {
            '\\' => k += 1,
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return k + 1;
                }
            }
            _ => {}
        }
        k += 1;
    }
    b.len()
}

/// `[@a; @b]` → `\cite{a,b}`, `[@a, p. 3]` → `\cite[p. 3]{a}`.
fn citation(b: &[char], i: usize) -> Option<(String, usize)> {
    let close = (i..b.len()).find(|&k| b[k] == ']')?;
    let inner: String = b[i + 1..close].iter().collect();
    let mut keys = Vec::new();
    let mut note = None;
    for part in inner.split(';') {
        let part = part.trim();
        let rest = part.strip_prefix('@')?;
        let (key, n) = match rest.split_once(',') {
            Some((k, n)) => (k.trim(), Some(n.trim().to_string())),
            None => (rest, None),
        };
        if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || "_-:./".contains(c)) {
            return None;
        }
        keys.push(key.to_string());
        if n.is_some() {
            note = n;
        }
    }
    let cite = match note {
        Some(n) => format!("\\cite[{}]{{{}}}", escape_text(&n), keys.join(",")),
        None => format!("\\cite{{{}}}", keys.join(",")),
    };
    Some((cite, close + 1))
}

/// Text shown exactly as written (inline code).
pub fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '#' | '%' | '&' | '_' | '{' | '}' | '$' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn escape_url(s: &str) -> String {
    s.replace('\\', "/").replace('%', "\\%").replace('#', "\\#")
}

// ------------------------------------------------------------------ line map

/// Stored next to the generated `.tex` so SyncTeX can map back later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineMap {
    /// Markdown file relative to the project root.
    pub source: String,
    /// Markdown line for every LaTeX line.
    pub lines: Vec<u32>,
}

impl LineMap {
    pub fn to_text(&self) -> String {
        let mut s = format!("{}\n", self.source);
        for l in &self.lines {
            let _ = writeln!(s, "{l}");
        }
        s
    }

    pub fn parse(s: &str) -> Option<LineMap> {
        let mut it = s.lines();
        let source = it.next()?.to_string();
        let lines = it.map(|l| l.trim().parse().ok()).collect::<Option<Vec<u32>>>()?;
        Some(LineMap { source, lines })
    }

    /// Markdown line for a 1-based LaTeX line.
    pub fn md_line(&self, tex_line: u32) -> Option<u32> {
        self.lines.get((tex_line as usize).checked_sub(1)?).copied()
    }

    /// First LaTeX line (1-based) that comes from this Markdown line or,
    /// if none does, from the next one after it.
    pub fn tex_line(&self, md_line: u32) -> Option<u32> {
        let exact = self.lines.iter().position(|&l| l == md_line);
        let after = || self.lines.iter().enumerate().filter(|(_, l)| **l > md_line).min_by_key(|(i, l)| (**l, *i)).map(|(i, _)| i);
        exact.or_else(after).map(|i| i as u32 + 1)
    }
}

#[cfg(test)]
mod tests;
