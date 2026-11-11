-- =============================================================================
-- 025_scope_director_write_groups.sql
--
-- "director_write_groups" / "director_write_members" (012_restaurant_groups.sql)
-- grant FOR ALL (read/insert/update/delete) to role 'director' with no
-- ownership check at all. Same bug family already fixed on promotions
-- (024): a site director could rename, delete or reassign membership of
-- ANY group in the chain, not just the ones they created. Fixing only
-- restaurant_groups and leaving restaurant_group_members unscoped would
-- be pointless -- a director could still add/remove any site to/from any
-- group via membership even if they couldn't touch the group row itself,
-- so both are fixed together here.
--
-- The backoffice nav shows "Groupes" to any role >= 'director' (useRole's
-- rank system), so director genuinely needs to create/manage groups
-- through this UI -- this isn't removing the feature, just scoping it to
-- groups the director/regional_director actually created (created_by =
-- auth.uid()), the same ownership model can_access_site() already uses
-- for regional_director elsewhere.
--
-- pos_admin also never had an explicit write policy on either table --
-- only service_role and the buggy director one did (same gap already
-- found and fixed on promotions in 024).
--
-- Not touched here: "authenticated_read_groups"/"authenticated_read_members"
-- (USING (true)) are a separate, lower-severity unscoped-read leak on the
-- same tables -- left alone since this was scoped to director_write_groups.
-- =============================================================================

DROP POLICY IF EXISTS "director_write_groups"  ON public.restaurant_groups;
DROP POLICY IF EXISTS "director_write_members" ON public.restaurant_group_members;

-- ---------------------------------------------------------------------------
-- pos_admin : chain-wide, comme pour promotions (024)
-- ---------------------------------------------------------------------------
CREATE POLICY "pos_admin_write_groups" ON public.restaurant_groups
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');

CREATE POLICY "pos_admin_write_members" ON public.restaurant_group_members
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');

-- ---------------------------------------------------------------------------
-- director / regional_director : uniquement les groupes qu'ils ont créés
-- ---------------------------------------------------------------------------
CREATE POLICY "owner_write_groups" ON public.restaurant_groups
  FOR ALL TO authenticated
  USING (
    public.auth_app_role() IN ('director', 'regional_director')
    AND created_by = auth.uid()
  )
  WITH CHECK (
    public.auth_app_role() IN ('director', 'regional_director')
    AND created_by = auth.uid()
  );

CREATE POLICY "owner_write_members" ON public.restaurant_group_members
  FOR ALL TO authenticated
  USING (
    EXISTS (
      SELECT 1 FROM public.restaurant_groups rg
      WHERE rg.id = group_id
        AND public.auth_app_role() IN ('director', 'regional_director')
        AND rg.created_by = auth.uid()
    )
  )
  WITH CHECK (
    EXISTS (
      SELECT 1 FROM public.restaurant_groups rg
      WHERE rg.id = group_id
        AND public.auth_app_role() IN ('director', 'regional_director')
        AND rg.created_by = auth.uid()
    )
  );
