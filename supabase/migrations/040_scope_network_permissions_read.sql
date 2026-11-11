-- =============================================================================
-- 040_scope_network_permissions_read.sql
--
-- "np_read" (017) was USING (true) for authenticated -- any backoffice
-- user could see the lock/unlock matrix for every site and group in the
-- chain, not just their own. Documented at the time as needed "to load
-- inherited rules" (network -> group -> site), but that only requires
-- seeing: network-wide rows (apply to everyone by definition), rows for
-- your own site, and rows for groups you own or that your site belongs
-- to -- not every other site/group's rows too.
--
-- Reuses can_access_site() (site rows) and the group_created_by()/
-- site_belongs_to_group() SECURITY DEFINER helpers from 026 (group rows)
-- rather than querying restaurant_group_members directly, consistent
-- with how that cross-table RLS cycle was avoided there.
-- =============================================================================

DROP POLICY IF EXISTS "np_read" ON public.network_permissions;

CREATE POLICY "np_read" ON public.network_permissions
  FOR SELECT TO authenticated
  USING (
    public.auth_app_role() IN ('pos_admin', 'pos_auditeur')
    OR (site_id IS NULL AND group_id IS NULL)
    OR (site_id IS NOT NULL AND public.can_access_site(site_id))
    OR (group_id IS NOT NULL AND (
          public.group_created_by(group_id) = auth.uid()
          OR public.site_belongs_to_group(group_id, public.auth_site_id())
        ))
  );
