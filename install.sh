#!/usr/bin/env bash
# PortKyu — instalador para o usuário atual (não precisa de sudo).
#
#   Dentro do repositório clonado:   ./install.sh            (compila com cargo e instala)
#   Sem clonar (baixa a release):    curl -fsSL https://raw.githubusercontent.com/RafaelProfMgz/portkyu/main/install.sh | bash
#   Desinstalar:                     ./install.sh --uninstall
#
# Instala em ~/.local/bin, ~/.local/share/applications e ~/.local/share/icons.
set -euo pipefail

REPO="RafaelProfMgz/portkyu"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor"

say()  { printf '\033[1;35m▸\033[0m %s\n' "$*"; }
ok()   { printf '\033[1;32m✔\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m✘\033[0m %s\n' "$*" >&2; exit 1; }

refresh_caches() {
  command -v update-desktop-database >/dev/null && update-desktop-database "$APP_DIR" 2>/dev/null || true
  command -v gtk-update-icon-cache  >/dev/null && gtk-update-icon-cache -f -t "$ICON_DIR" 2>/dev/null || true
  command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
  command -v kbuildsycoca5 >/dev/null && kbuildsycoca5 >/dev/null 2>&1 || true
}

uninstall() {
  rm -f "$BIN_DIR/portkyu" "$APP_DIR/portkyu.desktop" \
        "$ICON_DIR/scalable/apps/portkyu.svg" "$ICON_DIR/256x256/apps/portkyu.png"
  refresh_caches
  ok "PortKyu removido."
}

[[ "${1:-}" == "--uninstall" ]] && { uninstall; exit 0; }

[[ "$(uname -s)" == "Linux" ]] || die "PortKyu só funciona no Linux (lê /proc)."

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || echo "")"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [[ -n "$SRC_DIR" && -f "$SRC_DIR/Cargo.toml" ]] && grep -q '^name = "portkyu"' "$SRC_DIR/Cargo.toml" && command -v cargo >/dev/null; then
  say "Compilando a partir do código-fonte (cargo build --release)…"
  (cd "$SRC_DIR" && cargo build --release --locked)
  BIN="$SRC_DIR/target/release/portkyu"
  ASSETS="$SRC_DIR"
else
  [[ "$(uname -m)" == "x86_64" ]] || die "Binário pronto só para x86_64. Clone o repositório e rode ./install.sh com Rust instalado."
  command -v curl >/dev/null || die "Precisa do curl."
  say "Baixando a última versão de github.com/$REPO…"
  curl -fsSL "https://github.com/$REPO/releases/latest/download/portkyu-x86_64-linux.tar.gz" -o "$TMP/portkyu.tar.gz" \
    || die "Não consegui baixar a release. Veja https://github.com/$REPO/releases"
  tar -xzf "$TMP/portkyu.tar.gz" -C "$TMP"
  BIN="$TMP/portkyu/portkyu"
  ASSETS="$TMP/portkyu"
fi

[[ -x "$BIN" ]] || die "Binário não encontrado em $BIN"

say "Instalando…"
install -Dm755 "$BIN" "$BIN_DIR/portkyu"
install -Dm644 "$ASSETS/packaging/portkyu.desktop" "$APP_DIR/portkyu.desktop"
install -Dm644 "$ASSETS/assets/portkyu.svg" "$ICON_DIR/scalable/apps/portkyu.svg"
install -Dm644 "$ASSETS/assets/portkyu-256.png" "$ICON_DIR/256x256/apps/portkyu.png"
# usa o caminho absoluto para funcionar mesmo se ~/.local/bin não estiver no PATH do ambiente gráfico
sed -i "s|^Exec=portkyu|Exec=$BIN_DIR/portkyu|" "$APP_DIR/portkyu.desktop"
refresh_caches

ok "PortKyu $("$BIN_DIR/portkyu" --version | awk '{print $2}') instalado."
echo "   • Abra pelo menu de aplicativos (\"PortKyu\") ou rode: portkyu"
echo "   • No KDE Plasma o atalho Meta+Shift+P é registrado automaticamente."
case ":$PATH:" in *":$BIN_DIR:"*) ;; *) echo "   • Adicione $BIN_DIR ao PATH para usar no terminal." ;; esac
