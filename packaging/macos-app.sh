#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
APP="dist/Quark Projects.app"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/quark-projects-manager "$APP/Contents/MacOS/QuarkProjects"
chmod +x "$APP/Contents/MacOS/QuarkProjects"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.aurorasoftwares.quarkprojects</string>
<key>CFBundleName</key><string>Quark Projects</string>
<key>CFBundleDisplayName</key><string>Quark Projects</string>
<key>CFBundleExecutable</key><string>QuarkProjects</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>${VERSION}</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --deep --sign - "$APP"
hdiutil create -volname "Quark Projects" -srcfolder "$APP" -ov -format UDZO "dist/QuarkProjects-macOS.dmg"
