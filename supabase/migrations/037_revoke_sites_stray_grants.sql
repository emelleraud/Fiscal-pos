-- =============================================================================
-- 037_revoke_sites_stray_grants.sql
--
-- Same per-project default privilege as fiscal_entries/z_reports/sessions
-- (032/034/035/036): `authenticated` has TRUNCATE, TRIGGER and REFERENCES
-- on public.sites, none granted by any migration.
--
-- UPDATE/DELETE/INSERT are left alone -- "admin_all_sites" (pos_admin) is
-- a real, intentional FOR ALL policy (pos_admin manages the site
-- directory). "backoffice_read_sites" (USING (true), 009) is also left
-- untouched: the site directory (site_code/name/address/siret) is
-- deliberately readable by every backoffice role for the multi-site
-- dropdown, documented as intentional in 009 -- unlike the other
-- unscoped-read bugs fixed earlier, this isn't fiscal/business-sensitive
-- data gated by site, it's the reference list every page needs.
--
-- TRUNCATE is the same unconditional RLS gap as the other three tables.
-- =============================================================================

REVOKE TRUNCATE, TRIGGER, REFERENCES
  ON public.sites FROM authenticated;
