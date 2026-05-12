#!/bin/bash
# =============================================================================
# 06_smoke_test.sh
# Vérifie rapidement que l'edge-api fonctionne correctement.
# Prérequis : edge-api lancée (script 03).
# =============================================================================

set -euo pipefail

API="http://localhost:8080"

echo "=== Smoke test edge-api ($API) ==="
echo ""

# Health check
echo "--- GET /api/v1/health ---"
curl -s "$API/api/v1/health" | python3 -m json.tool
echo ""

# Menu
echo "--- GET /api/v1/menu ---"
curl -s "$API/api/v1/menu" | python3 -m json.tool
echo ""

# Ouvrir une session
echo "--- POST /api/v1/sessions/open ---"
SESSION=$(curl -s -X POST "$API/api/v1/sessions/open")
echo "$SESSION" | python3 -m json.tool
SESSION_ID=$(echo "$SESSION" | python3 -c "import sys,json; print(json.load(sys.stdin)['session_id'])")
echo "Session ID : $SESSION_ID"
echo ""

# Enregistrer une vente
echo "--- POST /api/v1/orders (vente 11,00 € TVA 10%) ---"
ORDER=$(curl -s -X POST "$API/api/v1/orders" \
  -H "Content-Type: application/json" \
  -d '{
    "order_reference": "ORD-SMOKE-001",
    "amount_ttc_cents": 1100,
    "tva_rate": "10",
    "payment_method": "card"
  }')
echo "$ORDER" | python3 -m json.tool
echo ""

# Vérifier la session active
echo "--- GET /api/v1/sessions/current ---"
curl -s "$API/api/v1/sessions/current" | python3 -m json.tool
echo ""

echo "✅ Smoke test terminé — l'edge-api répond correctement."
echo ""
echo "   Pour clôturer la session :"
echo "   curl -X POST $API/api/v1/sessions/close | python3 -m json.tool"
