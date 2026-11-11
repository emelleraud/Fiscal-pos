-- =============================================================================
-- 035_revoke_z_reports_stray_grants.sql
--
-- z_reports has the same stray default-privilege grants as fiscal_entries
-- (032/034): `authenticated` has UPDATE, DELETE, TRUNCATE, TRIGGER and
-- REFERENCES, none granted by any migration.
--
-- Unlike fiscal_entries, UPDATE/DELETE are NOT pure dead weight here --
-- "admin_all_z_reports" (002) is a real, intentional FOR ALL policy that
-- lets a pos_admin-claimed user correct or delete a Z-report. Left alone:
-- that's an existing design decision (Z-reports are a derived/summary
-- document, not the signed fiscal_entries ledger itself, which has its
-- own dedicated append-only triggers and is untouched by this), not
-- something to silently override here.
--
-- TRUNCATE/TRIGGER/REFERENCES are different: TRUNCATE is a table-level
-- operation RLS never filters (no policy, however permissive, makes it
-- "pos_admin-only" the way USING clauses do for UPDATE/DELETE), and
-- neither TRIGGER nor REFERENCES matches any real application use case
-- for any role. Revoking those three only -- UPDATE/DELETE stay granted
-- to authenticated so admin_all_z_reports keeps working as designed.
-- =============================================================================

REVOKE TRUNCATE, TRIGGER, REFERENCES
  ON public.z_reports FROM authenticated;
