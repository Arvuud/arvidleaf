#!/bin/sh
# Removes an arvidleaf installation: every file install.sh recorded in files.txt.
#   uninstall.sh [--quiet]
set -eu
own=$(cd "$(dirname "$0")" && pwd)
list="$own/files.txt"
[ -f "$list" ] || { echo "no installation found in $own" >&2; exit 1; }
data=$(dirname "$own")
version=$(cat "$own/VERSION" 2>/dev/null || echo "?")

while IFS= read -r f; do
    [ -n "$f" ] && rm -f "$f"
done < "$list"
rm -f "$list"
rmdir "$own" 2>/dev/null || true

update-desktop-database "$data/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true
[ "${1:-}" = "--quiet" ] || echo "arvidleaf $version removed."
