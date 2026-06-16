#!/usr/bin/env bash
# Install the meusic icon + .desktop entry into the per-user freedesktop dirs so
# GNOME/KDE show the app logo in the dock/overview and the GNOME media popup.
#
# Why this is needed in dev: a `pnpm tauri dev` run is just a raw binary with no
# installed desktop entry, so the shell falls back to a generic icon. The window
# reports the Wayland app_id "meusic" (GTK uses the program name — tao doesn't
# create a GtkApplication), so the entry MUST be named `meusic.desktop` to match.
# `pnpm tauri build` installs an equivalent entry; this is the dev-time stand-in.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="meusic"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
BIN="${MEUSIC_BIN:-$PROJECT_DIR/src-tauri/target/release/$APP_ID}"
[ -x "$BIN" ] || BIN="$PROJECT_DIR/src-tauri/target/debug/$APP_ID"

install -Dm644 "$PROJECT_DIR/assets/logo/logo_icon-color.svg" "$ICONS/scalable/apps/$APP_ID.svg"
install -Dm644 "$PROJECT_DIR/src-tauri/icons/32x32.png"        "$ICONS/32x32/apps/$APP_ID.png"
install -Dm644 "$PROJECT_DIR/src-tauri/icons/64x64.png"        "$ICONS/64x64/apps/$APP_ID.png"
install -Dm644 "$PROJECT_DIR/src-tauri/icons/128x128.png"      "$ICONS/128x128/apps/$APP_ID.png"
install -Dm644 "$PROJECT_DIR/src-tauri/icons/128x128@2x.png"   "$ICONS/256x256/apps/$APP_ID.png"

install -d "$APPS"
cat > "$APPS/$APP_ID.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=meusic
GenericName=Music Player
Comment=Lightweight local music player
Exec=$BIN
Icon=$APP_ID
Terminal=false
Categories=AudioVideo;Audio;Player;
StartupWMClass=$APP_ID
StartupNotify=true
EOF

gtk-update-icon-cache -f -t "$ICONS" 2>/dev/null || true
update-desktop-database "$APPS" 2>/dev/null || true
echo "Installed $APP_ID icon + $APPS/$APP_ID.desktop (Exec=$BIN)"
