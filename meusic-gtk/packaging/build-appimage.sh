#!/usr/bin/env bash
# Build a self-contained AppImage of the GTK4 meusic binary.
#
# Bundles GTK4 + libadwaita via linuxdeploy-plugin-gtk (DEPLOY_GTK_VERSION=4),
# plus Fedora's glycin out-of-process image loaders (see step 3).
#
# IMPORTANT — the "de-patchelf" step 2: linuxdeploy rewrites every bundled ELF
# with its vendored patchelf to add an $ORIGIN RUNPATH. That patchelf CORRUPTS
# binaries/libraries that use DT_RELR relative relocations (the default in
# Fedora 43's toolchain), so the app segfaults in _dl_init / a module's _init
# (e.g. the dlopened GTK IM module). We don't need the RUNPATH at all — AppRun
# exports LD_LIBRARY_PATH=$APPDIR/usr/lib — so after linuxdeploy we overwrite
# every bundled ELF (main binary + libs + the dlopened modules in subdirs) with
# the pristine host copy. That neutralises the corruption and the app launches.
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
export PATH="$TOOLS:$PATH"
export LINUXDEPLOY_OUTPUT_VERSION="$VERSION"

rm -rf "$APPDIR" "$HERE/target/appimage/"*.AppImage
mkdir -p "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/scalable/apps"
cp "$HERE/packaging/com.sarta.meusic.gtk.desktop"  "$APPDIR/usr/share/applications/"
cp "$HERE/icons/com.sarta.meusic.gtk.svg"          "$APPDIR/usr/share/icons/hicolor/scalable/apps/"

# ---- 1. Populate the AppDir (main binary + glycin loaders + GTK) -------------
cd "$HERE/target/appimage"
"$TOOLS/linuxdeploy.AppImage" \
    --appdir "$APPDIR" \
    -e "$HERE/target/release/meusic" \
    -e "$GLYCIN_LIBEXEC/glycin-svg" \
    -e "$GLYCIN_LIBEXEC/glycin-image-rs" \
    -d "$APPDIR/usr/share/applications/com.sarta.meusic.gtk.desktop" \
    -i "$APPDIR/usr/share/icons/hicolor/scalable/apps/com.sarta.meusic.gtk.svg" \
    --plugin gtk

# ---- 2. De-patchelf: restore every bundled ELF from the pristine host copy ---
restore_pristine() {
    local f bn src real
    while IFS= read -r f; do
        bn="$(basename "$f")"
        # Prefer the same relative path under the host libdir, else basename search.
        src=""
        for d in /usr/lib64 /lib64 /usr/lib; do
            [ -e "$d/$bn" ] && { src="$d/$bn"; break; }
        done
        [ -z "$src" ] && src="$(find /usr/lib64 /lib64 -name "$bn" 2>/dev/null | head -1)"
        if [ -n "$src" ]; then
            real="$(readlink -f "$src")"
            cp -f --remove-destination "$real" "$f"
        fi
    done < <(find "$APPDIR" -type f -name "*.so*")
}
restore_pristine
# The main binary + glycin loaders are also patchelf'd — restore them pristine.
cp -f --remove-destination "$HERE/target/release/meusic" "$APPDIR/usr/bin/meusic"
cp -f --remove-destination "$GLYCIN_LIBEXEC/glycin-svg"      "$APPDIR/usr/bin/glycin-svg"
cp -f --remove-destination "$GLYCIN_LIBEXEC/glycin-image-rs" "$APPDIR/usr/bin/glycin-image-rs"

# ---- 3. glycin: ship loader configs + a runtime hook ------------------------
# Fedora's gdk-pixbuf delegates ALL image decoding (our SVG logos/icons AND
# PNG/JPEG cover art) to glycin's out-of-process loaders. Ship their .conf and
# append a glycin block to the gtk AppRun hook (linuxdeploy's AppRun sources
# ONLY that hook by name — it does not glob *.sh). The hook rewrites the loader
# Exec= paths to the extracted AppDir, sets GLYCIN_DATA_DIR, and shadows bwrap
# so glycin runs the loaders unsandboxed (resolving bundled libs via the
# AppRun-exported LD_LIBRARY_PATH) instead of the host /usr that other distros
# won't have.
mkdir -p "$APPDIR/usr/share/glycin-loaders/2+/conf.d"
cp "$GLYCIN_CONFD/glycin-svg.conf"      "$APPDIR/usr/share/glycin-loaders/2+/conf.d/"
cp "$GLYCIN_CONFD/glycin-image-rs.conf" "$APPDIR/usr/share/glycin-loaders/2+/conf.d/"
cat >> "$APPDIR/apprun-hooks/linuxdeploy-plugin-gtk.sh" <<'HOOK'
# Make Fedora's glycin image backend use the loaders bundled in this AppImage.
_gly="${XDG_RUNTIME_DIR:-/tmp}/meusic-glycin.$$"
rm -rf "$_gly"; mkdir -p "$_gly/glycin-loaders/2+/conf.d" "$_gly/bin"
for _c in "$APPDIR/usr/share/glycin-loaders/2+/conf.d/"*.conf; do
    sed "s|Exec=/usr/libexec/glycin-loaders/2+/|Exec=$APPDIR/usr/bin/|g" \
        "$_c" > "$_gly/glycin-loaders/2+/conf.d/$(basename "$_c")"
done
export GLYCIN_DATA_DIR="$_gly"
printf '#!/bin/sh\nexit 1\n' > "$_gly/bin/bwrap"
chmod +x "$_gly/bin/bwrap"
export PATH="$_gly/bin:$PATH"
HOOK

# ---- 4. Package -------------------------------------------------------------
"$TOOLS/appimagetool.AppImage" "$APPDIR" \
    "$HERE/target/appimage/meusic-${VERSION}-x86_64.AppImage"

echo "=== AppImage built ==="
ls -la "$HERE/target/appimage/"*.AppImage
