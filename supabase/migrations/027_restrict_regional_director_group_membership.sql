-- =============================================================================
-- 027_restrict_regional_director_group_membership.sql
--
-- "owner_write_members" (025) let both 'director' and 'regional_director'
-- INSERT/UPDATE/DELETE restaurant_group_members for any group they created.
-- For 'regional_director' this is a privilege-escalation hole: can_access_site()
-- grants a regional_director access to a site purely because it's a member
-- of a group they created, and owner_write_members never checked that the
-- site being added was already something they had authority over. A
-- regional_director could add ANY site in the chain to their own group and
-- immediately gain can_access_site() on it -- which cascades into write
-- access on promotions (024) and read access on z_reports/site_configs
-- (022) and the fiscal views (023). Verified locally: can_access_site()
-- went from false to true for an arbitrary site right after a
-- self-inserted membership row, with no other change.
--
-- 'director' has no such issue -- can_access_site() for 'director' is
-- based on auth_site_id(), never on group membership -- so director keeps
-- full control over membership of groups they created (GroupForm.tsx's
-- site checkboxes keep working for them).
--
-- For regional_director, site assignment into a group now requires
-- pos_admin (pos_admin_write_members, already in place since 025) --
-- territory assignment is a network-admin decision, not self-service.
-- =============================================================================

DROP POLICY IF EXISTS "owner_write_members" ON public.restaurant_group_members;

CREATE POLICY "director_write_own_members" ON public.restaurant_group_members
  FOR ALL TO authenticated
  USING (
    EXISTS (
      SELECT 1 FROM public.restaurant_groups rg
      WHERE rg.id = group_id
        AND public.auth_app_role() = 'director'
        AND rg.created_by = auth.uid()
    )
  )
  WITH CHECK (
    EXISTS (
      SELECT 1 FROM public.restaurant_groups rg
      WHERE rg.id = group_id
        AND public.auth_app_role() = 'director'
        AND rg.created_by = auth.uid()
    )
  );
