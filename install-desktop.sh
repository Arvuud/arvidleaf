#!/bin/sh
# Registers arvidleaf with the desktop (app menu + window/taskbar icon).
# Wayland shows an app's icon only via a .desktop file matching its app id.
set -e
root=$(cd "$(dirname "$0")" && pwd)
data=${XDG_DATA_HOME:-$HOME/.local/share}
install -Dm644 "$root/icons/arvidleaf-icon.svg" "$data/icons/hicolor/scalable/apps/arvidleaf.svg"
install -Dm644 "$root/icons/arvidleaf-256.png" "$data/icons/hicolor/256x256/apps/arvidleaf.png"
install -Dm644 /dev/stdin "$data/applications/arvidleaf.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=arvidleaf
Comment=Local LaTeX editor with PDF preview
Exec=$root/target/release/arvidleaf %F
Icon=arvidleaf
Terminal=false
Categories=Office;Development;Publishing;
MimeType=text/x-tex;application/x-tex;
StartupWMClass=arvidleaf
DESKTOP
update-desktop-database "$data/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true
echo "Installed: $data/applications/arvidleaf.desktop"
