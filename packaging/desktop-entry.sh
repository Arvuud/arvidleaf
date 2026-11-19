#!/bin/sh
# Prints arvidleaf's .desktop entry (app menu, file associations, window icon).
#   desktop-entry.sh <path to the binary> <version>
# Wayland shows the window icon only if the file name (arvidleaf.desktop)
# matches the app id the program sets.
set -eu
exec_path=${1:?binary path}
version=${2:?version}
cat <<DESKTOP
[Desktop Entry]
Type=Application
Version=1.5
Name=arvidleaf
GenericName=LaTeX Editor
Comment=Local LaTeX and Markdown editor with live PDF preview
Keywords=LaTeX;TeX;Markdown;PDF;Thesis;Editor;
Exec="$exec_path" %F
TryExec=$exec_path
Icon=arvidleaf
Terminal=false
StartupNotify=true
StartupWMClass=arvidleaf
Categories=Office;Publishing;TextEditor;
MimeType=text/x-tex;application/x-tex;text/x-bibtex;text/markdown;
X-Arvidleaf-Version=$version
DESKTOP
