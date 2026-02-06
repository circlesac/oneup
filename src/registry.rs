use anyhow::{Context, Result, bail};

pub struct RegistryClient {
    http: reqwest::blocking::Client,
    registry_url: String,
    auth_token: Option<String>,
}

/// Result of querying the registry for a package
pub enum PackageInfo {
    /// Package exists with all published version strings and dist-tags.latest
    Found {
        versions: Vec<String>,
        latest: String,
    },
    /// Package does not exist in the registry (new package)
    NotFound,
}

impl RegistryClient {
    pub fn new(registry_url: &str, auth_token: Option<String>) -> Self {
        Self {
            http: reqwest::blocking::Client::new(),
            registry_url: registry_url.to_string(),
            auth_token,
        }
    }

    /// GET /<package> → fetch all versions and dist-tags.latest
    pub fn get_package(&self, package_name: &str, verbose: bool) -> Result<PackageInfo> {
        let encoded = encode_package_name(package_name);
        let url = format!("{}/{}", self.registry_url, encoded);

        if verbose {
            eprintln!("[registry] GET {}", url);
        }

        let mut req = self.http.get(&url).header("Accept", "application/json");
        if let Some(token) = &self.auth_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let resp = req
            .send()
            .with_context(|| format!("failed to query registry {}", self.registry_url))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            if verbose {
                eprintln!("[registry] package not found (404)");
            }
            return Ok(PackageInfo::NotFound);
        }

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            bail!(
                "registry authentication failed for {} (HTTP 401)",
                self.registry_url
            );
        }

        if !resp.status().is_success() {
            bail!(
                "failed to query registry {}: HTTP {}",
                self.registry_url,
                resp.status()
            );
        }

        let body: serde_json::Value = resp.json().context("failed to parse registry response")?;

        let latest = body
            .pointer("/dist-tags/latest")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string();

        let versions: Vec<String> = body
            .get("versions")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        if verbose {
            eprintln!("[registry] latest: {}", latest);
            eprintln!("[registry] total versions: {}", versions.len());
        }

        Ok(PackageInfo::Found { versions, latest })
    }
}

/// Encode scoped package names: @scope/name → @scope%2fname
fn encode_package_name(name: &str) -> String {
    if name.starts_with('@') {
        name.replacen('/', "%2f", 1)
    } else {
        name.to_string()
    }
}
