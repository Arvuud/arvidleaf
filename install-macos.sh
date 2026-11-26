#!/bin/sh
# Installs arvidleaf on macOS (12 or newer, Apple silicon or Intel):
#   1. tools via Homebrew: MacTeX without GUI apps (TeX Live incl. latexmk, biber, synctex,
#      German), poppler (PDF preview); Rust via rustup
#   2. release build
#   3. arvidleaf.app in ~/Applications (Launchpad, Spotlight, Dock) and the command
#      `arvidleaf` in ~/.local/bin
#
#   ./install-macos.sh               everything
#   ./install-macos.sh --update      rebuild and reinstall the app only
#   ./install-macos.sh --no-deps     skip Homebrew
#   ./install-macos.sh --small-tex   BasicTeX (~100 MB) plus the needed packages instead of MacTeX (~5 GB)
#   ./install-macos.sh --system      app into /Applications (for all users; asks for the password)
#   ./install-macos.sh --uninstall   remove app and command (the tools stay)
set -eu
root=$(cd "$(dirname "$0")" && pwd)
cd "$root"

MIN_RUST=1.95
deps=1
small=0
apps="$HOME/Applications"
for arg in "$@"; do
    case "$arg" in
        --update|--no-deps) deps=0 ;;
        --small-tex) small=1 ;;
        --system) apps=/Applications ;;
        --uninstall)
            for a in "$HOME/Applications/arvidleaf.app" /Applications/arvidleaf.app; do
                if [ -d "$a" ]; then
                    case "$a" in /Applications/*) sudo rm -rf "$a" ;; *) rm -rf "$a" ;; esac
                    echo "removed $a"
                fi
            done
            rm -f "$HOME/.local/bin/arvidleaf"
            exit 0
            ;;
        -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown option: $arg (see --help)" >&2; exit 2 ;;
    esac
done
[ "$(uname -s)" = Darwin ] || { echo "This script is for macOS; on Linux use ./install.sh" >&2; exit 1; }

step() { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m%s\033[0m\n' "$*" >&2; }
have() { command -v "$1" >/dev/null 2>&1; }

# tools land here; apps started from Finder do not see the shell's PATH
for d in /opt/homebrew/bin /usr/local/bin /Library/TeX/texbin "$HOME/.cargo/bin"; do
    [ -d "$d" ] && PATH="$d:$PATH"
done
export PATH

rust_ok() {
    have rustc || return 1
    v=$(rustc --version | sed 's/^rustc \([0-9]*\.[0-9]*\).*/\1/')
    [ "$(printf '%s\n%s\n' "$MIN_RUST" "$v" | sort -t. -k1,1n -k2,2n | head -n1)" = "$MIN_RUST" ]
}

if [ "$deps" = 1 ]; then
    if ! xcode-select -p >/dev/null 2>&1; then
        step "Installing the Xcode command line tools (linker)"
        xcode-select --install || true
        warn "Finish the installation dialog, then run this script again."
        exit 1
    fi
    if ! have brew; then
        step "Installing Homebrew"
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
        for d in /opt/homebrew/bin /usr/local/bin; do [ -x "$d/brew" ] && PATH="$d:$PATH"; done
    fi
    step "Installing LaTeX and poppler"
    brew install poppler
    if ! have latexmk; then
        if [ "$small" = 1 ]; then
            brew install --cask basictex
            PATH="/Library/TeX/texbin:$PATH"
            sudo tlmgr update --self
            sudo tlmgr install latexmk biber biblatex babel-german hyphen-german collection-latexrecommended \
                collection-fontsrecommended enumitem booktabs siunitx csquotes microtype pgf xcolor hyperref
        else
            brew install --cask mactex-no-gui
            PATH="/Library/TeX/texbin:$PATH"
        fi
    fi
    if ! rust_ok; then
        step "Installing Rust $MIN_RUST or newer"
        if have rustup; then
            rustup toolchain install stable --profile minimal
            rustup default stable
        else
            # the official installer; everything goes to ~/.cargo and ~/.rustup
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --no-modify-path
        fi
        PATH="$HOME/.cargo/bin:$PATH"
    fi
fi

if ! rust_ok; then
    warn "Rust $MIN_RUST or newer is needed (found: $(rustc --version 2>/dev/null || echo none)). Run without --update/--no-deps, or: rustup update stable"
    exit 1
fi

step "Building arvidleaf (release)"
cargo build --release -p arvidleaf
version=$(./target/release/arvidleaf --version | sed 's/^arvidleaf //')

step "Creating arvidleaf.app"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
app="$tmp/arvidleaf.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/arvidleaf "$app/Contents/MacOS/arvidleaf"
# icon: every size the icon set wants, from the 256 px PNG
set_dir="$tmp/arvidleaf.iconset"
mkdir -p "$set_dir"
for s in 16 32 128 256 512; do
    sips -z "$s" "$s" icons/arvidleaf-256.png --out "$set_dir/icon_${s}x${s}.png" >/dev/null
    d=$((s * 2))
    sips -z "$d" "$d" icons/arvidleaf-256.png --out "$set_dir/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$set_dir" -o "$app/Contents/Resources/arvidleaf.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>arvidleaf</string>
    <key>CFBundleDisplayName</key><string>arvidleaf</string>
    <key>CFBundleIdentifier</key><string>io.github.arvuud.arvidleaf</string>
    <key>CFBundleExecutable</key><string>arvidleaf</string>
    <key>CFBundleIconFile</key><string>arvidleaf</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>LaTeX / Markdown document</string>
            <key>CFBundleTypeRole</key><string>Editor</string>
            <key>LSHandlerRank</key><string>Alternate</string>
            <key>CFBundleTypeExtensions</key><array><string>tex</string><string>bib</string><string>md</string></array>
        </dict>
    </array>
</dict>
</plist>
PLIST
# the self-built app is not notarized: sign it locally so Gatekeeper lets it start
codesign --force --deep --sign - "$app" 2>/dev/null || true

step "Installing to $apps"
if pgrep -x arvidleaf >/dev/null 2>&1; then
    warn "arvidleaf is running - quit it first, then run the script again."
    exit 1
fi
if [ "$apps" = /Applications ]; then
    sudo rm -rf /Applications/arvidleaf.app
    sudo cp -R "$app" /Applications/
else
    mkdir -p "$apps"
    rm -rf "$apps/arvidleaf.app"
    cp -R "$app" "$apps/"
fi
xattr -dr com.apple.quarantine "$apps/arvidleaf.app" 2>/dev/null || true
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$apps/arvidleaf.app" 2>/dev/null || true
mkdir -p "$HOME/.local/bin"
ln -sf "$apps/arvidleaf.app/Contents/MacOS/arvidleaf" "$HOME/.local/bin/arvidleaf"

step "Checking the setup"
missing=0
for t in latexmk pdflatex pdftoppm pdfinfo synctex biber; do
    if have "$t"; then echo "  ok       $t"; else echo "  missing  $t"; missing=1; fi
done
# optional: reference library (pdftotext comes with poppler, curl and sqlite3 with macOS)
for t in pdftotext curl sqlite3; do
    if have "$t"; then echo "  ok       $t"; else echo "  optional $t (reference library)"; fi
done
[ "$missing" = 0 ] || warn "Some tools are missing - run without --no-deps, or see Help > System Check in the app."
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo "Note: add ~/.local/bin to PATH for the arvidleaf command." ;; esac
echo
echo "arvidleaf $version: start it from Launchpad/Spotlight or with: open -a arvidleaf"
