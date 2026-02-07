use anyhow::{Context, Result, bail};

use crate::registry::PackageInfo;

pub struct CratesIoClient {
    http: reqwest::blocking::Client,
    registry_url: String,
}

impl CratesIoClient {
    pub fn new(registry_url: Option<&str>) -> Self {
        Self {
            http: reqwest::blocking::Client::builder()
                .user_agent("oneup (https://github.com/circlesac/oneup)")
                .build()
                .expect("failed to build HTTP client"),
            registry_url: registry_url
                .unwrap_or("https://crates.io")
                .trim_end_matches('/')
                .to_string(),
        }
    }

    pub fn get_package(&self, crate_name: &str, verbose: bool) -> Result<PackageInfo> {
        let url = format!("{}/api/v1/crates/{}", self.registry_url, crate_name);

        if verbose {
            eprintln!("[registry] GET {}", url);
        }

        let resp = self
            .http
            .get(&url)
            .send()
            .with_context(|| format!("failed to query crates.io for {}", crate_name))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            if verbose {
                eprintln!("[registry] crate not found (404)");
            }
            return Ok(PackageInfo::NotFound);
        }

        if !resp.status().is_success() {
            bail!("failed to query crates.io: HTTP {}", resp.status());
        }

        let body: serde_json::Value = resp.json().context("failed to parse crates.io response")?;

        let latest = body
            .pointer("/crate/max_version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string();

        let versions: Vec<String> = body
            .get("versions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|v| !v.get("yanked").and_then(|y| y.as_bool()).unwrap_or(false))
                    .filter_map(|v| v.get("num").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if verbose {
            eprintln!("[registry] latest: {}", latest);
            eprintln!("[registry] total versions: {}", versions.len());
        }

        Ok(PackageInfo::Found { versions, latest })
    }
}
