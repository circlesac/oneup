use std::path::Path;
use std::process::Command;

use crate::format::VersionFormat;
use crate::registry::PackageInfo;

/// Build a `PackageInfo` from the git tags in `dir`.
///
/// Runs `git -C <dir> tag --list`, strips an optional leading `v` from each tag,
/// keeps those that parse under the active `VersionFormat`, and returns
/// `PackageInfo::Found { versions, latest }`. If `dir` is not a git repo, git is
/// unavailable, or no tag matches the format, returns `PackageInfo::NotFound`.
pub fn get_package(dir: &Path, fmt: &VersionFormat, verbose: bool) -> PackageInfo {
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
    }

    let info = build_package_info(tags, fmt);

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
/// Strips a leading `v`, keeps only tags valid under `fmt`, and picks the highest
/// as `latest`. Empty result → `PackageInfo::NotFound`.
pub fn build_package_info(tags: Vec<String>, fmt: &VersionFormat) -> PackageInfo {
    let mut versions: Vec<String> = tags
        .iter()
        .map(|t| t.strip_prefix('v').unwrap_or(t).to_string())
        .filter(|t| fmt.extract_values(t).is_some())
        .collect();

    if versions.is_empty() {
        return PackageInfo::NotFound;
    }

    versions.sort_by(|a, b| compare_versions(a, b));
    let latest = versions.last().cloned().unwrap();

    PackageInfo::Found { versions, latest }
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
        match build_package_info(tags, &fmt()) {
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
        match build_package_info(tags, &fmt()) {
            PackageInfo::Found { latest, .. } => assert_eq!(latest, "26.7.10"),
            PackageInfo::NotFound => panic!("expected Found"),
        }
    }

    #[test]
    fn no_matching_tags_is_not_found() {
        let tags = vec!["nightly".to_string(), "release-1".to_string()];
        assert!(matches!(
            build_package_info(tags, &fmt()),
            PackageInfo::NotFound
        ));
    }

    #[test]
    fn empty_tags_is_not_found() {
        assert!(matches!(
            build_package_info(vec![], &fmt()),
            PackageInfo::NotFound
        ));
    }

    #[test]
    fn get_package_on_non_repo_is_not_found() {
        let dir = std::env::temp_dir().join("oneup-definitely-not-a-git-repo-xyz");
        assert!(matches!(
            get_package(&dir, &fmt(), false),
            PackageInfo::NotFound
        ));
    }
}
