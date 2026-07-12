//! Transparent usage tracking: `shims/iwasm` appends one JSON line per run.

use crate::layout::Layout;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;

const CAP: usize = 10_000;

#[derive(Debug, Clone)]
pub struct VersionUsage {
    pub version: String,
    pub count: i64,
    pub last_used: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRef {
    pub name: String,
    pub dir: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtimes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEntry {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<AppRef>,
    pub invoked_at: i64,
}

pub fn record(layout: &Layout, entry: &UsageEntry) -> Result<()> {
    let path = layout.usage_log();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(entry)?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("appending to {}", path.display()))?;
    Ok(())
}

pub fn read(layout: &Layout) -> Result<Vec<UsageEntry>> {
    let path = layout.usage_log();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };
    let mut entries: Vec<UsageEntry> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<UsageEntry>(l).ok())
        .collect();

    if entries.len() > CAP {
        entries.drain(..entries.len() - CAP);
        let mut out = String::new();
        for e in &entries {
            if let Ok(line) = serde_json::to_string(e) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        std::fs::write(&path, out).with_context(|| format!("compacting {}", path.display()))?;
    }
    Ok(entries)
}

pub fn by_version(entries: &[UsageEntry]) -> Vec<VersionUsage> {
    use std::collections::HashMap;
    let mut map: HashMap<&str, (i64, i64)> = HashMap::new();
    for e in entries {
        let slot = map.entry(e.version.as_str()).or_insert((0, i64::MIN));
        slot.0 += 1;
        slot.1 = slot.1.max(e.invoked_at);
    }
    let mut out: Vec<VersionUsage> = map
        .into_iter()
        .map(|(version, (count, last_used))| VersionUsage {
            version: version.to_string(),
            count,
            last_used,
        })
        .collect();
    out.sort_by_key(|u| std::cmp::Reverse(u.last_used));
    out
}

pub fn recent(entries: &[UsageEntry], limit: usize) -> Vec<UsageEntry> {
    let mut ordered: Vec<UsageEntry> = entries.to_vec();
    ordered.sort_by_key(|e| std::cmp::Reverse(e.invoked_at));
    ordered.truncate(limit);
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn layout_in(dir: &tempfile::TempDir) -> Layout {
        Layout {
            root: dir.path().to_path_buf(),
        }
    }

    fn entry(version: &str, invoked_at: i64) -> UsageEntry {
        UsageEntry {
            version: version.to_string(),
            variant: None,
            runtime_path: None,
            app: None,
            caller: None,
            cwd: None,
            args: vec![],
            module: None,
            module_path: None,
            module_sha256: None,
            manifest: None,
            invoked_at,
        }
    }

    #[test]
    fn read_missing_returns_empty() {
        let tmp = tempdir().unwrap();
        let layout = layout_in(&tmp);
        assert!(read(&layout).unwrap().is_empty());
    }

    #[test]
    fn record_then_read_round_trips() {
        let tmp = tempdir().unwrap();
        let layout = layout_in(&tmp);
        record(&layout, &entry("2.4.5", 100)).unwrap();
        record(&layout, &entry("2.4.4", 200)).unwrap();
        let got = read(&layout).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].version, "2.4.5");
        assert_eq!(got[1].invoked_at, 200);
    }

    #[test]
    fn by_version_rolls_up() {
        let entries = vec![
            entry("2.4.5", 100),
            entry("2.4.5", 300),
            entry("2.4.4", 200),
        ];
        let rollup = by_version(&entries);
        assert_eq!(rollup.len(), 2);
        // Sorted by last_used descending.
        assert_eq!(rollup[0].version, "2.4.5");
        assert_eq!(rollup[0].count, 2);
        assert_eq!(rollup[0].last_used, 300);
        assert_eq!(rollup[1].version, "2.4.4");
        assert_eq!(rollup[1].count, 1);
    }

    #[test]
    fn recent_returns_newest_first_capped() {
        let entries = vec![
            entry("a", 100),
            entry("b", 300),
            entry("c", 200),
            entry("d", 400),
        ];
        let got = recent(&entries, 2);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].version, "d");
        assert_eq!(got[1].version, "b");
    }

    #[test]
    fn read_compacts_when_over_cap() {
        let tmp = tempdir().unwrap();
        let layout = layout_in(&tmp);
        // Write CAP + 5 entries with monotonic invoked_at.
        for i in 0..(CAP + 5) {
            record(&layout, &entry("2.4.5", i as i64)).unwrap();
        }
        let got = read(&layout).unwrap();
        assert_eq!(got.len(), CAP);
        // The oldest 5 should have been dropped; last entry should be
        // invoked_at = CAP + 4.
        assert_eq!(got.first().unwrap().invoked_at, 5);
        assert_eq!(got.last().unwrap().invoked_at, (CAP + 4) as i64);
        // File should now be exactly CAP lines.
        let text = std::fs::read_to_string(layout.usage_log()).unwrap();
        assert_eq!(text.lines().count(), CAP);
    }
}
