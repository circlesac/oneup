use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::cli::{Source, VersionArgs};
use crate::crates_io::CratesIoClient;
use crate::format::VersionFormat;
use crate::git_source;
use crate::npmrc::NpmrcConfig;
use crate::registry::{PackageInfo, RegistryClient};
use crate::target::TargetFile;

/// The concrete version source after resolving `--source auto`.
enum ResolvedSource {
    Git,
    Crates,
    Npm,
}

pub fn run(args: VersionArgs) -> Result<()> {
    // 1. Parse version format
    let fmt = VersionFormat::parse(&args.format)?;

    // 2. Resolve target paths
    let target_paths = if args.target.is_empty() {
        detect_targets()?
    } else {
        args.target.clone()
    };

    // 3. Read all targets, pick the primary (highest version) for registry query
    let mut targets: Vec<(PathBuf, TargetFile)> = Vec::new();
    for path in &target_paths {
        targets.push((path.clone(), TargetFile::read(path)?));
    }

    // Sort by version descending — first entry is primary
    targets.sort_by(|a, b| compare_versions(&b.1.version, &a.1.version));

    let (primary_path, primary_target) = &targets[0];

    if args.verbose {
        for (path, t) in &targets {
            eprintln!("[target] file: {} ({})", path.display(), t.version);
        }
        eprintln!("[target] primary: {}", primary_path.display());
        eprintln!("[target] package: {}", primary_target.package_name);
        eprintln!(
            "[format] {} (MICRO: {})",
            args.format,
            if fmt.has_micro() { "yes" } else { "no" }
        );
    }

    // 4. Query the version source for published versions (using primary target).
    //    Resolve `auto`: crates.io for Cargo.toml, git tags for gradle/Go, npm otherwise.
    let resolved = match args.source {
        Source::Git => ResolvedSource::Git,
        Source::Crates => ResolvedSource::Crates,
        Source::Npm => ResolvedSource::Npm,
        Source::Auto => {
            if primary_target.is_cargo() {
                ResolvedSource::Crates
            } else if primary_target.is_gradle() || primary_target.is_go() {
                ResolvedSource::Git
            } else {
                ResolvedSource::Npm
            }
        }
    };

    let project_dir = primary_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let info = match resolved {
        ResolvedSource::Git => {
            if args.verbose {
                eprintln!("[source] type: git tags");
                eprintln!("[source] dir: {}", project_dir.display());
            }

            git_source::get_package(project_dir, &fmt, args.verbose)
        }
        ResolvedSource::Crates => {
            let client = CratesIoClient::new(args.registry.as_deref());

            if args.verbose {
                eprintln!("[registry] type: crates.io");
            }

            client.get_package(&primary_target.package_name, args.verbose)?
        }
        ResolvedSource::Npm => {
            let scope = if primary_target.package_name.starts_with('@') {
                primary_target.package_name.split('/').next()
            } else {
                None
            };

            let (registry_url, auth_token) = if let Some(ref url) = args.registry {
                (url.trim_end_matches('/').to_string(), None)
            } else {
                let npmrc = NpmrcConfig::load(project_dir)?;
                let url = npmrc.registry_url(scope);
                let token = npmrc.auth_token(&url);
                (url, token)
            };

            if args.verbose {
                eprintln!("[registry] type: npm");
                eprintln!("[registry] url: {}", registry_url);
                eprintln!(
                    "[registry] auth: {}",
                    if auth_token.is_some() {
                        "token"
                    } else {
                        "none"
                    }
                );
            }

            let client = RegistryClient::new(&registry_url, auth_token);
            client.get_package(&primary_target.package_name, args.verbose)?
        }
    };

    // 5. Determine next version
    let new_version = determine_version(info, &primary_target.package_name, &fmt, args.verbose)?;

    // 6. Check if version actually changed
    if new_version == primary_target.version {
        if args.verbose {
            eprintln!("[bump] version unchanged: {}", new_version);
        }
        println!("{}", new_version);
        return Ok(());
    }

    if args.verbose {
        eprintln!("[bump] {} → {}", primary_target.version, new_version);
    }

    // 7. Dry run — just print and exit
    if args.dry_run {
        eprintln!(
            "[dry-run] would update {} → {}",
            primary_target.version, new_version
        );
        for (path, _) in &targets {
            eprintln!("[dry-run] would write {}", path.display());
        }
        println!("{}", new_version);
        return Ok(());
    }

    // 8. Update all target files
    for (path, target) in &targets {
        target.write(path, &new_version)?;

        if args.verbose {
            eprintln!("[file] updated {}", path.display());
        }
    }

    // 9. Print version to stdout
    println!("{}", new_version);

    Ok(())
}

fn detect_targets() -> Result<Vec<PathBuf>> {
    let mut targets = Vec::new();

    // Highest priority: npm / cargo manifests.
    for p in ["Cargo.toml", "package.json"] {
        let pb = PathBuf::from(p);
        if pb.exists() {
            targets.push(pb);
        }
    }

    // Lowest priority: gradle / Go source files (common Android module locations).
    for p in [
        "build.gradle",
        "build.gradle.kts",
        "app/build.gradle",
        "app/build.gradle.kts",
        "presentation/build.gradle",
        "presentation/build.gradle.kts",
        "version.go",
    ] {
        let pb = PathBuf::from(p);
        if pb.exists() {
            targets.push(pb);
        }
    }

    if targets.is_empty() {
        bail!(
            "no target files found (Cargo.toml, package.json, build.gradle, version.go) in current directory"
        );
    }

    Ok(targets)
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(a).cmp(&parse(b))
}

/// Bump logic:
///
/// With MICRO:
///   1. Fetch all versions from registry
///   2. Filter to versions matching today's date prefix
///   3. Find highest MICRO → next = highest + 1 (or 0 if none)
///   4. Warn if registry latest is ahead of today's date
///
/// Without MICRO:
///   1. Build today's date version (e.g., "26.2.0")
///   2. Check if it already exists in registry
///   3. If exists → no change (already current)
///   4. If not → use today's version
fn determine_version(
    info: PackageInfo,
    _package_name: &str,
    fmt: &VersionFormat,
    verbose: bool,
) -> Result<String> {
    match info {
        PackageInfo::NotFound => {
            let version = fmt.build_version(0);
            if verbose {
                eprintln!("[bump] package not in registry, starting at {}", version);
            }
            Ok(version)
        }
        PackageInfo::Found { versions, latest } => {
            // Warn if registry latest is ahead of today
            if let Some(latest_values) = fmt.extract_values(&latest) {
                if fmt.ahead_of_today(&latest_values) {
                    eprintln!(
                        "warning: registry latest {} is ahead of current date prefix",
                        latest
                    );
                }
            }

            if fmt.has_micro() {
                // With MICRO: find highest micro for today's prefix, increment
                let mut max_micro: Option<u64> = None;

                for v in &versions {
                    if let Some(values) = fmt.extract_values(v) {
                        if fmt.matches_today(&values) {
                            if let Some(micro) = fmt.micro_value(&values) {
                                max_micro = Some(max_micro.map_or(micro, |m: u64| m.max(micro)));
                            }
                        }
                    }
                }

                let next_micro = match max_micro {
                    Some(m) => m + 1,
                    None => 0,
                };

                let version = fmt.build_version(next_micro);

                if verbose {
                    match max_micro {
                        Some(m) => eprintln!(
                            "[bump] highest MICRO for today's prefix: {} → next: {}",
                            m, version
                        ),
                        None => eprintln!("[bump] no versions match today's prefix → {}", version),
                    }
                }

                Ok(version)
            } else {
                // Without MICRO: today's date version, no-op if already exists
                let version = fmt.build_version(0);

                let exists = versions.iter().any(|v| {
                    if let Some(values) = fmt.extract_values(v) {
                        fmt.matches_today(&values)
                    } else {
                        false
                    }
                });

                if exists {
                    if verbose {
                        eprintln!("[bump] {} already exists in registry, no change", version);
                    }
                } else if verbose {
                    eprintln!("[bump] new period → {}", version);
                }

                Ok(version)
            }
        }
    }
}
