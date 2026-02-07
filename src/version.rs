use anyhow::{Result, bail};

use crate::cli::VersionArgs;
use crate::format::VersionFormat;
use crate::git::GitRepo;
use crate::npmrc::NpmrcConfig;
use crate::registry::{PackageInfo, RegistryClient};
use crate::target::TargetFile;

pub fn run(args: VersionArgs) -> Result<()> {
    // 1. Parse version format
    let fmt = VersionFormat::parse(&args.format)?;

    // 2. Read target file
    let target = TargetFile::read(&args.target)?;

    if args.verbose {
        eprintln!("[target] file: {}", args.target.display());
        eprintln!("[target] package: {}", target.package_name);
        eprintln!("[target] version: {}", target.version);
        eprintln!(
            "[format] {} (MICRO: {})",
            args.format,
            if fmt.has_micro() { "yes" } else { "no" }
        );
    }

    // 3. Resolve registry URL
    let project_dir = args
        .target
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let scope = if target.package_name.starts_with('@') {
        target.package_name.split('/').next()
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

    // 4. Query registry and determine next version
    let client = RegistryClient::new(&registry_url, auth_token);
    let new_version = determine_version(&client, &target.package_name, &fmt, args.verbose)?;

    // 5. Check if version actually changed
    if new_version == target.version {
        if args.verbose {
            eprintln!("[bump] version unchanged: {}", new_version);
        }
        println!("{}", new_version);
        return Ok(());
    }

    if args.verbose {
        eprintln!("[bump] {} → {}", target.version, new_version);
    }

    // 6. Dry run — just print and exit
    if args.dry_run {
        eprintln!(
            "[dry-run] would update {} → {}",
            target.version, new_version
        );
        if !args.no_git_tag_version {
            let msg = args.message.replace("%s", &new_version);
            eprintln!("[dry-run] would commit: \"{}\"", msg);
            eprintln!("[dry-run] would tag: v{}", new_version);
        }
        println!("{}", new_version);
        return Ok(());
    }

    // 7. Check working tree before making changes
    if !args.no_git_tag_version {
        let git = GitRepo::open(&args.target)?;

        if !args.force && !git.is_clean()? {
            bail!("working tree has uncommitted changes (use --force to proceed)");
        }
    }

    // 8. Update target file
    target.write(&args.target, &new_version)?;

    if args.verbose {
        eprintln!("[file] updated {}", args.target.display());
    }

    // 9. Git commit + tag (unless --no-git-tag-version)
    if !args.no_git_tag_version {
        let git = GitRepo::open(&args.target)?;

        if args.force {
            git.commit_and_tag_force(&args.target, &new_version, &args.message)?;
        } else {
            git.commit_and_tag(&args.target, &new_version, &args.message)?;
        }

        if args.verbose {
            let msg = args.message.replace("%s", &new_version);
            eprintln!("[git] committed: \"{}\"", msg);
            eprintln!("[git] tagged: v{}", new_version);
        }
    }

    // 9. Print version to stdout
    println!("{}", new_version);

    Ok(())
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
    client: &RegistryClient,
    package_name: &str,
    fmt: &VersionFormat,
    verbose: bool,
) -> Result<String> {
    let info = client.get_package(package_name, verbose)?;

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
