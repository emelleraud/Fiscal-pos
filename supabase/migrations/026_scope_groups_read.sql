-- =============================================================================
-- 026_scope_groups_read.sql
--
-- "authenticated_read_groups" / "authenticated_read_members" (012) are
-- USING (true) for any authenticated user -- the last unscoped-read leak
-- on these two tables after 025 fixed the write side. Any manager/director
-- could see the name/criteria/owner of every group in the chain and its
-- full site membership, not just groups relevant to their own site.
--
-- Scope: visible to pos_admin/pos_auditeur (as elsewhere), to the
-- director/regional_director who created the group, and to any site that
-- is itself a member of the group.
--
-- Naively cross-referencing restaurant_group_members from a policy on
-- restaurant_groups (and vice versa) creates a two-table RLS cycle --
-- Postgres raises "infinite recursion detected in policy". The two
-- SECURITY DEFINER helpers below run as the table owner, which bypasses
-- RLS on the table they query, breaking the cycle.
-- =============================================================================

CREATE OR REPLACE FUNCTION public.site_belongs_to_group(p_group_id uuid, p_site_id uuid)
RETURNS boolean LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = public AS $$
  SELECT EXISTS (
    SELECT 1 FROM public.restaurant_group_members
    WHERE group_id = p_group_id AND site_id = p_site_id
  );
$$;
REVOKE ALL ON FUNCTION public.site_belongs_to_group(uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.site_belongs_to_group(uuid, uuid) TO authenticated;

CREATE OR REPLACE FUNCTION public.group_created_by(p_group_id uuid)
RETURNS uuid LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = public AS $$
  SELECT created_by FROM public.restaurant_groups WHERE id = p_group_id;
$$;
REVOKE ALL ON FUNCTION public.group_created_by(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.group_created_by(uuid) TO authenticated;

DROP POLICY IF EXISTS "authenticated_read_groups"  ON public.restaurant_groups;
DROP POLICY IF EXISTS "authenticated_read_members" ON public.restaurant_group_members;

CREATE POLICY "scoped_read_groups" ON public.restaurant_groups
  FOR SELECT TO authenticated
  USING (
    public.auth_app_role() IN ('pos_admin', 'pos_auditeur')
    OR created_by = auth.uid()
    OR public.site_belongs_to_group(id, public.auth_site_id())
  );

CREATE POLICY "scoped_read_members" ON public.restaurant_group_members
  FOR SELECT TO authenticated
  USING (
    public.auth_app_role() IN ('pos_admin', 'pos_auditeur')
    OR public.group_created_by(group_id) = auth.uid()
    OR public.site_belongs_to_group(group_id, public.auth_site_id())
  );
