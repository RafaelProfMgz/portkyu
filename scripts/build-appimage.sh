#!/usr/bin/env bash
# Gera dist/PortKyu-<versão>-x86_64.AppImage e dist/portkyu-x86_64-linux.tar.gz
#
#   ./scripts/build-appimage.sh
#
# O appimagetool é baixado em .cache/tools na primeira vez. Não precisa de FUSE para gerar.
set -euo pipefail
cd "$(dirname "$0")/.."

ARCH="x86_64"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
TOOLS=".cache/tools"
APPDIR="dist/PortKyu.AppDir"
TOOL="$TOOLS/appimagetool-$ARCH.AppImage"

echo "▸ compilando portkyu $VERSION"
cargo build --release --locked

echo "▸ montando AppDir"
rm -rf "$APPDIR" && mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$APPDIR/usr/share/icons/hicolor/scalable/apps"
install -m755 target/release/portkyu "$APPDIR/usr/bin/portkyu"
install -m644 packaging/portkyu.desktop "$APPDIR/usr/share/applications/portkyu.desktop"
install -m644 packaging/portkyu.desktop "$APPDIR/portkyu.desktop"
install -m644 assets/portkyu-256.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/portkyu.png"
install -m644 assets/portkyu.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/portkyu.svg"
install -m644 assets/portkyu-256.png "$APPDIR/portkyu.png"
ln -sf portkyu.png "$APPDIR/.DirIcon"
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/portkyu" "$@"
EOF
chmod +x "$APPDIR/AppRun"

if [ ! -x "$TOOL" ]; then
  echo "▸ baixando appimagetool"
  mkdir -p "$TOOLS"
  curl -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
  chmod +x "$TOOL"
fi

echo "▸ gerando AppImage"
OUT="dist/PortKyu-$VERSION-$ARCH.AppImage"
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" "$TOOL" --no-appstream "$APPDIR" "$OUT"

echo "▸ gerando tarball"
rm -rf dist/portkyu && mkdir -p dist/portkyu/packaging dist/portkyu/assets
cp target/release/portkyu install.sh README.md LICENSE dist/portkyu/
cp packaging/portkyu.desktop dist/portkyu/packaging/
cp assets/portkyu.svg assets/portkyu-256.png dist/portkyu/assets/
tar -czf "dist/portkyu-$ARCH-linux.tar.gz" -C dist portkyu

(cd dist && sha256sum "$(basename "$OUT")" "portkyu-$ARCH-linux.tar.gz" > SHA256SUMS)
echo "✔ pronto:"
ls -lh "$OUT" "dist/portkyu-$ARCH-linux.tar.gz" dist/SHA256SUMS
