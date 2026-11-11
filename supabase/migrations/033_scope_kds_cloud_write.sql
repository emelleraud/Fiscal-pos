-- =============================================================================
-- 033_scope_kds_cloud_write.sql
--
-- The 5 KDS cloud tables (021_kds_cloud_tables.sql: kds_routing_profiles,
-- kds_station_configs, kds_routing_configs, kds_channel_triggers,
-- kds_timer_thresholds) all have the same write policy:
--
--   USING (public.auth_app_role() IN ('pos_admin','regional_director'))
--
-- -- no site_id check at all. Same bug family as director_write_promotions
-- (024) and director_write_groups/members (025): any 'regional_director'
-- could write (and, since these are FOR ALL policies, also read -- their
-- own USING clause grants SELECT too, alongside the dedicated kds_*_read
-- policy) KDS config for ANY site in the chain, not just the sites/groups
-- they actually manage. 021's own comment says these are meant to be
-- "Écrites depuis le backoffice (pos_admin / regional_director)" -- the
-- role set was right, it just needed the same can_access_site(site_id)
-- scoping the read policies already use (which already covers pos_admin
-- unconditionally, so this only tightens regional_director).
-- =============================================================================

DROP POLICY IF EXISTS "kds_rp_write" ON public.kds_routing_profiles;
CREATE POLICY "kds_rp_write" ON public.kds_routing_profiles
    FOR ALL TO authenticated
    USING (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    )
    WITH CHECK (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    );

DROP POLICY IF EXISTS "kds_sc_write" ON public.kds_station_configs;
CREATE POLICY "kds_sc_write" ON public.kds_station_configs
    FOR ALL TO authenticated
    USING (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    )
    WITH CHECK (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    );

DROP POLICY IF EXISTS "kds_rc_write" ON public.kds_routing_configs;
CREATE POLICY "kds_rc_write" ON public.kds_routing_configs
    FOR ALL TO authenticated
    USING (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    )
    WITH CHECK (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    );

DROP POLICY IF EXISTS "kds_ct_write" ON public.kds_channel_triggers;
CREATE POLICY "kds_ct_write" ON public.kds_channel_triggers
    FOR ALL TO authenticated
    USING (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    )
    WITH CHECK (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    );

DROP POLICY IF EXISTS "kds_tt_write" ON public.kds_timer_thresholds;
CREATE POLICY "kds_tt_write" ON public.kds_timer_thresholds
    FOR ALL TO authenticated
    USING (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    )
    WITH CHECK (
        public.auth_app_role() IN ('pos_admin', 'regional_director')
        AND public.can_access_site(site_id)
    );
