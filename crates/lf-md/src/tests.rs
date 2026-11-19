use super::*;

fn conv(src: &str) -> Output {
    convert(src, &Options::default())
}

fn body(o: &Output) -> &str {
    let s = o.tex.split("\\begin{document}\n").nth(1).unwrap();
    s.split("\\end{document}").next().unwrap()
}

#[test]
fn headings_paragraphs_and_inline_markup() {
    let o = conv("# Intro {#sec:intro}\n\nSome *emph*, **bold**, `a_b{}` and ~~old~~ text.\nNext line.\n");
    let b = body(&o);
    assert!(b.contains("\\section{Intro}\\label{sec:intro}"), "{b}");
    assert!(b.contains("Some \\emph{emph}, \\textbf{bold}, \\texttt{a\\_b\\{\\}} and \\sout{old} text.\nNext line."), "{b}");
}

#[test]
fn chapters_in_report() {
    let o = conv("---\nclass: report\n---\n# One\n## Two\n");
    let b = body(&o);
    assert!(b.contains("\\chapter{One}") && b.contains("\\section{Two}"), "{b}");
    assert_eq!(headings("---\nclass: report\n---\n# One\n## Two\n"), vec![(1, "One".into(), 4), (2, "Two".into(), 5)]);
    assert_eq!(headings("# A\n```\n# not\n```\nB\n---\n"), vec![(2, "A".into(), 1), (3, "B".into(), 5)]);
}

#[test]
fn special_characters_are_escaped() {
    let o = conv("50% of R&D costs $5 #1 a_b ^ {x} \\\\ C:\\\\path\n");
    let b = body(&o);
    assert!(b.contains("50\\% of R\\&D"), "{b}");
    assert!(b.contains("\\#1 a\\_b \\^{} \\{x\\}"), "{b}");
    assert!(b.contains("\\textbackslash{}"), "{b}");
}

#[test]
fn math_is_passed_through() {
    let src = "Inline $a_b^2 + \\frac{1}{2}$ here.\n\n$$\n\\int_0^1 x\\,dx\n$$\n\n$$\n\\begin{aligned} a &= b \\\\ c &= d \\end{aligned}\n$$\n";
    let o = conv(src);
    let b = body(&o);
    assert!(b.contains("\\(a_b^2 + \\frac{1}{2}\\)"), "{b}");
    assert!(b.contains("\\[\n\\int_0^1 x\\,dx\n\\]"), "{b}");
    assert!(b.contains("\\begin{aligned} a &= b \\\\ c &= d \\end{aligned}"), "{b}");
}

#[test]
fn raw_latex_environments_and_commands() {
    let src = "Text with \\cite[p.~3]{knuth} and \\ref{fig:x}.\n\n\\begin{align}\n  a &= b \\\\\n  c &= d\n\\end{align}\n\nAfter.\n\\newpage\n";
    let o = conv(src);
    let b = body(&o);
    assert!(b.contains("\\cite[p.~3]{knuth} and \\ref{fig:x}."), "{b}");
    assert!(b.contains("\\begin{align}\n  a &= b \\\\\n  c &= d\n\\end{align}"), "{b}");
    assert!(b.contains("\\newpage"), "{b}");
    // the raw block keeps its Markdown lines
    let l = o.tex.lines().position(|l| l == "  c &= d").unwrap();
    assert_eq!(o.map[l], 5);
}

#[test]
fn lists_tasks_quotes_rules() {
    let src = "- a\n- b\n  1. x\n  2. y\n\n3. three\n4. four\n\n- [ ] todo\n- [x] done\n\n> quoted\n\n---\n";
    let b = conv(src).tex;
    assert!(b.contains("\\begin{itemize}\n\\item a\n\\item b\n\\begin{enumerate}\n\\item x\n\\item y\n\\end{enumerate}"), "{b}");
    assert!(b.contains("\\begin{enumerate}[start=3]"), "{b}");
    assert!(b.contains("\\item[$\\square$] todo") && b.contains("\\item[$\\boxtimes$] done"), "{b}");
    assert!(b.contains("\\begin{quote}\nquoted"), "{b}");
    assert!(b.contains("\\rule{\\linewidth}"), "{b}");
}

#[test]
fn code_blocks_tables_images_links_footnotes() {
    let src = "```python\nprint(\"%_{}\")\n```\n\n```{=latex}\n\\vspace{1cm}\n```\n\n| A | B |\n|:--|--:|\n| 1 | 2 |\n\n![A *plot*](figs/p.png \"fig:p\")\n\nSee [docs](https://x.org/a_b#c%20d), [sec](#sec:intro) and[^n].\n\n[^n]: A note.\n";
    let o = conv(src);
    let b = body(&o);
    assert!(b.contains("\\begin{lstlisting}[language=Python]\nprint(\"%_{}\")\n\\end{lstlisting}"), "{b}");
    assert!(b.contains("\\vspace{1cm}") && !b.contains("lstlisting}\n\\vspace"), "{b}");
    assert!(b.contains("\\begin{tabular}{lr}\n\\toprule\nA & B \\\\\n\\midrule\n1 & 2 \\\\\n\\bottomrule"), "{b}");
    assert!(b.contains("{figs/p.png}") && b.contains("\\caption{A \\emph{plot}}") && b.contains("\\label{fig:p}"), "{b}");
    assert!(b.contains("\\href{https://x.org/a_b\\#c\\%20d}{docs}"), "{b}");
    assert!(b.contains("\\hyperref[sec:intro]{sec}"), "{b}");
    assert!(b.contains("and\\footnote{A note.}."), "{b}");
    assert!(!b.contains("A note.\n"), "definition not repeated: {b}");
}

#[test]
fn citations() {
    let b = conv("As shown [@knuth1984; @lamport] and [@knuth1984, p. 3]. Mail a@b.de [x].\n").tex;
    assert!(b.contains("\\cite{knuth1984,lamport}"), "{b}");
    assert!(b.contains("\\cite[p. 3]{knuth1984}"), "{b}");
    assert!(b.contains("a@b.de [x]"), "{b}");
}

#[test]
fn front_matter_builds_the_preamble() {
    let src = "---\ntitle: \"Notes\"\nauthor: [A, B]\nlang: ngerman\npackages:\n  - siunitx\n  - \"[margin=1cm]{geometry}\"\npreamble: |\n  \\newcommand{\\R}{\\mathbb{R}}\nbibliography: refs.bib\nengine: xelatex\ntoc: yes\nnumbersections: false\n---\nHallo $\\R$\n";
    let f = front_matter(src);
    assert_eq!(f.engine.as_deref(), Some("xelatex"));
    let o = convert(src, &Options { base_dir: "docs".into(), unicode_engine: true, source_name: "n.md".into() });
    let t = &o.tex;
    assert!(t.contains("\\usepackage{fontspec}") && !t.contains("fontenc"), "{t}");
    assert!(t.contains("\\usepackage[ngerman]{babel}"), "{t}");
    assert!(t.contains("\\usepackage{siunitx}") && t.contains("\\usepackage[margin=1cm]{geometry}"), "{t}");
    assert!(t.contains("\\newcommand{\\R}{\\mathbb{R}}"), "{t}");
    assert!(t.contains("\\addbibresource{docs/refs.bib}") && t.contains("\\printbibliography"), "{t}");
    assert!(t.contains("\\graphicspath{{docs/}}"), "{t}");
    assert!(t.contains("\\title{Notes}") && t.contains("\\author{A \\and B}") && t.contains("\\maketitle\n\\tableofcontents"), "{t}");
    assert!(t.contains("\\setcounter{secnumdepth}{0}"), "{t}");
    // the body line maps to Markdown line 15
    let l = o.tex.lines().position(|l| l.starts_with("Hallo")).unwrap();
    assert_eq!(o.map[l], 15);
}

#[test]
fn line_map_round_trip() {
    let src = "# A\n\nText one\ntext two\n\n- item\n";
    let o = conv(src);
    assert_eq!(o.map.len(), o.tex.lines().count());
    let m = LineMap { source: "x.md".into(), lines: o.map.clone() };
    let m2 = LineMap::parse(&m.to_text()).unwrap();
    assert_eq!(m, m2);
    let tl = m.tex_line(4).unwrap();
    assert_eq!(o.tex.lines().nth(tl as usize - 1), Some("text two"));
    assert_eq!(m.md_line(tl), Some(4));
    // a blank Markdown line maps to the next content
    assert_eq!(m.md_line(m.tex_line(5).unwrap()), Some(6));
}

#[test]
fn lexer_covers_everything() {
    let src = "---\ntitle: x\n---\n# Head\nText *a* `c` $x^2$ \\cite{k} [l](u) ä\\ö\n- [ ] task\n> q\n```\ncode $\n```\n$$\na\n$$\n\\begin{align}\nx\n\\end{align}\n";
    let spans = tokenize(src);
    let mut pos = 0;
    for s in &spans {
        assert_eq!(s.range.start, pos);
        pos = s.range.end;
    }
    assert_eq!(pos, src.len());
    let tok_at = |needle: &str| spans.iter().find(|s| s.range.contains(&src.find(needle).unwrap())).unwrap().tok;
    assert_eq!(tok_at("title"), MdTok::Meta);
    assert_eq!(tok_at("# Head"), MdTok::Heading);
    assert_eq!(tok_at("x^2"), MdTok::Math);
    assert_eq!(tok_at("`c`"), MdTok::Code);
    assert_eq!(tok_at("\\cite"), MdTok::Command);
    assert_eq!(tok_at("(u)"), MdTok::Link);
    assert_eq!(tok_at("[ ]"), MdTok::Marker);
    assert_eq!(tok_at("code $"), MdTok::Code);
    assert_eq!(tok_at("a\n$$"), MdTok::Math);
    assert_eq!(tok_at("x\n\\end"), MdTok::Math);
}
