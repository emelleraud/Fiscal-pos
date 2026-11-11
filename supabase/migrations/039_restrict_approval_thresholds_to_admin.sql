-- =============================================================================
-- 039_restrict_approval_thresholds_to_admin.sql
--
-- promotion_approval_thresholds (013) has no site_id/group_id at all --
-- it's a generic, chain-wide rule table ("a promo at scope X above Y
-- cents needs role Z"), not a per-site/per-group instance. Its only
-- write policy, "regional_director_write_thresholds", let ANY
-- regional_director edit it -- affecting approval rules for the whole
-- chain, including territories they don't manage, since there's no
-- site/group dimension to scope it to. pos_admin was never included in
-- any write policy at all (same gap already found and fixed on
-- promotions in 024 and restaurant_groups in 025).
--
-- Per decision: this chain-wide config is pos_admin-only going forward.
-- =============================================================================

DROP POLICY IF EXISTS "regional_director_write_thresholds" ON public.promotion_approval_thresholds;

CREATE POLICY "pos_admin_write_thresholds" ON public.promotion_approval_thresholds
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');
