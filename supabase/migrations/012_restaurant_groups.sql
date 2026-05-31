-- supabase/migrations/012_restaurant_groups.sql
CREATE TABLE IF NOT EXISTS public.restaurant_groups (
  id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  name         text        NOT NULL,
  group_type   text        NOT NULL CHECK (group_type IN ('static','dynamic','mixed')),
  criteria     jsonb,
  created_by   uuid        REFERENCES auth.users(id),
  created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS public.restaurant_group_members (
  group_id  uuid NOT NULL REFERENCES public.restaurant_groups(id) ON DELETE CASCADE,
  site_id   uuid NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
  PRIMARY KEY (group_id, site_id)
);

ALTER TABLE public.restaurant_groups ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.restaurant_group_members ENABLE ROW LEVEL SECURITY;

CREATE POLICY "service_role_all_groups" ON public.restaurant_groups
  FOR ALL TO service_role USING (true) WITH CHECK (true);
CREATE POLICY "authenticated_read_groups" ON public.restaurant_groups
  FOR SELECT TO authenticated USING (true);
CREATE POLICY "director_write_groups" ON public.restaurant_groups
  FOR ALL TO authenticated
  USING ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'))
  WITH CHECK ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'));

CREATE POLICY "service_role_all_members" ON public.restaurant_group_members
  FOR ALL TO service_role USING (true) WITH CHECK (true);
CREATE POLICY "authenticated_read_members" ON public.restaurant_group_members
  FOR SELECT TO authenticated USING (true);
CREATE POLICY "director_write_members" ON public.restaurant_group_members
  FOR ALL TO authenticated
  USING ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'))
  WITH CHECK ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'));
