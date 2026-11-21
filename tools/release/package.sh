#!/usr/bin/env bash
# Builds `lks`, `ferry` and `lks-lsp` for one target and packs them for a release:
# `dist/lakescript-<version>-<target>.tar.gz` (`.zip` on Windows), with
# its `.sha256` next to it, and the pages of the standard library inside it
# in `doc/std/` and next to it as `lakescript-<version>-stdlib.tar.gz`.
#
#   tools/release/package.sh <target> [version]
#
# The version is the one in Cargo.toml when it isn't given *(i.e. `v0.7.0`)*.
# The release workflow runs it on each platform, and it also works by hand
# for the targets the machine can build.
set -euo pipefail

target="${1:?usage: package.sh <target> [version]}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"
version="${2:-v$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"

cargo build --release --locked --target "$target" -p lks -p ferry -p lks-lsp

name="lakescript-$version-$target"
stage="dist/$name"
rm -rf "$stage"
mkdir -p "$stage"
exe=""
case "$target" in
    *windows*) exe=".exe" ;;
esac
for bin in lks ferry lks-lsp; do
    cp "target/$target/release/$bin$exe" "$stage/"
done
cp LICENSE NOTICE "$stage/"

# The pages of the standard library, made by the ferry just built *(see ideas/tooling/docs.md)*
docs_home="$(mktemp -d)"
std_index="$(FERRY_HOME="$docs_home" "target/$target/release/ferry$exe" doc std | tr -d '\r')"
mkdir -p "$stage/doc"
cp -R "$(dirname "$std_index")" "$stage/doc/std"
tar -czf "dist/lakescript-$version-stdlib.tar.gz" -C "$stage/doc" std
rm -rf "$docs_home"

cd dist
case "$target" in
    *windows*)
        archive="$name.zip"
        rm -f "$archive"
        if command -v 7z >/dev/null; then
            7z a -tzip "$archive" "$name" >/dev/null
        else
            zip -qr "$archive" "$name"
        fi
        ;;
    *)
        archive="$name.tar.gz"
        tar -czf "$archive" "$name"
        ;;
esac
if command -v sha256sum >/dev/null; then
    sha256sum "$archive" > "$archive.sha256"
else
    shasum -a 256 "$archive" > "$archive.sha256"
fi
rm -rf "$name"
echo "dist/$archive"
