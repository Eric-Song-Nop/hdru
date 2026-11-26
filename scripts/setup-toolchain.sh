#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)
revision=$(cat scripts/hd-revision)
src="$root/.toolchain/hd-lang"
if [ ! -d "$src/.git" ] || [ "$(git -C "$src" rev-parse HEAD)" != "$revision" ]; then
  mkdir -p .toolchain
  tmp=$(mktemp -d "$root/.toolchain/checkout.XXXXXX")
  trap 'rm -rf "$tmp"' EXIT
  git -C "$tmp" init -q
  git -C "$tmp" fetch --depth 1 https://github.com/HerringtonDarkholme/hd-lang "$revision"
  git -C "$tmp" checkout -q --detach FETCH_HEAD
  git -C "$tmp" apply --index "$root/scripts/playground-compiler.patch"
  if [ -e "$src" ]; then
    backup=$(mktemp -d "$root/.toolchain/previous.XXXXXX")
    mv "$src" "$backup/hd-lang"
    printf 'Previous toolchain saved in %s\n' "$backup/hd-lang"
  fi
  mv "$tmp" "$src"
fi
test "$(git -C "$src" rev-parse HEAD)" = "$revision"
git -C "$src" diff --quiet
git -C "$src" diff --cached --binary --full-index | cmp - "$root/scripts/playground-compiler.patch"
(cd "$src" && cargo build --locked --release -p hd_command)
