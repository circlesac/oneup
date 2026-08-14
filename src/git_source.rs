use std::path::Path;
use std::process::Command;

use crate::format::VersionFormat;
use crate::registry::PackageInfo;

/// Build a `PackageInfo` from the git tags in `dir`.
///
/// Runs `git -C <dir> tag --list`, keeps only tags with `tag_prefix` when one is
/// configured, strips an optional leading `v` from the remaining version, and
/// keeps those whose numeric base parses under the active `VersionFormat`.
/// Channel suffixes such as `-internal` or `-rc.1` consume the same numeric
/// sequence as their suffix-free release tag. If `dir` is not a git repo, git
/// is unavailable, or no tag matches, returns `PackageInfo::NotFound`.
pub fn get_package(
    dir: &Path,
    fmt: &VersionFormat,
    tag_prefix: Option<&str>,
    verbose: bool,
) -> PackageInfo {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("tag")
        .arg("--list")
        .output();

    let tags: Vec<String> = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        _ => {
            if verbose {
                eprintln!(
                    "[git] not a git repository or git unavailable in {}",
                    dir.display()
                );
            }
            return PackageInfo::NotFound;
        }
    };

    if verbose {
        eprintln!("[git] found {} tag(s)", tags.len());
        if let Some(prefix) = tag_prefix {
            eprintln!("[git] tag prefix: {}", prefix);
        }
    }

    let info = build_package_info(tags, fmt, tag_prefix);

    if verbose {
        match &info {
            PackageInfo::Found { versions, latest } => {
                eprintln!(
                    "[git] {} matching version(s), latest: {}",
                    versions.len(),
                    latest
                );
            }
            PackageInfo::NotFound => eprintln!("[git] no tags match the active format"),
        }
    }

    info
}

/// Pure tag → `PackageInfo` transform (no I/O), suitable for unit testing.
///
/// Requires and strips `tag_prefix` when configured, strips an optional leading
/// `v` from the remaining version, strips a valid channel suffix, keeps only
/// numeric versions valid under `fmt`, and picks the highest as `latest`.
/// Empty result → `PackageInfo::NotFound`.
pub fn build_package_info(
    tags: Vec<String>,
    fmt: &VersionFormat,
    tag_prefix: Option<&str>,
) -> PackageInfo {
    let mut versions: Vec<String> = tags
        .iter()
        .filter_map(|tag| {
            let version = match tag_prefix {
                Some(prefix) => tag.strip_prefix(prefix)?,
                None => tag,
            };
            numeric_version(version.strip_prefix('v').unwrap_or(version), fmt)
        })
        .collect();

    if versions.is_empty() {
        return PackageInfo::NotFound;
    }

    versions.sort_by(|a, b| compare_versions(a, b));
    versions.dedup();
    let latest = versions.last().cloned().unwrap();

    PackageInfo::Found { versions, latest }
}

fn numeric_version(tag: &str, fmt: &VersionFormat) -> Option<String> {
    if fmt.extract_values(tag).is_some() {
        return Some(tag.to_string());
    }

    let (version, suffix) = tag.split_once('-')?;
    let valid_suffix = suffix.split('.').all(|identifier| {
        !identifier.is_empty()
            && identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    });

    (valid_suffix && fmt.extract_values(version).is_some()).then(|| version.to_string())
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(a).cmp(&parse(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt() -> VersionFormat {
        VersionFormat::parse("YY.MM.MICRO").unwrap()
    }

    #[test]
    fn strips_leading_v_and_filters_invalid() {
        let tags = vec![
            "v26.7.0".to_string(),
            "26.7.1".to_string(),
            "release-1".to_string(), // not a version
            "v26.13.0".to_string(),  // invalid month
            "nightly".to_string(),
        ];
        match build_package_info(tags, &fmt(), None) {
            PackageInfo::Found { versions, latest } => {
                assert_eq!(versions, vec!["26.7.0", "26.7.1"]);
                assert_eq!(latest, "26.7.1");
            }
            PackageInfo::NotFound => panic!("expected Found"),
        }
    }

    #[test]
    fn picks_highest_as_latest() {
        let tags = vec![
            "v26.2.3".to_string(),
            "v26.7.0".to_string(),
            "v26.7.10".to_string(),
            "v25.12.9".to_string(),
        ];
        match build_package_info(tags, &fmt(), None) {
            PackageInfo::Found { latest, .. } => assert_eq!(latest, "26.7.10"),
            PackageInfo::NotFound => panic!("expected Found"),
        }
    }

    #[test]
    fn channel_suffix_consumes_numeric_version() {
        let tags = vec![
            "v26.8.0".to_string(),
            "v26.8.1-internal".to_string(),
            "v26.8.2-rc.1".to_string(),
        ];
        match build_package_info(tags, &fmt(), None) {
            PackageInfo::Found { versions, latest } => {
                assert_eq!(versions, vec!["26.8.0", "26.8.1", "26.8.2"]);
                assert_eq!(latest, "26.8.2");
            }
            PackageInfo::NotFound => panic!("expected Found"),
        }
    }

    #[test]
    fn stable_and_channel_tags_share_one_numeric_version() {
        let tags = vec!["v26.8.2-internal".to_string(), "v26.8.2".to_string()];
        match build_package_info(tags, &fmt(), None) {
            PackageInfo::Found { versions, latest } => {
                assert_eq!(versions, vec!["26.8.2"]);
                assert_eq!(latest, "26.8.2");
            }
            PackageInfo::NotFound => panic!("expected Found"),
        }
    }

    #[test]
    fn rejects_invalid_channel_suffixes() {
        let tags = vec![
            "v26.8.1-".to_string(),
            "v26.8.2-internal/one".to_string(),
            "v26.8.3-internal..one".to_string(),
        ];
        assert!(matches!(
            build_package_info(tags, &fmt(), None),
            PackageInfo::NotFound
        ));
    }

    #[test]
    fn no_matching_tags_is_not_found() {
        let tags = vec!["nightly".to_string(), "release-1".to_string()];
        assert!(matches!(
            build_package_info(tags, &fmt(), None),
            PackageInfo::NotFound
        ));
    }

    #[test]
    fn empty_tags_is_not_found() {
        assert!(matches!(
            build_package_info(vec![], &fmt(), None),
            PackageInfo::NotFound
        ));
    }

    #[test]
    fn tag_prefix_selects_only_its_namespace() {
        let tags = vec![
            "v26.7.9".to_string(),
            "cli@26.7.8".to_string(),
            "auth@26.7.1".to_string(),
            "auth@26.7.3".to_string(),
            "auth-dev".to_string(),
        ];
        match build_package_info(tags, &fmt(), Some("auth@")) {
            PackageInfo::Found { versions, latest } => {
                assert_eq!(versions, vec!["26.7.1", "26.7.3"]);
                assert_eq!(latest, "26.7.3");
            }
            PackageInfo::NotFound => panic!("expected Found"),
        }
    }

    #[test]
    fn tag_prefix_preserves_optional_leading_v() {
        let tags = vec!["auth@v26.7.2".to_string()];
        match build_package_info(tags, &fmt(), Some("auth@")) {
            PackageInfo::Found { versions, latest } => {
                assert_eq!(versions, vec!["26.7.2"]);
                assert_eq!(latest, "26.7.2");
            }
            PackageInfo::NotFound => panic!("expected Found"),
        }
    }

    #[test]
    fn tag_prefix_preserves_channel_suffix() {
        let tags = vec!["auth@v26.8.2-internal".to_string()];
        match build_package_info(tags, &fmt(), Some("auth@")) {
            PackageInfo::Found { versions, latest } => {
                assert_eq!(versions, vec!["26.8.2"]);
                assert_eq!(latest, "26.8.2");
            }
            PackageInfo::NotFound => panic!("expected Found"),
        }
    }

    #[test]
    fn missing_tag_prefix_is_not_found() {
        let tags = vec!["v26.7.2".to_string(), "cli@26.7.3".to_string()];
        assert!(matches!(
            build_package_info(tags, &fmt(), Some("auth@")),
            PackageInfo::NotFound
        ));
    }

    #[test]
    fn get_package_on_non_repo_is_not_found() {
        let dir = std::env::temp_dir().join("oneup-definitely-not-a-git-repo-xyz");
        assert!(matches!(
            get_package(&dir, &fmt(), None, false),
            PackageInfo::NotFound
        ));
    }
}
