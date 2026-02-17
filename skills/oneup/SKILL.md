---
name: oneup
description: CalVer-based version management with oneup — use when working with versioning, releases, or CI/CD workflows that use oneup
---

# oneup — CalVer Version Management

oneup calculates the next CalVer version from the registry and writes it to target files. Projects stay versionless in git — oneup fills in the version at release time.

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
| `--target <PATH>` | Target file(s) to update — repeatable. Auto-detected if omitted (looks for package.json and Cargo.toml) |
| `--registry <URL>` | Registry URL override (auto-detected from .npmrc or crates.io) |
| `--format <FMT>` | Version format using CalVer tokens. Default: `YY.MM.MICRO` |
| `--dry-run` | Show what would happen without making changes |
| `--verbose` | Print detailed debug output |

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
