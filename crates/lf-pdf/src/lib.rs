//! PDF page geometry and rendering.
//!
//! The backend is poppler's `pdfinfo`/`pdftoppm`, run as separate processes
//! (raw PPM over stdout, no image decoding). A pool of worker threads renders
//! the most recently requested pages first, so whatever is visible right now
//! wins over pages the user already scrolled past.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

/// Page sizes in PDF points (1/72 inch), rotation applied.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfInfo {
    pub pages: Vec<[f32; 2]>,
}

/// Reads page count and sizes with `pdfinfo`.
pub fn info(path: &Path) -> Result<PdfInfo, String> {
    let run = |args: &[&str]| -> Result<String, String> {
        let o = tool("pdfinfo").args(args).arg(path).stderr(Stdio::piped()).output().map_err(|e| format!("pdfinfo: {e}"))?;
        if !o.status.success() {
            return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    };
    let head = run(&[])?;
    let n: usize = head.lines().find_map(|l| l.strip_prefix("Pages:")).and_then(|v| v.trim().parse().ok()).ok_or("no page count")?;
    if n == 0 {
        return Ok(PdfInfo::default());
    }
    let all = run(&["-f", "1", "-l", &n.to_string()])?;
    Ok(parse_info(&all, n))
}

fn parse_info(s: &str, n: usize) -> PdfInfo {
    let mut pages = vec![[595.0f32, 842.0]; n];
    let mut rot = vec![0u32; n];
    for l in s.lines() {
        let Some(rest) = l.strip_prefix("Page") else { continue };
        let rest = rest.trim_start();
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(p) = num.parse::<usize>() else { continue };
        if p == 0 || p > n {
            continue;
        }
        let after = rest[num.len()..].trim_start();
        if let Some(sz) = after.strip_prefix("size:") {
            let mut it = sz.split_whitespace();
            let w = it.next().and_then(|v| v.parse().ok());
            let _x = it.next();
            let h = it.next().and_then(|v| v.parse().ok());
            if let (Some(w), Some(h)) = (w, h) {
                pages[p - 1] = [w, h];
            }
        } else if let Some(r) = after.strip_prefix("rot:") {
            rot[p - 1] = r.trim().parse().unwrap_or(0);
        }
    }
    for (pg, r) in pages.iter_mut().zip(rot) {
        if r % 180 == 90 {
            pg.swap(0, 1);
        }
    }
    PdfInfo { pages }
}

/// An RGB8 image.
#[derive(Clone, Debug)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

/// Parses a binary PPM (P6, maxval 255).
pub fn parse_ppm(data: &[u8]) -> Result<Image, String> {
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < data.len() && data[i] == b'#' {
            while i < data.len() && data[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let s = i;
        while i < data.len() && !data[i].is_ascii_whitespace() {
            i += 1;
        }
        if s == i {
            return Err("truncated PPM header".into());
        }
        fields.push(std::str::from_utf8(&data[s..i]).map_err(|_| "bad PPM header")?.to_string());
    }
    i += 1; // single whitespace after maxval
    if fields[0] != "P6" || fields[3] != "255" {
        return Err(format!("unsupported PPM ({} / {})", fields[0], fields[3]));
    }
    let w: usize = fields[1].parse().map_err(|_| "bad width")?;
    let h: usize = fields[2].parse().map_err(|_| "bad height")?;
    let need = w * h * 3;
    if data.len() < i + need {
        return Err("truncated PPM data".into());
    }
    Ok(Image { width: w, height: h, rgb: data[i..i + need].to_vec() })
}

/// A poppler command; on Windows without a console window.
fn tool(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Renders one page (1-based) at `dpi`.
pub fn render_page(path: &Path, page: u32, dpi: f32) -> Result<Image, String> {
    let o = tool("pdftoppm")
        .args(["-f", &page.to_string(), "-l", &page.to_string(), "-r", &format!("{dpi:.1}"), "-singlefile"])
        .arg(path)
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("pdftoppm: {e}"))?;
    if !o.status.success() || o.stdout.is_empty() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    parse_ppm(&o.stdout)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// Caller-defined document generation (changes when the PDF is rebuilt).
    pub generation: u64,
    pub path: PathBuf,
    /// 1-based page
    pub page: u32,
    pub dpi: f32,
}

pub struct Rendered {
    pub request: Request,
    pub result: Result<Image, String>,
}

struct Queue {
    items: Vec<Request>,
    shutdown: bool,
}

/// Background page renderer.
pub struct Renderer {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    pub rx: Receiver<Rendered>,
    in_flight: Arc<Mutex<HashSet<(u64, u32, u32)>>>,
}

fn key(r: &Request) -> (u64, u32, u32) {
    (r.generation, r.page, (r.dpi * 10.0) as u32)
}

impl Renderer {
    /// `notify` is called after each finished page (e.g. to request a repaint).
    pub fn new(threads: usize, notify: impl Fn() + Send + Sync + 'static) -> Renderer {
        let queue = Arc::new((Mutex::new(Queue { items: Vec::new(), shutdown: false }), Condvar::new()));
        let (tx, rx): (Sender<Rendered>, Receiver<Rendered>) = channel();
        let notify = Arc::new(notify);
        let in_flight = Arc::new(Mutex::new(HashSet::new()));
        for _ in 0..threads.max(1) {
            let queue = queue.clone();
            let tx = tx.clone();
            let notify = notify.clone();
            let in_flight = in_flight.clone();
            std::thread::spawn(move || {
                loop {
                    let req = {
                        let (lock, cv) = &*queue;
                        let mut q = lock.lock().unwrap();
                        loop {
                            if q.shutdown {
                                return;
                            }
                            if let Some(r) = q.items.pop() {
                                break r;
                            }
                            q = cv.wait(q).unwrap();
                        }
                    };
                    let result = render_page(&req.path, req.page, req.dpi);
                    in_flight.lock().unwrap().remove(&key(&req));
                    if tx.send(Rendered { request: req, result }).is_err() {
                        return;
                    }
                    notify();
                }
            });
        }
        Renderer { queue, rx, in_flight }
    }

    /// Queues a page unless the same page/dpi is already queued or rendering.
    /// Newer requests are served first.
    pub fn request(&self, r: Request) {
        let k = key(&r);
        if !self.in_flight.lock().unwrap().insert(k) {
            return;
        }
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap().items.push(r);
        cv.notify_one();
    }

    /// Drops queued (not yet started) requests that fail `keep`.
    pub fn retain(&self, keep: impl Fn(&Request) -> bool) {
        let (lock, _) = &*self.queue;
        let mut q = lock.lock().unwrap();
        let mut fl = self.in_flight.lock().unwrap();
        q.items.retain(|r| {
            let k = keep(r);
            if !k {
                fl.remove(&key(r));
            }
            k
        });
    }

    pub fn pending(&self) -> usize {
        self.in_flight.lock().unwrap().len()
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap().shutdown = true;
        cv.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppm() {
        let mut d = b"P6\n# c\n2 1\n255\n".to_vec();
        d.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        let img = parse_ppm(&d).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.rgb, vec![1, 2, 3, 4, 5, 6]);
        assert!(parse_ppm(b"P6\n2 1\n255\n\x01").is_err());
    }

    #[test]
    fn info_parsing() {
        let s = "Page    1 size: 595.276 x 841.89 pts (A4)\nPage    1 rot:  0\nPage    2 size: 595.276 x 841.89 pts (A4)\nPage    2 rot:  90\n";
        let i = parse_info(s, 2);
        assert_eq!(i.pages[0], [595.276, 841.89]);
        assert_eq!(i.pages[1], [841.89, 595.276]);
    }
}
