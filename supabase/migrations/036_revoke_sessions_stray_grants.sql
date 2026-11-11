-- =============================================================================
-- 036_revoke_sessions_stray_grants.sql
--
-- Same per-project default privilege as fiscal_entries/z_reports (032/034/
-- 035): `authenticated` has TRUNCATE, TRIGGER and REFERENCES on
-- public.sessions, none granted by any migration.
--
-- UPDATE/DELETE are left alone -- both "admin_all_sessions" (pos_admin)
-- and "caissier_own_sessions" (site_id = auth_site_id(), which also
-- covers manager/director since they carry the same site_id claim) are
-- real, intentional FOR ALL policies, not dead grants.
--
-- TRUNCATE is the same unconditional gap as the other two tables: RLS
-- never filters it regardless of policy, so even pos_auditeur (read-only
-- everywhere else on this table) could wipe every site's sessions. Revokes
-- TRUNCATE/TRIGGER/REFERENCES only.
-- =============================================================================

REVOKE TRUNCATE, TRIGGER, REFERENCES
  ON public.sessions FROM authenticated;
