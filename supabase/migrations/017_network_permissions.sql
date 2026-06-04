-- supabase/migrations/017_network_permissions.sql
-- Matrice lock/unlock : dimension × rôle cible × scope × fenêtre temporelle

CREATE TABLE IF NOT EXISTS public.network_permissions (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  -- Scope mutuellement exclusif :
  --   (null, null)      → réseau entier
  --   (null, group_id)  → per-groupe
  --   (site_id, null)   → per-site (priorité max)
  site_id     uuid REFERENCES public.sites(id)              ON DELETE CASCADE,
  group_id    uuid REFERENCES public.restaurant_groups(id)  ON DELETE CASCADE,
  dimension   text NOT NULL,
    -- 'menu' | 'prices' | 'promotions' | 'discounts' | 'user_management' | 'z_reports'
  target_role text NOT NULL,
    -- 'manager' | 'director' | 'regional_director'
  locked      boolean NOT NULL DEFAULT false,
  lock_from   time,
  lock_until  time,
  reason      text,
  updated_by  uuid REFERENCES auth.users(id),
  updated_at  timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT chk_scope CHECK (
    NOT (site_id IS NOT NULL AND group_id IS NOT NULL)
  ),
  CONSTRAINT chk_time_window CHECK (
    (lock_from IS NULL) = (lock_until IS NULL)
    AND (lock_from IS NULL OR lock_from < lock_until)
  ),
  UNIQUE NULLS NOT DISTINCT (site_id, group_id, dimension, target_role)
);

ALTER TABLE public.network_permissions ENABLE ROW LEVEL SECURITY;

-- Lecture : tout authentifié (nécessaire pour charger les règles héritées)
DROP POLICY IF EXISTS "np_read" ON public.network_permissions;
CREATE POLICY "np_read" ON public.network_permissions
  FOR SELECT TO authenticated USING (true);

-- Écriture pos_admin : toutes les lignes
DROP POLICY IF EXISTS "np_admin_write" ON public.network_permissions;
CREATE POLICY "np_admin_write" ON public.network_permissions
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');

-- Écriture regional_director : per-site (ses sites) et per-group (ses groupes) uniquement
DROP POLICY IF EXISTS "np_rd_write" ON public.network_permissions;
CREATE POLICY "np_rd_write" ON public.network_permissions
  FOR ALL TO authenticated
  USING (
    public.auth_app_role() = 'regional_director' AND (
      (site_id  IS NOT NULL AND group_id IS NULL AND public.can_access_site(site_id)) OR
      (group_id IS NOT NULL AND site_id  IS NULL AND EXISTS (
        SELECT 1 FROM public.restaurant_groups WHERE id = group_id AND created_by = auth.uid()
      ))
    )
  )
  WITH CHECK (
    public.auth_app_role() = 'regional_director' AND (
      (site_id  IS NOT NULL AND group_id IS NULL AND public.can_access_site(site_id)) OR
      (group_id IS NOT NULL AND site_id  IS NULL AND EXISTS (
        SELECT 1 FROM public.restaurant_groups WHERE id = group_id AND created_by = auth.uid()
      ))
    )
  );

DROP POLICY IF EXISTS "np_service_role" ON public.network_permissions;
CREATE POLICY "np_service_role" ON public.network_permissions
  FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.network_permissions TO authenticated;
