# Quark Projects v0.2.2 — GitHub CLI compatibility

**Reported issue**: your `gh` build rejects `gh api --slurp`, and GitHub Projects reports a missing `read:project` scope.

## Changes

- `src/github.rs`: organisation membership fetch now invokes `gh api --paginate --jq '.[].login' user/orgs?per_page=100`, avoiding `--slurp` entirely.
- Results are parsed as paginated newline-delimited organisation logins, ordered and deduplicated case-insensitively.
- Project-scoped errors include the command to request permissions and a reminder to check `GH_TOKEN` / `GITHUB_TOKEN` overrides.
- Mock GitHub CLI rejects `--slurp` in order to catch regressions, and tests include line endings, duplicates and permission guidance.

## On your machine

```bash
gh auth status
gh auth refresh -s project,read:org
gh project list --owner Aurora-softwares
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo run --release
```

If your CLI uses an environment token, refresh alone will not change its scopes. Either configure that token to have `project` access or remove the override from the app's environment.

**Verification note:** This archive has been checked for complete source files and patch presence, but `cargo`/`rustc` were not installed in the packaging runtime. Do not treat this as a build-verified release.
