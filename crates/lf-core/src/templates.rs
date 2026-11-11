//! Templates: built-in ones (compiled in from `templates/`) and user templates
//! in `~/.local/share/arvidleaf/templates/<id>/`.
//!
//! A template is a folder with a `template.toml` and any number of files:
//!
//! ```toml
//! name = "Article (English)"
//! category = "Article"
//! description = "Single-column article with title, abstract and bibliography."
//! main = "main.tex"
//! engine = "pdflatex"   # pdflatex | xelatex | lualatex
//! order = 10            # sort position inside the category
//! ```
//!
//! In text files the placeholders `{{title}}`, `{{author}}`, `{{date}}` and
//! `{{project}}` are replaced when a project is created.

use std::io;
use std::path::{Path, PathBuf};

use lf_tex::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::project::{Project, ProjectMeta};

mod builtin {
    include!(concat!(env!("OUT_DIR"), "/builtin_templates.rs"));
}

pub const META_FILE: &str = "template.toml";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemplateMeta {
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_main")]
    pub main: String,
    #[serde(default)]
    pub engine: Engine,
    #[serde(default)]
    pub order: i32,
}

fn default_main() -> String {
    "main.tex".into()
}

impl Default for TemplateMeta {
    fn default() -> Self {
        TemplateMeta { name: String::new(), category: "Other".into(), description: String::new(), main: default_main(), engine: Engine::Pdflatex, order: 0 }
    }
}

#[derive(Clone, Debug)]
pub enum Source {
    Builtin(&'static [(&'static str, &'static [u8])]),
    User(PathBuf),
}

#[derive(Clone, Debug)]
pub struct Template {
    pub id: String,
    pub meta: TemplateMeta,
    pub source: Source,
}

#[derive(Clone, Debug, Default)]
pub struct Vars {
    pub title: String,
    pub author: String,
    pub project: String,
}

pub fn user_dir() -> PathBuf {
    crate::paths::data_dir().join("templates")
}

impl Template {
    pub fn is_builtin(&self) -> bool {
        matches!(self.source, Source::Builtin(_))
    }

    pub fn folder(&self) -> Option<&Path> {
        match &self.source {
            Source::User(p) => Some(p),
            Source::Builtin(_) => None,
        }
    }

    /// All files except `template.toml` (relative path, content).
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        match &self.source {
            Source::Builtin(fs) => fs.iter().filter(|(p, _)| *p != META_FILE).map(|(p, d)| (p.to_string(), d.to_vec())).collect(),
            Source::User(dir) => {
                let mut out = Vec::new();
                collect(dir, dir, &mut out);
                out.retain(|(p, _)| p != META_FILE && p != "preview.png");
                out.sort();
                out
            }
        }
    }

    /// Hash of metadata and files (preview cache key).
    pub fn content_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.meta.main.as_bytes());
        h.update(self.meta.engine.label().as_bytes());
        for (p, d) in self.files() {
            h.update(p.as_bytes());
            h.update((d.len() as u64).to_le_bytes());
            h.update(&d);
        }
        h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
    }
}

fn collect(base: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            collect(base, &p, out);
        } else if let Ok(d) = std::fs::read(&p) {
            out.push((p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"), d));
        }
    }
}

fn parse_meta(s: &str, fallback_name: &str) -> TemplateMeta {
    let mut m: TemplateMeta = toml::from_str(s).unwrap_or_default();
    if m.name.trim().is_empty() {
        m.name = fallback_name.to_string();
    }
    if m.category.trim().is_empty() {
        m.category = "Other".into();
    }
    m
}

pub fn builtins() -> Vec<Template> {
    builtin::BUILTIN
        .iter()
        .map(|(id, files)| {
            let meta = files.iter().find(|(p, _)| *p == META_FILE).map(|(_, d)| parse_meta(&String::from_utf8_lossy(d), id)).unwrap_or_default();
            Template { id: format!("builtin:{id}"), meta, source: Source::Builtin(files) }
        })
        .collect()
}

pub fn user_templates() -> Vec<Template> {
    let dir = user_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    rd.filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .map(|p| {
            let id = p.file_name().unwrap().to_string_lossy().to_string();
            let meta = parse_meta(&std::fs::read_to_string(p.join(META_FILE)).unwrap_or_default(), &id);
            Template { id: format!("user:{id}"), meta, source: Source::User(p) }
        })
        .collect()
}

/// Templates shown in the gallery: visible built-ins and all user templates,
/// sorted by category, order, name.
pub fn list(hidden: &[String]) -> Vec<Template> {
    let mut v: Vec<Template> = builtins().into_iter().filter(|t| !hidden.contains(&t.id)).chain(user_templates()).collect();
    sort(&mut v);
    v
}

pub fn sort(v: &mut [Template]) {
    v.sort_by(|a, b| {
        a.meta.category.to_lowercase().cmp(&b.meta.category.to_lowercase()).then(a.meta.order.cmp(&b.meta.order)).then(a.meta.name.cmp(&b.meta.name))
    });
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn substitute(text: &str, vars: &Vars) -> String {
    text.replace("{{title}}", &vars.title).replace("{{author}}", &vars.author).replace("{{date}}", &today()).replace("{{project}}", &vars.project)
}

/// Creates a new project folder `dest` from a template.
pub fn instantiate(t: &Template, dest: &Path, vars: &Vars) -> io::Result<Project> {
    if dest.exists() && std::fs::read_dir(dest)?.next().is_some() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} already exists and is not empty", dest.display())));
    }
    std::fs::create_dir_all(dest)?;
    for (rel, data) in t.files() {
        crate::project::safe_rel(Path::new(&rel))?;
        let p = dest.join(&rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let is_text = crate::project::is_text_file(Path::new(&rel)) && std::str::from_utf8(&data).is_ok();
        if is_text {
            std::fs::write(&p, substitute(std::str::from_utf8(&data).unwrap(), vars))?;
        } else {
            std::fs::write(&p, &data)?;
        }
    }
    let mut project = Project::open(dest)?;
    project.meta = ProjectMeta {
        main: if dest.join(&t.meta.main).is_file() { Some(t.meta.main.clone()) } else { project.meta.main.clone() },
        engine: t.meta.engine,
        ..ProjectMeta::default()
    };
    project.meta.open_files = project.meta.main.iter().cloned().collect();
    project.save_meta()?;
    Ok(project)
}

/// File-system friendly id from a name.
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        let c = match c {
            'ä' | 'Ä' => "ae".to_string(),
            'ö' | 'Ö' => "oe".to_string(),
            'ü' | 'Ü' => "ue".to_string(),
            'ß' => "ss".to_string(),
            c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase().to_string(),
            _ => "-".to_string(),
        };
        s.push_str(&c);
    }
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "template".into() } else { s }
}

fn unique_dir(name: &str) -> PathBuf {
    let base = user_dir();
    let s = slug(name);
    let mut p = base.join(&s);
    let mut i = 1;
    while p.exists() {
        i += 1;
        p = base.join(format!("{s}-{i}"));
    }
    p
}

pub fn write_meta(dir: &Path, meta: &TemplateMeta) -> io::Result<()> {
    let s = toml::to_string_pretty(meta).map_err(io::Error::other)?;
    crate::write_atomic(&dir.join(META_FILE), s.as_bytes())
}

/// Creates a user template from a folder (e.g. a project). Hidden files and
/// TeX by-products are skipped.
pub fn create_from_folder(src: &Path, meta: &TemplateMeta) -> io::Result<Template> {
    let dir = unique_dir(&meta.name);
    std::fs::create_dir_all(&dir)?;
    for rel in crate::project::list_files(src) {
        let to = dir.join(&rel);
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::copy(src.join(&rel), to)?;
    }
    let _ = std::fs::remove_file(dir.join(META_FILE));
    write_meta(&dir, meta)?;
    let id = dir.file_name().unwrap().to_string_lossy().to_string();
    Ok(Template { id: format!("user:{id}"), meta: meta.clone(), source: Source::User(dir) })
}

/// Copies a built-in template into the user folder so it can be edited.
pub fn copy_to_user(t: &Template) -> io::Result<Template> {
    let mut meta = t.meta.clone();
    if t.is_builtin() {
        meta.name = format!("{} (copy)", meta.name);
    }
    let dir = unique_dir(&meta.name);
    std::fs::create_dir_all(&dir)?;
    for (rel, data) in t.files() {
        crate::project::safe_rel(Path::new(&rel))?;
        let p = dir.join(&rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, data)?;
    }
    write_meta(&dir, &meta)?;
    let id = dir.file_name().unwrap().to_string_lossy().to_string();
    Ok(Template { id: format!("user:{id}"), meta, source: Source::User(dir) })
}

/// Deletes a user template (moved to the trash).
pub fn delete(t: &Template) -> io::Result<()> {
    match &t.source {
        Source::User(dir) => crate::project::trash(dir),
        Source::Builtin(_) => Err(io::Error::other("built-in templates can only be hidden")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_have_metadata_and_main() {
        let b = builtins();
        assert!(b.len() >= 5, "{}", b.len());
        for t in &b {
            assert!(!t.meta.name.is_empty());
            assert!(t.files().iter().any(|(p, _)| *p == t.meta.main), "{} lacks {}", t.id, t.meta.main);
        }
    }

    #[test]
    fn instantiate_and_roundtrip_user_template() {
        let home = crate::testutil::tempdir("tplhome");
        // SAFETY: tests touching ARVIDLEAF_HOME run in this one test only
        unsafe { std::env::set_var("ARVIDLEAF_HOME", &home) };
        let t = builtins().into_iter().find(|t| t.id == "builtin:article-en").unwrap();
        let dest = home.join("proj");
        let p = instantiate(&t, &dest, &Vars { title: "My Title".into(), author: "Ada".into(), project: "proj".into() }).unwrap();
        let main = std::fs::read_to_string(dest.join("main.tex")).unwrap();
        assert!(main.contains("My Title") && main.contains("Ada") && !main.contains("{{"));
        assert_eq!(p.meta.main.as_deref(), Some("main.tex"));
        let meta = TemplateMeta { name: "Mein Bericht".into(), category: "Eigene".into(), ..Default::default() };
        let u = create_from_folder(&dest, &meta).unwrap();
        assert_eq!(u.id, "user:mein-bericht");
        let found = user_templates();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].meta.name, "Mein Bericht");
        assert!(found[0].files().iter().any(|(p, _)| p == "main.tex"));
        assert!(!found[0].files().iter().any(|(p, _)| p.starts_with(".arvidleaf")));
        assert_eq!(slug("Übung Nr. 1"), "uebung-nr-1");
    }
}
