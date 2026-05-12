#!/bin/bash
# =============================================================================
# 03_run_edge_api.sh
# Lance le serveur edge-api en mode développement.
# À exécuter dans un terminal dédié.
# =============================================================================

set -euo pipefail

PROJECT_DIR="/home/angelo/PROJ_POS_QSR/pos-fiscal"
cd "$PROJECT_DIR"

# Créer le .env s'il n'existe pas encore
if [ ! -f ".env" ]; then
    cp .env.example .env
    echo "⚠️  Fichier .env créé depuis .env.example — vérifier les valeurs si besoin."
fi

echo "=== Lancement edge-api ==="
echo "    Base SQLite : $PROJECT_DIR/restaurant.db"
echo "    API locale  : http://localhost:8080"
echo "    Logs        : edge_api=debug, fiscal_engine=debug"
echo ""
echo "    Ctrl+C pour arrêter proprement."
echo ""

DATABASE_URL="sqlite:$PROJECT_DIR/restaurant.db" \
EDGE_API_HOST="0.0.0.0" \
EDGE_API_PORT="8080" \
DATA_DIR="$PROJECT_DIR/data" \
RUST_LOG="edge_api=debug,fiscal_engine=debug,tower_http=info" \
cargo run --bin edge-api
