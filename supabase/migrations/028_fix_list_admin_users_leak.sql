-- =============================================================================
-- 028_fix_list_admin_users_leak.sql
--
-- list_admin_users() (016, patched in 019/020) let any 'regional_director'
-- caller see every 'pos_admin' AND every OTHER 'regional_director' account
-- (email, display_name), unrelated to their own territory. Cause: pos_admin
-- and regional_director accounts never carry a site_id in app_metadata, and
-- the WHERE clause let any row with a NULL site_id through unconditionally
-- ((u.raw_app_meta_data ->> 'site_id') IS NULL), regardless of caller.
-- Verified locally: a regional_director with no relation to another
-- regional_director account saw it in the result set.
--
-- Fix: a regional_director now sees their own row (so the UI can still show
-- "myself" in the list) plus any user whose site_id falls within their
-- managed scope via can_access_site() -- not other admins' accounts.
-- pos_admin is unaffected (still sees everyone, via the first clause).
-- =============================================================================

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
  IF v_role NOT IN ('pos_admin', 'regional_director') THEN
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
