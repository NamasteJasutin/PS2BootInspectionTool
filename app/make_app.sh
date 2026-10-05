#!/bin/sh
# Builds a release binary and wraps it in BootScreen.app (next to this script).
set -e
cd "$(dirname "$0")"
swift build -c release
APP=BootScreen.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp .build/release/BootScreen "$APP/Contents/MacOS/BootScreen"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>BootScreen</string>
    <key>CFBundleIdentifier</key><string>local.ps2bootscreen</string>
    <key>CFBundleName</key><string>PS2 Boot Screen</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
echo "built $PWD/$APP"
