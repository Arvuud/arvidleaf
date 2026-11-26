//! Application settings (`~/.config/arvidleaf/config.toml`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub recent: Vec<PathBuf>,
    pub dark: bool,
    pub font_size: f32,
    /// Default author for new projects.
    pub author: String,
    pub projects_dir: Option<PathBuf>,
    /// Built-in templates the user removed from the gallery.
    pub hidden_templates: Vec<String>,
    /// Show the files & outline side panel.
    pub show_outline: bool,
    /// Vim key bindings in the editor.
    pub vim: bool,
    /// arvidref: folder of the reference library (default `<data_dir>/arvidref`).
    pub library_dir: Option<PathBuf>,
    /// arvidref: pattern for new citation keys (empty = default).
    pub key_pattern: String,
    /// arvidref: contact address sent to Crossref with online lookups (empty = none).
    pub ref_mailto: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            recent: Vec::new(),
            dark: true,
            font_size: 13.5,
            author: std::env::var("USER").unwrap_or_default(),
            projects_dir: None,
            hidden_templates: Vec::new(),
            show_outline: true,
            vim: false,
            library_dir: None,
            key_pattern: String::new(),
            ref_mailto: String::new(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        crate::paths::config_dir().join("config.toml")
    }

    pub fn load() -> Config {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let s = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::write_atomic(&p, s.as_bytes())
    }

    /// Moves a project to the top of the recent list (max 20, existing only).
    pub fn add_recent(&mut self, root: &Path) {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        self.recent.retain(|p| p != &root && p.is_dir());
        self.recent.insert(0, root);
        self.recent.truncate(20);
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.projects_dir.clone().unwrap_or_else(crate::paths::default_projects_dir)
    }

    /// Folder of the arvidref library.
    pub fn library_dir(&self) -> PathBuf {
        self.library_dir.clone().unwrap_or_else(|| crate::paths::data_dir().join("arvidref"))
    }
}
