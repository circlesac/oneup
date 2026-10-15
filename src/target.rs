use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::Value;
use std::path::Path;

enum TargetFormat {
    Json(Value),
    Toml(toml_edit::DocumentMut),
    /// Android build.gradle / build.gradle.kts — holds the raw file content.
    Gradle(String),
    /// Go source file with a `Version = "..."` const — holds the raw file content.
    Go(String),
}

pub struct TargetFile {
    pub package_name: String,
    pub version: String,
    format: TargetFormat,
}

impl TargetFile {
    pub fn read(path: &Path) -> Result<Self> {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with(".gradle") || name.ends_with(".gradle.kts") {
            Self::read_gradle(path)
        } else if name.ends_with(".go") {
            Self::read_go(path)
        } else {
            match path.extension().and_then(|e| e.to_str()) {
                Some("toml") => Self::read_toml(path),
                _ => Self::read_json(path),
            }
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

    fn read_gradle(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("target file not found: {}", path.display()))?;

        // Matches both Groovy (`versionName "1.0"`) and Kotlin DSL (`versionName = "1.0"`).
        let ver_re = Regex::new(r#"versionName\s*=?\s*['"]([^'"]+)['"]"#).unwrap();
        let version = ver_re
            .captures(&content)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .with_context(|| format!("no versionName found in {}", path.display()))?;

        let app_re = Regex::new(r#"applicationId\s*=?\s*['"]([^'"]+)['"]"#).unwrap();
        let package_name = app_re
            .captures(&content)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "app".to_string());

        Ok(Self {
            package_name,
            version,
            format: TargetFormat::Gradle(content),
        })
    }

    fn read_go(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("target file not found: {}", path.display()))?;

        let ver_re = Regex::new(r#"(?m)Version\s*=\s*"([^"]*)""#).unwrap();
        let version = ver_re
            .captures(&content)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .with_context(|| format!("no Version constant found in {}", path.display()))?;

        let package_name = find_go_module(path).unwrap_or_else(|| "app".to_string());

        Ok(Self {
            package_name,
            version,
            format: TargetFormat::Go(content),
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
            TargetFormat::Gradle(content) => {
                // Surgically replace only the versionName string value.
                let ver_re = Regex::new(r#"(versionName\s*=?\s*['"])([^'"]+)(['"])"#).unwrap();
                let mut output = ver_re
                    .replace(content, |c: &regex::Captures| {
                        format!("{}{}{}", &c[1], new_version, &c[3])
                    })
                    .into_owned();

                // Update versionCode to a monotonic value derived from the CalVer version,
                // but only if a versionCode line exists.
                let code_re = Regex::new(r#"(versionCode\s*=?\s*)(\d+)"#).unwrap();
                if code_re.is_match(&output) {
                    let code = version_code(new_version)?;
                    output = code_re
                        .replace(&output, |c: &regex::Captures| format!("{}{}", &c[1], code))
                        .into_owned();
                }

                std::fs::write(path, output)
                    .with_context(|| format!("failed to write {}", path.display()))?;
            }
            TargetFormat::Go(content) => {
                // Surgically replace only the quoted version string.
                let ver_re = Regex::new(r#"(Version\s*=\s*")([^"]*)(")"#).unwrap();
                let output = ver_re
                    .replace(content, |c: &regex::Captures| {
                        format!("{}{}{}", &c[1], new_version, &c[3])
                    })
                    .into_owned();

                std::fs::write(path, output)
                    .with_context(|| format!("failed to write {}", path.display()))?;
            }
        }
        Ok(())
    }

    pub fn is_cargo(&self) -> bool {
        matches!(self.format, TargetFormat::Toml(_))
    }

    pub fn is_gradle(&self) -> bool {
        matches!(self.format, TargetFormat::Gradle(_))
    }

    pub fn is_go(&self) -> bool {
        matches!(self.format, TargetFormat::Go(_))
    }
}

/// Pack three-component versions as YYMMXXXX, leaving four digits for MICRO.
/// Four-component formats retain two digits per trailing component.
fn version_code(version: &str) -> Result<u64> {
    let nums = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("invalid Android version: {version}"))?;
    if nums.is_empty() || nums.len() > 4 {
        bail!("Android versions require one to four numeric components: {version}");
    }
    let a = nums[0];
    let b = nums.get(1).copied().unwrap_or(0);
    let c = nums.get(2).copied().unwrap_or(0);
    let tail = if nums.len() == 4 {
        let d = nums[3];
        if c > 99 || d > 99 {
            bail!("four-component Android versions require trailing components <= 99: {version}");
        }
        c * 100 + d
    } else {
        if c > 9999 {
            bail!("Android MICRO must be <= 9999: {version}");
        }
        c
    };
    if a > 2100 || b > 99 {
        bail!("Android version components exceed versionCode capacity: {version}");
    }
    let code = a * 1_000_000 + b * 10_000 + tail;
    if code == 0 || code > 2_100_000_000 {
        bail!("Android versionCode must be between 1 and 2100000000: {version}");
    }
    Ok(code)
}

/// Walk up from a Go source file's directory looking for a `go.mod`,
/// returning its module path if found.
fn find_go_module(path: &Path) -> Option<String> {
    let mut dir = path.parent();
    let go_mod = loop {
        let d = dir?;
        let candidate = d.join("go.mod");
        if candidate.exists() {
            break candidate;
        }
        dir = d.parent();
    };

    let content = std::fs::read_to_string(&go_mod).ok()?;
    let re = Regex::new(r"(?m)^module\s+(\S+)").unwrap();
    re.captures(&content).map(|c| c[1].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_json(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    fn temp_toml(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    fn temp_with_suffix(suffix: &str, content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(suffix).tempfile().unwrap();
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

    // --- version_code ---

    #[test]
    fn version_code_derivation() {
        assert_eq!(version_code("26.7.0").unwrap(), 26_070_000);
        assert_eq!(version_code("26.7.3").unwrap(), 26_070_003);
        assert_eq!(version_code("26.10.100").unwrap(), 26_100_100);
        assert_eq!(version_code("26.10.9999").unwrap(), 26_109_999);
        assert_eq!(version_code("26.12.5.9").unwrap(), 26_120_509);
        assert_eq!(version_code("26.7").unwrap(), 26_070_000);
        assert_eq!(version_code("1").unwrap(), 1_000_000);
        assert_eq!(version_code("2100.0.0").unwrap(), 2_100_000_000);
        assert_eq!(version_code("2026.10.15.0").unwrap(), 2_026_101_500);
    }

    #[test]
    fn version_code_is_monotonic() {
        let versions = [
            "26.10.0",
            "26.10.1",
            "26.10.99",
            "26.10.100",
            "26.10.9999",
            "26.11.0",
            "26.12.9999",
            "27.1.0",
        ];
        for pair in versions.windows(2) {
            assert!(version_code(pair[0]).unwrap() < version_code(pair[1]).unwrap());
        }
        assert!(version_code("26.7.31.99").unwrap() < version_code("26.8.1.0").unwrap());
    }

    #[test]
    fn version_code_rejects_overflow_and_invalid_components() {
        for version in [
            "26.10.10000",
            "26.100.0",
            "26.12.100.0",
            "26.12.1.100",
            "2100.0.1",
            "18446744073709551615.1.0",
            "18446744073709551616.1.0",
            "0.0.0",
            "26.10.bad",
            "26..0",
            "26.10.0.0.1",
            "",
        ] {
            assert!(version_code(version).is_err(), "accepted {version}");
        }
    }

    #[test]
    fn invalid_version_code_does_not_modify_gradle() {
        let f = temp_with_suffix(".gradle", GROOVY_GRADLE);
        let target = TargetFile::read(f.path()).unwrap();
        assert!(target.write(f.path(), "26.10.10000").is_err());
        assert_eq!(std::fs::read_to_string(f.path()).unwrap(), GROOVY_GRADLE);
    }

    // --- Gradle (Groovy) ---

    const GROOVY_GRADLE: &str = r#"android {
    defaultConfig {
        applicationId 'com.example.app'
        versionCode 1234
        versionName '25.6.2'
    }
}
"#;

    #[test]
    fn read_gradle_groovy() {
        let f = temp_with_suffix(".gradle", GROOVY_GRADLE);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.version, "25.6.2");
        assert_eq!(target.package_name, "com.example.app");
        assert!(target.is_gradle());
        assert!(!target.is_cargo());
    }

    #[test]
    fn write_gradle_groovy_updates_version_and_code() {
        let f = temp_with_suffix(".gradle", GROOVY_GRADLE);
        let target = TargetFile::read(f.path()).unwrap();
        target.write(f.path(), "26.7.100").unwrap();

        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("versionName '26.7.100'"));
        assert!(content.contains("versionCode 26070100"));
        // Preserve the rest of the file.
        assert!(content.contains("applicationId 'com.example.app'"));
        assert!(content.contains("android {"));

        let updated = TargetFile::read(f.path()).unwrap();
        assert_eq!(updated.version, "26.7.100");
    }

    #[test]
    fn gradle_missing_version_code_left_alone() {
        let f = temp_with_suffix(
            ".gradle",
            "android {\n    defaultConfig {\n        versionName '25.6.2'\n    }\n}\n",
        );
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.package_name, "app"); // no applicationId → placeholder
        target.write(f.path(), "26.7.100").unwrap();

        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("versionName '26.7.100'"));
        assert!(!content.contains("versionCode"));
    }

    // --- Gradle (Kotlin DSL) ---

    const KTS_GRADLE: &str = r#"android {
    defaultConfig {
        applicationId = "com.example.app"
        versionCode = 1234
        versionName = "25.6.2"
    }
}
"#;

    #[test]
    fn read_gradle_kts() {
        let f = temp_with_suffix(".gradle.kts", KTS_GRADLE);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.version, "25.6.2");
        assert_eq!(target.package_name, "com.example.app");
        assert!(target.is_gradle());
    }

    #[test]
    fn write_gradle_kts_updates_version_and_code() {
        let f = temp_with_suffix(".gradle.kts", KTS_GRADLE);
        let target = TargetFile::read(f.path()).unwrap();
        target.write(f.path(), "26.7.100").unwrap();

        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains("versionName = \"26.7.100\""));
        assert!(content.contains("versionCode = 26070100"));
        assert!(content.contains("applicationId = \"com.example.app\""));
    }

    // --- Go ---

    const GO_SRC: &str = r#"package version

// Version is the current build version.
const Version = "25.6.2"
"#;

    #[test]
    fn read_go() {
        let f = temp_with_suffix(".go", GO_SRC);
        let target = TargetFile::read(f.path()).unwrap();
        assert_eq!(target.version, "25.6.2");
        assert!(target.is_go());
        // No sibling go.mod in the temp dir → placeholder.
        assert_eq!(target.package_name, "app");
    }

    #[test]
    fn write_go_updates_version() {
        let f = temp_with_suffix(".go", GO_SRC);
        let target = TargetFile::read(f.path()).unwrap();
        target.write(f.path(), "26.7.0").unwrap();

        let content = std::fs::read_to_string(f.path()).unwrap();
        assert!(content.contains(r#"const Version = "26.7.0""#));
        assert!(content.contains("package version"));

        let updated = TargetFile::read(f.path()).unwrap();
        assert_eq!(updated.version, "26.7.0");
    }

    #[test]
    fn read_go_with_var_and_module() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("go.mod"),
            "module github.com/acme/tool\n\ngo 1.22\n",
        )
        .unwrap();
        let go_path = dir.path().join("version.go");
        std::fs::write(&go_path, "package main\n\nvar Version = \"25.6.2\"\n").unwrap();

        let target = TargetFile::read(&go_path).unwrap();
        assert_eq!(target.version, "25.6.2");
        assert_eq!(target.package_name, "github.com/acme/tool");
    }
}
