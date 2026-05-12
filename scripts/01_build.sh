#!/bin/bash
# =============================================================================
# 01_build.sh
# Compile le workspace Rust complet.
# =============================================================================

set -euo pipefail

PROJECT_DIR="/home/angelo/PROJ_POS_QSR/pos-fiscal"
cd "$PROJECT_DIR"

echo "=== [1/2] cargo check (vérification rapide) ==="
cargo check --workspace

echo ""
echo "=== [2/2] cargo build --release ==="
cargo build --workspace --release

echo ""
echo "✅ Build terminé."
echo "   Binaires : $PROJECT_DIR/target/release/edge-api"
echo "              $PROJECT_DIR/target/release/sync-client"
