#!/bin/sh
# Installs arvidleaf from an unpacked package.
#
#   ./install.sh                 # for this user: ~/.local (no root needed)
#   sudo ./install.sh --system   # for all users: /usr/local
#   ./install.sh --prefix DIR    # any prefix (bin/, share/ below DIR)
#
# Installing again replaces an earlier version. Removal: share/arvidleaf/uninstall.sh
set -eu
src=$(cd "$(dirname "$0")" && pwd)
prefix="$HOME/.local"
system=0
while [ $# -gt 0 ]; do
    case "$1" in
        --system) prefix=/usr/local; system=1 ;;
        --prefix) shift; prefix=${1:?--prefix needs a directory} ;;
        -h|--help) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
    shift
done
if [ "$system" = 1 ] && [ "$(id -u)" != 0 ]; then
    echo "--system needs root: sudo $0 --system" >&2
    exit 1
fi

version=$(cat "$src/VERSION")
data="$prefix/share"
own="$data/arvidleaf"
list="$own/files.txt"

# an earlier installation is removed first, so no stale files remain
if [ -x "$own/uninstall.sh" ]; then
    "$own/uninstall.sh" --quiet
fi

mkdir -p "$own"
: > "$list"
put() { # mode source target
    install -Dm"$1" "$2" "$3"
    echo "$3" >> "$list"
}
put 755 "$src/bin/arvidleaf" "$prefix/bin/arvidleaf"
put 644 "$src/share/icons/arvidleaf.svg" "$data/icons/hicolor/scalable/apps/arvidleaf.svg"
put 644 "$src/share/icons/arvidleaf-256.png" "$data/icons/hicolor/256x256/apps/arvidleaf.png"
for f in LICENSE-MIT LICENSE-APACHE README.md CHANGELOG.md VERSION; do
    put 644 "$src/$f" "$own/$f"
done
put 755 "$src/uninstall.sh" "$own/uninstall.sh"

desktop="$data/applications/arvidleaf.desktop"
mkdir -p "$(dirname "$desktop")"
cat > "$desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=arvidleaf
Comment=Local LaTeX editor with PDF preview
Exec=$prefix/bin/arvidleaf %F
Icon=arvidleaf
Terminal=false
Categories=Office;Publishing;
MimeType=text/x-tex;application/x-tex;
StartupWMClass=arvidleaf
Version=1.5
X-Arvidleaf-Version=$version
DESKTOP
echo "$desktop" >> "$list"

update-desktop-database "$data/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true

echo "arvidleaf $version installed to $prefix"
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) echo "Note: $prefix/bin is not in PATH; the app menu entry works anyway." ;;
esac
echo "Uninstall: $own/uninstall.sh"
