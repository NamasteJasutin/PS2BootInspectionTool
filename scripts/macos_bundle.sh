#!/bin/sh
# Wraps the release build of psxbootinspect in "PSX Boot Inspection Tool.app" (in dist/ by
# default, or the directory given), with the project icon. Builds first if needed.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/dist}"
VERSION="$(grep -m1 '^version' "$ROOT/psxbootinspect/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')"
cargo build --release -p psxbootinspect --manifest-path "$ROOT/Cargo.toml"

# bundle <name> <executable> <bundle id>
bundle() {
    APP="$OUT/$1.app"
    rm -rf "$APP"
    mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
    cp "$ROOT/target/release/$2" "$APP/Contents/MacOS/$2"
    cp "$ROOT/app/Icon/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
    cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>$2</string>
    <key>CFBundleIdentifier</key><string>$3</string>
    <key>CFBundleName</key><string>$1</string>
    <key>CFBundleDisplayName</key><string>$1</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    plutil -lint "$APP/Contents/Info.plist" >/dev/null
    echo "built $APP"
}

bundle "PSX Boot Inspection Tool" psxbootinspect jp.live.justiin.psxbootinspectiontool
