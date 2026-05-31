-- supabase/migrations/014_promotions.sql
CREATE TABLE IF NOT EXISTS public.promotions (
  id                    uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  name                  text        NOT NULL,
  scope                 text        NOT NULL CHECK (scope IN ('chain','group','site')),
  site_id               uuid        REFERENCES public.sites(id),
  group_id              uuid        REFERENCES public.restaurant_groups(id),
  promo_type            text        NOT NULL CHECK (promo_type IN ('fixed_amount','percentage','item_discount','bogo','happy_hour')),
  value_cents           int,
  value_bps             int,
  target_sku            text,
  trigger               text        NOT NULL CHECK (trigger IN ('auto','manual')),
  exclusion_group       text,
  priority              int         NOT NULL DEFAULT 0,
  valid_from            date,
  valid_to              date,
  days_of_week          int[],
  time_from             time,
  time_to               time,
  status                text        NOT NULL DEFAULT 'draft'
                                    CHECK (status IN ('draft','pending_approval','approved','active','rejected')),
  required_approval_role text       CHECK (required_approval_role IN ('manager','director','regional_director')),
  approved_by           uuid        REFERENCES auth.users(id),
  approved_at           timestamptz,
  rejected_by           uuid        REFERENCES auth.users(id),
  rejection_reason      text,
  created_by            uuid        NOT NULL REFERENCES auth.users(id),
  active                boolean     NOT NULL DEFAULT false,
  created_at            timestamptz NOT NULL DEFAULT now(),
  updated_at_ms         bigint      NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
  CONSTRAINT scope_consistency CHECK (
    (scope = 'site'  AND site_id IS NOT NULL  AND group_id IS NULL) OR
    (scope = 'group' AND group_id IS NOT NULL AND site_id IS NULL)  OR
    (scope = 'chain' AND site_id IS NULL      AND group_id IS NULL)
  )
);

ALTER TABLE public.promotions ENABLE ROW LEVEL SECURITY;

CREATE POLICY "service_role_all_promotions" ON public.promotions
  FOR ALL TO service_role USING (true) WITH CHECK (true);

CREATE POLICY "authenticated_read_promotions" ON public.promotions
  FOR SELECT TO authenticated USING (true);

CREATE POLICY "manager_write_site_promotions" ON public.promotions
  FOR INSERT TO authenticated
  WITH CHECK (
    (auth.jwt() -> 'app_metadata' ->> 'role') IN ('manager','director','regional_director')
    AND scope = 'site'
  );

CREATE POLICY "director_write_promotions" ON public.promotions
  FOR ALL TO authenticated
  USING ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'))
  WITH CHECK ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'));
