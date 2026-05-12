#!/bin/bash
# =============================================================================
# 00_install_prerequisites.sh
# À exécuter UNE SEULE FOIS sur une machine vierge.
# Pop!OS / Ubuntu — installe Rust, SQLite, Node.js
# =============================================================================

set -euo pipefail

echo "=== [1/4] Outils système ==="
sudo apt update
sudo apt install -y libsqlite3-dev build-essential pkg-config curl

echo "=== [2/4] Rust ==="
if command -v rustc &>/dev/null; then
    echo "Rust déjà installé : $(rustc --version)"
else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi
rustup update stable

echo "=== [3/4] Node.js via nvm ==="
if command -v node &>/dev/null; then
    echo "Node déjà installé : $(node --version)"
else
    curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
    export NVM_DIR="$HOME/.nvm"
    # shellcheck source=/dev/null
    source "$NVM_DIR/nvm.sh"
    nvm install --lts
    nvm use --lts
fi

echo "=== [4/4] Versions installées ==="
rustc --version
cargo --version
node --version
npm --version

echo ""
echo "✅ Prérequis installés."
echo "   Relance un nouveau terminal avant de continuer (pour recharger PATH)."
