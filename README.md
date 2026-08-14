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
