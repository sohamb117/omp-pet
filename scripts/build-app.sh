#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
profile="${1:-release}"
case "$profile" in
  release) cargo build --locked --release ;;
  debug) cargo build --locked ;;
  *) printf 'Usage: %s [release|debug]\n' "$0" >&2; exit 2 ;;
esac
app="$PWD/dist/OMP Pet.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
if [ -d "$app/Contents/Resources/zorua" ]; then rm -r "$app/Contents/Resources/zorua"; fi
cp "target/$profile/omp-pet" "$app/Contents/MacOS/omp-pet.new"
mv -f "$app/Contents/MacOS/omp-pet.new" "$app/Contents/MacOS/omp-pet"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>OMP Pet</string>
<key>CFBundleDisplayName</key><string>OMP Pet</string>
<key>CFBundleIdentifier</key><string>dev.soham.omp-pet</string>
<key>CFBundleExecutable</key><string>omp-pet</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleURLTypes</key><array><dict>
<key>CFBundleURLName</key><string>dev.soham.omp-pet.install</string>
<key>CFBundleURLSchemes</key><array><string>omppet</string></array>
<key>CFBundleTypeRole</key><string>Viewer</string>
</dict></array>
<key>LSUIElement</key><true/>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$app/Contents/Info.plist"
codesign --force --sign - "$app"
printf '%s\n' "$app"
