#!/bin/sh
# Builds the examples and bundles the browser pages into web/dist/.
# Needs the hd binary on PATH and pnpm (for esbuild).
set -e
here="$(cd "$(dirname "$0")" && pwd)"
PATH="$here/../.toolchain/hd-lang/target/release:$PATH"
export PATH
test -x "$here/../.toolchain/hd-lang/target/release/hd"
mkdir -p "$here/dist"
for app in counter todos fetcher editor; do
  (cd "$here/../examples/$app" && hd build)
  cp "$here/../examples/$app/build/debug/$app.wasm" "$here/dist/$app.wasm"
done
pnpm exec esbuild "$here/main.js" --bundle --format=esm --platform=browser \
  --target=es2022 --loader:.hd=text --outfile="$here/dist/main.js" --log-level=warning
echo "built $here/dist"
