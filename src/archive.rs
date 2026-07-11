//! Archive extraction. v0.1 supports `.tar.gz` (Linux/macOS WAMR releases).
//! Windows `.zip` support is deferred alongside Windows install support.

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use std::io::{BufReader, Read};
use std::path::Path;

/// A regular file extracted from a release archive.
pub struct ExtractedFile {
    /// Logical path within the variant directory (e.g. `bin/iwasm`).
    pub logical_path: String,
    pub mode: u32,
    pub data: Vec<u8>,
}

/// Extract a `.tar.gz` archive into in-memory files.
///
/// WAMR archives are laid out inconsistently across variants: some contain a
/// single top-level directory holding `iwasm`; others put files at the root; a
/// few flatten headers/libs directly. We heuristically strip the common
/// top-level directory when there is exactly one, and route recognized
/// executables into `bin/`.
pub fn extract_tar_gz(archive: &Path) -> Result<Vec<ExtractedFile>> {
    let file = std::fs::File::open(archive)
        .with_context(|| format!("opening archive {}", archive.display()))?;
    let mut tar = tar::Archive::new(GzDecoder::new(BufReader::new(file)));

    let mut raw: Vec<(Vec<String>, u32, Vec<u8>)> = Vec::new();
    for entry in tar.entries().context("reading tar entries")? {
        let mut entry = entry.context("reading tar entry")?;
        if entry.header().entry_type() != tar::EntryType::Regular {
            continue;
        }
        let path = entry.path().context("reading tar entry path")?.into_owned();
        let comps: Vec<String> = path
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str().map(String::from),
                _ => None,
            })
            .collect();
        if comps.is_empty() {
            continue;
        }
        let mode = entry.header().mode().unwrap_or(0o644);
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .context("reading tar entry data")?;
        raw.push((comps, mode, data));
    }

    if raw.is_empty() {
        bail!("archive {} contained no files", archive.display());
    }

    // Strip a common single top-level directory if one exists.
    let strip = has_single_top(&raw);
    let mut out = Vec::with_capacity(raw.len());
    for (comps, mode, data) in raw {
        let rest: Vec<&str> = if strip && comps.len() > 1 {
            comps[1..].iter().map(String::as_str).collect()
        } else {
            comps.iter().map(String::as_str).collect()
        };
        if rest.is_empty() {
            continue;
        }
        let joined = rest.join("/");
        let logical = route(&joined);
        out.push(ExtractedFile {
            logical_path: logical,
            mode,
            data,
        });
    }
    Ok(out)
}

fn has_single_top(raw: &[(Vec<String>, u32, Vec<u8>)]) -> bool {
    let mut top: Option<&str> = None;
    for (c, _, _) in raw {
        let Some(first) = c.first().map(String::as_str) else {
            return false;
        };
        match top {
            None => top = Some(first),
            Some(t) if t == first => {}
            _ => return false,
        }
    }
    top.is_some()
}

/// Route recognized executables into `bin/` so every variant has a predictable
/// launch path. Leaves everything else where it lands.
fn route(path: &str) -> String {
    let base = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path);
    if !path.contains('/') && matches!(base, "iwasm" | "iwasm.exe" | "wamrc" | "wamrc.exe") {
        return format!("bin/{base}");
    }
    path.to_string()
}
