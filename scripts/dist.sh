#!/usr/bin/env bash
# Builds the release files for one target in dist/:
#   lazybw-VERSION-TARGET.tar.gz           static binary for any Linux (and Termux)
#   lazybw_VERSION-1_ARCH.deb              Debian, Ubuntu
#   lazybw-VERSION-1.ARCH.rpm              Fedora, openSUSE, RHEL
#   lazybw-VERSION-1-ARCH.pkg.tar.zst      Arch Linux
#
#   scripts/dist.sh x86_64-unknown-linux-musl
#   scripts/dist.sh aarch64-unknown-linux-musl --package-only
#
# --package-only skips the build and packages target/TARGET/release/lazybw,
# for a binary built on another machine. Requires cargo-deb,
# cargo-generate-rpm, and makepkg or Docker for the Arch package.
set -euo pipefail
cd "$(dirname "$0")/.."

target=${1:?usage: scripts/dist.sh TARGET [--package-only]}
version=$(cargo metadata --no-deps --format-version 1 |
  python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "lazybw"))')
arch=${target%%-*}

if [ "${2:-}" != "--package-only" ]; then
  cargo build --release --locked --target "$target" -p lazybw
fi
binary="target/$target/release/lazybw"
[ -x "$binary" ] || { echo "missing $binary" >&2; exit 1; }

mkdir -p dist
package="lazybw-$version-$target"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
mkdir "$staging/$package"
cp "$binary" LICENSE README.md CHANGELOG.md "$staging/$package/"
tar -C "$staging" -czf "dist/$package.tar.gz" --owner=0 --group=0 --mode=go-w "$package"

# The binary is already stripped by the release profile; the host strip may
# not understand a foreign architecture.
cargo deb -p lazybw --no-build --no-strip --target "$target" --output dist/
cargo generate-rpm -p crates/lazybw --target "$target" --output dist/
scripts/arch-package.sh "$version" "$arch"

ls -l dist
