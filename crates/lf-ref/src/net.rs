//! HTTP GET through the `curl` program.
//!
//! arvidleaf runs external tools as processes (latexmk, pdftoppm); curl fits that model:
//! no TLS stack in the build, and it ships with Linux, macOS and Windows 10+. Calls block,
//! so run them on a worker thread; `cancel` stops a running request.
//!
//! Only identifiers the user typed in are sent (a DOI, arXiv ID or ISBN). The user agent is
//! `arvidleaf/<version>`; a contact address is added only if one is configured.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum NetError {
    /// curl is not installed.
    NoCurl,
    /// No connection (DNS, refused, reset).
    Offline(String),
    Timeout,
    NotFound,
    RateLimited,
    Http(u16),
    Cancelled,
    Other(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::NoCurl => write!(f, "curl is not installed – online import needs it"),
            NetError::Offline(m) => write!(f, "offline – no connection ({m})"),
            NetError::Timeout => write!(f, "the server did not answer in time"),
            NetError::NotFound => write!(f, "not found"),
            NetError::RateLimited => write!(f, "too many requests – try again in a minute"),
            NetError::Http(c) => write!(f, "server error (HTTP {c})"),
            NetError::Cancelled => write!(f, "cancelled"),
            NetError::Other(m) => write!(f, "{m}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NetOptions {
    /// Optional contact address (Crossref's "polite pool"). Empty by default.
    pub mailto: String,
    /// Seconds per attempt (default 15).
    pub timeout: u64,
}

impl NetOptions {
    fn user_agent(&self) -> String {
        let base = format!("arvidleaf/{}", env!("CARGO_PKG_VERSION"));
        if self.mailto.trim().is_empty() { base } else { format!("{base} (mailto:{})", self.mailto.trim()) }
    }
}

/// GET `url` and return the body. `accept` sets the Accept header.
pub fn get(url: &str, accept: Option<&str>, opts: &NetOptions, cancel: &AtomicBool) -> Result<String, NetError> {
    match get_with(url, accept, opts, cancel, true) {
        // curl before 7.71 (e.g. an older Windows 10) does not know --retry-all-errors
        Err(NetError::Other(m)) if m.contains("retry-all-errors") || m.contains("option") && m.contains("unknown") => get_with(url, accept, opts, cancel, false),
        r => r,
    }
}

fn get_with(url: &str, accept: Option<&str>, opts: &NetOptions, cancel: &AtomicBool, retry_all: bool) -> Result<String, NetError> {
    let timeout = if opts.timeout == 0 { 15 } else { opts.timeout };
    let mut c = Command::new("curl");
    c.args(["-sS", "-L", "--compressed", "--retry", "2", "--retry-delay", "1", "--connect-timeout", "8"]);
    if retry_all {
        c.arg("--retry-all-errors");
    }
    c.args(["--max-time", &timeout.to_string(), "-A", &opts.user_agent(), "-w", "\n%{http_code}"]);
    if let Some(a) = accept {
        c.args(["-H", &format!("Accept: {a}")]);
    }
    c.arg("--").arg(url);
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = c.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { NetError::NoCurl } else { NetError::Other(e.to_string()) })?;
    // read the pipes on threads so a large answer cannot block curl
    let mut out = child.stdout.take().ok_or_else(|| NetError::Other("no stdout".into()))?;
    let mut err = child.stderr.take().ok_or_else(|| NetError::Other("no stderr".into()))?;
    let out_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = out.read_to_end(&mut v);
        v
    });
    let err_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NetError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(e) => return Err(NetError::Other(e.to_string())),
        }
    };
    let body = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    match status.code() {
        Some(0) => {}
        Some(6) | Some(7) | Some(35) | Some(52) | Some(55) | Some(56) => return Err(NetError::Offline(stderr.trim().trim_start_matches("curl: ").to_string())),
        Some(28) => return Err(NetError::Timeout),
        _ => return Err(NetError::Other(format!("curl failed: {}", stderr.trim()))),
    }
    let body = String::from_utf8_lossy(&body).into_owned();
    let (body, code) = body.rsplit_once('\n').unwrap_or(("", &body));
    match code.trim().parse::<u16>().unwrap_or(0) {
        200..=299 => Ok(body.to_string()),
        404 | 410 => Err(NetError::NotFound),
        429 => Err(NetError::RateLimited),
        c => Err(NetError::Http(c)),
    }
}

/// Percent-encodes a path segment, keeping `/` (DOIs contain slashes).
pub fn encode_path(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding() {
        assert_eq!(encode_path("10.1002/(SICI)1096-987X(199803)19:4<377::AID-JCC1>3.0.CO;2-P"), "10.1002/%28SICI%291096-987X%28199803%2919%3A4%3C377%3A%3AAID-JCC1%3E3.0.CO%3B2-P");
        assert_eq!(encode_path("hep-th/9711200"), "hep-th/9711200");
    }

    #[test]
    fn user_agent_has_no_address_by_default() {
        assert!(!NetOptions::default().user_agent().contains('@'));
        assert!(NetOptions { mailto: "a@b.c".into(), timeout: 0 }.user_agent().ends_with("(mailto:a@b.c)"));
    }
}
