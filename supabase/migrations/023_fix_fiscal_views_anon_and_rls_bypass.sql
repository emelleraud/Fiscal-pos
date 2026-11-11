-- =============================================================================
-- 023_fix_fiscal_views_anon_and_rls_bypass.sql
--
-- fiscal_entries_enriched / daily_revenue_by_site ont été passées en
-- SECURITY DEFINER par 005_backoffice_views_anon_read.sql (avant que le
-- back-office n'ait d'authentification — Sprint 4 / 009). En mode DEFINER,
-- une vue s'exécute avec les droits de son propriétaire (postgres) et
-- bypass totalement le RLS des tables sous-jacentes pour QUICONQUE la lit.
--
-- L'accès anon accordé par 005 n'était donc que la partie visible du
-- problème : même un utilisateur authentifié d'un seul site (manager/
-- director) voit aujourd'hui le CA et le journal fiscal (fiscal_entries)
-- de TOUS les sites via ces deux vues, pas seulement le sien.
--
-- fiscal_entries et sessions n'ont par ailleurs jamais eu de policy RLS
-- ni de GRANT pour les rôles back-office (manager/director/regional_director)
-- introduits en Sprint 4 — seuls pos_admin/pos_caissier/pos_auditeur (002)
-- en ont. C'est très probablement pour ça que 005 avait contourné le
-- problème via security_invoker=false plutôt que de l'ajouter alors.
--
-- Fix :
--   1. GRANT SELECT sur fiscal_entries/sessions à authenticated (jamais
--      accordé — nécessaire pour que les policies RLS ci-dessous
--      s'appliquent, voir note en fin de fichier).
--   2. Policies manquantes pour manager/director/regional_director,
--      scopées par can_access_site(site_id).
--   3. Repasser les deux vues en SECURITY INVOKER (comme prévu à l'origine
--      par 002_roles_rls.sql) pour qu'elles respectent le RLS des tables
--      de base au lieu de le bypasser.
--   4. Révoquer l'accès anon sur les deux vues.
-- =============================================================================

-- ---------------------------------------------------------------------------
-- TABLE : fiscal_entries — lecture back-office scopée au site
-- ---------------------------------------------------------------------------
GRANT SELECT ON public.fiscal_entries TO authenticated;

DROP POLICY IF EXISTS "backoffice_read_fiscal_entries" ON public.fiscal_entries;
CREATE POLICY "backoffice_read_fiscal_entries"
    ON public.fiscal_entries
    FOR SELECT
    TO authenticated
    USING (
        public.auth_app_role() IN ('manager', 'director', 'regional_director')
        AND public.can_access_site(site_id)
    );

-- ---------------------------------------------------------------------------
-- TABLE : sessions — lecture back-office scopée au site
-- (nécessaire pour l'enrichissement session_ref/operator_id dans la vue)
-- ---------------------------------------------------------------------------
GRANT SELECT ON public.sessions TO authenticated;

DROP POLICY IF EXISTS "backoffice_read_sessions" ON public.sessions;
CREATE POLICY "backoffice_read_sessions"
    ON public.sessions
    FOR SELECT
    TO authenticated
    USING (
        public.auth_app_role() IN ('manager', 'director', 'regional_director')
        AND public.can_access_site(site_id)
    );

-- ---------------------------------------------------------------------------
-- VUES : repasser en SECURITY INVOKER + fermer l'accès anon
-- ---------------------------------------------------------------------------
ALTER VIEW public.fiscal_entries_enriched SET (security_invoker = true);
ALTER VIEW public.daily_revenue_by_site   SET (security_invoker = true);

REVOKE SELECT ON public.fiscal_entries_enriched FROM anon;
REVOKE SELECT ON public.daily_revenue_by_site   FROM anon;
