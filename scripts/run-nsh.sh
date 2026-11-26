#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
toolchain="$root/.toolchain/hd-lang"
hd="$toolchain/target/release/hd"

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
    printf 'Usage: %s [PACKAGE_DIR EXECUTABLE]\n' "$0" >&2
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
cd "$HDRU_NSH_PACKAGE"
exec rustup run "${rust_toolchain%% *}" cargo run --quiet --locked --manifest-path "$root/ui-nsh/native/Cargo.toml"
