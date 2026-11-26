#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
toolchain="$root/.toolchain/hd-lang"
hd="$toolchain/target/release/hd"

bundle=false
if [ "${1-}" = --bundle ]; then
  bundle=true
  shift
  if [ "$(uname -s)" != Darwin ]; then
    printf '%s\n' '--bundle requires macOS.' >&2
    exit 2
  fi
fi

case "$#" in
  0)
    HDRU_NSH_PACKAGE="$root/examples/counter"
    HDRU_NSH_EXECUTABLE=counter-nsh
    ;;
  2)
    HDRU_NSH_PACKAGE=$(CDPATH= cd -- "$1" && pwd)
    HDRU_NSH_EXECUTABLE=$2
    ;;
  *)
    printf 'Usage: %s [--bundle] [PACKAGE_DIR EXECUTABLE]\n' "$0" >&2
    exit 2
    ;;
esac
case "$HDRU_NSH_EXECUTABLE" in
  ""|*/*)
    printf 'Expected an hd executable name.\n' >&2
    exit 2
    ;;
esac
export HDRU_NSH_PACKAGE HDRU_NSH_EXECUTABLE

"$root/scripts/setup-toolchain.sh" --features wasmtime
(cd "$HDRU_NSH_PACKAGE" && "$hd" build --engine wasmtime)
if [ ! -f "$HDRU_NSH_PACKAGE/build/debug/$HDRU_NSH_EXECUTABLE.host.rs" ]; then
  printf 'Build did not produce native host bindings for %s.\n' "$HDRU_NSH_EXECUTABLE" >&2
  exit 1
fi
rust_toolchain=$(cd "$toolchain" && rustup show active-toolchain)
rust_toolchain=${rust_toolchain%% *}
cargo_manifest="$root/ui-nsh/native/Cargo.toml"
cd "$HDRU_NSH_PACKAGE"
if [ "$bundle" = false ]; then
  exec rustup run "$rust_toolchain" cargo run --quiet --locked --manifest-path "$cargo_manifest"
fi

target=$(rustup run "$rust_toolchain" rustc --print host-tuple)
target_dir="$root/ui-nsh/native/target"
rustup run "$rust_toolchain" cargo build --locked --manifest-path "$cargo_manifest" \
  --target "$target" --target-dir "$target_dir"

app="$HDRU_NSH_PACKAGE/build/$HDRU_NSH_EXECUTABLE.app"
mkdir -p "$app/Contents/MacOS"
cp "$target_dir/$target/debug/hdru-nsh" "$app/Contents/MacOS/hdru-nsh"
plist="$app/Contents/Info.plist"
cat > "$plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>hdru-nsh</string>
  <key>CFBundleIdentifier</key>
  <string>org.hdru.NativeShell</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST
plutil -insert CFBundleName -string "$HDRU_NSH_EXECUTABLE" "$plist"
codesign --force --sign - "$app"
printf '%s\n' "$app"
