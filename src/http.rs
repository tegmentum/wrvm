//! Native HTTP client (ureq), with progress-aware download.

use crate::progress::Bar;
use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;

pub const USER_AGENT: &str = concat!("wrvm/", env!("CARGO_PKG_VERSION"));

/// GET a URL and return the response body as a string.
pub fn get_string(url: &str) -> Result<String> {
    ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .with_context(|| format!("requesting {url}"))?
        .into_string()
        .map_err(Into::into)
}

/// GET a URL and return the response body as raw bytes.
pub fn get_bytes(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("requesting {url}"))?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf)?;
    Ok(buf)
}

/// GET a URL and stream its body to `dest`, rendering a progress bar.
pub fn download_with_progress(url: &str, dest: &Path, label: &str) -> Result<u64> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let mut file =
        std::fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
    let mut bar = Bar::new(label.to_string(), total);
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; 64 * 1024];
    let mut written: u64 = 0;
    loop {
        let n = reader.read(&mut buf).with_context(|| "reading response")?;
        if n == 0 {
            break;
        }
        use std::io::Write;
        file.write_all(&buf[..n])
            .with_context(|| format!("writing {}", dest.display()))?;
        written += n as u64;
        bar.set(written);
    }
    bar.finish(&format!(
        "Downloaded {} ({})",
        label,
        crate::util::human_bytes(written)
    ));
    Ok(written)
}
