#!/bin/sh
# Development: puts this checkout's release build into the app menu.
# Same as ./install.sh --dev (kept for old habits).
exec "$(cd "$(dirname "$0")" && pwd)/install.sh" --dev "$@"
