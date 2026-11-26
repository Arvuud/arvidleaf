#!/bin/sh
# Installs arvidleaf on Linux with everything it needs:
#   1. system packages: TeX Live (incl. German), latexmk, biber, poppler, C linker,
#      curl and sqlite3 (reference library: DOI lookup, Zotero import)
#   2. Rust (>= 1.95; via rustup if the distribution's is missing or too old)
#   3. release build
#   4. program, icon and app-menu entry (.desktop)
#
#   ./install.sh               everything, for this user (~/.local; sudo only for step 1)
#   ./install.sh --update      rebuild and reinstall, no system packages (refreshes the menu entry)
#   ./install.sh --system      install for all users (/usr/local, asks for sudo)
#   ./install.sh --no-deps     skip the system packages
#   ./install.sh --deps-only   only the system packages and Rust
#   ./install.sh --dev         menu entry that starts this checkout's target/release build
#   ./install.sh --uninstall   remove program, icon and menu entry
#
# Package managers: pacman (Arch, Manjaro), apt (Debian, Ubuntu, Mint), dnf (Fedora),
# zypper (openSUSE). Windows: install.ps1, macOS: install-macos.sh.
set -eu
root=$(cd "$(dirname "$0")" && pwd)
cd "$root"

MIN_RUST=1.95
deps=1
build=1
system=0
dev=0
for arg in "$@"; do
    case "$arg" in
        --no-deps|--update) deps=0 ;;
        --deps-only) build=0 ;;
        --system) system=1 ;;
        --dev) deps=0; dev=1 ;;
        --uninstall)
            for u in "${XDG_DATA_HOME:-$HOME/.local/share}/arvidleaf/uninstall.sh" /usr/local/share/arvidleaf/uninstall.sh; do
                if [ -x "$u" ]; then
                    case "$u" in /usr/*) sudo "$u" ;; *) "$u" ;; esac
                    exit 0
                fi
            done
            echo "arvidleaf is not installed." >&2
            exit 1
            ;;
        -h|--help) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $arg (see --help)" >&2; exit 2 ;;
    esac
done

step() { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m%s\033[0m\n' "$*" >&2; }
have() { command -v "$1" >/dev/null 2>&1; }

[ -x "$HOME/.cargo/bin/cargo" ] && PATH="$HOME/.cargo/bin:$PATH"

# "1.95.0" >= "1.95"?
rust_ok() {
    have rustc || return 1
    v=$(rustc --version | sed 's/^rustc \([0-9]*\.[0-9]*\).*/\1/')
    [ "$(printf '%s\n%s\n' "$MIN_RUST" "$v" | sort -V | head -n1)" = "$MIN_RUST" ]
}

if [ "$deps" = 1 ]; then
    step "Installing system packages"
    if have pacman; then
        sudo pacman -S --needed --noconfirm \
            texlive-basic texlive-bin texlive-binextra texlive-latex texlive-latexrecommended texlive-latexextra \
            texlive-fontsrecommended texlive-fontsextra texlive-pictures texlive-mathscience texlive-publishers \
            texlive-bibtexextra texlive-xetex texlive-luatex texlive-langgerman texlive-langenglish \
            biber poppler gcc curl sqlite
    elif have apt-get; then
        sudo apt-get update
        sudo apt-get install -y \
            texlive-latex-extra texlive-fonts-recommended texlive-fonts-extra texlive-pictures texlive-science \
            texlive-publishers texlive-bibtex-extra texlive-xetex texlive-luatex texlive-lang-german \
            latexmk biber poppler-utils build-essential curl sqlite3
    elif have dnf; then
        sudo dnf install -y \
            texlive-scheme-medium texlive-collection-latexextra texlive-collection-langgerman \
            texlive-collection-bibtexextra texlive-xetex texlive-luatex \
            latexmk biber poppler-utils gcc curl sqlite
    elif have zypper; then
        sudo zypper --non-interactive install \
            texlive-scheme-medium texlive-collection-latexextra texlive-collection-langgerman \
            texlive-latexmk biber poppler-tools gcc curl sqlite3
    else
        warn "No supported package manager found. Install TeX Live, latexmk, biber, poppler and a C compiler by hand."
        exit 1
    fi

    if ! rust_ok; then
        step "Installing Rust $MIN_RUST or newer (rustup, for this user)"
        if have rustup; then
            rustup toolchain install stable --profile minimal
            rustup default stable
        elif have pacman; then
            sudo pacman -S --needed --noconfirm rustup
            rustup toolchain install stable --profile minimal
            rustup default stable
        else
            # the official installer; puts everything into ~/.cargo and ~/.rustup
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --no-modify-path
        fi
        PATH="$HOME/.cargo/bin:$PATH"
    fi
fi

[ "$build" = 1 ] || exit 0

if ! rust_ok; then
    warn "Rust $MIN_RUST or newer is needed (found: $(rustc --version 2>/dev/null || echo none))."
    warn "Run ./install.sh without --no-deps/--update, or: rustup update stable"
    exit 1
fi

step "Building arvidleaf (release)"
if [ "$dev" = 1 ]; then
    cargo build --release -p arvidleaf
    step "Registering this checkout's build in the app menu"
    data=${XDG_DATA_HOME:-$HOME/.local/share}
    if [ -x "$data/arvidleaf/uninstall.sh" ]; then
        warn "Note: this replaces the menu entry of the installed arvidleaf ($data/arvidleaf); ./install.sh --update brings it back."
    fi
    install -Dm644 icons/arvidleaf-icon.svg "$data/icons/hicolor/scalable/apps/arvidleaf.svg"
    install -Dm644 icons/arvidleaf-256.png "$data/icons/hicolor/256x256/apps/arvidleaf.png"
    sh packaging/desktop-entry.sh "$root/target/release/arvidleaf" "dev" > "$data/applications/arvidleaf.desktop.tmp"
    mv "$data/applications/arvidleaf.desktop.tmp" "$data/applications/arvidleaf.desktop"
    update-desktop-database "$data/applications" 2>/dev/null || true
    gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true
    echo "Menu entry: $data/applications/arvidleaf.desktop -> $root/target/release/arvidleaf"
    exit 0
fi
packaging/build-package.sh >/dev/null
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
pkg="dist/arvidleaf-$version-linux-$(uname -m).tar.gz"

step "Installing program, icon and app-menu entry"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
tar -C "$tmp" -xzf "$pkg"
inst=$(find "$tmp" -maxdepth 2 -name install.sh | head -n1)
if [ "$system" = 1 ]; then
    sudo "$inst" --system
    bin=/usr/local/bin/arvidleaf
else
    "$inst"
    bin=$HOME/.local/bin/arvidleaf
fi

step "Checking the setup"
"$bin" --version
missing=0
for t in latexmk pdflatex pdftoppm pdfinfo synctex biber; do
    if have "$t"; then echo "  ok       $t"; else echo "  missing  $t"; missing=1; fi
done
if kpsewhich ngerman.ldf >/dev/null 2>&1; then echo "  ok       German (babel ngerman)"; else echo "  missing  German (babel ngerman)"; fi
# optional: reference library (DOI in PDFs, online lookup, Zotero import)
for t in pdftotext curl sqlite3; do
    if have "$t"; then echo "  ok       $t"; else echo "  optional $t (reference library)"; fi
done
[ "$missing" = 0 ] || warn "Some tools are missing – run ./install.sh without --no-deps, or see Help → System Check in the app."
if pgrep -x arvidleaf >/dev/null 2>&1; then
    warn "arvidleaf is still running – restart it to use the new version."
fi
echo
echo "Start arvidleaf from the app menu or with: arvidleaf"
