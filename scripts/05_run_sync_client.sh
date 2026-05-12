#!/bin/bash
# =============================================================================
# 05_run_sync_client.sh
# Lance l'agent de synchronisation vers Supabase.
# OPTIONNEL — seulement si SUPABASE_URL et SUPABASE_SERVICE_KEY sont configurés.
# À exécuter dans un terminal dédié.
# =============================================================================

set -euo pipefail

PROJECT_DIR="/home/angelo/PROJ_POS_QSR/pos-fiscal"
cd "$PROJECT_DIR"

# Vérifier que les variables Supabase sont présentes
if [ -z "${SUPABASE_URL:-}" ] || [ -z "${SUPABASE_SERVICE_KEY:-}" ]; then
    echo "⚠️  Variables Supabase non définies."
    echo "   Édite le fichier .env et renseigne :"
    echo "     SUPABASE_URL=https://xxxx.supabase.co"
    echo "     SUPABASE_SERVICE_KEY=eyJ..."
    echo ""
    echo "   Puis relance ce script avec :"
    echo "     source .env && bash scripts/05_run_sync_client.sh"
    exit 1
fi

echo "=== Lancement sync-client ==="
echo "    Supabase  : $SUPABASE_URL"
echo "    Intervalle : ${SYNC_INTERVAL_SECONDS:-30}s"
echo ""
echo "    Ctrl+C pour arrêter proprement."
echo ""

DATABASE_URL="sqlite:$PROJECT_DIR/restaurant.db" \
SITE_ID="${SITE_ID:-SITE-DEV-001}" \
SYNC_INTERVAL_SECONDS="${SYNC_INTERVAL_SECONDS:-30}" \
RUST_LOG="sync_client=info" \
cargo run --bin sync-client
