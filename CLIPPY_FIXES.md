# Quark Projects v0.2.3 — Clippy compatibility fixes

This patch addresses the six Clippy diagnostics reported on Chimera with `-D warnings`:

- `src/main.rs`: use `= !` spacing for the Accounts pane toggle.
- `src/github.rs`: display `Project.url` in the Project header and `Item.state` / `Item.issue_type` in the item editor instead of carrying unused fields.
- `src/auth.rs`: change `map_or(true, ...)` to `is_none_or(...)`.
- `src/github.rs`: group the two import behaviour booleans in `ImportOptions`, updating UI and tests.
- `src/tests/auth_extended.rs`: use a boolean assertion rather than `assert_eq!(..., false)`.
- Add regression coverage for `ImportOptions::default()`.

## Verify on a machine with Rust installed

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

This archive has been statically patched and checked; Cargo was **not available in the authoring environment**. A successful build and test run are still required before release. All GitHub write paths remain as in v0.2.2; review the security audit before bulk import.
