#!/usr/bin/env bash
set -euo pipefail

DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"

rm -f "$DATA_HOME/applications/fastfind.desktop"
rm -f "$DATA_HOME/icons/hicolor/scalable/apps/fastfind.svg"
rm -f "$DATA_HOME/metainfo/io.github.vinayakahk.FastFind.metainfo.xml"
rm -f "$HOME/.local/bin/fastfind" "$HOME/.local/bin/fastfind-gui"
rm -rf "$HOME/.local/lib/fastfind"

update-desktop-database "$DATA_HOME/applications" 2>/dev/null || true
if command -v xdg-desktop-menu >/dev/null 2>&1; then
  xdg-desktop-menu forceupdate 2>/dev/null || true
fi

printf 'FastFind application files were removed.\n'
printf 'The index at %s was preserved.\n' "$DATA_HOME/fastfind/index.db"
