//! Version specifiers.
//!
//! | Spec              | Meaning              |
//! |-------------------|----------------------|
//! | `latest`          | newest overall       |
//! | `lts`             | (WAMR has no LTS — always resolves empty; error at use) |
//! | `2` / `2.x`       | latest major line    |
//! | `2.4` / `2.4.x`   | latest major.minor   |
//! | `2.4.5`           | exact                |

use crate::util::{is_lts, normalize_version, version_cmp};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSpec {
    Latest,
    Lts,
    Major(u64),
    MajorMinor(u64, u64),
    Exact(String),
}

impl VersionSpec {
    pub fn parse(input: &str) -> Result<VersionSpec, String> {
        let raw = input.trim();
        if raw.is_empty() {
            return Err("empty version spec".to_string());
        }
        match raw.to_ascii_lowercase().as_str() {
            "latest" | "*" | "x" => return Ok(VersionSpec::Latest),
            "lts" => return Ok(VersionSpec::Lts),
            _ => {}
        }

        let mut nums: Vec<u64> = Vec::new();
        for part in normalize_version(raw).split('.') {
            let p = part.trim();
            if p.is_empty() || p.eq_ignore_ascii_case("x") || p == "*" {
                break;
            }
            match p.parse::<u64>() {
                Ok(n) => nums.push(n),
                Err(_) => return Err(format!("invalid version spec '{input}'")),
            }
        }

        match nums.as_slice() {
            [m] => Ok(VersionSpec::Major(*m)),
            [m, mi] => Ok(VersionSpec::MajorMinor(*m, *mi)),
            [m, mi, pa, ..] => Ok(VersionSpec::Exact(format!("{m}.{mi}.{pa}"))),
            _ => Err(format!("invalid version spec '{input}'")),
        }
    }

    pub fn is_floating(&self) -> bool {
        !matches!(self, VersionSpec::Exact(_))
    }

    pub fn matches(&self, version: &str) -> bool {
        let comps = numeric_parts(version);
        match self {
            VersionSpec::Latest => true,
            VersionSpec::Lts => is_lts(version),
            VersionSpec::Major(m) => comps.first() == Some(m),
            VersionSpec::MajorMinor(m, mi) => comps.first() == Some(m) && comps.get(1) == Some(mi),
            VersionSpec::Exact(e) => normalize_version(version) == normalize_version(e),
        }
    }

    pub fn resolve<'a, S: AsRef<str>>(&self, candidates: &'a [S]) -> Option<&'a str> {
        candidates
            .iter()
            .map(AsRef::as_ref)
            .filter(|c| self.matches(c))
            .max_by(|a, b| version_cmp(a, b))
    }
}

impl FromStr for VersionSpec {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        VersionSpec::parse(s)
    }
}

impl fmt::Display for VersionSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionSpec::Latest => f.write_str("latest"),
            VersionSpec::Lts => f.write_str("lts"),
            VersionSpec::Major(m) => write!(f, "{m}"),
            VersionSpec::MajorMinor(m, mi) => write!(f, "{m}.{mi}"),
            VersionSpec::Exact(s) => f.write_str(s),
        }
    }
}

fn numeric_parts(v: &str) -> Vec<u64> {
    normalize_version(v)
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_channels() {
        assert_eq!(VersionSpec::parse("latest").unwrap(), VersionSpec::Latest);
        assert_eq!(VersionSpec::parse("LATEST").unwrap(), VersionSpec::Latest);
        assert_eq!(VersionSpec::parse("lts").unwrap(), VersionSpec::Lts);
    }

    #[test]
    fn parses_major_forms() {
        for s in ["2", "2.x", "2.*", "v2", "2.X", "WAMR-2"] {
            assert_eq!(
                VersionSpec::parse(s).unwrap(),
                VersionSpec::Major(2),
                "for {s}"
            );
        }
    }

    #[test]
    fn parses_exact() {
        assert_eq!(
            VersionSpec::parse("2.4.5").unwrap(),
            VersionSpec::Exact("2.4.5".to_string())
        );
        assert_eq!(
            VersionSpec::parse("WAMR-2.4.5").unwrap(),
            VersionSpec::Exact("2.4.5".to_string())
        );
    }

    #[test]
    fn lts_resolves_empty() {
        // WAMR has no LTS designation.
        let all = v(&["2.3.0", "2.4.5"]);
        assert_eq!(VersionSpec::Lts.resolve(&all), None);
    }

    #[test]
    fn resolves_lines() {
        let all = v(&["2.3.0", "2.4.0", "2.4.5", "3.0.0"]);
        assert_eq!(VersionSpec::Latest.resolve(&all), Some("3.0.0"));
        assert_eq!(VersionSpec::Major(2).resolve(&all), Some("2.4.5"));
        assert_eq!(VersionSpec::MajorMinor(2, 4).resolve(&all), Some("2.4.5"));
    }
}
