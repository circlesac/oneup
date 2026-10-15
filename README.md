# oneup

CalVer-based version management for npm packages, Rust crates, Android (gradle) apps, and Go projects.

Version sources: npm registry, crates.io, or **git tags** (used for gradle/Go, where there's no registry). The version format defaults to `YY.MM.MICRO`.

For a package or service in a monorepo, scope git-tag versions with a prefix:

```bash
VERSION=$(oneup version --source git --tag-prefix 'auth@' --target apps/auth/package.json | tail -1)
git tag "auth@$VERSION"
```

Only matching tags such as `auth@26.7.0` participate in that version sequence;
other package tags and mutable channel tags such as `auth-dev` are ignored.

Channel-qualified version tags reserve their numeric version in the same
sequence. For example, `v26.8.2-internal` makes the next `YY.MM.MICRO` version
`26.8.3`. Use a suffix-free tag such as `v26.8.3` for the stable release; the
suffix describes the distribution channel and is never written to a target
version field.

## Android versionCode

The default `YY.MM.MICRO` format maps to `YYMMXXXX`: `YY*1_000_000 + MM*10_000 + MICRO`. For example, `26.10.100` produces `26100100`, and `26.10.9999` produces `26109999`, below `26.11.0` (`26110000`). MICRO supports 0–9999; 10000 is rejected before the Gradle file is changed.

Four-component versions retain `a*1_000_000 + b*10_000 + c*100 + d`, with each component after the first limited to 0–99. Codes must be in Google Play's supported range, 1–2100000000. Invalid values fail instead of silently overlapping another component. Keep the same component count within a version sequence; the encodings are not ordered across formats.

### Migrating existing Android apps

Older oneup releases multiplied the third component by 100 even for three-component versions. This correction can lower the code in an existing month: old `26.10.3` was `26100300`, while new `26.10.4` is `26100004`. Before distributing with the corrected mapping, compare the generated code with the highest code already distributed for that application ID (including store test tracks and direct installs). The new code must be strictly greater. Advance the release sequence through the normal version source, or wait until a later month produces a greater code; do not rewrite existing tags. oneup does not query app stores or installed devices to discover that floor.

## Development

Rust's standard formatter and Clippy linter match the existing Cargo toolchain and require no extra dependencies. Run the same gate locally and in pull requests:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

Run the built CLI with `target/debug/oneup version --source git --target path/to/build.gradle`. Releases are dispatched separately through `.github/workflows/release.yml`.

## Skills

| Skill | Description |
|-------|-------------|
| **oneup** | CalVer-based version management with oneup |

### Claude Code

```bash
# Add marketplace
/plugin marketplace add circlesac/oneup

# Install plugin
/plugin install oneup
```

### Pi

```bash
pi install git:circlesac/oneup
# or: npx @mariozechner/pi-coding-agent install git:circlesac/oneup
```

## License

MIT
