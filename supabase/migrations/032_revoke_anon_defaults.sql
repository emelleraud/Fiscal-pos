-- =============================================================================
-- 032_revoke_anon_defaults.sql
--
-- Found while checking menu_combo_slots/menu_combo_slot_options grants:
-- the `anon` role has full SELECT/INSERT/UPDATE/DELETE/TRUNCATE on nearly
-- every table in `public` (confirmed via information_schema.table_privileges
-- against the linked project) -- not from any migration's own GRANT
-- statement (every migration here only ever grants to `authenticated`),
-- but from this Supabase project's schema-level default privileges, which
-- apply to every newly created table regardless of what the migration
-- itself grants. Currently NOT exploitable: every table has RLS enabled
-- (confirmed via pg_class.relrowsecurity) and no remaining policy grants
-- anon anything, so this is a defense-in-depth gap, not an active leak --
-- EXCEPT for functions (below), where it is active and critical.
--
-- `supabase db advisors --linked` confirms the same default-privilege
-- pattern hit four SECURITY DEFINER functions, and for one of them it is
-- NOT just defense-in-depth:
--
--   public.provision_fiscal_key(p_site_id uuid, p_key_hex text) --
--   writes the Ed25519 fiscal signing key for a site into Vault, with NO
--   internal role check (016_site_technical_configs.sql relied entirely on
--   REVOKE EXECUTE ... FROM PUBLIC, authenticated -- it never named `anon`).
--   Because of the same default-privilege grant, `anon` can currently call
--   it directly via POST /rest/v1/rpc/provision_fiscal_key with NO
--   authentication at all, and overwrite the NF525 signing key for ANY
--   site_id with a key of the caller's choosing (only format-checked, 64
--   hex chars). This is a live, unauthenticated, critical vulnerability
--   on production right now -- fixed first, below.
--
-- The other three (list_admin_users, group_created_by, site_belongs_to_group)
-- are each independently protected by their own query logic even when
-- called by anon (list_admin_users' WHERE clause needs a matching
-- site_id/role that anon never has; the other two just return a boolean
-- for a specific group/site pair), so no data was actually exposed through
-- them -- but anon has no legitimate reason to call any of them, so
-- revoking is still the right call.
-- =============================================================================

-- ---------------------------------------------------------------------------
-- Critical: close the unauthenticated fiscal-key-overwrite path first.
-- ---------------------------------------------------------------------------
REVOKE EXECUTE ON FUNCTION public.provision_fiscal_key(uuid, text) FROM anon;

-- ---------------------------------------------------------------------------
-- Other SECURITY DEFINER functions anon has no legitimate reason to call.
--
-- list_admin_users() additionally still had PUBLIC itself holding EXECUTE
-- (Postgres grants EXECUTE to PUBLIC on every new function by default;
-- none of 016/019/020/028 ever revoked it) -- revoking from `anon` alone
-- would have left it reachable through that PUBLIC grant, which every
-- role implicitly has. group_created_by/site_belongs_to_group already had
-- PUBLIC correctly revoked in 026; only their separate direct `anon` grant
-- (the same platform default as above) needed closing.
-- ---------------------------------------------------------------------------
REVOKE ALL ON FUNCTION public.list_admin_users() FROM PUBLIC;
REVOKE EXECUTE ON FUNCTION public.list_admin_users() FROM anon;
REVOKE EXECUTE ON FUNCTION public.group_created_by(uuid) FROM anon;
REVOKE EXECUTE ON FUNCTION public.site_belongs_to_group(uuid, uuid) FROM anon;

-- Belt-and-suspenders: list_admin_users' own guard clause only RAISEs when
-- v_role evaluates to a definite non-admin role. For anon, auth_app_role()
-- returns NULL, and `NULL NOT IN (...)` is NULL, not TRUE, so the RAISE
-- EXCEPTION branch was silently skipped for an anon caller (the WHERE
-- clause still returned zero rows independently, so nothing leaked, but
-- the guard itself didn't do its job for this case).
CREATE OR REPLACE FUNCTION public.list_admin_users()
RETURNS TABLE (
  id            uuid,
  email         text,
  role          text,
  site_id       text,
  display_name  text,
  is_banned     boolean,
  created_at    timestamptz
)
LANGUAGE plpgsql SECURITY DEFINER
SET search_path = public, auth AS $$
DECLARE
  v_role text := public.auth_app_role();
BEGIN
  IF v_role IS NULL OR v_role NOT IN ('pos_admin', 'regional_director') THEN
    RAISE EXCEPTION 'forbidden';
  END IF;
  RETURN QUERY
    SELECT
      u.id,
      u.email,
      (u.raw_app_meta_data ->> 'role')::text,
      (u.raw_app_meta_data ->> 'site_id')::text,
      (u.raw_app_meta_data ->> 'display_name')::text,
      (u.banned_until IS NOT NULL AND u.banned_until > now()),
      u.created_at
    FROM auth.users u
    WHERE v_role = 'pos_admin'
       OR u.id = auth.uid()
       OR public.can_access_site((u.raw_app_meta_data ->> 'site_id')::uuid)
    ORDER BY u.created_at DESC;
END; $$;

GRANT EXECUTE ON FUNCTION public.list_admin_users TO authenticated;

-- ---------------------------------------------------------------------------
-- Tables: anon has no legitimate role in this app at all anymore (every
-- remaining anon table grant was from a stray default privilege, not an
-- intentional migration GRANT -- the two tables that WERE meant to be
-- anon-readable, z_reports and the fiscal views, already had that access
-- explicitly revoked in 022/023). Strip anon from every table in `public`,
-- and stop it from happening again on future tables.
-- ---------------------------------------------------------------------------
REVOKE ALL ON ALL TABLES IN SCHEMA public FROM anon;
ALTER DEFAULT PRIVILEGES IN SCHEMA public REVOKE ALL ON TABLES FROM anon;
ALTER DEFAULT PRIVILEGES IN SCHEMA public REVOKE ALL ON FUNCTIONS FROM anon;
