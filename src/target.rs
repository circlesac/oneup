use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::Path;

pub struct TargetFile {
    pub package_name: String,
    pub version: String,
    pub raw: Value,
}

impl TargetFile {
    pub fn read(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("target file not found: {}", path.display()))?;

        let raw: Value = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}: invalid JSON", path.display()))?;

        let obj = raw
            .as_object()
            .with_context(|| format!("failed to parse {}: expected JSON object", path.display()))?;

        // Auto-detect format: "package" key (MCP server) or "name" key (package.json)
        let package_name = if let Some(pkg) = obj.get("package").and_then(|v| v.as_str()) {
            pkg.to_string()
        } else if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
            name.to_string()
        } else {
            bail!(
                "cannot determine package name from {}: missing 'package' or 'name' field",
                path.display()
            );
        };

        let version = obj
            .get("version")
            .and_then(|v| v.as_str())
            .with_context(|| {
                format!(
                    "failed to parse {}: missing 'version' field",
                    path.display()
                )
            })?
            .to_string();

        Ok(Self {
            package_name,
            version,
            raw,
        })
    }

    pub fn write(&self, path: &Path, new_version: &str) -> Result<()> {
        let mut raw = self.raw.clone();
        raw.as_object_mut().unwrap().insert(
            "version".to_string(),
            Value::String(new_version.to_string()),
        );

        // Preserve 2-space indent + trailing newline
        let mut output = serde_json::to_string_pretty(&raw)?;
        output.push('\n');

        std::fs::write(path, &output)
            .with_context(|| format!("failed to write {}", path.display()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn read_package_json_format() {
        let f = temp_file(r#"{"name": "my-pkg", "version": "1.0.0"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.package_name, "my-pkg");
        assert_eq!(target.version, "1.0.0");
    }

    #[test]
    fn read_mcp_server_format() {
        let f = temp_file(r#"{"package": "@scope/mcp-server", "version": "2.3.4"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.package_name, "@scope/mcp-server");
        assert_eq!(target.version, "2.3.4");
    }

    #[test]
    fn read_package_key_takes_precedence() {
        let f = temp_file(r#"{"package": "pkg-name", "name": "other-name", "version": "1.0.0"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.package_name, "pkg-name");
    }

    #[test]
    fn read_missing_name_and_package() {
        let f = temp_file(r#"{"version": "1.0.0"}"#);
        assert!(TargetFile::read(f.path()).is_err());
    }

    #[test]
    fn read_missing_version() {
        let f = temp_file(r#"{"name": "my-pkg"}"#);
        assert!(TargetFile::read(f.path()).is_err());
    }

    #[test]
    fn read_invalid_json() {
        let f = temp_file("not json");
        assert!(TargetFile::read(f.path()).is_err());
    }

    #[test]
    fn read_file_not_found() {
        assert!(TargetFile::read(Path::new("/nonexistent/file.json")).is_err());
    }

    #[test]
    fn write_updates_version() {
        let f = temp_file(r#"{"name": "my-pkg", "version": "1.0.0"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        target.write(f.path(), "2.0.0").unwrap();

        let updated = TargetFile::read(f.path()).unwrap();
        assert_eq!(updated.version, "2.0.0");
        assert_eq!(updated.package_name, "my-pkg");
    }

    #[test]
    fn write_preserves_trailing_newline() {
        let f = temp_file(r#"{"name": "my-pkg", "version": "1.0.0"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        target.write(f.path(), "2.0.0").unwrap();

        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.ends_with('\n'));
    }
}
