//! How often the user picked or typed each completion (`~/.config/arvidleaf/usage.json`),
//! so suggestions follow what they actually write.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    /// `kind:item` (e.g. `cmd:section{}`, `env:enumerate`) → times picked
    pub counts: HashMap<String, u32>,
    #[serde(skip)]
    dirty: bool,
}

/// Entries kept at most; the least used are dropped beyond this.
const MAX: usize = 2000;
/// When all counts together pass this, every count is halved: recent habits
/// win over old ones (switching from `\leq` to `\leqslant` shows soon).
const AGE_AT: u64 = 20_000;

impl Usage {
    pub fn path() -> PathBuf {
        crate::paths::config_dir().join("usage.json")
    }

    pub fn load() -> Usage {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    /// Writes the file if something was recorded since the last save.
    pub fn save(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let p = Self::path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let s = serde_json::to_string(self).map_err(std::io::Error::other)?;
        crate::write_atomic(&p, s.as_bytes())?;
        self.dirty = false;
        Ok(())
    }

    pub fn record(&mut self, key: &str) {
        *self.counts.entry(key.to_string()).or_default() += 1;
        self.dirty = true;
        if self.counts.values().map(|n| u64::from(*n)).sum::<u64>() > AGE_AT {
            self.counts.values_mut().for_each(|n| *n /= 2);
            self.counts.retain(|_, n| *n > 0);
        }
        if self.counts.len() > MAX {
            let mut v: Vec<(String, u32)> = self.counts.drain().collect();
            v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            v.truncate(MAX * 3 / 4);
            self.counts = v.into_iter().collect();
        }
    }

    pub fn get(&self, key: &str) -> u32 {
        self.counts.get(key).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_prunes() {
        let mut u = Usage::default();
        u.record("cmd:a");
        u.record("cmd:a");
        assert_eq!(u.get("cmd:a"), 2);
        assert_eq!(u.get("cmd:b"), 0);
        for i in 0..MAX {
            u.record(&format!("x:{i}"));
        }
        assert!(u.counts.len() <= MAX);
        assert_eq!(u.get("cmd:a"), 2, "frequent entries survive pruning");
    }

    #[test]
    fn old_habits_fade() {
        let mut u = Usage::default();
        for _ in 0..10_000 {
            u.record("cmd:leq");
        }
        for _ in 0..12_000 {
            u.record("cmd:leqslant");
        }
        assert!(u.get("cmd:leq") <= 5_000, "counts were halved");
        assert!(u.get("cmd:leqslant") > u.get("cmd:leq"));
    }
}
