# Quark Projects v0.2.2 — automated validation and manual acceptance

The Rust source contains **126 `#[test]` cases** across production modules and extended test modules. These are source-defined tests, **not a claim that tests have passed**: the packaging environment lacks Cargo and an installed Rust compiler. Before merging or tagging, run the checks on all three operating systems.

## Standard release gate

```sh
# After editing Rust sources, format first and commit the formatting changes:
cargo fmt --all

# CI / release verification (these commands must succeed without modification):
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

For a focused investigation:

```sh
cargo test auth:: -- --nocapture
cargo test csv_import:: -- --nocapture
cargo test github:: -- --nocapture
cargo test filters:: -- --nocapture
cargo test ui_extended_tests:: -- --nocapture
cargo test stress_extended_tests:: -- --nocapture
```

CI runs formatting checks, Clippy, tests and a release build on Linux, Windows, macOS Apple Silicon and macOS Intel. Builds now use read-only repository permissions; uploading verified installer artifacts for a published release is a separate job with write permission.

## Automated coverage matrix

| Area | Automated cases | Behaviours exercised |
| --- | ---: | --- |
| OAuth and account parsing | 18 | Active/invalid accounts, redacted device secret, username boundaries, device response parsing, incomplete responses, expiry and denials, client ID validation |
| CSV import parsing | 25 | Quoted/multiline/Unicode fields, BOM, CRLF, missing headers, wrong widths, invalid UTF-8, issue metadata, GitHub field mapping, repository names, real 75-row fixture |
| Project/issue filtering | 21 | All/personal/organisation scopes, closed projects, search, owner/number collisions, custom fields, simultaneous filters, original references |
| GitHub API parsing and write operations | 33 | Projects, fields, issue parsing, organisation pagination, valid/spoofed issue URLs, milestones, CLI argument construction, fake-CLI CRUD/import/failure and cancellation flows |
| Desktop state and headless rendering | 26 | UI defaults, selection, issue/draft state, load/reload, events, errors, bounded logs, CSV preview, table/board renders |
| Stress fixtures | 3 | 5,000 projects, 3,000 issues, 500 CSV rows |
| **Total** | **126** | Includes original tests and new extended coverage |

**Important:** Integration tests use a disposable fake `gh` executable on Unix. They do not create, edit, or delete actual GitHub resources. The UI tests exercise headless rendering and state transitions, but not pixel-perfect layout or real mouse clicks. Tests are intended to detect regressions, not guarantee every behaviour works under every environment.

## Known gaps (not automated yet)

- Live GitHub OAuth device authorisation, token persistence and account switching against real credentials.
- End-to-end testing of GitHub Enterprise / SSO / organisation-specific permissions.
- Real GitHub Projects V2 GraphQL and rate-limit behaviours, transient network outages, oversized CLI outputs, and import rollback/recovery.
- Real interactive UI input automation, screen reader accessibility, keyboard navigation and installer launch on each OS.
- Import conflict review/dry-run (not yet implemented in the application), ambiguous duplicate titles and unattended account changes during bulk imports.
- Network request timeout and resource limits under adversarial conditions.

Those need additional engineering and disposable test accounts before this project should be treated as security-approved. **Do not run the full roadmap import against a production organisation solely because these tests pass.**

## Recommended manual release acceptance

1. Install on a fresh Windows, macOS and Linux machine; open the native application.
2. Sign in using GitHub CLI and OAuth device login; verify account switch and logout behaviour.
3. List personal, organisation and all Projects, including closed Projects and multiple owners with the same Project number.
4. In a disposable repository, create and edit a Project, issue, draft and custom fields; update Status/Priority/Size.
5. Preview a two-row CSV, import it, re-import it, and confirm expected issue/Project item identities.
6. Repeat with a deliberately invalid CSV, an inaccessible repository, and a mid-import cancel.
7. Confirm the correct account and repository are shown before authorising any bulk write.
8. Verify no access token or OAuth device secret is present in the application log, exception output, CLI arguments, settings or installer packages.

## Reporting failures

Attach OS/Rust versions, the exact failing test output, and the relevant fixture. Never attach credential files, OAuth tokens, or complete `gh auth status` output unless secrets are redacted.
