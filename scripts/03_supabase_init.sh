#!/usr/bin/env bash
# =============================================================================
# 03_supabase_init.sh
# Initialise le projet Supabase CLI et applique les migrations fiscales
# =============================================================================
# Prérequis :
#   - Supabase CLI installé (voir instructions ci-dessous si absent)
#   - Compte Supabase créé sur https://supabase.com
#   - Être dans le répertoire racine du projet : PROJ_POS_QSR/pos-fiscal/
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SUPABASE_DIR="$PROJECT_ROOT/supabase"

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║          pos-fiscal — Supabase Init (Phase 1)           ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

# ---------------------------------------------------------------------------
# 0. Vérification de la CLI Supabase
# ---------------------------------------------------------------------------
if ! command -v supabase &>/dev/null; then
    echo "❌  Supabase CLI introuvable."
    echo ""
    echo "    Installation rapide (Linux/WSL) :"
    echo "    ─────────────────────────────────"
    echo "    curl -fsSL https://github.com/supabase/cli/releases/latest/download/supabase_linux_amd64.tar.gz \\"
    echo "      | tar -xz && sudo mv supabase /usr/local/bin/"
    echo ""
    echo "    Puis relancer ce script."
    exit 1
fi

SUPABASE_VERSION=$(supabase --version 2>&1 | head -1)
echo "✅  Supabase CLI : $SUPABASE_VERSION"

# ---------------------------------------------------------------------------
# 1. supabase init (idempotent — ne réinitialise pas si déjà fait)
# ---------------------------------------------------------------------------
if [ ! -f "$PROJECT_ROOT/supabase/config.toml" ]; then
    echo ""
    echo "▶  Initialisation du projet Supabase local..."
    cd "$PROJECT_ROOT"
    supabase init
    echo "✅  supabase/config.toml créé."
else
    echo "✅  supabase/config.toml déjà présent, init ignorée."
fi

# ---------------------------------------------------------------------------
# 2. Vérification des fichiers de migration
# ---------------------------------------------------------------------------
echo ""
echo "▶  Vérification des migrations..."

MIGRATIONS=(
    "$SUPABASE_DIR/migrations/001_fiscal_schema.sql"
    "$SUPABASE_DIR/migrations/002_roles_rls.sql"
)

for f in "${MIGRATIONS[@]}"; do
    if [ ! -f "$f" ]; then
        echo "❌  Fichier manquant : $f"
        echo "    Vérifier que les migrations ont été placées dans supabase/migrations/"
        exit 1
    fi
    echo "   ✓  $(basename "$f")"
done

echo "✅  2 fichiers de migration présents."

# ---------------------------------------------------------------------------
# 3. Login Supabase (ouvre le navigateur pour auth)
# ---------------------------------------------------------------------------
echo ""
echo "▶  Connexion à Supabase..."
echo "   (Une fenêtre de navigateur va s'ouvrir pour l'authentification)"
echo ""
supabase login

# ---------------------------------------------------------------------------
# 4. Lier au projet Supabase distant
# ---------------------------------------------------------------------------
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  ÉTAPE MANUELLE REQUISE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  1. Va sur https://supabase.com/dashboard"
echo "  2. Crée un nouveau projet  →  note le PROJECT_REF"
echo "     (format : abcdefghijklmnop — 20 caractères)"
echo "  3. Colle le PROJECT_REF ci-dessous :"
echo ""
read -rp "  PROJECT_REF : " PROJECT_REF

if [ -z "$PROJECT_REF" ]; then
    echo "❌  PROJECT_REF vide. Abandon."
    exit 1
fi

echo ""
echo "▶  Liaison au projet distant $PROJECT_REF..."
cd "$PROJECT_ROOT"
supabase link --project-ref "$PROJECT_REF"
echo "✅  Projet lié."

# ---------------------------------------------------------------------------
# 5. Push des migrations vers Supabase
# ---------------------------------------------------------------------------
echo ""
echo "▶  Application des migrations (db push)..."
echo ""
supabase db push

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║  ✅  Migrations appliquées avec succès                  ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""
echo "  Tables créées :"
echo "    • public.sites"
echo "    • public.sessions"
echo "    • public.fiscal_entries   (journal immuable)"
echo "    • public.z_reports"
echo ""
echo "  Vues créées :"
echo "    • public.fiscal_entries_enriched"
echo "    • public.daily_revenue_by_site"
echo ""
echo "  RLS activé sur toutes les tables métier."
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  PROCHAINE ÉTAPE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  Récupère les clés de ton projet dans le dashboard :"
echo "  Settings → API"
echo ""
echo "  Tu auras besoin de :"
echo "    SUPABASE_URL        = https://<PROJECT_REF>.supabase.co"
echo "    SUPABASE_ANON_KEY   = eyJ...  (clé publique, back-office React)"
echo "    SUPABASE_SERVICE_KEY= eyJ...  (clé privée, sync_client Rust — NE PAS COMMITER)"
echo ""
echo "  Ces valeurs alimenteront :"
echo "    → supabase/.env.local          (gitignored)"
echo "    → sync_client/.env             (gitignored)"
echo ""
