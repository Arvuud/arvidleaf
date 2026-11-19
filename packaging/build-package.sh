#!/bin/sh
# Builds the installable archive dist/arvidleaf-<version>-linux-<arch>.tar.gz.
#
#   packaging/build-package.sh            # release build, then package
#   packaging/build-package.sh --no-build # package the existing target/release/arvidleaf
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
arch=$(uname -m)
name="arvidleaf-$version-linux-$arch"
stage="dist/$name"

if [ "${1:-}" != "--no-build" ]; then
    cargo build --release -p arvidleaf
fi
bin=target/release/arvidleaf
[ -x "$bin" ] || { echo "missing $bin (run without --no-build)" >&2; exit 1; }
built=$("$bin" --version)
[ "$built" = "arvidleaf $version" ] || { echo "binary reports '$built', expected 'arvidleaf $version': rebuild" >&2; exit 1; }

rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/share/icons"
install -m755 "$bin" "$stage/bin/arvidleaf"
strip "$stage/bin/arvidleaf" 2>/dev/null || true
install -m644 icons/arvidleaf-icon.svg "$stage/share/icons/arvidleaf.svg"
install -m644 icons/arvidleaf-256.png "$stage/share/icons/arvidleaf-256.png"
install -m755 packaging/install.sh "$stage/install.sh"
install -m755 packaging/uninstall.sh "$stage/uninstall.sh"
install -m755 packaging/desktop-entry.sh "$stage/desktop-entry.sh"
install -m644 LICENSE-MIT LICENSE-APACHE README.md CHANGELOG.md "$stage/"
echo "$version" > "$stage/VERSION"

tar -C dist -czf "dist/$name.tar.gz" "$name"
(cd dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
echo "dist/$name.tar.gz"
