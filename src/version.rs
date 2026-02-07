use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::cli::VersionArgs;
use crate::crates_io::CratesIoClient;
use crate::format::VersionFormat;
use crate::git::GitRepo;
use crate::npmrc::NpmrcConfig;
use crate::registry::{PackageInfo, RegistryClient};
use crate::target::TargetFile;

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

    // 4. Query registry for published versions (using primary target)
    let info = if primary_target.is_cargo() {
        let client = CratesIoClient::new(args.registry.as_deref());

        if args.verbose {
            eprintln!("[registry] type: crates.io");
        }

        client.get_package(&primary_target.package_name, args.verbose)?
    } else {
        let project_dir = primary_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

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
    };

    // 5. Determine next version
    let new_version =
        determine_version(info, &primary_target.package_name, &fmt, args.verbose)?;

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
        if !args.no_git_tag_version {
            let msg = args.message.replace("%s", &new_version);
            eprintln!("[dry-run] would commit: \"{}\"", msg);
            eprintln!("[dry-run] would tag: v{}", new_version);
        }
        println!("{}", new_version);
        return Ok(());
    }

    // 8. Check working tree before making changes
    if !args.no_git_tag_version {
        let git = GitRepo::open(&targets[0].0)?;

        if !args.force && !git.is_clean()? {
            bail!("working tree has uncommitted changes (use --force to proceed)");
        }
    }

    // 9. Update all target files
    for (path, target) in &targets {
        target.write(path, &new_version)?;

        if args.verbose {
            eprintln!("[file] updated {}", path.display());
        }
    }

    // 10. Git commit + tag (unless --no-git-tag-version)
    if !args.no_git_tag_version {
        let git = GitRepo::open(&targets[0].0)?;
        let paths: Vec<&std::path::Path> = targets.iter().map(|(p, _)| p.as_path()).collect();

        if args.force {
            git.commit_and_tag_force(&paths, &new_version, &args.message)?;
        } else {
            git.commit_and_tag(&paths, &new_version, &args.message)?;
        }

        if args.verbose {
            let msg = args.message.replace("%s", &new_version);
            eprintln!("[git] committed: \"{}\"", msg);
            eprintln!("[git] tagged: v{}", new_version);
        }
    }

    // 11. Print version to stdout
    println!("{}", new_version);

    Ok(())
}

fn detect_targets() -> Result<Vec<PathBuf>> {
    let cargo = PathBuf::from("Cargo.toml");
    let package = PathBuf::from("package.json");

    match (cargo.exists(), package.exists()) {
        (true, true) => Ok(vec![cargo, package]),
        (true, false) => Ok(vec![cargo]),
        (false, true) => Ok(vec![package]),
        (false, false) => bail!("no Cargo.toml or package.json found in current directory"),
    }
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.').filter_map(|p| p.parse().ok()).collect()
    };
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
