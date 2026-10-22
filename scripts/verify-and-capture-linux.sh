#!/usr/bin/env bash
# Runs Quark Projects verification and captures a screenshot of the REAL running binary.
set -Eeuo pipefail
cd "$(dirname "$0")/.."
OUT="${1:-$(pwd)/verification}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
if ! command -v cargo >/dev/null 2>&1; then
  echo 'Rust/Cargo is not installed. Install Rust from rustup.rs and retry.' >&2
  exit 127
fi
printf '\n[1/5] Rust version\n'; { cargo --version; rustc --version; } | tee "$OUT/toolchain.txt"
printf '\n[2/5] Formatting\n'; cargo fmt --all -- --check 2>&1 | tee "$OUT/format.log"
printf '\n[3/5] Clippy\n'; cargo clippy --all-targets -- -D warnings 2>&1 | tee "$OUT/clippy.log"
printf '\n[4/5] Automated tests\n'; cargo test --all-targets 2>&1 | tee "$OUT/tests.log"
printf '\n[5/5] Release build\n'; cargo build --release 2>&1 | tee "$OUT/build.log"
BIN=target/release/quark-projects-manager
if [[ ! -x "$BIN" ]]; then echo "Binary missing: $BIN" >&2; exit 1; fi
if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
  echo 'No graphical desktop detected; completed build/testing but cannot capture screenshot.' >&2
  exit 2
fi
"$BIN" >"$OUT/gui-stdout.log" 2>"$OUT/gui-stderr.log" &
PID=$!
trap 'kill "$PID" 2>/dev/null || true' EXIT
for i in {1..24}; do
  kill -0 "$PID" 2>/dev/null || { echo "GUI terminated early; check $OUT/gui-stderr.log" >&2; exit 3; }
  if [[ -n "${DISPLAY:-}" ]] && command -v xwininfo >/dev/null 2>&1 && xwininfo -name 'Quark Projects' >/dev/null 2>&1; then break; fi
  sleep 0.5
done
sleep 2
SHOT="$OUT/quark-projects-real-ui.png"
if [[ -n "${DISPLAY:-}" ]] && command -v import >/dev/null 2>&1; then
  if import -window 'Quark Projects' "$SHOT" 2>"$OUT/screenshot-errors.log"; then
    echo "Real Quark Projects screenshot: $SHOT"
    exit 0
  fi
fi
if command -v gnome-screenshot >/dev/null 2>&1; then
  if gnome-screenshot -w -f "$SHOT" 2>>"$OUT/screenshot-errors.log"; then
    echo "Screenshot: $SHOT"
    exit 0
  fi
fi
if command -v scrot >/dev/null 2>&1 && [[ -n "${DISPLAY:-}" ]]; then
  if scrot -q 95 "$SHOT" 2>>"$OUT/screenshot-errors.log"; then
    echo "Whole-desktop screenshot (confirm Quark was visible): $SHOT"
    exit 0
  fi
fi
echo 'Tests/build may have succeeded, but screenshot capture is unavailable.' >&2
echo 'Take a screenshot manually, or install ImageMagick (import) on an X11 session.' >&2
exit 4
