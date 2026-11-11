//! arvidleaf entry point: desktop GUI, headless compile, or a screenshot of the UI.

use std::path::PathBuf;

fn usage() {
    eprintln!(
        "Usage: arvidleaf [options] [folder | file.tex | archive.zip]\n\
         \n  (no options)            open the desktop app (with a project, file or ZIP)\
         \n  --compile <folder>      compile a project without GUI, print problems, exit\
         \n  --screenshot out.png    render the UI offscreen [--size WxH] [--frames N] [--light]\
         \n                          [--click F:X,Y] [--dclick F:X,Y] [--drag F:X0,Y0,X1,Y1]\
         \n                          [--type F:text] [--key F:Ctrl+Enter]\
         \n  --version               print the version\n  -h, --help              this help"
    );
}

/// Headless compile: exit code 0 = PDF without errors.
fn compile(dir: &str) -> i32 {
    let p = match lf_core::project::Project::open(std::path::Path::new(dir)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{dir}: {e}");
            return 2;
        }
    };
    let Some(job) = p.compile_job() else {
        eprintln!("{dir}: no main file (no .tex with \\documentclass)");
        return 2;
    };
    let out = lf_tex::compile::run(&job, &lf_tex::compile::Cancel::default());
    if let Some(e) = &out.launch_error {
        eprintln!("{e}");
        return 2;
    }
    for e in &out.entries {
        let loc = match (&e.file, e.line) {
            (Some(f), Some(l)) => format!("{f}:{l}: "),
            (Some(f), None) => format!("{f}: "),
            _ => String::new(),
        };
        println!("{:?}: {loc}{}", e.level, e.message);
    }
    for m in &out.missing {
        println!("missing: {} ({})", m.file, m.provider.as_deref().unwrap_or("unknown package"));
    }
    println!(
        "{} in {:.2} s, {} page(s), {} error(s), {} warning(s) -> {}",
        if out.ok { "OK" } else { "FAILED" },
        out.duration.as_secs_f32(),
        out.pages.unwrap_or(0),
        out.errors(),
        out.warnings(),
        out.pdf.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "no PDF".into())
    );
    if out.ok { 0 } else { 1 }
}

fn screenshot(args: &[String]) -> i32 {
    use lf_gui::offscreen::{Action, Options, key_action, render, save_png};
    let mut out = PathBuf::from("arvidleaf.png");
    let (mut w, mut h, mut frames) = (1440usize, 900usize, 40usize);
    let mut open = Vec::new();
    let mut dark = true;
    let mut actions = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let next = |i: &mut usize| -> String {
            *i += 1;
            args.get(*i).cloned().unwrap_or_default()
        };
        match args[i].as_str() {
            "--screenshot" => out = next(&mut i).into(),
            "--size" => {
                let s = next(&mut i);
                let (a, b) = s.split_once('x').unwrap_or(("1440", "900"));
                w = a.parse().unwrap_or(1440);
                h = b.parse().unwrap_or(900);
            }
            "--frames" => frames = next(&mut i).parse().unwrap_or(40),
            "--light" => dark = false,
            k @ ("--click" | "--dclick" | "--drag") => {
                let k = k.to_string();
                let s = next(&mut i);
                let (f, rest) = s.split_once(':').unwrap_or(("0", "0,0"));
                let v: Vec<f32> = rest.split(',').filter_map(|t| t.parse().ok()).collect();
                let f: usize = f.parse().unwrap_or(0);
                let a = match (k.as_str(), v.as_slice()) {
                    ("--click", [x, y, ..]) => Action::Click(*x, *y),
                    ("--dclick", [x, y, ..]) => Action::DoubleClick(*x, *y),
                    ("--drag", [a, b, c, d, ..]) => Action::Drag(*a, *b, *c, *d),
                    _ => {
                        eprintln!("bad coordinates for {k}: {rest}");
                        return 2;
                    }
                };
                actions.push((f, a));
            }
            k @ ("--type" | "--key") => {
                let k = k.to_string();
                let s = next(&mut i);
                let (f, rest) = s.split_once(':').unwrap_or(("0", ""));
                let f: usize = f.parse().unwrap_or(0);
                let a = if k == "--type" {
                    Action::Text(rest.to_string())
                } else {
                    match key_action(rest) {
                        Some(a) => a,
                        None => {
                            eprintln!("unknown key '{rest}'");
                            return 2;
                        }
                    }
                };
                actions.push((f, a));
            }
            f => open.push(PathBuf::from(f)),
        }
        i += 1;
    }
    let rgba = render(Options { width: w, height: h, frames, open, dark, actions });
    match save_png(&out, w, h, &rgba) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("arvidleaf {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--compile") {
        std::process::exit(compile(args.get(i + 1).map(String::as_str).unwrap_or(".")));
    }
    if args.iter().any(|a| a == "--screenshot") {
        std::process::exit(screenshot(&args));
    }
    let open: Vec<PathBuf> = args.iter().filter(|a| !a.starts_with('-')).map(PathBuf::from).collect();
    if let Err(e) = lf_gui::run(open) {
        eprintln!("Could not start arvidleaf: {e}");
        std::process::exit(1);
    }
}
