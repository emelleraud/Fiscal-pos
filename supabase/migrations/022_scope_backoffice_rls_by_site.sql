-- =============================================================================
-- 022_scope_backoffice_rls_by_site.sql
--
-- Fix sécurité : les policies "backoffice_*" ajoutées par 009_backoffice_policies.sql
-- sur z_reports et site_configs n'étaient pas filtrées par site_id (USING/WITH
-- CHECK (true) pour tout `authenticated`). Concrètement, un simple 'manager'
-- ou 'director' d'un site pouvait lire le journal Z de TOUS les sites et
-- écrire le menu de N'IMPORTE QUEL site.
--
-- z_reports avait en plus une policy anon ouverte depuis 006_z_reports_anon_read.sql
-- (ajoutée avant que le back-office n'ait d'authentification) : la clé publique
-- anon pouvait lire tous les rapports Z sans aucun login. Jamais révoquée
-- après l'ajout de l'auth en Sprint 4 (009).
--
-- sync-client utilise toujours la clé service_role (bypass RLS) — ce correctif
-- ne le concerne pas, voir sync-client/src/config.rs.
-- =============================================================================

-- ---------------------------------------------------------------------------
-- can_access_site() : étendre aux rôles back-office mono-site
-- 'manager'/'director' portent app_metadata.site_id, comme 'pos_caissier'.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.can_access_site(p_site_id uuid)
RETURNS boolean LANGUAGE sql STABLE
SET search_path = public AS $$
  SELECT CASE public.auth_app_role()
    WHEN 'pos_admin'         THEN true
    WHEN 'pos_auditeur'      THEN true
    WHEN 'regional_director' THEN EXISTS (
      SELECT 1 FROM restaurant_group_members rgm
      JOIN   restaurant_groups rg ON rg.id = rgm.group_id
      WHERE  rgm.site_id = p_site_id AND rg.created_by = auth.uid()
    )
    WHEN 'pos_caissier' THEN p_site_id = public.auth_site_id()
    WHEN 'manager'      THEN p_site_id = public.auth_site_id()
    WHEN 'director'     THEN p_site_id = public.auth_site_id()
    ELSE false
  END;
$$;

-- ---------------------------------------------------------------------------
-- TABLE : z_reports
-- ---------------------------------------------------------------------------

-- Fermer l'accès anon resté ouvert depuis 006 : plus de lecture sans auth.
DROP POLICY IF EXISTS "anon_read_z_reports" ON public.z_reports;
REVOKE SELECT ON public.z_reports FROM anon;

-- Remplacer la lecture authenticated non filtrée (009) par une lecture scopée au site.
DROP POLICY IF EXISTS "backoffice_read_z_reports" ON public.z_reports;
CREATE POLICY "backoffice_read_z_reports"
    ON public.z_reports
    FOR SELECT
    TO authenticated
    USING (public.can_access_site(site_id));

-- ---------------------------------------------------------------------------
-- TABLE : site_configs
-- Lecture : tout rôle ayant accès au site (y compris pos_auditeur en lecture).
-- Écriture : uniquement les rôles habilités à gérer le menu, et seulement
-- sur leur périmètre (pos_auditeur exclu — rôle lecture seule).
-- ---------------------------------------------------------------------------

DROP POLICY IF EXISTS "backoffice_read_site_configs" ON public.site_configs;
CREATE POLICY "backoffice_read_site_configs"
    ON public.site_configs
    FOR SELECT
    TO authenticated
    USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "backoffice_write_site_configs" ON public.site_configs;
CREATE POLICY "backoffice_write_site_configs"
    ON public.site_configs
    FOR INSERT
    TO authenticated
    WITH CHECK (
        public.auth_app_role() IN ('manager', 'director', 'regional_director', 'pos_admin')
        AND public.can_access_site(site_id)
    );

DROP POLICY IF EXISTS "backoffice_update_site_configs" ON public.site_configs;
CREATE POLICY "backoffice_update_site_configs"
    ON public.site_configs
    FOR UPDATE
    TO authenticated
    USING (
        public.auth_app_role() IN ('manager', 'director', 'regional_director', 'pos_admin')
        AND public.can_access_site(site_id)
    )
    WITH CHECK (
        public.auth_app_role() IN ('manager', 'director', 'regional_director', 'pos_admin')
        AND public.can_access_site(site_id)
    );
