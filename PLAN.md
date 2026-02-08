# Add `oneup changelog` subcommand (#3)

## Context

Issue #3 requests a `oneup changelog` command that extracts git history between version tags and outputs structured data (commits, diff stats, changed files). The use case is feeding structured changelog data to coding agents for generating release notes.

## Approach

Follow the existing command pattern: args struct in `cli.rs`, new `Commands::Changelog` variant, new `src/changelog.rs` module with `pub fn run()`. Add git log/diff methods to the existing `GitRepo` struct in `src/git.rs`. No new dependencies needed.

## Changes

### 1. `src/cli.rs` — Add `ChangelogArgs` and format enum

- Add `ChangelogFormat` enum (Markdown/Json/Text) with `#[derive(ValueEnum)]`
- Add `ChangelogArgs` struct:
  - `--from-tag` (`Option<String>`) — start of range, default: latest `v*` tag
  - `--to` (`String`, default `"HEAD"`) — end of range
  - `--output` (`Option<PathBuf>`) — write to file, default: stdout
  - `--format` (`ChangelogFormat`, default: markdown)
- Add `Commands::Changelog(ChangelogArgs)` variant

### 2. `src/git.rs` — Add changelog methods to `GitRepo`

New structs: `CommitInfo`, `DiffStat`, `ChangedFile`

New methods:
- `latest_v_tag()` — walk from HEAD, find first commit with a `v*` tag. Uses `tag_foreach` + `peel_to_commit()` to build OID→tag map, then revwalk to find most recent
- `resolve_ref(refspec)` — thin wrapper around `revparse_single` + `peel_to_commit`
- `walk_commits(from_oid, to_ref)` — revwalk with `push(to)` / `hide(from)`, collect commit hash/author/date/message. Uses `chrono` for ISO 8601 date formatting
- `diff_stats(from_oid, to_ref)` — `diff_tree_to_tree` between the two commits' trees, return aggregate stats + per-file changes with status (added/modified/deleted/renamed)

### 3. `src/changelog.rs` — New module (orchestration + formatting)

Serializable structs: `ChangelogData`, `CommitEntry`, `StatsEntry`, `FileEntry` (all `#[derive(Serialize)]`)

`pub fn run(args)`:
1. Open git repo from cwd
2. Resolve `--from-tag` (or auto-detect via `latest_v_tag()`)
3. Gather commits via `walk_commits()`
4. Gather diff via `diff_stats()`
5. Format as markdown/json/text
6. Write to `--output` or stdout

Three format functions:
- `format_json` — `serde_json::to_string_pretty`
- `format_markdown` — header, commit list with short hash, stats summary, changed files
- `format_text` — plain text, same sections

### 4. `src/main.rs` — Register module + dispatch

- Add `mod changelog;`
- Add `Commands::Changelog(args) => changelog::run(args)` match arm

## Files touched
- `src/cli.rs` — `ChangelogFormat`, `ChangelogArgs`, `Commands::Changelog`
- `src/git.rs` — 3 structs + 4 methods (read-only git operations)
- `src/changelog.rs` — new file
- `src/main.rs` — mod + dispatch

## Files NOT touched
- `Cargo.toml` — no new dependencies
- `src/version.rs`, `src/format.rs`, `src/target.rs`, `src/registry.rs`, `src/crates_io.rs`, `src/npmrc.rs`

## Verification
- `cargo build` — compiles cleanly
- `cargo test` — existing tests pass + new formatter tests
- Manual: `oneup changelog --from-tag v26.2.4` against this repo
- Manual: `oneup changelog --format json` to verify JSON output
- Manual: `oneup changelog --from-tag v26.2.4 --to v26.2.5` for bounded range
