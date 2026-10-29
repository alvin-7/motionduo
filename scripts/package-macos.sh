#!/bin/sh
set -eu
# Run from the repository root. Builds/stages only; never installs or launches UI.
# MACOS_SIGN_IDENTITY (optional MACOS_SIGN_KEYCHAIN) selects a stable signing
# identity so TCC grants survive reinstalls; ad-hoc signatures change per build.
sign() {
    codesign --force ${MACOS_SIGN_KEYCHAIN:+--keychain "$MACOS_SIGN_KEYCHAIN"} --sign "${MACOS_SIGN_IDENTITY:--}" "$1"
}
[ "$(uname -s)" = Darwin ] || { echo 'macOS host required' >&2; exit 1; }
[ -f mousehop-mygo/go.mod ] || { echo 'Run from the repository root' >&2; exit 1; }
sh scripts/build-mygo.sh --release
sh scripts/makeicns.sh
version=$(sed -n 's/^version = "\([^"]*\)".*/\1/p' Cargo.toml | head -1)
bundle="target/release/bundle/osx/MotionDuo.app"
[ ! -e "$bundle" ] || { echo "Refusing to overwrite $bundle" >&2; exit 1; }
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp target/release/motionduo target/release/motionduo-ui "$bundle/Contents/MacOS/"
cp target/icon.icns "$bundle/Contents/Resources/"
cp LICENSE NOTICE mousehop-mygo/THIRD_PARTY_NOTICES.md "$bundle/Contents/Resources/"
python3 - "$bundle" "$version" <<'PY'
import plistlib
import re
import subprocess
from pathlib import Path
import sys
bundle, version = Path(sys.argv[1]), sys.argv[2]
if not re.fullmatch(r"\d+\.\d+\.\d+", version):
    raise ValueError(f"Invalid bundle version: {version!r}")
minimum = (12, 0)
for binary in (bundle / "Contents/MacOS").iterdir():
    commands = subprocess.check_output(["otool", "-l", str(binary)], text=True)
    versions = re.findall(r"\bminos\s+(\d+(?:\.\d+)+)", commands)
    versions += re.findall(r"cmd LC_VERSION_MIN_MACOSX\s+cmdsize \d+\s+version (\d+(?:\.\d+)+)", commands)
    if not versions:
        raise RuntimeError(f"Missing Mach-O deployment target: {binary}")
    minimum = max(minimum, *(tuple(map(int, value.split('.'))) for value in versions))
info = dict(
    CFBundleName="MotionDuo", CFBundleDisplayName="MotionDuo",
    CFBundleIdentifier="com.mousehop.Mousehop", CFBundlePackageType="APPL",
    CFBundleExecutable="motionduo-ui", CFBundleIconFile="icon.icns",
    CFBundleShortVersionString=version, CFBundleVersion=version,
    LSMinimumSystemVersion=".".join(map(str, minimum)), NSHighResolutionCapable=True,
    LSUIElement=True, NSAppSleepDisabled=True,
    NSInputMonitoringUsageDescription="MotionDuo captures input to share it with your computers.",
    NSLocalNetworkUsageDescription="MotionDuo discovers and connects to your computers on the local network.",
)
(bundle / "Contents/Info.plist").write_bytes(plistlib.dumps(info))
PY
# Both binaries may link only system frameworks/libraries. No GTK/Homebrew
# runtime is shipped; fail rather than quietly packaging an incomplete closure.
for binary in "$bundle"/Contents/MacOS/*; do
    otool -L "$binary"
    if otool -L "$binary" | awk 'NR > 1 {print $1}' | grep -Ev '^(/System/Library/|/usr/lib/)' | grep -q .; then
        echo "Non-system dependency in $binary; audit before packaging" >&2
        exit 1
    fi
    sign "$binary"
done
sign "$bundle"
codesign --verify --deep --strict "$bundle"
"$bundle/Contents/MacOS/motionduo" --version
"$bundle/Contents/MacOS/motionduo-ui" --version
printf 'Staged %s (minimum OS must also be checked against both Mach-O load commands)\n' "$bundle"
