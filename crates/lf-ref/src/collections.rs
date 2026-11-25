//! Collections: named groups of entry keys, nested with `/` (`Thesis/Chapter 2`).
//!
//! Stored in `collections.toml` next to the library:
//! ```toml
//! [collections]
//! "Thesis" = ["knuth1984texbook"]
//! "Thesis/Chapter 2" = ["lamport1994latex", "mueller2021neuronale"]
//! ```
//! A collection may be empty (it then exists only as a folder for its children).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Collections {
    #[serde(default)]
    pub collections: BTreeMap<String, Vec<String>>,
}

/// Normalizes a path: trims parts, drops empty ones (`" a // b "` → `"a/b"`).
pub fn normalize(path: &str) -> String {
    path.split('/').map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("/")
}

fn is_within(path: &str, root: &str) -> bool {
    path == root || path.strip_prefix(root).is_some_and(|r| r.starts_with('/'))
}

impl Collections {
    pub fn parse(src: &str) -> Result<Collections, String> {
        toml::from_str(src).map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    /// Creates a collection and its parents. Returns the normalized path.
    pub fn create(&mut self, path: &str) -> Option<String> {
        let path = normalize(path);
        if path.is_empty() {
            return None;
        }
        let mut cur = String::new();
        for part in path.split('/') {
            if !cur.is_empty() {
                cur.push('/');
            }
            cur.push_str(part);
            self.collections.entry(cur.clone()).or_default();
        }
        Some(path)
    }

    /// Adds a key (creating the collection if needed). Returns false if it was already there.
    pub fn add(&mut self, path: &str, key: &str) -> bool {
        let Some(path) = self.create(path) else { return false };
        let keys = self.collections.entry(path).or_default();
        if keys.iter().any(|k| k.eq_ignore_ascii_case(key)) {
            return false;
        }
        keys.push(key.to_string());
        true
    }

    pub fn remove(&mut self, path: &str, key: &str) -> bool {
        let Some(keys) = self.collections.get_mut(&normalize(path)) else { return false };
        let n = keys.len();
        keys.retain(|k| !k.eq_ignore_ascii_case(key));
        n != keys.len()
    }

    /// Keys directly in a collection (`recursive`: also in its sub-collections), no duplicates.
    pub fn keys(&self, path: &str, recursive: bool) -> Vec<String> {
        let path = normalize(path);
        let mut out: Vec<String> = Vec::new();
        for (p, keys) in &self.collections {
            if *p == path || (recursive && is_within(p, &path)) {
                for k in keys {
                    if !out.iter().any(|o| o.eq_ignore_ascii_case(k)) {
                        out.push(k.clone());
                    }
                }
            }
        }
        out
    }

    /// Collections that contain the key.
    pub fn of(&self, key: &str) -> Vec<String> {
        self.collections.iter().filter(|(_, ks)| ks.iter().any(|k| k.eq_ignore_ascii_case(key))).map(|(p, _)| p.clone()).collect()
    }

    /// Renames or moves a collection with all its children. Fails if the target exists or
    /// lies inside the source.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        let (from, to) = (normalize(from), normalize(to));
        if from.is_empty() || to.is_empty() {
            return Err("empty collection name".into());
        }
        if !self.collections.contains_key(&from) {
            return Err(format!("no collection \"{from}\""));
        }
        if from == to {
            return Ok(());
        }
        if is_within(&to, &from) {
            return Err(format!("cannot move \"{from}\" into itself"));
        }
        if self.collections.contains_key(&to) {
            return Err(format!("collection \"{to}\" already exists"));
        }
        let moved: Vec<String> = self.collections.keys().filter(|p| is_within(p, &from)).cloned().collect();
        for p in moved {
            let keys = self.collections.remove(&p).unwrap_or_default();
            let np = format!("{to}{}", &p[from.len()..]);
            self.collections.insert(np, keys);
        }
        if let Some((parent, _)) = to.rsplit_once('/') {
            self.create(parent);
        }
        Ok(())
    }

    /// Deletes a collection and its children (the entries stay in the library).
    pub fn delete(&mut self, path: &str) {
        let path = normalize(path);
        self.collections.retain(|p, _| !is_within(p, &path));
    }

    /// An entry key changed: update all collections.
    pub fn rename_key(&mut self, from: &str, to: &str) {
        for keys in self.collections.values_mut() {
            for k in keys.iter_mut() {
                if k.eq_ignore_ascii_case(from) {
                    *k = to.to_string();
                }
            }
            let mut seen: Vec<String> = Vec::new();
            keys.retain(|k| {
                let dup = seen.iter().any(|s| s.eq_ignore_ascii_case(k));
                seen.push(k.clone());
                !dup
            });
        }
    }

    /// An entry was deleted: remove it everywhere.
    pub fn forget_key(&mut self, key: &str) {
        for keys in self.collections.values_mut() {
            keys.retain(|k| !k.eq_ignore_ascii_case(key));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_add_and_list() {
        let mut c = Collections::default();
        assert!(c.add(" Thesis // Chapter 2 ", "a"));
        assert!(!c.add("Thesis/Chapter 2", "A"));
        assert!(c.add("Thesis", "b"));
        assert!(c.add("Thesis/Chapter 2", "b"));
        assert_eq!(c.collections.keys().collect::<Vec<_>>(), ["Thesis", "Thesis/Chapter 2"]);
        assert_eq!(c.keys("Thesis", false), ["b"]);
        assert_eq!(c.keys("Thesis", true), ["b", "a"]);
        assert_eq!(c.of("b"), ["Thesis", "Thesis/Chapter 2"]);
        assert!(c.remove("Thesis", "b"));
        assert!(c.create("  /  ").is_none());
    }

    #[test]
    fn rename_and_delete_with_children() {
        let mut c = Collections::default();
        c.add("Thesis/Chapter 2", "a");
        c.add("Thesis/Chapter 2/Notes", "b");
        c.add("Thesis2", "z");
        assert!(c.rename("Thesis", "Thesis/Inner").is_err());
        assert!(c.rename("Thesis", "Thesis2").is_err());
        c.rename("Thesis", "Archive/Old thesis").unwrap();
        assert_eq!(c.collections.keys().collect::<Vec<_>>(), ["Archive", "Archive/Old thesis", "Archive/Old thesis/Chapter 2", "Archive/Old thesis/Chapter 2/Notes", "Thesis2"]);
        c.delete("Archive/Old thesis");
        assert_eq!(c.collections.keys().collect::<Vec<_>>(), ["Archive", "Thesis2"]);
    }

    #[test]
    fn key_changes_and_toml() {
        let mut c = Collections::default();
        c.add("A", "old");
        c.add("A", "new");
        c.add("B", "old");
        c.add("B", "x");
        c.add("B", "new");
        c.rename_key("old", "new");
        assert_eq!(c.keys("A", false), ["new"]);
        assert_eq!(c.keys("B", false), ["new", "x"]);
        c.forget_key("NEW");
        assert!(c.keys("A", true).is_empty());
        assert_eq!(c.keys("B", false), ["x"]);
        c.add("Thesis/Chapter \"2\"", "k");
        let back = Collections::parse(&c.to_toml()).unwrap();
        assert_eq!(back, c);
        assert_eq!(Collections::parse("").unwrap(), Collections::default());
        assert!(Collections::parse("[collections]\nx = 3").is_err());
    }
}
