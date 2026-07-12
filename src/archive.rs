//! Archive extraction. Supports `.tar.gz` (Linux/macOS WAMR releases) and
//! `.zip` (upstream `wamr-wasi-extensions-<ver>.zip` on x86_64, Windows
//! releases when we ship them).

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

/// Extract a `.zip` archive with the same top-level-directory stripping and
/// `bin/` routing rules as [`extract_tar_gz`]. Used for upstream x86_64
/// `wamr-wasi-extensions-<ver>.zip` (which has no arch-runner suffix) and
/// planned Windows `.zip` releases.
pub fn extract_zip(archive: &Path) -> Result<Vec<ExtractedFile>> {
    let file = std::fs::File::open(archive)
        .with_context(|| format!("opening archive {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file)).context("reading zip archive")?;

    let mut raw: Vec<(Vec<String>, u32, Vec<u8>)> = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .with_context(|| format!("reading zip entry {i}"))?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name().to_string();
        let comps: Vec<String> = Path::new(&name)
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str().map(String::from),
                _ => None,
            })
            .collect();
        if comps.is_empty() {
            continue;
        }
        // Zip stores unix mode in the upper 16 bits of external_attributes when
        // the entry was created on a unix-family host; fall back to 0o644.
        let mode = entry.unix_mode().unwrap_or(0o644);
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .with_context(|| format!("reading zip entry {name}"))?;
        raw.push((comps, mode, data));
    }

    if raw.is_empty() {
        bail!("archive {} contained no files", archive.display());
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use tempfile::tempdir;

    /// Build an in-memory .tar.gz at `dest` from `(path, mode, bytes)` tuples.
    fn build_tar_gz(dest: &Path, entries: &[(&str, u32, &[u8])]) {
        let tar_bytes: Vec<u8> = {
            let mut builder = tar::Builder::new(Vec::new());
            for (path, mode, bytes) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(*mode);
                header.set_cksum();
                builder.append_data(&mut header, path, *bytes).unwrap();
            }
            builder.into_inner().unwrap()
        };
        let file = std::fs::File::create(dest).unwrap();
        let mut enc = GzEncoder::new(file, Compression::default());
        enc.write_all(&tar_bytes).unwrap();
        enc.finish().unwrap();
    }

    #[test]
    fn extract_strips_single_top_level_dir() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("t.tar.gz");
        build_tar_gz(
            &path,
            &[
                ("iwasm-2.4.5-x86_64-linux/bin/iwasm", 0o755, b"exe-bytes"),
                ("iwasm-2.4.5-x86_64-linux/LICENSE", 0o644, b"license"),
            ],
        );
        let files = extract_tar_gz(&path).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.logical_path.as_str()).collect();
        assert!(paths.contains(&"bin/iwasm"));
        assert!(paths.contains(&"LICENSE"));
        for f in &files {
            if f.logical_path == "bin/iwasm" {
                assert_eq!(f.mode & 0o777, 0o755);
                assert_eq!(f.data, b"exe-bytes");
            }
        }
    }

    #[test]
    fn extract_routes_bare_iwasm_to_bin() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("t.tar.gz");
        // No common top-level directory; `iwasm` sits at the archive root.
        build_tar_gz(
            &path,
            &[("iwasm", 0o755, b"exe-bytes"), ("LICENSE", 0o644, b"lic")],
        );
        let files = extract_tar_gz(&path).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.logical_path.as_str()).collect();
        assert!(paths.contains(&"bin/iwasm"));
        assert!(paths.contains(&"LICENSE"));
    }

    #[test]
    fn extract_leaves_non_executables_alone() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("t.tar.gz");
        // Not a recognized executable name — should stay at the root.
        build_tar_gz(
            &path,
            &[("README.md", 0o644, b"hello"), ("iwasm", 0o755, b"e")],
        );
        let files = extract_tar_gz(&path).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.logical_path.as_str()).collect();
        assert!(paths.contains(&"README.md"));
        assert!(paths.contains(&"bin/iwasm"));
    }

    #[test]
    fn route_preserves_nested_paths() {
        // Nested paths carry a slash and are never rewritten, even for iwasm.
        assert_eq!(route("share/wamr/iwasm"), "share/wamr/iwasm");
        assert_eq!(route("include/wasi/api.h"), "include/wasi/api.h");
    }

    /// Build an in-memory `.zip` at `dest` from `(path, mode, bytes)` tuples.
    fn build_zip(dest: &Path, entries: &[(&str, u32, &[u8])]) {
        use std::io::Write;
        let file = std::fs::File::create(dest).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (path, mode, bytes) in entries {
            zw.start_file(*path, opts.unix_permissions(*mode)).unwrap();
            zw.write_all(bytes).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn extract_zip_strips_single_top_level_dir() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("t.zip");
        build_zip(
            &path,
            &[
                (
                    "wamr-wasi-extensions-2.4.5/include/wasi/api.h",
                    0o644,
                    b"hdr",
                ),
                ("wamr-wasi-extensions-2.4.5/lib/libfoo.a", 0o644, b"static"),
            ],
        );
        let files = extract_zip(&path).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.logical_path.as_str()).collect();
        assert!(paths.contains(&"include/wasi/api.h"));
        assert!(paths.contains(&"lib/libfoo.a"));
    }

    #[test]
    fn extract_zip_routes_bare_iwasm_exe() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("t.zip");
        build_zip(
            &path,
            &[("iwasm.exe", 0o755, b"exe"), ("LICENSE", 0o644, b"lic")],
        );
        let files = extract_zip(&path).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.logical_path.as_str()).collect();
        assert!(paths.contains(&"bin/iwasm.exe"));
        assert!(paths.contains(&"LICENSE"));
    }
}
