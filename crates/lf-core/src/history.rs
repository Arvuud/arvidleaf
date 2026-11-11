//! Local version history: content-addressed snapshots in `.arvidleaf/history/`.
//!
//! ```text
//! history/objects/ab/cdef…   file contents (deflate), named by SHA-256
//! history/snapshots/<id>.json  { id, label, auto, files: { "main.tex": "<hash>" } }
//! ```
//! Identical contents are stored once, so frequent snapshots stay cheap.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Files above this size are not versioned.
pub const MAX_FILE: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Milliseconds since the Unix epoch (unique per project).
    pub id: u64,
    pub label: Option<String>,
    /// Taken automatically (compile, timer) rather than by the user.
    pub auto: bool,
    /// Why it was taken ("compile", "restore", "manual", …).
    #[serde(default)]
    pub reason: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    Added,
    Removed,
    Modified,
}

pub struct History {
    root: PathBuf,
    dir: PathBuf,
    /// path → (mtime, size, hash): avoids rehashing unchanged files
    cache: Mutex<HashMap<PathBuf, (SystemTime, u64, String)>>,
    /// Serialises snapshots/restores (timer, compile thread and UI may race).
    write: Mutex<()>,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn hash_bytes(d: &[u8]) -> String {
    Sha256::digest(d).iter().map(|b| format!("{b:02x}")).collect()
}

impl History {
    pub fn new(root: &Path) -> History {
        History {
            root: root.to_path_buf(),
            dir: root.join(crate::project::META_DIR).join("history"),
            cache: Mutex::new(HashMap::new()),
            write: Mutex::new(()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn object_path(&self, hash: &str) -> PathBuf {
        self.dir.join("objects").join(&hash[..2]).join(&hash[2..])
    }

    fn snap_path(&self, id: u64) -> PathBuf {
        self.dir.join("snapshots").join(format!("{id}.json"))
    }

    /// Hashes the current project files, storing new contents as objects.
    fn capture(&self, store: bool) -> io::Result<BTreeMap<String, String>> {
        let mut files = BTreeMap::new();
        let mut cache = self.cache.lock().unwrap();
        for rel in crate::project::list_files(&self.root) {
            let abs = self.root.join(&rel);
            let Ok(md) = std::fs::metadata(&abs) else { continue };
            if md.len() > MAX_FILE {
                continue;
            }
            let mtime = md.modified().unwrap_or(UNIX_EPOCH);
            let key = rel.to_string_lossy().replace('\\', "/");
            if let Some((t, s, h)) = cache.get(&rel)
                && *t == mtime && *s == md.len() && (!store || self.object_path(h).exists()) {
                    files.insert(key, h.clone());
                    continue;
                }
            let Ok(data) = std::fs::read(&abs) else { continue };
            let h = hash_bytes(&data);
            if store {
                let op = self.object_path(&h);
                if !op.exists() {
                    std::fs::create_dir_all(op.parent().unwrap())?;
                    crate::write_atomic(&op, &miniz_oxide::deflate::compress_to_vec(&data, 6))?;
                }
            }
            cache.insert(rel, (mtime, md.len(), h.clone()));
            files.insert(key, h);
        }
        Ok(files)
    }

    /// Current file → hash map without storing anything.
    pub fn current(&self) -> io::Result<BTreeMap<String, String>> {
        self.capture(false)
    }

    /// Takes a snapshot. Returns `None` if nothing changed since the latest one
    /// (unless a label is given).
    pub fn snapshot(&self, label: Option<String>, auto: bool, reason: &str) -> io::Result<Option<Snapshot>> {
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        self.snapshot_locked(label, auto, reason)
    }

    fn snapshot_locked(&self, label: Option<String>, auto: bool, reason: &str) -> io::Result<Option<Snapshot>> {
        let files = self.capture(true)?;
        let list = self.list();
        if label.is_none() && list.last().is_some_and(|l| l.files == files) {
            return Ok(None);
        }
        let mut id = now_ms();
        if let Some(l) = list.last() {
            id = id.max(l.id + 1);
        }
        let s = Snapshot { id, label: label.filter(|l| !l.trim().is_empty()), auto, reason: reason.to_string(), files };
        self.write_snapshot(&s)?;
        Ok(Some(s))
    }

    fn write_snapshot(&self, s: &Snapshot) -> io::Result<()> {
        std::fs::create_dir_all(self.dir.join("snapshots"))?;
        crate::write_atomic(&self.snap_path(s.id), &serde_json::to_vec_pretty(s).map_err(io::Error::other)?)
    }

    /// All snapshots, oldest first.
    pub fn list(&self) -> Vec<Snapshot> {
        let Ok(rd) = std::fs::read_dir(self.dir.join("snapshots")) else { return Vec::new() };
        let mut v: Vec<Snapshot> =
            rd.filter_map(|e| e.ok()).filter_map(|e| std::fs::read(e.path()).ok()).filter_map(|d| serde_json::from_slice(&d).ok()).collect();
        v.sort_by_key(|s| s.id);
        v
    }

    pub fn read_object(&self, hash: &str) -> io::Result<Vec<u8>> {
        let d = std::fs::read(self.object_path(hash))?;
        miniz_oxide::inflate::decompress_to_vec(&d).map_err(|e| io::Error::other(format!("corrupt object {hash}: {e:?}")))
    }

    /// Contents of a file in a snapshot.
    pub fn file(&self, snap: &Snapshot, rel: &str) -> Option<Vec<u8>> {
        self.read_object(snap.files.get(rel)?).ok()
    }

    pub fn set_label(&self, id: u64, label: Option<String>) -> io::Result<()> {
        let mut s: Snapshot = serde_json::from_slice(&std::fs::read(self.snap_path(id))?).map_err(io::Error::other)?;
        s.label = label.filter(|l| !l.trim().is_empty());
        self.write_snapshot(&s)
    }

    /// Restores one file from a snapshot (a safety snapshot is taken first).
    pub fn restore_file(&self, snap: &Snapshot, rel: &str) -> io::Result<()> {
        crate::project::safe_rel(Path::new(rel))?;
        let data = self.file(snap, rel).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "file not in snapshot"))?;
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        self.snapshot_locked(None, true, "before restore")?;
        let p = self.root.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        crate::write_atomic(&p, &data)
    }

    /// Makes the project match a snapshot exactly: writes its files and deletes
    /// files that did not exist then. Nothing is lost: a safety snapshot of the
    /// current state is taken first.
    pub fn restore_all(&self, snap: &Snapshot) -> io::Result<()> {
        // read everything first so a corrupt object aborts before any change
        let mut contents = Vec::new();
        for (rel, h) in &snap.files {
            crate::project::safe_rel(Path::new(rel))?;
            contents.push((rel.clone(), self.read_object(h)?));
        }
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        self.snapshot_locked(None, true, "before restore")?;
        let current = self.capture(false)?;
        for rel in current.keys() {
            if !snap.files.contains_key(rel) {
                std::fs::remove_file(self.root.join(rel))?;
            }
        }
        for (rel, data) in contents {
            let p = self.root.join(&rel);
            if current.get(&rel) == snap.files.get(&rel) {
                continue;
            }
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d)?;
            }
            crate::write_atomic(&p, &data)?;
        }
        self.cache.lock().unwrap().clear();
        Ok(())
    }

    /// Thins out automatic snapshots: all from the last day, one per hour up to
    /// a week, one per day after that. Labelled snapshots are always kept.
    /// Returns the number of removed snapshots.
    pub fn prune(&self, now: u64) -> io::Result<usize> {
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        const HOUR: u64 = 3_600_000;
        const DAY: u64 = 24 * HOUR;
        let list = self.list();
        let mut kept_buckets = HashSet::new();
        let mut removed = 0;
        // newest first, so each bucket keeps its latest snapshot
        for (i, s) in list.iter().enumerate().rev() {
            let age = now.saturating_sub(s.id);
            let is_latest = i + 1 == list.len();
            if s.label.is_some() || !s.auto || age < DAY || is_latest {
                continue;
            }
            let bucket = if age < 7 * DAY { (1u8, s.id / HOUR) } else { (2u8, s.id / DAY) };
            if !kept_buckets.insert(bucket) {
                std::fs::remove_file(self.snap_path(s.id))?;
                removed += 1;
            }
        }
        if removed > 0 {
            self.gc_locked()?;
        }
        Ok(removed)
    }

    /// Deletes objects no snapshot refers to.
    pub fn gc(&self) -> io::Result<usize> {
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        self.gc_locked()
    }

    fn gc_locked(&self) -> io::Result<usize> {
        let used: HashSet<String> = self.list().into_iter().flat_map(|s| s.files.into_values()).collect();
        let mut n = 0;
        let Ok(rd) = std::fs::read_dir(self.dir.join("objects")) else { return Ok(0) };
        for d in rd.filter_map(|e| e.ok()) {
            let prefix = d.file_name().to_string_lossy().to_string();
            for f in std::fs::read_dir(d.path())?.filter_map(|e| e.ok()) {
                let h = format!("{prefix}{}", f.file_name().to_string_lossy());
                if !used.contains(&h) {
                    std::fs::remove_file(f.path())?;
                    n += 1;
                }
            }
        }
        Ok(n)
    }
}

/// Changed files between two file maps.
pub fn changes(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>) -> Vec<(String, Change)> {
    let mut v = Vec::new();
    for (k, h) in new {
        match old.get(k) {
            None => v.push((k.clone(), Change::Added)),
            Some(o) if o != h => v.push((k.clone(), Change::Modified)),
            _ => {}
        }
    }
    for k in old.keys() {
        if !new.contains_key(k) {
            v.push((k.clone(), Change::Removed));
        }
    }
    v.sort();
    v
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Same,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
    pub text: String,
}

/// Line diff (Myers) with line numbers.
pub fn diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    let d = similar::TextDiff::from_lines(old, new);
    d.iter_all_changes()
        .map(|c| DiffLine {
            kind: match c.tag() {
                similar::ChangeTag::Equal => LineKind::Same,
                similar::ChangeTag::Insert => LineKind::Added,
                similar::ChangeTag::Delete => LineKind::Removed,
            },
            old_no: c.old_index().map(|i| i + 1),
            new_no: c.new_index().map(|i| i + 1),
            text: c.value().trim_end_matches('\n').to_string(),
        })
        .collect()
}

/// Whether bytes look like text (UTF-8 without NUL).
pub fn is_text(d: &[u8]) -> bool {
    !d.contains(&0) && std::str::from_utf8(d).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_diff_restore_roundtrip() {
        let d = crate::testutil::tempdir("hist");
        std::fs::write(d.join("main.tex"), "a\nb\nc\n").unwrap();
        std::fs::create_dir_all(d.join("img")).unwrap();
        std::fs::write(d.join("img/x.png"), [0u8, 1, 2, 3]).unwrap();
        std::fs::write(d.join("main.aux"), "aux").unwrap();
        let h = History::new(&d);
        let s1 = h.snapshot(None, true, "compile").unwrap().unwrap();
        assert_eq!(s1.files.len(), 2, "aux files are not versioned");
        assert!(h.snapshot(None, true, "compile").unwrap().is_none(), "unchanged → no snapshot");

        std::thread::sleep(std::time::Duration::from_millis(5));
        std::fs::write(d.join("main.tex"), "a\nB\nc\nd\n").unwrap();
        std::fs::write(d.join("new.tex"), "new").unwrap();
        std::fs::remove_file(d.join("img/x.png")).unwrap();
        let s2 = h.snapshot(Some("v2".into()), false, "manual").unwrap().unwrap();
        let ch = changes(&s1.files, &s2.files);
        assert_eq!(
            ch,
            vec![("img/x.png".into(), Change::Removed), ("main.tex".into(), Change::Modified), ("new.tex".into(), Change::Added)]
        );
        let old = String::from_utf8(h.file(&s1, "main.tex").unwrap()).unwrap();
        let new = std::fs::read_to_string(d.join("main.tex")).unwrap();
        let diff = diff_lines(&old, &new);
        assert_eq!(diff.iter().filter(|l| l.kind == LineKind::Added).count(), 2);
        assert_eq!(diff.iter().filter(|l| l.kind == LineKind::Removed).count(), 1);

        // restore everything: exactly the state of s1
        h.restore_all(&s1).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("main.tex")).unwrap(), "a\nb\nc\n");
        assert_eq!(std::fs::read(d.join("img/x.png")).unwrap(), vec![0, 1, 2, 3]);
        assert!(!d.join("new.tex").exists());
        assert_eq!(h.current().unwrap(), s1.files);
        // the state before restoring was already saved as s2: no duplicate
        let all = h.list();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].files, s2.files);
        // unsaved changes get a safety snapshot before a restore
        std::fs::write(d.join("main.tex"), "edited\n").unwrap();
        h.restore_file(&s2, "main.tex").unwrap();
        let all = h.list();
        assert_eq!(all.len(), 3);
        assert_eq!(all[2].reason, "before restore");
        assert_eq!(String::from_utf8(h.file(&all[2], "main.tex").unwrap()).unwrap(), "edited\n");
        assert_eq!(std::fs::read_to_string(d.join("main.tex")).unwrap(), "a\nB\nc\nd\n");
    }

    #[test]
    fn concurrent_snapshots_do_not_corrupt() {
        let d = crate::testutil::tempdir("race");
        for i in 0..20 {
            std::fs::write(d.join(format!("f{i}.tex")), format!("content {i}").repeat(500)).unwrap();
        }
        let h = std::sync::Arc::new(History::new(&d));
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let h = h.clone();
                std::thread::spawn(move || h.snapshot(Some(format!("t{t}")), true, "race").unwrap())
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let all = h.list();
        assert_eq!(all.len(), 8, "every labelled snapshot has its own id");
        for s in &all {
            for hash in s.files.values() {
                h.read_object(hash).expect("object readable");
            }
        }
    }

    #[test]
    fn dedup_and_prune() {
        let d = crate::testutil::tempdir("prune");
        std::fs::write(d.join("main.tex"), "x").unwrap();
        let h = History::new(&d);
        // fabricate old automatic snapshots: 10 within the same hour two days ago
        let now = now_ms();
        let base = now - 2 * 24 * 3_600_000;
        let files = h.capture(true).unwrap();
        for i in 0..10 {
            h.write_snapshot(&Snapshot { id: base + i * 1000, label: None, auto: true, reason: "t".into(), files: files.clone() }).unwrap();
        }
        h.write_snapshot(&Snapshot { id: base + 20_000, label: Some("keep".into()), auto: true, reason: "t".into(), files: files.clone() }).unwrap();
        let objects = std::fs::read_dir(d.join(".arvidleaf/history/objects")).unwrap().count();
        assert_eq!(objects, 1, "same content stored once");
        let removed = h.prune(now).unwrap();
        assert_eq!(removed, 9);
        let left = h.list();
        assert_eq!(left.len(), 2);
        assert!(left.iter().any(|s| s.label.as_deref() == Some("keep")));
    }
}
