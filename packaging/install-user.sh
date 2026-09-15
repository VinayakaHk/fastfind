#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$HOME/.local/lib/fastfind"
BIN_DIR="$HOME/.local/bin"
DESKTOP_DIR="$DATA_HOME/applications"
ICON_DIR="$DATA_HOME/icons/hicolor/scalable/apps"
METAINFO_DIR="$DATA_HOME/metainfo"

if [[ -x "$ROOT/dev" && -x "$ROOT/.toolchain/root/usr/bin/cargo" ]]; then
  "$ROOT/dev" cargo build --release --locked --bins
else
  cargo build --manifest-path "$ROOT/Cargo.toml" --release --locked --bins
fi

install -d "$APP_DIR" "$BIN_DIR" "$DESKTOP_DIR" "$ICON_DIR" "$METAINFO_DIR"
install -m 0755 "$ROOT/target/release/fastfind" "$APP_DIR/fastfind"
install -m 0755 "$ROOT/target/release/fastfind-gui" "$APP_DIR/fastfind-gui"
install -m 0644 "$ROOT/README.md" "$APP_DIR/README.md"
install -m 0644 "$ROOT/packaging/fastfind.svg" "$ICON_DIR/fastfind.svg"
install -m 0644 "$ROOT/packaging/io.github.vinayakahk.FastFind.metainfo.xml" \
  "$METAINFO_DIR/io.github.vinayakahk.FastFind.metainfo.xml"
ln -sfn "../lib/fastfind/fastfind" "$BIN_DIR/fastfind"
ln -sfn "../lib/fastfind/fastfind-gui" "$BIN_DIR/fastfind-gui"

escaped_exec=$(printf '%s' "\"$APP_DIR/fastfind-gui\"" | sed 's/[&|]/\\&/g')
escaped_tryexec=$(printf '%s' "$APP_DIR/fastfind-gui" | sed 's/[&|]/\\&/g')
sed \
  -e "s|@FASTFIND_GUI@|$escaped_exec|g" \
  -e "s|@FASTFIND_GUI_PATH@|$escaped_tryexec|g" \
  "$ROOT/packaging/fastfind.desktop.in" > "$DESKTOP_DIR/fastfind.desktop"
chmod 0644 "$DESKTOP_DIR/fastfind.desktop"

desktop-file-validate "$DESKTOP_DIR/fastfind.desktop"
update-desktop-database "$DESKTOP_DIR"
if command -v xdg-desktop-menu >/dev/null 2>&1; then
  xdg-desktop-menu forceupdate
fi
if [[ -f "$DATA_HOME/icons/hicolor/index.theme" ]]; then
  gtk-update-icon-cache -q -f -t "$DATA_HOME/icons/hicolor" || true
fi

printf 'FastFind installed successfully.\n'
printf 'Start menu entry: FastFind\n'
printf 'Application: %s\n' "$APP_DIR/fastfind-gui"
printf 'CLI: %s\n' "$BIN_DIR/fastfind"
printf 'Uninstall with: %s/packaging/uninstall-user.sh\n' "$ROOT"
