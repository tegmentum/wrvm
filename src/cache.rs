//! On-disk cache of the remote release list, so floating specs don't hit
//! GitHub on every activation.

use crate::layout::Layout;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Default refresh interval: one hour.
pub const DEFAULT_REFRESH_SECS: i64 = 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseCache {
    pub fetched_at: i64,
    pub all: bool,
    pub versions: Vec<String>,
}

impl ReleaseCache {
    pub fn is_fresh(&self, now: i64, ttl_secs: i64) -> bool {
        ttl_secs > 0 && now.saturating_sub(self.fetched_at) < ttl_secs
    }
}

pub fn read(layout: &Layout, all: bool) -> Option<ReleaseCache> {
    let text = std::fs::read_to_string(layout.release_cache_file(all)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write(layout: &Layout, all: bool, versions: &[String], now: i64) -> Result<()> {
    let path = layout.release_cache_file(all);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let cache = ReleaseCache {
        fetched_at: now,
        all,
        versions: versions.to_vec(),
    };
    std::fs::write(&path, serde_json::to_string(&cache)?)?;
    Ok(())
}

pub fn clear(layout: &Layout) {
    for all in [false, true] {
        let _ = std::fs::remove_file(layout.release_cache_file(all));
    }
}

/// Refresh interval in seconds. `WRVM_REFRESH_INTERVAL` overrides;
/// `0` stays offline.
pub fn refresh_interval() -> i64 {
    std::env::var("WRVM_REFRESH_INTERVAL")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(DEFAULT_REFRESH_SECS)
}
