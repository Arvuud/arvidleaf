//! Finding the DOI or arXiv ID of a PDF.
//!
//! Reads the first page with `pdftotext` (poppler, already needed for the PDF preview) and
//! the XMP metadata with `pdfinfo -meta`. Only the first page is searched for DOIs, because
//! later pages usually cite other works.

use super::{Identifier, find_dois};
use crate::library::normalize_arxiv;
use std::path::Path;
use std::process::Command;

fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<String, String> {
    let mut c = Command::new(program);
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let o = c.output().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { format!("{program} is not installed (poppler)") } else { e.to_string() })?;
    if !o.status.success() {
        return Err(format!("{program}: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Identifier of a PDF, or `None` if neither page 1 nor the metadata contain one.
pub fn identify(path: &Path) -> Result<Option<Identifier>, String> {
    let p = path.as_os_str();
    let text = run("pdftotext", &["-f".as_ref(), "1".as_ref(), "-l".as_ref(), "1".as_ref(), "-enc".as_ref(), "UTF-8".as_ref(), p, "-".as_ref()])?;
    let meta = run("pdfinfo", &["-meta".as_ref(), p]).unwrap_or_default();
    Ok(identify_text(&text, &meta))
}

/// Decides from page-1 text and metadata. A DOI wins over an arXiv ID (the journal record
/// is more complete; the arXiv ID is found again through it).
pub fn identify_text(page1: &str, meta: &str) -> Option<Identifier> {
    // metadata first: publishers put the article's own DOI there (prism:doi, dc:identifier)
    for m in ["prism:doi", "dc:identifier", "pdfx:doi", "crossmark:doi"] {
        if let Some(p) = meta.find(m)
            && let Some(d) = find_dois(&meta[p..(p + 300).min(meta.len())]).into_iter().next()
        {
            return doi_or_arxiv(&d);
        }
    }
    if let Some(d) = find_dois(page1).into_iter().next() {
        return doi_or_arxiv(&d);
    }
    // arXiv stamp in the margin: "arXiv:1706.03762v7 [cs.CL] 2 Aug 2023"
    let l = page1.find("arXiv:")?;
    let id: String = page1[l..].chars().take_while(|c| !c.is_whitespace()).collect();
    normalize_arxiv(&id).map(Identifier::Arxiv)
}

fn doi_or_arxiv(d: &str) -> Option<Identifier> {
    let d = d.to_lowercase();
    match d.strip_prefix("10.48550/arxiv.") {
        Some(a) => normalize_arxiv(a).map(Identifier::Arxiv),
        None => Some(Identifier::Doi(d)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_text() {
        let page = "Journal of X 19 (1998) 377\nhttps://doi.org/10.1002/jcc.540190402\nReferences: doi:10.1000/other";
        assert_eq!(identify_text(page, ""), Some(Identifier::Doi("10.1002/jcc.540190402".into())));
        let pre = "Attention Is All You Need\narXiv:1706.03762v7 [cs.CL] 2 Aug 2023\nAbstract";
        assert_eq!(identify_text(pre, ""), Some(Identifier::Arxiv("1706.03762".into())));
        let meta = "<rdf><prism:doi>10.1016/j.cell.2020.01.001</prism:doi></rdf>";
        assert_eq!(identify_text("cites 10.1000/x", meta), Some(Identifier::Doi("10.1016/j.cell.2020.01.001".into())));
        assert_eq!(identify_text("nothing here", ""), None);
    }
}
