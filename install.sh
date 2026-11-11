#!/bin/sh
# Installs arvidleaf with everything it needs:
#   1. system packages: TeX Live (incl. German), latexmk, biber, poppler, Rust
#   2. release build
#   3. program, icon and app-menu entry (.desktop) for this user (~/.local)
#
#   ./install.sh               all three steps (asks for the sudo password in step 1)
#   ./install.sh --no-deps     skip the system packages
#   ./install.sh --deps-only   only the system packages
#   ./install.sh --uninstall   remove program, icon and menu entry
#
# Supported package managers: pacman (Arch), apt (Debian/Ubuntu), dnf (Fedora).
set -eu
root=$(cd "$(dirname "$0")" && pwd)
cd "$root"

deps=1
build=1
case "${1:-}" in
    --no-deps) deps=0 ;;
    --deps-only) build=0 ;;
    --uninstall)
        u="${XDG_DATA_HOME:-$HOME/.local/share}/arvidleaf/uninstall.sh"
        [ -x "$u" ] && exec "$u"
        echo "arvidleaf is not installed." >&2
        exit 1
        ;;
    -h|--help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    "") ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
esac

step() { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }

if [ "$deps" = 1 ]; then
    step "Installing system packages"
    if command -v pacman >/dev/null; then
        pkgs="texlive-basic texlive-bin texlive-binextra texlive-latex texlive-latexrecommended texlive-latexextra
              texlive-fontsrecommended texlive-fontsextra texlive-pictures texlive-mathscience texlive-publishers
              texlive-bibtexextra texlive-xetex texlive-luatex texlive-langgerman texlive-langenglish
              biber poppler"
        command -v cargo >/dev/null || [ -x "$HOME/.cargo/bin/cargo" ] || pkgs="$pkgs rust"
        # shellcheck disable=SC2086
        sudo pacman -S --needed --noconfirm $pkgs
    elif command -v apt-get >/dev/null; then
        pkgs="texlive-latex-extra texlive-fonts-recommended texlive-fonts-extra texlive-pictures texlive-science
              texlive-publishers texlive-bibtex-extra texlive-xetex texlive-luatex texlive-lang-german
              latexmk biber poppler-utils"
        command -v cargo >/dev/null || [ -x "$HOME/.cargo/bin/cargo" ] || pkgs="$pkgs cargo"
        sudo apt-get update
        # shellcheck disable=SC2086
        sudo apt-get install -y $pkgs
    elif command -v dnf >/dev/null; then
        pkgs="texlive-scheme-medium texlive-collection-latexextra texlive-collection-langgerman latexmk biber poppler-utils"
        command -v cargo >/dev/null || [ -x "$HOME/.cargo/bin/cargo" ] || pkgs="$pkgs cargo"
        # shellcheck disable=SC2086
        sudo dnf install -y $pkgs
    else
        echo "No supported package manager found. Install TeX Live, latexmk, biber, poppler and Rust by hand." >&2
        exit 1
    fi
fi

[ "$build" = 1 ] || exit 0

step "Building arvidleaf (release)"
[ -x "$HOME/.cargo/bin/cargo" ] && PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null || { echo "cargo not found – run without --no-deps or install Rust" >&2; exit 1; }
packaging/build-package.sh >/dev/null
pkg=$(ls -t dist/arvidleaf-*-linux-*.tar.gz | head -n1)

step "Installing program, icon and app-menu entry"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
tar -C "$tmp" -xzf "$pkg"
"$tmp"/arvidleaf-*/install.sh

step "Checking the setup"
"$HOME/.local/bin/arvidleaf" --version
for t in latexmk pdflatex pdftoppm pdfinfo synctex biber; do
    if command -v "$t" >/dev/null; then echo "  ok       $t"; else echo "  missing  $t"; fi
done
if kpsewhich ngerman.ldf >/dev/null 2>&1; then echo "  ok       German (babel ngerman)"; else echo "  missing  German (babel ngerman)"; fi
echo
echo "Start arvidleaf from the app menu or with: arvidleaf"
