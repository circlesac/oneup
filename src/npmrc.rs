use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Parsed .npmrc configuration
pub struct NpmrcConfig {
    entries: HashMap<String, String>,
}

impl NpmrcConfig {
    /// Load .npmrc files following npm's resolution order:
    /// 1. Project-level .npmrc (directory of target file)
    /// 2. User-level ~/.npmrc
    /// Environment variables (NPM_CONFIG_*) override file values.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let mut entries = HashMap::new();

        // User-level first (lower priority)
        if let Some(home) = dirs_path() {
            let user_npmrc = home.join(".npmrc");
            if user_npmrc.exists() {
                parse_npmrc_file(&user_npmrc, &mut entries)?;
            }
        }

        // Project-level overrides user-level
        let project_npmrc = project_dir.join(".npmrc");
        if project_npmrc.exists() {
            parse_npmrc_file(&project_npmrc, &mut entries)?;
        }

        // Environment variables override everything
        for (key, value) in std::env::vars() {
            if let Some(npm_key) = key.strip_prefix("NPM_CONFIG_") {
                let normalized = npm_key.to_lowercase().replace('_', "-");
                entries.insert(normalized, value);
            } else if let Some(npm_key) = key.strip_prefix("npm_config_") {
                let normalized = npm_key.to_lowercase().replace('_', "-");
                entries.insert(normalized, value);
            }
        }

        Ok(Self { entries })
    }

    /// Get the registry URL for a given scope (e.g., "@myorg") or the default registry.
    pub fn registry_url(&self, scope: Option<&str>) -> String {
        // Check scoped registry first
        if let Some(scope) = scope {
            let key = format!("{scope}:registry");
            if let Some(url) = self.entries.get(&key) {
                return normalize_registry_url(url);
            }
        }

        // Fall back to default registry
        if let Some(url) = self.entries.get("registry") {
            return normalize_registry_url(url);
        }

        "https://registry.npmjs.org".to_string()
    }

    /// Get auth token for a registry URL.
    pub fn auth_token(&self, registry_url: &str) -> Option<String> {
        let host = registry_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');

        // Check //<host>/:_authToken
        let key = format!("//{host}/:_authToken");
        if let Some(token) = self.entries.get(&key) {
            return Some(resolve_env_var(token));
        }

        // Check _authToken (global)
        if let Some(token) = self.entries.get("_authToken") {
            return Some(resolve_env_var(token));
        }

        None
    }
}

fn dirs_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn parse_npmrc_file(path: &Path, entries: &mut HashMap<String, String>) -> Result<()> {
    let content = std::fs::read_to_string(path)?;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            entries.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    Ok(())
}

fn normalize_registry_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

/// Resolve ${ENV_VAR} references in token values.
fn resolve_env_var(value: &str) -> String {
    if let Some(var_name) = value.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        std::env::var(var_name).unwrap_or_default()
    } else {
        value.to_string()
    }
}
