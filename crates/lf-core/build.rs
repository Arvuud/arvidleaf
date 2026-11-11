//! Embeds every folder under `templates/` as a built-in template.

use std::fmt::Write;
use std::path::{Path, PathBuf};

fn files(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect()).unwrap_or_default();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            files(&p, base, out);
        } else {
            out.push(p.strip_prefix(base).unwrap().to_path_buf());
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates").canonicalize().expect("templates/ folder");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut dirs: Vec<_> = std::fs::read_dir(&root).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("template.toml").is_file()).collect();
    dirs.sort();
    let mut code = String::from("pub static BUILTIN: &[(&str, &[(&str, &[u8])])] = &[\n");
    for d in dirs {
        let id = d.file_name().unwrap().to_string_lossy().to_string();
        let mut fs = Vec::new();
        files(&d, &d, &mut fs);
        writeln!(code, "    ({id:?}, &[").unwrap();
        for f in fs {
            let rel = f.to_string_lossy().replace('\\', "/");
            writeln!(code, "        ({rel:?}, include_bytes!({:?})),", d.join(&f).display().to_string()).unwrap();
        }
        code.push_str("    ]),\n");
    }
    code.push_str("];\n");
    std::fs::write(Path::new(&std::env::var("OUT_DIR").unwrap()).join("builtin_templates.rs"), code).unwrap();
}
