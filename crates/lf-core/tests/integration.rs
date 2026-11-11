//! End-to-end tests with the real TeX installation: compile projects through
//! latexmk, render PDF pages, SyncTeX, templates and ZIP import.
//!
//! The 500-page test takes a while; run everything with
//! `cargo test -p lf-core --test integration -- --include-ignored`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use lf_core::project::Project;
use lf_tex::Engine;
use lf_tex::compile::{self, Cancel, Outcome};

fn tempdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("arvidleaf-it-{tag}-{}-{}", std::process::id(), lf_core::history::now_ms()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn build(root: &Path, engine: Engine) -> Outcome {
    let mut p = Project::open(root).unwrap();
    p.meta.engine = engine;
    let job = p.compile_job().expect("main file");
    compile::run(&job, &Cancel::default())
}

fn assert_ok(out: &Outcome) {
    assert!(
        out.ok,
        "compile failed: launch={:?} errors={:#?}\n{}",
        out.launch_error,
        out.entries.iter().filter(|e| e.level == lf_tex::log::Level::Error).collect::<Vec<_>>(),
        out.output.lines().rev().take(30).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn all_engines_compile_a_minimal_document() {
    for engine in Engine::ALL {
        let d = tempdir(&format!("min-{}", engine.label()));
        write(&d, "main.tex", "\\documentclass{article}\\begin{document}Hello $x^2$ ä ö ü\\end{document}\n");
        let out = build(&d, engine);
        assert_ok(&out);
        assert_eq!(out.pages, Some(1));
        assert!(out.pdf.as_ref().unwrap().with_extension("synctex.gz").exists(), "SyncTeX data for {}", engine.label());
    }
}

#[test]
fn own_packages_includes_images_and_bibtex() {
    let d = tempdir("imports");
    write(&d, "main.tex", r"\documentclass{article}
\usepackage{graphicx}
\usepackage{lib/house}     % own package in a subfolder, found without a path
\usepackage{tikz}
\graphicspath{{figures/}}
\begin{document}
\housegreeting
\input{chapters/one}
\include{chapters/two}
\includegraphics[width=2cm]{dot}
\cite{knuth}
\bibliographystyle{plain}
\bibliography{refs/library}
\end{document}
");
    write(&d, "lib/house.sty", "\\ProvidesPackage{lib/house}\n\\newcommand{\\housegreeting}{Greetings from my own package.}\n");
    write(&d, "chapters/one.tex", "\\section{One}\\input{chapters/nested/deep}\n");
    write(&d, "chapters/nested/deep.tex", "Deep \\begin{tikzpicture}\\draw (0,0) circle (1);\\end{tikzpicture}\n");
    write(&d, "chapters/two.tex", "\\section{Two}\\label{sec:two} See \\ref{sec:two}.\n");
    write(&d, "refs/library.bib", "@book{knuth, author={Donald Knuth}, title={The TeXbook}, year={1984}, publisher={AW}}\n");
    // a tiny PNG image (1x1 pixel)
    let png: [u8; 67] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08,
        0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00,
        0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    std::fs::create_dir_all(d.join("figures")).unwrap();
    std::fs::write(d.join("figures/dot.png"), png).unwrap();
    let out = build(&d, Engine::Pdflatex);
    assert_ok(&out);
    let log = &out.log;
    assert!(!log.contains("Citation `knuth' on page 1 undefined"), "bibtex resolved the citation");
    assert!(!out.entries.iter().any(|e| e.message.contains("Reference") && e.message.contains("undefined")), "{:#?}", out.entries);
}

#[test]
fn paths_with_spaces_and_umlauts() {
    let d = tempdir("paths");
    let root = d.join("Meine Arbeit – Übung");
    write(&root, "Kapitel 1/Einführung.tex", "Text aus einem Ordner mit Leerzeichen.\n");
    write(&root, "main.tex", "\\documentclass{article}\\begin{document}\\input{\"Kapitel 1/Einführung\"}\\end{document}\n");
    let out = build(&root, Engine::Pdflatex);
    assert_ok(&out);
}

#[test]
fn missing_package_is_reported_with_its_file() {
    let d = tempdir("missing");
    write(&d, "main.tex", "\\documentclass{article}\n\\usepackage{doesnotexist-arvidleaf}\n\\begin{document}x\\end{document}\n");
    let out = build(&d, Engine::Pdflatex);
    assert!(!out.ok);
    assert_eq!(out.missing.iter().map(|m| m.file.as_str()).collect::<Vec<_>>(), vec!["doesnotexist-arvidleaf.sty"]);
    let e = out.entries.iter().find(|e| e.level == lf_tex::log::Level::Error).unwrap();
    assert_eq!(e.file.as_deref(), Some("main.tex"), "project files are reported relative to the root");
    assert_eq!(e.line, Some(2));
    // a second run after fixing the cause must really run again
    write(&d, "doesnotexist-arvidleaf.sty", "\\ProvidesPackage{doesnotexist-arvidleaf}\n");
    let out = build(&d, Engine::Pdflatex);
    assert_ok(&out);
}

#[test]
fn errors_point_to_file_and_line() {
    let d = tempdir("errors");
    write(&d, "main.tex", "\\documentclass{article}\n\\begin{document}\n\\input{sub}\n\\end{document}\n");
    write(&d, "sub.tex", "line one\n\nHello \\undefinedmacro\n");
    let out = build(&d, Engine::Pdflatex);
    let e = out.entries.iter().find(|e| e.message.contains("Undefined control sequence")).expect("error reported");
    assert_eq!(e.file.as_deref(), Some("sub.tex"));
    assert_eq!(e.line, Some(3));
}

#[test]
fn synctex_roundtrip() {
    let d = tempdir("synctex");
    let mut body = String::new();
    for i in 1..=120 {
        body.push_str(&format!("Paragraph number {i} with some text to fill the page.\n\n"));
    }
    write(&d, "main.tex", &format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\\end{{document}}\n"));
    let out = build(&d, Engine::Pdflatex);
    assert_ok(&out);
    let pdf = out.pdf.unwrap();
    // line of paragraph 100
    let line = 2 + 2 * 99 + 1;
    let hit = lf_tex::synctex::forward(&pdf, &d.join("main.tex"), line, 0).expect("forward search");
    assert!(hit.page >= 2, "paragraph 100 is not on page 1: {hit:?}");
    let (file, back) = lf_tex::synctex::inverse(&pdf, hit.page, hit.x + 5.0, hit.y + hit.h * 0.5).expect("inverse search");
    assert!(file.ends_with("main.tex"), "{file:?}");
    assert!((back as i64 - line as i64).abs() <= 2, "inverse gave line {back}, expected ~{line}");
}

#[test]
fn builtin_templates_compile() {
    let german = lf_tex::tools::kpsewhich("ngerman.ldf").is_some();
    let home = tempdir("tpl");
    for t in lf_core::templates::builtins() {
        let dest = home.join(t.id.replace(':', "-"));
        let vars = lf_core::templates::Vars { title: "Test".into(), author: "Tester".into(), project: "p".into() };
        lf_core::templates::instantiate(&t, &dest, &vars).unwrap();
        let p = Project::open(&dest).unwrap();
        let out = compile::run(&p.compile_job().unwrap(), &Cancel::default());
        if t.id == "builtin:article-de" && !german {
            assert_eq!(out.missing.first().map(|m| m.file.as_str()), Some("ngerman.ldf"), "German template reports the missing language");
            eprintln!("skipped {} (texlive-langgerman not installed)", t.id);
            continue;
        }
        assert_ok(&out);
        assert!(out.pages.unwrap_or(0) >= 1, "{}", t.id);
    }
}

#[test]
fn overleaf_zip_import_compiles() {
    // an Overleaf "Download Source" archive: flat files at the top level
    let d = tempdir("zip");
    let src = d.join("src");
    write(&src, "main.tex", "\\documentclass{article}\\usepackage{graphicx}\\begin{document}\\input{sections/a}\\cite{x}\\bibliographystyle{plain}\\bibliography{references}\\end{document}\n");
    write(&src, "sections/a.tex", "Section A.\n");
    write(&src, "references.bib", "@misc{x, title={X}, year={2020}}\n");
    let zip = d.join("My Overleaf Paper.zip");
    lf_core::archive::export_zip(&src, &zip).unwrap();
    let root = lf_core::archive::import_zip(&zip, &d.join("projects")).unwrap();
    let p = Project::open(&root).unwrap();
    assert_eq!(p.meta.main.as_deref(), Some("main.tex"));
    let out = compile::run(&p.compile_job().unwrap(), &Cancel::default());
    assert_ok(&out);
}

#[test]
fn pdf_first_page_renders_fast() {
    let d = tempdir("render");
    write(&d, "main.tex", "\\documentclass{article}\\usepackage{lipsum}\\begin{document}\\lipsum[1-40]\\end{document}\n");
    let out = build(&d, Engine::Pdflatex);
    assert_ok(&out);
    let pdf = out.pdf.unwrap();
    let t = Instant::now();
    let info = lf_pdf::info(&pdf).unwrap();
    let img = lf_pdf::render_page(&pdf, 1, 110.0).unwrap();
    let ms = t.elapsed().as_millis();
    assert!(info.pages.len() >= 3);
    assert!(img.width > 800 && img.height > 1000, "{}x{}", img.width, img.height);
    // goal from the plan: first page visible < 300 ms after compiling
    assert!(ms < 300, "info + first page took {ms} ms");
    eprintln!("pdfinfo + first page at 110 dpi: {ms} ms");
}

/// R4: no page or time limits. 500+ pages with TikZ drawings on every page.
#[test]
#[ignore = "slow (about a minute); run with --include-ignored"]
fn large_document_without_limits() {
    let d = tempdir("large");
    let mut body = String::new();
    for i in 1..=520 {
        body.push_str(&format!(
            "\\section{{Section {i}}}\\label{{sec:{i}}}\n\\lipsum[{}]\n\\begin{{center}}\\begin{{tikzpicture}}\n\\foreach \\k in {{1,...,40}} {{ \\draw[rotate=\\k*9] (0,0) ellipse (2 and 0.{}); }}\n\\end{{tikzpicture}}\\end{{center}}\nSee Section~\\ref{{sec:{}}}.\n\\newpage\n",
            i % 150 + 1,
            i % 9 + 1,
            (i % 520) + 1
        ));
    }
    write(&d, "main.tex", &format!("\\documentclass{{article}}\n\\usepackage{{tikz,lipsum}}\n\\begin{{document}}\n\\tableofcontents\n{body}\\end{{document}}\n"));
    let t = Instant::now();
    let out = build(&d, Engine::Pdflatex);
    assert_ok(&out);
    let pages = out.pages.unwrap();
    assert!(pages >= 520, "{pages} pages");
    let pdf = out.pdf.unwrap();
    let info = lf_pdf::info(&pdf).unwrap();
    assert_eq!(info.pages.len() as u32, pages);
    // a page deep inside renders quickly as well
    let r = Instant::now();
    lf_pdf::render_page(&pdf, 400, 110.0).unwrap();
    eprintln!("{pages} pages compiled in {:.1} s; page 400 rendered in {} ms", t.elapsed().as_secs_f32(), r.elapsed().as_millis());
}
