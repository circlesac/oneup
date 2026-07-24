---
name: oneup
description: CalVer-based version management with oneup — use when working with versioning, releases, or CI/CD workflows that use oneup
---

# oneup — CalVer Version Management

oneup calculates the next CalVer version from a version source (npm registry, crates.io, or git tags) and writes it to target files. Projects stay versionless in git — oneup fills in the version at release time.

Supported targets: `package.json` (npm), `Cargo.toml` (crates), Android `build.gradle` / `build.gradle.kts` (gradle), and Go source files (a `Version = "..."` const). gradle and Go have no package registry, so they use **git tags** as the version source.

Install: `npm install -g @circlesac/oneup` or `brew install circlesac/tap/oneup` or `cargo install oneup`

## Philosophy

Versions don't belong in git. They're a release artifact, not source code.

- `package.json`: omit the `"version"` field entirely (npm allows versionless packages)
- `Cargo.toml`: use `version = "0.0.0"` (`cargo publish` requires the field to exist — oneup fills it before publish)

During release, oneup calculates the next version from the registry, writes it to target files, and prints it. Publishing and tagging happen separately in CI.

## CLI Reference

```
oneup version [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--target <PATH>` | Target file(s) to update — repeatable. Auto-detected if omitted (package.json, Cargo.toml, build.gradle / app/build.gradle / presentation/build.gradle, version.go) |
| `--registry <URL>` | Registry URL override for npm/crates (auto-detected from .npmrc or crates.io) |
| `--source <git\|npm\|crates\|auto>` | Version source. Default `auto`: crates.io for Cargo.toml, git tags for gradle/Go, npm otherwise. `git` forces git tags for any target |
| `--format <FMT>` | Version format using CalVer tokens. Default: `YY.MM.MICRO` |
| `--dry-run` | Show what would happen without making changes |
| `--verbose` | Print detailed debug output |

## Targets

| File | Detected by | Version field | Version source |
|------|-------------|---------------|----------------|
| `package.json` | `.json` | `version` | npm |
| `Cargo.toml` | `.toml` | `package.version` | crates.io |
| `build.gradle` / `build.gradle.kts` | `.gradle` / `.gradle.kts` | `versionName` (also bumps `versionCode`) | git tags |
| Go source (e.g. `version.go`) | `.go` | `Version = "..."` const/var | git tags |

**gradle:** oneup surgically rewrites only the `versionName` string (preserving quote style and formatting). It also updates `versionCode` (if present) to a monotonic integer derived from the CalVer version — numeric dot components `[a,b,c,d]` become `a*1_000_000 + b*10_000 + c*100 + d` (e.g. `26.7.0` → `26070000`). The `applicationId` is read as the package name (not otherwise used by the git source).

**Go:** oneup rewrites only the quoted string of a `Version = "..."` const/var. The module path is read from a sibling/ancestor `go.mod` for reference. `go.mod` itself is never touched (it has no module-version field).

## Git-tag version source

For gradle and Go there's no registry, so oneup derives published versions from **git tags**: it runs `git tag --list` in the target's directory, strips an optional leading `v`, keeps tags that parse under the active `--format`, and treats the highest as the latest. Non-matching tags (e.g. `nightly`) are ignored. If the directory isn't a git repo or no tag matches, oneup starts at MICRO `0`. Tag your releases (`git tag v$VERSION`) so the next MICRO increments correctly. Use `--source git` to force this source for any target.

## CalVer Format

Tokens: `YYYY` (full year), `YY` (short year), `MM` (month 1-12), `DD` (day 1-31), `MICRO` (auto-incrementing counter)

Rules:
- Separator must be `.` (dot only)
- MICRO must be last if present
- At least one date component required
- Auto-pads to 3 components for semver compatibility (e.g. `YY.MM` → `26.2.0`)

Common formats:
- `YY.MM.MICRO` → 26.2.5 (default — year.month.patch)
- `YYYY.MM.DD.MICRO` → 2026.2.17.0
- `YY.MM` → 26.2.0 (monthly, no counter)

## How Version Bumping Works

With MICRO: queries the registry for versions matching today's date prefix, finds the highest MICRO, increments by 1 (starts at 0 if none exist).

Without MICRO: uses today's date as the version. If it already exists in the registry, no change.

oneup prints the new version to stdout on success.

## CI Usage

In a release workflow, oneup writes the version, then you publish and tag:

```bash
VERSION=$(npx --yes @circlesac/oneup version | tail -1)
npm publish
git tag "v$VERSION" && git push origin "v$VERSION"
```

`tail -1` is needed because `npx` may print installation messages before the version output. oneup always prints the version as the last line of stdout.

No commits needed — the tag points at the source commit.
