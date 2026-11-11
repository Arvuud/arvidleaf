//! SyncTeX via the `synctex` command line tool.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A box in the PDF (PDF points, origin top-left of the page).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// 1-based page
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

fn field(out: &str, key: &str) -> Option<f32> {
    out.lines().find_map(|l| l.strip_prefix(key)).and_then(|v| v.trim().parse().ok())
}

fn parse_view(out: &str) -> Option<Hit> {
    let first = &out[out.find("Page:")?..];
    // only the first record
    let rec = match first[5..].find("Page:") {
        Some(p) => &first[..p + 5],
        None => first,
    };
    let page = field(rec, "Page:")? as u32;
    let hh = field(rec, "h:").unwrap_or(0.0);
    let v = field(rec, "v:").unwrap_or(0.0);
    let w = field(rec, "W:").unwrap_or(0.0);
    let h = field(rec, "H:").unwrap_or(0.0);
    let y = field(rec, "y:").unwrap_or(v);
    Some(Hit { page, x: hh, y: if h > 0.0 { v - h } else { y }, w, h })
}

/// Source position → PDF position.
pub fn forward(pdf: &Path, input: &Path, line: u32, column: u32) -> Option<Hit> {
    let o = Command::new("synctex")
        .arg("view")
        .arg("-i")
        .arg(format!("{}:{}:{}", line, column, input.display()))
        .arg("-o")
        .arg(pdf)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    parse_view(&String::from_utf8_lossy(&o.stdout))
}

/// PDF position (points from the top-left) → source file and line.
pub fn inverse(pdf: &Path, page: u32, x: f32, y: f32) -> Option<(PathBuf, u32)> {
    let o = Command::new("synctex")
        .arg("edit")
        .arg("-o")
        .arg(format!("{}:{:.2}:{:.2}:{}", page, x, y, pdf.display()))
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&o.stdout);
    let input = s.lines().find_map(|l| l.strip_prefix("Input:"))?.trim().to_string();
    let line = s.lines().find_map(|l| l.strip_prefix("Line:"))?.trim().parse().ok()?;
    Some((PathBuf::from(input), line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_view_output() {
        let out = "This is SyncTeX command line utility, version 1.5\nSyncTeX result begin\nOutput:/tmp/a.pdf\nPage:2\nx:133.768356\ny:150.500000\nh:133.768356\nv:152.000000\nW:343.711060\nH:8.000000\nbefore:\noffset:0\nmiddle:\nafter:\nSyncTeX result end\n";
        let h = parse_view(out).unwrap();
        assert_eq!(h.page, 2);
        assert!((h.y - 144.0).abs() < 1e-3);
        assert!((h.w - 343.71106).abs() < 1e-3);
    }
}
