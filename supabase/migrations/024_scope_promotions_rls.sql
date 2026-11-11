-- =============================================================================
-- 024_scope_promotions_rls.sql
--
-- "director_write_promotions" (014_promotions.sql) grants FOR ALL (SELECT +
-- INSERT + UPDATE + DELETE) to role 'director' with NO site/group filter,
-- even though 'director' is a single-site role everywhere else in the
-- codebase (can_access_site() scopes it by auth_site_id(), like 'manager').
-- A site director could read, edit or DELETE any other site's promotions.
-- The manager INSERT policy (015) has the same hole: its WITH CHECK lets
-- 'director'/'regional_director' through without checking site_id at all.
-- "authenticated_read_promotions" (014) is also unscoped (USING (true)) —
-- same unscoped-read pattern already fixed on other tables.
--
-- Also: pos_admin never had an explicit write policy on promotions at all
-- (only service_role and the buggy director/regional_director one did) —
-- filling that real gap here too.
--
-- Per decision: regional_director is scoped to the sites/groups they
-- manage (can_access_site() / restaurant_groups.created_by), NOT the
-- whole chain — consistent with network_permissions (017) and
-- can_access_site() itself. Only pos_admin gets unscoped chain-wide access,
-- and only pos_admin can write scope='chain' promotions.
-- =============================================================================

DROP POLICY IF EXISTS "authenticated_read_promotions"      ON public.promotions;
DROP POLICY IF EXISTS "manager_write_site_promotions"      ON public.promotions;
DROP POLICY IF EXISTS "manager_update_own_site_promotions" ON public.promotions;
DROP POLICY IF EXISTS "director_write_promotions"          ON public.promotions;

-- ---------------------------------------------------------------------------
-- READ : chain-scope visible à tous (s'applique partout) ; site-scope
-- via can_access_site() (couvre pos_admin/pos_auditeur/regional_director
-- de ses groupes/manager/director de son site) ; group-scope visible au
-- régional qui possède le groupe ou à un site membre de ce groupe.
-- ---------------------------------------------------------------------------
CREATE POLICY "scoped_read_promotions" ON public.promotions
  FOR SELECT TO authenticated
  USING (
    public.auth_app_role() IN ('pos_admin', 'pos_auditeur')
    OR scope = 'chain'
    OR (scope = 'site'  AND public.can_access_site(site_id))
    OR (scope = 'group' AND (
          EXISTS (
            SELECT 1 FROM public.restaurant_groups
            WHERE id = promotions.group_id AND created_by = auth.uid()
          )
          OR EXISTS (
            SELECT 1 FROM public.restaurant_group_members gm
            WHERE gm.group_id = promotions.group_id AND gm.site_id = public.auth_site_id()
          )
        ))
  );

-- ---------------------------------------------------------------------------
-- WRITE : pos_admin (chain-wide, y compris scope='chain')
-- ---------------------------------------------------------------------------
CREATE POLICY "pos_admin_write_promotions" ON public.promotions
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');

-- ---------------------------------------------------------------------------
-- WRITE : manager — INSERT/UPDATE sur son propre site uniquement, pas de DELETE
-- ---------------------------------------------------------------------------
CREATE POLICY "manager_write_own_site_promotions" ON public.promotions
  FOR INSERT TO authenticated
  WITH CHECK (
    public.auth_app_role() = 'manager'
    AND scope = 'site'
    AND site_id = public.auth_site_id()
  );

CREATE POLICY "manager_update_own_site_promotions" ON public.promotions
  FOR UPDATE TO authenticated
  USING (
    public.auth_app_role() = 'manager'
    AND scope = 'site'
    AND site_id = public.auth_site_id()
  )
  WITH CHECK (
    public.auth_app_role() = 'manager'
    AND scope = 'site'
    AND site_id = public.auth_site_id()
  );

-- ---------------------------------------------------------------------------
-- WRITE : director — CRUD complet mais borné à son propre site
-- ---------------------------------------------------------------------------
CREATE POLICY "director_write_own_site_promotions" ON public.promotions
  FOR ALL TO authenticated
  USING (
    public.auth_app_role() = 'director'
    AND scope = 'site'
    AND site_id = public.auth_site_id()
  )
  WITH CHECK (
    public.auth_app_role() = 'director'
    AND scope = 'site'
    AND site_id = public.auth_site_id()
  );

-- ---------------------------------------------------------------------------
-- WRITE : regional_director — CRUD complet, borné à ses sites (via
-- can_access_site, qui vérifie l'appartenance au groupe) et à ses groupes
-- (ceux qu'il a créés). Jamais scope='chain'.
-- ---------------------------------------------------------------------------
CREATE POLICY "regional_director_write_scoped_promotions" ON public.promotions
  FOR ALL TO authenticated
  USING (
    public.auth_app_role() = 'regional_director' AND (
      (scope = 'site'  AND public.can_access_site(site_id)) OR
      (scope = 'group' AND EXISTS (
        SELECT 1 FROM public.restaurant_groups WHERE id = group_id AND created_by = auth.uid()
      ))
    )
  )
  WITH CHECK (
    public.auth_app_role() = 'regional_director' AND (
      (scope = 'site'  AND public.can_access_site(site_id)) OR
      (scope = 'group' AND EXISTS (
        SELECT 1 FROM public.restaurant_groups WHERE id = group_id AND created_by = auth.uid()
      ))
    )
  );
