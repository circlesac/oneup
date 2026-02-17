use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::Path;

enum TargetFormat {
    Json(Value),
    Toml(toml_edit::DocumentMut),
}

pub struct TargetFile {
    pub package_name: String,
    pub version: String,
    format: TargetFormat,
}

impl TargetFile {
    pub fn read(path: &Path) -> Result<Self> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => Self::read_toml(path),
            _ => Self::read_json(path),
        }
    }

    fn read_json(path: &Path) -> Result<Self> {
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
            .unwrap_or("0.0.0")
            .to_string();

        Ok(Self {
            package_name,
            version,
            format: TargetFormat::Json(raw),
        })
    }

    fn read_toml(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("target file not found: {}", path.display()))?;

        let doc: toml_edit::DocumentMut = content
            .parse()
            .with_context(|| format!("failed to parse {}: invalid TOML", path.display()))?;

        let package_name = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .with_context(|| format!("missing package.name in {}", path.display()))?
            .to_string();

        let version = doc
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .with_context(|| format!("missing package.version in {}", path.display()))?
            .to_string();

        Ok(Self {
            package_name,
            version,
            format: TargetFormat::Toml(doc),
        })
    }

    pub fn write(&self, path: &Path, new_version: &str) -> Result<()> {
        match &self.format {
            TargetFormat::Json(raw) => {
                let mut raw = raw.clone();
                raw.as_object_mut().unwrap().insert(
                    "version".to_string(),
                    Value::String(new_version.to_string()),
                );

                // Preserve 2-space indent + trailing newline
                let mut output = serde_json::to_string_pretty(&raw)?;
                output.push('\n');

                std::fs::write(path, &output)
                    .with_context(|| format!("failed to write {}", path.display()))?;
            }
            TargetFormat::Toml(doc) => {
                let mut doc = doc.clone();
                doc["package"]["version"] = toml_edit::value(new_version);

                std::fs::write(path, doc.to_string())
                    .with_context(|| format!("failed to write {}", path.display()))?;
            }
        }
        Ok(())
    }

    pub fn is_cargo(&self) -> bool {
        matches!(self.format, TargetFormat::Toml(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_json(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".json")
            .tempfile()
            .unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    fn temp_toml(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".toml")
            .tempfile()
            .unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    // --- JSON tests ---

    #[test]
    fn read_package_json_format() {
        let f = temp_json(r#"{"name": "my-pkg", "version": "1.0.0"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.package_name, "my-pkg");
        assert_eq!(target.version, "1.0.0");
        assert!(!target.is_cargo());
    }

    #[test]
    fn read_mcp_server_format() {
        let f = temp_json(r#"{"package": "@scope/mcp-server", "version": "2.3.4"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.package_name, "@scope/mcp-server");
        assert_eq!(target.version, "2.3.4");
    }

    #[test]
    fn read_package_key_takes_precedence() {
        let f = temp_json(r#"{"package": "pkg-name", "name": "other-name", "version": "1.0.0"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.package_name, "pkg-name");
    }

    #[test]
    fn read_missing_name_and_package() {
        let f = temp_json(r#"{"version": "1.0.0"}"#);
        assert!(TargetFile::read(f.path()).is_err());
    }

    #[test]
    fn read_missing_version_defaults_to_zero() {
        let f = temp_json(r#"{"name": "my-pkg"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.package_name, "my-pkg");
        assert_eq!(target.version, "0.0.0");
    }

    #[test]
    fn read_invalid_json() {
        let f = temp_json("not json");
        assert!(TargetFile::read(f.path()).is_err());
    }

    #[test]
    fn read_file_not_found() {
        assert!(TargetFile::read(Path::new("/nonexistent/file.json")).is_err());
    }

    #[test]
    fn write_updates_version() {
        let f = temp_json(r#"{"name": "my-pkg", "version": "1.0.0"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        target.write(f.path(), "2.0.0").unwrap();

        let updated = TargetFile::read(f.path()).unwrap();
        assert_eq!(updated.version, "2.0.0");
        assert_eq!(updated.package_name, "my-pkg");
    }

    #[test]
    fn write_preserves_trailing_newline() {
        let f = temp_json(r#"{"name": "my-pkg", "version": "1.0.0"}"#);
        let target = TargetFile::read(f.path()).unwrap();
        target.write(f.path(), "2.0.0").unwrap();

        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.ends_with('\n'));
    }

    // --- TOML tests ---

    #[test]
    fn read_cargo_toml() {
        let f = temp_toml(
            r#"[package]
name = "my-crate"
version = "1.0.0"
"#,
        );
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.package_name, "my-crate");
        assert_eq!(target.version, "1.0.0");
        assert!(target.is_cargo());
    }

    #[test]
    fn read_cargo_toml_missing_name() {
        let f = temp_toml(
            r#"[package]
version = "1.0.0"
"#,
        );
        assert!(TargetFile::read(f.path()).is_err());
    }

    #[test]
    fn read_cargo_toml_missing_version() {
        let f = temp_toml(
            r#"[package]
name = "my-crate"
"#,
        );
        assert!(TargetFile::read(f.path()).is_err());
    }

    #[test]
    fn read_invalid_toml() {
        let f = temp_toml("not [valid toml");
        assert!(TargetFile::read(f.path()).is_err());
    }

    #[test]
    fn write_cargo_toml_updates_version() {
        let f = temp_toml(
            r#"[package]
name = "my-crate"
version = "1.0.0"
"#,
        );
        let target = TargetFile::read(f.path()).unwrap();
        target.write(f.path(), "2.0.0").unwrap();

        let updated = TargetFile::read(f.path()).unwrap();
        assert_eq!(updated.version, "2.0.0");
        assert_eq!(updated.package_name, "my-crate");
    }

    #[test]
    fn write_cargo_toml_preserves_comments() {
        let original = r#"[package]
name = "my-crate"
version = "1.0.0"
# This is a comment
edition = "2024"
"#;
        let f = temp_toml(original);
        let target = TargetFile::read(f.path()).unwrap();
        target.write(f.path(), "2.0.0").unwrap();

        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("# This is a comment"));
        assert!(content.contains("edition = \"2024\""));
        assert!(content.contains("version = \"2.0.0\""));
    }
}
