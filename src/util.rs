//! Shared helpers.

use std::cmp::Ordering;

/// Compare two dotted version strings numerically.
pub fn version_cmp(a: &str, b: &str) -> Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parse(a).cmp(&parse(b)).then_with(|| a.cmp(b))
}

/// Strip leading `v` **and** WAMR's `WAMR-` tag prefix from a version string.
/// GitHub release tags for WAMR are `WAMR-2.4.5`; internally we track `2.4.5`.
pub fn normalize_version(v: &str) -> String {
    let v = v.strip_prefix("WAMR-").unwrap_or(v);
    v.strip_prefix('v').unwrap_or(v).to_string()
}

/// WAMR has no LTS designation. Kept as a stub so the spec parser can accept
/// `lts` uniformly and error at resolution time rather than parse time.
pub fn is_lts(_version: &str) -> bool {
    false
}

/// Human-readable byte size, e.g. `54.4 MiB`.
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = n as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
