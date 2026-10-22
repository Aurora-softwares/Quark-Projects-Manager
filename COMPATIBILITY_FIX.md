# v0.2.4 — GitHub CLI compatibility

Some GitHub CLI versions do not support `gh project item-list --field`.
This release automatically retries `gh project item-list --format json --limit 10000`
**only** if the extended call fails with the known unsupported `--field` flag.
Other errors, including missing permissions, are returned without a fallback.

On older CLI releases, the Project and its issues remain viewable, but custom
column values such as Priority, Size, and Target Version may not be returned.
The UI displays a compatibility warning. Existing project fields can still be
retrieved via `gh project field-list`, and field editing is unchanged.

For full custom-column support, update GitHub CLI to a version that includes
`--field` in the output of `gh project item-list --help`.

## Regression coverage

- Basic item-list argument compatibility
- Optional custom field requests, including Status and Priority
- Older-CLI recovery after `unknown flag: --field`
- Normal results and custom values on newer versions
- Authentication errors do not trigger fallback
- Failure of fallback is propagated
- Basic-only project fields avoid unnecessary retry
- Unknown flags unrelated to `--field` remain errors
- Mock GitHub CLI script rejects unsupported flag and validates actual retry

## Verification

On a machine with Rust installed:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

This source release has **not** been compiled or tested by Cargo in the authoring environment.
