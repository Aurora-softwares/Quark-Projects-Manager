# Quark Projects v0.2.1 — verification status, 22 October 2026

## Actions attempted in this environment
- Confirmed the provided ZIP exists and passes ZIP integrity validation.
- Confirmed the source tree contains **124 `#[test]` declarations** (tests were NOT executed).
- Parsed the bundled 75-row Quark IDE import CSV successfully using Python's CSV parser.
- Parsed the GitHub Actions workflow YAML successfully.
- Inspected Rust source for GUI, CLI, import, OAuth, and packaging faults.
- Tried to locate Rust/Cargo, including system paths: **neither installed**.
- Tried to access the Rust distribution endpoint to install a compiler: **DNS/network access unavailable**.
- Confirmed Xvfb, ImageMagick, and GNOME screenshot tools exist, but the Quark executable is absent.

**Result: compilation NOT attempted (no compiler). Rust tests NOT run. App NOT launched. Real GUI screenshot NOT captured.**

## Findings from source review
1. **High, import correctness:** `src/github.rs::parse_existing_issues()` maps case-insensitive issue titles to just one URL and `import()` reuses it, risking updates against the wrong issue if a repository has two issues sharing a title. Add unique import IDs and a conflict review/dry-run before production imports.
2. **Medium, reliability:** `src/github.rs::cli()` uses `Command::output()` without a timeout or output-size cap. A stuck CLI or large API response can leave the job running indefinitely or consume excess memory.
3. **Medium, performance:** `src/main.rs::projects_page()` clones the entire `Board`, clones matching items, then lays out up to 1,500 table items on each redraw. Use borrowed/cached data and virtualized rows for large Projects.
4. **Medium, API latency:** `src/github.rs::all_projects()` loads organisation Projects sequentially. Bounded concurrency would make multi-organisation navigation quicker.
5. **Medium, data integrity:** CSV import has no complete preflight/dry-run, atomicity or persistent resume state. Cancellation leaves successful earlier writes in place by design.
6. **Low, packaging bug fixed in attached verified-source package:** `packaging/macos-app.sh` hardcoded `CFBundleShortVersionString=0.1.0` for this 0.2.1 build. Script now derives it from Cargo.toml; this patch is shell-syntax checked, not execution tested on macOS.
7. **Release reproducibility:** No `Cargo.lock` was present in the source ZIP. Commit a generated lockfile for reproducible application builds.
8. **Missing verification:** Headless egui tests do not prove that a native window opens, click interactions work, or OAuth succeeds with real GitHub permissions.

## Running the *real* test/build/screenshot on Linux (Chimera)
From extracted project directory:

```bash
./scripts/verify-and-capture-linux.sh
```

The script stops on the first failed check. If the checks pass, it launches the executable and saves a real screenshot to `verification/quark-projects-real-ui.png`, along with test/build logs. It only performs read-only authentication checks on launch; **do not interact with production Projects during a screenshot run**.

To investigate any failure, open the corresponding `verification/*.log` file. On Windows/macOS use `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, `cargo build --release`, then run the executable and use the operating system's screenshot utility.
