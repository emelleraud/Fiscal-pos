-- =============================================================================
-- 019_fix_list_admin_users.sql
-- Fix : auth.users.app_metadata → raw_app_meta_data (nom réel de la colonne)
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
       OR (u.raw_app_meta_data ->> 'site_id') IS NULL
       OR public.can_access_site((u.raw_app_meta_data ->> 'site_id')::uuid)
    ORDER BY u.created_at DESC;
END; $$;
