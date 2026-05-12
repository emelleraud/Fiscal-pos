#!/bin/bash
# =============================================================================
# 04_run_frontend.sh
# Lance le frontend React en mode développement.
# À exécuter dans un terminal dédié (séparé de edge-api).
# Ouvrir http://localhost:5173 dans le navigateur.
# =============================================================================

set -euo pipefail

PROJECT_DIR="/home/angelo/PROJ_POS_QSR/pos-fiscal/pos-app"
cd "$PROJECT_DIR"

# Installer les dépendances npm si besoin
if [ ! -d "node_modules" ]; then
    echo "=== Installation des dépendances npm (première fois) ==="
    npm install
fi

# Créer le .env.local si absent
if [ ! -f ".env.local" ]; then
    cp .env.example .env.local
    echo "⚠️  Fichier .env.local créé — VITE_API_URL=http://localhost:8080"
fi

echo "=== Lancement pos-app (Vite dev server) ==="
echo "    Frontend : http://localhost:5173"
echo "    API      : http://localhost:8080 (edge-api doit être lancée)"
echo ""
echo "    Ctrl+C pour arrêter."
echo ""

npm run dev
