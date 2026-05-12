#!/bin/bash
# =============================================================================
# 02_test.sh
# Lance tous les tests : Rust (workspace) + TypeScript (pos-app).
# =============================================================================

set -euo pipefail

PROJECT_DIR="/home/angelo/PROJ_POS_QSR/pos-fiscal"

echo "=== [1/3] Tests Rust — fiscal-engine (tests d'intégrité NF525) ==="
cd "$PROJECT_DIR"
cargo test --package fiscal-engine -- --nocapture

echo ""
echo "=== [2/3] Tests Rust — workspace complet ==="
cargo test --workspace

echo ""
echo "=== [3/3] Tests TypeScript — pos-app ==="
cd "$PROJECT_DIR/pos-app"
if [ ! -d "node_modules" ] || [ ! -d "node_modules/jsdom" ]; then
    echo "Installation des dépendances npm..."
    npm install
fi
npm test

echo ""
echo "✅ Tous les tests passés."
