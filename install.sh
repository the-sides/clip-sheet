#!/bin/zsh
# Build Clip Sheet and install it as ~/Applications/Clip Sheet.app (menu bar only, no Dock icon).
set -euo pipefail
cd "${0:A:h}"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
cargo build --release

APP="$HOME/Applications/Clip Sheet.app"
pkill -x clip-sheet 2>/dev/null && while pgrep -qx clip-sheet; do sleep 0.1; done || true
mkdir -p "$APP/Contents/MacOS"
cp target/release/clip-sheet "$APP/Contents/MacOS/clip-sheet"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Clip Sheet</string>
  <key>CFBundleIdentifier</key><string>local.clip-sheet</string>
  <key>CFBundleExecutable</key><string>clip-sheet</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSUIElement</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$APP"
open "$APP"
echo "Installed and launched: $APP"
