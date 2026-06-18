#!/usr/bin/env bash
# Build a self-contained AppImage of the GTK4 meusic binary.
#
# Bundles GTK4 + libadwaita via linuxdeploy-plugin-gtk (DEPLOY_GTK_VERSION=4).
#
# Fedora's gdk-pixbuf delegates ALL image decoding (incl. our SVG logos/icons
# AND PNG/JPEG cover art) to *glycin*, which runs out-of-process loaders
# (glycin-svg, glycin-image-rs). So we also bundle those loader binaries (their
# shared-lib deps come along via linuxdeploy -e), ship their .conf files, and a
# runtime AppRun hook (apprun-hooks/zz-glycin.sh) that points glycin at the
# bundled loaders and forces no-sandbox so the loaders resolve bundled libs via
# their patched RPATH ($ORIGIN/../lib) instead of the (absent on other distros)
# host /usr.
#
# FUSE is often unavailable in CI/containers, so everything runs through
# APPIMAGE_EXTRACT_AND_RUN=1. Run AFTER `cargo build --release`.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # meusic-gtk/
TOOLS="${TOOLS:-$HOME/appimage-tools}"
APPDIR="$HERE/target/appimage/AppDir"
VERSION="$(grep -m1 '^version' "$HERE/Cargo.toml" | cut -d'"' -f2)"

GLYCIN_LIBEXEC="/usr/libexec/glycin-loaders/2+"
GLYCIN_CONFD="/usr/share/glycin-loaders/2+/conf.d"

export APPIMAGE_EXTRACT_AND_RUN=1
export NO_STRIP=1
export DEPLOY_GTK_VERSION=4
export PATH="$TOOLS:$PATH"            # so linuxdeploy finds linuxdeploy-plugin-gtk
export LINUXDEPLOY_OUTPUT_VERSION="$VERSION"

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/scalable/apps"

cp "$HERE/packaging/com.sarta.meusic.gtk.desktop"  "$APPDIR/usr/share/applications/"
cp "$HERE/icons/com.sarta.meusic.gtk.svg"          "$APPDIR/usr/share/icons/hicolor/scalable/apps/"

# ---- 1. Populate the AppDir (main binary + glycin loaders + GTK) -------------
# -e on the glycin loaders pulls their shared-lib deps (libfontconfig, etc.) and
# patches their RPATH to $ORIGIN/../lib. They land in usr/bin/.
cd "$HERE/target/appimage"
"$TOOLS/linuxdeploy.AppImage" \
    --appdir "$APPDIR" \
    -e "$HERE/target/release/meusic" \
    -e "$GLYCIN_LIBEXEC/glycin-svg" \
    -e "$GLYCIN_LIBEXEC/glycin-image-rs" \
    -d "$APPDIR/usr/share/applications/com.sarta.meusic.gtk.desktop" \
    -i "$APPDIR/usr/share/icons/hicolor/scalable/apps/com.sarta.meusic.gtk.svg" \
    --plugin gtk

# ---- 2. Ship the glycin loader configs (Exec= rewritten at runtime) ----------
mkdir -p "$APPDIR/usr/share/glycin-loaders/2+/conf.d"
cp "$GLYCIN_CONFD/glycin-svg.conf"      "$APPDIR/usr/share/glycin-loaders/2+/conf.d/"
cp "$GLYCIN_CONFD/glycin-image-rs.conf" "$APPDIR/usr/share/glycin-loaders/2+/conf.d/"

# ---- 3. Runtime hook: aim glycin at the bundled loaders, no sandbox ----------
# linuxdeploy's AppRun only sources the gtk plugin hook BY NAME (it does not glob
# apprun-hooks/*.sh), so append our glycin setup to the end of that hook — which
# also means LD_LIBRARY_PATH=$APPDIR/usr/lib is already exported for the loaders.
cat >> "$APPDIR/apprun-hooks/linuxdeploy-plugin-gtk.sh" <<'HOOK'
# Make Fedora's glycin image backend use the loaders bundled in this AppImage.
_gly="${XDG_RUNTIME_DIR:-/tmp}/meusic-glycin.$$"
rm -rf "$_gly"; mkdir -p "$_gly/glycin-loaders/2+/conf.d" "$_gly/bin"
for _c in "$APPDIR/usr/share/glycin-loaders/2+/conf.d/"*.conf; do
    sed "s|Exec=/usr/libexec/glycin-loaders/2+/|Exec=$APPDIR/usr/bin/|g" \
        "$_c" > "$_gly/glycin-loaders/2+/conf.d/$(basename "$_c")"
done
export GLYCIN_DATA_DIR="$_gly"
# Force glycin's no-sandbox fallback: a bwrap that fails the namespace probe, so
# loaders run as direct children and resolve bundled libs via their RPATH. (The
# system packages keep the real bwrap sandbox; only this portable build opts out.)
printf '#!/bin/sh\nexit 1\n' > "$_gly/bin/bwrap"
chmod +x "$_gly/bin/bwrap"
export PATH="$_gly/bin:$PATH"
HOOK

# ---- 4. Package -------------------------------------------------------------
"$TOOLS/appimagetool.AppImage" "$APPDIR" \
    "$HERE/target/appimage/meusic-${VERSION}-x86_64.AppImage"

echo "=== AppImage built ==="
ls -la "$HERE/target/appimage/"*.AppImage
