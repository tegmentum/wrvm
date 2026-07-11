//! Application registrations, persisted as plain JSON in `apps.json`.

use crate::layout::Layout;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct AppRecord {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtimes: Vec<String>,
    pub registered_at: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct AppsFile {
    #[serde(default)]
    apps: Vec<AppRecord>,
}

pub fn read(layout: &Layout) -> Result<Vec<AppRecord>> {
    let path = layout.apps_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };
    Ok(serde_json::from_str::<AppsFile>(&text)
        .map(|f| f.apps)
        .unwrap_or_default())
}

pub fn write(layout: &Layout, apps: &[AppRecord]) -> Result<()> {
    let path = layout.apps_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let file = AppsFile {
        apps: apps.to_vec(),
    };
    let json = serde_json::to_string_pretty(&file)?;
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn register(
    layout: &Layout,
    name: &str,
    path: Option<&str>,
    variant: Option<&str>,
    runtime_path: Option<&str>,
    runtimes: &[String],
    registered_at: i64,
) -> Result<()> {
    let mut apps = read(layout)?;
    apps.retain(|a| a.name != name);
    apps.push(AppRecord {
        name: name.to_string(),
        path: path.map(str::to_string),
        variant: variant.map(str::to_string),
        runtime_path: runtime_path.map(str::to_string),
        runtimes: runtimes.to_vec(),
        registered_at,
    });
    write(layout, &apps)
}

pub fn unregister(layout: &Layout, name: &str) -> Result<bool> {
    let mut apps = read(layout)?;
    let before = apps.len();
    apps.retain(|a| a.name != name);
    let removed = apps.len() != before;
    if removed {
        write(layout, &apps)?;
    }
    Ok(removed)
}

/// Names of applications that depend on a given runtime version.
pub fn apps_using(layout: &Layout, version: &str) -> Result<Vec<String>> {
    let mut names: Vec<String> = read(layout)?
        .into_iter()
        .filter(|a| a.runtimes.iter().any(|v| v == version))
        .map(|a| a.name)
        .collect();
    names.sort();
    Ok(names)
}
