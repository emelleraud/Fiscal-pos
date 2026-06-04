-- supabase/migrations/016_site_technical_configs.sql
-- Paramètres serveur par site/device. Clés Ed25519 en Vault (jamais ici).

CREATE TABLE public.site_technical_configs (
  id                       uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  site_id                  uuid        NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
  device_type              text        NOT NULL DEFAULT 'pos',
  edge_api_port            integer     NOT NULL DEFAULT 8080,
  sync_interval_s          integer     NOT NULL DEFAULT 300,
  fiscal_key_configured_at timestamptz,
  updated_at               timestamptz NOT NULL DEFAULT now(),
  UNIQUE (site_id, device_type)
);

-- Helper RLS : accès d'un user à un site donné
CREATE OR REPLACE FUNCTION public.can_access_site(p_site_id uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT CASE public.auth_app_role()
    WHEN 'pos_admin'         THEN true
    WHEN 'pos_auditeur'      THEN true
    WHEN 'regional_director' THEN EXISTS (
      SELECT 1 FROM restaurant_group_members rgm
      JOIN   restaurant_groups rg ON rg.id = rgm.group_id
      WHERE  rgm.site_id = p_site_id AND rg.created_by = auth.uid()
    )
    WHEN 'pos_caissier'      THEN p_site_id = public.auth_site_id()
    ELSE false
  END;
$$;

-- Provision clé Ed25519 → Vault (appelée par Edge Function config-provision via service_role)
CREATE OR REPLACE FUNCTION public.provision_fiscal_key(p_site_id uuid, p_key_hex text)
RETURNS timestamptz LANGUAGE plpgsql SECURITY DEFINER
SET search_path = public, vault AS $$
DECLARE
  v_secret_name text := 'fiscal_key_' || p_site_id::text;
  v_existing_id uuid;
BEGIN
  IF p_key_hex !~ '^[0-9a-fA-F]{64}$' THEN
    RAISE EXCEPTION 'invalid_key_format';
  END IF;
  SELECT id INTO v_existing_id FROM vault.secrets WHERE name = v_secret_name;
  IF v_existing_id IS NOT NULL THEN
    PERFORM vault.update_secret(v_existing_id, p_key_hex);
  ELSE
    PERFORM vault.create_secret(p_key_hex, v_secret_name);
  END IF;
  INSERT INTO public.site_technical_configs (site_id, fiscal_key_configured_at, updated_at)
    VALUES (p_site_id, now(), now())
    ON CONFLICT (site_id, device_type) DO UPDATE
      SET fiscal_key_configured_at = now(), updated_at = now();
  RETURN now();
END; $$;

REVOKE EXECUTE ON FUNCTION public.provision_fiscal_key FROM PUBLIC, authenticated;
GRANT  EXECUTE ON FUNCTION public.provision_fiscal_key TO service_role;

-- Fonction helper pour lister les users admin (lit auth.users en SECURITY DEFINER)
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
      (u.app_metadata ->> 'role')::text,
      (u.app_metadata ->> 'site_id')::text,
      (u.app_metadata ->> 'display_name')::text,
      (u.banned_until IS NOT NULL AND u.banned_until > now()),
      u.created_at
    FROM auth.users u
    ORDER BY u.created_at DESC;
END; $$;

GRANT EXECUTE ON FUNCTION public.list_admin_users TO authenticated;

-- RLS site_technical_configs
ALTER TABLE public.site_technical_configs ENABLE ROW LEVEL SECURITY;

CREATE POLICY "stc_read" ON public.site_technical_configs
  FOR SELECT TO authenticated USING (public.can_access_site(site_id));

CREATE POLICY "stc_admin_write" ON public.site_technical_configs
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');

CREATE POLICY "stc_service_role" ON public.site_technical_configs
  FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.site_technical_configs TO authenticated;
