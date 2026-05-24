-- =============================================================================
-- 009_backoffice_policies.sql
-- Accès authenticated au back-office React (Sprint 4 — Auth + Multi-sites + Menu)
--
-- Contexte : le back-office passe en mode authentifié (email/password Supabase).
-- Les rôles applicatifs (pos_admin, pos_caissier…) nécessitent des claims JWT
-- custom qui ne sont pas encore configurés. Ces policies ouvrent la lecture à
-- tout utilisateur authentifié pour les besoins du back-office admin.
--
-- Tables concernées :
--   sites        → lecture de tous les sites (dropdown multi-sites)
--   z_reports    → lecture de tous les rapports Z (était anon seulement)
--   site_configs → lecture + écriture (CRUD menu via MenuManager)
-- =============================================================================

-- ---------------------------------------------------------------------------
-- TABLE : sites — lecture pour tout utilisateur authentifié
-- ---------------------------------------------------------------------------
GRANT SELECT ON public.sites TO authenticated;

CREATE POLICY "backoffice_read_sites"
    ON public.sites
    FOR SELECT
    TO authenticated
    USING (true);

-- ---------------------------------------------------------------------------
-- TABLE : z_reports — lecture pour tout utilisateur authentifié
-- (migration 006 avait accordé anon ; on complète pour authenticated)
-- ---------------------------------------------------------------------------
GRANT SELECT ON public.z_reports TO authenticated;

CREATE POLICY "backoffice_read_z_reports"
    ON public.z_reports
    FOR SELECT
    TO authenticated
    USING (true);

-- ---------------------------------------------------------------------------
-- TABLE : site_configs — lecture + upsert pour tout utilisateur authentifié
-- (était service_role only — migration 008)
-- ---------------------------------------------------------------------------
GRANT SELECT, INSERT, UPDATE ON public.site_configs TO authenticated;

CREATE POLICY "backoffice_read_site_configs"
    ON public.site_configs
    FOR SELECT
    TO authenticated
    USING (true);

CREATE POLICY "backoffice_write_site_configs"
    ON public.site_configs
    FOR INSERT
    TO authenticated
    WITH CHECK (true);

CREATE POLICY "backoffice_update_site_configs"
    ON public.site_configs
    FOR UPDATE
    TO authenticated
    USING (true)
    WITH CHECK (true);
