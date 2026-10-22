#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
ARCH="$(dpkg --print-architecture)"
PKG="dist/quark-projects-manager_${VERSION}_${ARCH}"
mkdir -p "$PKG/DEBIAN" "$PKG/usr/bin" "$PKG/usr/share/applications"
install -m755 target/release/quark-projects-manager "$PKG/usr/bin/quark-projects-manager"
cat > "$PKG/DEBIAN/control" <<CONTROL
Package: quark-projects-manager
Version: ${VERSION}
Section: devel
Priority: optional
Architecture: ${ARCH}
Maintainer: Quark Projects community
Depends: libc6, libx11-6, libgl1, libwayland-client0
Description: Native GitHub Projects manager in Rust
 View Projects, manage issues, and import CSV without a browser UI.
 Requires GitHub CLI (gh) installed separately for GitHub authentication.
CONTROL
cat > "$PKG/usr/share/applications/quark-projects-manager.desktop" <<'DESKTOP'
[Desktop Entry]
Name=Quark Projects
Comment=Native GitHub Projects manager
Exec=quark-projects-manager
Terminal=false
Type=Application
Categories=Development;ProjectManagement;
DESKTOP
dpkg-deb --build "$PKG" "dist/quark-projects-manager_${VERSION}_${ARCH}.deb"
printf 'Created %s\n' "dist/quark-projects-manager_${VERSION}_${ARCH}.deb"
