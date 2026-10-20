#!/bin/sh
# Installs APOST3Dview for the current user (no root needed): the program
# in ~/.local/bin, its icon and its entry in the applications menu.
set -e
cd "$(dirname "$0")"
data=${XDG_DATA_HOME:-$HOME/.local/share}
mkdir -p "$HOME/.local/bin" "$data/applications" "$data/icons/hicolor/512x512/apps"
cp apost3dview "$HOME/.local/bin/"
cp apost3dview.png "$data/icons/hicolor/512x512/apps/"
cp apost3dview.desktop "$data/applications/"
command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
echo "Installed. Start APOST3Dview from the applications menu, or run: apost3dview"
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo "(Add ~/.local/bin to your PATH to run it from a terminal.)" ;; esac
